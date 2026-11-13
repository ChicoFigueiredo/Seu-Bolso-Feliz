//! As linhas do contrato HTTP (`sbf-contrato`) viram o livro e alimentam as regras.
#![cfg(feature = "contrato")]

use sbf_contrato as c;
use sbf_contrato::{Id, LinhasDaColecao, Versao};

use sbf_livro::contrato::{ErroDeConversao, livro_das_linhas};
use sbf_livro::hoje::painel_hoje;
use sbf_livro::{Centavos, Data, StatusDaFatura, Taxa, TipoDeConta, TipoDeMovimentacao};

fn id(n: u128) -> Id {
    Id::from_u128(n)
}

fn v() -> Versao {
    Versao("1763049600000000".into())
}

fn conta(n: u128, tipo: c::TipoDeConta, saldo: Option<i64>) -> c::Conta {
    c::Conta {
        id: id(n),
        versao: v(),
        instituicao: id(1),
        nome: format!("Conta {n}"),
        tipo,
        saldo_atual: saldo,
        limite_de_credito: None,
        ativa: true,
        ordem: Some(i32::try_from(n).expect("ordem")),
    }
}

fn mov(
    n: u128,
    conta: u128,
    tipo: c::TipoDeMovimentacao,
    valor: i64,
    data: &str,
) -> c::Movimentacao {
    c::Movimentacao {
        id: id(n),
        versao: v(),
        conta: id(conta),
        tipo,
        valor,
        descricao: None,
        data: data.into(),
        competencia: None,
        fatura: None,
        parcela_de_divida: None,
        categoria: None,
        prioridade: None,
        confirmada: true,
        notas: None,
        origem: c::OrigemDaMovimentacao::Manual,
        ocorrencia: None,
        fornecedor: None,
        documento: None,
        tags: vec![id(900)],
        criada_em: "2026-11-10T12:00:00Z".into(),
    }
}

fn divida(taxa: &str) -> c::Divida {
    c::Divida {
        id: id(70),
        versao: v(),
        conta: id(10),
        nome: "Emprestimo".into(),
        tipo: c::TipoDeDivida::EmprestimoPessoal,
        valor_original: 1_000_000,
        saldo_devedor: 540_000,
        taxa_de_juros: Some(taxa.into()),
        periodo_da_taxa: Some(c::PeriodoDaTaxa::Mensal),
        sistema_de_amortizacao: Some(c::SistemaDeAmortizacao::Price),
        total_de_parcelas: Some(24),
        parcelas_pagas: 12,
        inicio: Some("2025-11-10".into()),
        fim: None,
        estado: c::EstadoDaDivida::Ativa,
        fornecedor: None,
        tags: vec![],
    }
}

fn colecoes() -> Vec<LinhasDaColecao> {
    let mut compra = mov(41, 11, c::TipoDeMovimentacao::Saida, 8_990, "2026-10-30");
    compra.fatura = Some(id(30));
    vec![
        LinhasDaColecao::Contas(vec![
            conta(10, c::TipoDeConta::ContaCorrente, Some(100_010)),
            conta(11, c::TipoDeConta::CartaoDeCredito, None),
            conta(12, c::TipoDeConta::EmprestimoPessoal, Some(-540_000)),
        ]),
        LinhasDaColecao::Cartoes(vec![c::Cartao {
            id: id(20),
            versao: v(),
            conta: id(11),
            final_do_numero: Some("1234".into()),
            bandeira: None,
            principal: true,
            titular: None,
            limite_de_credito: Some(800_000),
            dia_de_fechamento: Some(24),
            dia_de_vencimento: Some(5),
            ativo: true,
        }]),
        LinhasDaColecao::Faturas(vec![c::Fatura {
            id: id(30),
            versao: v(),
            cartao: id(20),
            mes_de_referencia: "2026-11-01".into(),
            inicio: "2026-10-25".into(),
            fim: "2026-11-24".into(),
            vencimento: "2026-12-05".into(),
            total: Some(8_990),
            pago: None,
            estado: c::EstadoDaFatura::Aberta,
        }]),
        LinhasDaColecao::Movimentacoes(vec![
            mov(
                40,
                10,
                c::TipoDeMovimentacao::Entrada,
                400_000,
                "2026-10-25",
            ),
            compra,
            mov(
                42,
                10,
                c::TipoDeMovimentacao::PagamentoDeFatura,
                150_000,
                "2026-11-05",
            ),
            mov(43, 10, c::TipoDeMovimentacao::Ajuste, 3_300, "2026-11-03"),
        ]),
        LinhasDaColecao::Dividas(vec![divida("0.015000")]),
    ]
}

#[test]
fn as_linhas_do_contrato_viram_o_livro() {
    let livro = livro_das_linhas(20, &colecoes()).expect("livro");
    assert_eq!(livro.corte.dia(), 20);
    assert_eq!(livro.contas.len(), 3);
    assert_eq!(livro.contas[1].tipo, TipoDeConta::CartaoDeCredito);
    assert_eq!(
        livro.contas[1].saldo,
        Centavos::ZERO,
        "saldo nulo é o DEFAULT 0"
    );
    assert_eq!(livro.contas[2].tipo, TipoDeConta::Emprestimo);
    assert_eq!(livro.cartoes[0].conta_id, id(11).to_string());
    assert_eq!(livro.faturas[0].pago, Centavos::ZERO);
    assert_eq!(livro.faturas[0].status, StatusDaFatura::Aberta);
    assert_eq!(
        livro.faturas[0].fechamento,
        Data::de_iso("2026-11-24").expect("data")
    );
    assert_eq!(livro.movimentacoes[0].tipo, TipoDeMovimentacao::Receita);
    assert_eq!(livro.movimentacoes[1].fatura_id, Some(id(30).to_string()));
    assert_eq!(
        livro.movimentacoes[0].versao.as_deref(),
        Some("1763049600000000")
    );
    assert_eq!(livro.tags_das_movimentacoes.len(), 4);
    assert_eq!(
        livro.dividas[0].taxa,
        Some(Taxa {
            milionesimos: 15_000
        })
    );
}

#[test]
fn as_regras_rodam_sobre_o_livro_do_contrato() {
    let livro = livro_das_linhas(20, &colecoes()).expect("livro");
    let p = painel_hoje(&livro, Data::de_iso("2026-11-13").expect("data"));
    assert_eq!(p.ciclo.nome(), "2026-10");
    assert_eq!(p.disponivel_agora, Centavos(100_010));
    assert_eq!(p.resultado.entrou, Centavos(400_000));
    assert_eq!(p.resultado.saiu, Centavos(150_000), "o ajuste fica fora");
    assert_eq!(p.resultado.no_cartao, Centavos(8_990));
    assert_eq!(p.cartoes[0].fatura_atual, Centavos(8_990));
    assert_eq!(p.livre_apos_compromissos, Centavos(100_010 - 8_990));
    assert_eq!(p.compromisso_mensal_de_dividas, Centavos(45_000));
}

#[test]
fn linha_ilegivel_e_erro_e_nao_entra_calada() {
    let mut m = mov(40, 10, c::TipoDeMovimentacao::Entrada, 1, "2026-02-30");
    m.tags.clear();
    let erro = livro_das_linhas(20, &[LinhasDaColecao::Movimentacoes(vec![m])]);
    assert!(
        matches!(erro, Err(ErroDeConversao::Data { .. })),
        "{erro:?}"
    );

    let erro = livro_das_linhas(20, &[LinhasDaColecao::Dividas(vec![divida("1,5%")])]);
    assert!(
        matches!(erro, Err(ErroDeConversao::Taxa { .. })),
        "{erro:?}"
    );

    assert_eq!(
        livro_das_linhas(0, &[]).map(|_| ()),
        Err(ErroDeConversao::DiaDeCorte(0))
    );
}
