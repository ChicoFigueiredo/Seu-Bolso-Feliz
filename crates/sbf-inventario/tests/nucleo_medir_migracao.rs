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

/// Um texto fictício de ~2,6 KB, diferente a cada `i` (palavras sorteadas por um LCG).
fn texto_ficticio(i: u64) -> String {
    const PALAVRAS: [&str; 16] = [
        "fatura",
        "energia",
        "vencimento",
        "referencia",
        "total",
        "pagamento",
        "recibo",
        "assinatura",
        "cliente",
        "documento",
        "valor",
        "emissao",
        "codigo",
        "barras",
        "banco",
        "conta",
    ];
    let mut estado = i.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let mut saida = String::with_capacity(2_700);
    while saida.len() < 2_600 {
        estado = estado
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        saida.push_str(PALAVRAS[(estado >> 60) as usize]);
        saida.push(if estado & 0x100 == 0 { ' ' } else { '\n' });
        if estado & 0x7000 == 0 {
            saida.push_str(&format!("{} ", estado >> 40));
        }
    }
    saida
}

/// A migração 5 sobre um cofre na v4 com ~390 mil textos (~1 GB), como o real.
///
/// `SBF_MEDIR_TEXTOS=390000 SBF_MEDIR_PASTA=/algum/lugar/novo cargo test -p sbf-inventario
/// --test nucleo_medir_migracao medir_migracao_5 -- --ignored --nocapture`
#[test]
#[ignore = "medição; rode com --ignored --nocapture"]
fn medir_migracao_5_com_muitos_textos() {
    use bancada_armazem::Armazem;
    use sbf_inventario::{banco::pragmas, migracoes};

    let textos: u64 = std::env::var("SBF_MEDIR_TEXTOS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(390_000);
    let base = std::env::var_os("SBF_MEDIR_PASTA").map(std::path::PathBuf::from);
    let pasta = match &base {
        Some(b) => tempfile::tempdir_in(b).unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    let caminho = pasta.path().join("inventario.sqlite");
    let t = Instant::now();
    {
        let mut armazem = Armazem::abrir(&caminho, &pragmas()).unwrap();
        armazem.migrar_ate(&migracoes::escada(), 4).unwrap();
    }
    let c = Connection::open(&caminho).unwrap();
    c.execute_batch("PRAGMA synchronous = OFF; BEGIN").unwrap();
    {
        let mut achado = c
            .prepare(
                "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 1000, 'A')",
            )
            .unwrap();
        let mut texto_st = c
            .prepare(
                "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
            )
            .unwrap();
        for i in 0..textos {
            let h = format!("{i:064x}");
            achado.execute(params![h]).unwrap();
            // Um em cada 20 é curto (foto com OCR vazio), um em cada 10 repete o texto anterior.
            let conteudo = if i % 20 == 0 {
                "ocr vazio".to_owned()
            } else if i % 10 == 1 {
                texto_ficticio(i - 1).replace(' ', "  ")
            } else {
                texto_ficticio(i)
            };
            texto_st.execute(params![h, conteudo]).unwrap();
        }
    }
    c.execute_batch("COMMIT").unwrap();
    c.close().unwrap();
    println!(
        "fixture v4: {textos} textos, {:.0} MB, gerada em {:.1} s",
        tamanho_mb(&caminho),
        t.elapsed().as_secs_f64()
    );

    let mut inventario = Inventario::abrir(&caminho).unwrap();
    let inicio = Instant::now();
    let marcas: RefCell<Vec<(String, Instant)>> = RefCell::new(Vec::new());
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
        "migrar_com_andamento v4 -> v5: {:.2} s",
        (fim - inicio).as_secs_f64()
    );
    let n = |sql: &str| -> i64 {
        inventario
            .conexao()
            .query_row(sql, [], |l| l.get(0))
            .unwrap()
    };
    println!(
        "impressoes: {} linhas, {} com impressao, {} impressoes distintas",
        n("SELECT COUNT(*) FROM impressao_do_texto"),
        n("SELECT COUNT(*) FROM impressao_do_texto WHERE impressao IS NOT NULL"),
        n("SELECT COUNT(DISTINCT impressao) FROM impressao_do_texto"),
    );
    let t = Instant::now();
    let mut mais_de_um = 0;
    for i in [1u64, 11, 21, 389_991] {
        if i < textos {
            let h = format!("{i:064x}");
            mais_de_um += usize::from(
                sbf_inventario::impressao_do_texto::original_do_mesmo_texto(
                    inventario.conexao(),
                    &h,
                )
                .unwrap()
                .is_some(),
            );
        }
    }
    println!(
        "4 buscas de original: {:.1} ms ({mais_de_um} acharam)",
        t.elapsed().as_secs_f64() * 1000.0
    );
    println!("banco depois: {:.0} MB", tamanho_mb(&caminho));
}
