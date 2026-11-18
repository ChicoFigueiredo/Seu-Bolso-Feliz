//! A página do registro da Pluggy: o que o Detalhe do movimento mostra (data, valor, conta,
//! descrição original, tipos, contraparte com o documento mascarado, id externo) e a busca
//! do registro pelo `external_ref`.

mod comum;

use comum::{Reg, banco_de_teste, inserir_registro};
use rusqlite::Connection;
use sbf_inventario::consulta::{Celula, Consulta};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::relacoes::registro_pelo_id_externo;
use sbf_inventario::{Ref, Tipo};

fn campo(leitor: &LeitorSqlite, id: i64, rotulo: &str) -> Celula {
    let alvo = Ref::nova(Tipo::Registro, id.to_string());
    leitor
        .item(&alvo)
        .expect("item")
        .expect("existe")
        .campos
        .into_iter()
        .find(|c| c.rotulo == rotulo)
        .unwrap_or_else(|| panic!("sem o campo {rotulo}"))
        .valor
}

fn um_pix() -> (comum::BancoDeTeste, i64) {
    let banco = banco_de_teste();
    let conn = Connection::open(&banco.caminho).expect("conexão");
    let id = inserir_registro(
        &conn,
        &Reg {
            external_ref: Some("pluggy:txn_pix_1"),
            tipo: "debito",
            data: "2026-11-05",
            valor: -15_000,
            descricao: "DEB PIX CHAVE",
            fornecedor: Some("Fulana de Tal"),
            categoria: Some("Transferências"),
            conta: Some("CAIXA · 0001-1"),
            contraparte: Some("Fulana de Tal"),
            ..Reg::default()
        },
    );
    conn.execute(
        "UPDATE registro SET tipo_original = 'PIX_ENVIADO', contraparte_doc = '123.456.789-09'
         WHERE id = ?",
        [id],
    )
    .expect("update");
    (banco, id)
}

#[test]
fn a_pagina_do_registro_traz_o_detalhe_do_movimento() {
    let (banco, id) = um_pix();
    let leitor = LeitorSqlite::abrir(&banco.caminho).expect("abrir");
    assert_eq!(
        campo(&leitor, id, "Descrição original"),
        Celula::Texto("DEB PIX CHAVE".into())
    );
    assert_eq!(
        campo(&leitor, id, "Tipo original"),
        Celula::Texto("PIX_ENVIADO".into())
    );
    assert_eq!(
        campo(&leitor, id, "Id externo"),
        Celula::Texto("pluggy:txn_pix_1".into())
    );
    assert_eq!(
        campo(&leitor, id, "Contraparte"),
        Celula::Texto("Fulana de Tal".into())
    );
    assert!(matches!(
        campo(&leitor, id, "Categoria provável"),
        Celula::Texto(t) if t == "Transferências"
    ));
}

#[test]
fn o_documento_da_contraparte_vai_mascarado_na_pagina() {
    let (banco, id) = um_pix();
    let leitor = LeitorSqlite::abrir(&banco.caminho).expect("abrir");
    let Celula::Texto(doc) = campo(&leitor, id, "Documento da contraparte") else {
        panic!("o documento deveria ser texto");
    };
    assert_eq!(doc, "123******09");
    assert!(!doc.contains("456"));
}

#[test]
fn acha_o_registro_pelo_id_externo() {
    let (banco, id) = um_pix();
    let conn = Connection::open(&banco.caminho).expect("conexão");
    assert_eq!(
        registro_pelo_id_externo(&conn, "pluggy:txn_pix_1").expect("busca"),
        Some(Ref::nova(Tipo::Registro, id.to_string()))
    );
    assert_eq!(
        registro_pelo_id_externo(&conn, "pluggy:nao_existe").expect("busca"),
        None
    );
}
