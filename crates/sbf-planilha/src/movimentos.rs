//! O extrato da Pluggy como o planejamento o enxerga: as linhas de `registro` do inventário
//! com `external_ref` (`pluggy:txn_…`), entre duas datas. Só leitura.
//!
//! Nenhum valor, descrição ou conta vai a log; o que falha vira "sem movimentos" (a fonte
//! do trait não tem erro) ou [`ErroPlanilha::Inventario`] nas funções com `Resultado`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use sbf_planejamento::{Centavos, Data, FonteDeMovimentos, MovimentoDoExtrato};

use crate::{ErroPlanilha, Resultado};

fn erro(e: rusqlite::Error) -> ErroPlanilha {
    ErroPlanilha::Inventario(e.to_string())
}

/// Um movimento do inventário e o fornecedor normalizado dele (a planilha ainda o usa).
#[derive(Debug, Clone)]
pub(crate) struct Registro {
    pub movimento: MovimentoDoExtrato,
    pub fornecedor: Option<String>,
}

fn colunas_de_registro(conn: &Connection) -> Resultado<HashSet<String>> {
    let mut st = conn.prepare("PRAGMA table_info(registro)").map_err(erro)?;
    let nomes = st
        .query_map([], |l| l.get::<_, String>(1))
        .map_err(erro)?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(erro)?;
    Ok(nomes)
}

/// Lê `registro` entre as datas (inclusive). Inventários antigos sem alguma coluna
/// opcional (`conta_provavel`, `descricao_original`…) devolvem o campo vazio.
pub(crate) fn registros_entre(conn: &Connection, de: Data, ate: Data) -> Resultado<Vec<Registro>> {
    let colunas = colunas_de_registro(conn)?;
    let opcional = |nome: &str| {
        if colunas.contains(nome) {
            nome.to_owned()
        } else {
            "NULL".to_owned()
        }
    };
    let sql = format!(
        "SELECT external_ref, tipo, data, valor_centavos, {conta}, {descricao}, {bruto}, {fornecedor}
         FROM registro
         WHERE external_ref IS NOT NULL AND valor_centavos <> 0 AND data >= ?1 AND data <= ?2
         ORDER BY data, external_ref",
        conta = opcional("conta_provavel"),
        descricao = opcional("descricao_original"),
        bruto = opcional("supplier_name_raw"),
        fornecedor = opcional("fornecedor_normalizado"),
    );
    let mut st = conn.prepare(&sql).map_err(erro)?;
    let linhas = st
        .query_map(rusqlite::params![de.to_string(), ate.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(erro)?;
    let mut saida = Vec::new();
    for l in linhas {
        let (movimento, tipo, data, valor, conta, descricao, bruto, fornecedor) =
            l.map_err(erro)?;
        let Some(data) = Data::de_iso(&data) else {
            continue;
        };
        saida.push(Registro {
            movimento: MovimentoDoExtrato {
                movimento,
                data,
                valor: Centavos(valor),
                conta_do_extrato: conta.unwrap_or_default(),
                descricao: descricao
                    .or(bruto)
                    .or_else(|| fornecedor.clone())
                    .unwrap_or_default(),
                tipo,
            },
            fornecedor,
        });
    }
    Ok(saida)
}

/// Os movimentos do extrato entre duas datas, de uma conexão já aberta.
///
/// # Errors
/// Falha do SQLite.
pub fn movimentos_entre(
    conn: &Connection,
    de: Data,
    ate: Data,
) -> Resultado<Vec<MovimentoDoExtrato>> {
    Ok(registros_entre(conn, de, ate)?
        .into_iter()
        .map(|r| r.movimento)
        .collect())
}

/// O extrato da Pluggy no `inventario.sqlite` do cofre. Abre uma conexão só de leitura a cada
/// consulta, então não segura o arquivo nem exige `Send` da conexão.
#[derive(Debug, Clone)]
pub struct MovimentosDaPluggy {
    inventario: PathBuf,
}

impl MovimentosDaPluggy {
    /// Do arquivo do inventário.
    #[must_use]
    pub fn novo(inventario: impl Into<PathBuf>) -> MovimentosDaPluggy {
        MovimentosDaPluggy {
            inventario: inventario.into(),
        }
    }

    /// Do cofre (`<cofre>/inventario.sqlite`).
    #[must_use]
    pub fn do_cofre(cofre: &Path) -> MovimentosDaPluggy {
        MovimentosDaPluggy::novo(cofre.join("inventario.sqlite"))
    }

    /// Lê os movimentos, devolvendo o erro (a fonte do trait o engole).
    ///
    /// # Errors
    /// O inventário não existe ou não abre, ou falha do SQLite.
    pub fn ler(&self, de: Data, ate: Data) -> Resultado<Vec<MovimentoDoExtrato>> {
        let conn = Connection::open_with_flags(
            &self.inventario,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(erro)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(erro)?;
        movimentos_entre(&conn, de, ate)
    }
}

impl FonteDeMovimentos for MovimentosDaPluggy {
    fn movimentos(&self, de: Data, ate: Data) -> Vec<MovimentoDoExtrato> {
        self.ler(de, ate).unwrap_or_default()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fixtura::{inventario_ficticio, registro_completo};

    fn d(dia: u8) -> Data {
        Data::nova(2026, 11, dia).expect("data")
    }

    #[test]
    fn le_o_extrato_do_inventario_entre_as_datas() {
        let pasta = tempfile::tempdir().expect("pasta");
        let caminho = pasta.path().join("inventario.sqlite");
        let c = Connection::open(&caminho).expect("banco");
        c.execute_batch(
            "CREATE TABLE registro (
               id INTEGER PRIMARY KEY, external_ref TEXT UNIQUE, tipo TEXT NOT NULL,
               data TEXT NOT NULL, ciclo_id TEXT, valor_centavos INTEGER NOT NULL,
               descricao_original TEXT, supplier_name_raw TEXT, fornecedor_normalizado TEXT,
               conta_provavel TEXT);",
        )
        .expect("esquema");
        registro_completo(
            &c,
            "pluggy:txn_s",
            "credito",
            "2026-11-06",
            480_000,
            Some("BANCO FICTICIO · 0001"),
            "SALARIO EMPRESA",
        );
        registro_completo(
            &c,
            "pluggy:txn_c",
            "compra",
            "2026-11-10",
            -3_990,
            Some("BANCO FICTICIO · 0001"),
            "STREAMING",
        );
        registro_completo(
            &c,
            "pluggy:txn_fora",
            "compra",
            "2026-10-01",
            -100,
            None,
            "X",
        );
        // Sem external_ref (documento) e valor zero não são extrato.
        c.execute(
            "INSERT INTO registro (external_ref, tipo, data, valor_centavos)
             VALUES (NULL, 'compra', '2026-11-07', -500), ('pluggy:txn_zero', 'compra', '2026-11-07', 0)",
            [],
        )
        .expect("insert");
        drop(c);

        let fonte = MovimentosDaPluggy::novo(&caminho);
        let m = fonte.movimentos(d(1), d(30));
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].movimento, "pluggy:txn_s");
        assert_eq!(m[0].data, d(6));
        assert_eq!(m[0].valor, Centavos(480_000));
        assert_eq!(m[0].conta_do_extrato, "BANCO FICTICIO · 0001");
        assert_eq!(m[0].descricao, "SALARIO EMPRESA");
        assert_eq!(m[0].tipo, "credito");
        assert_eq!(m[1].valor, Centavos(-3_990));
        assert_eq!(fonte.movimentos(d(8), d(30)).len(), 1);
        assert_eq!(
            MovimentosDaPluggy::do_cofre(pasta.path())
                .movimentos(d(1), d(30))
                .len(),
            2
        );
    }

    #[test]
    fn inventario_antigo_sem_colunas_opcionais_ainda_le() {
        let c = inventario_ficticio();
        c.execute_batch("ALTER TABLE registro DROP COLUMN descricao_original; ALTER TABLE registro DROP COLUMN supplier_name_raw;")
            .expect("alter");
        crate::fixtura::registro(
            &c,
            "pluggy:txn_1",
            "debito",
            "2026-11-05",
            -100,
            "FORNECEDOR",
        );
        let m = movimentos_entre(&c, d(1), d(30)).expect("ler");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].descricao, "FORNECEDOR");
        assert_eq!(m[0].conta_do_extrato, "");
    }

    #[test]
    fn sem_inventario_ou_com_erro_a_fonte_devolve_vazio() {
        let pasta = tempfile::tempdir().expect("pasta");
        let fonte = MovimentosDaPluggy::do_cofre(pasta.path());
        assert!(fonte.movimentos(d(1), d(30)).is_empty());
        assert!(fonte.ler(d(1), d(30)).is_err());
    }
}
