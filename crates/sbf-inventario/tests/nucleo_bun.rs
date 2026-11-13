//! INV-06 de ponta a ponta: o `e3-triar.ts` do Bun, rodado depois de uma decisão sua
//! gravada pelo Rust, não muda o achado. Pula (com aviso) se o `bun` não está instalado.
//!
//! Segurança (INV-17): o script roda com o ambiente limpo, `FINANCAS_DIR` apontando para a
//! pasta temporária e o diretório de trabalho nela (sem `.env` por perto).
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use std::path::PathBuf;
use std::process::Command;

use nucleo_comum::*;
use sbf_inventario::{Acao, Comandos, PedidoDeDecisao};

fn bun() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let caminho = PathBuf::from(home).join(".bun/bin/bun");
    caminho.exists().then_some(caminho)
}

#[test]
fn inv06_e3_triar_do_bun_nao_muda_o_que_o_usuario_decidiu() {
    let Some(bun) = bun() else {
        eprintln!("bun não encontrado: INV-06 de ponta a ponta não verificado aqui");
        return;
    };
    let pasta = tempfile::tempdir().unwrap();
    let mut inv = inventario_migrado(pasta.path());
    assert_eq!(inv.caminho(), pasta.path().join("inventario.sqlite"));
    // Um segundo achado novo, sem decisão: prova que o script rodou neste banco.
    inv.conexao()
        .execute_batch(&format!(
            "INSERT INTO achado (content_hash, file_size_bytes, extensao, camada, estado)
               VALUES ('{h}', 500, 'pdf', 'A', 'novo');
             INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                                 local_filepath, filename)
               VALUES ('{h}', 'disco:teste', 1, 'local_file', 'local:/f/5.pdf:1', '/f/boleto-5.pdf',
                       'boleto-5.pdf');",
            h = hash(5)
        ))
        .unwrap();
    inv.executar(&PedidoDeDecisao {
        acao: Acao::CorrigirTipo("recibo".into()),
        alvos: vec![achado(1)],
        motivo: None,
    })
    .unwrap();
    let linha = |c: &rusqlite::Connection, n: u8| {
        valor(
            c,
            &format!(
                "SELECT estado || '|' || COALESCE(document_type, '') || '|' ||
                        COALESCE(classificado_por, '') || '|' || COALESCE(decidido_por, '')
                   FROM achado WHERE content_hash = '{}'",
                hash(n)
            ),
        )
    };
    let antes = linha(inv.conexao(), 1);
    assert_eq!(antes, "novo|recibo||usuario");
    drop(inv);

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../workers/inventario/src/e3-triar.ts")
        .canonicalize()
        .unwrap();
    let saida = Command::new(&bun)
        .arg("run")
        .arg(&script)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", pasta.path())
        .env("FINANCAS_DIR", pasta.path())
        .current_dir(pasta.path())
        .output()
        .unwrap();
    assert!(
        saida.status.success(),
        "e3-triar falhou: {}",
        String::from_utf8_lossy(&saida.stderr)
    );

    let c = rusqlite::Connection::open(pasta.path().join("inventario.sqlite")).unwrap();
    assert_eq!(linha(&c, 1), antes, "o Bun mudou a decisão do usuário");
    let outro = linha(&c, 5);
    assert!(
        !outro.starts_with("novo") && outro.ends_with("regras@2|regras@2"),
        "o script não triou o achado sem decisão: {outro}"
    );
}
