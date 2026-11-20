#![allow(clippy::unwrap_used)]
//! Testes sem janela da Caixa de entrada (fila de reconhecimento): a área e a seção "Sem
//! padrão de reconhecimento" de A triar desenham, escolher uma linha põe o cartão e a
//! Prévia no dock, resolver e dispensar tiram da lista, confirmar todas as sugestões
//! resolve, e "Reconhecer de novo" muda o item de lista. Tudo com a fila fictícia.

use std::sync::Arc;

use bancada_casca::Casca;
use sbf_inventario::caixa_de_entrada::{Estado, Situacao};
use sbf_inventario::consulta::Area;
use sbf_inventario::{Ref, Tipo};
use sbf_janela::caixa_de_entrada::{FilaFicticia, PedidoDaFila};
use sbf_janela::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
use sbf_janela::reconhecimento_ficticio::{
    NOME_DO_CONTRACHEQUE, NOME_DO_PIX, NOME_DO_RECIBO_SEM_PADRAO, fila_de_demonstracao,
    reconhecedor_de_demonstracao,
};
use sbf_janela::{AppSbf, Destino, EstadoDaBarra, Janela, demonstracao};

struct Banco {
    ctx: egui::Context,
    casca: Casca<AppSbf>,
    fila: Arc<FilaFicticia>,
}

fn sha(nome: &str) -> String {
    sbf_janela::demonstracao::hash_do_nome(nome)
}

const NOME_DO_SEGUNDO: &str = "segundo-contracheque.pdf";

/// Uma fila com três reconhecidos (o mesmo contracheque fictício sob três nomes) e um sem
/// padrão (a foto do recibo).
fn fila_do_teste() -> FilaFicticia {
    use sbf_inventario::caixa_de_entrada::Origem;
    use sbf_janela::caixa_de_entrada::entrada_ficticia;
    use sbf_reconhecimento::Reconhecedor as _;
    let reconhecedor = reconhecedor_de_demonstracao(None);
    let base = reconhecedor
        .reconhecer(
            std::path::Path::new(NOME_DO_CONTRACHEQUE),
            &mut |_| {},
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let de_demo = fila_de_demonstracao(&reconhecedor);
    let mut fila = FilaFicticia::nova();
    for (nome, chegou, origem, estado) in [
        (
            NOME_DO_CONTRACHEQUE,
            "2026-11-20T12:10:00.000Z",
            Origem::Telegram,
            Estado::Pronto,
        ),
        (
            NOME_DO_PIX,
            "2026-11-20T09:42:00.000Z",
            Origem::Gmail,
            Estado::Pronto,
        ),
        (
            NOME_DO_SEGUNDO,
            "2026-11-19T21:03:00.000Z",
            Origem::Pasta,
            Estado::PrecisaRevisar,
        ),
    ] {
        let mut r = base.clone();
        r.sha256 = sha(nome);
        r.nome = nome.to_owned();
        let e = entrada_ficticia(
            &r.sha256,
            nome,
            chegou,
            origem,
            Some("Contracheque"),
            "Contracheque · 10/2026",
            estado,
        );
        fila = fila.com(e, r);
    }
    // O sem padrão é o da demonstração.
    use sbf_janela::caixa_de_entrada::FonteDaFila as _;
    let todas = sbf_inventario::caixa_de_entrada::Filtro {
        grupo: sbf_inventario::caixa_de_entrada::Grupo::SemPadrao,
        so_abertas: true,
        origem: None,
        limite: 10,
    };
    for e in de_demo.listar(&todas).unwrap() {
        let r = de_demo.cartao(&e.content_hash).unwrap();
        fila = fila.com(e, r);
    }
    fila
}

impl Banco {
    fn novo() -> Self {
        let (inventario, comandos) = demonstracao::nova();
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        let hoje = sbf_planejamento::Data::de_iso(HOJE_DA_DEMONSTRACAO).unwrap();
        janela.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje)));
        let fila = Arc::new(fila_do_teste());
        janela.ligar_fila(fila.clone());
        janela.ligar_reconhecedor(Arc::new(reconhecedor_de_demonstracao(None)));
        let app = AppSbf::novo(
            janela,
            Box::new(inventario),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self { ctx, casca, fila };
        b.quadro();
        b
    }

    fn quadro(&mut self) {
        let Banco { ctx, casca, .. } = self;
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 900.0),
            )),
            ..Default::default()
        };
        let mut saida = ctx.run_ui(entrada, |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
    }

    fn abrir(&mut self, destino: Destino) {
        bancada_casca::abrir(&self.ctx, destino.rota());
        self.quadro();
        self.quadro();
        assert_eq!(self.casca.app.janela().navegacao.atual(), &destino);
        assert!(
            self.casca.estado.erros.is_empty(),
            "{:?}",
            self.casca.estado.erros
        );
    }

    fn pedir(&mut self, p: PedidoDaFila) {
        self.casca.app.janela().fila.pendentes.push(p);
        self.quadro();
        self.quadro();
    }
}

#[test]
fn a_caixa_lista_os_reconhecidos_e_a_triagem_os_sem_padrao() {
    let mut b = Banco::novo();
    b.abrir(Destino::AreaFutura("caixa"));
    {
        let j = b.casca.app.janela();
        assert_eq!(j.fila.caixa.len(), 3, "contracheque, pix e ambíguo");
        assert!(j.fila.caixa.iter().all(|e| e.estado.e_reconhecido()));
        assert_eq!(j.fila.sem_padrao.len(), 1);
        assert_eq!(j.fila.sem_padrao[0].estado, Estado::NaoReconhecido);
        assert_eq!(j.fila.contagens.reconhecidos, 3);
        assert_eq!(j.fila.contagens.sem_padrao, 1);
    }
    // A triar ganha a seção dos sem padrão sem quebrar a tabela de sempre.
    b.abrir(Destino::Area(Area::ATriar));
    assert!(b.casca.app.janela().fila.secao_aberta);
}

#[test]
fn escolher_a_linha_poe_o_cartao_e_a_previa_no_dock() {
    let mut b = Banco::novo();
    b.abrir(Destino::AreaFutura("caixa"));
    let s = sha(NOME_DO_CONTRACHEQUE);
    b.pedir(PedidoDaFila::Selecionar(s.clone()));
    let j = b.casca.app.janela();
    let i = j.cartao_da_fila_em_foco_para_testes().expect("em foco");
    assert_eq!(j.reconhecimento.cartoes[i].nome, NOME_DO_CONTRACHEQUE);
    assert!(j.reconhecimento.cartoes[i].resultado().is_some());
    assert_eq!(j.foco(), Some(Ref::nova(Tipo::Achado, s)));
    assert!(
        !j.reconhecimento.cartoes[i]
            .resultado()
            .unwrap()
            .sugestoes
            .is_empty(),
        "as sugestões de ligação estão no cartão"
    );
}

#[test]
fn clicar_noutra_coisa_ou_soltar_arquivo_devolve_o_dock() {
    let mut b = Banco::novo();
    b.abrir(Destino::AreaFutura("caixa"));
    b.pedir(PedidoDaFila::Selecionar(sha(NOME_DO_PIX)));
    assert!(
        b.casca
            .app
            .janela()
            .cartao_da_fila_em_foco_para_testes()
            .is_some()
    );
    // A seleção da tabela mudou: o dock volta a seguir a tabela.
    b.casca.app.janela().centro.selecao.foco = Some(Ref::nova(Tipo::Achado, "outro"));
    assert!(
        b.casca
            .app
            .janela()
            .cartao_da_fila_em_foco_para_testes()
            .is_none()
    );
    // Escolhe de novo e depois solta um arquivo.
    b.casca.app.janela().centro.selecao.foco = None;
    b.pedir(PedidoDaFila::Selecionar(sha(NOME_DO_PIX)));
    assert!(
        b.casca
            .app
            .janela()
            .cartao_da_fila_em_foco_para_testes()
            .is_some()
    );
    b.casca.app.janela().soltar_arquivos(vec![], None);
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("x.pdf");
    std::fs::write(&a, b"ficticio").unwrap();
    b.casca.app.janela().soltar_arquivos(vec![a], None);
    assert!(
        b.casca
            .app
            .janela()
            .cartao_da_fila_em_foco_para_testes()
            .is_none()
    );
}

#[test]
fn resolver_dispensar_e_reabrir_mexem_na_lista_padrao() {
    let mut b = Banco::novo();
    b.abrir(Destino::AreaFutura("caixa"));
    let (pix, amb) = (sha(NOME_DO_PIX), sha(NOME_DO_SEGUNDO));
    b.pedir(PedidoDaFila::Selecionar(pix.clone()));
    b.pedir(PedidoDaFila::Resolver(pix.clone()));
    assert_eq!(b.fila.situacao_de(&pix), Some(Situacao::Resolvida));
    {
        let j = b.casca.app.janela();
        assert_eq!(j.fila.caixa.len(), 2, "sai da lista padrão");
        assert!(j.fila.selecionada.is_none());
    }
    b.pedir(PedidoDaFila::Dispensar(amb.clone()));
    assert_eq!(b.fila.situacao_de(&amb), Some(Situacao::Dispensada));
    assert_eq!(b.casca.app.janela().fila.contagens.reconhecidos, 1);
    // "Todos" mostra de novo o que já foi decidido.
    b.casca.app.janela().fila.escopo = sbf_janela::caixa_de_entrada::Escopo::Todos;
    b.pedir(PedidoDaFila::Recarregar);
    assert_eq!(b.casca.app.janela().fila.caixa.len(), 3);
    b.pedir(PedidoDaFila::Reabrir(pix.clone()));
    assert_eq!(b.fila.situacao_de(&pix), Some(Situacao::Aberta));
}

#[test]
fn confirmar_todas_as_sugestoes_resolve_o_item() {
    let mut b = Banco::novo();
    b.abrir(Destino::AreaFutura("caixa"));
    let s = sha(NOME_DO_CONTRACHEQUE);
    b.pedir(PedidoDaFila::Selecionar(s.clone()));
    {
        // Confirmar as rubricas faz nascer a sugestão de dar baixa: o item só se resolve
        // quando todas, inclusive a nova, estiverem decididas.
        let mut j = b.casca.app.janela();
        let i = j.cartao_da_fila_em_foco_para_testes().unwrap();
        for _ in 0..10 {
            let pendente = {
                let c = &j.reconhecimento.cartoes[i];
                let n = c.resultado().unwrap().sugestoes.len();
                (0..n).find(|k| !c.confirmadas.contains(k) && !c.ignoradas.contains(k))
            };
            match pendente {
                Some(k) => j.confirmar_sugestao(i, k),
                None => break,
            }
        }
        let c = &j.reconhecimento.cartoes[i];
        assert!(c.erro_de_sugestao.is_none(), "{:?}", c.erro_de_sugestao);
    }
    b.quadro();
    b.quadro();
    assert_eq!(b.fila.situacao_de(&s), Some(Situacao::Resolvida));
    assert_eq!(b.casca.app.janela().fila.caixa.len(), 2);
}

#[test]
fn reconhecer_de_novo_leva_o_sem_padrao_para_a_caixa() {
    let (inventario, comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let reconhecedor = reconhecedor_de_demonstracao(None);
    // "Depois de criar a regra", o recibo passa a ser reconhecido como o comprovante.
    let comprovante = {
        use sbf_reconhecimento::Reconhecedor as _;
        reconhecedor
            .reconhecer(
                std::path::Path::new(NOME_DO_CONTRACHEQUE),
                &mut |_| {},
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap()
    };
    let recibo = sha(NOME_DO_RECIBO_SEM_PADRAO);
    let fila = fila_do_teste().reconhecendo_depois_como(&recibo, comprovante, Estado::Pronto);
    let fila = Arc::new(fila);
    let mut janela = Janela::nova();
    janela.ligar_fila(fila.clone());
    let app = AppSbf::novo(
        janela,
        Box::new(inventario),
        Box::new(comandos),
        EstadoDaBarra::default(),
    );
    let mut b = Banco {
        ctx,
        casca: Casca::nova(app, sbf_janela::config_da_casca()).expect("temas"),
        fila,
    };
    b.quadro();
    b.abrir(Destino::Area(Area::ATriar));
    assert_eq!(b.casca.app.janela().fila.sem_padrao.len(), 1);
    assert_eq!(b.casca.app.janela().fila.caixa.len(), 3);
    b.pedir(PedidoDaFila::ReconhecerDeNovo(recibo.clone()));
    assert_eq!(b.fila.estado_de(&recibo), Some(Estado::Pronto));
    let j = b.casca.app.janela();
    assert!(j.fila.sem_padrao.is_empty(), "saiu de A triar");
    assert_eq!(j.fila.caixa.len(), 4, "entrou na Caixa de entrada");
    assert_eq!(j.fila.contagens.sem_padrao, 0);
}
