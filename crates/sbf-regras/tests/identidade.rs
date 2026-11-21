//! A identidade do documento (padrão com `identidade`): o mesmo documento que chega por dois
//! canais, com texto um pouco diferente, é repetido pelo padrão. Dados fictícios.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use sbf_regras::catalogo::{Catalogo, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::{Definicao, Extracao, FonteDaExtracao, Formato};
use sbf_regras::motor::Motor;
use sbf_regras::{
    Autor, Canal, Decisao, Dicas, Entrada, Estado, Leitura, Origem, Quem, Reconhecimento,
    TipoDeEntrada,
};

fn entrada(id: &str, texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: 1,
        id: id.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Gmail,
            id_externo: None,
        },
        sha256: Some(id.to_owned()),
        mime: Some("application/pdf".to_owned()),
        nome: Some("invoice.pdf".to_owned()),
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

fn def(contem: &str, identidade: &[&str]) -> Definicao {
    let mut d = Definicao::simples("fatura", contem);
    d.extrair.insert(
        "numero_da_invoice".into(),
        Extracao {
            regex: r"Invoice\s+n\S*\s*([A-Za-z0-9-]+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Texto,
        },
    );
    d.extrair.insert(
        "serie".into(),
        Extracao {
            regex: r"Serie:\s*(\S+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Texto,
        },
    );
    d.identidade = identidade.iter().map(|s| (*s).to_owned()).collect();
    d
}

fn nova(regra: &str, d: Definicao) -> NovaVersao {
    NovaVersao {
        regra: regra.to_owned(),
        nome: regra.to_owned(),
        definicao: d,
        script: None,
        autor: Autor::Usuario,
        motivo: "teste".to_owned(),
    }
}

fn ativa(c: &mut Catalogo, n: NovaVersao) -> VersaoDeRegra {
    let v = c.salvar_rascunho(n).unwrap();
    c.ativar(&v.regra, v.versao, &Quem::Usuario, None).unwrap();
    c.versao(&v.regra, v.versao).unwrap()
}

fn rec(l: &Leitura) -> &Reconhecimento {
    match &l.decisao {
        Decisao::Reconhecido(r) => r,
        outra => panic!("esperava reconhecido, veio {outra:?}"),
    }
}

/// Lê, marca o repetido e grava, como quem chama faz.
fn processar(c: &mut Catalogo, motor: &Motor, e: &Entrada) -> Leitura {
    let mut l = motor.ler(e);
    c.marcar_repetido(&mut l).unwrap();
    c.registrar_execucao(&l).unwrap();
    l
}

const POR_EMAIL: &str = "Servico Exemplo\nInvoice nº ABC123-0001\nTotal: 10\n";
const PELO_TELEGRAM: &str = "Servico Exemplo - encaminhado\n  Invoice n ABC123-0001  \nobrigado\n";

#[test]
fn a_segunda_entrada_com_a_mesma_identidade_e_repetida_da_primeira() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("invoice", def("servico exemplo", &["numero_da_invoice"])),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let primeira = processar(&mut c, &motor, &entrada("sha-email", POR_EMAIL));
    assert_eq!(rec(&primeira).identidade.as_deref(), Some("abc123-0001"));
    assert_eq!(rec(&primeira).repetido_de, None);
    let segunda = processar(&mut c, &motor, &entrada("sha-telegram", PELO_TELEGRAM));
    assert_eq!(rec(&segunda).repetido_de.as_deref(), Some("sha-email"));
    // Ler de novo a primeira continua sem repetido: a mais antiga é a original.
    let de_novo = processar(&mut c, &motor, &entrada("sha-email", POR_EMAIL));
    assert_eq!(rec(&de_novo).repetido_de, None);
    // E reprocessar a segunda não muda nada.
    let de_novo = processar(&mut c, &motor, &entrada("sha-telegram", PELO_TELEGRAM));
    assert_eq!(rec(&de_novo).repetido_de.as_deref(), Some("sha-email"));
}

#[test]
fn a_identidade_junta_os_campos_normalizados() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova(
            "invoice",
            def("servico exemplo", &["numero_da_invoice", "serie"]),
        ),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let a = processar(
        &mut c,
        &motor,
        &entrada("a", "Servico Exemplo\nInvoice nº ABC-1\nSerie: Zz\n"),
    );
    assert_eq!(rec(&a).identidade.as_deref(), Some("abc-1|zz"));
    // Mesmo número, outra série: outro documento.
    let b = processar(
        &mut c,
        &motor,
        &entrada("b", "Servico Exemplo\nInvoice nº ABC-1\nSerie: Yy\n"),
    );
    assert_eq!(rec(&b).repetido_de, None);
}

#[test]
fn a_mesma_identidade_em_padroes_diferentes_nao_e_repetida() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("fatura", def("fatura exemplo", &["numero_da_invoice"])),
    );
    ativa(
        &mut c,
        nova("recibo", def("recibo exemplo", &["numero_da_invoice"])),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let f = processar(
        &mut c,
        &motor,
        &entrada("f", "Fatura Exemplo\nInvoice nº ABC123-0001\n"),
    );
    let r = processar(
        &mut c,
        &motor,
        &entrada("r", "Recibo Exemplo\nInvoice nº ABC123-0001\n"),
    );
    assert_eq!(rec(&f).repetido_de, None);
    assert_eq!(rec(&r).repetido_de, None);
}

#[test]
fn identidade_incompleta_nao_grava_e_nao_marca() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova(
            "invoice",
            def("servico exemplo", &["numero_da_invoice", "serie"]),
        ),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    // Sem `Serie`: falta um campo da identidade.
    let a = processar(&mut c, &motor, &entrada("a", POR_EMAIL));
    let b = processar(&mut c, &motor, &entrada("b", POR_EMAIL));
    assert_eq!(rec(&a).identidade, None);
    assert_eq!(rec(&b).identidade, None);
    assert_eq!(rec(&b).repetido_de, None);
    assert_eq!(c.total_de_identidades().unwrap(), 0);
}

#[test]
fn padrao_sem_identidade_nunca_marca_repetido() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("invoice", def("servico exemplo", &[])));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    processar(&mut c, &motor, &entrada("a", POR_EMAIL));
    let b = processar(&mut c, &motor, &entrada("b", POR_EMAIL));
    assert_eq!(rec(&b).identidade, None);
    assert_eq!(rec(&b).repetido_de, None);
}

#[test]
fn versao_em_rascunho_nao_grava_identidade() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c
        .salvar_rascunho(nova(
            "invoice",
            def("servico exemplo", &["numero_da_invoice"]),
        ))
        .unwrap();
    assert_eq!(v.estado, Estado::Rascunho);
    // Um motor que carrega o rascunho (como a simulação e o pré-teste).
    let motor = Motor::novo(vec![v], Vec::new(), None);
    let a = processar(&mut c, &motor, &entrada("a", POR_EMAIL));
    let b = processar(&mut c, &motor, &entrada("b", POR_EMAIL));
    assert_eq!(rec(&a).identidade, None);
    assert_eq!(rec(&b).repetido_de, None);
    assert_eq!(c.total_de_identidades().unwrap(), 0);
}

#[test]
fn versao_consolidada_tambem_grava() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = ativa(
        &mut c,
        nova("invoice", def("servico exemplo", &["numero_da_invoice"])),
    );
    let motor = Motor::novo(
        vec![VersaoDeRegra {
            estado: Estado::Consolidada,
            ..v
        }],
        Vec::new(),
        None,
    );
    processar(&mut c, &motor, &entrada("a", POR_EMAIL));
    let b = processar(&mut c, &motor, &entrada("b", PELO_TELEGRAM));
    assert_eq!(rec(&b).repetido_de.as_deref(), Some("a"));
}

#[test]
fn a_identidade_ignora_maiusculas_e_espacos_das_pontas() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("invoice", def("servico exemplo", &["numero_da_invoice"])),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    processar(
        &mut c,
        &motor,
        &entrada("a", "Servico Exemplo\nInvoice nº Abc123-0001\n"),
    );
    let b = processar(
        &mut c,
        &motor,
        &entrada("b", "servico exemplo\nInvoice nº ABC123-0001 \n"),
    );
    assert_eq!(rec(&b).repetido_de.as_deref(), Some("a"));
}

#[test]
fn quantas_entradas_ja_vistas_teriam_a_mesma_identidade() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = ativa(
        &mut c,
        nova("invoice", def("servico exemplo", &["numero_da_invoice"])),
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    processar(&mut c, &motor, &entrada("a", POR_EMAIL));
    let b = motor.ler(&entrada("b", PELO_TELEGRAM));
    assert_eq!(
        c.entrada_da_identidade(&v.regra, rec(&b).identidade.as_deref().unwrap())
            .unwrap()
            .as_deref(),
        Some("a")
    );
}

#[test]
fn a_migracao_3_a_partir_da_2_preserva_os_dados() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("regras.sqlite");
    {
        let mut c = Catalogo::abrir(&caminho).unwrap();
        ativa(
            &mut c,
            nova("invoice", def("servico exemplo", &["numero_da_invoice"])),
        );
        let motor = Motor::das_vigentes(&c, None).unwrap();
        let l = motor.ler(&entrada("a", POR_EMAIL));
        c.registrar_execucao(&l).unwrap();
        // Volta o arquivo ao esquema 2: sem a tabela nova, como estava antes desta mudança.
        c.sql_cru_para_testes("DROP TABLE identidade_da_entrada")
            .unwrap();
        c.sql_cru_para_testes("PRAGMA user_version = 2").unwrap();
    }
    let c = Catalogo::abrir(&caminho).unwrap();
    assert_eq!(c.versao_do_esquema().unwrap(), 3);
    assert_eq!(c.total_de_execucoes().unwrap(), 1);
    assert_eq!(c.versoes("invoice").unwrap().len(), 1);
    assert_eq!(c.total_de_identidades().unwrap(), 0);
}
