//! `sugestao_ia` e `rodada_ia` (migração 3 do `sbf-inventario`). Uma sugestão nunca é
//! decisão: aceitar é [`crate::aceite`], e quem decide por outro caminho a deixa
//! `obsoleta` (o núcleo faz isso na mesma transação da decisão, IA-08).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::erro::Resultado;
use crate::fila::{MODELO_DA_REGRA_DE_CATEGORIA, SugestaoDeRegra, VERSAO_DAS_REGRAS};
use crate::resposta::SugestaoValida;
use crate::tipos::Alvo;

/// O `campo` do marcador "a IA não conseguiu" (IA-03).
pub const NAO_CLASSIFICADO: &str = sbf_inventario::sugestao::CAMPO_NAO_CLASSIFICADO;

/// Abre uma conexão de escrita ao banco já migrado, com a espera por lock do inventário.
///
/// # Errors
/// Falha do SQLite.
pub fn abrir(caminho: impl AsRef<Path>) -> Resultado<Connection> {
    let c = Connection::open(caminho)?;
    c.busy_timeout(Duration::from_secs(30))?;
    c.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(c)
}

/// O que o provedor `provedor` já gastou **neste mês** (UTC), em dezmilésimos de dólar, a
/// partir de `saida_ia`. Só conta o que não é nominal (a assinatura Claude não gasta cota
/// em dólar). Quem monta o OpenRouter restaura o teto do mês com este valor ao abrir o app.
///
/// # Errors
/// Falha do SQLite.
pub fn gasto_do_mes_dezmilesimos(caminho: impl AsRef<Path>, provedor: &str) -> Resultado<u64> {
    let c = abrir(caminho)?;
    let soma: i64 = c.query_row(
        "SELECT COALESCE(SUM(custo_dezmilesimos_usd), 0) FROM saida_ia
          WHERE provedor = ?1 AND nominal = 0
            AND substr(quando, 1, 7) = strftime('%Y-%m', 'now')",
        [provedor],
        |l| l.get(0),
    )?;
    Ok(u64::try_from(soma).unwrap_or(0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDaSugestao {
    Aberta,
    Aceita,
    Recusada,
    Obsoleta,
}

impl EstadoDaSugestao {
    pub fn codigo(self) -> &'static str {
        match self {
            Self::Aberta => "aberta",
            Self::Aceita => "aceita",
            Self::Recusada => "recusada",
            Self::Obsoleta => "obsoleta",
        }
    }
}

/// Uma sugestão a gravar.
#[derive(Debug, Clone, PartialEq)]
pub struct NovaSugestao {
    pub alvo: Alvo,
    pub campo: String,
    pub valor: Value,
    pub confianca: f64,
    pub justificativa: String,
    pub modelo: String,
    pub prompt_versao: String,
    pub rodada_id: Option<i64>,
    pub estado: EstadoDaSugestao,
}

impl NovaSugestao {
    pub fn da_ia(
        alvo: &Alvo,
        s: &SugestaoValida,
        modelo: &str,
        versao: &str,
        rodada_id: i64,
        estado: EstadoDaSugestao,
    ) -> Self {
        Self {
            alvo: alvo.clone(),
            campo: s.campo.clone(),
            valor: s.valor.clone(),
            confianca: s.confianca,
            justificativa: s.justificativa.clone(),
            modelo: modelo.to_owned(),
            prompt_versao: versao.to_owned(),
            rodada_id: Some(rodada_id),
            estado,
        }
    }

    /// O item que a IA não conseguiu classificar duas vezes (IA-03). Fica `recusada` para
    /// não aparecer na lista nem voltar à fila desta versão do prompt.
    pub fn nao_classificado(alvo: &Alvo, versao: &str, rodada_id: i64) -> Self {
        Self {
            alvo: alvo.clone(),
            campo: NAO_CLASSIFICADO.to_owned(),
            valor: Value::Null,
            confianca: 0.0,
            justificativa: "a IA não devolveu uma resposta válida em duas tentativas".to_owned(),
            modelo: "-".to_owned(),
            prompt_versao: versao.to_owned(),
            rodada_id: Some(rodada_id),
            estado: EstadoDaSugestao::Recusada,
        }
    }
}

/// Grava uma sugestão e devolve o id.
///
/// # Errors
/// Falha do SQLite.
pub fn gravar(conn: &Connection, s: &NovaSugestao) -> Resultado<i64> {
    conn.execute(
        "INSERT INTO sugestao_ia (alvo_tipo, alvo_id, campo, valor_json, confianca, justificativa,
           modelo, prompt_versao, rodada_id, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            s.alvo.tipo(),
            s.alvo.id(),
            s.campo,
            s.valor.to_string(),
            s.confianca,
            s.justificativa,
            s.modelo,
            s.prompt_versao,
            s.rodada_id,
            s.estado.codigo(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Grava as sugestões que as regras gratuitas acharam, sem repetir a que já existe
/// (aberta, aceita ou recusada). Devolve quantas gravou.
///
/// # Errors
/// Falha do SQLite.
pub fn gravar_regras(conn: &Connection, regras: &[SugestaoDeRegra]) -> Resultado<usize> {
    let mut gravadas = 0;
    for r in regras {
        let existe: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sugestao_ia
                             WHERE alvo_tipo = ?1 AND alvo_id = ?2 AND campo = ?3 AND modelo = ?4
                               AND estado IN ('aberta', 'aceita', 'recusada'))",
            params![
                r.alvo.tipo(),
                r.alvo.id(),
                r.campo,
                MODELO_DA_REGRA_DE_CATEGORIA
            ],
            |l| l.get(0),
        )?;
        if existe {
            continue;
        }
        gravar(
            conn,
            &NovaSugestao {
                alvo: r.alvo.clone(),
                campo: r.campo.to_owned(),
                valor: Value::String(r.valor.clone()),
                confianca: r.confianca,
                justificativa: r.justificativa.clone(),
                modelo: MODELO_DA_REGRA_DE_CATEGORIA.to_owned(),
                prompt_versao: VERSAO_DAS_REGRAS.to_owned(),
                rodada_id: None,
                estado: EstadoDaSugestao::Aberta,
            },
        )?;
        gravadas += 1;
    }
    Ok(gravadas)
}

/// Muda o estado das sugestões `ids`. Só mexe nas que estão `aberta` ou `obsoleta` (a
/// aceitação passa por `obsoleta` porque a própria decisão a obsoleta antes).
///
/// # Errors
/// Falha do SQLite.
pub fn marcar(conn: &Connection, ids: &[i64], estado: EstadoDaSugestao) -> Resultado<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE sugestao_ia SET estado = ?2 WHERE id = ?1 AND estado IN ('aberta', 'obsoleta')",
            params![id, estado.codigo()],
        )?;
    }
    Ok(n)
}

/// Quantas sugestões abertas há (a contagem da área).
///
/// # Errors
/// Falha do SQLite.
pub fn contar_abertas(conn: &Connection) -> Resultado<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sugestao_ia WHERE estado = 'aberta' AND campo <> ?1",
        [NAO_CLASSIFICADO],
        |l| l.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}

// ---------------------------------------------------------------------------------------
// Rodadas

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rodada {
    pub id: i64,
    pub iniciada_em: String,
    pub terminada_em: Option<String>,
    pub itens: i64,
    pub chamadas: i64,
    pub custo_nominal_centavos_usd: i64,
    pub provedor: String,
    pub situacao: String,
    pub piloto: bool,
}

/// Abre uma rodada.
///
/// # Errors
/// Falha do SQLite.
pub fn iniciar_rodada(conn: &Connection, provedor: &str, piloto: bool) -> Resultado<i64> {
    conn.execute(
        "INSERT INTO rodada_ia (provedor, piloto) VALUES (?1, ?2)",
        params![provedor, piloto],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Atualiza os totais e a situação; `terminada` carimba `terminada_em`.
///
/// # Errors
/// Falha do SQLite.
pub fn atualizar_rodada(
    conn: &Connection,
    id: i64,
    itens: i64,
    chamadas: i64,
    custo_centavos_usd: i64,
    situacao: &str,
    terminada: bool,
) -> Resultado<()> {
    conn.execute(
        "UPDATE rodada_ia SET itens = ?2, chamadas = ?3, custo_nominal_centavos_usd = ?4,
                situacao = ?5,
                terminada_em = CASE WHEN ?6 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END
          WHERE id = ?1",
        params![id, itens, chamadas, custo_centavos_usd, situacao, terminada],
    )?;
    Ok(())
}

/// Lê uma rodada.
///
/// # Errors
/// Falha do SQLite.
pub fn ler_rodada(conn: &Connection, id: i64) -> Resultado<Option<Rodada>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT id, iniciada_em, terminada_em, itens, chamadas, custo_nominal_centavos_usd,
                    provedor, situacao, piloto FROM rodada_ia WHERE id = ?1",
            [id],
            |l| {
                Ok(Rodada {
                    id: l.get(0)?,
                    iniciada_em: l.get(1)?,
                    terminada_em: l.get(2)?,
                    itens: l.get(3)?,
                    chamadas: l.get(4)?,
                    custo_nominal_centavos_usd: l.get(5)?,
                    provedor: l.get(6)?,
                    situacao: l.get(7)?,
                    piloto: l.get(8)?,
                })
            },
        )
        .optional()?)
}

// ---------------------------------------------------------------------------------------
// Lista da área

/// Uma sugestão aberta, para a lista.
#[derive(Debug, Clone, PartialEq)]
pub struct SugestaoListada {
    pub id: i64,
    pub alvo: Alvo,
    /// O nome para mostrar: a descrição do registro ou o arquivo do achado.
    pub titulo: String,
    pub campo: String,
    pub valor: Value,
    pub confianca: f64,
    pub justificativa: String,
    pub modelo: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FiltroDeSugestoes {
    /// Só as com esta confiança ou mais.
    pub confianca_minima: Option<f64>,
    pub campo: Option<String>,
}

/// As sugestões abertas, da mais confiante para a menos.
///
/// # Errors
/// Falha do SQLite.
pub fn listar_abertas(
    conn: &Connection,
    filtro: &FiltroDeSugestoes,
) -> Resultado<Vec<SugestaoListada>> {
    let mut c = conn.prepare(
        "SELECT s.id, s.alvo_tipo, s.alvo_id, s.campo, s.valor_json, s.confianca,
                s.justificativa, s.modelo,
                CASE s.alvo_tipo
                  WHEN 'registro' THEN (SELECT COALESCE(NULLIF(r.descricao_original, ''),
                                                        r.supplier_name_raw, 'registro ' || r.id)
                                          FROM registro r WHERE CAST(r.id AS TEXT) = s.alvo_id)
                  ELSE (SELECT COALESCE(NULLIF(o.gmail_subject, ''), o.filename)
                          FROM origem o WHERE o.content_hash = s.alvo_id ORDER BY o.id LIMIT 1)
                END
           FROM sugestao_ia s
          WHERE s.estado = 'aberta' AND s.campo <> ?1
            AND s.confianca >= ?2 AND (?3 IS NULL OR s.campo = ?3)
          ORDER BY s.confianca DESC, s.id",
    )?;
    let linhas = c.query_map(
        params![
            NAO_CLASSIFICADO,
            filtro.confianca_minima.unwrap_or(0.0),
            filtro.campo
        ],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, Option<String>>(4)?,
                l.get::<_, f64>(5)?,
                l.get::<_, String>(6)?,
                l.get::<_, String>(7)?,
                l.get::<_, Option<String>>(8)?,
            ))
        },
    )?;
    let mut saida = Vec::new();
    for linha in linhas {
        let (id, tipo, alvo_id, campo, valor, confianca, justificativa, modelo, titulo) = linha?;
        let Some(alvo) = Alvo::de(&tipo, &alvo_id) else {
            continue;
        };
        saida.push(SugestaoListada {
            id,
            titulo: titulo.unwrap_or_else(|| alvo_id.clone()),
            alvo,
            campo,
            valor: valor
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or(Value::Null),
            confianca,
            justificativa,
            modelo,
        });
    }
    Ok(saida)
}

/// "142 registros → categoria Mercado": as sugestões abertas que dizem a mesma coisa.
#[derive(Debug, Clone, PartialEq)]
pub struct GrupoDeSugestoes {
    pub alvo_tipo: &'static str,
    pub campo: String,
    pub valor: Value,
    /// O valor em texto, para o rótulo.
    pub valor_texto: String,
    pub sugestoes: Vec<SugestaoListada>,
}

impl GrupoDeSugestoes {
    pub fn ids(&self) -> Vec<i64> {
        self.sugestoes.iter().map(|s| s.id).collect()
    }
}

/// O nome do campo para a tela.
pub fn rotulo_do_campo(alvo_tipo: &str, campo: &str) -> &'static str {
    match (alvo_tipo, campo) {
        (_, "e_financeiro") => "é financeiro",
        (_, "tipo_documento") => "tipo de documento",
        (_, "fornecedor") => "fornecedor",
        (_, "data_chave") => "data-chave",
        (_, "valor") => "valor",
        (_, "categoria") => "categoria",
        (_, "conta") => "conta",
        (_, "tipo") => "tipo",
        (_, "duplicata_de") => "duplicata de",
        _ => "campo",
    }
}

/// O valor em texto para a tela: centavos viram reais, o resto passa como veio.
pub fn texto_do_valor(campo: &str, valor: &Value) -> String {
    match (campo, valor) {
        ("valor", Value::Number(n)) => crate::entrada::formatar_reais(n.as_i64().unwrap_or(0)),
        (_, Value::String(s)) => s.clone(),
        (_, outro) => outro.to_string(),
    }
}

/// Agrupa por (tipo de alvo, campo, valor): o maior grupo primeiro.
pub fn agrupar(sugestoes: Vec<SugestaoListada>) -> Vec<GrupoDeSugestoes> {
    let mut mapa: BTreeMap<(&'static str, String, String), GrupoDeSugestoes> = BTreeMap::new();
    for s in sugestoes {
        let tipo = s.alvo.tipo();
        let chave = (tipo, s.campo.clone(), s.valor.to_string());
        mapa.entry(chave)
            .or_insert_with(|| GrupoDeSugestoes {
                alvo_tipo: tipo,
                campo: s.campo.clone(),
                valor_texto: texto_do_valor(&s.campo, &s.valor),
                valor: s.valor.clone(),
                sugestoes: Vec::new(),
            })
            .sugestoes
            .push(s);
    }
    let mut grupos: Vec<GrupoDeSugestoes> = mapa.into_values().collect();
    grupos.sort_by(|a, b| {
        b.sugestoes
            .len()
            .cmp(&a.sugestoes.len())
            .then_with(|| a.campo.cmp(&b.campo))
            .then_with(|| a.valor_texto.cmp(&b.valor_texto))
    });
    grupos
}
