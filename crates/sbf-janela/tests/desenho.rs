//! Testes sem janela (INV-14/19, parcial): a janela dentro da `bancada-casca`, desenhada com
//! `egui::Context::run_ui`, com os dados de demonstração. Toda área, a página de todo `Tipo`
//! em todas as abas, totais, flutuantes, diálogos, temas, Ações e lotes, sem pânico.

use bancada_casca::{Casca, Local, Pedido as PedidoCasca};
use sbf_inventario::consulta::{Area, Celula, Filtro, Fonte, PedidoDeTabela};
use sbf_inventario::{Consulta, Ref, Tipo};
use sbf_janela::demonstracao::{self, ConsultaDemonstracao};
use sbf_janela::{AppSbf, Destino, EstadoDaBarra, IdAcao, Janela, MolduraProvisoria, Pedido};

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
    casca: Casca<AppSbf>,
    consulta: ConsultaDemonstracao,
}

impl Banco {
    fn novo() -> Self {
        let (consulta, comandos) = demonstracao::nova();
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = "2026-11-13".into();
        let app = AppSbf::novo(
            janela,
            Box::new(consulta.clone()),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self {
            ctx,
            casca,
            consulta,
        };
        b.quadro();
        b
    }

    fn quadro(&mut self) {
        let Banco { ctx, casca, .. } = self;
        let mut saida = ctx.run_ui(entrada(), |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
    }

    fn abrir(&mut self, destino: Destino) {
        bancada_casca::abrir(&self.ctx, destino.rota());
        self.quadro();
        self.quadro();
        assert_eq!(
            self.casca.estado.historico.atual().como_texto(),
            destino.rota()
        );
        assert_eq!(self.casca.app.janela().navegacao.atual(), &destino);
    }

    fn executar(&mut self, id: IdAcao) {
        bancada_casca::executar(&self.ctx, sbf_janela::acoes::definicao(id).codigo);
        self.quadro();
    }
}

#[test]
fn varios_quadros_seguidos_nao_travam_e_a_lista_de_acoes_vale() {
    let mut b = Banco::novo();
    for _ in 0..5 {
        b.quadro();
    }
    // A lista única entra inteira na casca, sem id repetido nem atalho em conflito.
    let acoes = bancada_casca::AppCasca::acoes(&b.casca.app);
    assert!(bancada_casca::ListaDeAcoes::nova(acoes).is_ok());
    // A casca não acusa erro (id, atalho, KeyTip ou aba inválidos).
    assert!(
        b.casca.estado.erros.is_empty(),
        "{:?}",
        b.casca.estado.erros
    );
    // E em toda área (as abas contextuais mudam com a seleção e a página).
    for area in sbf_janela::areas::areas_da_fatia() {
        b.abrir(Destino::Area(area));
        assert!(
            b.casca.estado.erros.is_empty(),
            "{area:?}: {:?}",
            b.casca.estado.erros
        );
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
            b.casca.app.janela().centro.abas.insert(r.clone(), i);
            b.quadro();
        }
    }
}

#[test]
fn totais_voltar_flutuantes_dialogos_e_temas() {
    let mut b = Banco::novo();
    let f = Ref::nova(Tipo::Fornecedor, "Energia Exemplo");
    b.abrir(Destino::Item(f.clone()));
    let total = Destino::Total {
        titulo: "Total".into(),
        pedido: PedidoDeTabela::ItensDoTotal {
            de: f.clone(),
            fonte: Fonte::Inventario,
            ciclo: None,
            recorte: None,
        },
    };
    b.abrir(total);
    // Voltar refaz o caminho (o histórico é o da casca; a janela acompanha).
    bancada_casca::pedir(&b.ctx, PedidoCasca::Voltar(Local::Centro));
    b.quadro();
    b.quadro();
    assert_eq!(
        b.casca.app.janela().navegacao.atual(),
        &Destino::Item(f.clone())
    );
    // Ctrl+clique: painel flutuante, sem mexer no centro.
    bancada_casca::abrir_flutuante(
        &b.ctx,
        Destino::Item(Ref::nova(Tipo::Ciclo, "2026-10")).rota(),
    );
    b.quadro();
    b.quadro();
    assert_eq!(b.casca.estado.flutuantes.len(), 1);
    assert_eq!(
        b.casca.estado.historico.atual().como_texto(),
        Destino::Item(f).rota()
    );
    // Diálogos.
    b.abrir(Destino::Area(Area::Registros));
    b.casca
        .app
        .janela()
        .centro
        .selecao
        .so(Ref::nova(Tipo::Registro, "1"));
    for id in [
        IdAcao::Rejeitar,
        IdAcao::CorrigirCategoria,
        IdAcao::NovoLote,
        IdAcao::Sinalizar,
    ] {
        b.executar(id);
        b.quadro();
        assert!(b.casca.app.janela().dialogo.is_some(), "{id:?} sem diálogo");
        b.casca.app.janela().dialogo = None;
    }
    // Todos os temas desenham.
    for t in sbf_janela::tema::temas() {
        bancada_casca::pedir(&b.ctx, PedidoCasca::Tema(t.id.clone()));
        b.quadro();
        b.quadro();
    }
}

#[test]
fn acoes_pela_casca_gravam_e_desfazem() {
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
    b.casca.app.janela().centro.selecao.so(alvo);
    b.executar(IdAcao::Documento);
    assert_eq!(
        b.consulta.contagens().expect("contagens").a_triar,
        antes.a_triar - 1
    );
    assert_eq!(b.casca.app.janela().decisoes_da_sessao().len(), 1);
    b.executar(IdAcao::Desfazer);
    assert_eq!(b.consulta.contagens().expect("contagens"), antes);
    // Enviar ao servidor fica desabilitado na lista e nunca roda.
    let acoes = bancada_casca::AppCasca::acoes(&b.casca.app);
    let enviar = acoes
        .iter()
        .find(|a| a.id == "lotes.enviar")
        .expect("enviar");
    assert_eq!(enviar.motivo(), Some("Ainda não ligado a esta janela"));
}

#[test]
fn lote_pela_janela_cria_adiciona_e_aprova() {
    let mut b = Banco::novo();
    b.casca
        .app
        .janela()
        .pedir(Pedido::CriarLote("Teste da janela".into()));
    b.abrir(Destino::Area(Area::Registros));
    let lote = b.casca.app.janela().lote_atual.clone().expect("lote atual");
    let reg = Ref::nova(Tipo::Registro, "70");
    b.casca.app.janela().centro.selecao.so(reg.clone());
    b.executar(IdAcao::AdicionarSelecao);
    let itens = b
        .consulta
        .tabela(&PedidoDeTabela::Ligados {
            de: lote.clone(),
            tipo: Tipo::Registro,
        })
        .expect("tabela");
    assert_eq!(itens.linhas.len(), 1);
    b.executar(IdAcao::AprovarLote);
    let item = b.consulta.item(&reg).expect("item").expect("existe");
    assert!(matches!(&item.campos[0].valor, Celula::Estado { rotulo, .. } if rotulo == "Aprovado"));
    // Clicar noutro lote troca o lote atual.
    let outro = Ref::nova(Tipo::Lote, "1");
    b.casca.app.janela().centro.selecao.so(outro.clone());
    b.quadro();
    assert_eq!(b.casca.app.janela().lote_atual, Some(outro));
}

/// A moldura provisória continua de pé (reserva, sem a casca).
#[test]
fn moldura_provisoria_ainda_desenha() {
    let (consulta, mut comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    let moldura = MolduraProvisoria::nova(Janela::nova(), EstadoDaBarra::default());
    moldura.preparar(&ctx);
    let mut moldura = moldura;
    for _ in 0..3 {
        let mut saida = ctx.run_ui(entrada(), |ui| moldura.ui(ui, &consulta, &mut comandos));
        saida.textures_delta.clear();
    }
}

#[test]
fn filtros_da_aba_inicio_chegam_na_consulta() {
    let mut b = Banco::novo();
    b.abrir(Destino::Area(Area::EmDuvida));
    b.executar(IdAcao::QuemVoce);
    let filtro = b.casca.app.janela().filtro();
    assert_eq!(
        filtro.quem_decidiu,
        Some(sbf_inventario::consulta::Quem::Usuario)
    );
    let t = b
        .consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::EmDuvida,
            filtro,
        })
        .expect("tabela");
    assert_eq!(t.linhas.len(), 1);
    b.executar(IdAcao::PeriodoAtual);
    assert_eq!(
        b.casca.app.janela().filtro().ciclos,
        Some(("2026-10".into(), "2026-10".into()))
    );
    b.executar(IdAcao::QuemTodos);
    b.executar(IdAcao::PeriodoTodos);
    assert_eq!(b.casca.app.janela().filtro(), Filtro::default());
}

fn conferir_keytips(app: &AppSbf) {
    use bancada_casca::AppCasca;
    let abas = app.abas();
    let mut acoes = app.acoes();
    // As Ações da casca (painéis, modos do Ribbon) entram como estão, sem KeyTip.
    let faltando: Vec<String> = abas
        .iter()
        .flat_map(|a| a.acoes().map(str::to_owned).collect::<Vec<_>>())
        .filter(|id| !acoes.iter().any(|x| &x.id == id))
        .collect();
    for id in faltando {
        acoes.push(bancada_casca::Acao::nova(id.clone(), id));
    }
    let lista = bancada_casca::ListaDeAcoes::nova(acoes).expect("lista única");
    bancada_casca::ribbon::validar_abas(&abas, &lista).expect("abas e KeyTips sem ambiguidade");
}

#[test]
fn keytips_sem_ambiguidade_em_todas_as_abas() {
    let mut b = Banco::novo();
    conferir_keytips(&b.casca.app);
    // Com as abas contextuais visíveis: um achado selecionado numa página aberta.
    b.abrir(Destino::Item(Ref::nova(
        Tipo::Fornecedor,
        "Energia Exemplo",
    )));
    let t = b
        .consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::ATriar,
            filtro: Filtro::default(),
        })
        .expect("tabela");
    let achado = t.linhas[0].alvo.clone().expect("alvo");
    b.casca.app.janela().centro.selecao.so(achado);
    b.quadro();
    conferir_keytips(&b.casca.app);
    assert!(
        b.casca.estado.erros.is_empty(),
        "{:?}",
        b.casca.estado.erros
    );
}

#[test]
fn na_area_a_triar_o_ribbon_abre_na_triagem_e_inicio_tem_os_filtros() {
    use bancada_casca::AppCasca;
    let mut b = Banco::novo();
    b.abrir(Destino::Area(Area::ATriar));
    b.quadro();
    assert_eq!(b.casca.estado.ribbon.aba_ativa.as_deref(), Some("triagem"));
    let abas = b.casca.app.abas();
    let inicio = abas.iter().find(|a| a.id == "inicio").expect("início");
    let ids: Vec<&str> = inicio.acoes().collect();
    for id in [
        "inicio.pesquisar",
        "inicio.periodo",
        "inicio.estado",
        "inicio.tipo",
        "inicio.quem-decidiu",
        "dados.atualizar",
    ] {
        assert!(ids.contains(&id), "falta {id} em Início: {ids:?}");
    }
}

#[test]
fn intervalo_conexao_tema_previa_e_nova_regra() {
    use bancada_casca::AppCasca;
    use sbf_janela::janela::{Dialogo, confirmar_dialogo};
    let mut b = Banco::novo();
    b.abrir(Destino::Area(Area::Documentos));
    // Período › Intervalo…
    b.executar(IdAcao::PeriodoIntervalo);
    let d = b.casca.app.janela().dialogo.clone().expect("diálogo");
    assert!(matches!(d, Dialogo::Intervalo { .. }));
    let ruim = Dialogo::Intervalo {
        de: "2026-10".into(),
        ate: "2026-05".into(),
        erro: None,
    };
    assert!(confirmar_dialogo(&ruim).is_err());
    let bom = Dialogo::Intervalo {
        de: "2026-08".into(),
        ate: "2026-10".into(),
        erro: None,
    };
    b.casca.app.janela().dialogo = None;
    b.casca
        .app
        .janela()
        .pedir(confirmar_dialogo(&bom).expect("pedido"));
    b.quadro();
    assert_eq!(
        b.casca.app.janela().filtro().ciclos,
        Some(("2026-08".into(), "2026-10".into()))
    );
    // Conexão: as Ações vêm das conexões das coletas.
    let acoes = b.casca.app.acoes();
    let disco = acoes
        .iter()
        .find(|a| a.id == "inicio.fonte.disco:d")
        .expect("conexão Disco D");
    assert_eq!(disco.rotulo, "Disco D");
    assert!(
        b.casca.estado.erros.is_empty(),
        "{:?}",
        b.casca.estado.erros
    );
    bancada_casca::executar(&b.ctx, "inicio.fonte.disco:d");
    b.quadro();
    let filtro = b.casca.app.janela().filtro();
    assert_eq!(filtro.fontes, vec!["disco:d".to_owned()]);
    let t = b
        .consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::Documentos,
            filtro,
        })
        .expect("tabela");
    assert!(!t.linhas.is_empty());
    // Tema por menu: os 7.
    let acoes = b.casca.app.acoes();
    assert_eq!(
        acoes
            .iter()
            .filter(|a| a.id.starts_with("exibir.tema."))
            .count(),
        7
    );
    // Prévia: um painel a mais no dock.
    assert!(b.casca.app.paineis().iter().any(|p| p.id == "previa"));
    // Nova regra: grava pelos Comandos.
    b.executar(IdAcao::NovaRegraDeSenha);
    let nova = Dialogo::NovaRegra {
        fornecedor: "Banco Beta".into(),
        tipo_de_documento: "fatura_cartao".into(),
        estrategia: "cpf_ultimos:5".into(),
        prioridade: "50".into(),
        erro: None,
    };
    assert!(
        confirmar_dialogo(&Dialogo::NovaRegra {
            fornecedor: "Banco Beta".into(),
            tipo_de_documento: "fatura".into(),
            estrategia: "palpite".into(),
            prioridade: "100".into(),
            erro: None
        })
        .is_err()
    );
    b.casca.app.janela().dialogo = None;
    b.casca
        .app
        .janela()
        .pedir(confirmar_dialogo(&nova).expect("pedido"));
    b.quadro();
    let regras = b
        .consulta
        .tabela(&PedidoDeTabela::Ligados {
            de: Ref::nova(Tipo::Fornecedor, "Banco Beta"),
            tipo: Tipo::RegraDeSenha,
        })
        .expect("regras");
    assert_eq!(regras.linhas.len(), 1);
    // Prioridade que não é número não passa.
    assert!(
        confirmar_dialogo(&Dialogo::NovaRegra {
            fornecedor: "Banco Beta".into(),
            tipo_de_documento: "fatura".into(),
            estrategia: "cpf_completo".into(),
            prioridade: "alta".into(),
            erro: None
        })
        .is_err()
    );
}

#[test]
fn opcoes_ia_com_chaves_e_contas_desenha_em_todos_os_temas() {
    use sbf_janela::chaves_ia::GuardaFicticia;
    let mut b = Banco::novo();
    b.casca
        .app
        .janela()
        .ligar_triagem(Box::new(sbf_triagem::servico::TriagemFicticia::nova()));
    for (guarda, claude) in [
        (None, None),
        (
            Some(GuardaFicticia::com_chave(
                "openrouter",
                "sk-or-v1-ficticia-0000a1b2",
            )),
            Some(std::path::PathBuf::from("/casa/exemplo/.local/bin/claude")),
        ),
    ] {
        b.casca.app.janela().ligar_chaves_de_ia(
            guarda.map(|g| Box::new(g) as Box<dyn sbf_janela::chaves_ia::GuardaDeChaves>),
            claude,
        );
        b.casca.estado.ribbon.bastidores_abertos = true;
        b.casca.estado.secao_bastidores = Some("opcoes".into());
        for t in sbf_janela::tema::temas() {
            bancada_casca::pedir(&b.ctx, PedidoCasca::Tema(t.id.clone()));
            b.quadro();
            b.quadro();
        }
    }
    // Apagar com confirmação na tela.
    b.casca
        .app
        .janela()
        .triagem
        .chaves
        .pedir_para_apagar("openrouter");
    b.quadro();
}

#[test]
fn perguntar_e_pontos_desenham_sem_servico_e_com_o_da_demonstracao() {
    let mut b = Banco::novo();
    // Sem serviço: o aviso de que não está ligado.
    for rota in ["perguntar", "pontos"] {
        bancada_casca::abrir(&b.ctx, rota.to_owned());
        b.quadro();
        b.quadro();
    }
    let docs: Vec<Ref> = (0..6)
        .map(|i| Ref::nova(Tipo::Achado, format!("hash-demo-{i}")))
        .collect();
    b.casca
        .app
        .janela()
        .ligar_perguntar(sbf_janela::analise_ficticia::perguntar_da_demonstracao(
            &docs,
        ));
    b.casca
        .app
        .janela()
        .ligar_pontos(sbf_janela::analise_ficticia::pontos_da_demonstracao(&docs));
    bancada_casca::abrir(&b.ctx, "perguntar".to_owned());
    b.quadro();
    let ctx = b.ctx.clone();
    b.casca
        .app
        .janela()
        .perguntar
        .perguntar(&ctx, "fatura de energia");
    for _ in 0..200 {
        b.quadro();
        if !b.casca.app.janela().perguntar.trocas.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(b.casca.app.janela().perguntar.trocas.len(), 1);
    assert!(b.casca.app.janela().perguntar.trocas[0].resposta.is_ok());
    bancada_casca::abrir(&b.ctx, "pontos".to_owned());
    for t in sbf_janela::tema::temas() {
        bancada_casca::pedir(&b.ctx, PedidoCasca::Tema(t.id.clone()));
        b.quadro();
        b.quadro();
    }
    assert_eq!(b.casca.app.janela().pontos.total_aberto(), 6);
    bancada_casca::abrir(&b.ctx, "perguntar".to_owned());
    b.quadro();
}
