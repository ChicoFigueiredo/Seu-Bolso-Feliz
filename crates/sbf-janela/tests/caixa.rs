#![allow(clippy::unwrap_used)]
//! Testes sem janela da caixa de entrada e da edição (spec 08, CX-10): a área desenha, os
//! atalhos A, R e Shift+A agem, a página de dinheiro mostra Editar, Ratear e Excluir com a
//! marca de pendente, a Sincronização mostra a fila e as recusas, e o envio de lote anda.

use bancada_casca::Casca;
use sbf_inventario::alteracao::CampoLadoALado;
use sbf_inventario::consulta::{Area, Celula, Filtro, PedidoDeTabela};
use sbf_inventario::{Consulta, Ref, Tipo};
use sbf_janela::caixa::{
    self, CaixaFicticia, EnvioDoLote, EstadoDeEnvio, FonteDaCaixa, StatusDoRascunho,
};
use sbf_janela::caixa_ui::{DialogoDaCaixa, PedidoDaCaixa};
use sbf_janela::janela::Dialogo;
use sbf_janela::livro_ficticio::{self, LivroFicticio};
use sbf_janela::{
    AppSbf, ConsultaComposta, Destino, EstadoDaBarra, IdAcao, Janela, Pedido, demonstracao,
};

struct Banco {
    ctx: egui::Context,
    casca: Casca<AppSbf>,
    caixa: CaixaFicticia,
    livro: LivroFicticio,
}

fn entrada(eventos: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1500.0, 900.0),
        )),
        events: eventos,
        ..Default::default()
    }
}

fn tecla(key: egui::Key, shift: bool) -> Vec<egui::Event> {
    let modifiers = egui::Modifiers {
        shift,
        ..Default::default()
    };
    vec![egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }]
}

impl Banco {
    fn novo() -> Self {
        let (inventario, comandos) = demonstracao::nova();
        let livro = livro_ficticio::novo();
        let documentos: Vec<Ref> = inventario
            .tabela(&PedidoDeTabela::Area {
                area: Area::Documentos,
                filtro: Filtro::default(),
            })
            .expect("documentos")
            .linhas
            .into_iter()
            .filter_map(|l| l.alvo)
            .take(5)
            .collect();
        let caixa = caixa::demonstracao(&documentos, Box::new(livro.clone()));
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = livro_ficticio::HOJE.into();
        janela.ligar_estacao(Box::new(livro.clone()));
        janela.ligar_caixa(Box::new(caixa.clone()));
        let consulta = ConsultaComposta::nova(Box::new(inventario), Some(Box::new(livro.clone())));
        let app = AppSbf::novo(
            janela,
            Box::new(consulta),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self {
            ctx,
            casca,
            caixa,
            livro,
        };
        b.quadro();
        b
    }

    fn quadro(&mut self) {
        self.quadro_com(Vec::new());
    }

    fn quadro_com(&mut self, eventos: Vec<egui::Event>) {
        let Banco { ctx, casca, .. } = self;
        let mut saida = ctx.run_ui(entrada(eventos), |ui| casca.desenhar(ui));
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

    fn pedir(&mut self, p: PedidoDaCaixa) {
        self.casca.app.janela().pedir(Pedido::Caixa(p));
        self.quadro();
        self.quadro();
    }

    fn rascunho(&self, id: &str) -> caixa::Rascunho {
        self.caixa
            .lotes()
            .unwrap()
            .into_iter()
            .flat_map(|l| l.rascunhos)
            .find(|r| r.id == id)
            .expect("rascunho")
    }

    fn mensagem(&self) -> Option<(String, bool)> {
        self.casca
            .app
            .janela()
            .mensagem
            .as_ref()
            .map(|m| (m.texto.clone(), m.erro))
    }
}

fn uma_movimentacao(livro: &LivroFicticio) -> Ref {
    let t = livro
        .tabela(&PedidoDeTabela::Area {
            area: Area::Movimentacoes,
            filtro: Filtro::default(),
        })
        .expect("movimentações");
    t.linhas
        .iter()
        .find_map(|l| {
            let alvo = l.alvo.clone()?;
            matches!(&l.celulas[1], Celula::Link { .. }).then_some(alvo)
        })
        .expect("movimentação")
}

fn caixa_de_entrada() -> Destino {
    Destino::AreaFutura("caixa")
}

#[test]
fn a_area_caixa_de_entrada_desenha_com_a_lista_o_formulario_e_a_previa() {
    let mut b = Banco::novo();
    b.abrir(caixa_de_entrada());
    // A primeira aberta é a selecionada.
    assert_eq!(b.rascunho("r1").status, StatusDoRascunho::EmRevisao);
    for t in sbf_janela::tema::temas() {
        bancada_casca::pedir(&b.ctx, bancada_casca::Pedido::Tema(t.id.clone()));
        b.quadro();
    }
}

#[test]
fn atalho_a_aprova_o_rascunho_selecionado_e_marca_pendente() {
    let mut b = Banco::novo();
    b.abrir(caixa_de_entrada());
    b.quadro_com(tecla(egui::Key::A, false));
    b.quadro();
    b.quadro();
    let r = b.rascunho("r1");
    assert_eq!(r.status, StatusDoRascunho::Aprovado);
    assert_eq!(r.envio, Some(EstadoDeEnvio::Pendente));
    let fila = b.caixa.escritas().unwrap();
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].estado, EstadoDeEnvio::Pendente);
    // O aceite tira a marca de pendente.
    b.caixa.servidor_aceita_tudo();
    b.quadro();
    assert_eq!(b.rascunho("r1").envio, Some(EstadoDeEnvio::Aceita));
}

#[test]
fn atalho_r_abre_o_motivo_e_recusar_grava() {
    let mut b = Banco::novo();
    b.abrir(caixa_de_entrada());
    b.quadro_com(tecla(egui::Key::R, false));
    b.quadro();
    let aberto = matches!(
        b.casca.app.janela().dialogo,
        Some(Dialogo::Caixa(DialogoDaCaixa::Recusar { .. }))
    );
    assert!(aberto, "R abre o diálogo do motivo");
    // Sem motivo, o diálogo não confirma.
    let d = b.casca.app.janela().dialogo.clone().unwrap();
    assert!(sbf_janela::janela::confirmar_dialogo(&d).is_err());
    b.pedir(PedidoDaCaixa::Recusar {
        id: "r1".into(),
        motivo: "não é meu".into(),
    });
    assert_eq!(b.rascunho("r1").status, StatusDoRascunho::Recusado);
}

#[test]
fn atalho_shift_a_pede_confirmacao_e_o_lote_e_tudo_ou_nada() {
    let mut b = Banco::novo();
    b.abrir(caixa_de_entrada());
    b.quadro_com(tecla(egui::Key::A, true));
    b.quadro();
    let Some(Dialogo::Caixa(DialogoDaCaixa::AprovarLote {
        id, incompletos, ..
    })) = b.casca.app.janela().dialogo.clone()
    else {
        panic!("Shift+A abre a confirmação do lote");
    };
    assert_eq!(id, "lote-gmail-nov");
    // O r2 não tem conta: o lote não passa e nada muda.
    assert_eq!(incompletos.len(), 1, "{incompletos:?}");
    assert!(
        sbf_janela::janela::confirmar_dialogo(&Dialogo::Caixa(b.caixa_dialogo_do_lote().clone()))
            .is_err()
    );
    b.pedir(PedidoDaCaixa::AprovarLote("lote-gmail-nov".into()));
    assert!(b.mensagem().is_some_and(|(_, erro)| erro));
    assert_eq!(b.rascunho("r1").status, StatusDoRascunho::EmRevisao);
    // Corrigido o r2, o lote passa inteiro.
    let mut campos = b.rascunho("r2").campos;
    campos.conta = Some("conta-corrente".into());
    b.pedir(PedidoDaCaixa::Corrigir {
        id: "r2".into(),
        campos,
    });
    b.pedir(PedidoDaCaixa::AprovarLote("lote-gmail-nov".into()));
    for id in ["r1", "r2", "r3"] {
        assert_eq!(b.rascunho(id).status, StatusDoRascunho::Aprovado, "{id}");
    }
    assert_eq!(b.rascunho("r4").status, StatusDoRascunho::EmRevisao);
}

impl Banco {
    fn caixa_dialogo_do_lote(&self) -> DialogoDaCaixa {
        sbf_janela::caixa_ui::dialogo_de_lote(&self.caixa, "lote-gmail-nov").unwrap()
    }
}

#[test]
fn pagina_de_movimentacao_edita_rateia_e_exclui_com_marca_de_pendente() {
    let mut b = Banco::novo();
    let mov = uma_movimentacao(&b.livro);
    b.abrir(Destino::Item(mov.clone()));
    // Editar abre o diálogo preenchido.
    b.pedir(PedidoDaCaixa::PedirEdicao(mov.clone()));
    let Some(Dialogo::Caixa(DialogoDaCaixa::Editar {
        descricao, valor, ..
    })) = b.casca.app.janela().dialogo.clone()
    else {
        panic!("Editar abre o diálogo");
    };
    assert!(!descricao.is_empty() && !valor.is_empty());
    let mut campos = b.caixa.movimentacao(&mov).unwrap().unwrap().campos;
    campos.descricao = "Descrição nova".into();
    b.casca.app.janela().dialogo = None;
    b.pedir(PedidoDaCaixa::Editar {
        alvo: mov.clone(),
        campos: campos.clone(),
    });
    assert_eq!(
        caixa::envio_da_movimentacao(&b.caixa.escritas().unwrap(), &mov),
        Some(EstadoDeEnvio::Pendente)
    );
    // Ratear: soma errada é recusada antes de gravar; a certa grava.
    let total = campos.valor_centavos.abs();
    let antes = b.caixa.escritas().unwrap().len();
    let linha = |c: &str, v| caixa::RateioDaMovimentacao {
        centro: c.into(),
        centavos: v,
    };
    b.pedir(PedidoDaCaixa::Ratear {
        alvo: mov.clone(),
        linhas: vec![linha("casa", total - 1)],
    });
    assert!(b.mensagem().is_some_and(|(_, erro)| erro));
    assert_eq!(b.caixa.escritas().unwrap().len(), antes);
    b.pedir(PedidoDaCaixa::Ratear {
        alvo: mov.clone(),
        linhas: vec![linha("casa", total - 100), linha("lazer", 100)],
    });
    assert_eq!(b.caixa.escritas().unwrap().len(), antes + 1);
    // Excluir pede confirmação: o pedido abre o diálogo e só Excluir apaga.
    b.pedir(PedidoDaCaixa::PedirExclusao(mov.clone()));
    assert!(matches!(
        b.casca.app.janela().dialogo,
        Some(Dialogo::Caixa(DialogoDaCaixa::Excluir { n_rateios: 2, .. }))
    ));
    assert!(b.caixa.movimentacao(&mov).unwrap().is_some());
    b.pedir(PedidoDaCaixa::Excluir(mov.clone()));
    assert!(b.caixa.movimentacao(&mov).unwrap().is_none());
    b.quadro();
}

#[test]
fn sincronizacao_mostra_a_fila_e_a_recusa_com_refazer_e_descartar() {
    let mut b = Banco::novo();
    b.pedir(PedidoDaCaixa::Aprovar("r1".into()));
    b.pedir(PedidoDaCaixa::Aprovar("r4".into()));
    b.abrir(Destino::Area(Area::Sincronizacao));
    let id = b.caixa.escritas().unwrap()[1].id.clone();
    b.caixa.servidor_recusa(
        &id,
        "O rascunho mudou no servidor depois que você aprovou.",
        vec![CampoLadoALado::novo("Valor", "R$ 312,44", "R$ 320,00")],
    );
    b.quadro();
    b.quadro();
    assert_eq!(caixa::contar(&b.caixa.escritas().unwrap()), (1, 1));
    // Refazer: nasce outra pendente.
    b.pedir(PedidoDaCaixa::RefazerEscrita(id.clone()));
    assert_eq!(caixa::contar(&b.caixa.escritas().unwrap()), (2, 0));
    // Descartar uma recusada some com ela.
    let nova = b.caixa.escritas().unwrap()[0].id.clone();
    b.caixa.servidor_recusa(&nova, "de novo", vec![]);
    b.pedir(PedidoDaCaixa::DescartarEscrita(nova));
    assert_eq!(caixa::contar(&b.caixa.escritas().unwrap()), (1, 0));
    b.abrir(Destino::Area(Area::Sincronizacao));
}

#[test]
fn enviar_ao_servidor_habilita_com_lote_e_anda_com_a_barra() {
    let mut b = Banco::novo();
    let lote = Ref::nova(Tipo::Lote, "1");
    let enviar = |b: &Banco| {
        bancada_casca::AppCasca::acoes(&b.casca.app)
            .iter()
            .find(|a| a.id == "lotes.enviar")
            .and_then(|a| a.motivo().map(str::to_owned))
    };
    assert_eq!(enviar(&b).as_deref(), Some("Escolha um lote"));
    b.casca.app.janela().lote_atual = Some(lote.clone());
    b.quadro();
    assert_eq!(enviar(&b), None, "habilitada com um lote pronto");
    b.abrir(Destino::Area(Area::Lotes));
    b.casca.app.janela().executar(IdAcao::EnviarAoServidor);
    b.quadro();
    b.quadro();
    assert!(matches!(
        b.caixa.envio_do_lote(&lote),
        EnvioDoLote::Enviando(_)
    ));
    assert_eq!(enviar(&b).as_deref(), Some("O envio está em andamento"));
    for _ in 0..5 {
        b.caixa.avancar_envio();
        b.quadro();
    }
    assert_eq!(
        b.caixa.envio_do_lote(&lote),
        EnvioDoLote::Enviado { documentos: 5 }
    );
    // Parou no meio: retoma.
    let outro = Ref::nova(Tipo::Lote, "2");
    b.casca.app.janela().lote_atual = Some(outro.clone());
    b.pedir(PedidoDaCaixa::EnviarLote(outro.clone()));
    b.caixa.avancar_envio();
    b.caixa.derrubar_envio(&outro, "sem rede");
    b.quadro();
    assert_eq!(enviar(&b), None, "falhou: pode retomar");
    b.pedir(PedidoDaCaixa::EnviarLote(outro.clone()));
    assert!(matches!(
        b.caixa.envio_do_lote(&outro),
        EnvioDoLote::Enviando(p) if p.enviados == 1
    ));
}

#[test]
fn sem_caixa_ligada_a_area_mostra_o_selo_e_enviar_fica_desabilitado() {
    let (inventario, comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let app = AppSbf::novo(
        Janela::nova(),
        Box::new(inventario),
        Box::new(comandos),
        EstadoDaBarra::default(),
    );
    let mut casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
    let mut quadro = || {
        let mut saida = ctx.run_ui(entrada(Vec::new()), |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
    };
    quadro();
    bancada_casca::abrir(&ctx, "caixa");
    quadro();
    quadro();
    let motivo = bancada_casca::AppCasca::acoes(&casca.app)
        .iter()
        .find(|a| a.id == "lotes.enviar")
        .and_then(|a| a.motivo().map(str::to_owned));
    assert_eq!(motivo.as_deref(), Some("Ainda não ligado a esta janela"));
}
