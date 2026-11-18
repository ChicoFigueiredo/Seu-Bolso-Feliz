//! A biblioteca de regras de fábrica (`sbf_regras::biblioteca`): toda regra valida, todo
//! exemplo passa (com o script Luau de verdade), cada regra tem um exemplo que reconhece e
//! um "não se aplica", e as regras não se confundem entre si. Só dados fictícios.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::Arc;

use sbf_regras::biblioteca;
use sbf_regras::catalogo::{Catalogo, Esperado};
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Autor, Decisao, Estado};
use sbf_script::ExecutorNoProcesso;

fn semeado() -> Catalogo {
    let mut c = Catalogo::em_memoria().unwrap();
    c.semear_fabrica().unwrap();
    c
}

#[test]
fn todas_as_regras_validam_e_ficam_em_rascunho() {
    let regras = biblioteca::regras();
    assert_eq!(regras.len(), 16, "a biblioteca tem 16 regras");
    let ids: BTreeSet<_> = regras.iter().map(|(n, _)| n.regra.clone()).collect();
    assert_eq!(ids.len(), regras.len(), "ids repetidos");
    let executor = ExecutorNoProcesso::novo();
    for (nova, estado) in &regras {
        assert_eq!(*estado, Estado::Rascunho, "{}", nova.regra);
        assert_eq!(nova.autor, Autor::Fabrica);
        nova.definicao
            .validar()
            .unwrap_or_else(|e| panic!("{}: {e}", nova.regra));
        if let Some(s) = &nova.script {
            executor
                .verificar(s)
                .unwrap_or_else(|e| panic!("{}: script: {e}", nova.regra));
            assert!(nova.definicao.parametros.is_empty());
        }
        assert!(
            !nova.motivo.is_empty() && !nova.nome.is_empty(),
            "{}",
            nova.regra
        );
    }
}

#[test]
fn todo_exemplo_passa_com_o_executor_de_luau() {
    let c = semeado();
    let executor = ExecutorNoProcesso::novo();
    for (nova, _) in biblioteca::regras() {
        let versoes = c.versoes(&nova.regra).unwrap();
        assert_eq!(versoes.len(), 1, "{}", nova.regra);
        let v = versoes[0].versao;
        let rel = c.testar(&nova.regra, v, Some(&executor)).unwrap();
        assert!(
            rel.aprovado(),
            "{}: {:#?}",
            nova.regra,
            rel.resultados
                .iter()
                .filter(|r| !r.passou)
                .collect::<Vec<_>>()
        );
        assert!(rel.resultados.len() >= 2, "{}: poucos exemplos", nova.regra);
    }
}

#[test]
fn toda_regra_tem_exemplo_que_reconhece_e_um_nao_se_aplica() {
    for (nova, _) in biblioteca::regras() {
        let seus: Vec<_> = biblioteca::exemplos()
            .into_iter()
            .filter(|(r, ..)| *r == nova.regra)
            .collect();
        assert!(
            seus.iter()
                .any(|(.., e)| matches!(e, Esperado::Reconhece { .. })),
            "{}: sem exemplo que reconhece",
            nova.regra
        );
        assert!(
            seus.iter()
                .any(|(.., e)| matches!(e, Esperado::NaoSeAplica)),
            "{}: sem exemplo `não se aplica`",
            nova.regra
        );
        let nomes: BTreeSet<_> = seus.iter().map(|(_, n, ..)| n.clone()).collect();
        assert_eq!(nomes.len(), seus.len(), "{}: nomes repetidos", nova.regra);
    }
}

/// Com todas as regras da biblioteca juntas, cada exemplo que reconhece cai na regra certa:
/// as de banco ou fornecedor não se confundem entre si e vencem o boleto geral.
#[test]
fn as_regras_nao_se_confundem_entre_si() {
    let c = semeado();
    let versoes: Vec<_> = biblioteca::regras()
        .into_iter()
        .map(|(n, _)| c.versoes(&n.regra).unwrap().remove(0))
        .collect();
    let motor = Motor::novo(
        versoes,
        vec![],
        Some(Arc::new(ExecutorNoProcesso::novo()) as Arc<dyn ExecutorDeScript>),
    );
    for (regra, nome, entrada, esperado) in biblioteca::exemplos() {
        if let Esperado::Reconhece {
            tipo_documental, ..
        } = esperado
        {
            match motor.ler(&entrada).decisao {
                Decisao::Reconhecido(r) => {
                    assert_eq!(r.regra, regra, "{regra} / {nome}: venceu {}", r.regra);
                    assert_eq!(r.tipo_documental, tipo_documental);
                }
                outra => panic!("{regra} / {nome}: {outra:?}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Comportamento de regras específicas (propostas, linhas do cartão, boleto)
// ---------------------------------------------------------------------------------------

use sbf_regras::{Canal, Dicas, Entrada, Origem, Proposta, Reconhecimento, TipoDeEntrada, Valor};

fn entrada(texto: &str) -> Entrada {
    Entrada {
        versao_do_esquema: 1,
        id: "teste".to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Arrastado,
            id_externo: None,
        },
        sha256: None,
        mime: Some("application/pdf".to_owned()),
        nome: Some("teste.pdf".to_owned()),
        texto: Some(texto.to_owned()),
        dicas: Dicas::default(),
        momento: None,
    }
}

/// Lê o texto só com a regra pedida; `None` se ela não se aplica.
fn ler_com(regra: &str, texto: &str) -> Option<Reconhecimento> {
    let c = semeado();
    let v = c.versoes(regra).unwrap().remove(0);
    let motor = Motor::novo(
        vec![v],
        vec![],
        Some(Arc::new(ExecutorNoProcesso::novo()) as Arc<dyn ExecutorDeScript>),
    );
    match motor.ler(&entrada(texto)).decisao {
        Decisao::Reconhecido(r) => Some(*r),
        Decisao::Nenhuma => None,
        outra => panic!("{outra:?}"),
    }
}

fn texto_do_exemplo(regra: &str, nome: &str) -> String {
    biblioteca::exemplos()
        .into_iter()
        .find(|(r, n, ..)| r == regra && n == nome)
        .and_then(|(.., e, _)| e.texto)
        .unwrap_or_else(|| panic!("exemplo {regra}/{nome}"))
}

#[test]
fn prestacao_caixa_propoe_conciliar_e_mostra_o_resumo() {
    let t = texto_do_exemplo(
        "caixa-habitacao-comprovante-prestacao",
        "comprovante do app com todos os rótulos",
    );
    let r = ler_com("caixa-habitacao-comprovante-prestacao", &t).unwrap();
    assert!(!r.revisar, "{:?}", r.motivos);
    assert_eq!(
        r.exibir,
        vec![
            "Amortização R$ 700,00 · Juros R$ 650,00 · Taxas R$ 150,00 · Saldo devedor R$ 150.000,00"
        ]
    );
    assert_eq!(
        r.propostas,
        vec![Proposta::ConciliarOcorrencia {
            valor_centavos: 151_234,
            data: "2026-11-09".to_owned(),
            janela_dias: 5,
            compromisso: Some("Prestação Casa".to_owned()),
        }]
    );
}

#[test]
fn prestacoes_pagas_mostram_resumo_e_meses_em_grupos() {
    let t = texto_do_exemplo(
        "caixa-habitacao-prestacoes-pagas",
        "lista de janeiro a março",
    );
    let r = ler_com("caixa-habitacao-prestacoes-pagas", &t).unwrap();
    assert!(!r.revisar);
    assert_eq!(r.exibir[0], "3 prestações pagas · total R$ 4.512,34");
    assert_eq!(
        r.exibir[1],
        "Jan R$ 1.500,00 · Fev R$ 1.500,00 · Mar R$ 1.512,34"
    );
    // Abril ficou em aberto: sem campo.
    assert!(!r.campos.contains_key("mes_04_centavos"));
}

#[test]
fn fgts_mostra_a_linha_do_cartao() {
    let t = texto_do_exemplo(
        "caixa-habitacao-fgts",
        "amortização extraordinária com FGTS",
    );
    let r = ler_com("caixa-habitacao-fgts", &t).unwrap();
    assert_eq!(
        r.exibir,
        vec![
            "FGTS · amortização extraordinária R$ 20.000,00 · 05/11/2026 · Saldo devedor R$ 130.000,00"
        ]
    );
}

#[test]
fn pix_propoe_provar_o_registro() {
    for (regra, exemplo) in [
        ("comprovante-pix-caixa", "Pix enviado com todos os campos"),
        (
            "comprovante-pix-c6",
            "Pix enviado com destinatário na linha de baixo",
        ),
        (
            "comprovante-pix-nubank",
            "transferência Pix com data por extenso",
        ),
    ] {
        let r = ler_com(regra, &texto_do_exemplo(regra, exemplo)).unwrap();
        assert!(!r.revisar, "{regra}: {:?}", r.motivos);
        assert!(
            matches!(r.propostas.as_slice(), [Proposta::ProvarRegistro { .. }]),
            "{regra}: {:?}",
            r.propostas
        );
    }
}

#[test]
fn fatura_propoe_pagar_a_fatura() {
    let t = texto_do_exemplo("fatura-cartao-nubank", "vencimento por extenso");
    let r = ler_com("fatura-cartao-nubank", &t).unwrap();
    assert_eq!(
        r.propostas,
        vec![Proposta::PagarFatura {
            valor_centavos: 234_567,
            vencimento: "2026-12-15".to_owned(),
            cartao: Some("0000".to_owned()),
        }]
    );
}

#[test]
fn contas_propoem_conciliar_o_compromisso_do_mesmo_nome() {
    for (regra, exemplo, compromisso) in [
        (
            "conta-neoenergia",
            "conta com código do cliente e referência",
            "Luz",
        ),
        ("conta-vivo", "conta com número do cliente", "Celular Vivo"),
        ("conta-canaa", "fatura com valor simples", "Internet Canaã"),
    ] {
        let r = ler_com(regra, &texto_do_exemplo(regra, exemplo)).unwrap();
        assert!(!r.revisar, "{regra}: {:?}", r.motivos);
        assert!(
            matches!(
                r.propostas.as_slice(),
                [Proposta::ConciliarOcorrencia { compromisso: Some(c), .. }] if c == compromisso
            ),
            "{regra}: {:?}",
            r.propostas
        );
    }
}

#[test]
fn invoices_guardam_moeda_e_valor_em_dolar_sem_virar_reais() {
    for regra in ["invoice-anthropic", "invoice-openai"] {
        let t = biblioteca::exemplos()
            .into_iter()
            .find(|(r, ..)| r == regra)
            .and_then(|(.., e, _)| e.texto)
            .unwrap();
        let r = ler_com(regra, &t).unwrap();
        assert_eq!(r.campos["moeda"].valor, Valor::Texto("usd".to_owned()));
        assert!(r.campos.contains_key("valor_usd_centavos"));
        assert!(
            !r.campos.contains_key("valor_centavos"),
            "dólar nunca em valor_centavos"
        );
        assert!(r.propostas.is_empty());
    }
}

#[test]
fn boleto_confere_digitos_e_aceita_os_dois_ciclos_do_fator() {
    // Ciclo atual (fator 1656 = 10/12/2026), do exemplo da biblioteca.
    let t = texto_do_exemplo("boleto", "boleto bancário com dígitos conferidos");
    let r = ler_com("boleto", &t).unwrap();
    assert!(!r.revisar, "{:?}", r.motivos);
    assert_eq!(
        r.campos["vencimento"].valor,
        Valor::Data("2026-12-10".to_owned())
    );
    assert_eq!(r.campos["valor_centavos"].valor, Valor::Centavos(123_456));

    // Ciclo anterior (fator 9936 = 20/12/2024), valor R$ 500,00, sem nada mais no texto.
    let r = ler_com(
        "boleto",
        "Linha digitável\n99990000120000000000000000000000199360000050000\n",
    )
    .unwrap();
    assert!(!r.revisar, "{:?}", r.motivos);
    assert_eq!(
        r.campos["vencimento"].valor,
        Valor::Data("2024-12-20".to_owned())
    );
    assert_eq!(r.campos["valor_centavos"].valor, Valor::Centavos(50_000));
}

#[test]
fn boleto_com_digito_errado_vai_para_revisao_sem_valor_da_linha() {
    let t = texto_do_exemplo("boleto", "linha com dígito errado fica para revisão");
    let r = ler_com("boleto", &t).unwrap();
    assert!(r.revisar);
    assert!(
        r.motivos.iter().any(|m| m.contains("dígito verificador")),
        "{:?}",
        r.motivos
    );
    assert!(
        r.evidencias
            .contains(&"digito-verificador-invalido".to_owned())
    );
}

#[test]
fn boleto_com_valor_impresso_diferente_da_linha_vai_para_revisao() {
    let t = texto_do_exemplo("boleto", "boleto bancário com dígitos conferidos")
        .replace("R$ 1.234,56", "R$ 9.999,99");
    let r = ler_com("boleto", &t).unwrap();
    assert!(r.revisar);
    assert!(
        r.motivos.iter().any(|m| m.contains("valor")),
        "{:?}",
        r.motivos
    );
}

#[test]
fn regra_de_banco_vence_o_boleto_geral() {
    // Uma fatura do C6 com a linha digitável do boleto de pagamento.
    let c = semeado();
    let versoes: Vec<_> = ["fatura-cartao-c6", "boleto"]
        .iter()
        .map(|r| c.versoes(r).unwrap().remove(0))
        .collect();
    let motor = Motor::novo(
        versoes,
        vec![],
        Some(Arc::new(ExecutorNoProcesso::novo()) as Arc<dyn ExecutorDeScript>),
    );
    let boleto = texto_do_exemplo("boleto", "boleto bancário com dígitos conferidos");
    let texto = format!(
        "C6 Bank\nFatura do cartão C6 Mastercard final 0000\nVencimento 10/12/2026\n\
         Total da fatura R$ 2.345,67\n{boleto}"
    );
    match motor.ler(&entrada(&texto)).decisao {
        Decisao::Reconhecido(r) => assert_eq!(r.regra, "fatura-cartao-c6"),
        outra => panic!("{outra:?}"),
    }
}

#[test]
fn semear_e_idempotente_e_nao_ativa_nada() {
    let mut c = semeado();
    c.semear_fabrica().unwrap();
    for (nova, _) in biblioteca::regras() {
        let vs = c.versoes(&nova.regra).unwrap();
        assert_eq!(vs.len(), 1, "{}", nova.regra);
        assert_eq!(vs[0].estado, Estado::Rascunho);
        assert!(c.exemplos(&nova.regra).unwrap().len() >= 2);
    }
    // Só o contracheque de fábrica fica em vigor.
    let vigentes: Vec<_> = c.vigentes().unwrap().into_iter().map(|v| v.regra).collect();
    assert_eq!(vigentes, vec!["contracheque".to_owned()]);
}
