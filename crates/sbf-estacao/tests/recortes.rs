//! Os recortes da `ConsultaDoLivro` que antes devolviam `NaoSuportado` (origem, próximos,
//! fornecedor-e-contrato, simulação, compromissos-e-dividas, parcelas-e-dividas,
//! documentos-a-revisar, faturas-abertas) e o `relatorio()`. Livro fictício em memória.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

use sbf_contrato::{Documento, OrigemDoDocumento};
use sbf_estacao::ConsultaDoLivro;
use sbf_inventario::consulta::{
    Celula, Consulta, ErroDeConsulta, Filtro, Fonte, PedidoDeTabela, Pergunta, Tabela,
};
use sbf_inventario::referencia::{Ref, Tipo};
use sbf_livro::hoje::painel_hoje;
use sbf_livro::{
    Cartao, Centavos, Conta, Data, Divida, Fatura, Fornecedor, Frequencia, LivroCaixa,
    Movimentacao, Ocorrencia, Parcela, Recorrente, SistemaDeAmortizacao, StatusDaDivida,
    StatusDaFatura, StatusDaOcorrencia, StatusDaParcela, Taxa, TipoDeConta, TipoDeMovimentacao,
};

fn hoje() -> Data {
    Data::nova(2026, 11, 13).unwrap()
}

fn data(a: i32, m: u8, d: u8) -> Data {
    Data::nova(a, m, d).unwrap()
}

fn livro() -> LivroCaixa {
    let mut l = LivroCaixa::default();
    l.contas = vec![
        Conta {
            id: "c-corrente".into(),
            instituicao_id: "i1".into(),
            nome: "Conta Exemplo".into(),
            tipo: TipoDeConta::ContaCorrente,
            saldo: Centavos(100_000),
            limite: None,
            ativa: true,
            ordem: None,
        },
        Conta {
            id: "c-cartao".into(),
            instituicao_id: "i1".into(),
            nome: "Cartão Exemplo".into(),
            tipo: TipoDeConta::CartaoDeCredito,
            saldo: Centavos(0),
            limite: None,
            ativa: true,
            ordem: None,
        },
    ];
    l.cartoes = vec![Cartao {
        id: "k1".into(),
        conta_id: "c-cartao".into(),
        final_do_numero: Some("1234".into()),
        bandeira: None,
        limite: None,
        dia_de_fechamento: Some(5),
        dia_de_vencimento: Some(12),
        ativo: true,
    }];
    l.faturas = vec![
        Fatura {
            id: "f-aberta".into(),
            cartao_id: "k1".into(),
            mes_de_referencia: data(2026, 11, 1),
            inicio: data(2026, 10, 6),
            fechamento: data(2026, 11, 5),
            vencimento: data(2026, 11, 12),
            total: Centavos(50_000),
            pago: Centavos(10_000),
            status: StatusDaFatura::Parcial,
        },
        Fatura {
            id: "f-paga".into(),
            cartao_id: "k1".into(),
            mes_de_referencia: data(2026, 10, 1),
            inicio: data(2026, 9, 6),
            fechamento: data(2026, 10, 5),
            vencimento: data(2026, 10, 12),
            total: Centavos(30_000),
            pago: Centavos(30_000),
            status: StatusDaFatura::Paga,
        },
    ];
    l.fornecedores = vec![Fornecedor {
        id: "forn1".into(),
        nome: "Energia Exemplo".into(),
        nome_fantasia: None,
        razao_social: None,
        ativo: true,
    }];
    l.dividas = vec![
        Divida {
            id: "d1".into(),
            conta_id: "c-corrente".into(),
            nome: "Financiamento Exemplo".into(),
            tipo: "mortgage".into(),
            valor_original: Centavos(1_200_000),
            saldo_devedor: Centavos(1_000_000),
            taxa: Some(Taxa {
                milionesimos: 10_000,
            }),
            tipo_de_taxa: Some("monthly".into()),
            sistema: Some(SistemaDeAmortizacao::Price),
            total_de_parcelas: Some(12),
            parcelas_pagas: 2,
            inicio: None,
            fim: None,
            status: StatusDaDivida::Ativa,
            fornecedor_id: Some("forn1".into()),
        },
        Divida {
            id: "d-sem-taxa".into(),
            conta_id: "c-cartao".into(),
            nome: "Acordo sem juros".into(),
            tipo: "other".into(),
            valor_original: Centavos(10_000),
            saldo_devedor: Centavos(10_000),
            taxa: None,
            tipo_de_taxa: None,
            sistema: None,
            total_de_parcelas: None,
            parcelas_pagas: 0,
            inicio: None,
            fim: None,
            status: StatusDaDivida::Ativa,
            fornecedor_id: None,
        },
    ];
    l.parcelas = ["p2", "p1"]
        .iter()
        .enumerate()
        .map(|(i, id)| Parcela {
            id: (*id).into(),
            divida_id: "d1".into(),
            numero: if *id == "p1" { 1 } else { 2 },
            vencimento: data(2026, 12, 10 + i as u8),
            total: Centavos(100_000),
            principal: None,
            juros: None,
            seguro: None,
            tarifa: None,
            pago: Centavos(0),
            data_do_pagamento: None,
            status: StatusDaParcela::Pendente,
        })
        .collect();
    l.recorrentes = vec![Recorrente {
        id: "r1".into(),
        nome: "Luz".into(),
        tipo: TipoDeMovimentacao::Despesa,
        conta_id: Some("c-corrente".into()),
        valor: Some(Centavos(20_000)),
        valor_variavel: false,
        frequencia: Frequencia::Mensal,
        dia_do_mes: Some(20),
        intervalo_dias: None,
        categoria_id: None,
        fornecedor_id: Some("forn1".into()),
        inicia_em: None,
        termina_em: None,
        ativo: true,
    }];
    l.ocorrencias = vec![
        ocorrencia("o-depois", data(2026, 12, 20), StatusDaOcorrencia::Pendente),
        ocorrencia("o-antes", data(2026, 11, 18), StatusDaOcorrencia::Pendente),
        ocorrencia("o-paga", data(2026, 10, 20), StatusDaOcorrencia::Paga),
    ];
    l.movimentacoes = vec![Movimentacao {
        id: "m1".into(),
        conta_id: "c-corrente".into(),
        tipo: TipoDeMovimentacao::Despesa,
        valor: Centavos(-5_000),
        descricao: Some("Mercado".into()),
        data: data(2026, 11, 10),
        data_competencia: None,
        fatura_id: None,
        parcela_id: None,
        categoria_id: None,
        fornecedor_id: None,
        ocorrencia_id: None,
        documento_id: None,
        confirmada: true,
        versao: None,
    }];
    l
}

fn ocorrencia(id: &str, quando: Data, status: StatusDaOcorrencia) -> Ocorrencia {
    Ocorrencia {
        id: id.into(),
        recorrente_id: "r1".into(),
        data_esperada: quando,
        valor_esperado: Some(Centavos(20_000)),
        valor_real: None,
        status,
        data_do_pagamento: None,
        movimentacao_id: None,
    }
}

fn documento(n: u128, nome: &str, estado: &str) -> Documento {
    Documento {
        id: sbf_contrato::Id::from_u128(n),
        versao: None,
        origem: OrigemDoDocumento::EnvioManual,
        nome_do_arquivo: nome.into(),
        tipo_mime: None,
        tamanho_em_bytes: None,
        estado: estado.into(),
        hash_do_conteudo: None,
        tipo_de_documento: Some("boleto".into()),
        fornecedor: None,
        nome_do_fornecedor_bruto: None,
        assunto_do_email: None,
        remetente_do_email: None,
        recebido_em: None,
        criado_em: None,
    }
}

fn consulta() -> ConsultaDoLivro {
    ConsultaDoLivro::do_livro(
        livro(),
        vec![
            documento(1, "boleto-exemplo.pdf", "pending_review"),
            documento(2, "ja-lido.pdf", "processed"),
            documento(3, "rascunho.pdf", "drafted"),
        ],
        hoje(),
    )
}

fn recorte(c: &ConsultaDoLivro, tipo: Tipo, chave: &str, recorte: &str) -> Tabela {
    c.tabela(&PedidoDeTabela::ItensDoTotal {
        de: Ref::nova(tipo, chave),
        fonte: Fonte::LivroCaixa,
        ciclo: None,
        recorte: Some(recorte.to_owned()),
    })
    .unwrap()
}

fn texto(c: &Celula) -> String {
    match c {
        Celula::Texto(t) | Celula::Link { texto: t, .. } | Celula::Data(t) => t.clone(),
        outra => format!("{outra:?}"),
    }
}

fn centavos(c: &Celula) -> i64 {
    match c {
        Celula::Valor { centavos, .. } => *centavos,
        outra => panic!("não é valor: {outra:?}"),
    }
}

#[test]
fn faturas_abertas_so_as_que_faltam_pagar() {
    let t = recorte(&consulta(), Tipo::Ciclo, "2026-11", "faturas-abertas");
    assert_eq!(t.linhas.len(), 1);
    let l = &t.linhas[0];
    assert_eq!(l.alvo, Some(Ref::nova(Tipo::Fatura, "f-aberta")));
    assert_eq!(
        texto(&l.celulas[0]),
        "Fatura Cartão Exemplo final 1234 · 2026-11"
    );
    assert_eq!(centavos(&l.celulas[2]), -40_000);
}

#[test]
fn documentos_a_revisar_so_pendentes_e_rascunhos() {
    let t = recorte(&consulta(), Tipo::Ciclo, "2026-11", "documentos-a-revisar");
    let nomes: Vec<String> = t.linhas.iter().map(|l| texto(&l.celulas[0])).collect();
    assert_eq!(nomes, ["boleto-exemplo.pdf", "rascunho.pdf"]);
}

#[test]
fn origem_do_compromisso_aponta_o_recorrente() {
    let c = consulta();
    let compromisso = painel_hoje(c.livro(), hoje())
        .compromissos
        .into_iter()
        .find(|x| x.id == "o-antes")
        .expect("o compromisso da ocorrência");
    let t = recorte(&c, Tipo::Compromisso, &compromisso.id, "origem");
    assert_eq!(t.linhas.len(), 1);
    assert_eq!(t.linhas[0].alvo, Some(Ref::nova(Tipo::Recorrente, "r1")));
    assert_eq!(texto(&t.linhas[0].celulas[1]), "Luz");
    // Um compromisso que não existe não tem origem.
    assert!(
        recorte(&c, Tipo::Compromisso, "nao-existe", "origem")
            .linhas
            .is_empty()
    );
}

#[test]
fn proximos_do_recorrente_em_ordem_e_sem_as_pagas() {
    let t = recorte(&consulta(), Tipo::Recorrente, "r1", "proximos");
    let datas: Vec<String> = t.linhas.iter().map(|l| texto(&l.celulas[0])).collect();
    assert_eq!(datas, ["2026-11-18", "2026-12-20"]);
    assert_eq!(centavos(&t.linhas[0].celulas[2]), -20_000);
}

#[test]
fn fornecedor_e_contrato_do_recorrente() {
    let t = recorte(&consulta(), Tipo::Recorrente, "r1", "fornecedor-e-contrato");
    assert_eq!(t.linhas.len(), 1);
    assert_eq!(t.linhas[0].alvo, Some(Ref::nova(Tipo::Fornecedor, "forn1")));
    assert_eq!(texto(&t.linhas[0].celulas[1]), "Energia Exemplo");
}

#[test]
fn simulacao_compara_sac_e_price_nas_parcelas_que_faltam() {
    let t = recorte(&consulta(), Tipo::Divida, "d1", "simulacao");
    assert_eq!(t.linhas.len(), 2);
    assert_eq!(texto(&t.linhas[0].celulas[0]), "SAC");
    assert_eq!(texto(&t.linhas[1].celulas[0]), "Price (o contratado)");
    // 10 parcelas sobre 10.000,00 a 1% ao mês: a Price tem parcela fixa, a SAC decresce.
    let (sac, price) = (&t.linhas[0].celulas, &t.linhas[1].celulas);
    assert!(
        centavos(&sac[1]) < centavos(&sac[2]),
        "SAC: a primeira é a maior"
    );
    assert_eq!(
        centavos(&price[1]),
        centavos(&price[2]),
        "Price: parcela fixa"
    );
    assert!(
        centavos(&sac[3]) > centavos(&price[3]),
        "a SAC paga menos juros"
    );
    assert!(
        centavos(&price[4]) < -1_000_000,
        "o total passa do principal"
    );
    // Sem taxa, não há o que simular.
    assert!(
        recorte(&consulta(), Tipo::Divida, "d-sem-taxa", "simulacao")
            .linhas
            .is_empty()
    );
}

#[test]
fn compromissos_do_cartao_dividas_e_faturas_em_aberto() {
    let t = recorte(&consulta(), Tipo::Cartao, "k1", "compromissos-e-dividas");
    let alvos: Vec<Option<Ref>> = t.linhas.iter().map(|l| l.alvo.clone()).collect();
    assert_eq!(
        alvos,
        [
            Some(Ref::nova(Tipo::Divida, "d-sem-taxa")),
            Some(Ref::nova(Tipo::Fatura, "f-aberta")),
        ]
    );
}

#[test]
fn parcelas_e_dividas_da_conta_e_do_fornecedor() {
    let c = consulta();
    let da_conta = recorte(&c, Tipo::Conta, "c-corrente", "parcelas-e-dividas");
    let alvos: Vec<Ref> = da_conta
        .linhas
        .iter()
        .filter_map(|l| l.alvo.clone())
        .collect();
    assert_eq!(
        alvos,
        [
            Ref::nova(Tipo::Divida, "d1"),
            Ref::nova(Tipo::Parcela, "p1"),
            Ref::nova(Tipo::Parcela, "p2"),
        ]
    );
    assert_eq!(
        texto(&da_conta.linhas[1].celulas[1]),
        "Financiamento Exemplo · parcela 1"
    );
    let do_fornecedor = recorte(&c, Tipo::Fornecedor, "forn1", "parcelas-e-dividas");
    assert_eq!(do_fornecedor.linhas.len(), 3);
    assert!(
        recorte(&c, Tipo::Conta, "c-cartao", "parcelas-e-dividas")
            .linhas
            .len()
            == 1
    );
}

#[test]
fn relatorio_usa_os_gastos_do_livro() {
    let c = consulta();
    let painel = c
        .relatorio(Pergunta::QuantoSaiu, Fonte::LivroCaixa, &Filtro::default())
        .unwrap();
    assert!(!painel.secoes.is_empty());
    // As perguntas do inventário e a fonte errada seguem recusadas, com o motivo.
    for p in [
        Pergunta::NaoClassificado,
        Pergunta::PossiveisDuplicidades,
        Pergunta::OQueFaltaParaConfiar,
    ] {
        let e = c
            .relatorio(p, Fonte::LivroCaixa, &Filtro::default())
            .unwrap_err();
        assert!(matches!(e, ErroDeConsulta::NaoSuportado(_)));
    }
    assert!(matches!(
        c.relatorio(Pergunta::QuantoSaiu, Fonte::Inventario, &Filtro::default()),
        Err(ErroDeConsulta::NaoSuportado(_))
    ));
}
