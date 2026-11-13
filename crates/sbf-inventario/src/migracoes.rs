//! Dono: agente do núcleo (spec 06, "O banco"). Ver `lib.rs`.
//!
//! A escada do `inventario.sqlite`, por `PRAGMA user_version`:
//!
//! - **1**: o esquema do Bun na versão "2" ([`esquema::MIGRACAO_1_BASE_BUN`]). Um banco do
//!   Bun (`meta.schema_versao='2'`, `user_version=0`) é **adotado**: as tabelas são
//!   conferidas e só se carimba `user_version=1`, sem rodar DDL.
//! - **2**: o log de decisões, lotes, `decidido_por` e índices
//!   ([`esquema::MIGRACAO_2_DECISOES_E_LOTES`]), as colunas que o `aplicarSchema` do Bun
//!   acrescentava por `garantirColuna` (para o banco novo e o adotado ficarem iguais) e o
//!   trigger que faz de `decisao` uma tabela só de acréscimo (INV-07), mais
//!   [`GATILHO_QUE_RECUSA_SUBSTITUIR`], que vale também para conexões de fora do app.
//!
//! Regra de ouro: migração aplicada num banco real nunca é editada; só se acrescenta uma.

use bancada_armazem::{Escada, LinhaDeBase, Migracao, gerar_sql_somente_acrescimo};
use rusqlite::{Connection, Transaction};

use crate::esquema;

/// A versão de esquema que esta build entende e produz.
pub const VERSAO_ATUAL: u32 = 2;

/// O valor de `meta.schema_versao` que o Bun grava no esquema adotado.
pub const VERSAO_DO_BUN_ADOTADA: &str = "2";

/// As tabelas do esquema do Bun "2" (migração 1), conferidas na adoção.
pub const TABELAS_DA_BASE: &[&str] = &[
    "meta",
    "ciclo",
    "fonte",
    "varredura",
    "achado",
    "texto",
    "origem",
    "area_excluida",
    "registro",
    "sub_registro",
    "relacao",
    "regra_senha",
    "tentativa_senha",
    "pendencia",
    "refazer",
];

/// Colunas que o `aplicarSchema` do Bun acrescenta por `garantirColuna` e que não estão no
/// `schema.sql` congelado na migração 1. A migração 2 as cria quando faltam.
pub const COLUNAS_DO_GARANTIR_COLUNA: &[(&str, &str, &str)] = &[
    ("varredura", "resumo", "TEXT"),
    ("registro", "tipo_original", "TEXT"),
    ("registro", "contraparte", "TEXT"),
    ("registro", "contraparte_doc", "TEXT"),
    ("registro", "data_compra", "TEXT"),
    ("registro", "merchant_cnpj", "TEXT"),
    ("registro", "fatura_ref", "TEXT"),
];

const MARCADOR_DO_BUN: &str = "SELECT valor FROM meta WHERE chave = 'schema_versao'";

static MIGRACOES: [Migracao; 2] = [
    Migracao::sql(
        "esquema do inventário do Bun, versão 2",
        esquema::MIGRACAO_1_BASE_BUN,
    ),
    Migracao::codigo(
        "log de decisões só de acréscimo, lotes, quem decidiu e índices",
        migracao_2,
    ),
];

/// A escada do inventário, com a linha de base que adota o banco do Bun "2".
pub fn escada() -> Escada<'static> {
    Escada::nova(&MIGRACOES).com_linha_de_base(
        LinhaDeBase::nova(1, TABELAS_DA_BASE).com_marcador(MARCADOR_DO_BUN, VERSAO_DO_BUN_ADOTADA),
    )
}

pub(crate) fn tem_coluna(
    conexao: &Connection,
    tabela: &str,
    coluna: &str,
) -> rusqlite::Result<bool> {
    let mut consulta = conexao.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?;
    consulta.exists([tabela, coluna])
}

/// O terceiro trigger de `decisao` (achado A4 da revisão de segurança da fatia 1).
///
/// Os triggers de `UPDATE` e `DELETE` da bancada não pegam um `INSERT OR REPLACE` vindo de
/// uma conexão sem `recursive_triggers` (o padrão do SQLite: um script, o `sqlite3`): o
/// REPLACE apaga a linha antiga sem disparar o trigger de `DELETE`. Este recusa, antes de
/// tudo, inserir um `id` que já existe, com ou sem o PRAGMA. O `INSERT` sem `id` continua
/// valendo (no `BEFORE INSERT` o `NEW.id` de um rowid automático é -1, que não existe).
pub const GATILHO_QUE_RECUSA_SUBSTITUIR: &str = "\
CREATE TRIGGER IF NOT EXISTS \"decisao_recusa_substituir\" BEFORE INSERT ON \"decisao\"
WHEN EXISTS (SELECT 1 FROM decisao WHERE id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'a tabela decisao é somente de acréscimo: substituir recusado');
END;
";

fn migracao_2(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    for (tabela, coluna, tipo) in COLUNAS_DO_GARANTIR_COLUNA {
        if !tem_coluna(tx, tabela, coluna)? {
            tx.execute_batch(&format!("ALTER TABLE {tabela} ADD COLUMN {coluna} {tipo}"))?;
        }
    }
    tx.execute_batch(esquema::MIGRACAO_2_DECISOES_E_LOTES)?;
    let gatilhos = gerar_sql_somente_acrescimo("decisao").map_err(|erro| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(erro.to_string()),
        )
    })?;
    tx.execute_batch(&gatilhos)?;
    tx.execute_batch(GATILHO_QUE_RECUSA_SUBSTITUIR)
}

/// Confere, coluna a coluna, que um banco a adotar tem tudo o que a migração 1 cria.
/// Devolve `tabela.coluna` de cada coluna que falta (vazio quando bate).
///
/// # Errors
/// Falha do SQLite.
pub fn colunas_que_faltam_na_base(conexao: &Connection) -> rusqlite::Result<Vec<String>> {
    let referencia = Connection::open_in_memory()?;
    referencia.execute_batch(esquema::MIGRACAO_1_BASE_BUN)?;
    let mut faltando = Vec::new();
    for tabela in TABELAS_DA_BASE {
        let mut consulta = referencia.prepare("SELECT name FROM pragma_table_info(?1)")?;
        let colunas = consulta
            .query_map([tabela], |l| l.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for coluna in colunas {
            if !tem_coluna(conexao, tabela, &coluna)? {
                faltando.push(format!("{tabela}.{coluna}"));
            }
        }
    }
    Ok(faltando)
}
