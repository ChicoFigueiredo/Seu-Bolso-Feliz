//! Opções › Nuvens: conectar e desconectar Google Drive e OneDrive (rclone, num arquivo de
//! configuração próprio do SBF), a coleta das nuvens ligada ou desligada e a última cópia
//! de segurança. O token nunca passa por aqui: quem o recebe e o grava é o rclone.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use egui::{RichText, Ui};
use sbf_chaveiro::Chaveiro;
use sbf_estante::nuvens::Conexao;
use sbf_estante::{Conexoes, Nuvem, Rclone};
use serde::{Deserialize, Serialize};

use crate::{componentes, tema};

/// Rótulo, no chaveiro, da senha do arquivo de configuração do rclone (só se o usuário o
/// cifrou por conta própria).
pub const ROTULO_DA_SENHA_DA_CONFIGURACAO: &str = "rclone-config";

/// O arquivo de configuração do rclone próprio do SBF, na pasta de dados do app (nunca no
/// cofre).
#[must_use]
pub fn arquivo_de_configuracao(cofre: &Path) -> Option<PathBuf> {
    let pasta = sbf_chaveiro::pasta_de_dados_do_app().ok()?;
    sbf_chaveiro::garantir_fora_do_cofre(&pasta, cofre).ok()?;
    Some(pasta.join("rclone-sbf.conf"))
}

/// O rclone com o arquivo do SBF, se ele já existir e tiver conteúdo; senão o rclone puro
/// (quem configurou o remoto por conta própria continua funcionando).
#[must_use]
pub fn rclone_do_sbf(cofre: &Path, chaveiro: Option<&Chaveiro>) -> Rclone {
    let rclone = Rclone::novo();
    let Some(arquivo) = arquivo_de_configuracao(cofre) else {
        return rclone;
    };
    if std::fs::metadata(&arquivo).map_or(true, |m| m.len() == 0) {
        return rclone;
    }
    let rclone = rclone.com_configuracao(arquivo);
    match chaveiro.and_then(|c| c.senha(ROTULO_DA_SENHA_DA_CONFIGURACAO).ok()) {
        Some(senha) => rclone.com_senha_da_configuracao(&senha),
        None => rclone,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Preferencias {
    #[serde(default)]
    coleta_ligada: bool,
    #[serde(default)]
    ultimo_backup: Option<String>,
}

type Conexao_ = Result<(), String>;

/// O estado da seção Nuvens de Opções.
#[derive(Debug, Default)]
pub struct Nuvens {
    cofre: PathBuf,
    conexoes: Vec<Conexao>,
    lidas: bool,
    conectando: Option<(Nuvem, Receiver<Conexao_>)>,
    aviso: Option<String>,
    /// A coleta das nuvens (Drive e OneDrive) está ligada? O `sbf worker` consulta.
    pub coleta_ligada: bool,
    /// Instante UTC da última cópia de segurança, lembrado entre sessões.
    pub ultimo_backup: Option<String>,
}

impl Nuvens {
    /// Estado inicial; as preferências vêm de [`Nuvens::carregar`].
    #[must_use]
    pub fn nova(cofre: PathBuf) -> Self {
        Self {
            cofre,
            ..Self::default()
        }
    }

    fn arquivo_de_preferencias(&self) -> Option<PathBuf> {
        Some(arquivo_de_configuracao(&self.cofre)?.with_file_name("nuvens.json"))
    }

    /// Lê coleta ligada e último backup.
    pub fn carregar(&mut self) {
        let Some(arquivo) = self.arquivo_de_preferencias() else {
            return;
        };
        if let Ok(texto) = std::fs::read_to_string(arquivo)
            && let Ok(p) = serde_json::from_str::<Preferencias>(&texto)
        {
            self.coleta_ligada = p.coleta_ligada;
            self.ultimo_backup = p.ultimo_backup;
        }
    }

    fn salvar(&mut self) {
        let Some(arquivo) = self.arquivo_de_preferencias() else {
            return;
        };
        let p = Preferencias {
            coleta_ligada: self.coleta_ligada,
            ultimo_backup: self.ultimo_backup.clone(),
        };
        let resultado = (|| -> Result<(), String> {
            if let Some(pasta) = arquivo.parent() {
                std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
            }
            let texto = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
            std::fs::write(&arquivo, texto).map_err(|e| e.to_string())
        })();
        if let Err(e) = resultado {
            self.aviso = Some(format!("Não deu para guardar as opções das nuvens: {e}"));
        }
    }

    /// Guarda o instante da última cópia de segurança.
    pub fn registrar_backup(&mut self, quando: &str) {
        self.ultimo_backup = Some(quando.to_owned());
        self.salvar();
    }

    fn conexoes_do_sbf(&self, chaveiro: Option<&Chaveiro>) -> Result<Conexoes, String> {
        let arquivo = arquivo_de_configuracao(&self.cofre)
            .ok_or("sem pasta de dados do app (ou ela fica dentro do cofre)")?;
        let mut rclone = Rclone::novo().com_configuracao(arquivo);
        if let Some(senha) = chaveiro.and_then(|c| c.senha(ROTULO_DA_SENHA_DA_CONFIGURACAO).ok()) {
            rclone = rclone.com_senha_da_configuracao(&senha);
        }
        Conexoes::nova(rclone).map_err(|e| e.to_string())
    }

    fn reler(&mut self, chaveiro: Option<&Chaveiro>) {
        self.lidas = true;
        match self
            .conexoes_do_sbf(chaveiro)
            .and_then(|c| c.listar().map_err(|e| e.to_string()))
        {
            Ok(c) => self.conexoes = c,
            Err(e) => self.aviso = Some(e),
        }
    }

    fn conectar(&mut self, nuvem: Nuvem, chaveiro: Option<&Chaveiro>) {
        let conexoes = match self.conexoes_do_sbf(chaveiro) {
            Ok(c) => c,
            Err(e) => {
                self.aviso = Some(e);
                return;
            }
        };
        let mut nome = nuvem.nome_sugerido().to_owned();
        let mut n = 2;
        while self.conexoes.iter().any(|c| c.nome == nome) {
            nome = format!("{}{n}", nuvem.nome_sugerido());
            n += 1;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(conexoes.conectar(&nome, nuvem).map_err(|e| e.to_string()));
        });
        self.aviso = None;
        self.conectando = Some((nuvem, rx));
    }

    /// A seção Opções › Nuvens. `destinos_ativos`: quantos destinos da Estante estão ativos.
    pub fn ui(&mut self, ui: &mut Ui, chaveiro: Option<&Chaveiro>, destinos_ativos: usize) {
        let p = tema::paleta(ui.ctx());
        componentes::rotulo_de_secao(ui, "Nuvens");
        if !self.lidas {
            self.reler(chaveiro);
        }
        if let Some((nuvem, rx)) = &self.conectando {
            let nuvem = *nuvem;
            match rx.try_recv() {
                Ok(r) => {
                    self.conectando = None;
                    match r {
                        Ok(()) => self.aviso = None,
                        Err(e) => self.aviso = Some(format!("Não deu para conectar: {e}")),
                    }
                    self.reler(chaveiro);
                }
                Err(TryRecvError::Empty) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Autorize o {} no navegador que se abriu…",
                            nome_da_nuvem(nuvem)
                        ));
                    });
                    ui.ctx().request_repaint_after(Duration::from_millis(500));
                }
                Err(TryRecvError::Disconnected) => {
                    self.conectando = None;
                    self.aviso = Some("A conexão terminou sem resposta.".to_owned());
                }
            }
        }
        let mut desconectar = None;
        for c in &self.conexoes {
            ui.horizontal(|ui| {
                ui.label(RichText::new(nome_da_nuvem(c.nuvem)).strong());
                ui.label(RichText::new(&c.nome).color(p.tinta_suave));
                if ui.small_button("Desconectar").clicked() {
                    desconectar = Some(c.nome.clone());
                }
            });
        }
        if self.conexoes.is_empty() {
            ui.label(RichText::new("Nenhuma nuvem conectada.").color(p.tinta_suave));
        }
        if let Some(nome) = desconectar {
            match self
                .conexoes_do_sbf(chaveiro)
                .and_then(|c| c.desconectar(&nome).map_err(|e| e.to_string()))
            {
                Ok(()) => self.aviso = None,
                Err(e) => self.aviso = Some(format!("Não deu para desconectar: {e}")),
            }
            self.reler(chaveiro);
        }
        let livre = self.conectando.is_none();
        ui.horizontal(|ui| {
            for nuvem in [Nuvem::Drive, Nuvem::OneDrive] {
                if ui
                    .add_enabled(
                        livre,
                        egui::Button::new(format!("Conectar {}", nome_da_nuvem(nuvem))),
                    )
                    .clicked()
                {
                    self.conectar(nuvem, chaveiro);
                }
            }
        });
        ui.label(
            RichText::new("O navegador abre para você autorizar; o acesso fica só neste computador, no arquivo de configuração do SBF. Desconectar apaga esse acesso daqui, mas não o revoga na sua conta Google ou Microsoft.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.add_space(6.0);
        if ui
            .checkbox(
                &mut self.coleta_ligada,
                "Coletar documentos do Drive e do OneDrive",
            )
            .changed()
        {
            self.salvar();
        }
        ui.label(
            RichText::new(format!(
                "Destinos da Estante ativos: {destinos_ativos} (Opções › Estante e cópia de segurança)."
            ))
            .size(12.0)
            .color(p.tinta_suave),
        );
        ui.label(format!(
            "Última cópia de segurança: {}",
            self.ultimo_backup.as_deref().unwrap_or("—")
        ));
        if let Some(a) = &self.aviso {
            ui.colored_label(p.aviso, a);
        }
    }
}

fn nome_da_nuvem(nuvem: Nuvem) -> &'static str {
    match nuvem {
        Nuvem::Drive => "Google Drive",
        Nuvem::OneDrive => "OneDrive",
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn preferencias_ida_e_volta_sem_segredo() {
        let p = Preferencias {
            coleta_ligada: true,
            ultimo_backup: Some("2026-11-14T120000Z".to_owned()),
        };
        let texto = serde_json::to_string(&p).expect("json");
        assert!(!texto.to_lowercase().contains("token"));
        let de: Preferencias = serde_json::from_str(&texto).expect("volta");
        assert!(de.coleta_ligada);
        assert_eq!(de.ultimo_backup.as_deref(), Some("2026-11-14T120000Z"));
    }

    #[test]
    fn estado_inicial_desligado() {
        let n = Nuvens::nova(PathBuf::from("/tmp/cofre-ficticio"));
        assert!(!n.coleta_ligada && n.ultimo_backup.is_none());
    }
}
