//! A marcação das listas (spec 15, D3 e D4): quantos padrões casaram numa entrada, se a
//! última decisão foi Ambíguo, e o ponto de parada da passada de fundo. Dados fictícios.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use sbf_regras::catalogo::{Catalogo, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::{Condicao, Definicao};
use sbf_regras::marcacao::{CorDaMarca, Marca};
use sbf_regras::motor::Motor;
use sbf_regras::{
    Autor, Canal, Decisao, Dicas, Entrada, Estado, Execucao, Leitura, Origem, Quem,
    ResultadoDaExecucao, TipoDeEntrada, VERSAO_DO_ESQUEMA,
};

const COMPROVANTE: &str =
    "Banco Exemplo\nComprovante PIX\nFavorecido: Fulano de Tal\nValor: R$ 10,00\n";

fn entrada(id: &str, texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: id.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Cofre,
            id_externo: None,
        },
        sha256: Some(id.to_owned()),
        mime: Some("application/pdf".to_owned()),
        nome: Some("comprovante.pdf".to_owned()),
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

fn nova(regra: &str, tipo: &str, contem: &str, prioridade: i32) -> NovaVersao {
    let mut d = Definicao::simples(tipo, contem);
    d.prioridade = prioridade;
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

fn marca(c: &Catalogo, id: &str) -> Marca {
    c.marcas(&[id.to_owned()])
        .unwrap()
        .get(id)
        .copied()
        .unwrap_or_default()
}

#[test]
fn entrada_que_nenhum_padrao_casou_fica_sem_cor() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("a1", "texto que nenhum padrão conhece"));
    assert!(matches!(l.decisao, Decisao::Nenhuma));
    c.registrar_execucao(&l).unwrap();
    let m = marca(&c, "a1");
    assert_eq!(m.padroes, 0);
    assert_eq!(m.cor(), CorDaMarca::SemCor);
    // Uma entrada que o catálogo nunca viu também é sem cor, e nem aparece no mapa.
    assert!(c.marcas(&["nunca-vista".to_owned()]).unwrap().is_empty());
}

#[test]
fn padrao_que_casou_pinta_de_verde_e_conta_os_padroes_distintos() {
    let mut c = Catalogo::em_memoria().unwrap();
    // Prioridade diferente: um vence, o outro "não prevaleceu", e os dois casaram.
    ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 5));
    ativa(&mut c, nova("banco", "banco", "banco exemplo", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("b1", COMPROVANTE));
    assert!(matches!(l.decisao, Decisao::Reconhecido(_)));
    c.registrar_execucao(&l).unwrap();
    // Gravar de novo não duplica a contagem.
    c.registrar_execucao(&l).unwrap();
    let m = marca(&c, "b1");
    assert_eq!(m.padroes, 2);
    assert!(!m.ambiguo);
    assert_eq!(m.cor(), CorDaMarca::Verde);
}

#[test]
fn ultima_decisao_ambigua_pinta_de_ambar_e_uma_decisao_clara_depois_tira_o_ambar() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("regra-a", "tipo-a", "comprovante pix", 0));
    let b = ativa(&mut c, nova("regra-b", "tipo-b", "banco exemplo", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("c1", COMPROVANTE));
    assert!(matches!(l.decisao, Decisao::Ambiguo { .. }));
    c.registrar_execucao(&l).unwrap();
    let m = marca(&c, "c1");
    assert_eq!(m.padroes, 2);
    assert!(m.ambiguo);
    assert_eq!(m.cor(), CorDaMarca::Ambar);

    // O usuário desativa um dos dois: a próxima leitura é clara, e o âmbar sai. O verde fica,
    // inclusive para o que foi desativado ("em algum momento").
    c.desativar(&b.regra, &Quem::Usuario).unwrap();
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("c1", COMPROVANTE));
    assert!(matches!(l.decisao, Decisao::Reconhecido(_)));
    c.registrar_execucao(&l).unwrap();
    let m = marca(&c, "c1");
    assert!(!m.ambiguo);
    assert_eq!(m.cor(), CorDaMarca::Verde);
    assert_eq!(m.padroes, 2);
}

#[test]
fn desativar_o_padrao_mantem_o_verde() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("d1", COMPROVANTE));
    c.registrar_execucao(&l).unwrap();
    c.desativar(&v.regra, &Quem::Usuario).unwrap();
    assert!(c.vigentes().unwrap().is_empty());
    assert_eq!(marca(&c, "d1").cor(), CorDaMarca::Verde);
}

#[test]
fn a_leitura_de_um_padrao_com_erro_nao_conta_como_casou() {
    let mut c = Catalogo::em_memoria().unwrap();
    let l = Leitura {
        entrada: "e1".to_owned(),
        decisao: Decisao::Nenhuma,
        execucoes: vec![Execucao {
            regra: "quebrada".to_owned(),
            versao: 1,
            resultado: ResultadoDaExecucao::Erro,
            motivo: Some("regex".to_owned()),
            detalhe: None,
            duracao_us: 1,
        }],
    };
    c.registrar_execucao(&l).unwrap();
    assert_eq!(c.total_de_execucoes().unwrap(), 1);
    assert_eq!(marca(&c, "e1").cor(), CorDaMarca::SemCor);
}

#[test]
fn a_consulta_em_lote_responde_varias_entradas_de_uma_vez() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    // Mais entradas que o tamanho de um pedaço da consulta.
    let ids: Vec<String> = (0..1200).map(|i| format!("lote-{i:04}")).collect();
    for (i, id) in ids.iter().enumerate() {
        let texto = if i % 3 == 0 { COMPROVANTE } else { "nada" };
        c.registrar_execucao(&motor.ler(&entrada(id, texto)))
            .unwrap();
    }
    let m = c.marcas(&ids).unwrap();
    assert_eq!(m.len(), 400);
    assert!(m.values().all(|x| x.padroes == 1));
    assert!(m.contains_key("lote-0000") && !m.contains_key("lote-0001"));
}

#[test]
fn o_esquema_novo_nao_estraga_um_catalogo_antigo_e_migra_o_ambiguo() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("regras.sqlite");
    {
        let mut c = Catalogo::abrir(&caminho).unwrap();
        ativa(&mut c, nova("regra-a", "tipo-a", "comprovante pix", 0));
        ativa(&mut c, nova("regra-b", "tipo-b", "banco exemplo", 0));
        let motor = Motor::das_vigentes(&c, None).unwrap();
        c.registrar_execucao(&motor.ler(&entrada("f1", COMPROVANTE)))
            .unwrap();
        // Volta ao esquema 1: o que havia antes da marcação.
        c.sql_cru_para_testes("DROP TABLE decisao_da_entrada")
            .unwrap();
        c.sql_cru_para_testes("DROP TABLE passada_de_fundo")
            .unwrap();
        c.sql_cru_para_testes("PRAGMA user_version = 1").unwrap();
    }
    let c = Catalogo::abrir(&caminho).unwrap();
    assert_eq!(c.versao_do_esquema().unwrap(), 2);
    let m = marca(&c, "f1");
    assert!(
        m.ambiguo,
        "o ambíguo antigo é recuperado pelo motivo da execução"
    );
    assert_eq!(m.padroes, 2);
}

#[test]
fn a_execucao_de_fundo_nao_mexe_na_decisao() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("regra-a", "tipo-a", "comprovante pix", 0));
    ativa(&mut c, nova("regra-b", "tipo-b", "banco exemplo", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let ambigua = motor.ler(&entrada("g1", COMPROVANTE));
    c.registrar_execucao(&ambigua).unwrap();
    assert!(marca(&c, "g1").ambiguo);

    // Uma leitura de fundo de uma versão só (reconhece, sem enxergar o empate) não limpa o
    // âmbar: só a leitura do conjunto inteiro decide.
    let so_a = Motor::novo(c.versoes("regra-a").unwrap(), Vec::new(), None);
    let parcial = so_a.ler(&entrada("g1", COMPROVANTE));
    assert!(matches!(parcial.decisao, Decisao::Reconhecido(_)));
    c.registrar_execucao_de_fundo(&parcial).unwrap();
    assert!(marca(&c, "g1").ambiguo);

    // E, para uma entrada nova, só a execução é gravada: a marca é verde, sem decisão.
    let nova_ = so_a.ler(&entrada("g2", COMPROVANTE));
    c.registrar_execucao_de_fundo(&nova_).unwrap();
    let m = marca(&c, "g2");
    assert_eq!((m.padroes, m.ambiguo), (1, false));
    // A decisão completa depois resolve o âmbar da entrada nova.
    c.registrar_decisao(&motor.ler(&entrada("g2", COMPROVANTE)))
        .unwrap();
    assert!(marca(&c, "g2").ambiguo);
}

#[test]
fn a_passada_retoma_do_ponto_e_repete_so_a_versao_nova() {
    let mut c = Catalogo::em_memoria().unwrap();
    let a = ativa(&mut c, nova("regra-a", "tipo-a", "comprovante pix", 0));
    assert_eq!(c.ponto_da_passada(&a).unwrap(), 0);
    c.avancar_a_passada(&a, 40, 40).unwrap();
    c.avancar_a_passada(&a, 75, 35).unwrap();
    assert_eq!(c.ponto_da_passada(&a).unwrap(), 75);
    assert_eq!(c.progresso_da_passada().unwrap(), 75);

    // O ponto ficou no arquivo: reabrir o catálogo retoma de onde parou.
    // (Mesma conexão aqui; o teste de arquivo abaixo cobre o reinício.)

    // Uma versão alterada (outro checksum) começa do zero; a antiga não é refeita.
    let mut def = Definicao::simples("tipo-a", "comprovante pix");
    def.detectar
        .todos
        .push(Condicao::Contem("comprovante pix".into()));
    def.detectar
        .todos
        .push(Condicao::Contem("favorecido".into()));
    let a2 = ativa(
        &mut c,
        NovaVersao {
            regra: "regra-a".to_owned(),
            nome: "regra-a".to_owned(),
            definicao: def,
            script: None,
            autor: Autor::Usuario,
            motivo: "refina".to_owned(),
        },
    );
    assert_eq!(a2.versao, 2);
    assert_eq!(a2.estado, Estado::Ativa);
    assert_eq!(c.ponto_da_passada(&a2).unwrap(), 0);
    assert_eq!(c.ponto_da_passada(&a).unwrap(), 75);
}

#[test]
fn o_ponto_da_passada_sobrevive_a_reabrir_o_arquivo() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("regras.sqlite");
    let v = {
        let mut c = Catalogo::abrir(&caminho).unwrap();
        let v = ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 0));
        c.avancar_a_passada(&v, 12, 12).unwrap();
        v
    };
    let c = Catalogo::abrir(&caminho).unwrap();
    assert_eq!(c.ponto_da_passada(&v).unwrap(), 12);
}

#[test]
fn ja_casaram_lista_os_padroes_que_reconheceram_a_entrada_mesmo_desativados() {
    let mut c = Catalogo::em_memoria().unwrap();
    let pix = ativa(&mut c, nova("pix", "comprovante pix", "comprovante pix", 1));
    ativa(&mut c, nova("banco", "comprovante", "banco exemplo", 0));
    ativa(&mut c, nova("boleto", "boleto", "linha digitavel", 0));
    let motor = Motor::das_vigentes(&c, None).unwrap();
    c.registrar_execucao(&motor.ler(&entrada("a1", COMPROVANTE)))
        .unwrap();
    c.desativar(&pix.regra, &Quem::Usuario).unwrap();
    let casaram = c.padroes_que_casaram("a1").unwrap();
    assert_eq!(
        casaram.into_iter().collect::<Vec<_>>(),
        vec!["banco".to_owned(), "pix".to_owned()]
    );
    assert!(c.padroes_que_casaram("nunca-vista").unwrap().is_empty());
}
