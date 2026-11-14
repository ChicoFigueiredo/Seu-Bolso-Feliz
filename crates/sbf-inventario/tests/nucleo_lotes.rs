//! Lotes (spec 06, "Lotes"; INV-10, parte de domínio): montar, aprovar marca os itens,
//! lote aprovado só muda desaprovando, Enviar desabilitado com o motivo.
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use nucleo_comum::*;
use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_inventario::{Comandos, ErroDeComando};

fn novo() -> (tempfile::TempDir, Inventario) {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    (pasta, inventario)
}

fn estado(c: &Connection, id: i64) -> String {
    valor(c, &format!("SELECT estado FROM registro WHERE id = {id}"))
}

fn itens(c: &Connection, lote: &str) -> String {
    let mut consulta = c
        .prepare("SELECT alvo_tipo || ':' || alvo_id FROM lote_item WHERE lote_id = ?1 ORDER BY 1")
        .unwrap();
    consulta
        .query_map([lote], |l| l.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join(",")
}

#[test]
fn inv10_montar_lote_com_documentos_e_registros() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Outubro fictício").unwrap();
    assert_eq!(lote.chave, "1");
    inv.adicionar_ao_lote(&lote, &[achado(2), registro(1), registro(2)])
        .unwrap();
    // Repetir não duplica nem grava decisão.
    let n = valor(inv.conexao(), "SELECT COUNT(*) FROM decisao");
    inv.adicionar_ao_lote(&lote, &[registro(1)]).unwrap();
    assert_eq!(valor(inv.conexao(), "SELECT COUNT(*) FROM decisao"), n);
    // Achado que não é Documento não entra.
    let erro = inv.adicionar_ao_lote(&lote, &[achado(1)]).unwrap_err();
    assert!(matches!(erro, ErroDeComando::PromocaoNaoSeDesfaz), "{erro}");
    inv.remover_do_lote(&lote, &[registro(2)]).unwrap();
    let c = inv.conexao();
    assert_eq!(itens(c, "1"), format!("achado:{},registro:1", hash(2)));
    assert_eq!(
        valor(c, "SELECT nome FROM lote WHERE id = 1"),
        "Outubro fictício"
    );
    assert_eq!(
        valor(c, "SELECT COUNT(*) FROM decisao WHERE alvo_tipo = 'lote'"),
        "5"
    );
}

#[test]
fn inv10_aprovar_lote_marca_os_registros_como_decisoes() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Lote A").unwrap();
    inv.adicionar_ao_lote(&lote, &[achado(4), registro(1), registro(3)])
        .unwrap();
    inv.aprovar_lote(&lote).unwrap();
    let c = inv.conexao();
    assert_ne!(valor(c, "SELECT aprovado_em FROM lote WHERE id = 1"), "∅");
    assert_eq!(estado(c, 1), "aprovado");
    assert_eq!(estado(c, 3), "aprovado");
    assert_eq!(estado(c, 2), "conferido", "fora do lote não muda");
    assert_eq!(
        valor(
            c,
            "SELECT COUNT(*) FROM decisao WHERE alvo_tipo = 'registro' AND campo = 'estado'
               AND motivo = 'aprovação do lote 1'"
        ),
        "2"
    );
    assert_eq!(
        valor(c, "SELECT decidido_por FROM registro WHERE id = 3"),
        "usuario"
    );
}

#[test]
fn inv10_lote_aprovado_so_muda_desaprovando() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Lote B").unwrap();
    inv.adicionar_ao_lote(&lote, &[registro(1)]).unwrap();
    inv.aprovar_lote(&lote).unwrap();
    assert!(matches!(
        inv.adicionar_ao_lote(&lote, &[registro(2)]),
        Err(ErroDeComando::LoteAprovado(_))
    ));
    assert!(matches!(
        inv.remover_do_lote(&lote, &[registro(1)]),
        Err(ErroDeComando::LoteAprovado(_))
    ));
    assert!(matches!(
        inv.aprovar_lote(&lote),
        Err(ErroDeComando::LoteAprovado(_))
    ));
    inv.desaprovar_lote(&lote).unwrap();
    let c = inv.conexao();
    assert_eq!(valor(c, "SELECT aprovado_em FROM lote WHERE id = 1"), "∅");
    assert_eq!(estado(c, 1), "extraido", "a aprovação do lote foi desfeita");
    assert_eq!(
        valor(c, "SELECT decidido_por FROM registro WHERE id = 1"),
        "extrair@1"
    );
    inv.adicionar_ao_lote(&lote, &[registro(2)]).unwrap();
    assert_eq!(itens(inv.conexao(), "1"), "registro:1,registro:2");
}

#[test]
fn inv10_desaprovar_nao_desfaz_aprovacao_individual() {
    let (_p, mut inv) = novo();
    inv.executar(&sbf_inventario::PedidoDeDecisao {
        acao: sbf_inventario::Acao::Aprovar,
        alvos: vec![registro(1)],
        motivo: None,
    })
    .unwrap();
    let lote = inv.criar_lote("Lote C").unwrap();
    inv.adicionar_ao_lote(&lote, &[registro(1), registro(3)])
        .unwrap();
    inv.aprovar_lote(&lote).unwrap();
    inv.desaprovar_lote(&lote).unwrap();
    let c = inv.conexao();
    assert_eq!(estado(c, 1), "aprovado", "aprovado antes, por você");
    assert_eq!(estado(c, 3), "extraido");
}

#[test]
fn inv10_desfazer_ultima_depois_de_aprovar_desaprova() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Lote D").unwrap();
    inv.adicionar_ao_lote(&lote, &[registro(3)]).unwrap();
    inv.aprovar_lote(&lote).unwrap();
    inv.desfazer_ultima().unwrap().unwrap();
    let c = inv.conexao();
    assert_eq!(valor(c, "SELECT aprovado_em FROM lote WHERE id = 1"), "∅");
    assert_eq!(estado(c, 3), "extraido");
    // A seguir desfaz a adição e depois a criação.
    inv.desfazer_ultima().unwrap().unwrap();
    assert_eq!(itens(inv.conexao(), "1"), "");
    inv.desfazer_ultima().unwrap().unwrap();
    assert_eq!(valor(inv.conexao(), "SELECT COUNT(*) FROM lote"), "0");
    // Um lote novo não reaproveita o id do desfeito.
    assert_eq!(inv.criar_lote("Lote E").unwrap().chave, "2");
}

#[test]
fn inv10_enviar_ao_servidor_fica_desabilitado_com_o_motivo() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Lote F").unwrap();
    inv.aprovar_lote(&lote).unwrap();
    let motivo = inv.enviar_ao_servidor(&lote).unwrap_err();
    assert!(motivo.contains("fatia 3"), "{motivo}");
    assert_eq!(
        valor(inv.conexao(), "SELECT promovido_em FROM lote WHERE id = 1"),
        "∅"
    );
}

#[test]
fn inv10_lote_inexistente() {
    let (_p, mut inv) = novo();
    let lote = sbf_inventario::Ref::nova(sbf_inventario::Tipo::Lote, "9");
    assert!(matches!(
        inv.aprovar_lote(&lote),
        Err(ErroDeComando::NaoEncontrado(_))
    ));
    assert!(matches!(
        inv.adicionar_ao_lote(&lote, &[registro(1)]),
        Err(ErroDeComando::NaoEncontrado(_))
    ));
}

#[test]
fn promocao_nao_se_desfaz_e_desfazer_ultima_recusa_sem_desfazer_nada() {
    let (_p, mut inv) = novo();
    let lote = inv.criar_lote("Lote P").unwrap();
    inv.adicionar_ao_lote(&lote, &[registro(1)]).unwrap();
    inv.aprovar_lote(&lote).unwrap();
    assert_eq!(estado(inv.conexao(), 1), "aprovado");
    let ids = inv.promover(&[registro(1)], "o servidor aceitou").unwrap();
    assert_eq!(estado(inv.conexao(), 1), "promovido");

    // Pedir para desfazer a promoção, diretamente, é recusado com uma mensagem clara.
    let erro = inv.desfazer(ids[0]).unwrap_err();
    assert!(matches!(erro, ErroDeComando::PromocaoNaoSeDesfaz), "{erro}");
    assert!(erro.to_string().contains("promoção"), "{erro}");
    assert_eq!(estado(inv.conexao(), 1), "promovido");

    // "Desfazer decisão" recusa quando a última é a promoção e não desfaz a anterior.
    let erro = inv.desfazer_ultima().unwrap_err();
    assert_eq!(
        erro.to_string(),
        "A promoção ao servidor não se desfaz; para corrigir, edite no livro-caixa."
    );
    assert_eq!(estado(inv.conexao(), 1), "promovido");
    assert_eq!(
        valor(
            inv.conexao(),
            "SELECT COUNT(*) FROM decisao WHERE desfaz IS NOT NULL"
        ),
        "0"
    );
    assert_eq!(
        valor(
            inv.conexao(),
            "SELECT COUNT(*) FROM decisao WHERE desfaz IS NOT NULL AND alvo_tipo = 'registro' AND alvo_id = '1' AND depois LIKE '%promovido%'"
        ),
        "0"
    );
}
