//! Mede a migração sobre um inventário do Bun **sintético** e grande (dados fictícios,
//! gerados aqui). Ignorado por padrão; rode com
//!
//! `SBF_MEDIR_ACHADOS=300000 cargo test --release -p sbf-inventario --test nucleo_medir_migracao -- --ignored --nocapture`
//!
//! Imprime o tempo de cada etapa de `migrar_com_andamento` e, numa segunda cópia, o de
//! cada comando da migração 2, para decidir onde vale otimizar.
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use nucleo_comum::aplicar_schema_do_bun;
use rusqlite::{Connection, params};
use sbf_inventario::banco::Inventario;

fn sintetico(caminho: &Path, achados: usize) {
    let c = Connection::open(caminho).unwrap();
    c.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
        .unwrap();
    c.execute_batch("PRAGMA synchronous = OFF").unwrap();
    aplicar_schema_do_bun(&c);
    c.execute_batch(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:x', 'disco', 'fictício');
         INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
           VALUES (1, 'disco:x', 'E1', '/x', 'teste@1');
         INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-10', '2026-10-20', '2026-11-19');",
    )
    .unwrap();
    let texto = "texto fictício de documento extraído ".repeat(40);
    c.execute_batch("BEGIN").unwrap();
    {
        let mut achado = c
            .prepare(
                "INSERT INTO achado (content_hash, file_size_bytes, extensao, camada, estado,
                                     document_type, classificado_por, notas)
                 VALUES (?1, 1000, 'pdf', 'A', ?2, 'boleto', 'regras@2', 'E3 regras@2: nota')",
            )
            .unwrap();
        let mut origem = c
            .prepare(
                "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                                     local_filepath, filename, gmail_message_id)
                 VALUES (?1, 'disco:x', 1, 'local_file', ?2, ?3, 'arquivo.pdf', ?4)",
            )
            .unwrap();
        let mut texto_st = c
            .prepare(
                "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
            )
            .unwrap();
        let mut registro = c
            .prepare(
                "INSERT INTO registro (content_hash, fonte_id, tipo, data, data_tipo, ciclo_id,
                                       valor_centavos, extraido_por)
                 VALUES (?1, 'disco:x', 'pagamento', '2026-10-25', 'vencimento', '2026-10', -100,
                         'extrair@1')",
            )
            .unwrap();
        for i in 0..achados {
            let h = format!("{i:064x}");
            let estado = ["novo", "documento", "descartado", "duvida"][i % 4];
            achado.execute(params![h, estado]).unwrap();
            origem
                .execute(params![
                    h,
                    format!("local:/x/{i}.pdf:1"),
                    format!("/x/pasta-{}/{i}.pdf", i % 1000),
                    (i % 2 == 0).then(|| format!("msg{i}"))
                ])
                .unwrap();
            if i % 3 == 0 {
                texto_st.execute(params![h, texto]).unwrap();
            }
            if i % 10 == 0 {
                registro.execute(params![h]).unwrap();
            }
        }
    }
    c.execute_batch("COMMIT").unwrap();
    c.close().unwrap();
}

fn tamanho_mb(caminho: &Path) -> f64 {
    std::fs::metadata(caminho).unwrap().len() as f64 / 1e6
}

#[test]
#[ignore = "medição; rode com --ignored --nocapture"]
fn medir_migracao_de_um_inventario_grande() {
    let achados: usize = std::env::var("SBF_MEDIR_ACHADOS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300_000);
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("inventario.sqlite");
    let t = Instant::now();
    sintetico(&caminho, achados);
    println!(
        "fixture: {achados} achados, {:.0} MB, gerada em {:.1} s",
        tamanho_mb(&caminho),
        t.elapsed().as_secs_f64()
    );
    std::fs::copy(&caminho, pasta.path().join("segunda.sqlite")).unwrap();

    let marcas: RefCell<Vec<(String, Instant)>> = RefCell::new(Vec::new());
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    let inicio = Instant::now();
    inventario
        .migrar_com_andamento(pasta.path().join("copia.sqlite"), &|etapa| {
            marcas
                .borrow_mut()
                .push((format!("{etapa:?}"), Instant::now()));
        })
        .unwrap();
    let fim = Instant::now();
    let marcas = marcas.into_inner();
    for (i, (nome, comeco)) in marcas.iter().enumerate() {
        let termino = marcas.get(i + 1).map_or(fim, |m| m.1);
        let nome: String = nome.chars().take(40).collect();
        println!("  {nome:<40} {:>7.2} s", (termino - *comeco).as_secs_f64());
    }
    println!(
        "migrar_com_andamento: {:.2} s",
        (fim - inicio).as_secs_f64()
    );

    // Os comandos da migração 2, um a um, numa cópia (mesma transação, como na escada).
    let c = Connection::open(pasta.path().join("segunda.sqlite")).unwrap();
    c.execute_batch("PRAGMA synchronous = NORMAL; BEGIN IMMEDIATE")
        .unwrap();
    for (nome, sql) in [
        (
            "ALTER achado/registro ADD decidido_por",
            "ALTER TABLE achado ADD COLUMN decidido_por TEXT; ALTER TABLE registro ADD COLUMN decidido_por TEXT",
        ),
        (
            "UPDATE achado SET decidido_por",
            "UPDATE achado SET decidido_por = classificado_por",
        ),
        (
            "UPDATE registro SET decidido_por",
            "UPDATE registro SET decidido_por = extraido_por",
        ),
        (
            "CREATE INDEX origem(gmail_message_id)",
            "CREATE INDEX IF NOT EXISTS origem_gmail_message ON origem(gmail_message_id)",
        ),
        (
            "CREATE INDEX origem(local_filepath)",
            "CREATE INDEX IF NOT EXISTS origem_local_filepath ON origem(local_filepath)",
        ),
        ("COMMIT", "COMMIT"),
    ] {
        let t = Instant::now();
        c.execute_batch(sql).unwrap();
        println!("  {nome:<40} {:>7.2} s", t.elapsed().as_secs_f64());
    }
}
