//! Testes sem janela (INV-14/19, parcial): desenham com `egui::Context::run_ui` a moldura,
//! toda área, a página de todo `Tipo` e os destinos de total com os dados de
//! demonstração, sem pânico.

use sbf_inventario::consulta::{Area, Celula, Filtro, Fonte, PedidoDeTabela};
use sbf_inventario::{Consulta, Ref, Tipo};
use sbf_janela::demonstracao::{self, ComandosDemonstracao, ConsultaDemonstracao};
use sbf_janela::{Destino, EstadoDaBarra, IdAcao, Janela, MolduraProvisoria, Pedido};

fn entrada() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    }
}

struct Banco {
    ctx: egui::Context,
    moldura: MolduraProvisoria,
    consulta: ConsultaDemonstracao,
    comandos: ComandosDemonstracao,
}

impl Banco {
    fn novo() -> Self {
        let (consulta, comandos) = demonstracao::nova();
        let ctx = egui::Context::default();
        let mut janela = Janela::nova();
        janela.hoje = "2026-11-13".into();
        let moldura = MolduraProvisoria::nova(janela, EstadoDaBarra::default());
        moldura.preparar(&ctx);
        Self {
            ctx,
            moldura,
            consulta,
            comandos,
        }
    }

    fn quadro(&mut self) {
        let Banco {
            ctx,
            moldura,
            consulta,
            comandos,
        } = self;
        let mut saida = ctx.run_ui(entrada(), |ui| moldura.ui(ui, consulta, comandos));
        saida.textures_delta.clear();
    }

    fn abrir(&mut self, destino: Destino) {
        self.moldura.janela.pedir(Pedido::Abrir(destino.clone()));
        self.quadro();
        assert_eq!(self.moldura.janela.navegacao.atual(), &destino);
        self.quadro();
    }
}

#[test]
fn varios_quadros_seguidos_nao_travam() {
    let mut b = Banco::novo();
    for _ in 0..5 {
        b.quadro();
    }
}

#[test]
fn toda_area_desenha() {
    let mut b = Banco::novo();
    for area in sbf_janela::areas::areas_da_fatia() {
        b.abrir(Destino::Area(area));
    }
    for a in sbf_janela::areas::AREAS_FUTURAS {
        b.abrir(Destino::AreaFutura(a.id));
    }
}

fn um_de_cada_tipo(c: &ConsultaDemonstracao) -> Vec<Ref> {
    let mut refs: Vec<Ref> = Vec::new();
    for area in sbf_janela::areas::AREAS_DO_INVENTARIO {
        let t = c
            .tabela(&PedidoDeTabela::Area {
                area,
                filtro: Filtro::default(),
            })
            .expect("tabela");
        for l in &t.linhas {
            for cel in l.celulas.iter() {
                let mut add = |r: &Ref| {
                    if !refs.iter().any(|x| x.tipo == r.tipo) {
                        refs.push(r.clone());
                    }
                };
                match cel {
                    Celula::Link { alvo, .. } => add(alvo),
                    Celula::Links(v) => v.iter().for_each(|(r, _)| add(r)),
                    _ => {}
                }
            }
        }
    }
    refs.push(Ref::nova(Tipo::Decisao, "1"));
    refs.push(Ref::nova(Tipo::Pendencia, "1"));
    refs.push(Ref::nova(Tipo::RegraDeSenha, "1"));
    refs
}

#[test]
fn a_pagina_de_todo_tipo_desenha_em_todas_as_abas() {
    let mut b = Banco::novo();
    let mut refs = um_de_cada_tipo(&b.consulta);
    // Os tipos sem dados na demonstração (fatias futuras) abrem a página do selo.
    for t in Tipo::TODOS {
        if !refs.iter().any(|r| r.tipo == t) {
            refs.push(Ref::nova(t, "exemplo"));
        }
    }
    for r in refs {
        b.abrir(Destino::Item(r.clone()));
        let abas = sbf_janela::pagina::abas_de(r.tipo);
        for i in 0..abas.len() {
            b.moldura.janela.centro.abas.insert(r.clone(), i);
            b.quadro();
        }
    }
}

#[test]
fn totais_flutuantes_e_dialogos_desenham() {
    let mut b = Banco::novo();
    let f = Ref::nova(Tipo::Fornecedor, "Energia Exemplo");
    b.abrir(Destino::Item(f.clone()));
    b.abrir(Destino::Total {
        titulo: "Total".into(),
        pedido: PedidoDeTabela::ItensDoTotal {
            de: f.clone(),
            fonte: Fonte::Inventario,
            ciclo: None,
            recorte: None,
        },
    });
    b.moldura
        .janela
        .pedir(Pedido::AbrirFlutuante(Destino::Item(Ref::nova(
            Tipo::Ciclo,
            "2026-10",
        ))));
    b.quadro();
    assert_eq!(b.moldura.janela.navegacao.flutuantes().len(), 1);
    b.quadro();
    // Diálogos.
    b.abrir(Destino::Area(Area::Registros));
    b.moldura
        .janela
        .centro
        .selecao
        .so(Ref::nova(Tipo::Registro, "1"));
    for id in [
        IdAcao::Rejeitar,
        IdAcao::CorrigirCategoria,
        IdAcao::NovoLote,
        IdAcao::Sinalizar,
    ] {
        b.moldura.janela.executar(id);
        b.quadro();
        assert!(b.moldura.janela.dialogo.is_some(), "{id:?} sem diálogo");
        b.quadro();
        b.moldura.janela.dialogo = None;
    }
    // Todos os temas desenham.
    for t in sbf_janela::tema::temas() {
        sbf_janela::tema::aplicar(&b.ctx, &t.id);
        b.quadro();
    }
}

#[test]
fn acoes_pela_janela_gravam_e_desfazem() {
    let mut b = Banco::novo();
    b.abrir(Destino::Area(Area::ATriar));
    let antes = b.consulta.contagens().expect("contagens");
    let t = b
        .consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::ATriar,
            filtro: Filtro::default(),
        })
        .expect("tabela");
    let alvo = t.linhas[0].alvo.clone().expect("alvo");
    b.moldura.janela.centro.selecao.so(alvo);
    b.moldura.janela.executar(IdAcao::Documento);
    b.quadro();
    assert_eq!(
        b.consulta.contagens().expect("contagens").a_triar,
        antes.a_triar - 1
    );
    assert_eq!(b.moldura.janela.decisoes_da_sessao().len(), 1);
    b.moldura.janela.executar(IdAcao::Desfazer);
    b.quadro();
    assert_eq!(b.consulta.contagens().expect("contagens"), antes);
    // Enviar ao servidor nunca roda.
    b.moldura.janela.executar(IdAcao::EnviarAoServidor);
    b.quadro();
    assert!(b.moldura.janela.mensagem.as_ref().is_some_and(|m| m.erro));
}

#[test]
fn lote_pela_janela_cria_adiciona_e_aprova() {
    let mut b = Banco::novo();
    b.moldura
        .janela
        .pedir(Pedido::CriarLote("Teste da janela".into()));
    b.quadro();
    let lote = b.moldura.janela.lote_atual.clone().expect("lote atual");
    b.abrir(Destino::Area(Area::Registros));
    let reg = Ref::nova(Tipo::Registro, "70");
    b.moldura.janela.centro.selecao.so(reg.clone());
    b.moldura.janela.executar(IdAcao::AdicionarSelecao);
    b.quadro();
    let itens = b
        .consulta
        .tabela(&PedidoDeTabela::Ligados {
            de: lote.clone(),
            tipo: Tipo::Registro,
        })
        .expect("tabela");
    assert_eq!(itens.linhas.len(), 1);
    b.moldura.janela.executar(IdAcao::AprovarLote);
    b.quadro();
    let item = b.consulta.item(&reg).expect("item").expect("existe");
    assert!(matches!(&item.campos[0].valor, Celula::Estado { rotulo, .. } if rotulo == "Aprovado"));
    // Clicar noutro lote troca o lote atual.
    let outro = Ref::nova(Tipo::Lote, "1");
    b.moldura.janela.centro.selecao.so(outro.clone());
    b.quadro();
    assert_eq!(b.moldura.janela.lote_atual, Some(outro));
}
