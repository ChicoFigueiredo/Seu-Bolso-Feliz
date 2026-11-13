//! A moldura **provisória** da janela, até a `bancada-casca` chegar: barra de título com
//! a Barra de Acesso Rápido, uma faixa de Ações simples no lugar do Ribbon (as abas e os
//! grupos vêm da lista única), painel de navegação, centro, dock e barra de status.
//!
//! O integrador troca esta moldura pela casca e liga as mesmas peças de [`Janela`]:
//! `navegacao_ui`, `centro_ui`, `dock_ui`, `flutuantes_ui`, `dialogos_ui`, `atalhos` e
//! `processar`.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui};
use sbf_inventario::{Comandos, Consulta, Tipo};

use crate::acoes::{self, ACOES, Aba};
use crate::componentes;
use crate::formato;
use crate::icones;
use crate::janela::Janela;
use crate::tema;

/// O que a barra de status mostra. O que ainda não existe fica `None` e aparece como "—".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EstadoDaBarra {
    /// O inventário aberto ("Modo demonstração · dados fictícios", ou o caminho do banco).
    pub inventario: String,
    /// "aberto" ou "fechado".
    pub chaveiro: Option<String>,
    /// Se o disco do cofre é cifrado: "sim", "não" ou "não se sabe" (ADR-015).
    pub disco_cifrado: Option<String>,
    /// O texto do aviso de disco sem cifra (`sbf_disco::texto_do_aviso`). Com ele, a
    /// barra mostra o aviso com ícone e texto, e o texto inteiro no tooltip.
    pub aviso_de_disco: Option<String>,
    pub ultima_copia: Option<String>,
    /// Selo à direita do título ("Demonstração · dados fictícios").
    pub selo: Option<String>,
}

/// A moldura provisória com a janela dentro.
#[derive(Debug)]
pub struct MolduraProvisoria {
    pub janela: Janela,
    pub barra: EstadoDaBarra,
    aba: Aba,
    titulo: String,
}

impl MolduraProvisoria {
    pub fn nova(janela: Janela, barra: EstadoDaBarra) -> Self {
        Self {
            janela,
            barra,
            aba: Aba::Triagem,
            titulo: String::new(),
        }
    }

    /// Instala as fontes e aplica o tema da janela. Chame uma vez, no início.
    pub fn preparar(&self, ctx: &egui::Context) {
        tema::instalar_fontes(ctx);
        tema::aplicar(ctx, &self.janela.tema);
    }

    /// Desenha um quadro e aplica os pedidos dele.
    pub fn ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta, comandos: &mut dyn Comandos) {
        let ctx = ui.ctx().clone();
        let p = tema::paleta(&ctx);
        self.janela.atalhos(&ctx);

        let titulo = self.janela.titulo(consulta);
        if titulo != self.titulo {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.titulo = titulo.clone();
        }

        egui::Panel::top("sbf-titulo")
            .frame(
                Frame::new()
                    .fill(p.superficie_2)
                    .inner_margin(Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| self.barra_de_titulo(ui, &titulo));
        egui::Panel::top("sbf-faixa")
            .frame(
                Frame::new()
                    .fill(p.superficie_2)
                    .stroke(Stroke::new(1.0, p.borda))
                    .inner_margin(Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| self.faixa_de_acoes(ui, consulta));
        egui::Panel::bottom("sbf-status")
            .frame(
                Frame::new()
                    .fill(p.superficie_2)
                    .stroke(Stroke::new(1.0, p.borda))
                    .inner_margin(Margin::symmetric(10, 3)),
            )
            .show(ui, |ui| self.barra_de_status(ui, consulta));
        egui::Panel::left("sbf-navegacao")
            .resizable(true)
            .default_size(250.0)
            .frame(
                Frame::new()
                    .fill(p.superficie_2)
                    .stroke(Stroke::new(1.0, p.borda))
                    .inner_margin(Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| self.janela.navegacao_ui(ui, consulta));
        if !self.janela.paineis_visiveis.is_empty() {
            egui::Panel::right("sbf-dock")
                .resizable(true)
                .default_size(330.0)
                .frame(
                    Frame::new()
                        .fill(p.superficie_1)
                        .stroke(Stroke::new(1.0, p.borda))
                        .inner_margin(Margin::same(0)),
                )
                .show(ui, |ui| self.dock(ui, consulta));
        }
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.superficie_1)
                    .inner_margin(Margin::symmetric(14, 8)),
            )
            .show(ui, |ui| self.janela.centro_ui(ui, consulta));

        self.janela.flutuantes_ui(&ctx, consulta);
        self.janela.dialogos_ui(&ctx);
        self.janela.processar(&ctx, consulta, comandos);
    }

    fn barra_de_titulo(&mut self, ui: &mut Ui, titulo: &str) {
        let p = tema::paleta(ui.ctx());
        let contexto = self.janela.contexto_das_acoes();
        ui.horizontal(|ui| {
            for id in ACOES
                .iter()
                .filter(|d| d.aba == Aba::AcessoRapido)
                .map(|d| d.id)
            {
                let def = acoes::definicao(id);
                let disp = acoes::disponibilidade(id, &contexto);
                let mut dica = def.rotulo.to_owned();
                if let Some(a) = def.atalho {
                    dica.push_str(&format!(" ({})", a.texto()));
                }
                let dica = match disp.motivo() {
                    Some(m) => format!("{dica}: {m}"),
                    None => dica,
                };
                if componentes::botao_icone(ui, def.icone, &dica, disp.habilitada()).clicked() {
                    self.janela.executar(id);
                }
            }
            ui.separator();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(s) = &self.barra.selo {
                    componentes::selo(ui, s, p.ouro_sutil, p.ouro_tinta);
                }
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(titulo).strong().color(p.tinta));
                });
            });
        });
    }

    fn faixa_de_acoes(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        let contexto = self.janela.contexto_das_acoes();
        let achado = contexto.selecao.len() == 1
            && matches!(contexto.selecao[0].tipo, Tipo::Achado | Tipo::Documento);
        let pagina = contexto.item_aberto.is_some() || contexto.selecao.len() == 1;
        let mut abas = vec![
            Aba::Inicio,
            Aba::Triagem,
            Aba::Lotes,
            Aba::Ferramentas,
            Aba::Exibir,
        ];
        if achado {
            abas.push(Aba::Achado);
        }
        if pagina {
            abas.push(Aba::Pagina);
        }
        if !abas.contains(&self.aba) {
            self.aba = Aba::Triagem;
        }
        ui.horizontal(|ui| {
            for a in abas {
                let ativa = a == self.aba;
                let texto =
                    RichText::new(a.rotulo()).color(if ativa { p.destaque_tinta } else { p.tinta });
                let r = ui.add(
                    egui::Button::new(if ativa { texto.strong() } else { texto }).frame(false),
                );
                if ativa {
                    ui.painter().hline(
                        r.rect.x_range(),
                        r.rect.bottom() + 1.0,
                        Stroke::new(2.0, p.destaque),
                    );
                }
                if r.clicked() {
                    self.aba = a;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new("faixa provisória · o Ribbon chega com a casca")
                        .size(11.5)
                        .color(p.tinta_desabilitada),
                );
            });
        });
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            let mut grupo_atual = "";
            for def in ACOES.iter().filter(|d| d.aba == self.aba) {
                if def.grupo != grupo_atual {
                    if !grupo_atual.is_empty() {
                        ui.separator();
                    }
                    ui.label(RichText::new(def.grupo).size(11.0).color(p.tinta_suave));
                    grupo_atual = def.grupo;
                }
                if componentes::botao_de_acao(ui, def.id, &contexto) {
                    self.janela.executar(def.id);
                }
            }
            if self.aba == Aba::Lotes {
                ui.separator();
                ui.label(RichText::new("Lote atual").size(11.0).color(p.tinta_suave));
                let rotulo = self
                    .janela
                    .rotulo_do_lote_atual(consulta)
                    .unwrap_or_else(|| "nenhum: crie um ou clique num lote".to_owned());
                ui.label(RichText::new(rotulo).strong());
            }
            if self.aba == Aba::Exibir {
                ui.separator();
                ui.label(RichText::new("Tema").size(11.0).color(p.tinta_suave));
                let atual = self.janela.tema.clone();
                let mut escolhido = atual.clone();
                egui::ComboBox::from_id_salt("sbf-tema")
                    .selected_text(
                        tema::temas()
                            .into_iter()
                            .find(|t| t.id == atual)
                            .map_or(atual.clone(), |t| t.nome),
                    )
                    .show_ui(ui, |ui| {
                        for t in tema::temas() {
                            ui.selectable_value(
                                &mut escolhido,
                                t.id.clone(),
                                format!("{} · {}", t.nome, t.familia),
                            );
                        }
                    });
                if escolhido != atual {
                    self.janela.tema = escolhido;
                    tema::aplicar(ui.ctx(), &self.janela.tema);
                }
            }
        });
    }

    fn dock(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        let paineis = self.janela.paineis_visiveis.clone();
        let altura = ui.available_height() / paineis.len().max(1) as f32;
        for painel in paineis {
            ui.allocate_ui(egui::vec2(ui.available_width(), altura), |ui| {
                Frame::new()
                    .fill(p.superficie_2)
                    .stroke(Stroke::new(1.0, p.borda))
                    .inner_margin(Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(icones::icone_cor(painel.icone(), 16.0, p.tinta_suave));
                            ui.label(RichText::new(painel.titulo()).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if componentes::botao_icone(ui, "close", "Fechar painel", true)
                                    .clicked()
                                {
                                    self.janela.paineis_visiveis.retain(|x| *x != painel);
                                }
                            });
                        });
                    });
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 6))
                    .corner_radius(CornerRadius::ZERO)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("dock", painel))
                            .auto_shrink([false, false])
                            .max_height(altura - 44.0)
                            .show(ui, |ui| self.janela.dock_ui(ui, consulta, painel));
                    });
            });
        }
    }

    fn barra_de_status(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        let contagens = self.janela.contagens(consulta);
        let traco = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_owned());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let item = |ui: &mut Ui, icone: &str, texto: String, cor: egui::Color32| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(icones::icone_cor(icone, 14.0, cor));
                    ui.label(RichText::new(texto).size(12.0).color(cor));
                });
            };
            item(
                ui,
                "inventory_2",
                self.barra.inventario.clone(),
                p.tinta_suave,
            );
            if let Some(c) = contagens {
                item(
                    ui,
                    "find_in_page",
                    format!("{} achados", formato::inteiro(c.achados)),
                    p.tinta_suave,
                );
                item(
                    ui,
                    "task_alt",
                    format!("{} documentos", formato::inteiro(c.documentos)),
                    p.tinta_suave,
                );
                item(
                    ui,
                    "help",
                    format!("{} em dúvida", formato::inteiro(c.em_duvida)),
                    p.tinta_suave,
                );
                item(
                    ui,
                    "schedule",
                    format!("{} a triar", formato::inteiro(c.a_triar)),
                    p.tinta_suave,
                );
            }
            item(
                ui,
                "key",
                format!("Chaveiro {}", traco(&self.barra.chaveiro)),
                p.tinta_suave,
            );
            let disco = traco(&self.barra.disco_cifrado);
            match &self.barra.aviso_de_disco {
                Some(aviso) => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(icones::icone_cor("warning", 14.0, p.aviso));
                        ui.label(
                            RichText::new(format!("Disco cifrado: {disco}"))
                                .size(12.0)
                                .color(p.aviso),
                        )
                        .on_hover_text(aviso.as_str());
                    });
                }
                None => item(
                    ui,
                    "hard_drive",
                    format!("Disco cifrado: {disco}"),
                    p.tinta_suave,
                ),
            }
            item(
                ui,
                "backup",
                format!("Última cópia: {}", traco(&self.barra.ultima_copia)),
                p.tinta_suave,
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("sbf {}", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(p.tinta_desabilitada),
                );
            });
        });
    }
}
