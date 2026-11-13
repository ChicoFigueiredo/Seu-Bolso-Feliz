//! O modo `sbf --inventario <pasta-do-cofre>`: abre o `inventario.sqlite` de verdade.
//!
//! - Abrir não grava nada (INV-01). Se o banco precisa migrar, a janela mostra o plano e a
//!   cópia que vai ser feita, e só migra com o seu OK. A migração roda numa thread, com
//!   andamento: num inventário real ela leva dezenas de segundos, e a janela não pode ficar
//!   "não respondendo".
//! - Com o banco em dia, abre a janela completa: `LeitorSqlite` como `Consulta` (conexão
//!   só de leitura), o próprio `Inventario` como `Comandos`, e as Ferramentas (PDF
//!   protegido, estante, cópia de segurança, chaveiro) ligadas ao banco e ao cofre.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, Ui};
use sbf_disco::Cifra;
use sbf_inventario::banco::{Inventario, MigracaoFeita, PlanoDeMigracao};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_janela::ferramentas::{self, Ferramentas};
use sbf_janela::{EstadoDaBarra, Janela, componentes, tema};

type Migracao = JoinHandle<(Inventario, Result<MigracaoFeita, String>)>;

/// O que a tela do inventário real mostra.
enum Tela {
    /// O plano de migração, esperando o OK.
    Migrar {
        plano: PlanoDeMigracao,
        copia: PathBuf,
        erro: Option<String>,
    },
    /// Migrando numa thread.
    Migrando {
        inicio: Instant,
        copia: PathBuf,
        tarefa: Option<Migracao>,
    },
    /// Não deu para abrir a janela completa.
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
    let precisa = plano.precisa_migrar();
    let tela = Tela::Migrar {
        copia: inventario.caminho_sugerido_da_copia(),
        plano,
        erro: None,
    };
    let mut app = AppDoInventario {
        inventario: Some(inventario),
        pasta: pasta.to_path_buf(),
        tela,
        janela: None,
    };
    if !precisa {
        app.ligar(None);
    }
    Ok(app)
}

impl AppDoInventario {
    /// Abre a leitura e troca esta tela pela janela completa.
    fn ligar(&mut self, aviso: Option<String>) {
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
                let mut f =
                    Ferramentas::nova(inventario.caminho().to_path_buf(), self.pasta.clone());
                // Só lê o estado: não pede senha. Se o keyring travar, a barra mostra "—".
                match ferramentas::abrir_chaveiro_com_espera(Duration::from_secs(3)) {
                    Ok(c) => f.chaveiro = Some(c),
                    Err(e) => f.erro_do_chaveiro = Some(e),
                }
                janela.ferramentas = Some(f);
                if let Some(t) = aviso {
                    janela.mensagem = Some(sbf_janela::janela::Mensagem {
                        texto: t,
                        erro: false,
                    });
                }
                match crate::App::novo(janela, Box::new(leitor), Box::new(inventario), barra) {
                    Ok(app) => self.janela = Some(app),
                    Err(e) => {
                        self.tela = Tela::Pronto {
                            aviso: format!("Os temas não carregaram: {e}"),
                        }
                    }
                }
            }
            Err(e) => {
                self.tela = Tela::Pronto {
                    aviso: format!("O banco abriu, mas a leitura não: {e}"),
                };
                self.inventario = Some(inventario);
            }
        }
    }

    fn comecar_migracao(&mut self, copia: PathBuf) {
        let Some(mut inventario) = self.inventario.take() else {
            return;
        };
        let destino = copia.clone();
        let tarefa = std::thread::spawn(move || {
            let r = inventario.migrar(&destino).map_err(|e| e.to_string());
            (inventario, r)
        });
        self.tela = Tela::Migrando {
            inicio: Instant::now(),
            copia,
            tarefa: Some(tarefa),
        };
    }

    /// A migração terminou? Liga a janela (ou volta ao plano com o erro).
    fn sondar_migracao(&mut self) {
        let Tela::Migrando { tarefa, .. } = &mut self.tela else {
            return;
        };
        if !tarefa.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        let Some(h) = tarefa.take() else { return };
        match h.join() {
            Ok((inventario, Ok(feita))) => {
                self.inventario = Some(inventario);
                let aviso = format!(
                    "Migrado da versão {} para a {}.{}",
                    feita.versao_inicial,
                    feita.versao_final,
                    feita
                        .copia
                        .map(|c| format!(" A cópia de antes está em {}.", c.display()))
                        .unwrap_or_default()
                );
                self.ligar(Some(aviso));
            }
            Ok((inventario, Err(e))) => {
                self.tela = match inventario.plano() {
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
                };
                self.inventario = Some(inventario);
            }
            Err(_) => {
                self.tela = Tela::Pronto {
                    aviso: "A migração parou no meio (a thread caiu). O banco e a cópia ficam como estão."
                        .to_owned(),
                };
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
        self.sondar_migracao();
        if self.janela.is_some() {
            ui.ctx().request_repaint();
            return;
        }
        let p = tema::paleta(ui.ctx());
        let caminho = self.pasta.join("inventario.sqlite").display().to_string();
        let mut migrar = None;
        let mut sair = false;
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
                        ui.label(RichText::new(&caminho).size(12.0).color(p.tinta_suave));
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
                                    ui.label(format!(
                                        "• versão {}: {}",
                                        passo.versao, passo.descricao
                                    ));
                                }
                                ui.add_space(6.0);
                                if plano.faz_copia {
                                    componentes::aviso(
                                        ui,
                                        "backup",
                                        &format!(
                                            "Antes, uma cópia do banco vai para {}",
                                            copia.display()
                                        ),
                                        p.info_sutil,
                                        p.info_borda,
                                        p.info,
                                    );
                                }
                                if let Some(e) = erro {
                                    componentes::aviso(
                                        ui,
                                        "error",
                                        e,
                                        p.perigo_sutil,
                                        p.borda,
                                        p.perigo,
                                    );
                                }
                                ui.add_space(10.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button(RichText::new("Migrar").strong()).clicked() {
                                        migrar = Some(copia.clone());
                                    }
                                    if ui.button("Sair sem mudar nada").clicked() {
                                        sair = true;
                                    }
                                });
                            }
                            Tela::Migrando { inicio, copia, .. } => {
                                componentes::titulo(ui, "Migrando o inventário", 20.0);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new());
                                    ui.label(format!(
                                        "Copiando o banco para {} e migrando… {} s",
                                        copia.display(),
                                        inicio.elapsed().as_secs()
                                    ));
                                });
                                ui.label(
                                    RichText::new(
                                        "Num inventário grande isso leva dezenas de segundos. Não feche a janela.",
                                    )
                                    .size(12.0)
                                    .color(p.tinta_suave),
                                );
                                ui.ctx().request_repaint_after(Duration::from_millis(200));
                            }
                            Tela::Pronto { aviso } => {
                                componentes::titulo(ui, "Inventário", 20.0);
                                ui.label(aviso.as_str());
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
        if let Some(copia) = migrar {
            self.comecar_migracao(copia);
        }
        if sair {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
