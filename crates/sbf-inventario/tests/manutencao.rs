//! `sbf reprojetar --inventario <pasta>` (INV-05, comando de manutenção): refaz a projeção a
//! partir do log, não cria banco e não migra. Banco fictício, gerado aqui.

#[path = "nucleo_comum/mod.rs"]
mod nucleo_comum;

use nucleo_comum::*;
use rusqlite::Connection;
use sbf_inventario::manutencao::{ErroDeManutencao, reprojetar_a_pasta};
use sbf_inventario::{Acao, Comandos, PedidoDeDecisao};

#[test]
fn sem_banco_falha_sem_criar_arquivo() {
    let pasta = tempfile::tempdir().unwrap();
    let erro = reprojetar_a_pasta(pasta.path()).unwrap_err();
    assert!(matches!(erro, ErroDeManutencao::SemBanco(_)), "{erro}");
    assert_eq!(std::fs::read_dir(pasta.path()).unwrap().count(), 0);
}

#[test]
fn banco_nao_migrado_e_recusado_sem_mudar_nada() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    let erro = reprojetar_a_pasta(pasta.path()).unwrap_err();
    assert!(matches!(erro, ErroDeManutencao::NaoMigrado), "{erro}");
    let versao: u32 = Connection::open(&caminho)
        .unwrap()
        .query_row("PRAGMA user_version", [], |l| l.get(0))
        .unwrap();
    assert_eq!(versao, 0, "a manutenção não migra");
}

#[test]
fn reprojeta_a_partir_do_log_e_resume() {
    let pasta = tempfile::tempdir().unwrap();
    let mut inventario = inventario_migrado(pasta.path());
    inventario
        .executar(&PedidoDeDecisao {
            acao: Acao::Descartar,
            alvos: vec![achado(1)],
            motivo: None,
        })
        .unwrap();
    let estado = |c: &Connection| -> String {
        c.query_row(
            "SELECT estado FROM achado WHERE content_hash = ?1",
            [hash(1)],
            |l| l.get(0),
        )
        .unwrap()
    };
    assert_eq!(estado(inventario.conexao()), "descartado");
    // Alguém mexeu na projeção por fora (um script antigo, uma edição à mão).
    inventario
        .conexao()
        .execute(
            "UPDATE achado SET estado = 'novo' WHERE content_hash = ?1",
            [hash(1)],
        )
        .unwrap();
    drop(inventario);

    let feito = reprojetar_a_pasta(pasta.path()).unwrap();
    assert_eq!(feito.reprojecao.decisoes, 1);
    assert_eq!(feito.reprojecao.alvos, 1);
    let resumo = feito.resumo();
    assert!(resumo.contains("1 decisão(ões) reaplicada(s)"), "{resumo}");
    assert!(resumo.contains("inventario.sqlite"), "{resumo}");
    let c = Connection::open(pasta.path().join("inventario.sqlite")).unwrap();
    assert_eq!(estado(&c), "descartado");
}
