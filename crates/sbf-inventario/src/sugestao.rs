//! A triagem por IA no banco (spec 09): o que o núcleo precisa saber das sugestões e o
//! [`RegistroDaIa`], que implementa `sbf_ia::Registro` sobre `saida_ia` e `cache_ia`.
//!
//! Uma sugestão **nunca** é decisão. O núcleo só faz uma coisa com elas: quando o usuário
//! decide um campo por qualquer caminho, a sugestão aberta daquele campo vira `obsoleta`
//! na mesma transação da decisão (IA-08). Quem cria, lista, aceita e recusa sugestões é o
//! crate `sbf-triagem`.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use rusqlite::{Connection, Transaction, params};
use sbf_ia::{Consumo, ErroDeIa, NivelDeConfianca, Registro, ResultadoDeIa, Saida};
use serde_json::Value;

use crate::banco::ESPERA_OCUPADO;
use crate::projecao::campo;

/// Os campos que a IA sugere para um achado.
pub const CAMPOS_DO_ACHADO: [&str; 5] = [
    "e_financeiro",
    "tipo_documento",
    "fornecedor",
    "data_chave",
    "valor",
];

/// Os campos que a IA sugere para um registro.
pub const CAMPOS_DO_REGISTRO: [&str; 5] =
    ["categoria", "fornecedor", "conta", "tipo", "duplicata_de"];

/// O campo marcador de "a IA não conseguiu" (IA-03). Nunca aparece na lista.
pub const CAMPO_NAO_CLASSIFICADO: &str = "nao_classificado";

/// O campo da decisão (`decisao.campo`) que decide o campo da sugestão, para o alvo.
/// `None`: nenhuma decisão do §10 cobre o campo (conta e duplicata de registro).
pub fn campo_da_decisao(alvo_tipo: &str, campo_da_sugestao: &str) -> Option<&'static str> {
    match (alvo_tipo, campo_da_sugestao) {
        ("achado", "e_financeiro") => Some(campo::ESTADO),
        ("achado", "tipo_documento") => Some(campo::DOCUMENT_TYPE),
        ("achado", "fornecedor") | ("registro", "fornecedor") => Some(campo::FORNECEDOR),
        ("achado", "data_chave" | "valor") => Some(campo::DATA_E_VALOR),
        ("registro", "categoria") => Some(campo::CATEGORIA),
        ("registro", "tipo") => Some(campo::TIPO),
        _ => None,
    }
}

/// Torna `obsoleta` a sugestão aberta que o usuário acabou de decidir por outro caminho.
/// Num banco sem a migração 3 (fixtures antigas) não faz nada.
pub(crate) fn obsoletar(
    tx: &Transaction<'_>,
    alvo_tipo: &str,
    alvo_id: &str,
    campo_decidido: &str,
    depois: Option<&Value>,
) -> rusqlite::Result<()> {
    let candidatos: &[&str] = match (alvo_tipo, campo_decidido) {
        ("achado", c) if c == campo::ESTADO => &["e_financeiro"],
        ("achado", c) if c == campo::DOCUMENT_TYPE => &["tipo_documento"],
        (_, c) if c == campo::FORNECEDOR => &["fornecedor"],
        ("registro", c) if c == campo::CATEGORIA => &["categoria"],
        ("registro", c) if c == campo::TIPO => &["tipo"],
        ("achado", c) if c == campo::DATA_E_VALOR => &["data_chave", "valor"],
        _ => return Ok(()),
    };
    let tem = |chaves: &[&str]| {
        depois
            .and_then(Value::as_object)
            .is_some_and(|o| chaves.iter().any(|k| o.contains_key(*k)))
    };
    for campo_da_sugestao in candidatos {
        let afetado = match *campo_da_sugestao {
            "data_chave" => tem(&["data_chave", "data"]),
            "valor" => tem(&["valor_centavos"]),
            _ => true,
        };
        if !afetado {
            continue;
        }
        let resultado = tx.execute(
            "UPDATE sugestao_ia SET estado = 'obsoleta'
              WHERE alvo_tipo = ?1 AND alvo_id = ?2 AND campo = ?3 AND estado = 'aberta'",
            params![alvo_tipo, alvo_id, campo_da_sugestao],
        );
        match resultado {
            Ok(_) => {}
            Err(e) if e.to_string().contains("no such table") => return Ok(()),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn nivel_para_texto(nivel: NivelDeConfianca) -> &'static str {
    match nivel {
        NivelDeConfianca::Mascarado => "mascarado",
        NivelDeConfianca::Parcial => "parcial",
        NivelDeConfianca::Completo => "completo",
    }
}

fn erro_de_ia(e: rusqlite::Error) -> ErroDeIa {
    ErroDeIa::Outro(format!("banco: {e}"))
}

/// `sbf_ia::Registro` sobre `saida_ia` e `cache_ia` do inventário. Abre a própria conexão
/// (WAL, espera por lock) e a guarda atrás de um `Mutex`, porque o trait é `Sync`.
/// Nunca grava texto enviado: só o que a [`Saida`] carrega.
#[derive(Debug)]
pub struct RegistroDaIa {
    conexao: Mutex<Connection>,
    /// `SEM_RODADA` quando nenhuma rodada está aberta.
    rodada_id: AtomicI64,
}

const SEM_RODADA: i64 = i64::MIN;

impl RegistroDaIa {
    /// Abre o banco em `caminho`, já migrado (versão 3). `rodada_id` entra em cada
    /// `saida_ia` (o lote de triagem).
    ///
    /// # Errors
    /// Falha do SQLite ao abrir.
    pub fn abrir(caminho: impl AsRef<Path>, rodada_id: Option<i64>) -> rusqlite::Result<Self> {
        let conexao = Connection::open(caminho)?;
        conexao.busy_timeout(ESPERA_OCUPADO)?;
        Ok(Self::sobre(conexao, rodada_id))
    }

    /// Sobre uma conexão sua (testes).
    pub fn sobre(conexao: Connection, rodada_id: Option<i64>) -> Self {
        Self {
            conexao: Mutex::new(conexao),
            rodada_id: AtomicI64::new(rodada_id.unwrap_or(SEM_RODADA)),
        }
    }

    /// Passa a marcar as próximas saídas com a rodada `id` (a rodada nasce depois do
    /// registro, na hora de iniciar a triagem).
    pub fn definir_rodada(&self, id: Option<i64>) {
        self.rodada_id
            .store(id.unwrap_or(SEM_RODADA), Ordering::SeqCst);
    }

    fn conexao(&self) -> ResultadoDeIa<std::sync::MutexGuard<'_, Connection>> {
        self.conexao
            .lock()
            .map_err(|_| ErroDeIa::Outro("conexão do registro envenenada".into()))
    }
}

impl RegistroDaIa {
    fn inserir(&self, saida: &Saida) -> ResultadoDeIa<i64> {
        let c = self.conexao()?;
        c.execute(
            "INSERT INTO saida_ia (destinatario, provedor, modelo, nivel, bytes_enviados,
               tokens_entrada, tokens_saida, custo_dezmilesimos_usd, nominal, quando, rodada_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                saida.destinatario,
                saida.provedor,
                saida.modelo,
                nivel_para_texto(saida.nivel),
                i64::try_from(saida.bytes_enviados).unwrap_or(i64::MAX),
                i64::try_from(saida.consumo.tokens_entrada).unwrap_or(i64::MAX),
                i64::try_from(saida.consumo.tokens_saida).unwrap_or(i64::MAX),
                i64::try_from(saida.consumo.custo_dezmilesimos_usd).unwrap_or(i64::MAX),
                saida.consumo.nominal,
                saida.quando,
                Some(self.rodada_id.load(Ordering::SeqCst)).filter(|r| *r != SEM_RODADA),
            ],
        )
        .map_err(erro_de_ia)?;
        Ok(c.last_insert_rowid())
    }
}

impl Registro for RegistroDaIa {
    fn registrar_saida(&self, saida: &Saida) -> ResultadoDeIa<()> {
        self.inserir(saida).map(|_| ())
    }

    fn abrir_saida(&self, saida: &Saida) -> ResultadoDeIa<Option<i64>> {
        self.inserir(saida).map(Some)
    }

    fn fechar_saida(&self, id: i64, consumo: Consumo) -> ResultadoDeIa<()> {
        let c = self.conexao()?;
        c.execute(
            "UPDATE saida_ia SET tokens_entrada = ?2, tokens_saida = ?3,
               custo_dezmilesimos_usd = ?4, nominal = ?5 WHERE id = ?1",
            params![
                id,
                i64::try_from(consumo.tokens_entrada).unwrap_or(i64::MAX),
                i64::try_from(consumo.tokens_saida).unwrap_or(i64::MAX),
                i64::try_from(consumo.custo_dezmilesimos_usd).unwrap_or(i64::MAX),
                consumo.nominal,
            ],
        )
        .map_err(erro_de_ia)?;
        Ok(())
    }

    fn ler_cache(&self, chave: &str) -> ResultadoDeIa<Option<Value>> {
        let c = self.conexao()?;
        let texto: Option<String> = c
            .query_row(
                "SELECT resposta_json FROM cache_ia WHERE chave = ?1",
                [chave],
                |l| l.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                outro => Err(outro),
            })
            .map_err(erro_de_ia)?;
        Ok(texto.and_then(|t| serde_json::from_str(&t).ok()))
    }

    fn gravar_cache(&self, chave: &str, json: &Value) -> ResultadoDeIa<()> {
        let c = self.conexao()?;
        c.execute(
            "INSERT OR REPLACE INTO cache_ia (chave, resposta_json) VALUES (?1, ?2)",
            params![chave, json.to_string()],
        )
        .map_err(erro_de_ia)?;
        Ok(())
    }
}

#[cfg(test)]
mod testes_do_registro {
    use super::*;

    fn registro() -> RegistroDaIa {
        let c = Connection::open_in_memory().expect("memória");
        c.execute_batch(
            "CREATE TABLE saida_ia (id INTEGER PRIMARY KEY, destinatario TEXT NOT NULL,
               provedor TEXT NOT NULL, modelo TEXT NOT NULL, nivel TEXT NOT NULL,
               bytes_enviados INTEGER NOT NULL, tokens_entrada INTEGER NOT NULL DEFAULT 0,
               tokens_saida INTEGER NOT NULL DEFAULT 0,
               custo_dezmilesimos_usd INTEGER NOT NULL DEFAULT 0,
               nominal INTEGER NOT NULL DEFAULT 0, quando TEXT NOT NULL, rodada_id INTEGER);",
        )
        .expect("tabela");
        RegistroDaIa::sobre(c, None)
    }

    fn saida() -> Saida {
        Saida {
            destinatario: "rita".into(),
            provedor: "local".into(),
            modelo: "m".into(),
            nivel: NivelDeConfianca::Completo,
            bytes_enviados: 42,
            consumo: Consumo::default(),
            quando: "2026-11-20T00:00:00Z".into(),
        }
    }

    #[test]
    fn abrir_grava_antes_e_fechar_completa_a_mesma_linha() {
        let r = registro();
        let id = r.abrir_saida(&saida()).expect("abrir").expect("id");
        let c = r.conexao().expect("conexão");
        let (bytes, tokens): (i64, i64) = c
            .query_row(
                "SELECT bytes_enviados, tokens_entrada FROM saida_ia WHERE id = ?1",
                [id],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .expect("linha");
        assert_eq!((bytes, tokens), (42, 0), "a linha existe antes da resposta");
        drop(c);
        r.fechar_saida(
            id,
            Consumo {
                tokens_entrada: 900,
                tokens_saida: 40,
                custo_dezmilesimos_usd: 7,
                nominal: false,
            },
        )
        .expect("fechar");
        let c = r.conexao().expect("conexão");
        let (n, entrada, custo): (i64, i64, i64) = c
            .query_row(
                "SELECT COUNT(*), MAX(tokens_entrada), MAX(custo_dezmilesimos_usd) FROM saida_ia",
                [],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
            )
            .expect("soma");
        assert_eq!((n, entrada, custo), (1, 900, 7), "uma linha só, completada");
    }

    #[test]
    fn sem_a_tabela_abrir_falha() {
        let r = RegistroDaIa::sobre(Connection::open_in_memory().expect("memória"), None);
        assert!(r.abrir_saida(&saida()).is_err());
    }
}
