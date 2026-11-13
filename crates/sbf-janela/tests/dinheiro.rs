//! Testes sem janela das áreas de Dinheiro e da Sincronização (spec 07), com o livro-caixa
//! fictício da demonstração: toda área e a página de todo tipo de dinheiro desenham, mudar
//! categoria gera alteração, a recusa mostra o lado a lado e offline mostra a hora.

use bancada_casca::Casca;
use sbf_inventario::alteracao::{Alteracoes, EstadoDaAlteracao, EstadoDaRede, Sincronizacao};
use sbf_inventario::consulta::{Area, Celula, Filtro, PedidoDeTabela};
use sbf_inventario::{Consulta, Ref, Tipo};
use sbf_janela::livro_ficticio::{self, LivroFicticio};
use sbf_janela::{AppSbf, ConsultaComposta, Destino, EstadoDaBarra, Janela, Pedido, demonstracao};

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
    livro: LivroFicticio,
}

impl Banco {
    fn novo() -> Self {
        let (inventario, comandos) = demonstracao::nova();
        let livro = livro_ficticio::novo();
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = livro_ficticio::HOJE.into();
        janela.ligar_estacao(Box::new(livro.clone()));
        let consulta = ConsultaComposta::nova(Box::new(inventario), Some(Box::new(livro.clone())));
        let app = AppSbf::novo(
            janela,
            Box::new(consulta),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self { ctx, casca, livro };
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
        assert_eq!(self.casca.app.janela().navegacao.atual(), &destino);
        assert!(
            self.casca.estado.erros.is_empty(),
            "{:?}",
            self.casca.estado.erros
        );
    }

    fn pedir(&mut self, p: Pedido) {
        self.casca.app.janela().pedir(p);
        self.quadro();
    }
}

fn uma_movimentacao(livro: &LivroFicticio, descricao: &str) -> Ref {
    let t = livro
        .tabela(&PedidoDeTabela::Area {
            area: Area::Movimentacoes,
            filtro: Filtro::default(),
        })
        .expect("movimentações");
    t.linhas
        .iter()
        .find(|l| matches!(&l.celulas[1], Celula::Link { texto, .. } if texto == descricao))
        .and_then(|l| l.alvo.clone())
        .expect("movimentação")
}

#[test]
fn toda_area_de_dinheiro_e_a_sincronizacao_desenham() {
    let mut b = Banco::novo();
    for area in Area::DO_LIVRO_CAIXA {
        b.abrir(Destino::Area(area));
        let painel = b
            .livro
            .painel_da_area(area, &Filtro::default())
            .expect("painel");
        if area != Area::FornecedoresDoLivro {
            assert!(!painel.indicadores.is_empty(), "{area:?} sem números");
        }
    }
    b.abrir(Destino::Area(Area::Sincronizacao));
    b.abrir(Destino::Area(Area::Relatorios));
}

#[test]
fn hoje_tem_para_resolver_e_a_marca_de_estimativa() {
    let b = Banco::novo();
    let hoje = b
        .livro
        .painel_da_area(Area::Hoje, &Filtro::default())
        .expect("hoje");
    assert_eq!(hoje.secoes[0].titulo, "Para resolver");
    let rotulos: Vec<&str> = hoje.indicadores.iter().map(|i| i.rotulo.as_str()).collect();
    for r in [
        "Disponível agora",
        "Livre após compromissos",
        "Resultado do mês",
    ] {
        assert!(rotulos.contains(&r), "falta {r}");
    }
    let livre = &hoje.indicadores[1];
    assert!(livre.estimativa.is_some(), "a conta de água não tem valor");
    assert!(hoje.indicadores[0].instantaneo);
    // Todo número do topo é clicável.
    for i in &hoje.indicadores {
        assert!(
            matches!(&i.valor, Celula::Valor { itens: Some(_), .. }),
            "{} não é clicável",
            i.rotulo
        );
    }
}

#[test]
fn a_pagina_de_todo_tipo_de_dinheiro_desenha_em_todas_as_abas() {
    let mut b = Banco::novo();
    let mut refs: Vec<Ref> = Vec::new();
    let mut juntar = |t: &sbf_inventario::consulta::Tabela| {
        for l in &t.linhas {
            for c in &l.celulas {
                let mut add = |r: &Ref| {
                    if !refs.iter().any(|x| x.tipo == r.tipo) {
                        refs.push(r.clone());
                    }
                };
                match c {
                    Celula::Link { alvo, .. } => add(alvo),
                    Celula::Links(v) => v.iter().for_each(|(r, _)| add(r)),
                    _ => {}
                }
            }
        }
    };
    for area in Area::DO_LIVRO_CAIXA {
        if let Ok(t) = b.livro.tabela(&PedidoDeTabela::Area {
            area,
            filtro: Filtro::default(),
        }) {
            juntar(&t);
        }
        if let Ok(p) = b.livro.painel_da_area(area, &Filtro::default()) {
            p.secoes.iter().for_each(|s| juntar(&s.tabela));
        }
    }
    let ciclo = Ref::nova(Tipo::Ciclo, livro_ficticio::CICLO_ATUAL);
    juntar(
        &b.livro
            .tabela(&PedidoDeTabela::Ligados {
                de: ciclo,
                tipo: Tipo::Transferencia,
            })
            .expect("transferências"),
    );
    refs.push(Ref::nova(
        Tipo::Estacao,
        "5bf00000-0000-4000-8000-000000000961",
    ));
    refs.push(Ref::nova(Tipo::Alteracao, "2"));
    for tipo in [
        Tipo::Movimentacao,
        Tipo::Conta,
        Tipo::Instituicao,
        Tipo::Cartao,
        Tipo::Fatura,
        Tipo::Categoria,
        Tipo::Centro,
        Tipo::Compromisso,
        Tipo::Recorrente,
        Tipo::Divida,
        Tipo::Parcela,
        Tipo::Fornecedor,
        Tipo::Produto,
        Tipo::Transferencia,
    ] {
        assert!(refs.iter().any(|r| r.tipo == tipo), "falta {tipo}");
    }
    for r in refs {
        b.abrir(Destino::Item(r.clone()));
        for aba in 0..8 {
            b.casca.app.janela().centro.abas.insert(r.clone(), aba);
            b.quadro();
        }
    }
}

#[test]
fn o_rastro_da_movimentacao_chega_aos_centros() {
    let b = Banco::novo();
    let escola = uma_movimentacao(&b.livro, "Escola Gama");
    let rastro = b.livro.rastro(&escola).expect("rastro");
    let centros: Vec<&str> = rastro
        .abaixo
        .iter()
        .filter(|e| e.alvo.tipo == Tipo::Centro)
        .map(|e| e.rotulo.as_str())
        .collect();
    assert!(centros.contains(&"Projeto Ômega") && centros.contains(&"Família"));
}

#[test]
fn mudar_categoria_gera_alteracao_aguardando_e_a_troca_aceita() {
    let mut b = Banco::novo();
    let pix = uma_movimentacao(&b.livro, "PIX enviado");
    assert_eq!(b.livro.categoria_de(&pix), None);
    let lazer = b
        .livro
        .categorias()
        .expect("categorias")
        .into_iter()
        .find(|c| c.nome == "Lazer")
        .expect("lazer");
    b.pedir(Pedido::Alterar(
        sbf_inventario::alteracao::PedidoDeAlteracao::MudarCategoria {
            movimentacao: pix.clone(),
            categoria: Some(lazer.alvo),
        },
    ));
    let aguardando = b.livro.alteracoes(true).expect("lista");
    assert!(
        aguardando
            .iter()
            .any(|a| a.estado == EstadoDaAlteracao::Aguardando && a.pedido.alvo() == &pix)
    );
    // Ainda não foi ao servidor: a cópia continua igual.
    assert_eq!(b.livro.categoria_de(&pix), None);
    b.abrir(Destino::Area(Area::Sincronizacao));
    b.pedir(Pedido::Sincronizar);
    assert_eq!(b.livro.categoria_de(&pix).as_deref(), Some("Lazer"));
    let a = b
        .livro
        .alteracoes(false)
        .expect("lista")
        .into_iter()
        .find(|a| a.pedido.alvo() == &pix)
        .expect("alteração");
    assert_eq!(a.estado, EstadoDaAlteracao::Aceita);
}

#[test]
fn recusa_mostra_o_lado_a_lado_e_refazer_aplica_sobre_a_nova() {
    let mut b = Banco::novo();
    let recusada = b
        .livro
        .alteracoes(true)
        .expect("lista")
        .into_iter()
        .find(|a| a.estado == EstadoDaAlteracao::Recusada)
        .expect("a demonstração traz uma recusada");
    let recusa = recusada.recusa.clone().expect("recusa");
    assert!(recusa.campos.iter().any(|c| c.difere()));
    assert!(recusa.campos.iter().any(|c| !c.difere()));
    // A área e a página desenham o lado a lado.
    b.abrir(Destino::Area(Area::Sincronizacao));
    b.abrir(Destino::Item(recusada.alvo_da_pagina()));
    // Refazer pela Ação do Ribbon, com a recusada aberta.
    b.casca
        .app
        .janela()
        .executar(sbf_janela::IdAcao::RefazerAlteracao);
    b.quadro();
    let todas = b.livro.alteracoes(false).expect("lista");
    let velha = todas.iter().find(|a| a.id == recusada.id).expect("velha");
    assert_eq!(velha.estado, EstadoDaAlteracao::Refeita);
    let nova = velha.refeita_em.clone().expect("a nova");
    b.pedir(Pedido::Sincronizar);
    let nova = b.livro.alteracao(&nova).expect("ok").expect("nova");
    assert_eq!(nova.estado, EstadoDaAlteracao::Aceita);
    assert_eq!(
        b.livro.categoria_de(nova.pedido.alvo()).as_deref(),
        Some("Casa")
    );
}

#[test]
fn mudanca_sobre_item_que_mudou_no_servidor_e_recusada_e_pode_ser_descartada() {
    let mut b = Banco::novo();
    let mercado = uma_movimentacao(&b.livro, "Estorno Mercado Delta");
    b.livro.mudar_no_servidor(&mercado, 2_990);
    let casa = b
        .livro
        .categorias()
        .expect("categorias")
        .into_iter()
        .find(|c| c.nome == "Casa")
        .expect("casa");
    b.pedir(Pedido::Alterar(
        sbf_inventario::alteracao::PedidoDeAlteracao::MudarCategoria {
            movimentacao: mercado.clone(),
            categoria: Some(casa.alvo),
        },
    ));
    b.pedir(Pedido::Sincronizar);
    let a = b
        .livro
        .alteracoes(true)
        .expect("lista")
        .into_iter()
        .find(|a| a.pedido.alvo() == &mercado)
        .expect("recusada");
    assert_eq!(a.estado, EstadoDaAlteracao::Recusada);
    let campos = a.recusa.as_ref().expect("recusa").campos.clone();
    let valor = campos.iter().find(|c| c.rotulo == "Valor").expect("valor");
    assert!(valor.difere());
    b.abrir(Destino::Item(a.alvo_da_pagina()));
    b.pedir(Pedido::DescartarAlteracao(a.id.clone()));
    let a = b.livro.alteracao(&a.id).expect("ok").expect("a");
    assert_eq!(a.estado, EstadoDaAlteracao::Descartada);
    assert_eq!(b.livro.categoria_de(&mercado).as_deref(), Some("Mercado"));
}

#[test]
fn offline_mostra_a_hora_da_copia() {
    let mut b = Banco::novo();
    b.livro.definir_rede(EstadoDaRede::SemRede);
    let estado = b.livro.estado();
    assert_eq!(
        sbf_janela::dinheiro::texto_da_barra(&estado, livro_ficticio::HOJE).0,
        "Sem rede, mostrando a cópia de 08:12"
    );
    assert_eq!(
        sbf_janela::dinheiro::hora_da_copia(Some(&estado), livro_ficticio::HOJE).as_deref(),
        Some("08:12")
    );
    for area in Area::DO_LIVRO_CAIXA {
        b.abrir(Destino::Area(area));
    }
    // A barra de status diz o estado.
    let status = bancada_casca::AppCasca::status(&b.casca.app);
    assert!(
        status
            .iter()
            .any(|s| s.texto == "Sem rede, mostrando a cópia de 08:12"),
        "a barra não mostra a cópia"
    );
    // Sincronizar sem rede não muda a cópia e registra a falha.
    b.pedir(Pedido::Sincronizar);
    let h = b.livro.historico().expect("histórico");
    assert!(h[0].erro.is_some());
}

#[test]
fn estacao_renomeada_e_revogada() {
    let mut b = Banco::novo();
    let outra = b
        .livro
        .estacoes()
        .expect("estações")
        .into_iter()
        .find(|e| !e.esta && e.credencial == sbf_inventario::alteracao::Credencial::Ativa)
        .expect("outra");
    b.pedir(Pedido::RenomearEstacao(
        outra.alvo.clone(),
        "Escritório".into(),
    ));
    b.pedir(Pedido::RevogarEstacao(outra.alvo.clone()));
    let e = b
        .livro
        .estacoes()
        .expect("estações")
        .into_iter()
        .find(|e| e.alvo == outra.alvo)
        .expect("e");
    assert_eq!(e.nome, "Escritório");
    assert_eq!(
        e.credencial,
        sbf_inventario::alteracao::Credencial::Revogada
    );
    b.abrir(Destino::Item(outra.alvo));
}

#[test]
fn sem_estacao_as_areas_de_dinheiro_dizem_o_motivo() {
    let (inventario, comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let consulta = ConsultaComposta::nova(Box::new(inventario), None);
    let erro = consulta
        .painel_da_area(Area::Hoje, &Filtro::default())
        .expect_err("sem livro");
    assert!(erro.to_string().contains("Sincronização"));
    let app = AppSbf::novo(
        Janela::nova(),
        Box::new(consulta),
        Box::new(comandos),
        EstadoDaBarra::default(),
    );
    let mut casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
    for area in Area::DO_LIVRO_CAIXA
        .into_iter()
        .chain([Area::Sincronizacao, Area::Relatorios])
    {
        bancada_casca::abrir(&ctx, Destino::Area(area).rota());
        for _ in 0..2 {
            let mut s = ctx.run_ui(entrada(), |ui| casca.desenhar(ui));
            s.textures_delta.clear();
        }
    }
}
