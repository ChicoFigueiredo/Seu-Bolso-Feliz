//! Vários destinos ao mesmo tempo: idênticos entre si, falha isolada, rename nos dois.
//! Só pastas locais temporárias; nunca um remoto real.

#![cfg(unix)]

mod comum;

use comum::{Doc, arvore, inserir, nomes, novo_banco};
use sbf_estante::{Destino, ListaDeDestinos, Rclone, TipoDeDestino, atualizar_destinos};
use std::fs;

fn doc<'a>(hash: &'a str, fornecedor: &'a str, conteudo: &'a [u8]) -> Doc<'a> {
    Doc {
        hash,
        estado: "documento",
        tipo: Some("fatura"),
        data: Some("2026-10-25"),
        ciclo: Some("2026-10"),
        fornecedor: Some(fornecedor),
        valor: Some(-1000),
        ext: "pdf",
        conteudo: Some(conteudo),
        nome_original: "f.pdf",
    }
}

fn lista(a: &std::path::Path, b: &std::path::Path) -> ListaDeDestinos {
    let mut l = ListaDeDestinos::nova();
    l.adicionar(Destino::pasta_local("a", a).expect("a"))
        .expect("a");
    l.adicionar(Destino::pasta_local("b", b).expect("b"))
        .expect("b");
    l
}

#[test]
fn dois_destinos_locais_ficam_identicos_e_refletem_rename() {
    let cofre = tempfile::tempdir().expect("cofre");
    let a = tempfile::tempdir().expect("a");
    let b = tempfile::tempdir().expect("b");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(&c, cofre.path(), &doc("aaaaaaaa11111111", "Luz", b"um"));
    inserir(&c, cofre.path(), &doc("bbbbbbbb22222222", "Agua", b"dois"));
    let l = lista(a.path(), b.path());
    let rclone = Rclone::default();

    let rel = atualizar_destinos(&rclone, &l, &c, cofre.path(), &|_, _| {});
    assert_eq!(rel.len(), 2);
    for r in &rel {
        assert_eq!(r.resultado.as_ref().expect("ok").enviados, 2, "{r:?}");
    }
    assert_eq!(arvore(a.path()), arvore(b.path()));
    assert_eq!(nomes(a.path()).len(), 2);

    // Alheio nunca é tocado, nos dois.
    fs::write(a.path().join("nota.txt"), "x").expect("nota");
    fs::write(b.path().join("nota.txt"), "x").expect("nota");

    // Correção de fornecedor: rename nos dois, sem reenvio.
    c.execute(
        "UPDATE achado SET supplier_name_raw = 'Cemig' WHERE content_hash = 'aaaaaaaa11111111'",
        [],
    )
    .expect("corrigir");
    let rel = atualizar_destinos(&rclone, &l, &c, cofre.path(), &|_, _| {});
    for r in &rel {
        let r = r.resultado.as_ref().expect("ok");
        assert_eq!((r.enviados, r.renomeados, r.apagados), (0, 1, 0), "{r:?}");
    }
    assert_eq!(arvore(a.path()), arvore(b.path()));
    assert!(nomes(a.path()).iter().any(|n| n.contains("cemig")));
    assert!(a.path().join("nota.txt").is_file());
}

#[test]
fn destino_invalido_nao_para_o_outro() {
    let cofre = tempfile::tempdir().expect("cofre");
    let b = tempfile::tempdir().expect("b");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(&c, cofre.path(), &doc("aaaaaaaa11111111", "Luz", b"um"));
    let mut l = ListaDeDestinos::nova();
    // Alvo que o rclone leria como opção: só entra na lista por arquivo adulterado.
    let json = r#"{"destinos":[{"nome":"ruim","tipo":"drive","alvo":"--x:y","ativo":true}]}"#;
    let pasta = tempfile::tempdir().expect("prefs");
    let arq = pasta.path().join("d.json");
    fs::write(&arq, json).expect("prefs");
    let ruim = ListaDeDestinos::carregar(&arq).expect("carregar");
    l.adicionar(Destino::pasta_local("b", b.path()).expect("b"))
        .expect("b");
    // Remoto inexistente: falha do rclone, também isolada.
    let mut fantasma = Destino::nuvem("fantasma", TipoDeDestino::Drive, "naoexiste:SBF/Estante")
        .expect("fantasma");
    fantasma.ativo = true;
    let mut todos = ruim.clone();
    todos.adicionar(fantasma).expect("f");
    for d in l.todos() {
        todos.adicionar(d.clone()).expect("b");
    }
    // `adicionar` valida, então o inválido do arquivo é o único que não passou por ela.
    assert_eq!(todos.todos().len(), 3);

    let rel = atualizar_destinos(&Rclone::default(), &todos, &c, cofre.path(), &|_, _| {});
    assert_eq!(rel.len(), 3);
    assert!(rel[0].resultado.is_err(), "{:?}", rel[0]);
    assert!(rel[1].resultado.is_err(), "{:?}", rel[1]);
    assert_eq!(rel[2].resultado.as_ref().expect("b").enviados, 1);
    assert_eq!(nomes(b.path()).len(), 1);
}

#[test]
fn destino_inativo_e_ignorado() {
    let cofre = tempfile::tempdir().expect("cofre");
    let a = tempfile::tempdir().expect("a");
    let b = tempfile::tempdir().expect("b");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(&c, cofre.path(), &doc("aaaaaaaa11111111", "Luz", b"um"));
    let mut l = lista(a.path(), b.path());
    assert!(l.definir_ativo("b", false));
    let rel = atualizar_destinos(&Rclone::default(), &l, &c, cofre.path(), &|_, _| {});
    assert_eq!(rel.len(), 1);
    assert!(nomes(b.path()).is_empty());
    assert_eq!(nomes(a.path()).len(), 1);
}
