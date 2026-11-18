//! Testes de comportamento do motor de regras, com dados fictícios ("Banco Exemplo",
//! "Fulano de Tal"). Cada AC-LUA citado é da spec 13.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;
use sbf_regras::catalogo::{Catalogo, Esperado, EstadoDaProposta, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::{
    Condicao, Definicao, Extracao, FonteDaExtracao, Formato, ModeloDeProposta,
};
use sbf_regras::motor::{Motor, sem_duracao};
use sbf_regras::rascunho::{Correcao, rascunho_da_correcao};
use sbf_regras::script::{ErroDoScript, ExecutorDeScript, ExecutorFicticio, RespostaDoScript};
use sbf_regras::simulacao::{ResumoDaDecisao, simular};
use sbf_regras::{
    Autor, Campo, Canal, DeOnde, Decisao, Dicas, Entrada, ErroDasRegras, Estado,
    LIMIAR_DE_CONSOLIDACAO, Origem, Proposta, Quem, ResultadoDaExecucao, Retorno, TipoDeEntrada,
    Valor,
};
use serde_json::json;

const COMPROVANTE: &str = "Banco Exemplo\nComprovante PIX\nFavorecido: Fulano de Tal\n\
Valor: R$ 1.234,56\nData: 06/10/2026\nAutenticação: ABC123XYZ\n";

fn entrada(id: &str, texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: 1,
        id: id.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Arrastado,
            id_externo: None,
        },
        sha256: None,
        mime: Some("application/pdf".to_owned()),
        nome: Some("comprovante.pdf".to_owned()),
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

fn extracao(regex: &str, formato: Formato) -> Extracao {
    Extracao {
        regex: regex.to_owned(),
        grupo: 1,
        formato,
        fonte: FonteDaExtracao::Texto,
    }
}

fn def_comprovante() -> Definicao {
    let mut d = Definicao::simples("comprovante-pix-banco-exemplo", "comprovante pix");
    d.detectar
        .todos
        .push(Condicao::Contem("banco exemplo".into()));
    d.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Valor:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    d.extrair.insert(
        "data".into(),
        extracao(r"Data:\s*(\d{2}/\d{2}/\d{4})", Formato::Data),
    );
    d.extrair.insert(
        "autenticacao".into(),
        extracao(r"Autentica\S+:\s*(\S+)", Formato::Texto),
    );
    d.obrigatorios = vec!["valor_centavos".into()];
    d.propor = vec![ModeloDeProposta::ProvarRegistro { janela_dias: 2 }];
    d
}

fn nova(regra: &str, def: Definicao) -> NovaVersao {
    NovaVersao {
        regra: regra.to_owned(),
        nome: regra.to_owned(),
        definicao: def,
        script: None,
        autor: Autor::Usuario,
        motivo: "teste".to_owned(),
    }
}

/// Salva e ativa (pelo usuário) uma versão.
fn ativa(c: &mut Catalogo, n: NovaVersao, exec: Option<&dyn ExecutorDeScript>) -> VersaoDeRegra {
    let v = c.salvar_rascunho(n).unwrap();
    c.ativar(&v.regra, v.versao, &Quem::Usuario, exec).unwrap();
    c.versao(&v.regra, v.versao).unwrap()
}

fn reconhecido(l: &sbf_regras::Leitura) -> &sbf_regras::Reconhecimento {
    match &l.decisao {
        Decisao::Reconhecido(r) => r,
        outra => panic!("esperava reconhecido, veio {outra:?}"),
    }
}

// ---------------------------------------------------------------- AC-LUA-01 e AC-LUA-08

#[test]
fn ac_lua_01_tipo_novo_por_configuracao_reconhece_apos_reload() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("regras.sqlite");
    {
        let mut c = Catalogo::abrir(&caminho).unwrap();
        let v = c
            .salvar_rascunho(nova("comprovante-pix-banco-exemplo", def_comprovante()))
            .unwrap();
        assert_eq!(v.versao, 1);
        assert_eq!(v.estado, Estado::Rascunho);
        // Rascunho nunca roda na ingestão.
        assert!(c.vigentes().unwrap().is_empty());
        c.ativar(&v.regra, 1, &Quem::Usuario, None).unwrap();
    }
    // Reload: abre de novo o arquivo e monta o motor das vigentes.
    let c = Catalogo::abrir(&caminho).unwrap();
    assert_eq!(c.versao_do_esquema().unwrap(), 1);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = entrada("doc-1", COMPROVANTE);
    let l = motor.ler(&e);
    let r = reconhecido(&l);
    assert_eq!(r.tipo_documental, "comprovante-pix-banco-exemplo");
    assert_eq!(r.estado, Estado::Ativa);
    assert_eq!(r.campos["valor_centavos"].valor, Valor::Centavos(123_456));
    assert_eq!(r.campos["data"].valor, Valor::Data("2026-10-06".into()));
    assert_eq!(
        r.campos["autenticacao"].valor,
        Valor::Texto("ABC123XYZ".into())
    );
    assert!(!r.revisar);
    assert_eq!(
        r.propostas,
        vec![Proposta::ProvarRegistro {
            valor_centavos: 123_456,
            data: "2026-10-06".into(),
            janela_dias: 2,
            conta: None
        }]
    );
    // Outro tipo de documento: nenhuma regra.
    assert_eq!(
        motor.ler(&entrada("x", "Fatura de energia")).decisao,
        Decisao::Nenhuma
    );
}

#[test]
fn ac_lua_08_todo_campo_tem_de_onde_e_o_trecho_aponta_para_o_texto_certo() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("comprovante-pix-banco-exemplo", def_comprovante()),
        None,
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = entrada("doc-1", COMPROVANTE);
    let l = motor.ler(&e);
    let r = reconhecido(&l);
    let texto = e.texto.as_deref().unwrap();
    assert_eq!(r.campos.len(), 3);
    for (nome, campo) in &r.campos {
        match &campo.de_onde {
            DeOnde::Trecho { inicio, fim } => {
                let trecho = &texto[*inicio..*fim];
                let reparsed = match &campo.valor {
                    Valor::Centavos(_) => {
                        sbf_regras::definicao::parse_centavos(trecho).map(Valor::Centavos)
                    }
                    Valor::Data(_) => sbf_regras::definicao::parse_data(trecho).map(Valor::Data),
                    Valor::Texto(_) => Some(Valor::Texto(trecho.to_owned())),
                    _ => None,
                };
                assert_eq!(
                    reparsed.as_ref(),
                    Some(&campo.valor),
                    "campo {nome}: trecho `{trecho}`"
                );
            }
            outra => panic!("campo {nome} sem trecho: {outra:?}"),
        }
    }
    assert_eq!(
        &texto[match &r.campos["valor_centavos"].de_onde {
            DeOnde::Trecho { inicio, .. } => *inicio,
            _ => 0,
        }..][..8],
        "1.234,56"
    );
}

#[test]
fn campo_obrigatorio_ausente_manda_revisar() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("comprovante-pix-banco-exemplo", def_comprovante()),
        None,
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada(
        "d",
        "Banco Exemplo\nComprovante PIX\nsem valor aqui\n",
    ));
    let r = reconhecido(&l);
    assert!(r.revisar);
    assert!(r.motivos.iter().any(|m| m.contains("valor_centavos")));
    assert!(r.propostas.is_empty());
    assert!(
        r.motivos
            .iter()
            .any(|m| m.contains("proposta provar_registro não gerada"))
    );
}

// ---------------------------------------------------------------- AC-LUA-02

#[test]
fn ac_lua_02_regra_especifica_vence_a_geral_e_nao_mexe_em_tipo_antigo() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova(
            "comprovante-pix",
            Definicao::simples("comprovante-pix", "comprovante pix"),
        ),
        None,
    );
    ativa(
        &mut c,
        nova("comprovante-pix-banco-exemplo", def_comprovante()),
        None,
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();

    let novo = entrada("novo", COMPROVANTE);
    let antigo = entrada("antigo", "Banco Outro\nComprovante PIX\nValor: R$ 5,00\n");
    let l = motor.ler(&novo);
    let r = reconhecido(&l);
    assert_eq!(r.tipo_documental, "comprovante-pix-banco-exemplo");
    assert!(
        r.motivos
            .iter()
            .any(|m| m.contains("mais específica: 2 condições contra 1")),
        "{:?}",
        r.motivos
    );
    assert_eq!(l.execucoes.len(), 2);

    let antes = motor.ler(&antigo);
    assert_eq!(reconhecido(&antes).tipo_documental, "comprovante-pix");

    // A regra específica muda (nova versão): o tipo antigo segue idêntico.
    let mut d2 = def_comprovante();
    d2.tipo_documental = "comprovante-pix-banco-exemplo-v2".into();
    ativa(&mut c, nova("comprovante-pix-banco-exemplo", d2), None);
    let motor2 = Motor::das_vigentes(&c, None).unwrap();
    let depois = motor2.ler(&antigo);
    assert_eq!(antes.decisao, depois.decisao);
    assert_eq!(
        reconhecido(&motor2.ler(&novo)).tipo_documental,
        "comprovante-pix-banco-exemplo-v2"
    );
}

#[test]
fn prioridade_maior_vence_com_motivo() {
    let mut c = Catalogo::em_memoria().unwrap();
    let mut geral = Definicao::simples("geral", "comprovante pix");
    geral.prioridade = 10;
    ativa(&mut c, nova("a-geral", geral), None);
    ativa(&mut c, nova("b-especifica", def_comprovante()), None);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("x", COMPROVANTE));
    let r = reconhecido(&l);
    assert_eq!(r.regra, "a-geral");
    assert!(
        r.motivos
            .iter()
            .any(|m| m == "prioridade 10 > 0 da regra b-especifica")
    );
}

// ---------------------------------------------------------------- AC-LUA-03

#[test]
fn ac_lua_03_empate_discordante_e_ambiguo_e_nao_grava_proposta() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("regra-a", Definicao::simples("tipo-a", "comprovante pix")),
        None,
    );
    ativa(
        &mut c,
        nova("regra-b", Definicao::simples("tipo-b", "banco exemplo")),
        None,
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let l = motor.ler(&entrada("x", COMPROVANTE));
    match &l.decisao {
        Decisao::Ambiguo { candidatas, motivo } => {
            assert_eq!(candidatas.len(), 2);
            assert!(motivo.contains("tipo_documental"), "{motivo}");
        }
        outra => panic!("{outra:?}"),
    }
    c.registrar_execucao(&l).unwrap();
    assert!(c.propostas(None).unwrap().is_empty());
}

#[test]
fn empate_com_campo_critico_diferente_e_ambiguo_e_concordante_escolhe_menor_id() {
    let mut c = Catalogo::em_memoria().unwrap();
    // Mesmo tipo, mas valores críticos diferentes.
    let mut a = Definicao::simples("t", "comprovante pix");
    a.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Valor:\s*R\$\s*([\d.,]+)", Formato::Centavos),
    );
    let mut b = Definicao::simples("t", "banco exemplo");
    b.extrair.insert(
        "valor_centavos".into(),
        extracao(r"Fatura:\s*([\d.,]+)", Formato::Centavos),
    );
    ativa(&mut c, nova("regra-a", a), None);
    ativa(&mut c, nova("regra-b", b), None);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let texto = format!("{COMPROVANTE}Fatura: 10,00\n");
    let l = motor.ler(&entrada("x", &texto));
    match &l.decisao {
        Decisao::Ambiguo { motivo, .. } => assert!(motivo.contains("valor_centavos"), "{motivo}"),
        outra => panic!("{outra:?}"),
    }
    // Se concordam (a segunda não acha o valor), vence a de menor id.
    let l = motor.ler(&entrada("y", COMPROVANTE));
    let r = reconhecido(&l);
    assert_eq!(r.regra, "regra-a");
    assert!(r.motivos.iter().any(|m| m.contains("menor id")));
}

// ---------------------------------------------------------------- AC-LUA-04

#[test]
fn ac_lua_04_mesma_entrada_mesmos_bytes_e_mesma_chave() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(
        &mut c,
        nova("comprovante-pix-banco-exemplo", def_comprovante()),
        None,
    );
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = entrada("doc-1", COMPROVANTE);
    let primeira = motor.ler(&e);
    for _ in 0..20 {
        let outra = motor.ler(&e);
        assert_eq!(
            serde_json::to_string(&sem_duracao(&primeira)).unwrap(),
            serde_json::to_string(&sem_duracao(&outra)).unwrap()
        );
        assert_eq!(reconhecido(&primeira).chave, reconhecido(&outra).chave);
    }
    // Entrada diferente, chave diferente.
    assert_ne!(
        reconhecido(&motor.ler(&entrada("doc-2", COMPROVANTE))).chave,
        reconhecido(&primeira).chave
    );
}

proptest! {
    #[test]
    fn ac_lua_04_proptest_leitura_estavel(texto in ".{0,200}", id in "[a-z0-9]{1,12}") {
        let mut c = Catalogo::em_memoria().unwrap();
        ativa(&mut c, nova("comprovante-pix-banco-exemplo", def_comprovante()), None);
        ativa(&mut c, nova("geral", Definicao::simples("geral", "banco")), None);
        let motor = Motor::das_vigentes(&c, None).unwrap();
        let texto = format!("{COMPROVANTE}{texto}");
        let e = entrada(&id, &texto);
        let a = serde_json::to_string(&sem_duracao(&motor.ler(&e))).unwrap();
        let b = serde_json::to_string(&sem_duracao(&motor.ler(&e))).unwrap();
        prop_assert_eq!(a, b);
    }
}

// ---------------------------------------------------------------- AC-LUA-07

#[test]
fn ac_lua_07_exemplo_falhando_bloqueia_a_ativacao() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c
        .salvar_rascunho(nova("comprovante-pix-banco-exemplo", def_comprovante()))
        .unwrap();
    let e = entrada("ex", COMPROVANTE);
    let errado = Esperado::Reconhece {
        tipo_documental: "comprovante-pix-banco-exemplo".into(),
        campos: BTreeMap::from([("valor_centavos".into(), Valor::Centavos(999))]),
    };
    c.salvar_exemplo(&v.regra, "comprovante de outubro", &e, errado)
        .unwrap();
    c.salvar_exemplo(
        &v.regra,
        "outro documento",
        &entrada("o", "Fatura de energia"),
        Esperado::NaoSeAplica,
    )
    .unwrap();
    let rel = c.testar(&v.regra, 1, None).unwrap();
    assert!(!rel.aprovado());
    assert_eq!(rel.falhas(), vec!["comprovante de outubro".to_owned()]);
    assert!(rel.resultados[0].detalhe.contains("valor_centavos"));
    let r = c.ativar(&v.regra, 1, &Quem::Usuario, None);
    assert_eq!(
        r,
        Err(ErroDasRegras::Exemplos(vec![
            "comprovante de outubro".into()
        ]))
    );
    assert_eq!(c.versao(&v.regra, 1).unwrap().estado, Estado::Rascunho);
    assert!(c.vigentes().unwrap().is_empty());

    // Corrige o exemplo: agora ativa.
    let certo = Esperado::Reconhece {
        tipo_documental: "comprovante-pix-banco-exemplo".into(),
        campos: BTreeMap::from([("valor_centavos".into(), Valor::Centavos(123_456))]),
    };
    c.salvar_exemplo(&v.regra, "comprovante de outubro", &e, certo)
        .unwrap();
    c.ativar(&v.regra, 1, &Quem::Usuario, None).unwrap();
    assert_eq!(c.versao(&v.regra, 1).unwrap().estado, Estado::Ativa);
}

#[test]
fn exemplo_que_nao_deveria_se_aplicar_mas_se_aplica_falha() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    c.salvar_exemplo(
        "r",
        "nao",
        &entrada("e", COMPROVANTE),
        Esperado::NaoSeAplica,
    )
    .unwrap();
    assert_eq!(
        c.ativar("r", v.versao, &Quem::Usuario, None),
        Err(ErroDasRegras::Exemplos(vec!["nao".into()]))
    );
}

#[test]
fn ac_lua_07_ia_e_sistema_nao_ativam_nem_consolidam_nem_revertem() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    for quem in [Quem::Ia, Quem::Sistema] {
        assert!(matches!(
            c.ativar("r", 1, &quem, None),
            Err(ErroDasRegras::NaoPermitido(_))
        ));
        assert!(matches!(
            c.desativar("r", &quem),
            Err(ErroDasRegras::NaoPermitido(_))
        ));
        assert!(matches!(
            c.reverter("r", &quem),
            Err(ErroDasRegras::NaoPermitido(_))
        ));
        assert!(matches!(
            c.consolidar("r", 1, &quem),
            Err(ErroDasRegras::NaoPermitido(_))
        ));
    }
    assert_eq!(c.versao("r", v.versao).unwrap().estado, Estado::Rascunho);
}

#[test]
fn ac_lua_07_rollback_preserva_todas_as_versoes() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let mut d2 = def_comprovante();
    d2.tipo_documental = "outro-tipo".into();
    ativa(&mut c, nova("r", d2), None);
    assert_eq!(c.versao("r", 1).unwrap().estado, Estado::Desativada);
    assert_eq!(c.versao("r", 2).unwrap().estado, Estado::Ativa);
    assert_eq!(c.vigentes().unwrap().len(), 1);

    c.reverter("r", &Quem::Usuario).unwrap();
    assert_eq!(c.versoes("r").unwrap().len(), 2, "nada é apagado");
    assert_eq!(c.versao("r", 1).unwrap().estado, Estado::Ativa);
    assert_eq!(c.versao("r", 2).unwrap().estado, Estado::Desativada);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    assert_eq!(
        reconhecido(&motor.ler(&entrada("x", COMPROVANTE))).tipo_documental,
        "comprovante-pix-banco-exemplo"
    );
    // O histórico guarda quem fez.
    let h = c.historico_de_estado("r", 1).unwrap();
    assert!(
        h.iter()
            .any(|(_, para, quem, _, _)| para == "ativa" && quem == "usuario")
    );

    c.desativar("r", &Quem::Usuario).unwrap();
    assert!(c.vigentes().unwrap().is_empty());
    assert!(matches!(
        c.desativar("r", &Quem::Usuario),
        Err(ErroDasRegras::NaoEncontrada(_))
    ));
}

#[test]
fn ac_lua_07_versao_e_imutavel_no_banco() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    assert!(
        c.sql_cru_para_testes("UPDATE versao_de_regra SET definicao = '{}'")
            .is_err()
    );
    assert!(
        c.sql_cru_para_testes("UPDATE versao_de_regra SET script = 'x'")
            .is_err()
    );
    assert!(
        c.sql_cru_para_testes("UPDATE versao_de_regra SET checksum = 'x'")
            .is_err()
    );
    assert!(
        c.sql_cru_para_testes("DELETE FROM versao_de_regra")
            .is_err()
    );
    // O estado, sim, muda.
    assert!(
        c.sql_cru_para_testes("UPDATE versao_de_regra SET aceitas_seguidas = 1")
            .is_ok()
    );
}

#[test]
fn salvar_o_mesmo_conteudo_nao_cria_versao_e_mudar_cria() {
    let mut c = Catalogo::em_memoria().unwrap();
    let a = c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    let b = c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    assert_eq!((a.versao, b.versao), (1, 1));
    assert_eq!(a.checksum.len(), 64);
    let mut d = def_comprovante();
    d.prioridade = 3;
    assert_eq!(c.salvar_rascunho(nova("r", d)).unwrap().versao, 2);
    assert!(matches!(
        c.salvar_rascunho(nova("Maiúscula!", def_comprovante())),
        Err(ErroDasRegras::Definicao(_))
    ));
    let mut ruim = def_comprovante();
    ruim.extrair
        .insert("x".into(), extracao("(", Formato::Texto));
    assert!(matches!(
        c.salvar_rascunho(nova("r2", ruim)),
        Err(ErroDasRegras::Definicao(_))
    ));
    assert_eq!(
        c.regras().unwrap().len(),
        1,
        "regra inválida não cria registro"
    );
}

// ---------------------------------------------------------------- consolidação e propostas

/// Ativa `r`, lê `n` documentos distintos e devolve as chaves das propostas.
fn propostas(c: &mut Catalogo, n: usize) -> Vec<String> {
    let motor = Motor::das_vigentes(c, None).unwrap();
    (0..n)
        .map(|i| {
            let e = entrada(&format!("doc-{i}"), COMPROVANTE);
            let l = motor.ler(&e);
            let r = reconhecido(&l);
            assert!(c.registrar_proposta(r, &e.id).unwrap());
            r.chave.clone()
        })
        .collect()
}

#[test]
fn cinco_aceitos_sugerem_consolidar() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let chaves = propostas(&mut c, 6);
    for (i, k) in chaves.iter().take(5).enumerate() {
        assert!(
            c.sugestoes_de_consolidacao().unwrap().is_empty(),
            "antes do {i}"
        );
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "ok")
            .unwrap();
    }
    assert_eq!(LIMIAR_DE_CONSOLIDACAO, 5);
    assert_eq!(
        c.sugestoes_de_consolidacao().unwrap(),
        vec![("r".to_owned(), 5)]
    );
    // Retorno repetido na mesma proposta não conta duas vezes.
    assert!(matches!(
        c.registrar_retorno(&chaves[0], Retorno::Aceito, &Quem::Usuario, ""),
        Err(ErroDasRegras::NaoPermitido(_))
    ));
    assert!(matches!(
        c.registrar_retorno(&chaves[5], Retorno::Aceito, &Quem::Ia, ""),
        Err(ErroDasRegras::NaoPermitido(_))
    ));
    assert!(matches!(
        c.consolidar("r", 1, &Quem::Ia),
        Err(ErroDasRegras::NaoPermitido(_))
    ));
    c.consolidar("r", 1, &Quem::Usuario).unwrap();
    assert_eq!(c.versao("r", 1).unwrap().estado, Estado::Consolidada);
    assert!(c.sugestoes_de_consolidacao().unwrap().is_empty());
    let resumo = &c.regras().unwrap()[0];
    assert_eq!(resumo.estado, Some(Estado::Consolidada));
    assert_eq!(resumo.versao_em_vigor, Some(1));
    assert_eq!(resumo.retornos_aceitos, 5);
    assert_eq!(resumo.taxa_de_acerto(), Some(1.0));

    // Desfeito rebaixa a consolidada (o sistema pode pedir) e zera a sequência.
    c.registrar_retorno(
        &chaves[0],
        Retorno::Desfeito,
        &Quem::Sistema,
        "usuário desfez",
    )
    .unwrap();
    let v = c.versao("r", 1).unwrap();
    assert_eq!((v.estado, v.aceitas_seguidas), (Estado::Ativa, 0));
    assert_eq!(
        c.propostas(Some(EstadoDaProposta::Desfeita)).unwrap().len(),
        1
    );
}

#[test]
fn corrigido_e_rejeitado_zeram_a_sequencia() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let chaves = propostas(&mut c, 7);
    for k in &chaves[..4] {
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "")
            .unwrap();
    }
    assert_eq!(c.versao("r", 1).unwrap().aceitas_seguidas, 4);
    c.registrar_retorno(
        &chaves[4],
        Retorno::Corrigido,
        &Quem::Usuario,
        "mudou o valor",
    )
    .unwrap();
    assert_eq!(c.versao("r", 1).unwrap().aceitas_seguidas, 0);
    for k in &chaves[5..] {
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "")
            .unwrap();
    }
    assert!(c.sugestoes_de_consolidacao().unwrap().is_empty());
    assert!(matches!(
        c.consolidar("r", 1, &Quem::Usuario),
        Err(ErroDasRegras::NaoPermitido(_))
    ));
    let rs = &c.regras().unwrap()[0];
    assert_eq!((rs.retornos_aceitos, rs.retornos_total), (6, 7));
}

#[test]
fn proposta_e_idempotente_pela_chave_e_execucao_tambem() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = entrada("doc-1", COMPROVANTE);
    let l1 = motor.ler(&e);
    let l2 = motor.ler(&e);
    assert!(c.registrar_proposta(reconhecido(&l1), &e.id).unwrap());
    assert!(!c.registrar_proposta(reconhecido(&l2), &e.id).unwrap());
    assert_eq!(c.propostas(None).unwrap().len(), 1);
    c.registrar_execucao(&l1).unwrap();
    c.registrar_execucao(&l2).unwrap();
    assert_eq!(c.total_de_execucoes().unwrap(), 1);
    assert_eq!(c.regras().unwrap()[0].execucoes, 1);
    assert!(matches!(
        c.registrar_retorno("inexistente", Retorno::Aceito, &Quem::Usuario, ""),
        Err(ErroDasRegras::NaoEncontrada(_))
    ));
}

// ---------------------------------------------------------------- scripts e parâmetros

fn resposta(campos: serde_json::Value) -> RespostaDoScript {
    RespostaDoScript {
        corresponde: true,
        tipo_documental: None,
        campos: serde_json::from_value(campos).unwrap(),
        revisar: false,
        motivos: vec![],
        evidencias: vec!["rubrica_irrf".into()],
        exibir: vec![],
    }
}

fn def_com_script() -> Definicao {
    let mut d = def_comprovante();
    d.parametros = vec!["irrf".into()];
    d
}

#[test]
fn script_recebe_parametros_da_competencia_e_vira_campo_rastreavel() {
    let exec = Arc::new(ExecutorFicticio::novo().com_funcao("fonte-1", |p| {
        assert!(p.entrada.get("texto").is_none(), "script_le_texto é falso");
        assert_eq!(
            p.entrada["tipo_documental"],
            "comprovante-pix-banco-exemplo"
        );
        assert_eq!(p.entrada["canal"], "arrastado");
        assert_eq!(p.entrada["campos"]["valor_centavos"], 123_456);
        assert!(
            p.entrada["campos"]["data"]
                .as_str()
                .unwrap()
                .starts_with("202")
        );
        let faixa = p.parametros["irrf"]["faixa"].as_i64().unwrap();
        Ok(resposta(
            json!({"irrf_estimado_centavos": faixa * 2, "vencimento": "2026-11-10", "nota": "ok"}),
        ))
    }));
    let mut c = Catalogo::em_memoria().unwrap();
    c.salvar_parametro(
        "irrf",
        "2025-01",
        Some("2025-12"),
        json!({"faixa": 1}),
        "fonte oficial 2025",
    )
    .unwrap();
    let p2 = c
        .salvar_parametro(
            "irrf",
            "2026-01",
            None,
            json!({"faixa": 100}),
            "fonte oficial 2026",
        )
        .unwrap();
    assert_eq!(p2.versao, 2);
    let mut n = nova("r", def_com_script());
    n.script = Some("fonte-1".into());
    // Sem executor, não ativa.
    let v = c.salvar_rascunho(n).unwrap();
    assert!(matches!(
        c.ativar("r", 1, &Quem::Usuario, None),
        Err(ErroDasRegras::Script(_))
    ));
    c.ativar("r", v.versao, &Quem::Usuario, Some(&*exec))
        .unwrap();

    let motor = Motor::das_vigentes(&c, Some(exec.clone())).unwrap();
    let e = entrada("doc-1", COMPROVANTE);
    let l = motor.ler(&e);
    let r = reconhecido(&l);
    assert_eq!(
        r.campos["irrf_estimado_centavos"].valor,
        Valor::Centavos(200)
    );
    assert_eq!(
        r.campos["vencimento"].valor,
        Valor::Data("2026-11-10".into())
    );
    assert_eq!(r.campos["nota"].valor, Valor::Texto("ok".into()));
    match &r.campos["irrf_estimado_centavos"].de_onde {
        DeOnde::Script { a_partir_de } => {
            assert!(a_partir_de.contains(&"valor_centavos".to_owned()))
        }
        outra => panic!("{outra:?}"),
    }
    assert!(r.evidencias.contains(&"tabela:irrf:2".to_owned()));
    assert!(r.evidencias.contains(&"rubrica_irrf".to_owned()));
    // A chave inclui o checksum do parâmetro usado: parâmetro novo, chave nova.
    c.salvar_parametro("irrf", "2026-01", None, json!({"faixa": 200}), "retificada")
        .unwrap();
    let motor3 = Motor::das_vigentes(&c, Some(exec.clone())).unwrap();
    let l3 = motor3.ler(&e);
    assert_ne!(reconhecido(&l3).chave, r.chave);
    assert_eq!(
        reconhecido(&l3).campos["irrf_estimado_centavos"].valor,
        Valor::Centavos(400)
    );
    // Competência de 2025 pega a tabela de 2025.
    let e25 = entrada("doc-25", &COMPROVANTE.replace("06/10/2026", "06/10/2025"));
    assert_eq!(
        reconhecido(&motor3.ler(&e25)).campos["irrf_estimado_centavos"].valor,
        Valor::Centavos(2)
    );
    // O parâmetro é imutável.
    assert!(
        c.sql_cru_para_testes("UPDATE parametro SET valor = '{}'")
            .is_err()
    );
    assert!(c.sql_cru_para_testes("DELETE FROM parametro").is_err());
}

#[test]
fn sem_competencia_ou_sem_parametro_o_script_nao_roda_e_manda_revisar() {
    let exec = Arc::new(ExecutorFicticio::novo().com("fonte-1", Ok(resposta(json!({})))));
    let mut c = Catalogo::em_memoria().unwrap();
    let mut n = nova("r", def_com_script());
    n.script = Some("fonte-1".into());
    ativa(&mut c, n, Some(&*exec));
    let motor = Motor::das_vigentes(&c, Some(exec.clone())).unwrap();
    // Sem data em lugar nenhum.
    let l = motor.ler(&entrada(
        "a",
        "Banco Exemplo\nComprovante PIX\nValor: R$ 1,00\n",
    ));
    let r = reconhecido(&l);
    assert!(r.revisar);
    assert!(r.motivos.iter().any(|m| m.contains("competência")));
    // Com data mas sem parâmetro cadastrado.
    let l = motor.ler(&entrada("b", COMPROVANTE));
    assert!(reconhecido(&l).revisar);
    assert!(reconhecido(&l).motivos.iter().any(|m| m.contains("irrf")));
    assert!(exec.pedidos().is_empty());
    // A competência também sai de dicas.data e de momento.
    c.salvar_parametro("irrf", "2026-01", None, json!(1), "x")
        .unwrap();
    let motor = Motor::das_vigentes(&c, Some(exec.clone())).unwrap();
    let mut e = entrada("c", "Banco Exemplo\nComprovante PIX\nValor: R$ 1,00\n");
    e.momento = Some("2026-03-05T10:00:00-03:00".into());
    assert!(!reconhecido(&motor.ler(&e)).revisar);
    assert_eq!(exec.pedidos().len(), 1);
}

#[test]
fn script_errado_nao_trava_a_fila_nem_as_outras_regras() {
    let exec = Arc::new(
        ExecutorFicticio::novo()
            .com("quebra", Err(ErroDoScript::Tempo))
            .com("float", Ok(resposta(json!({"x_centavos": 1.5}))))
            .com("bool", Ok(resposta(json!({"x": [1]})))),
    );
    let mut c = Catalogo::em_memoria().unwrap();
    for (id, fonte) in [
        ("a-quebra", "quebra"),
        ("b-float", "float"),
        ("c-forma", "bool"),
    ] {
        let mut n = nova(id, Definicao::simples(id, "comprovante pix"));
        n.definicao.prioridade = 5;
        n.script = Some(fonte.into());
        ativa(&mut c, n, Some(&*exec));
    }
    ativa(
        &mut c,
        nova("z-boa", Definicao::simples("z-boa", "comprovante pix")),
        None,
    );
    let motor = Motor::das_vigentes(&c, Some(exec)).unwrap();
    let l = motor.ler(&entrada("x", COMPROVANTE));
    assert_eq!(reconhecido(&l).regra, "z-boa");
    let erros: Vec<_> = l
        .execucoes
        .iter()
        .filter(|e| e.resultado == ResultadoDaExecucao::Erro)
        .collect();
    assert_eq!(erros.len(), 3);
    assert!(
        erros[0].motivo.as_deref() == Some("tempo")
            && erros[0]
                .detalhe
                .as_deref()
                .unwrap()
                .contains("passou do tempo")
    );
    assert_eq!(erros[1].motivo.as_deref(), Some("retorno"));
    assert!(erros[1].detalhe.as_deref().unwrap().contains("inteiro"));
    // As execuções com erro ficam registradas.
    c.registrar_execucao(&l).unwrap();
    assert_eq!(c.total_de_execucoes().unwrap(), 4);
}

#[test]
fn script_que_nao_corresponde_nao_se_aplica_e_texto_so_se_pedido() {
    let exec = Arc::new(ExecutorFicticio::novo().com_funcao("f", |p| {
        assert!(
            p.entrada["texto"]
                .as_str()
                .unwrap()
                .contains("Comprovante PIX")
        );
        Ok(RespostaDoScript {
            corresponde: false,
            ..resposta(json!({}))
        })
    }));
    let mut c = Catalogo::em_memoria().unwrap();
    let mut n = nova("r", Definicao::simples("t", "comprovante pix"));
    n.definicao.script_le_texto = true;
    n.script = Some("f".into());
    ativa(&mut c, n, Some(&*exec));
    let motor = Motor::das_vigentes(&c, Some(exec)).unwrap();
    let l = motor.ler(&entrada("x", COMPROVANTE));
    assert_eq!(l.decisao, Decisao::Nenhuma);
    assert_eq!(l.execucoes[0].resultado, ResultadoDaExecucao::NaoSeAplica);
}

fn resposta_com_exibir(linhas: &[&str]) -> RespostaDoScript {
    RespostaDoScript {
        exibir: linhas.iter().map(|l| (*l).to_owned()).collect(),
        ..resposta(json!({}))
    }
}

/// Uma regra de script (sem parâmetros) com o executor de mentira dado.
fn motor_do_script(exec: &Arc<ExecutorFicticio>, leitor: bool) -> Motor {
    let mut c = Catalogo::em_memoria().unwrap();
    let mut n = nova("r", Definicao::simples("t", "comprovante pix"));
    n.definicao.leitor = leitor.then(|| "contracheque".to_owned());
    n.script = Some("f".into());
    ativa(&mut c, n, Some(&**exec));
    Motor::das_vigentes(&c, Some(exec.clone())).unwrap()
}

#[test]
fn exibir_do_script_chega_ao_reconhecimento_sem_mexer_na_chave() {
    let com = |linhas: &'static [&'static str]| {
        Arc::new(ExecutorFicticio::novo().com_funcao("f", move |_| Ok(resposta_com_exibir(linhas))))
    };
    let e = entrada("doc-1", COMPROVANTE);
    let l1 = motor_do_script(&com(&["IRRF conferido ✓", "segunda"]), false).ler(&e);
    let l2 = motor_do_script(&com(&["outra coisa"]), false).ler(&e);
    let l3 = motor_do_script(&com(&[]), false).ler(&e);
    assert_eq!(reconhecido(&l1).exibir, ["IRRF conferido ✓", "segunda"]);
    assert_eq!(reconhecido(&l2).exibir, ["outra coisa"]);
    assert!(reconhecido(&l3).exibir.is_empty());
    // O texto exibido não entra na chave de idempotência.
    assert_eq!(reconhecido(&l1).chave, reconhecido(&l2).chave);
    assert_eq!(reconhecido(&l1).chave, reconhecido(&l3).chave);
}

#[test]
fn exibir_tambem_chega_quando_o_leitor_completa() {
    let exec = Arc::new(
        ExecutorFicticio::novo().com_funcao("f", |_| Ok(resposta_com_exibir(&["linha do script"]))),
    );
    let motor = motor_do_script(&exec, true);
    let e = entrada("doc-1", COMPROVANTE);
    let l = motor.ler(&e);
    let antes = reconhecido(&l).clone();
    assert!(antes.leitor.is_some() && antes.exibir.is_empty());
    let pronto = motor.completar(antes.clone(), BTreeMap::new(), &e);
    assert_eq!(pronto.exibir, ["linha do script"]);
    assert_eq!(pronto.chave, antes.chave, "exibir não mexe na chave");
}

#[test]
fn exibir_nao_vai_para_a_tabela_de_execucao_nem_para_a_proposta() {
    let exec = Arc::new(
        ExecutorFicticio::novo()
            .com_funcao("f", |_| Ok(resposta_com_exibir(&["texto-do-documento"]))),
    );
    let mut c = Catalogo::em_memoria().unwrap();
    let mut n = nova("r", def_comprovante());
    n.script = Some("f".into());
    ativa(&mut c, n, Some(&*exec));
    let motor = Motor::das_vigentes(&c, Some(exec.clone())).unwrap();
    let l = motor.ler(&entrada("doc-1", COMPROVANTE));
    assert_eq!(reconhecido(&l).exibir, ["texto-do-documento"]);
    c.registrar_execucao(&l).unwrap();
    assert!(reconhecido(&l).propostas.len() == 1);
    assert!(c.registrar_proposta(reconhecido(&l), "doc-1").unwrap());
    let gravada = c.propostas(None).unwrap().remove(0);
    assert!(
        gravada.conteudo.exibir.is_empty(),
        "a proposta gravada não guarda as linhas de exibir"
    );
    assert_eq!(gravada.conteudo.campos, reconhecido(&l).campos);
}

#[test]
fn reconhecimento_antigo_sem_exibir_ainda_le() {
    let exec = Arc::new(ExecutorFicticio::novo().com_funcao("f", |_| Ok(resposta(json!({})))));
    let l = motor_do_script(&exec, false).ler(&entrada("doc-1", COMPROVANTE));
    let mut json = serde_json::to_value(reconhecido(&l)).unwrap();
    json.as_object_mut().unwrap().remove("exibir");
    let lido: sbf_regras::Reconhecimento = serde_json::from_value(json).unwrap();
    assert!(lido.exibir.is_empty());
}

#[test]
fn parametros_vigentes_por_competencia() {
    let mut c = Catalogo::em_memoria().unwrap();
    c.salvar_parametro("irrf", "2025-01", Some("2025-12"), json!(1), "a")
        .unwrap();
    c.salvar_parametro("irrf", "2026-01", None, json!(2), "b")
        .unwrap();
    c.salvar_parametro("inss", "2026-01", None, json!(3), "c")
        .unwrap();
    let v26 = c.parametros_vigentes("2026-05").unwrap();
    assert_eq!(
        v26.iter()
            .map(|p| (p.nome.as_str(), p.versao))
            .collect::<Vec<_>>(),
        vec![("inss", 1), ("irrf", 2)]
    );
    let v25 = c.parametros_vigentes("2025-06").unwrap();
    assert_eq!(v25.len(), 1);
    assert_eq!(v25[0].valor, json!(1));
    assert!(c.parametros_vigentes("2024-12").unwrap().is_empty());
    assert!(
        c.salvar_parametro("x", "2026-13", None, json!(1), "")
            .is_err()
    );
    assert!(
        c.salvar_parametro("x", "2026-05", Some("2026-01"), json!(1), "")
            .is_err()
    );
}

// ---------------------------------------------------------------- leitor e fábrica

#[test]
fn fabrica_e_idempotente_e_o_contracheque_pede_o_leitor() {
    let mut c = Catalogo::em_memoria().unwrap();
    c.semear_fabrica().unwrap();
    c.semear_fabrica().unwrap();
    let regras = c.regras().unwrap();
    // Contracheque e pix-na-descricao, mais as 16 da biblioteca.
    assert_eq!(regras.len(), 18);
    assert_eq!(
        c.versoes("contracheque").unwrap().len(),
        2,
        "a v1 ativa e a v2 (IRRF) rascunho"
    );
    assert_eq!(
        c.versao("contracheque", 2).unwrap().estado,
        Estado::Rascunho
    );
    assert_eq!(
        c.parametros().unwrap().len(),
        1,
        "a tabela do IRRF, uma vez só"
    );
    assert_eq!(c.versoes("pix-na-descricao").unwrap().len(), 1);
    let vigentes = c.vigentes().unwrap();
    assert_eq!(vigentes.len(), 1, "só o contracheque é ativo");
    assert_eq!(vigentes[0].autor, Autor::Fabrica);
    let h = c.historico_de_estado("contracheque", 1).unwrap();
    assert!(
        h.iter()
            .any(|(_, para, quem, _, _)| para == "ativa" && quem == "sistema")
    );

    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = entrada(
        "cc",
        "Fulano de Tal\nDEMONSTRATIVO de Pagamento\nLíquido 1.000,00\n",
    );
    let l = motor.ler(&e);
    let r = reconhecido(&l).clone();
    assert_eq!(r.tipo_documental, "contracheque");
    assert_eq!(r.leitor.as_deref(), Some("contracheque"));
    let campos = BTreeMap::from([(
        "liquido_centavos".to_owned(),
        Campo {
            valor: Valor::Centavos(100_000),
            de_onde: DeOnde::Leitor {
                leitor: "contracheque".into(),
            },
        },
    )]);
    let pronto = motor.completar(r, campos, &e);
    assert_eq!(pronto.leitor, None);
    assert_eq!(
        pronto.campos["liquido_centavos"].valor,
        Valor::Centavos(100_000)
    );
    assert!(!pronto.revisar);
    // Texto sem o título não é contracheque.
    assert_eq!(
        motor.ler(&entrada("x", "Recibo qualquer")).decisao,
        Decisao::Nenhuma
    );
}

#[test]
fn script_novo_de_fabrica_vira_versao_nova_em_rascunho_sem_mexer_na_ativa() {
    // Um catálogo semeado por um sbf antigo: a v2 (IRRF) tem o script de antes do `exibir`,
    // e o usuário já a ativou.
    let antigo = ExecutorFicticio::novo().com(
        "-- script antigo\nfunction transformar() end",
        Ok(resposta(json!({}))),
    );
    let mut c = Catalogo::em_memoria().unwrap();
    for (i, (mut nova, estado)) in sbf_regras::fabrica::regras_de_fabrica()
        .into_iter()
        .enumerate()
    {
        if nova.regra == "contracheque" && nova.script.is_some() {
            nova.script = Some("-- script antigo\nfunction transformar() end".to_owned());
        }
        let v = c.salvar_rascunho(nova).unwrap();
        if estado == Estado::Ativa || i == 1 {
            c.ativar(&v.regra, v.versao, &Quem::Usuario, Some(&antigo))
                .unwrap();
        }
    }
    assert_eq!(c.versoes("contracheque").unwrap().len(), 2);
    assert_eq!(c.versao("contracheque", 2).unwrap().estado, Estado::Ativa);

    c.semear_fabrica().unwrap();
    c.semear_fabrica().unwrap();

    let versoes = c.versoes("contracheque").unwrap();
    assert_eq!(versoes.len(), 3, "o script novo é a v3, uma vez só");
    let v3 = c.versao("contracheque", 3).unwrap();
    assert_eq!(v3.estado, Estado::Rascunho, "quem ativa a v3 é o usuário");
    assert_eq!(v3.script.as_deref(), Some(sbf_regras::fabrica::SCRIPT_IRRF));
    assert!(
        sbf_regras::fabrica::SCRIPT_IRRF.contains("exibir"),
        "o script de fábrica devolve exibir"
    );
    assert_eq!(c.versao("contracheque", 2).unwrap().estado, Estado::Ativa);
}

#[test]
fn fabrica_nao_reativa_o_que_o_usuario_desativou() {
    let mut c = Catalogo::em_memoria().unwrap();
    c.semear_fabrica().unwrap();
    c.desativar("contracheque", &Quem::Usuario).unwrap();
    c.semear_fabrica().unwrap();
    assert!(c.vigentes().unwrap().is_empty());
}

#[test]
fn pix_na_descricao_le_favorecido_e_tipo_do_registro() {
    let mut c = Catalogo::em_memoria().unwrap();
    c.semear_fabrica().unwrap();
    let rascunho = c.versao("pix-na-descricao", 1).unwrap();
    assert_eq!(rascunho.estado, Estado::Rascunho);
    let motor = Motor::novo(vec![rascunho], vec![], None);
    let mut e = entrada("pluggy:1", "");
    e.tipo = TipoDeEntrada::Registro;
    e.texto = None;
    e.mime = None;
    e.nome = None;
    e.origem.canal = Canal::Pluggy;
    e.dicas.descricao = Some("PIX ENVIADO FULANO DE TAL".into());
    let l = motor.ler(&e);
    let r = reconhecido(&l);
    assert_eq!(
        r.campos["favorecido"].valor,
        Valor::Texto("FULANO DE TAL".into())
    );
    assert_eq!(
        r.campos["tipo_de_transferencia"].valor,
        Valor::Texto("pix".into())
    );
    assert_eq!(
        r.campos["favorecido"].de_onde,
        DeOnde::Dica {
            nome: "descricao".into()
        }
    );
    assert_eq!(r.estado, Estado::Rascunho);
    e.dicas.descricao = Some("COMPRA NO CARTAO BANCO EXEMPLO".into());
    assert_eq!(motor.ler(&e).decisao, Decisao::Nenhuma);
    // Um documento não é registro: a regra nem é avaliada.
    assert!(motor.ler(&entrada("d", COMPROVANTE)).execucoes.is_empty());
}

// ---------------------------------------------------------------- simulação

#[test]
fn simulacao_mostra_o_que_mudaria_sem_gravar() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let atual = Motor::das_vigentes(&c, None).unwrap();
    let mut d2 = def_comprovante();
    d2.extrair.remove("autenticacao");
    let candidato_v = VersaoDeRegra::avulsa(nova("r", d2), 2, Estado::Rascunho).unwrap();
    let candidato = Motor::novo(vec![candidato_v], vec![], None);
    let entradas = [
        entrada("com", COMPROVANTE),
        entrada("outro", "Fatura de energia"),
    ];
    let d = simular(&atual, &candidato, &entradas);
    assert_eq!(d.iguais, 1);
    assert_eq!(d.mudaram.len(), 1);
    assert_eq!(d.mudaram[0].entrada, "com");
    match (&d.mudaram[0].antes, &d.mudaram[0].depois) {
        (
            ResumoDaDecisao::Reconhecido { campos: a, .. },
            ResumoDaDecisao::Reconhecido { campos: b, .. },
        ) => {
            assert!(a.contains_key("autenticacao") && !b.contains_key("autenticacao"));
        }
        outro => panic!("{outro:?}"),
    }
    // Só leitura: nada foi gravado.
    assert_eq!(c.total_de_execucoes().unwrap(), 0);
    assert_eq!(c.versoes("r").unwrap().len(), 1);
    // O resumo não carrega texto do documento.
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("Fulano"));
}

// ---------------------------------------------------------------- rascunho da correção

#[test]
fn rascunho_da_correcao_reconhece_o_proprio_documento_e_outro_do_mesmo_layout() {
    let texto = "Banco Exemplo\nComprovante de Transferência\nFavorecido: Fulano de Tal\n\
Valor: R$ 1.234,56\nData: 06/10/2026\nAutenticação: 9F8E7D6C5B4A\n";
    let e = entrada("doc-corrigido", texto);
    let correcao = Correcao {
        regra: None,
        nome: "Comprovante de Transferência Banco Exemplo".into(),
        tipo_documental: "comprovante-transferencia-banco-exemplo".into(),
        campos: BTreeMap::from([
            ("valor_centavos".to_owned(), Valor::Centavos(123_456)),
            ("data".to_owned(), Valor::Data("2026-10-06".into())),
            (
                "favorecido".to_owned(),
                Valor::Texto("Fulano de Tal".into()),
            ),
            (
                "fantasma".to_owned(),
                Valor::Texto("não existe no texto".into()),
            ),
        ]),
    };
    let (nova, esperado) = rascunho_da_correcao(&e, &correcao).unwrap();
    assert_eq!(nova.autor, Autor::Correcao);
    assert_eq!(nova.regra, "comprovante-de-transferencia-banco-exemplo");
    assert!(nova.motivo.contains("fantasma"));
    assert!(!nova.definicao.extrair.contains_key("fantasma"));
    assert!(nova.definicao.detectar.todos.len() >= 2 && nova.definicao.detectar.todos.len() <= 3);
    let Esperado::Reconhece {
        tipo_documental,
        campos,
    } = &esperado
    else {
        panic!("{esperado:?}")
    };
    assert_eq!(tipo_documental, "comprovante-transferencia-banco-exemplo");
    assert_eq!(campos.len(), 3);

    // Entra como exemplo e a ativação passa.
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c.salvar_rascunho(nova).unwrap();
    c.salvar_exemplo(&v.regra, "documento corrigido", &e, esperado.clone())
        .unwrap();
    assert!(c.testar(&v.regra, v.versao, None).unwrap().aprovado());
    c.ativar(&v.regra, v.versao, &Quem::Usuario, None).unwrap();
    let motor = Motor::das_vigentes(&c, None).unwrap();

    let l = motor.ler(&e);
    let r = reconhecido(&l);
    for (nome, valor) in campos {
        assert_eq!(&r.campos[nome].valor, valor, "{nome}");
    }
    // Outro documento do mesmo layout, valores diferentes.
    let outro = entrada(
        "outro",
        "Banco Exemplo\nComprovante de Transferência\nFavorecido: Beltrana da Silva\n\
Valor: R$ 50,00\nData: 01/11/2026\nAutenticação: 111A222B333C\n",
    );
    let r2 = motor.ler(&outro);
    let r2 = reconhecido(&r2);
    assert_eq!(r2.campos["valor_centavos"].valor, Valor::Centavos(5000));
    assert_eq!(r2.campos["data"].valor, Valor::Data("2026-11-01".into()));
    assert_eq!(
        r2.campos["favorecido"].valor,
        Valor::Texto("Beltrana da Silva".into())
    );
    // E um documento de outro banco não casa.
    assert_eq!(
        motor
            .ler(&entrada("x", "Banco Outro\nRecibo\nValor: R$ 1,00\n"))
            .decisao,
        Decisao::Nenhuma
    );
}

#[test]
fn rascunho_sem_texto_ou_sem_ancora_e_erro() {
    let mut e = entrada("a", "x");
    e.texto = None;
    let c = Correcao {
        regra: Some("r".into()),
        nome: "n".into(),
        tipo_documental: "t".into(),
        campos: BTreeMap::new(),
    };
    assert!(matches!(
        rascunho_da_correcao(&e, &c),
        Err(ErroDasRegras::Definicao(_))
    ));
    e.texto = Some("12345678\nab\n".into());
    assert!(matches!(
        rascunho_da_correcao(&e, &c),
        Err(ErroDasRegras::Definicao(_))
    ));
}

// ---------------------------------------------------------------- exemplos

#[test]
fn exemplos_guardam_historico() {
    let mut c = Catalogo::em_memoria().unwrap();
    c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    let e = entrada("e", COMPROVANTE);
    c.salvar_exemplo("r", "um", &e, Esperado::NaoSeAplica)
        .unwrap();
    c.salvar_exemplo(
        "r",
        "um",
        &e,
        Esperado::Reconhece {
            tipo_documental: "t".into(),
            campos: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(c.exemplos("r").unwrap().len(), 1);
    c.apagar_exemplo("r", "um").unwrap();
    assert!(c.exemplos("r").unwrap().is_empty());
    assert_eq!(c.total_do_historico_de_exemplos("r").unwrap(), 3);
    assert!(matches!(
        c.apagar_exemplo("r", "um"),
        Err(ErroDasRegras::NaoEncontrada(_))
    ));
    assert!(matches!(
        c.salvar_exemplo("nao-existe", "x", &e, Esperado::NaoSeAplica),
        Err(ErroDasRegras::NaoEncontrada(_))
    ));
}

#[test]
fn definicao_json_do_catalogo_ida_e_volta() {
    let d = def_comprovante();
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(serde_json::from_str::<Definicao>(&json).unwrap(), d);
}

// ---------------------------------------------------------------- D4 e execuções sem texto

/// Consolida a regra "r" (5 aceitos) e devolve as chaves das propostas (a sexta, aberta).
fn consolidada_com_proposta_aberta(c: &mut Catalogo) -> Vec<String> {
    ativa(c, nova("r", def_comprovante()), None);
    let chaves = propostas(c, 6);
    for k in chaves.iter().take(5) {
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "ok")
            .unwrap();
    }
    c.consolidar("r", 1, &Quem::Usuario).unwrap();
    chaves
}

#[test]
fn rejeitado_e_corrigido_devolvem_a_consolidada_a_ativa() {
    for (retorno, motivo) in [
        (Retorno::Rejeitado, "rejeitada pelo usuário"),
        (Retorno::Corrigido, "corrigida pelo usuário"),
    ] {
        let mut c = Catalogo::em_memoria().unwrap();
        let chaves = consolidada_com_proposta_aberta(&mut c);
        assert_eq!(c.versao("r", 1).unwrap().estado, Estado::Consolidada);
        c.registrar_retorno(&chaves[5], retorno, &Quem::Usuario, "")
            .unwrap();
        let v = c.versao("r", 1).unwrap();
        assert_eq!((v.estado, v.aceitas_seguidas), (Estado::Ativa, 0));
        let h = c.historico_de_estado("r", 1).unwrap();
        assert!(
            h.iter().any(|l| l.3 == motivo),
            "sem o motivo {motivo}: {h:?}"
        );
    }
}

#[test]
fn rejeitado_numa_ativa_so_zera_a_sequencia() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let chaves = propostas(&mut c, 3);
    for k in &chaves[..2] {
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "")
            .unwrap();
    }
    c.registrar_retorno(&chaves[2], Retorno::Rejeitado, &Quem::Usuario, "")
        .unwrap();
    let v = c.versao("r", 1).unwrap();
    assert_eq!((v.estado, v.aceitas_seguidas), (Estado::Ativa, 0));
}

#[test]
fn erro_de_script_grava_so_a_categoria_nunca_o_texto_do_documento() {
    const MARCADOR: &str = "SEGREDO-FICTICIO-123";
    let exec = Arc::new(ExecutorFicticio::novo().com_funcao("f", |p| {
        Err(ErroDoScript::Execucao(
            p.entrada["texto"].as_str().unwrap_or_default().to_owned(),
        ))
    }));
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("regras.sqlite");
    let mut c = Catalogo::abrir(&arquivo).unwrap();
    let mut n = nova("r", Definicao::simples("t", "comprovante pix"));
    n.definicao.script_le_texto = true;
    n.script = Some("f".into());
    ativa(&mut c, n, Some(&*exec));
    let motor = Motor::das_vigentes(&c, Some(exec)).unwrap();
    let l = motor.ler(&entrada("x", &format!("Comprovante PIX {MARCADOR}")));
    assert_eq!(l.execucoes.len(), 1);
    assert_eq!(l.execucoes[0].motivo.as_deref(), Some("execucao"));
    // A mensagem completa só existe em memória, para o editor.
    assert!(
        l.execucoes[0]
            .detalhe
            .as_deref()
            .unwrap()
            .contains(MARCADOR)
    );
    c.registrar_execucao(&l).unwrap();

    let conn = rusqlite::Connection::open(&arquivo).unwrap();
    let mut st = conn.prepare("SELECT motivo FROM execucao").unwrap();
    let motivos: Vec<Option<String>> = st
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(motivos, vec![Some("execucao".to_owned())]);
    assert!(!motivos.iter().flatten().any(|m| m.contains(MARCADOR)));
}

// ---------------------------------------------------------------- favorecido de um Pix (D4, D13)

const DOC_FICTICIO: &str = "000.000.000-00";

fn registro_pix(id: &str, nome: Option<&str>, doc: Option<&str>) -> Entrada {
    Entrada {
        versao_do_esquema: 1,
        id: id.to_owned(),
        tipo: TipoDeEntrada::Registro,
        origem: Origem {
            canal: Canal::Pluggy,
            id_externo: Some(id.to_owned()),
        },
        sha256: None,
        mime: None,
        nome: None,
        texto: None,
        dicas: Dicas {
            conta: Some("Banco Exemplo 000000-1".to_owned()),
            valor_centavos: Some(-15_000),
            data: Some("2026-11-12".to_owned()),
            descricao: Some("PIX ENVIADO".to_owned()),
            contraparte: nome.map(str::to_owned),
            contraparte_doc: doc.map(str::to_owned),
        },
        momento: Some("2026-11-12".to_owned()),
    }
}

fn def_do_favorecido(condicao: Condicao) -> Definicao {
    let mut d = Definicao::simples("pix-do-favorecido", "x");
    d.aplica_a = vec![TipoDeEntrada::Registro];
    d.detectar.todos = vec![condicao];
    d
}

fn casa(def: &Definicao, e: &Entrada) -> bool {
    let motor = Motor::novo(
        vec![VersaoDeRegra::avulsa(nova("r", def.clone()), 1, Estado::Ativa).unwrap()],
        Vec::new(),
        None,
    );
    matches!(motor.ler(e).decisao, Decisao::Reconhecido(_))
}

#[test]
fn condicao_por_documento_ignora_a_pontuacao_dos_dois_lados() {
    let formatada = def_do_favorecido(Condicao::ContraparteDoc(vec![DOC_FICTICIO.into()]));
    let seca = def_do_favorecido(Condicao::ContraparteDoc(vec!["12345678909".into()]));
    // A regra formatada casa o registro com e sem pontuação.
    assert!(casa(
        &formatada,
        &registro_pix("pluggy:a", None, Some(DOC_FICTICIO))
    ));
    assert!(casa(
        &formatada,
        &registro_pix("pluggy:b", None, Some("00000000000"))
    ));
    // A regra só com dígitos casa o registro formatado.
    assert!(casa(
        &seca,
        &registro_pix("pluggy:c", None, Some("123.456.789-09"))
    ));
    assert!(casa(
        &seca,
        &registro_pix("pluggy:d", None, Some("12345678909"))
    ));
    // Outro documento, ou nenhum, não casa.
    assert!(!casa(
        &seca,
        &registro_pix("pluggy:e", None, Some("987.654.321-00"))
    ));
    assert!(!casa(
        &seca,
        &registro_pix("pluggy:f", Some("Fulana de Tal"), None)
    ));
}

#[test]
fn condicao_por_documento_casa_se_for_igual_a_algum_da_lista() {
    let d = def_do_favorecido(Condicao::ContraparteDoc(vec![
        "111.111.111-11".into(),
        DOC_FICTICIO.into(),
    ]));
    assert!(casa(
        &d,
        &registro_pix("pluggy:a", None, Some("00000000000"))
    ));
    assert!(casa(
        &d,
        &registro_pix("pluggy:b", None, Some("11111111111"))
    ));
    assert!(!casa(
        &d,
        &registro_pix("pluggy:c", None, Some("22222222222"))
    ));
}

#[test]
fn condicao_por_nome_usa_regex_sobre_o_nome_normalizado() {
    let d = def_do_favorecido(Condicao::Contraparte("^fulana de tal$".into()));
    assert!(casa(
        &d,
        &registro_pix("pluggy:a", Some("FULANA DE TAL"), None)
    ));
    assert!(casa(
        &d,
        &registro_pix("pluggy:b", Some("  Fulana  de Tal "), None)
    ));
    let acento = def_do_favorecido(Condicao::Contraparte("antonia".into()));
    assert!(casa(
        &acento,
        &registro_pix("pluggy:c", Some("Antônia Exemplo"), None)
    ));
    assert!(!casa(
        &d,
        &registro_pix("pluggy:d", Some("Fulano de Tal"), None)
    ));
    assert!(!casa(&d, &registro_pix("pluggy:e", None, None)));
}

#[test]
fn as_condicoes_novas_entram_na_especificidade_e_na_validacao() {
    let mut d = def_do_favorecido(Condicao::ContraparteDoc(vec![DOC_FICTICIO.into()]));
    d.detectar
        .todos
        .push(Condicao::Contraparte("fulana".into()));
    d.detectar
        .nenhum
        .push(Condicao::Descricao("ESTORNO".into()));
    assert_eq!(d.especificidade(), 3);
    assert!(d.validar().is_ok());
    // Lista vazia, documento sem 11 ou 14 dígitos e regex quebrado são recusados.
    for ruim in [
        Condicao::ContraparteDoc(Vec::new()),
        Condicao::ContraparteDoc(vec!["123".into()]),
        Condicao::ContraparteDoc(vec!["abc.def.ghi-jk".into()]),
        Condicao::Contraparte("(fulana".into()),
    ] {
        assert!(def_do_favorecido(ruim).validar().is_err());
    }
    // O formato em JSON.
    let j = r#"{"tipo_documental":"x","aplica_a":["registro"],
        "detectar":{"todos":[{"contraparte_doc":["000.000.000-00"]},{"contraparte":"fulana"}]}}"#;
    let lida: Definicao = serde_json::from_str(j).unwrap();
    assert!(lida.validar().is_ok());
}

#[test]
fn dicas_antigas_sem_contraparte_ainda_se_leem() {
    let j = r#"{"conta":null,"valor_centavos":-100,"data":"2026-11-12","descricao":"PIX"}"#;
    let d: Dicas = serde_json::from_str(j).unwrap();
    assert_eq!((d.contraparte, d.contraparte_doc), (None, None));
}

#[test]
fn favorecido_e_documento_saem_da_contraparte_com_origem_de_dica() {
    let mut d = def_do_favorecido(Condicao::ContraparteDoc(vec![DOC_FICTICIO.into()]));
    d.extrair.insert(
        "favorecido".into(),
        Extracao {
            regex: "(.+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Contraparte,
        },
    );
    d.extrair.insert(
        "favorecido_doc".into(),
        Extracao {
            regex: "([0-9.\\-/]+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::ContraparteDoc,
        },
    );
    let motor = Motor::novo(
        vec![VersaoDeRegra::avulsa(nova("r", d), 1, Estado::Ativa).unwrap()],
        Vec::new(),
        None,
    );
    let l = motor.ler(&registro_pix(
        "pluggy:a",
        Some("Fulana de Tal"),
        Some(DOC_FICTICIO),
    ));
    let r = reconhecido(&l);
    assert_eq!(
        r.campos["favorecido"].valor,
        Valor::Texto("Fulana de Tal".into())
    );
    assert_eq!(
        r.campos["favorecido"].de_onde,
        DeOnde::Dica {
            nome: "contraparte".into()
        }
    );
    assert_eq!(
        r.campos["favorecido_doc"].de_onde,
        DeOnde::Dica {
            nome: "contraparte_doc".into()
        }
    );
}

#[test]
fn o_documento_do_favorecido_nunca_e_gravado_na_execucao_nem_na_proposta() {
    let mut d = def_do_favorecido(Condicao::ContraparteDoc(vec![DOC_FICTICIO.into()]));
    d.extrair.insert(
        "favorecido".into(),
        Extracao {
            regex: "(.+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Contraparte,
        },
    );
    d.extrair.insert(
        "favorecido_doc".into(),
        Extracao {
            regex: "(.+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::ContraparteDoc,
        },
    );
    d.propor = vec![ModeloDeProposta::ConciliarOcorrencia {
        janela_dias: 3,
        compromisso: Some("Diarista".into()),
    }];
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("regras.sqlite");
    let mut c = Catalogo::abrir(&arquivo).unwrap();
    ativa(&mut c, nova("r", d), None);
    let motor = Motor::das_vigentes(&c, None).unwrap();
    let e = registro_pix("pluggy:a", Some("Fulana de Tal"), Some(DOC_FICTICIO));
    let l = motor.ler(&e);
    c.registrar_execucao(&l).unwrap();
    c.registrar_proposta(reconhecido(&l), &e.id).unwrap();

    let p = &c.propostas(None).unwrap()[0];
    assert!(p.conteudo.campos.contains_key("favorecido"));
    assert!(
        !p.conteudo.campos.contains_key("favorecido_doc"),
        "o documento não fica na proposta"
    );
    drop(c);
    let conn = rusqlite::Connection::open(&arquivo).unwrap();
    for tabela in ["execucao", "proposta", "retorno"] {
        let mut st = conn.prepare(&format!("SELECT * FROM {tabela}")).unwrap();
        let n = st.column_count();
        let mut linhas = st.query([]).unwrap();
        while let Some(l) = linhas.next().unwrap() {
            for i in 0..n {
                if let Ok(t) = l.get::<_, String>(i) {
                    assert!(!t.contains("000.000.000-00") && !t.contains("00000000000"));
                }
            }
        }
    }
}

#[test]
fn a_proposta_de_um_registro_usa_valor_data_e_conta_da_origem() {
    let mut d = def_do_favorecido(Condicao::ContraparteDoc(vec![DOC_FICTICIO.into()]));
    d.propor = vec![ModeloDeProposta::ConciliarOcorrencia {
        janela_dias: 3,
        compromisso: Some("Diarista".into()),
    }];
    let motor = Motor::novo(
        vec![VersaoDeRegra::avulsa(nova("r", d), 1, Estado::Ativa).unwrap()],
        Vec::new(),
        None,
    );
    let l = motor.ler(&registro_pix("pluggy:a", None, Some(DOC_FICTICIO)));
    let r = reconhecido(&l);
    assert_eq!(
        r.propostas,
        vec![Proposta::ConciliarOcorrencia {
            valor_centavos: 15_000,
            data: "2026-11-12".into(),
            janela_dias: 3,
            compromisso: Some("Diarista".into()),
        }]
    );
    // Os campos continuam só os extraídos: a dica não vira campo.
    assert!(r.campos.is_empty());
}

#[test]
fn a_chave_de_idempotencia_nao_depende_do_documento_nem_do_nome() {
    let d = def_do_favorecido(Condicao::Descricao("PIX".into()));
    let motor = Motor::novo(
        vec![VersaoDeRegra::avulsa(nova("r", d), 1, Estado::Ativa).unwrap()],
        Vec::new(),
        None,
    );
    let a = motor.ler(&registro_pix("pluggy:a", Some("A"), Some(DOC_FICTICIO)));
    let b = motor.ler(&registro_pix("pluggy:a", None, None));
    assert_eq!(reconhecido(&a).chave, reconhecido(&b).chave);
}

// ---------------------------------------------------------------- D15: o MCP decide

#[test]
fn d15_mcp_ativa_desativa_reverte_e_consolida_com_quem_mcp_no_historico() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let mut d2 = def_comprovante();
    d2.prioridade = 7;
    let v2 = c.salvar_rascunho(nova("r", d2)).unwrap();

    // Ativa a versão 2 pelo MCP: a 1 sai de cena, tudo gravado com quem = mcp.
    c.ativar("r", v2.versao, &Quem::Mcp, None).unwrap();
    assert_eq!(c.versao("r", 2).unwrap().estado, Estado::Ativa);
    let h = c.historico_de_estado("r", 2).unwrap();
    let ativacao = h.last().unwrap();
    assert_eq!((ativacao.1.as_str(), ativacao.2.as_str()), ("ativa", "mcp"));
    assert!(
        ativacao.3.contains("mcp"),
        "o motivo diz quem decidiu: {h:?}"
    );
    let h1 = c.historico_de_estado("r", 1).unwrap();
    assert_eq!(
        h1.last().unwrap().2,
        "mcp",
        "a 1 saiu de cena por decisão do mcp"
    );

    // Reverte (volta à 1) e reverte de novo (volta à 2).
    c.reverter("r", &Quem::Mcp).unwrap();
    assert_eq!(c.versao("r", 1).unwrap().estado, Estado::Ativa);
    c.reverter("r", &Quem::Mcp).unwrap();
    assert_eq!(c.versao("r", 2).unwrap().estado, Estado::Ativa);

    // Desativa.
    c.desativar("r", &Quem::Mcp).unwrap();
    assert!(c.vigentes().unwrap().is_empty());
    let h = c.historico_de_estado("r", 2).unwrap();
    assert_eq!(h.last().unwrap().2, "mcp");

    // Consolida: precisa das aceitas seguidas, e a decisão do MCP é gravada.
    c.ativar("r", 2, &Quem::Mcp, None).unwrap();
    let chaves = propostas(&mut c, 5);
    for k in &chaves {
        c.registrar_retorno(k, Retorno::Aceito, &Quem::Usuario, "ok")
            .unwrap();
    }
    c.consolidar("r", 2, &Quem::Mcp).unwrap();
    assert_eq!(c.versao("r", 2).unwrap().estado, Estado::Consolidada);
    assert_eq!(
        c.historico_de_estado("r", 2).unwrap().last().unwrap().2,
        "mcp"
    );
}

#[test]
fn d15_mcp_nao_registra_retorno_do_usuario() {
    let mut c = Catalogo::em_memoria().unwrap();
    ativa(&mut c, nova("r", def_comprovante()), None);
    let chaves = propostas(&mut c, 1);
    assert!(matches!(
        c.registrar_retorno(&chaves[0], Retorno::Aceito, &Quem::Mcp, ""),
        Err(ErroDasRegras::NaoPermitido(_))
    ));
}

#[test]
fn d12_vale_para_o_mcp_exemplo_falhando_bloqueia_a_ativacao() {
    let mut c = Catalogo::em_memoria().unwrap();
    let v = c.salvar_rascunho(nova("r", def_comprovante())).unwrap();
    c.salvar_exemplo(
        "r",
        "nao",
        &entrada("e", COMPROVANTE),
        Esperado::NaoSeAplica,
    )
    .unwrap();
    assert_eq!(
        c.ativar("r", v.versao, &Quem::Mcp, None),
        Err(ErroDasRegras::Exemplos(vec!["nao".into()]))
    );
    assert_eq!(c.versao("r", v.versao).unwrap().estado, Estado::Rascunho);
}

// ---------------------------------------------------------------- texto de OCR

/// O que o tesseract devolve de um comprovante em tela escura: espaços sobrando, `RS` no
/// lugar de `R$`, `O` no lugar de `0` e `l` no lugar de `1` dentro dos números.
const COMPROVANTE_DE_OCR: &str = "Banco  Exemplo\nComprovante   PIX\n\
Favorecido:  Fulano de Tal\nValor:  RS 1.234,5O\nData:  O6/lO/2O26\n";

fn def_do_ocr(texto_de_ocr: bool) -> Definicao {
    let mut d = def_comprovante();
    d.extrair.remove("autenticacao");
    d.texto_de_ocr = texto_de_ocr;
    d
}

fn leitura_do_ocr(texto_de_ocr: bool) -> sbf_regras::Leitura {
    let pasta = tempfile::tempdir().unwrap();
    let mut c = Catalogo::abrir(&pasta.path().join("regras.sqlite")).unwrap();
    ativa(
        &mut c,
        nova("comprovante-ocr", def_do_ocr(texto_de_ocr)),
        None,
    );
    Motor::das_vigentes(&c, None)
        .unwrap()
        .ler(&entrada("img-1", COMPROVANTE_DE_OCR))
}

#[test]
fn regra_sem_texto_de_ocr_nao_le_o_texto_ruidoso() {
    let l = leitura_do_ocr(false);
    // `Valor: RS 1.234,5O` não casa `R\$` e a data com `O` não é data: a regra reconhece
    // (a detecção é por `contem`), mas sem os campos e com o obrigatório faltando.
    let r = reconhecido(&l);
    assert!(!r.campos.contains_key("valor_centavos"));
    assert!(!r.campos.contains_key("data"));
    assert!(r.revisar);
}

#[test]
fn regra_com_texto_de_ocr_le_valor_e_data_do_texto_ruidoso() {
    let l = leitura_do_ocr(true);
    let r = reconhecido(&l);
    assert_eq!(r.campos["valor_centavos"].valor, Valor::Centavos(123_450));
    assert_eq!(r.campos["data"].valor, Valor::Data("2026-10-06".into()));
    assert!(!r.revisar);
}

#[test]
fn texto_de_ocr_falso_nao_muda_o_checksum_das_regras_que_ja_existem() {
    let d = def_comprovante();
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("texto_de_ocr"), "{json}");
    let lido: Definicao = serde_json::from_str(&json).unwrap();
    assert!(!lido.texto_de_ocr);
    let com: Definicao = serde_json::from_str(
        r#"{"tipo_documental":"x","detectar":{"todos":[{"contem":"x"}]},"texto_de_ocr":true}"#,
    )
    .unwrap();
    assert!(com.texto_de_ocr);
}

#[test]
fn normalizacao_de_ocr_junta_espacos_e_conserta_so_dentro_de_numeros() {
    use sbf_regras::definicao::normalizar_ocr;
    // Espaços juntos; quebras de linha ficam.
    assert_eq!(normalizar_ocr("a   b \t c  \nd"), "a b c\nd");
    // O -> 0 e l -> 1 só em tokens que são número.
    assert_eq!(normalizar_ocr("R$ l.234,5O"), "R$ 1.234,50");
    assert_eq!(normalizar_ocr("12/ll/2O26"), "12/11/2026");
    assert_eq!(normalizar_ocr("10:3O"), "10:30");
    // Palavras ficam como estão, mesmo com O e l, e também as que misturam letras e dígitos.
    assert_eq!(
        normalizar_ocr("Olá Lola Pol1 BOL2 ol"),
        "Olá Lola Pol1 BOL2 ol"
    );
    // `l` ou `O` sem nenhum dígito de verdade não é número.
    assert_eq!(normalizar_ocr("OO ll l O"), "OO ll l O");
    // R$, RS e R§ (esta só antes de valor com vírgula e dois decimais).
    assert_eq!(normalizar_ocr("RS 1.234,56"), "R$ 1.234,56");
    assert_eq!(normalizar_ocr("R§ 12,OO"), "R$ 12,00");
    assert_eq!(normalizar_ocr("R$1.234,56"), "R$1.234,56");
    assert_eq!(normalizar_ocr("RS1.234,56"), "R$1.234,56");
    // `RS` de Rio Grande do Sul não vira dinheiro.
    assert_eq!(
        normalizar_ocr("Porto Alegre RS 90000-000"),
        "Porto Alegre RS 90000-000"
    );
}

#[test]
fn normalizacao_de_ocr_e_idempotente() {
    use sbf_regras::definicao::normalizar_ocr;
    let uma = normalizar_ocr(COMPROVANTE_DE_OCR);
    assert_eq!(normalizar_ocr(&uma), uma);
}
