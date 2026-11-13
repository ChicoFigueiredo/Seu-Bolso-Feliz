//! O modo `sbf --inventario <pasta-do-cofre>`: abre o `inventario.sqlite` de verdade.
//!
//! - Abrir não grava nada (INV-01). Se o banco precisa migrar, a janela mostra o plano e a
//!   cópia que vai ser feita, e só migra com o seu OK.
//! - Com o banco em dia, abre a janela completa: `LeitorSqlite` como `Consulta` (conexão
//!   só de leitura) e o próprio `Inventario` como `Comandos`.

use std::path::{Path, PathBuf};

use egui::{Align, Layout, RichText, Ui};
use sbf_disco::Cifra;
use sbf_inventario::banco::{Inventario, PlanoDeMigracao};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_janela::{EstadoDaBarra, Janela, MolduraProvisoria, componentes, tema};

/// O que a tela do inventário real mostra.
enum Tela {
    /// O plano de migração, esperando o OK.
    Migrar {
        plano: PlanoDeMigracao,
        copia: PathBuf,
        erro: Option<String>,
    },
    /// Banco em dia (ou recém-migrado), esperando a leitura ser ligada.
    Pronto { aviso: String },
}

pub struct AppDoInventario {
    inventario: Option<Inventario>,
    pasta: PathBuf,
    tela: Tela,
    janela: Option<crate::App>,
}

/// Abre o banco da pasta do cofre sem gravar nada.
pub fn abrir(pasta: &Path) -> Result<AppDoInventario, String> {
    let banco = pasta.join("inventario.sqlite");
    if !banco.is_file() {
        return Err(format!("não achei {} (nada foi criado)", banco.display()));
    }
    let inventario = Inventario::abrir(&banco).map_err(|e| e.to_string())?;
    let plano = inventario.plano().map_err(|e| e.to_string())?;
    let tela = if plano.precisa_migrar() {
        Tela::Migrar {
            copia: inventario.caminho_sugerido_da_copia(),
            plano,
            erro: None,
        }
    } else {
        Tela::Pronto {
            aviso: format!("O inventário está em dia (versão {}).", plano.versao_atual),
        }
    };
    let mut app = AppDoInventario {
        inventario: Some(inventario),
        pasta: pasta.to_path_buf(),
        tela,
        janela: None,
    };
    if matches!(app.tela, Tela::Pronto { .. }) {
        app.ligar();
    }
    Ok(app)
}

impl AppDoInventario {
    /// Abre a leitura e troca esta tela pela janela completa.
    fn ligar(&mut self) {
        let Some(inventario) = self.inventario.take() else {
            return;
        };
        match LeitorSqlite::abrir(inventario.caminho()) {
            Ok(leitor) => {
                let cifra = sbf_disco::cifra_do_volume(&self.pasta);
                let barra = EstadoDaBarra {
                    inventario: format!(
                        "{} · inventario.sqlite",
                        self.pasta.file_name().map_or_else(
                            || self.pasta.display().to_string(),
                            |n| n.to_string_lossy().into_owned()
                        )
                    ),
                    disco_cifrado: Some(
                        match cifra {
                            Cifra::Cifrado => "sim",
                            Cifra::NaoCifrado => "não",
                            Cifra::Desconhecido => "não se sabe",
                        }
                        .to_owned(),
                    ),
                    aviso_de_disco: sbf_disco::texto_do_aviso(cifra).map(str::to_owned),
                    ..Default::default()
                };
                let mut janela = Janela::nova();
                janela.ligacoes.cofre = true;
                self.janela = Some(crate::App {
                    moldura: MolduraProvisoria::nova(janela, barra),
                    consulta: Box::new(leitor),
                    comandos: Box::new(inventario),
                    captura: std::env::var_os("SBF_CAPTURA").map(PathBuf::from),
                    quadros: 0,
                });
            }
            Err(e) => {
                self.tela = Tela::Pronto {
                    aviso: format!("O banco abriu, mas a leitura não: {e}"),
                };
                self.inventario = Some(inventario);
            }
        }
    }
}

impl eframe::App for AppDoInventario {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.janela {
            app.ui(ui, frame);
            return;
        }
        let Some(inventario) = &mut self.inventario else {
            return;
        };
        let p = tema::paleta(ui.ctx());
        let mut migrar = false;
        let mut sair = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.superficie_0).inner_margin(egui::Margin::same(40)))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(640.0);
                    componentes::cartao(ui, |ui| {
                        ui.set_width(600.0);
                        ui.label(RichText::new(inventario.caminho().display().to_string()).size(12.0).color(p.tinta_suave));
                        ui.add_space(6.0);
                        match &self.tela {
                            Tela::Migrar { plano, copia, erro } => {
                                componentes::titulo(ui, "O inventário precisa ser migrado", 20.0);
                                ui.label(format!(
                                    "Da versão {} para a {}. Nada muda no banco até você confirmar.",
                                    plano.versao_atual, plano.versao_final
                                ));
                                ui.add_space(6.0);
                                for passo in &plano.passos {
                                    ui.label(format!("• versão {}: {}", passo.versao, passo.descricao));
                                }
                                ui.add_space(6.0);
                                if plano.faz_copia {
                                    componentes::aviso(ui, "backup", &format!("Antes, uma cópia do banco vai para {}", copia.display()), p.info_sutil, p.info_borda, p.info);
                                }
                                if let Some(e) = erro {
                                    componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                                }
                                ui.add_space(10.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button(RichText::new("Migrar").strong()).clicked() {
                                        migrar = true;
                                    }
                                    if ui.button("Sair sem mudar nada").clicked() {
                                        sair = true;
                                    }
                                });
                            }
                            Tela::Pronto { aviso } => {
                                componentes::titulo(ui, "Inventário aberto", 20.0);
                                ui.label(aviso.as_str());
                                ui.add_space(6.0);
                                componentes::aviso(
                                    ui,
                                    "info",
                                    "A leitura do inventário real nesta janela chega quando o LeitorSqlite for ligado. Até lá, use sbf --demonstracao para ver a janela com dados fictícios.",
                                    p.info_sutil,
                                    p.info_borda,
                                    p.info,
                                );
                                ui.add_space(10.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Fechar").clicked() {
                                        sair = true;
                                    }
                                });
                            }
                        }
                    });
                });
            });
        let mut migrou = None;
        if migrar && let Tela::Migrar { copia, .. } = &self.tela {
            let copia = copia.clone();
            self.tela = match inventario.migrar(&copia) {
                Ok(feita) => {
                    let t = Tela::Pronto {
                        aviso: format!(
                            "Migrado da versão {} para a {}.{}",
                            feita.versao_inicial,
                            feita.versao_final,
                            feita
                                .copia
                                .map(|c| format!(" A cópia de antes está em {}.", c.display()))
                                .unwrap_or_default()
                        ),
                    };
                    migrou = Some(());
                    t
                }
                Err(e) => match inventario.plano() {
                    Ok(plano) => Tela::Migrar {
                        plano,
                        copia: inventario.caminho_sugerido_da_copia(),
                        erro: Some(format!("A migração não foi feita: {e}")),
                    },
                    Err(e2) => Tela::Pronto {
                        aviso: format!(
                            "A migração falhou ({e}) e o banco não pôde ser relido ({e2})."
                        ),
                    },
                },
            };
        }
        if migrou.is_some() {
            let aviso = match &self.tela {
                Tela::Pronto { aviso } => aviso.clone(),
                Tela::Migrar { .. } => String::new(),
            };
            self.ligar();
            if let Some(app) = &mut self.janela {
                app.moldura.janela.mensagem = Some(sbf_janela::janela::Mensagem {
                    texto: aviso,
                    erro: false,
                });
            }
        }
        if sair {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
