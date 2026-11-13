//! A janela do SBF: o estado da interface (navegação, seleção, filtros, lote atual,
//! diálogos, mensagens) e as vistas que a casca põe nos seus lugares.
//!
//! Regra do quadro: as vistas só **leem** (`&dyn Consulta`, com cache) e emitem
//! [`Pedido`]s. As escritas acontecem em [`Janela::processar`], depois do desenho, com
//! `&mut dyn Comandos`; nenhuma transação atravessa um quadro (spec 06, LICOES.md).

use std::collections::HashMap;

use egui::{Align, Context, Id, Layout, PointerButton, RichText, Ui};
use sbf_inventario::comando::{Acao, IdDecisao, PedidoDeDecisao};
use sbf_inventario::consulta::{Area, Contagens, Filtro, PedidoDeTabela};
use sbf_inventario::{Comandos, Consulta, Ref, Tipo};

use crate::acoes::{self, ContextoDasAcoes, IdAcao};
use crate::areas;
use crate::componentes;
use crate::dock::{self, PainelDoDock};
use crate::formato;
use crate::icones;
use crate::leitura::{Cache, Leitor};
use crate::navegacao::{Destino, Navegacao, Pedido};
use crate::pagina::{self, Vista};
use crate::painel::{self, EstadoDosRelatorios};
use crate::tabela;
use crate::tema;

/// O período do filtro (aba Início).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Periodo {
    CicloAtual,
    CicloAnterior,
    #[default]
    TodosOsCiclos,
}

/// Uma mensagem curta no topo do centro (resultado da última Ação).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mensagem {
    pub texto: String,
    pub erro: bool,
}

/// O que outros crates ligam à janela (o integrador marca o que existe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ligacoes {
    pub pdf: bool,
    pub estante: bool,
    pub cofre: bool,
}

/// Um diálogo de Ação que pede dados.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialogo {
    Texto {
        acao: IdAcao,
        alvos: Vec<Ref>,
        rotulo: String,
        valor: String,
    },
    DataEValor {
        alvos: Vec<Ref>,
        data: String,
        valor: String,
        erro: Option<String>,
    },
    NovoLote {
        nome: String,
    },
}

/// A janela do SBF.
#[derive(Debug)]
pub struct Janela {
    pub navegacao: Navegacao,
    pub centro: Vista,
    flutuantes: HashMap<u64, Vista>,
    cache: Cache,
    pesquisa: String,
    pub periodo: Periodo,
    focar_pesquisa: bool,
    relatorios: EstadoDosRelatorios,
    pub lote_atual: Option<Ref>,
    pub paineis_visiveis: Vec<PainelDoDock>,
    pedidos: Vec<Pedido>,
    pub dialogo: Option<Dialogo>,
    pub mensagem: Option<Mensagem>,
    decisoes_da_sessao: Vec<IdDecisao>,
    pub tema: String,
    pub ligacoes: Ligacoes,
    /// "Hoje" em `AAAA-MM-DD` (o ciclo atual sai daqui). Fixável para teste.
    pub hoje: String,
}

impl Default for Janela {
    fn default() -> Self {
        Self::nova()
    }
}

impl Janela {
    pub fn nova() -> Self {
        Self {
            navegacao: Navegacao::nova(Destino::Area(Area::VisaoGeral)),
            centro: Vista::default(),
            flutuantes: HashMap::new(),
            cache: Cache::default(),
            pesquisa: String::new(),
            periodo: Periodo::default(),
            focar_pesquisa: false,
            relatorios: EstadoDosRelatorios::default(),
            lote_atual: None,
            paineis_visiveis: vec![PainelDoDock::Detalhe, PainelDoDock::Historico],
            pedidos: Vec::new(),
            dialogo: None,
            mensagem: None,
            decisoes_da_sessao: Vec::new(),
            tema: tema::TEMA_PADRAO.to_owned(),
            ligacoes: Ligacoes::default(),
            hoje: hoje_em_sao_paulo(),
        }
    }

    /// Enfileira um pedido (a casca chama isto para os botões do Ribbon e da Paleta).
    pub fn pedir(&mut self, pedido: Pedido) {
        self.pedidos.push(pedido);
    }

    /// Enfileira uma Ação da lista única.
    pub fn executar(&mut self, id: IdAcao) {
        self.pedidos.push(Pedido::Executar(id));
    }

    /// As decisões que você tomou nesta sessão.
    pub fn decisoes_da_sessao(&self) -> &[IdDecisao] {
        &self.decisoes_da_sessao
    }

    /// O item aberto no centro, se for uma página.
    pub fn item_aberto(&self) -> Option<&Ref> {
        match self.navegacao.atual() {
            Destino::Item(r) => Some(r),
            _ => None,
        }
    }

    /// O item que o dock mostra: a última linha clicada, ou a página aberta.
    pub fn foco(&self) -> Option<Ref> {
        self.centro
            .selecao
            .foco
            .clone()
            .or_else(|| self.item_aberto().cloned())
    }

    /// A seleção que as Ações usam: a da tabela ou, sem ela, o item aberto.
    pub fn selecao(&self) -> Vec<Ref> {
        if self.centro.selecao.itens.is_empty() {
            self.item_aberto().cloned().into_iter().collect()
        } else {
            self.centro.selecao.itens.clone()
        }
    }

    /// O contexto para habilitar as Ações (Ribbon, Paleta, faixa provisória).
    pub fn contexto_das_acoes(&self) -> ContextoDasAcoes {
        ContextoDasAcoes {
            selecao: self.selecao(),
            lote_atual: self.lote_atual.clone(),
            pode_voltar: self.navegacao.pode_voltar(),
            pode_avancar: self.navegacao.pode_avancar(),
            item_aberto: self.item_aberto().cloned(),
            pdf_ligado: self.ligacoes.pdf,
            estante_ligada: self.ligacoes.estante,
            cofre_aberto: self.ligacoes.cofre,
        }
    }

    /// O ciclo atual (`AAAA-MM`), pelo dia de corte 20.
    pub fn ciclo_atual(&self) -> String {
        ciclo_de(&self.hoje)
    }

    /// O filtro da aba Início, montado da pesquisa e do período.
    pub fn filtro(&self) -> Filtro {
        let atual = self.ciclo_atual();
        let ciclos = match self.periodo {
            Periodo::TodosOsCiclos => None,
            Periodo::CicloAtual => Some((atual.clone(), atual)),
            Periodo::CicloAnterior => {
                let a = ciclo_anterior(&atual);
                Some((a.clone(), a))
            }
        };
        Filtro {
            texto: Some(self.pesquisa.trim().to_owned()).filter(|t| !t.is_empty()),
            ciclos,
            ..Filtro::default()
        }
    }

    /// Esquece as leituras: a próxima vez vai à `Consulta` (Atualizar, ou depois de
    /// algo que os outros crates mudaram no banco).
    pub fn atualizar(&mut self) {
        self.cache.limpar();
    }

    /// As contagens da barra de status.
    pub fn contagens(&mut self, consulta: &dyn Consulta) -> Option<Contagens> {
        Leitor::novo(consulta, &mut self.cache)
            .contagens()
            .ok()
            .map(|c| *c)
    }

    /// "Seu Bolso Feliz — Energia Exemplo".
    pub fn titulo(&mut self, consulta: &dyn Consulta) -> String {
        let atual = self.navegacao.atual().clone();
        let nome = match &atual {
            Destino::Item(r) => Leitor::novo(consulta, &mut self.cache).rotulo(r),
            outro => areas::rotulo_do_destino(outro),
        };
        format!("Seu Bolso Feliz — {nome}")
    }

    // ------------------------------------------------------------------ vistas

    /// O painel de navegação (à esquerda).
    pub fn navegacao_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let contagens = self.contagens(consulta);
        let atual = self.navegacao.atual().clone();
        areas::painel_de_navegacao(ui, &atual, contagens.as_ref(), &mut self.pedidos);
    }

    /// A trilha com Voltar e Avançar ("Inventário › Fornecedores › Energia Exemplo").
    pub fn trilha_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        let atual = self.navegacao.atual().clone();
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        let pedidos = &mut self.pedidos;
        ui.horizontal(|ui| {
            if componentes::botao_icone(
                ui,
                "arrow_back",
                "Voltar (Alt+←)",
                self.navegacao.pode_voltar(),
            )
            .clicked()
            {
                pedidos.push(Pedido::Voltar);
            }
            if componentes::botao_icone(
                ui,
                "arrow_forward",
                "Avançar (Alt+→)",
                self.navegacao.pode_avancar(),
            )
            .clicked()
            {
                pedidos.push(Pedido::Avancar);
            }
            let separador = |ui: &mut Ui| {
                ui.label(icones::icone_cor("chevron_right", 14.0, p.tinta_suave));
            };
            let destino_da_area = match &atual {
                Destino::Item(r) => areas::destino_do_tipo(r.tipo),
                Destino::Total { .. } => None,
                outro => Some(outro.clone()),
            };
            if let Some(d) = &destino_da_area {
                ui.label(RichText::new(areas::grupo_de(d)).color(p.tinta_suave));
                separador(ui);
                if matches!(atual, Destino::Item(_)) {
                    componentes::link_para(ui, &areas::rotulo_do_destino(d), d.clone(), pedidos);
                } else {
                    ui.label(
                        RichText::new(areas::rotulo_do_destino(d))
                            .strong()
                            .color(p.tinta),
                    );
                }
            }
            match &atual {
                Destino::Item(r) => {
                    separador(ui);
                    let rotulo = leitor.rotulo(r);
                    ui.label(RichText::new(rotulo).strong().color(p.tinta));
                }
                Destino::Total { titulo, .. } => {
                    ui.label(RichText::new("Total").color(p.tinta_suave));
                    separador(ui);
                    ui.label(RichText::new(titulo).strong().color(p.tinta));
                }
                _ => {}
            }
        });
    }

    /// O centro: a área, a página do item ou a lista de um total.
    pub fn centro_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        self.trilha_ui(ui, consulta);
        if let Some(m) = &self.mensagem {
            let (fundo, borda, cor, icone) = if m.erro {
                (p.perigo_sutil, p.borda, p.perigo, "error")
            } else {
                (p.sucesso_sutil, p.destaque_borda, p.sucesso, "check_circle")
            };
            componentes::aviso(ui, icone, &m.texto, fundo, borda, cor);
        }
        ui.add_space(4.0);
        let atual = self.navegacao.atual().clone();
        let filtro = self.filtro();
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        let pedidos = &mut self.pedidos;
        match &atual {
            Destino::Area(Area::VisaoGeral) => {
                cabecalho_da_area(
                    ui,
                    Area::VisaoGeral,
                    None,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                painel::visao_geral(ui, &mut leitor, &mut self.centro, pedidos);
            }
            Destino::Area(Area::Relatorios) => {
                cabecalho_da_area(
                    ui,
                    Area::Relatorios,
                    None,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                painel::relatorios(
                    ui,
                    &mut leitor,
                    &mut self.relatorios,
                    &filtro,
                    &mut self.centro,
                    pedidos,
                );
            }
            Destino::Area(area) => {
                cabecalho_da_area(
                    ui,
                    *area,
                    Some(&mut self.pesquisa),
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                if *area == Area::Lotes {
                    faixa_do_lote(ui, &mut leitor, self.lote_atual.as_ref(), pedidos);
                }
                let pedido = PedidoDeTabela::Area {
                    area: *area,
                    filtro,
                };
                match leitor.tabela(&pedido) {
                    Ok(t) => {
                        ui.horizontal(|ui| {
                            let n = t.linhas.len();
                            let sel = self.centro.selecao.itens.len();
                            let mut texto = format!("{} {}", formato::inteiro(n as i64), if n == 1 { "linha" } else { "linhas" });
                            if sel > 0 {
                                texto.push_str(&format!(" · {sel} selecionada{}", if sel == 1 { "" } else { "s" }));
                            }
                            ui.label(RichText::new(texto).size(12.0).color(p.tinta_suave));
                            ui.label(
                                RichText::new("Clique seleciona, Ctrl+clique soma à seleção, Shift+clique seleciona o intervalo, duplo clique abre.")
                                    .size(12.0)
                                    .color(p.tinta_desabilitada),
                            );
                        });
                        let chave = format!("area:{area:?}");
                        let mut estado = self.centro.tabelas.remove(&chave).unwrap_or_default();
                        tabela::desenhar(
                            ui,
                            &chave,
                            &t,
                            &mut estado,
                            &mut self.centro.selecao,
                            pedidos,
                            None,
                        );
                        self.centro.tabelas.insert(chave, estado);
                    }
                    Err(e) => {
                        componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo)
                    }
                }
            }
            Destino::AreaFutura(id) => {
                if let Some(a) = areas::area_futura(id) {
                    componentes::fatia_futura(ui, a.icone, a.rotulo, a.fatia, a.motivo);
                }
            }
            Destino::Item(r) => {
                egui::ScrollArea::vertical()
                    .id_salt("pagina")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        pagina::desenhar(ui, &mut leitor, r, &mut self.centro, pedidos)
                    });
            }
            Destino::Total { titulo, pedido } => {
                pagina::desenhar_total(ui, &mut leitor, titulo, pedido, &mut self.centro, pedidos);
            }
        }
    }

    /// Um painel do dock.
    pub fn dock_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta, painel: PainelDoDock) {
        let foco = self.foco();
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        dock::desenhar(
            ui,
            painel,
            &mut leitor,
            foco.as_ref(),
            &self.decisoes_da_sessao,
            &mut self.pedidos,
        );
    }

    /// Os painéis flutuantes (Ctrl+clique), como janelas egui dentro da janela.
    pub fn flutuantes_ui(&mut self, ctx: &Context, consulta: &dyn Consulta) {
        let p = tema::paleta(ctx);
        let lista: Vec<_> = self.navegacao.flutuantes().to_vec();
        for f in lista {
            let mut leitor = Leitor::novo(consulta, &mut self.cache);
            let titulo = match &f.destino {
                Destino::Item(r) => format!("{} · {}", r.tipo.singular(), leitor.rotulo(r)),
                outro => areas::rotulo_do_destino(outro),
            };
            let mut aberto = true;
            let vista = self.flutuantes.entry(f.id).or_default();
            let pedidos = &mut self.pedidos;
            egui::Window::new(RichText::new(titulo).strong())
                .id(Id::new(("flutuante", f.id)))
                .open(&mut aberto)
                .default_size([560.0, 460.0])
                .resizable(true)
                .collapsible(true)
                .frame(egui::Frame::window(&ctx.global_style()).fill(p.superficie_1))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match &f.destino {
                            Destino::Item(r) => {
                                pagina::desenhar(ui, &mut leitor, r, vista, pedidos)
                            }
                            Destino::Total { titulo, pedido } => pagina::desenhar_total(
                                ui,
                                &mut leitor,
                                titulo,
                                pedido,
                                vista,
                                pedidos,
                            ),
                            Destino::Area(a) => {
                                let pedido = PedidoDeTabela::Area {
                                    area: *a,
                                    filtro: Filtro::default(),
                                };
                                if let Ok(t) = leitor.tabela(&pedido) {
                                    let mut estado = tabela::EstadoDaTabela::default();
                                    tabela::desenhar(
                                        ui,
                                        ("flutuante-area", f.id),
                                        &t,
                                        &mut estado,
                                        &mut vista.selecao,
                                        pedidos,
                                        Some(380.0),
                                    );
                                }
                            }
                            Destino::AreaFutura(id) => {
                                if let Some(a) = areas::area_futura(id) {
                                    componentes::fatia_futura(
                                        ui, a.icone, a.rotulo, a.fatia, a.motivo,
                                    );
                                }
                            }
                        });
                });
            if !aberto {
                self.pedidos.push(Pedido::FecharFlutuante(f.id));
            }
        }
    }

    /// Os diálogos das Ações que pedem dados (corrigir, rejeitar, sinalizar, novo lote).
    pub fn dialogos_ui(&mut self, ctx: &Context) {
        let Some(mut dialogo) = self.dialogo.take() else {
            return;
        };
        let mut fechar = false;
        let mut confirmar = false;
        let resposta = egui::Modal::new(Id::new("sbf-dialogo")).show(ctx, |ui| {
            ui.set_width(380.0);
            match &mut dialogo {
                Dialogo::Texto {
                    acao,
                    alvos,
                    rotulo,
                    valor,
                } => {
                    let def = acoes::definicao(*acao);
                    componentes::titulo(ui, def.rotulo, 17.0);
                    ui.label(format!(
                        "{} {}",
                        alvos.len(),
                        if alvos.len() == 1 { "item" } else { "itens" }
                    ));
                    ui.add_space(6.0);
                    ui.label(rotulo.as_str());
                    let r = ui.add(egui::TextEdit::singleline(valor).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirmar = true;
                    }
                }
                Dialogo::DataEValor {
                    alvos,
                    data,
                    valor,
                    erro,
                } => {
                    componentes::titulo(ui, "Corrigir data e valor", 17.0);
                    ui.label(format!(
                        "{} achado. O ciclo é recalculado pela data.",
                        alvos.len()
                    ));
                    ui.add_space(6.0);
                    ui.label("Data (dd/mm/aaaa), vazio para não mudar");
                    ui.add(egui::TextEdit::singleline(data).desired_width(f32::INFINITY));
                    ui.label("Valor em reais (ex.: -312,44), vazio para não mudar");
                    ui.add(egui::TextEdit::singleline(valor).desired_width(f32::INFINITY));
                    if let Some(e) = erro {
                        ui.colored_label(tema::paleta(ui.ctx()).perigo, e.as_str());
                    }
                }
                Dialogo::NovoLote { nome } => {
                    componentes::titulo(ui, "Novo lote", 17.0);
                    ui.label("Nome do lote");
                    let r = ui.add(egui::TextEdit::singleline(nome).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirmar = true;
                    }
                }
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Confirmar").clicked() {
                    confirmar = true;
                }
                if ui.button("Cancelar").clicked() {
                    fechar = true;
                }
            });
        });
        if resposta.should_close() {
            fechar = true;
        }
        if confirmar {
            match confirmar_dialogo(&dialogo) {
                Ok(pedido) => {
                    self.pedidos.push(pedido);
                    return;
                }
                Err(e) => {
                    if let Dialogo::DataEValor { erro, .. } = &mut dialogo {
                        *erro = Some(e);
                    } else {
                        self.mensagem = Some(Mensagem {
                            texto: e,
                            erro: true,
                        });
                    }
                }
            }
        }
        if !fechar {
            self.dialogo = Some(dialogo);
        }
    }

    /// Lê os atalhos da lista única e os botões laterais do mouse.
    pub fn atalhos(&mut self, ctx: &Context) {
        if self.dialogo.is_some() {
            return;
        }
        if let Some(id) = acoes::atalho_apertado(ctx)
            && acoes::disponibilidade(id, &self.contexto_das_acoes()).habilitada()
        {
            self.executar(id);
        }
        let (tras, frente) = ctx.input(|i| {
            (
                i.pointer.button_clicked(PointerButton::Extra1),
                i.pointer.button_clicked(PointerButton::Extra2),
            )
        });
        if tras {
            self.pedidos.push(Pedido::Voltar);
        }
        if frente {
            self.pedidos.push(Pedido::Avancar);
        }
    }

    /// Se a pesquisa deve ganhar o foco neste quadro (Ctrl+F).
    pub fn pesquisa(&mut self) -> &mut String {
        &mut self.pesquisa
    }

    // ------------------------------------------------------------------ escrita

    /// Aplica os pedidos do quadro: navegação, Ações e decisões. Chame depois de desenhar.
    pub fn processar(
        &mut self,
        ctx: &Context,
        consulta: &dyn Consulta,
        comandos: &mut dyn Comandos,
    ) {
        let pedidos = std::mem::take(&mut self.pedidos);
        for pedido in pedidos {
            let antes = self.navegacao.atual().clone();
            if self.navegacao.aplicar(&pedido) {
                let depois = self.navegacao.atual().clone();
                if antes != depois {
                    self.centro.selecao.limpar();
                    if let Destino::Item(r) = &depois
                        && r.tipo == Tipo::Lote
                    {
                        self.lote_atual = Some(r.clone());
                    }
                }
                self.flutuantes
                    .retain(|id, _| self.navegacao.flutuantes().iter().any(|f| f.id == *id));
                continue;
            }
            match pedido {
                Pedido::Executar(id) => self.acao(ctx, consulta, comandos, id),
                Pedido::Decidir(d) => self.decidir(comandos, d),
                Pedido::Desfazer(id) => match comandos.desfazer(id) {
                    Ok(nova) => {
                        self.decisoes_da_sessao.push(nova);
                        self.ok(format!("Decisão {id} desfeita pela decisão {nova}."));
                    }
                    Err(e) => self.erro(e.to_string()),
                },
                Pedido::CriarLote(nome) => match comandos.criar_lote(&nome) {
                    Ok(lote) => {
                        self.ok(format!(
                            "Lote \u{201c}{nome}\u{201d} criado. Ele é o lote atual."
                        ));
                        self.lote_atual = Some(lote);
                    }
                    Err(e) => self.erro(e.to_string()),
                },
                Pedido::EscolherLote(r) => self.lote_atual = Some(r),
                Pedido::Copiar(texto) => {
                    ctx.copy_text(texto);
                    self.mensagem = Some(Mensagem {
                        texto: "Copiado.".to_owned(),
                        erro: false,
                    });
                }
                // Já tratados pela navegação.
                Pedido::Abrir(_)
                | Pedido::AbrirFlutuante(_)
                | Pedido::FecharFlutuante(_)
                | Pedido::Voltar
                | Pedido::Avancar => {}
            }
        }
    }

    fn ok(&mut self, texto: String) {
        self.cache.limpar();
        self.mensagem = Some(Mensagem { texto, erro: false });
    }

    fn erro(&mut self, texto: String) {
        self.mensagem = Some(Mensagem { texto, erro: true });
    }

    fn decidir(&mut self, comandos: &mut dyn Comandos, pedido: PedidoDeDecisao) {
        match comandos.executar(&pedido) {
            Ok(ids) => {
                let n = pedido.alvos.len();
                let rotulo = rotulo_da_acao(&pedido.acao);
                self.decisoes_da_sessao.extend(ids.iter().copied());
                self.ok(format!(
                    "{rotulo}: {n} {} · {} decisão{} gravada{}. Desfazer: Ctrl+Z.",
                    if n == 1 { "item" } else { "itens" },
                    ids.len(),
                    if ids.len() == 1 { "" } else { "ões" },
                    if ids.len() == 1 { "" } else { "s" },
                ));
                if matches!(
                    pedido.acao,
                    Acao::MarcarDocumento | Acao::Descartar | Acao::EmDuvida
                ) {
                    self.centro.selecao.itens.clear();
                }
            }
            Err(e) => self.erro(e.to_string()),
        }
    }

    fn acao(
        &mut self,
        ctx: &Context,
        consulta: &dyn Consulta,
        comandos: &mut dyn Comandos,
        id: IdAcao,
    ) {
        let contexto = self.contexto_das_acoes();
        if let Some(m) = acoes::disponibilidade(id, &contexto).motivo() {
            self.erro(format!("{}: {m}", acoes::definicao(id).rotulo));
            return;
        }
        let sel = contexto.selecao.clone();
        let simples = |acao: Acao| PedidoDeDecisao {
            acao,
            alvos: sel.clone(),
            motivo: None,
        };
        let texto = |acao: IdAcao, rotulo: &str| Dialogo::Texto {
            acao,
            alvos: sel.clone(),
            rotulo: rotulo.to_owned(),
            valor: String::new(),
        };
        match id {
            IdAcao::Voltar => {
                self.navegacao.voltar();
                self.centro.selecao.limpar();
            }
            IdAcao::Avancar => {
                self.navegacao.avancar();
                self.centro.selecao.limpar();
            }
            IdAcao::Atualizar => {
                self.atualizar();
                self.mensagem = Some(Mensagem {
                    texto: "Inventário relido.".to_owned(),
                    erro: false,
                });
            }
            IdAcao::Pesquisar => self.focar_pesquisa = true,
            IdAcao::AbrirEmPainelFlutuante => {
                if let Some(r) = sel.first() {
                    self.navegacao.abrir_flutuante(Destino::Item(r.clone()));
                }
            }
            IdAcao::Documento => self.decidir(comandos, simples(Acao::MarcarDocumento)),
            IdAcao::Descartar => self.decidir(comandos, simples(Acao::Descartar)),
            IdAcao::EmDuvida => self.decidir(comandos, simples(Acao::EmDuvida)),
            IdAcao::CorrigirFornecedor => self.dialogo = Some(texto(id, "Fornecedor")),
            IdAcao::CorrigirCategoria => self.dialogo = Some(texto(id, "Categoria")),
            IdAcao::CorrigirTipo => {
                self.dialogo = Some(texto(id, "Tipo (ex.: boleto, fatura_cartao, compra)"))
            }
            IdAcao::CorrigirDataEValor => {
                self.dialogo = Some(Dialogo::DataEValor {
                    alvos: sel,
                    data: String::new(),
                    valor: String::new(),
                    erro: None,
                });
            }
            IdAcao::MarcarDuplicidade | IdAcao::UnirConciliados => {
                if let [a, b] = sel.as_slice() {
                    let acao = if id == IdAcao::MarcarDuplicidade {
                        Acao::MarcarDuplicidade { com: b.clone() }
                    } else {
                        Acao::UnirConciliados { com: b.clone() }
                    };
                    self.decidir(
                        comandos,
                        PedidoDeDecisao {
                            acao,
                            alvos: vec![a.clone()],
                            motivo: None,
                        },
                    );
                }
            }
            IdAcao::ConfirmarRecorrencia => {
                self.decidir(comandos, simples(Acao::ConfirmarRecorrencia))
            }
            IdAcao::Aprovar => self.decidir(comandos, simples(Acao::Aprovar)),
            IdAcao::Rejeitar => self.dialogo = Some(texto(id, "Motivo (obrigatório)")),
            IdAcao::Sinalizar => self.dialogo = Some(texto(id, "A pergunta que trava o item")),
            IdAcao::Responder => self.dialogo = Some(texto(id, "Resposta")),
            IdAcao::Desfazer => match comandos.desfazer_ultima() {
                Ok(Some(nova)) => {
                    self.decisoes_da_sessao.push(nova);
                    self.ok(format!("Última decisão desfeita (decisão {nova})."));
                }
                Ok(None) => self.erro("Nada a desfazer.".to_owned()),
                Err(e) => self.erro(e.to_string()),
            },
            IdAcao::VerDecisoes => {
                if !self.paineis_visiveis.contains(&PainelDoDock::Historico) {
                    self.paineis_visiveis.push(PainelDoDock::Historico);
                }
            }
            IdAcao::NovoLote => {
                self.dialogo = Some(Dialogo::NovoLote {
                    nome: String::new(),
                })
            }
            IdAcao::AdicionarSelecao
            | IdAcao::RemoverDoLote
            | IdAcao::AprovarLote
            | IdAcao::DesaprovarLote => {
                let Some(lote) = self.lote_atual.clone() else {
                    return;
                };
                let r = match id {
                    IdAcao::AdicionarSelecao => comandos.adicionar_ao_lote(&lote, &sel).map(|()| format!("{} itens no lote.", sel.len())),
                    IdAcao::RemoverDoLote => comandos.remover_do_lote(&lote, &sel).map(|()| format!("{} itens tirados do lote.", sel.len())),
                    IdAcao::AprovarLote => comandos.aprovar_lote(&lote).map(|()| "Lote aprovado: os registros dele ficam aprovados. Enviar chega na fatia 3.".to_owned()),
                    _ => comandos.desaprovar_lote(&lote).map(|()| "Lote de volta a editável.".to_owned()),
                };
                match r {
                    Ok(t) => self.ok(t),
                    Err(e) => self.erro(e.to_string()),
                }
            }
            IdAcao::PainelDetalhe
            | IdAcao::PainelOrigens
            | IdAcao::PainelTexto
            | IdAcao::PainelHistorico => {
                let painel = match id {
                    IdAcao::PainelDetalhe => PainelDoDock::Detalhe,
                    IdAcao::PainelOrigens => PainelDoDock::Origens,
                    IdAcao::PainelTexto => PainelDoDock::Texto,
                    _ => PainelDoDock::Historico,
                };
                if let Some(i) = self.paineis_visiveis.iter().position(|x| *x == painel) {
                    self.paineis_visiveis.remove(i);
                } else {
                    self.paineis_visiveis.push(painel);
                }
            }
            IdAcao::VerTexto => {
                if !self.paineis_visiveis.contains(&PainelDoDock::Texto) {
                    self.paineis_visiveis.push(PainelDoDock::Texto);
                }
            }
            IdAcao::TrocarTema => {
                let todos = tema::temas();
                let i = todos
                    .iter()
                    .position(|t| t.id == self.tema)
                    .map_or(0, |i| (i + 1) % todos.len().max(1));
                if let Some(t) = todos.get(i) {
                    self.tema = t.id.clone();
                    tema::aplicar(ctx, &self.tema);
                    self.mensagem = Some(Mensagem {
                        texto: format!("Tema {}.", t.nome),
                        erro: false,
                    });
                }
            }
            IdAcao::CopiarHash => {
                if let Some(r) = sel.first() {
                    ctx.copy_text(r.chave.clone());
                    self.mensagem = Some(Mensagem {
                        texto: "Hash copiado.".to_owned(),
                        erro: false,
                    });
                }
            }
            IdAcao::CopiarRastro => {
                if let Some(r) = self.item_aberto().cloned() {
                    let mut leitor = Leitor::novo(consulta, &mut self.cache);
                    let rotulo = leitor.rotulo(&r);
                    match leitor.rastro(&r) {
                        Ok(rastro) => {
                            ctx.copy_text(pagina::texto_do_rastro(&r, &rotulo, &rastro));
                            self.mensagem = Some(Mensagem {
                                texto: "Rastro copiado.".to_owned(),
                                erro: false,
                            });
                        }
                        Err(e) => self.erro(e),
                    }
                }
            }
            IdAcao::EnviarAoServidor
            | IdAcao::AbrirPdfProtegido
            | IdAcao::RegrasDeSenha
            | IdAcao::AtualizarEstante
            | IdAcao::FazerCopiaDeSeguranca
            | IdAcao::AbrirOriginal
            | IdAcao::MostrarNoCofre
            | IdAcao::AbrirProtegido => {
                self.erro(format!(
                    "{}: {}",
                    acoes::definicao(id).rotulo,
                    acoes::MOTIVO_NAO_LIGADO
                ));
            }
        }
    }
}

fn rotulo_da_acao(acao: &Acao) -> &'static str {
    let id = match acao {
        Acao::MarcarDocumento => IdAcao::Documento,
        Acao::Descartar => IdAcao::Descartar,
        Acao::EmDuvida => IdAcao::EmDuvida,
        Acao::CorrigirFornecedor(_) => IdAcao::CorrigirFornecedor,
        Acao::CorrigirCategoria(_) => IdAcao::CorrigirCategoria,
        Acao::CorrigirTipo(_) => IdAcao::CorrigirTipo,
        Acao::CorrigirDataEValor { .. } => IdAcao::CorrigirDataEValor,
        Acao::MarcarDuplicidade { .. } => IdAcao::MarcarDuplicidade,
        Acao::UnirConciliados { .. } => IdAcao::UnirConciliados,
        Acao::ConfirmarRecorrencia => IdAcao::ConfirmarRecorrencia,
        Acao::Aprovar => IdAcao::Aprovar,
        Acao::Rejeitar => IdAcao::Rejeitar,
        Acao::Sinalizar { .. } => IdAcao::Sinalizar,
        Acao::Responder { .. } => IdAcao::Responder,
        Acao::GuardarRegraDeSenha { .. } => return "Guardar regra de senha",
    };
    acoes::definicao(id).rotulo
}

/// Monta o pedido de um diálogo confirmado.
pub fn confirmar_dialogo(d: &Dialogo) -> Result<Pedido, String> {
    match d {
        Dialogo::Texto {
            acao, alvos, valor, ..
        } => {
            let v = valor.trim().to_owned();
            if v.is_empty() {
                return Err("Preencha o campo.".to_owned());
            }
            let (acao, motivo) = match acao {
                IdAcao::CorrigirFornecedor => (Acao::CorrigirFornecedor(v), None),
                IdAcao::CorrigirCategoria => (Acao::CorrigirCategoria(v), None),
                IdAcao::CorrigirTipo => (Acao::CorrigirTipo(v), None),
                IdAcao::Rejeitar => (Acao::Rejeitar, Some(v)),
                IdAcao::Sinalizar => (Acao::Sinalizar { pergunta: v }, None),
                IdAcao::Responder => (Acao::Responder { resposta: v }, None),
                outra => return Err(format!("{outra:?} não tem diálogo de texto")),
            };
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao,
                alvos: alvos.clone(),
                motivo,
            }))
        }
        Dialogo::DataEValor {
            alvos, data, valor, ..
        } => {
            let data = if data.trim().is_empty() {
                None
            } else {
                Some(data_iso(data.trim()).ok_or("Data inválida: use dd/mm/aaaa.")?)
            };
            let valor_centavos = if valor.trim().is_empty() {
                None
            } else {
                Some(
                    centavos_de(valor.trim())
                        .ok_or("Valor inválido: use, por exemplo, -312,44.")?,
                )
            };
            if data.is_none() && valor_centavos.is_none() {
                return Err("Preencha a data, o valor ou os dois.".to_owned());
            }
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao: Acao::CorrigirDataEValor {
                    data,
                    valor_centavos,
                },
                alvos: alvos.clone(),
                motivo: None,
            }))
        }
        Dialogo::NovoLote { nome } => {
            let n = nome.trim();
            if n.is_empty() {
                Err("Dê um nome ao lote.".to_owned())
            } else {
                Ok(Pedido::CriarLote(n.to_owned()))
            }
        }
    }
}

/// `28/10/2026` ou `2026-10-28` → `2026-10-28`.
pub fn data_iso(texto: &str) -> Option<String> {
    let (a, m, d) = if let Some((d, resto)) = texto.split_once('/') {
        let (m, a) = resto.split_once('/')?;
        (a, m, d)
    } else {
        let mut it = texto.splitn(3, '-');
        (it.next()?, it.next()?, it.next()?)
    };
    let (a, m, d): (u32, u32, u32) = (a.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1900..=2200).contains(&a) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(format!("{a:04}-{m:02}-{d:02}"))
}

/// `-1.234,56` → `-123456`; `312` → `31200`.
pub fn centavos_de(texto: &str) -> Option<i64> {
    let t = texto.replace("R$", "").replace(['.', ' '], "");
    let negativo = t.starts_with('-');
    let t = t.trim_start_matches(['-', '+']);
    let (inteiro, frac) = t.split_once(',').unwrap_or((t, "0"));
    let frac = format!("{frac:0<2}");
    if frac.len() > 2 {
        return None;
    }
    let v = inteiro.parse::<i64>().ok()? * 100 + frac.parse::<i64>().ok()?;
    Some(if negativo { -v } else { v })
}

/// O ciclo de uma data `AAAA-MM-DD`: dia 20 em diante é o ciclo do mês; antes, o do mês anterior.
pub fn ciclo_de(data: &str) -> String {
    let mut it = data.splitn(3, '-');
    let (a, m, d) = (
        it.next()
            .and_then(|x| x.parse::<i32>().ok())
            .unwrap_or(2026),
        it.next().and_then(|x| x.parse::<i32>().ok()).unwrap_or(1),
        it.next()
            .and_then(|x| x.get(..2))
            .and_then(|x| x.parse::<i32>().ok())
            .unwrap_or(1),
    );
    let ciclo = format!("{a:04}-{m:02}");
    if d >= 20 {
        ciclo
    } else {
        ciclo_anterior(&ciclo)
    }
}

/// `2026-01` → `2025-12`.
pub fn ciclo_anterior(ciclo: &str) -> String {
    let (a, m) = ciclo.split_once('-').unwrap_or(("2026", "01"));
    let (a, m): (i32, i32) = (a.parse().unwrap_or(2026), m.parse().unwrap_or(1));
    if m == 1 {
        format!("{:04}-12", a - 1)
    } else {
        format!("{a:04}-{:02}", m - 1)
    }
}

/// Hoje em America/Sao_Paulo (UTC−3, sem horário de verão desde 2019).
pub fn hoje_em_sao_paulo() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        - 3 * 3600;
    let dias = segundos.div_euclid(86_400);
    // Algoritmo de dias para data civil (Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn cabecalho_da_area(
    ui: &mut Ui,
    area: Area,
    pesquisa: Option<&mut String>,
    periodo: &mut Periodo,
    focar: &mut bool,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            componentes::titulo(ui, areas::rotulo_da_area(area), 20.0);
            ui.label(
                RichText::new(areas::descricao_da_area(area))
                    .size(12.5)
                    .color(p.tinta_suave),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(texto) = pesquisa {
                let r = ui.add(
                    egui::TextEdit::singleline(texto)
                        .hint_text("Pesquisar nesta área (Ctrl+F)")
                        .desired_width(240.0),
                );
                if *focar {
                    r.request_focus();
                    *focar = false;
                }
                ui.label(icones::icone_cor("search", 16.0, p.tinta_suave));
            }
            if area != Area::VisaoGeral {
                ui.separator();
                ui.selectable_value(periodo, Periodo::TodosOsCiclos, "Todos os ciclos");
                ui.selectable_value(periodo, Periodo::CicloAnterior, "Ciclo anterior");
                ui.selectable_value(periodo, Periodo::CicloAtual, "Ciclo atual");
            }
        });
    });
    ui.add_space(4.0);
}

fn faixa_do_lote(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    lote_atual: Option<&Ref>,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(icones::icone_cor("inventory_2", 16.0, p.tinta));
            ui.label(RichText::new("Lote atual").color(p.tinta_suave));
            match lote_atual {
                Some(l) => {
                    let rotulo = leitor.rotulo(l);
                    componentes::link_item(ui, l, &rotulo, pedidos);
                }
                None => {
                    ui.label(RichText::new("nenhum: crie um (Novo lote) ou abra um da lista").color(p.tinta_suave));
                }
            }
            ui.separator();
            ui.label(
                RichText::new("Selecione documentos ou registros em qualquer área e use Lotes › Adicionar seleção. Enviar ao servidor chega na fatia 3.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        });
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ciclo_pelo_dia_de_corte() {
        assert_eq!(ciclo_de("2026-11-13"), "2026-10");
        assert_eq!(ciclo_de("2026-11-20"), "2026-11");
        assert_eq!(ciclo_de("2026-01-05"), "2025-12");
        assert_eq!(ciclo_anterior("2026-10"), "2026-09");
    }

    #[test]
    fn hoje_tem_formato_de_data() {
        let h = hoje_em_sao_paulo();
        assert_eq!(h.len(), 10);
        assert!(data_iso(&h).is_some());
    }

    #[test]
    fn valores_e_datas_dos_dialogos() {
        assert_eq!(centavos_de("-312,44"), Some(-31244));
        assert_eq!(centavos_de("1.234,5"), Some(123450));
        assert_eq!(centavos_de("R$ 300"), Some(30000));
        assert_eq!(centavos_de("abc"), None);
        assert_eq!(data_iso("28/10/2026").as_deref(), Some("2026-10-28"));
        assert_eq!(data_iso("2026-10-28").as_deref(), Some("2026-10-28"));
        assert_eq!(data_iso("31/13/2026"), None);
    }

    #[test]
    fn rejeitar_sem_motivo_nao_passa_e_com_motivo_vira_decisao() {
        let alvo = Ref::nova(Tipo::Registro, "7");
        let mut d = Dialogo::Texto {
            acao: IdAcao::Rejeitar,
            alvos: vec![alvo.clone()],
            rotulo: String::new(),
            valor: "  ".into(),
        };
        assert!(confirmar_dialogo(&d).is_err());
        if let Dialogo::Texto { valor, .. } = &mut d {
            *valor = "valor diferente do boleto".into();
        }
        assert_eq!(
            confirmar_dialogo(&d),
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao: Acao::Rejeitar,
                alvos: vec![alvo],
                motivo: Some("valor diferente do boleto".into())
            }))
        );
    }
}
