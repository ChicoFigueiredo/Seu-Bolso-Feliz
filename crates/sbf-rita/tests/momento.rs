//! Spec 15: o `momento` do item de Rita vem do inventário (a menor data de origem), e é ele que
//! completa o ano de "Vencimento em 25 de Novembro" no pré-teste. Cofre fictício, sem rede.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use sbf_inventario::banco::Inventario;
use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::{Definicao, Extracao, FonteDaExtracao, Formato};
use sbf_regras::{Autor, Valor};
use sbf_rita::cofre::momento_do_documento;
use sbf_rita::pre_teste::pre_testar;
use sbf_rita::{Contexto, DadosDaTela, entrada_de_documento};

const CONTA_DE_AGUA: &str =
    "Conta de Agua Exemplo\nContrato 998877-6\nVencimento em 25 de Novembro\nValor R$ 89,90\n";

fn sha() -> String {
    "ab".repeat(32)
}

fn cofre_com_documento() -> tempfile::TempDir {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    std::fs::create_dir_all(&cofre).expect("cofre");
    let mut inv = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inv.migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    let c = inv.conexao();
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:r', 'disco', 'Disco')",
        [],
    )
    .expect("fonte");
    c.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
         VALUES (900, 'disco:r', 'E1', 'raiz', 't')",
        [],
    )
    .expect("varredura");
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, mime_type, primeiro_visto_em)
         VALUES (?1, 10, 'A', 'text/plain', '2026-11-20T10:00:00.000Z')",
        [sha()],
    )
    .expect("achado");
    for (chave, mtime) in [
        ("a", "2026-11-18T12:00:00.000Z"),
        ("b", "2026-11-19T08:00:00.000Z"),
    ] {
        c.execute(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
               local_mtime, filename) VALUES (?1, 'disco:r', 900, 'local_file', ?2, ?3, 'agua.txt')",
            rusqlite::params![sha(), chave, mtime],
        )
        .expect("origem");
    }
    raiz
}

fn regra_da_competencia() -> NovaVersao {
    let mut d = Definicao::simples("conta-agua-exemplo", "conta de agua exemplo");
    d.extrair.insert(
        "competencia".into(),
        Extracao {
            regex: r"Vencimento em (\d{1,2} de [A-Za-zç]+)".into(),
            grupo: 1,
            formato: Formato::Competencia,
            fonte: FonteDaExtracao::Texto,
        },
    );
    d.obrigatorios.push("competencia".into());
    NovaVersao {
        regra: "conta-agua-exemplo".into(),
        nome: "Conta de água exemplo".into(),
        definicao: d,
        script: None,
        autor: Autor::Usuario,
        motivo: "teste".into(),
    }
}

fn contexto(cofre: &std::path::Path, momento: Option<String>) -> Contexto {
    let mut item = entrada_de_documento(
        &sha(),
        Some("agua.txt"),
        Some("text/plain"),
        Some(CONTA_DE_AGUA),
    );
    item.momento = momento;
    Contexto::novo(
        DadosDaTela::de_documento(cofre, item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
}

#[test]
fn pre_testar_completa_o_ano_pelo_momento_do_inventario() {
    let raiz = cofre_com_documento();
    let cofre = raiz.path().join("cofre");
    let mut catalogo = Catalogo::abrir(&cofre.join("regras.sqlite")).expect("catálogo");
    let v = catalogo
        .salvar_rascunho(regra_da_competencia())
        .expect("rascunho");

    let momento = momento_do_documento(&cofre, &sha());
    assert_eq!(momento.as_deref(), Some("2026-11-18T12:00:00.000Z"));
    let pre = pre_testar(&contexto(&cofre, momento), &catalogo, &v.regra, v.versao).expect("pré");
    assert!(pre.item.casa, "{:?}", pre.item.diagnostico);
    assert!(pre.item.faltam.is_empty(), "{:?}", pre.item.faltam);
    let valor = pre
        .item
        .campos
        .iter()
        .find(|(n, _)| n == "competencia")
        .map(|(_, v)| v.clone())
        .expect("competencia");
    assert_eq!(valor, Valor::Competencia("2026-11".into()).to_string());

    // Sem o momento o ano não se completa: o campo obrigatório falta.
    let sem = pre_testar(&contexto(&cofre, None), &catalogo, &v.regra, v.versao).expect("pré");
    assert!(sem.item.faltam.contains(&"competencia".to_owned()));
}
