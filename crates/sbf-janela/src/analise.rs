//! As áreas do grupo **Análise** que funcionam (spec 10, frente C2): **Perguntar** (RAG
//! local, RG-03) e **Pontos de atenção** (PA-01 a PA-03).
//!
//! - **Perguntar.** A pergunta busca os trechos mais próximos no índice local; só eles vão
//!   ao motor de IA, mascarados no nível do destinatário, e o que saiu fica em `saida_ia`.
//!   A resposta cita os trechos, cada um com link para o item. Antes da primeira pergunta, se
//!   o modelo de busca não está na máquina, a tela **avisa do download** e espera o clique.
//! - **Pontos de atenção.** Filtro por área e por estado; cada ponto mostra a evidência (com
//!   link para os itens) e o impacto com a **estimativa marcada**; Resolver e Ignorar pedem
//!   o motivo.
//!
//! Os serviços são traits ([`ServicoDePerguntar`], [`ServicoDePontos`]); o real é montado no
//! `sbf`, o fictício na demonstração. Chamadas que podem demorar (a pergunta, o download, a
//! análise) rodam numa thread, e a tela consulta o resultado a cada quadro.

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use egui::{Align, Layout, RichText, ScrollArea, Ui};
use sbf_indice::ItemRef;
use sbf_indice::perguntar::{Resposta, ServicoDePerguntar};
use sbf_inventario::{Ref, Tipo};
use sbf_pontos::{Area, Estado, Filtro, Ponto, ServicoDePontos};

use crate::componentes;
use crate::navegacao::{Destino, Pedido};
use crate::tema;

/// O item que um trecho ou uma evidência aponta, se a janela sabe abrir.
#[must_use]
pub fn ref_do_item(tipo: &str, id: &str) -> Option<Ref> {
    let t = match tipo {
        "achado" => Tipo::Achado,
        "registro" => Tipo::Registro,
        "movimentacao" => Tipo::Movimentacao,
        _ => return None,
    };
    Some(Ref::nova(t, id.to_owned()))
}

fn ref_de(item: &ItemRef) -> Option<Ref> {
    ref_do_item(&item.tipo, &item.id)
}

// -- Perguntar -------------------------------------------------------------------------------

type ServicoCompartilhado = Arc<Mutex<Box<dyn ServicoDePerguntar>>>;

/// Uma pergunta e o que veio.
#[derive(Debug, Clone)]
pub struct Troca {
    pub pergunta: String,
    pub resposta: Result<Resposta, String>,
}

/// O estado da área Perguntar.
#[derive(Default)]
pub struct EstadoDePerguntar {
    servico: Option<ServicoCompartilhado>,
    pub entrada: String,
    pub trocas: Vec<Troca>,
    pendente: Option<(String, Receiver<Result<Resposta, String>>)>,
    baixando: Option<Receiver<Result<(), String>>>,
    modelo_pronto: Option<bool>,
    trechos: Option<usize>,
    erro: Option<String>,
}

impl std::fmt::Debug for EstadoDePerguntar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstadoDePerguntar")
            .field("ligado", &self.servico.is_some())
            .finish_non_exhaustive()
    }
}

impl EstadoDePerguntar {
    #[must_use]
    pub fn com_servico(servico: Box<dyn ServicoDePerguntar>) -> Self {
        Self {
            servico: Some(Arc::new(Mutex::new(servico))),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn ligado(&self) -> bool {
        self.servico.is_some()
    }

    fn atualizar_o_cabecalho(&mut self) {
        if self.modelo_pronto.is_some() {
            return;
        }
        if let Some(Ok(s)) = self.servico.as_ref().map(|s| s.try_lock()) {
            self.modelo_pronto = Some(s.modelo_pronto());
            self.trechos = s.trechos_no_indice();
        }
    }

    /// Faz a pergunta numa thread. Ignora pergunta vazia ou com outra em andamento.
    pub fn perguntar(&mut self, ctx: &egui::Context, pergunta: &str) {
        let pergunta = pergunta.trim().to_owned();
        if pergunta.is_empty() || self.pendente.is_some() {
            return;
        }
        let Some(servico) = self.servico.clone() else {
            return;
        };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let p = pergunta.clone();
        std::thread::spawn(move || {
            let r = match servico.lock() {
                Ok(mut s) => s.perguntar(&p),
                Err(_) => Err("o serviço de perguntas travou".to_owned()),
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.pendente = Some((pergunta, rx));
    }

    /// Baixa o modelo numa thread (só depois do clique em "Baixar modelo").
    pub fn baixar_modelo(&mut self, ctx: &egui::Context) {
        let Some(servico) = self.servico.clone() else {
            return;
        };
        if self.baixando.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = match servico.lock() {
                Ok(mut s) => s.baixar_modelo(),
                Err(_) => Err("o serviço de perguntas travou".to_owned()),
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.baixando = Some(rx);
    }

    fn sondar(&mut self) {
        if let Some((pergunta, rx)) = &self.pendente
            && let Ok(resposta) = rx.try_recv()
        {
            let pergunta = pergunta.clone();
            self.trocas.push(Troca { pergunta, resposta });
            self.pendente = None;
        }
        if let Some(rx) = &self.baixando
            && let Ok(r) = rx.try_recv()
        {
            self.baixando = None;
            match r {
                Ok(()) => {
                    self.modelo_pronto = None;
                    self.erro = None;
                }
                Err(e) => self.erro = Some(e),
            }
        }
    }
}

fn trecho_curto(t: &str) -> String {
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= 140 {
        t
    } else {
        let mut c: String = t.chars().take(139).collect();
        c.push('…');
        c
    }
}

/// A área Perguntar.
pub fn perguntar(ui: &mut Ui, estado: &mut EstadoDePerguntar, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    if !estado.ligado() {
        componentes::aviso(
            ui,
            "forum",
            "Perguntar não está ligado nesta máquina: ele precisa do índice local e de um provedor de IA em Opções → IA.",
            p.info_sutil,
            p.info_borda,
            p.info,
        );
        return;
    }
    estado.sondar();
    estado.atualizar_o_cabecalho();
    let ocupado = estado.pendente.is_some() || estado.baixando.is_some();
    if ocupado {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(400));
    }
    componentes::titulo(ui, "Perguntar", 20.0);
    ui.label(
        RichText::new(
            "A pergunta procura no índice do seu computador. Só os trechos mais próximos \
             saem para o provedor de IA, com números longos (CPF, conta, cartão) mascarados. \
             Cada saída fica registrada.",
        )
        .color(p.tinta_suave),
    );
    ui.add_space(8.0);

    if estado.modelo_pronto == Some(false) {
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new("Falta baixar o modelo de busca (uma vez só)")
                    .strong()
                    .color(p.tinta),
            );
            ui.label(
                "São cerca de 120 MB da internet. Depois disso o índice é calculado e guardado \
                 só aqui, sem enviar nada seu.",
            );
            ui.add_space(4.0);
            let baixando = estado.baixando.is_some();
            if ui
                .add_enabled(
                    !baixando,
                    egui::Button::new(if baixando {
                        "Baixando…"
                    } else {
                        "Baixar modelo (~120 MB)"
                    }),
                )
                .clicked()
            {
                let ctx = ui.ctx().clone();
                estado.baixar_modelo(&ctx);
            }
        });
        if let Some(e) = &estado.erro {
            ui.add_space(4.0);
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        }
        return;
    }

    let mut enviar = false;
    ui.horizontal(|ui| {
        let r = ui.add_enabled(
            !ocupado,
            egui::TextEdit::singleline(&mut estado.entrada)
                .hint_text("Pergunte sobre seus documentos, registros e movimentações…")
                .desired_width((ui.available_width() - 110.0).max(200.0)),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            enviar = true;
        }
        if ui
            .add_enabled(
                !ocupado && !estado.entrada.trim().is_empty(),
                egui::Button::new("Perguntar"),
            )
            .clicked()
        {
            enviar = true;
        }
    });
    if enviar {
        let pergunta = std::mem::take(&mut estado.entrada);
        let ctx = ui.ctx().clone();
        estado.perguntar(&ctx, &pergunta);
    }
    if let Some(n) = estado.trechos {
        ui.label(
            RichText::new(format!("{n} trechos no índice local"))
                .size(12.0)
                .color(p.tinta_suave),
        );
    }
    ui.add_space(6.0);
    ScrollArea::vertical()
        .id_salt("perguntar")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some((q, _)) = &estado.pendente {
                componentes::cartao(ui, |ui| {
                    ui.label(RichText::new(q).strong());
                    ui.label(RichText::new("Procurando e perguntando…").color(p.tinta_suave));
                });
            }
            for t in estado.trocas.iter().rev() {
                componentes::cartao(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&t.pergunta).strong().color(p.tinta));
                    ui.add_space(2.0);
                    match &t.resposta {
                        Err(e) => {
                            ui.label(RichText::new(e).color(p.perigo));
                        }
                        Ok(r) => {
                            ui.label(&r.texto);
                            if !r.citacoes.is_empty() {
                                ui.add_space(4.0);
                                componentes::rotulo_de_secao(ui, "Trechos citados");
                                for c in &r.citacoes {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!("[{}]", c.numero))
                                                .color(p.tinta_suave),
                                        );
                                        match ref_de(&c.item) {
                                            Some(rf) => {
                                                if ui
                                                    .link(format!(
                                                        "{} {}",
                                                        c.item.tipo,
                                                        curto_id(&c.item.id)
                                                    ))
                                                    .clicked()
                                                {
                                                    crate::navegacao::pedir_abrir(
                                                        ui.ctx(),
                                                        pedidos,
                                                        Destino::Item(rf),
                                                        false,
                                                    );
                                                }
                                            }
                                            None => {
                                                ui.label(format!("{} {}", c.item.tipo, c.item.id));
                                            }
                                        }
                                        ui.label(
                                            RichText::new(trecho_curto(&c.trecho))
                                                .size(12.0)
                                                .color(p.tinta_suave),
                                        );
                                    });
                                }
                            }
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} trechos saíram para o provedor de IA",
                                    r.trechos_enviados
                                ))
                                .size(11.5)
                                .color(p.tinta_suave),
                            );
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
}

fn curto_id(id: &str) -> String {
    if id.chars().count() > 12 {
        let mut s: String = id.chars().take(10).collect();
        s.push('…');
        s
    } else {
        id.to_owned()
    }
}

// -- Pontos de atenção -----------------------------------------------------------------------

type PontosCompartilhado = Arc<Mutex<Box<dyn ServicoDePontos>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motivo {
    Resolver,
    Ignorar,
}

/// O estado da área Pontos de atenção.
pub struct EstadoDePontos {
    servico: Option<PontosCompartilhado>,
    pub area: Option<Area>,
    pub estado: Estado,
    pontos: Vec<Ponto>,
    abertos: std::collections::HashMap<Area, usize>,
    sujo: bool,
    /// O ponto que está pedindo motivo, a ação e o texto digitado.
    motivo: Option<(i64, Motivo, String)>,
    analisando: Option<Receiver<Result<String, String>>>,
    mensagem: Option<Result<String, String>>,
}

impl Default for EstadoDePontos {
    fn default() -> Self {
        Self {
            servico: None,
            area: None,
            estado: Estado::Aberto,
            pontos: Vec::new(),
            abertos: std::collections::HashMap::new(),
            sujo: true,
            motivo: None,
            analisando: None,
            mensagem: None,
        }
    }
}

impl std::fmt::Debug for EstadoDePontos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstadoDePontos")
            .field("ligado", &self.servico.is_some())
            .finish_non_exhaustive()
    }
}

impl EstadoDePontos {
    #[must_use]
    pub fn com_servico(servico: Box<dyn ServicoDePontos>) -> Self {
        Self {
            servico: Some(Arc::new(Mutex::new(servico))),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn ligado(&self) -> bool {
        self.servico.is_some()
    }

    /// Os pontos abertos, para o contador do painel.
    #[must_use]
    pub fn total_aberto(&self) -> usize {
        self.abertos.values().sum()
    }

    fn ler(&mut self) {
        if !self.sujo {
            return;
        }
        let Some(s) = self.servico.clone() else {
            return;
        };
        let Ok(mut s) = s.try_lock() else {
            return;
        };
        self.sujo = false;
        match s.listar(Filtro {
            area: self.area,
            estado: Some(self.estado),
        }) {
            Ok(v) => self.pontos = v,
            Err(e) => self.mensagem = Some(Err(e)),
        }
        if let Ok(m) = s.abertos_por_area() {
            self.abertos = m;
        }
    }

    fn fechar(&mut self, id: i64, como: Motivo, motivo: &str) {
        let Some(s) = self.servico.clone() else {
            return;
        };
        let Ok(mut s) = s.try_lock() else {
            return;
        };
        let r = match como {
            Motivo::Resolver => s.resolver(id, motivo),
            Motivo::Ignorar => s.ignorar(id, motivo),
        };
        match r {
            Ok(()) => {
                self.motivo = None;
                self.sujo = true;
            }
            Err(e) => self.mensagem = Some(Err(e)),
        }
    }

    /// Roda a análise agora, numa thread.
    pub fn analisar_agora(&mut self, ctx: &egui::Context) {
        let Some(servico) = self.servico.clone() else {
            return;
        };
        if self.analisando.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = match servico.lock() {
                Ok(mut s) => s.analisar_agora().map(|r| {
                    format!(
                        "Análise feita: {} novos, {} reabertos, {} fechados por não valerem mais.",
                        r.novos, r.reabertos, r.fechados_sozinhos
                    )
                }),
                Err(_) => Err("o serviço de pontos travou".to_owned()),
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.analisando = Some(rx);
    }

    fn sondar(&mut self) {
        if let Some(rx) = &self.analisando
            && let Ok(r) = rx.try_recv()
        {
            self.analisando = None;
            self.mensagem = Some(r);
            self.sujo = true;
        }
    }
}

/// A área Pontos de atenção.
pub fn pontos(ui: &mut Ui, estado: &mut EstadoDePontos, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    if !estado.ligado() {
        componentes::aviso(
            ui,
            "lightbulb",
            "Os pontos de atenção não estão ligados nesta máquina: a análise roda no sbf worker.",
            p.info_sutil,
            p.info_borda,
            p.info,
        );
        return;
    }
    estado.sondar();
    estado.ler();
    if estado.analisando.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(400));
    }
    ui.horizontal(|ui| {
        componentes::titulo(ui, "Pontos de atenção", 20.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let analisando = estado.analisando.is_some();
            if ui
                .add_enabled(
                    !analisando,
                    egui::Button::new(if analisando {
                        "Analisando…"
                    } else {
                        "Analisar agora"
                    }),
                )
                .clicked()
            {
                let ctx = ui.ctx().clone();
                estado.analisar_agora(&ctx);
            }
        });
    });
    ui.label(
        RichText::new(
            "A análise é por regras, sem IA, e roda sozinha de 6 em 6 horas. Valores marcados \
             como estimativa são projeções, não o que você já pagou.",
        )
        .color(p.tinta_suave),
    );
    ui.add_space(6.0);

    // Filtro por área (com a contagem de abertos) e por estado.
    ui.horizontal_wrapped(|ui| {
        let total: usize = estado.abertos.values().sum();
        let mut mudou = false;
        if ui
            .selectable_label(estado.area.is_none(), format!("Todas ({total})"))
            .clicked()
        {
            estado.area = None;
            mudou = true;
        }
        for a in Area::TODAS {
            let n = estado.abertos.get(&a).copied().unwrap_or(0);
            if ui
                .selectable_label(estado.area == Some(a), format!("{} ({n})", a.rotulo()))
                .clicked()
            {
                estado.area = Some(a);
                mudou = true;
            }
        }
        ui.separator();
        for (e, rotulo) in [
            (Estado::Aberto, "Abertos"),
            (Estado::Resolvido, "Resolvidos"),
            (Estado::Ignorado, "Ignorados"),
        ] {
            if ui.selectable_label(estado.estado == e, rotulo).clicked() {
                estado.estado = e;
                mudou = true;
            }
        }
        if mudou {
            estado.sujo = true;
        }
    });
    match &estado.mensagem {
        Some(Ok(m)) => {
            ui.add_space(4.0);
            componentes::aviso(ui, "check_circle", m, p.sucesso_sutil, p.borda, p.sucesso);
        }
        Some(Err(e)) => {
            ui.add_space(4.0);
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        }
        None => {}
    }
    ui.add_space(6.0);

    if estado.pontos.is_empty() {
        ui.label(RichText::new("Nada aqui. Nenhum ponto neste filtro.").color(p.tinta_suave));
        return;
    }
    let lista = estado.pontos.clone();
    let mut fechar: Option<(i64, Motivo, String)> = None;
    ScrollArea::vertical()
        .id_salt("pontos")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for ponto in &lista {
                componentes::cartao(ui, |ui| {
                    ui.horizontal(|ui| {
                        componentes::selo(
                            ui,
                            ponto.area.rotulo(),
                            p.destaque_sutil,
                            p.destaque_tinta,
                        );
                        ui.label(RichText::new(&ponto.titulo).strong().color(p.tinta));
                    });
                    if !ponto.detalhe.is_empty() {
                        ui.label(RichText::new(&ponto.detalhe).color(p.tinta_suave));
                    }
                    if let Some(i) = &ponto.impacto {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Impacto:").color(p.tinta_suave));
                            ui.label(RichText::new(i.valor_texto()).strong());
                            if i.estimativa {
                                componentes::selo(ui, "estimativa", p.aviso_sutil, p.aviso);
                            }
                        });
                    }
                    if !ponto.evidencia.is_empty() {
                        ui.add_space(2.0);
                        componentes::rotulo_de_secao(ui, "Evidência");
                        ui.horizontal_wrapped(|ui| {
                            for e in &ponto.evidencia {
                                match ref_do_item(&e.tipo, &e.id) {
                                    Some(rf) => {
                                        if ui.link(&e.rotulo).clicked() {
                                            crate::navegacao::pedir_abrir(
                                                ui.ctx(),
                                                pedidos,
                                                Destino::Item(rf),
                                                false,
                                            );
                                        }
                                    }
                                    None => {
                                        ui.label(RichText::new(&e.rotulo).color(p.tinta_suave));
                                    }
                                }
                            }
                        });
                    }
                    if ponto.estado != Estado::Aberto {
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                if ponto.estado == Estado::Resolvido {
                                    "Resolvido"
                                } else {
                                    "Ignorado"
                                },
                                ponto.motivo.as_deref().unwrap_or("sem motivo")
                            ))
                            .size(12.0)
                            .color(p.tinta_suave),
                        );
                        return;
                    }
                    ui.add_space(4.0);
                    match &mut estado.motivo {
                        Some((id, como, texto)) if *id == ponto.id => {
                            let rotulo = if *como == Motivo::Resolver {
                                "Como foi resolvido?"
                            } else {
                                "Por que ignorar?"
                            };
                            ui.label(rotulo);
                            ui.add(
                                egui::TextEdit::singleline(texto)
                                    .hint_text("Motivo (obrigatório)")
                                    .desired_width(320.0),
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !texto.trim().is_empty(),
                                        egui::Button::new("Confirmar"),
                                    )
                                    .clicked()
                                {
                                    fechar = Some((*id, *como, texto.clone()));
                                }
                                if ui.button("Cancelar").clicked() {
                                    fechar = Some((*id, *como, String::new()));
                                }
                            });
                        }
                        _ => {
                            ui.horizontal(|ui| {
                                if ui.button("Resolver").clicked() {
                                    estado.motivo =
                                        Some((ponto.id, Motivo::Resolver, String::new()));
                                }
                                if ui.button("Ignorar").clicked() {
                                    estado.motivo =
                                        Some((ponto.id, Motivo::Ignorar, String::new()));
                                }
                            });
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
    if let Some((id, como, texto)) = fechar {
        if texto.is_empty() {
            estado.motivo = None;
        } else {
            estado.fechar(id, como, &texto);
        }
    }
}
