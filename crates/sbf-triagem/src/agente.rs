//! O agente contínuo de triagem (spec 09, "Agente contínuo"). Aprende com as decisões do
//! usuário **sem treinar modelo**, em três camadas:
//!
//! 1. **Regras aprendidas** ([`Agente::atualizar_regras`]): `(fornecedor, tipo) -> categoria`
//!    e `padrão do nome do arquivo -> tipo de documento`. A regra nasce com
//!    [`MINIMO_PARA_REGRA`] confirmações do usuário sem nenhuma recusa nem conflito, e morre
//!    na primeira recusa de uma sugestão dela. Aceitar uma sugestão da IA não conta como
//!    exemplo (a decisão leva o motivo [`crate::aceite::MOTIVO_DA_DECISAO`]).
//! 2. **Exemplos no prompt** ([`Agente::exemplos_para`]): até [`EXEMPLOS_NO_PROMPT`] decisões
//!    do usuário sobre o mesmo fornecedor entram no prompt `triagem-v2`.
//! 3. **Calibração** ([`Agente::atualizar_calibracao`]): o acerto por faixa de confiança,
//!    contra o que o usuário aceita e recusa. Uma faixa que erra mais do que diz vale menos
//!    ([`Agente::confianca_calibrada`]).
//!
//! O agente nunca aplica nada: só grava sugestões. O estado dele (regras, calibração,
//! chegadas) mora num SQLite próprio, `agente.sqlite`, fora do `inventario.sqlite`: o
//! esquema do inventário é do Rust por migrações e o Bun o lê, e o agente não precisa dele.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension as _, params};
use serde::Serialize;

use crate::aceite::MOTIVO_DA_DECISAO;
use crate::erro::Resultado;
use crate::fila::{Aprendida, CONFIANCA_DA_REGRA, CONFIANCA_DA_REGRA_FORTE, SugestaoDeRegra};
use crate::normalizar::{normalizar, normalizar_fornecedor};
use crate::tipos::Alvo;

pub use crate::prompt::{EXEMPLOS_NO_PROMPT, PROMPT_VERSAO_V2};

/// Confirmações do usuário, sem recusa, para uma regra nascer.
pub const MINIMO_PARA_REGRA: usize = 3;
/// Amostra mínima de uma faixa para a calibração valer.
pub const AMOSTRA_MINIMA_DA_FAIXA: i64 = 10;
/// Separa fornecedor e tipo na chave das regras de categoria.
const SEP: char = '\u{1f}';

/// O tipo de uma regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TipoDeRegra {
    /// `(fornecedor, tipo do registro) -> categoria`.
    Categoria,
    /// `padrão do nome do arquivo -> tipo de documento`.
    TipoDocumento,
}

impl TipoDeRegra {
    fn codigo(self) -> &'static str {
        match self {
            Self::Categoria => "categoria",
            Self::TipoDocumento => "tipo_documento",
        }
    }
    fn de(codigo: &str) -> Option<Self> {
        match codigo {
            "categoria" => Some(Self::Categoria),
            "tipo_documento" => Some(Self::TipoDocumento),
            _ => None,
        }
    }
}

/// Uma regra aprendida, para o painel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegraAprendida {
    /// Id para apagar.
    pub id: i64,
    /// O tipo.
    pub tipo: TipoDeRegra,
    /// O que a regra reconhece, legível: `mercado delta (compra)` ou `fatura #`.
    pub quando: String,
    /// O que ela conclui.
    pub valor: String,
    /// Quantas confirmações a sustentam.
    pub exemplos: usize,
}

/// As faixas da calibração: `(mínimo, máximo exclusivo, nome)`.
pub const FAIXAS: [(f64, f64, &str); 4] = [
    (0.0, 0.7, "abaixo de 0,70"),
    (0.7, 0.85, "0,70 a 0,85"),
    (0.85, 0.95, "0,85 a 0,95"),
    (0.95, 1.01, "0,95 ou mais"),
];

/// A faixa de uma confiança.
#[must_use]
pub fn faixa_de(confianca: f64) -> usize {
    FAIXAS
        .iter()
        .position(|(min, max, _)| confianca >= *min && confianca < *max)
        .unwrap_or(FAIXAS.len() - 1)
}

/// O acerto de uma faixa.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Calibracao {
    /// O nome da faixa.
    pub faixa: &'static str,
    /// Sugestões aceitas.
    pub aceitas: i64,
    /// Sugestões recusadas.
    pub recusadas: i64,
}

impl Calibracao {
    /// A fração aceita, se há ao menos uma decisão.
    #[must_use]
    pub fn acerto(&self) -> Option<f64> {
        let total = self.aceitas + self.recusadas;
        (total > 0).then(|| self.aceitas as f64 / total as f64)
    }
}

/// Os números do painel da área Triagem por IA.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Painel {
    /// Itens que chegaram nos últimos 7 dias.
    pub chegaram: usize,
    /// Itens que ganharam sugestão nos últimos 7 dias.
    pub sugeridas: usize,
    /// Sugestões abertas esperando você.
    pub abertas: usize,
    /// Sugestões abertas de alta confiança (já calibrada) prontas para confirmar.
    pub prontas: usize,
    /// As regras aprendidas.
    pub regras: Vec<RegraAprendida>,
    /// Aceitas e recusadas nos últimos 7 dias.
    pub aceitas_da_semana: i64,
    /// Recusadas nos últimos 7 dias.
    pub recusadas_da_semana: i64,
    /// Itens na fila agora (para a barra de status).
    pub na_fila: usize,
}

impl Painel {
    /// O acerto da semana, se houve decisão.
    #[must_use]
    pub fn acerto_da_semana(&self) -> Option<f64> {
        let t = self.aceitas_da_semana + self.recusadas_da_semana;
        (t > 0).then(|| self.aceitas_da_semana as f64 / t as f64)
    }
}

/// O padrão do nome de um arquivo: minúsculas, sem acento, sem extensão, cada sequência de
/// dígitos vira `#`. `Fatura-Luz_Out-2026.pdf` vira `fatura luz out #`.
#[must_use]
pub fn padrao_do_nome(nome: &str) -> String {
    let sem_ext = match nome.rfind('.') {
        Some(i) if i > 0 && nome.len() - i <= 6 => &nome[..i],
        _ => nome,
    };
    let mut saida = String::new();
    let mut em_digitos = false;
    for c in normalizar(sem_ext).chars() {
        if c.is_ascii_digit() {
            if !em_digitos {
                saida.push('#');
            }
            em_digitos = true;
        } else {
            em_digitos = false;
            saida.push(c);
        }
    }
    let mut palavras: Vec<&str> = Vec::new();
    for p in saida.split_whitespace() {
        // `2026-10` vira um `#` só.
        if !(p == "#" && palavras.last() == Some(&"#")) {
            palavras.push(p);
        }
    }
    palavras.join(" ")
}

/// `(tipo, chave) -> valor -> (quantas confirmações, instante da última)`.
type PorChave = BTreeMap<(&'static str, String), BTreeMap<String, (usize, String)>>;

/// O estado do agente (`agente.sqlite`).
#[derive(Debug)]
pub struct Agente {
    estado: Connection,
}

impl Agente {
    /// Abre (criando se preciso) o estado do agente em `caminho`.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn abrir(caminho: impl AsRef<Path>) -> Resultado<Self> {
        let estado = Connection::open(caminho)?;
        estado.busy_timeout(Duration::from_secs(30))?;
        estado.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS regra_aprendida (
               id INTEGER PRIMARY KEY,
               tipo TEXT NOT NULL,
               chave TEXT NOT NULL,
               valor TEXT NOT NULL,
               exemplos INTEGER NOT NULL,
               criada_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
               apagada_em TEXT,
               motivo_da_morte TEXT,
               UNIQUE (tipo, chave, valor)
             );
             CREATE TABLE IF NOT EXISTS chegada (
               alvo_tipo TEXT NOT NULL,
               alvo_id TEXT NOT NULL,
               vista_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
               inicial INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (alvo_tipo, alvo_id)
             );
             CREATE TABLE IF NOT EXISTS calibracao (
               faixa INTEGER PRIMARY KEY,
               aceitas INTEGER NOT NULL,
               recusadas INTEGER NOT NULL,
               atualizada_em TEXT NOT NULL
             );",
        )?;
        Ok(Self { estado })
    }

    // -- regras ---------------------------------------------------------------------------

    /// Confirmações do usuário por `(fornecedor|tipo, categoria)` e por `(padrão, tipo de
    /// documento)`, já sem as que vieram de aceitar sugestão da IA, com o instante da última.
    fn confirmacoes(inv: &Connection) -> Resultado<Vec<(TipoDeRegra, String, String, String)>> {
        let mut saida = Vec::new();
        let mut c = inv.prepare(
            "SELECT r.fornecedor_normalizado, r.supplier_name_raw, r.tipo, r.categoria_provavel, d.em
               FROM decisao d JOIN registro r ON CAST(r.id AS TEXT) = d.alvo_id
              WHERE d.alvo_tipo = 'registro' AND d.campo = 'categoria' AND d.autor = 'usuario'
                AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?1
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                AND r.categoria_provavel IS NOT NULL AND r.categoria_provavel <> ''
                AND json_extract(d.depois, '$.categoria_provavel') = r.categoria_provavel",
        )?;
        let linhas = c.query_map([MOTIVO_DA_DECISAO], |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, Option<String>>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, String>(4)?,
            ))
        })?;
        for l in linhas {
            let (norm, bruto, tipo, categoria, em) = l?;
            let Some(nome) = norm.filter(|n| !n.trim().is_empty()).or(bruto) else {
                continue;
            };
            let f = normalizar_fornecedor(&nome);
            if !f.is_empty() {
                saida.push((
                    TipoDeRegra::Categoria,
                    format!("{f}{SEP}{tipo}"),
                    categoria,
                    em,
                ));
            }
        }
        let mut c = inv.prepare(
            "SELECT (SELECT o.filename FROM origem o WHERE o.content_hash = d.alvo_id
                      ORDER BY o.id LIMIT 1),
                    json_extract(d.depois, '$.document_type'), d.em
               FROM decisao d
              WHERE d.alvo_tipo = 'achado' AND d.campo = 'document_type' AND d.autor = 'usuario'
                AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?1
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)",
        )?;
        let linhas = c.query_map([MOTIVO_DA_DECISAO], |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, Option<String>>(1)?,
                l.get::<_, String>(2)?,
            ))
        })?;
        for l in linhas {
            let (nome, tipo, em) = l?;
            if let (Some(nome), Some(tipo)) = (nome, tipo) {
                let p = padrao_do_nome(&nome);
                if p.chars().any(char::is_alphabetic) {
                    saida.push((TipoDeRegra::TipoDocumento, p, tipo, em));
                }
            }
        }
        Ok(saida)
    }

    /// Valores recusados (de sugestões de categoria e de tipo de documento), por chave.
    fn recusadas(inv: &Connection) -> Resultado<Vec<(TipoDeRegra, String, String)>> {
        let mut saida = Vec::new();
        let mut c = inv.prepare(
            "SELECT r.fornecedor_normalizado, r.supplier_name_raw, r.tipo, s.valor_json
               FROM sugestao_ia s JOIN registro r ON CAST(r.id AS TEXT) = s.alvo_id
              WHERE s.alvo_tipo = 'registro' AND s.campo = 'categoria' AND s.estado = 'recusada'",
        )?;
        for l in c.query_map([], |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, Option<String>>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<String>>(3)?,
            ))
        })? {
            let (norm, bruto, tipo, valor) = l?;
            let (Some(nome), Some(valor)) = (
                norm.filter(|n| !n.trim().is_empty()).or(bruto),
                valor.and_then(|v| serde_json::from_str::<String>(&v).ok()),
            ) else {
                continue;
            };
            saida.push((
                TipoDeRegra::Categoria,
                format!("{}{SEP}{tipo}", normalizar_fornecedor(&nome)),
                valor,
            ));
        }
        let mut c = inv.prepare(
            "SELECT (SELECT o.filename FROM origem o WHERE o.content_hash = s.alvo_id
                      ORDER BY o.id LIMIT 1), s.valor_json
               FROM sugestao_ia s
              WHERE s.alvo_tipo = 'achado' AND s.campo = 'tipo_documento' AND s.estado = 'recusada'",
        )?;
        for l in c.query_map([], |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, Option<String>>(1)?,
            ))
        })? {
            let (nome, valor) = l?;
            if let (Some(nome), Some(valor)) = (
                nome,
                valor.and_then(|v| serde_json::from_str::<String>(&v).ok()),
            ) {
                saida.push((TipoDeRegra::TipoDocumento, padrao_do_nome(&nome), valor));
            }
        }
        Ok(saida)
    }

    /// Recalcula as regras a partir do log de decisões e das recusas: cria as que atingiram o
    /// mínimo, mata as que tiveram recusa ou conflito. Devolve as ativas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn atualizar_regras(&self, inv: &Connection) -> Resultado<Vec<RegraAprendida>> {
        let confirmacoes = Self::confirmacoes(inv)?;
        let recusadas = Self::recusadas(inv)?;
        // (tipo, chave) -> valor -> (quantas, instante da última)
        let mut por_chave: PorChave = BTreeMap::new();
        for (t, chave, valor, em) in confirmacoes {
            let e = por_chave
                .entry((t.codigo(), chave))
                .or_default()
                .entry(valor)
                .or_insert((0, String::new()));
            e.0 += 1;
            if em > e.1 {
                e.1 = em;
            }
        }
        // O que o usuário apagou à mão só volta com 3 confirmações novas: guardamos quando.
        let apagadas_a_mao: HashMap<(String, String, String), String> = {
            let mut c = self.estado.prepare(
                "SELECT tipo, chave, valor, apagada_em FROM regra_aprendida
                  WHERE apagada_em IS NOT NULL AND motivo_da_morte = 'manual'",
            )?;
            c.query_map([], |l| Ok(((l.get(0)?, l.get(1)?, l.get(2)?), l.get(3)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut vivas: Vec<(&'static str, String, String, usize)> = Vec::new();
        for ((tipo, chave), valores) in &por_chave {
            if valores.len() != 1 {
                continue; // conflito: o usuário decidiu de dois jeitos
            }
            let Some((valor, (n, _))) = valores.iter().next() else {
                continue;
            };
            let recusada = recusadas
                .iter()
                .any(|(t, c, v)| t.codigo() == *tipo && c == chave && v == valor);
            if recusada {
                continue;
            }
            let mut n = *n;
            if let Some(quando) =
                apagadas_a_mao.get(&((*tipo).to_owned(), chave.clone(), valor.clone()))
            {
                // Só as confirmações depois da exclusão contam.
                n = Self::confirmacoes_depois(inv, TipoDeRegra::de(tipo), chave, valor, quando)?;
            }
            if n >= MINIMO_PARA_REGRA {
                vivas.push((tipo, chave.clone(), valor.clone(), n));
            }
        }
        let tx = self.estado.unchecked_transaction()?;
        for (tipo, chave, valor, n) in &vivas {
            tx.execute(
                "INSERT INTO regra_aprendida (tipo, chave, valor, exemplos) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (tipo, chave, valor) DO UPDATE
                    SET exemplos = excluded.exemplos, apagada_em = NULL, motivo_da_morte = NULL",
                params![tipo, chave, valor, i64::try_from(*n).unwrap_or(i64::MAX)],
            )?;
        }
        // Quem estava ativa e não está mais viva (recusa ou conflito) morre.
        let ativas: Vec<(i64, String, String, String)> = {
            let mut c = tx.prepare(
                "SELECT id, tipo, chave, valor FROM regra_aprendida WHERE apagada_em IS NULL",
            )?;
            c.query_map([], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)))?
                .collect::<Result<_, _>>()?
        };
        for (id, tipo, chave, valor) in ativas {
            if !vivas
                .iter()
                .any(|(t, c, v, _)| *t == tipo && *c == chave && *v == valor)
            {
                tx.execute(
                    "UPDATE regra_aprendida SET apagada_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                       motivo_da_morte = 'recusa' WHERE id = ?1",
                    [id],
                )?;
            }
        }
        tx.commit()?;
        self.regras()
    }

    fn confirmacoes_depois(
        inv: &Connection,
        tipo: Option<TipoDeRegra>,
        chave: &str,
        valor: &str,
        depois_de: &str,
    ) -> Resultado<usize> {
        Ok(Self::confirmacoes(inv)?
            .into_iter()
            .filter(|(t, c, v, em)| {
                Some(*t) == tipo && c == chave && v == valor && em.as_str() > depois_de
            })
            .count())
    }

    /// As regras ativas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn regras(&self) -> Resultado<Vec<RegraAprendida>> {
        let mut c = self.estado.prepare(
            "SELECT id, tipo, chave, valor, exemplos FROM regra_aprendida
              WHERE apagada_em IS NULL ORDER BY tipo, chave",
        )?;
        let linhas = c.query_map([], |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, i64>(4)?,
            ))
        })?;
        let mut saida = Vec::new();
        for l in linhas {
            let (id, tipo, chave, valor, n) = l?;
            let Some(tipo) = TipoDeRegra::de(&tipo) else {
                continue;
            };
            let quando = match chave.split_once(SEP) {
                Some((f, t)) => format!("{f} ({t})"),
                None => chave,
            };
            saida.push(RegraAprendida {
                id,
                tipo,
                quando,
                valor,
                exemplos: usize::try_from(n).unwrap_or(0),
            });
        }
        Ok(saida)
    }

    /// Apaga uma regra a pedido do usuário. Ela só volta com 3 confirmações novas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn apagar_regra(&self, id: i64) -> Resultado<bool> {
        Ok(self.estado.execute(
            "UPDATE regra_aprendida SET apagada_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
               motivo_da_morte = 'manual' WHERE id = ?1 AND apagada_em IS NULL",
            [id],
        )? > 0)
    }

    /// As regras de categoria no formato que a fila usa.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn categorias_para_a_fila(&self) -> Resultado<HashMap<(String, String), Aprendida>> {
        let mut c = self.estado.prepare(
            "SELECT chave, valor, exemplos FROM regra_aprendida
              WHERE apagada_em IS NULL AND tipo = 'categoria'",
        )?;
        let mut mapa = HashMap::new();
        for l in c.query_map([], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, i64>(2)?,
            ))
        })? {
            let (chave, categoria, n) = l?;
            if let Some((f, t)) = chave.split_once(SEP) {
                mapa.insert(
                    (f.to_owned(), t.to_owned()),
                    Aprendida {
                        categoria,
                        exemplos: usize::try_from(n).unwrap_or(0),
                    },
                );
            }
        }
        Ok(mapa)
    }

    /// Sugestões de tipo de documento pelas regras de nome de arquivo, para os achados em
    /// `alvos`. Não substitui a IA (ela ainda diz se é financeiro); a rodada obsoleta a
    /// sugestão repetida da IA ([`Agente::obsoletar_repetidas`]).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn sugestoes_de_tipo(
        &self,
        inv: &Connection,
        alvos: &[Alvo],
    ) -> Resultado<Vec<SugestaoDeRegra>> {
        let mut c = self.estado.prepare(
            "SELECT chave, valor, exemplos FROM regra_aprendida
              WHERE apagada_em IS NULL AND tipo = 'tipo_documento'",
        )?;
        let regras: HashMap<String, (String, i64)> = c
            .query_map([], |l| Ok((l.get(0)?, (l.get(1)?, l.get(2)?))))?
            .collect::<Result<_, _>>()?;
        if regras.is_empty() {
            return Ok(Vec::new());
        }
        let mut saida = Vec::new();
        for alvo in alvos {
            let Alvo::Achado(hash) = alvo else { continue };
            let nome: Option<String> = inv
                .query_row(
                    "SELECT filename FROM origem WHERE content_hash = ?1 ORDER BY id LIMIT 1",
                    [hash],
                    |l| l.get(0),
                )
                .optional()?;
            let Some((valor, n)) = nome.and_then(|n| regras.get(&padrao_do_nome(&n)).cloned())
            else {
                continue;
            };
            let n = usize::try_from(n).unwrap_or(0);
            saida.push(SugestaoDeRegra {
                alvo: alvo.clone(),
                campo: "tipo_documento",
                valor,
                confianca: if n >= 3 {
                    CONFIANCA_DA_REGRA_FORTE
                } else {
                    CONFIANCA_DA_REGRA
                },
                justificativa: format!("você pôs esse tipo em {n} arquivos com nome parecido"),
            });
        }
        Ok(saida)
    }

    /// Marca `obsoleta` a sugestão de tipo da IA quando uma regra já sugeriu o mesmo achado.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn obsoletar_repetidas(inv: &Connection) -> Resultado<usize> {
        Ok(inv.execute(
            "UPDATE sugestao_ia SET estado = 'obsoleta'
              WHERE estado = 'aberta' AND campo = 'tipo_documento' AND alvo_tipo = 'achado'
                AND modelo NOT LIKE 'regra:%'
                AND EXISTS (SELECT 1 FROM sugestao_ia r
                             WHERE r.alvo_tipo = 'achado' AND r.alvo_id = sugestao_ia.alvo_id
                               AND r.campo = 'tipo_documento' AND r.modelo LIKE 'regra:%'
                               AND r.estado = 'aberta')",
            [],
        )?)
    }

    // -- exemplos -------------------------------------------------------------------------

    /// Até [`EXEMPLOS_NO_PROMPT`] decisões do usuário sobre o mesmo fornecedor de cada item
    /// (as mais recentes). Sem o índice local de texto (fatia 6), o critério é o fornecedor.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn exemplos_para(inv: &Connection, alvos: &[Alvo]) -> Resultado<crate::entrada::Exemplos> {
        let mut mapa = crate::entrada::Exemplos::new();
        let mut reg = inv.prepare(
            "SELECT COALESCE(NULLIF(r.descricao_original, ''), r.supplier_name_raw, ''),
                    json_extract(d.depois, '$.categoria_provavel'), r.tipo
               FROM decisao d JOIN registro r ON CAST(r.id AS TEXT) = d.alvo_id
              WHERE d.alvo_tipo = 'registro' AND d.campo = 'categoria' AND d.autor = 'usuario'
                AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?2
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                AND r.id <> ?3
                AND (lower(r.fornecedor_normalizado) = ?1 OR lower(r.supplier_name_raw) = ?1)
              ORDER BY d.em DESC, d.id DESC LIMIT ?4",
        )?;
        let mut ach = inv.prepare(
            "SELECT COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = d.alvo_id
                               ORDER BY o.id LIMIT 1), ''),
                    json_extract(d.depois, '$.document_type')
               FROM decisao d JOIN achado a ON a.content_hash = d.alvo_id
              WHERE d.alvo_tipo = 'achado' AND d.campo = 'document_type' AND d.autor = 'usuario'
                AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?2
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                AND a.content_hash <> ?3 AND lower(a.supplier_name_raw) = ?1
              ORDER BY d.em DESC, d.id DESC LIMIT ?4",
        )?;
        let limite = i64::try_from(EXEMPLOS_NO_PROMPT).unwrap_or(8);
        for alvo in alvos {
            let chave = crate::entrada::chave_de_exemplos(alvo);
            let linhas: Vec<String> = match alvo {
                Alvo::Registro(id) => {
                    let nome: Option<String> = inv
                        .query_row(
                            "SELECT COALESCE(NULLIF(fornecedor_normalizado, ''), supplier_name_raw)
                               FROM registro WHERE id = ?1",
                            [id],
                            |l| l.get(0),
                        )
                        .optional()?
                        .flatten();
                    let Some(nome) = nome.filter(|n| !n.trim().is_empty()) else {
                        continue;
                    };
                    reg.query_map(
                        params![nome.to_lowercase(), MOTIVO_DA_DECISAO, id, limite],
                        |l| {
                            Ok(format!(
                                "\"{}\" ({}) -> categoria {}",
                                crate::normalizar::cortar(&l.get::<_, String>(0)?, 80),
                                l.get::<_, String>(2)?,
                                l.get::<_, String>(1)?
                            ))
                        },
                    )?
                    .collect::<Result<_, _>>()?
                }
                Alvo::Achado(hash) => {
                    let nome: Option<String> = inv
                        .query_row(
                            "SELECT supplier_name_raw FROM achado WHERE content_hash = ?1",
                            [hash],
                            |l| l.get(0),
                        )
                        .optional()?
                        .flatten();
                    let Some(nome) = nome.filter(|n| !n.trim().is_empty()) else {
                        continue;
                    };
                    ach.query_map(
                        params![nome.to_lowercase(), MOTIVO_DA_DECISAO, hash, limite],
                        |l| {
                            Ok(format!(
                                "arquivo \"{}\" -> tipo {}",
                                crate::normalizar::cortar(&l.get::<_, String>(0)?, 80),
                                l.get::<_, String>(1)?
                            ))
                        },
                    )?
                    .collect::<Result<_, _>>()?
                }
            };
            if !linhas.is_empty() {
                mapa.insert(chave, linhas);
            }
        }
        Ok(mapa)
    }

    // -- calibração -----------------------------------------------------------------------

    /// Recalcula o acerto por faixa: sugestões da IA (não das regras, não do piloto)
    /// aceitas contra recusadas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn atualizar_calibracao(&self, inv: &Connection) -> Resultado<Vec<Calibracao>> {
        let mut contagem = [(0_i64, 0_i64); FAIXAS.len()];
        let mut c = inv.prepare(
            "SELECT s.confianca, s.estado FROM sugestao_ia s
              WHERE s.estado IN ('aceita', 'recusada') AND s.modelo NOT LIKE 'regra:%'
                AND s.modelo <> '-' AND s.campo <> ?1",
        )?;
        for l in c.query_map([crate::sugestoes::NAO_CLASSIFICADO], |l| {
            Ok((l.get::<_, f64>(0)?, l.get::<_, String>(1)?))
        })? {
            let (conf, estado) = l?;
            let f = faixa_de(conf);
            if estado == "aceita" {
                contagem[f].0 += 1;
            } else {
                contagem[f].1 += 1;
            }
        }
        let tx = self.estado.unchecked_transaction()?;
        for (i, (a, r)) in contagem.iter().enumerate() {
            tx.execute(
                "INSERT OR REPLACE INTO calibracao (faixa, aceitas, recusadas, atualizada_em)
                 VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![i64::try_from(i).unwrap_or(0), a, r],
            )?;
        }
        tx.commit()?;
        self.calibracao()
    }

    /// A última calibração registrada.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn calibracao(&self) -> Resultado<Vec<Calibracao>> {
        let mut saida: Vec<Calibracao> = FAIXAS
            .iter()
            .map(|(_, _, n)| Calibracao {
                faixa: n,
                aceitas: 0,
                recusadas: 0,
            })
            .collect();
        let mut c = self
            .estado
            .prepare("SELECT faixa, aceitas, recusadas FROM calibracao")?;
        for l in c.query_map([], |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, i64>(1)?,
                l.get::<_, i64>(2)?,
            ))
        })? {
            let (f, a, r) = l?;
            if let Some(x) = usize::try_from(f).ok().and_then(|f| saida.get_mut(f)) {
                x.aceitas = a;
                x.recusadas = r;
            }
        }
        Ok(saida)
    }

    /// A confiança que vale: a da IA, ou o acerto observado da faixa se ele for menor e a
    /// amostra tiver ao menos [`AMOSTRA_MINIMA_DA_FAIXA`] decisões. Uma faixa que erra mais
    /// do que diz passa a valer menos; uma que acerta mais não sobe (cautela).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn confianca_calibrada(&self, confianca: f64) -> Resultado<f64> {
        let cal = self.calibracao()?;
        let f = &cal[faixa_de(confianca)];
        Ok(match f.acerto() {
            Some(a) if f.aceitas + f.recusadas >= AMOSTRA_MINIMA_DA_FAIXA => confianca.min(a),
            _ => confianca,
        })
    }

    // -- chegadas e painel ----------------------------------------------------------------

    /// Registra o que chegou desde a última vez: achados `novo`/`duvida` e registros fora do
    /// inventário já promovido. Na primeira chamada tudo o que já existe vira `inicial` e não
    /// conta como chegada. Devolve quantos chegaram agora.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn registrar_chegadas(&self, inv: &Connection) -> Resultado<usize> {
        let primeira: bool =
            self.estado
                .query_row("SELECT COUNT(*) = 0 FROM chegada", [], |l| l.get(0))?;
        let mut itens: Vec<(&'static str, String)> = Vec::new();
        let mut c = inv.prepare(
            "SELECT content_hash FROM achado WHERE estado IN ('novo', 'duvida')
                AND COALESCE(decidido_por, '') <> 'usuario'",
        )?;
        for l in c.query_map([], |l| l.get::<_, String>(0))? {
            itens.push(("achado", l?));
        }
        let mut c = inv.prepare(
            "SELECT id FROM registro WHERE estado <> 'promovido' AND movimentacao_id IS NULL
                AND COALESCE(decidido_por, '') <> 'usuario'",
        )?;
        for l in c.query_map([], |l| l.get::<_, i64>(0))? {
            itens.push(("registro", l?.to_string()));
        }
        let tx = self.estado.unchecked_transaction()?;
        let mut novos = 0;
        for (tipo, id) in &itens {
            let n = tx.execute(
                "INSERT OR IGNORE INTO chegada (alvo_tipo, alvo_id, inicial) VALUES (?1, ?2, ?3)",
                params![tipo, id, i64::from(primeira)],
            )?;
            if !primeira {
                novos += n;
            }
        }
        tx.commit()?;
        Ok(novos)
    }

    /// Os números do painel.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn painel(&self, inv: &Connection) -> Resultado<Painel> {
        let chegaram: i64 = self.estado.query_row(
            "SELECT COUNT(*) FROM chegada WHERE inicial = 0
                AND vista_em >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-7 day')",
            [],
            |l| l.get(0),
        )?;
        let sugeridas: i64 = inv.query_row(
            "SELECT COUNT(DISTINCT alvo_tipo || ':' || alvo_id) FROM sugestao_ia
              WHERE campo <> ?1 AND criada_em >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-7 day')",
            [crate::sugestoes::NAO_CLASSIFICADO],
            |l| l.get(0),
        )?;
        let abertas: i64 = inv.query_row(
            "SELECT COUNT(*) FROM sugestao_ia WHERE estado = 'aberta' AND campo <> ?1",
            [crate::sugestoes::NAO_CLASSIFICADO],
            |l| l.get(0),
        )?;
        let (aceitas, recusadas): (i64, i64) = inv.query_row(
            "SELECT COALESCE(SUM(estado = 'aceita'), 0), COALESCE(SUM(estado = 'recusada'), 0)
               FROM sugestao_ia
              WHERE estado IN ('aceita', 'recusada') AND modelo <> '-' AND campo <> ?1
                AND criada_em >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-7 day')",
            [crate::sugestoes::NAO_CLASSIFICADO],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )?;
        // Uma conta aproximada (sem as duplicatas nem os pares de conciliação): serve ao painel.
        let na_fila: i64 = inv.query_row(
            "SELECT (SELECT COUNT(*) FROM achado a WHERE a.estado IN ('novo', 'duvida')
                        AND COALESCE(a.decidido_por, '') <> 'usuario'
                        AND NOT EXISTS (SELECT 1 FROM sugestao_ia s WHERE s.alvo_tipo = 'achado'
                                          AND s.alvo_id = a.content_hash AND s.estado = 'aberta'))
                  + (SELECT COUNT(*) FROM registro r WHERE r.estado <> 'promovido'
                        AND r.movimentacao_id IS NULL AND COALESCE(r.decidido_por, '') <> 'usuario'
                        AND NOT EXISTS (SELECT 1 FROM sugestao_ia s WHERE s.alvo_tipo = 'registro'
                                          AND s.alvo_id = CAST(r.id AS TEXT) AND s.estado = 'aberta'))",
            [],
            |l| l.get(0),
        )?;
        let mut prontas = 0;
        let mut c = inv.prepare(
            "SELECT confianca FROM sugestao_ia WHERE estado = 'aberta' AND campo <> ?1 AND confianca >= 0.85",
        )?;
        for l in c.query_map([crate::sugestoes::NAO_CLASSIFICADO], |l| l.get::<_, f64>(0))? {
            if self.confianca_calibrada(l?)? >= 0.85 {
                prontas += 1;
            }
        }
        Ok(Painel {
            chegaram: usize::try_from(chegaram).unwrap_or(0),
            sugeridas: usize::try_from(sugeridas).unwrap_or(0),
            abertas: usize::try_from(abertas).unwrap_or(0),
            prontas,
            regras: self.regras()?,
            aceitas_da_semana: aceitas,
            recusadas_da_semana: recusadas,
            na_fila: usize::try_from(na_fila).unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padrao_do_nome_troca_digitos_e_tira_extensao() {
        assert_eq!(
            padrao_do_nome("Fatura-Luz_Out-2026.pdf"),
            "fatura luz out #"
        );
        assert_eq!(padrao_do_nome("Conta 03 de 2026.PDF"), "conta # de #");
        assert_eq!(padrao_do_nome("Fatura 2026-10-05.pdf"), "fatura #");
        assert_eq!(padrao_do_nome("sem extensão"), "sem extensao");
        assert_eq!(padrao_do_nome("a.tar.gz"), "a tar");
    }

    #[test]
    fn faixas_cobrem_0_a_1() {
        assert_eq!(faixa_de(0.0), 0);
        assert_eq!(faixa_de(0.69), 0);
        assert_eq!(faixa_de(0.7), 1);
        assert_eq!(faixa_de(0.9), 2);
        assert_eq!(faixa_de(0.95), 3);
        assert_eq!(faixa_de(1.0), 3);
    }
}

// -- o ciclo ------------------------------------------------------------------------------

/// O que um ciclo do agente fez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumoDoCiclo {
    /// Itens que chegaram desde o ciclo anterior.
    pub chegaram: usize,
    /// Sugestões de regra gravadas, de graça.
    pub por_regra: usize,
    /// Itens enviados à IA neste lote.
    pub enviados: usize,
    /// Itens que ainda esperam na fila (depois deste lote).
    pub na_fila: usize,
    /// Sugestões que a IA gravou neste lote.
    pub sugestoes: usize,
    /// Por que o lote parou antes do fim (teto, limite, provedor fora).
    pub aviso: Option<String>,
}

/// Um ciclo do agente: registra o que chegou, atualiza regras e calibração, grava as sugestões
/// de regra (de graça) e roda **um lote** de até `lote` itens pela IA com o prompt v2 e as
/// decisões parecidas. Nunca decide: só grava sugestões.
///
/// # Errors
/// Falha do SQLite. Falha do provedor não é erro: volta em [`ResumoDoCiclo::aviso`].
#[allow(clippy::too_many_arguments)]
pub fn rodar_ciclo(
    inv: &Connection,
    agente: &Agente,
    motor: &std::sync::Arc<sbf_ia::Fila>,
    provedor: &str,
    lote: usize,
    controle: &sbf_ia::Controle,
    progresso: &std::sync::Mutex<crate::rodada::Progresso>,
    ao_abrir: &dyn Fn(i64),
    rastro: Option<crate::rodada::RastroDeFalhas>,
) -> Resultado<ResumoDoCiclo> {
    use crate::fila::{OpcoesDaFila, selecionar};
    use crate::rodada::{ConfigDaRodada, SituacaoDaRodada, rodar};
    use crate::sugestoes::gravar_regras;

    let mut resumo = ResumoDoCiclo {
        chegaram: agente.registrar_chegadas(inv)?,
        ..ResumoDoCiclo::default()
    };
    agente.atualizar_regras(inv)?;
    agente.atualizar_calibracao(inv)?;
    let opcoes = OpcoesDaFila {
        prompt_versao: PROMPT_VERSAO_V2.to_owned(),
        regras_categoria: Some(agente.categorias_para_a_fila()?),
        ..OpcoesDaFila::padrao()
    };
    let mut fila = selecionar(inv, &opcoes)?;
    // Item que já tem sugestão aberta (de qualquer versão) espera você: não vai de novo.
    let abertas: std::collections::HashSet<(String, String)> = {
        let mut c = inv.prepare(
            "SELECT DISTINCT alvo_tipo, alvo_id FROM sugestao_ia WHERE estado = 'aberta'",
        )?;
        c.query_map([], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    fila.alvos
        .retain(|a| !abertas.contains(&(a.tipo().to_owned(), a.id())));
    resumo.por_regra += gravar_regras(inv, &fila.regras)?;
    let de_tipo = agente.sugestoes_de_tipo(inv, &fila.alvos)?;
    resumo.por_regra += gravar_regras(inv, &de_tipo)?;
    let total = fila.alvos.len();
    fila.alvos.truncate(lote);
    resumo.enviados = fila.alvos.len();
    resumo.na_fila = total - fila.alvos.len();
    if fila.alvos.is_empty() {
        return Ok(resumo);
    }
    let exemplos = Agente::exemplos_para(inv, &fila.alvos)?;
    let config = ConfigDaRodada {
        prompt_versao: PROMPT_VERSAO_V2.to_owned(),
        rastro,
        exemplos: Some(std::sync::Arc::new(exemplos)),
        ..ConfigDaRodada::default()
    };
    let p = rodar(
        inv,
        motor,
        provedor,
        &fila.alvos,
        &config,
        controle,
        progresso,
        ao_abrir,
    )?;
    resumo.sugestoes = p.sugestoes;
    if matches!(
        p.situacao,
        SituacaoDaRodada::Falhou | SituacaoDaRodada::Pausada | SituacaoDaRodada::Cancelada
    ) {
        resumo.aviso = p.aviso;
    }
    Agente::obsoletar_repetidas(inv)?;
    Ok(resumo)
}
