//! INV-11: a estante tem só documentos, com o nome de `vista-nome`, bytes originais,
//! PDF protegido protegido, e é incremental (correção vira rename).

#![cfg(unix)]

mod comum;

use comum::{Doc, arvore, inserir, nomes, novo_banco, rclone_com_registro};
use sbf_estante::Estante;
use std::fs;

const PDF_PROTEGIDO: &[u8] =
    b"%PDF-1.7\n1 0 obj<</Encrypt 2 0 R>>endobj\nconteudo ficticio cifrado\n";
const PDF_COMUM: &[u8] = b"%PDF-1.4\nfatura ficticia\n";

fn doc<'a>(hash: &'a str, estado: &'a str, conteudo: Option<&'a [u8]>) -> Doc<'a> {
    Doc {
        hash,
        estado,
        tipo: Some("fatura"),
        data: Some("2026-10-25"),
        ciclo: Some("2026-10"),
        fornecedor: Some("Nu Pagamentos S.A."),
        valor: Some(-123_456),
        ext: "pdf",
        conteudo,
        nome_original: "fatura.pdf",
    }
}

fn sem_progresso() -> impl Fn(sbf_estante::Andamento) + Sync {
    |_| {}
}

#[test]
fn so_documentos_com_nome_da_vista_e_bytes_originais() {
    let cofre = tempfile::tempdir().expect("cofre");
    let remoto = tempfile::tempdir().expect("remoto");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));

    inserir(
        &c,
        cofre.path(),
        &doc("aaaaaaaa11111111", "documento", Some(PDF_COMUM)),
    );
    inserir(
        &c,
        cofre.path(),
        &Doc {
            tipo: Some("boleto"),
            data: None,
            ciclo: None,
            fornecedor: None,
            valor: None,
            nome_original: "Boleto Luz.pdf",
            ..doc("bbbbbbbb22222222", "documento", Some(PDF_PROTEGIDO))
        },
    );
    // Não entram: descartado, novo, dúvida e documento sem original no acervo.
    inserir(
        &c,
        cofre.path(),
        &doc("cccccccc33333333", "descartado", Some(b"x")),
    );
    inserir(
        &c,
        cofre.path(),
        &doc("dddddddd44444444", "novo", Some(b"y")),
    );
    inserir(
        &c,
        cofre.path(),
        &doc("eeeeeeee55555555", "duvida", Some(b"z")),
    );
    inserir(
        &c,
        cofre.path(),
        &doc("ffffffff66666666", "documento", None),
    );

    let estante =
        Estante::nova(Default::default(), remoto.path().to_string_lossy()).expect("estante");
    let rel = estante
        .atualizar(&c, cofre.path(), &sem_progresso())
        .expect("atualizar");
    assert_eq!(rel.enviados, 2, "{rel:?}");
    assert_eq!(rel.sem_copia, 1);
    assert!(rel.falhas.is_empty(), "{:?}", rel.falhas);

    assert_eq!(
        nomes(remoto.path()),
        vec![
            "2026-10/faturas/2026-10-25_nu-pagamentos-s-a_1234,56_aaaaaaaa.pdf".to_owned(),
            // Sem data-chave, sem fornecedor e sem valor: o nome original e o hash.
            "a-classificar/boletos/boleto-luz_bbbbbbbb.pdf".to_owned(),
        ]
    );
    let arq = arvore(remoto.path());
    assert_eq!(arq[0].1, PDF_COMUM);
    // O PDF protegido chega protegido: os bytes são idênticos aos do acervo.
    assert_eq!(arq[1].1, PDF_PROTEGIDO);
}

#[test]
fn atualizacao_incremental_renomeia_em_vez_de_reenviar() {
    let cofre = tempfile::tempdir().expect("cofre");
    let remoto = tempfile::tempdir().expect("remoto");
    let registro = tempfile::tempdir().expect("registro");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(
        &c,
        cofre.path(),
        &doc("aaaaaaaa11111111", "documento", Some(PDF_COMUM)),
    );
    inserir(
        &c,
        cofre.path(),
        &doc("bbbbbbbb22222222", "documento", Some(b"segundo")),
    );

    let embrulho = rclone_com_registro(registro.path());
    let estante =
        Estante::nova(embrulho.rclone.clone(), remoto.path().to_string_lossy()).expect("estante");
    estante
        .atualizar(&c, cofre.path(), &sem_progresso())
        .expect("primeira");
    let antes = arvore(remoto.path());
    assert_eq!(antes.len(), 2);

    // Alheio na estante: nunca é tocado.
    fs::write(remoto.path().join("minha-nota.txt"), "meu").expect("nota");

    // Segunda execução sem mudança: não faz nada.
    fs::write(&embrulho.cmdlines, "").expect("zerar");
    let parado = estante
        .atualizar(&c, cofre.path(), &sem_progresso())
        .expect("parada");
    assert_eq!(
        (parado.enviados, parado.renomeados, parado.apagados),
        (0, 0, 0)
    );
    assert_eq!(parado.intactos, 2);
    assert_eq!(parado.estranhos, 1);
    let chamadas = fs::read_to_string(&embrulho.cmdlines).expect("log");
    assert!(
        !chamadas.contains("copyto") && !chamadas.contains("moveto"),
        "{chamadas}"
    );

    // Correção de fornecedor e de data do primeiro; o segundo é descartado.
    c.execute(
        "UPDATE achado SET supplier_name_raw = 'Nubank', data_chave = '2026-11-02', ciclo_id = '2026-11'
          WHERE content_hash = 'aaaaaaaa11111111'",
        [],
    )
    .expect("corrigir");
    c.execute(
        "UPDATE achado SET estado = 'descartado' WHERE content_hash = 'bbbbbbbb22222222'",
        [],
    )
    .expect("descartar");
    inserir(
        &c,
        cofre.path(),
        &Doc {
            ciclo: Some("2026-11"),
            ..doc("99999999aaaaaaaa", "documento", Some(b"terceiro"))
        },
    );

    fs::write(&embrulho.cmdlines, "").expect("zerar");
    let rel = estante
        .atualizar(&c, cofre.path(), &sem_progresso())
        .expect("terceira");
    assert_eq!(
        (rel.renomeados, rel.apagados, rel.enviados),
        (1, 1, 1),
        "{rel:?}"
    );
    assert!(rel.falhas.is_empty(), "{:?}", rel.falhas);

    let depois = arvore(remoto.path());
    let achado = depois
        .iter()
        .find(|(n, _)| n.starts_with("2026-11/faturas/2026-11-02_nubank_1234,56_aaaaaaaa"))
        .expect("arquivo renomeado");
    assert_eq!(achado.1, PDF_COMUM, "mesmos bytes depois do rename");
    let todos: Vec<_> = depois.iter().map(|(n, _)| n.as_str()).collect();
    assert!(todos.contains(&"minha-nota.txt"), "{todos:?}");
    assert!(
        !todos.iter().any(|n| n.contains("bbbbbbbb")),
        "descartado saiu: {todos:?}"
    );
    assert!(
        todos.iter().any(|n| n.contains("99999999")),
        "novo entrou: {todos:?}"
    );
    // A pasta do ciclo antigo ficou vazia e foi removida.
    assert!(!remoto.path().join("2026-10").exists(), "{todos:?}");

    // O envio foi só o novo; o renomeado usou moveto, não copyto.
    let chamadas = fs::read_to_string(&embrulho.cmdlines).expect("log");
    assert_eq!(chamadas.matches("copyto").count(), 1, "{chamadas}");
    assert_eq!(chamadas.matches("moveto").count(), 1, "{chamadas}");
}

#[test]
fn plano_nao_mexe_em_nada() {
    let cofre = tempfile::tempdir().expect("cofre");
    let remoto = tempfile::tempdir().expect("remoto");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(
        &c,
        cofre.path(),
        &doc("aaaaaaaa11111111", "documento", Some(PDF_COMUM)),
    );
    let estante =
        Estante::nova(Default::default(), remoto.path().to_string_lossy()).expect("estante");
    let plano = estante.planejar(&c, cofre.path()).expect("plano");
    assert_eq!(plano.enviar.len(), 1);
    assert!(nomes(remoto.path()).is_empty());
}

#[test]
fn estante_ainda_inexistente_conta_como_vazia() {
    let remoto = tempfile::tempdir().expect("remoto");
    let estante = Estante::nova(
        Default::default(),
        remoto.path().join("SBF/Estante").to_string_lossy(),
    )
    .expect("estante");
    assert!(estante.listar().expect("listar").is_empty());
}

#[test]
fn destino_invalido_e_recusado() {
    assert!(Estante::nova(Default::default(), "").is_err());
    assert!(Estante::nova(Default::default(), "--config=/x").is_err());
}
