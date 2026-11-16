//! `sbf` sem argumentos: a tela de entrada, com "Abrir pasta do cofre…" e "Modo
//! demonstração". Com `FINANCAS_DIR` definido, a pasta é sugerida no campo, mas **nunca**
//! aberta sozinha: só com o clique.

use egui::{Align, Layout, RichText, Ui};
use sbf_janela::{componentes, tema};

pub struct Entrada {
    pasta: String,
    erro: Option<String>,
    atual: Option<Box<dyn eframe::App>>,
}

impl Entrada {
    pub fn nova() -> Self {
        Self {
            pasta: std::env::var("FINANCAS_DIR").unwrap_or_default(),
            erro: None,
            atual: None,
        }
    }
}

impl eframe::App for Entrada {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.atual {
            app.ui(ui, frame);
            return;
        }
        let p = tema::paleta(ui.ctx());
        let mut abrir = false;
        let mut demonstracao = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.superficie_0)
                    .inner_margin(egui::Margin::same(40)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(640.0);
                    componentes::cartao(ui, |ui| {
                        ui.set_width(600.0);
                        componentes::titulo(ui, "Seu Bolso Feliz", 22.0);
                        ui.label(RichText::new("O inventário no desktop (fatia 1).").color(p.tinta_suave));
                        ui.add_space(12.0);
                        ui.label("Pasta do cofre (a que tem o inventario.sqlite)");
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.pasta)
                                .hint_text("/caminho/da/pasta-do-cofre")
                                .desired_width(f32::INFINITY),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            abrir = true;
                        }
                        if std::env::var_os("FINANCAS_DIR").is_some() {
                            ui.label(
                                RichText::new("Sugerida pela variável FINANCAS_DIR. Nada é aberto antes do seu clique.")
                                    .size(12.0)
                                    .color(p.tinta_suave),
                            );
                        }
                        if let Some(e) = &self.erro {
                            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                        }
                        ui.add_space(10.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !self.pasta.trim().is_empty(),
                                    egui::Button::new(RichText::new("Abrir pasta do cofre…").strong()),
                                )
                                .clicked()
                            {
                                abrir = true;
                            }
                            if ui.button("Modo demonstração").clicked() {
                                demonstracao = true;
                            }
                        });
                    });
                });
            });
        if abrir && !self.pasta.trim().is_empty() {
            match crate::inventario::abrir(std::path::Path::new(self.pasta.trim())) {
                Ok(app) => self.atual = Some(Box::new(app)),
                Err(e) => self.erro = Some(format!("Não deu para abrir: {e}")),
            }
        }
        if demonstracao {
            match crate::app_de_demonstracao() {
                Ok(app) => self.atual = Some(Box::new(app)),
                Err(e) => self.erro = Some(e),
            }
        }
    }
}
