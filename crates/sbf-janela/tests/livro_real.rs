//! A janela sobre a `ConsultaDoLivro` de verdade (a leitura da cópia da estação), sem
//! janela do sistema: um livro fictício em memória no lugar do cache, e o pareamento
//! de mentira para o botão "Parear esta estação".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::rc::Rc;

use bancada_casca::Casca;
use sbf_estacao::ConsultaDoLivro;
use sbf_inventario::alteracao::EstadoDaRede;
use sbf_inventario::consulta::Area;
use sbf_janela::livro_ficticio;
use sbf_janela::{
    AndamentoDoPareamento, AppSbf, ConsultaComposta, Destino, EstadoDaBarra, Janela, Pareamento,
    Pedido, demonstracao,
};
use sbf_livro::{
    Centavos, Conta, Data, Divida, Fatura, LivroCaixa, Movimentacao, SistemaDeAmortizacao,
    StatusDaDivida, StatusDaFatura, Taxa, TipoDeConta, TipoDeMovimentacao,
};

fn entrada() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    }
}

fn data(m: u8, d: u8) -> Data {
    Data::nova(2026, m, d).unwrap()
}

/// O livro de um cache fictício: uma conta, uma movimentação, uma fatura e uma dívida.
fn livro() -> LivroCaixa {
    let mut l = LivroCaixa::default();
    l.contas = vec![Conta {
        id: "c1".into(),
        instituicao_id: "i1".into(),
        nome: "Conta Exemplo".into(),
        tipo: TipoDeConta::ContaCorrente,
        saldo: Centavos(250_000),
        limite: None,
        ativa: true,
        ordem: None,
    }];
    l.movimentacoes = vec![Movimentacao {
        id: "m1".into(),
        conta_id: "c1".into(),
        tipo: TipoDeMovimentacao::Despesa,
        valor: Centavos(-12_345),
        descricao: Some("Mercado Exemplo".into()),
        data: data(11, 10),
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
    l.faturas = vec![Fatura {
        id: "f1".into(),
        cartao_id: "k1".into(),
        mes_de_referencia: data(11, 1),
        inicio: data(10, 6),
        fechamento: data(11, 5),
        vencimento: data(11, 12),
        total: Centavos(50_000),
        pago: Centavos(0),
        status: StatusDaFatura::Aberta,
    }];
    l.dividas = vec![Divida {
        id: "d1".into(),
        conta_id: "c1".into(),
        nome: "Financiamento Exemplo".into(),
        tipo: "mortgage".into(),
        valor_original: Centavos(600_000),
        saldo_devedor: Centavos(500_000),
        taxa: Some(Taxa {
            milionesimos: 10_000,
        }),
        tipo_de_taxa: Some("monthly".into()),
        sistema: Some(SistemaDeAmortizacao::Sac),
        total_de_parcelas: Some(12),
        parcelas_pagas: 2,
        inicio: None,
        fim: None,
        status: StatusDaDivida::Ativa,
        fornecedor_id: None,
    }];
    l
}

#[derive(Debug, Clone, Default)]
struct PareamentoDeMentira {
    iniciados: Rc<Cell<u32>>,
}

impl Pareamento for PareamentoDeMentira {
    fn iniciar(&mut self) {
        self.iniciados.set(self.iniciados.get() + 1);
    }

    fn andamento(&self) -> AndamentoDoPareamento {
        AndamentoDoPareamento::Parado
    }
}

fn casca(estacao_nao_pareada: bool) -> (egui::Context, Casca<AppSbf>, Rc<Cell<u32>>) {
    let (inventario, comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let consulta_do_livro = ConsultaDoLivro::do_livro(livro(), Vec::new(), data(11, 13));
    let composta = ConsultaComposta::nova(Box::new(inventario), Some(Box::new(consulta_do_livro)));
    let mut janela = Janela::nova();
    janela.hoje = "2026-11-13".into();
    let estacao = livro_ficticio::novo();
    if estacao_nao_pareada {
        estacao.definir_rede(EstadoDaRede::NaoPareada);
    }
    janela.ligar_estacao(Box::new(estacao));
    let pareamento = PareamentoDeMentira::default();
    let iniciados = pareamento.iniciados.clone();
    janela.ligar_pareamento(Box::new(pareamento));
    let app = AppSbf::novo(
        janela,
        Box::new(composta),
        Box::new(comandos),
        EstadoDaBarra::default(),
    );
    let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
    (ctx, casca, iniciados)
}

fn quadro(ctx: &egui::Context, casca: &mut Casca<AppSbf>) {
    let mut saida = ctx.run_ui(entrada(), |ui| casca.desenhar(ui));
    saida.textures_delta.clear();
}

#[test]
fn a_janela_desenha_toda_area_de_dinheiro_sobre_a_consulta_do_livro() {
    let (ctx, mut casca, _) = casca(false);
    quadro(&ctx, &mut casca);
    for area in Area::DO_LIVRO_CAIXA
        .into_iter()
        .chain([Area::Sincronizacao, Area::Relatorios])
    {
        bancada_casca::abrir(&ctx, Destino::Area(area).rota());
        quadro(&ctx, &mut casca);
        quadro(&ctx, &mut casca);
        assert!(
            casca.estado.erros.is_empty(),
            "{area:?}: {:?}",
            casca.estado.erros
        );
    }
}

#[test]
fn parear_esta_estacao_chama_o_pareamento_ligado() {
    let (ctx, mut casca, iniciados) = casca(true);
    bancada_casca::abrir(&ctx, Destino::Area(Area::Sincronizacao).rota());
    quadro(&ctx, &mut casca);
    quadro(&ctx, &mut casca);
    assert_eq!(iniciados.get(), 0);
    casca.app.janela().pedir(Pedido::Parear);
    quadro(&ctx, &mut casca);
    assert_eq!(iniciados.get(), 1);
    assert!(casca.estado.erros.is_empty(), "{:?}", casca.estado.erros);
}
