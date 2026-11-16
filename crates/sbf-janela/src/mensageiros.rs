//! Opções › Mensageiros: o interruptor da coleta, o pareamento do WhatsApp por QR (com o aviso
//! de risco do ADR-017), a escolha das conversas que entram e a importação do export do
//! Telegram Desktop. **Padrão: nenhuma conversa incluída.** Só leitura; nada aqui envia.
//!
//! O que é lento (abrir o export, listar conversas pela ponte, importar) roda numa thread; a
//! interface só espia o canal.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use egui::{Color32, RichText, Ui, Vec2};
use sbf_chaveiro::Chaveiro;
use sbf_mensageiros::ciclo::{ARQUIVO_DA_CONFIG, rodar_no_cofre};
use sbf_mensageiros::config::Config;
use sbf_mensageiros::modelo::{Conversa, Plataforma};
use sbf_mensageiros::telegram::ExportDoTelegram;
use sbf_mensageiros::whatsapp::ponte::{
    Pareamento, Passo, PonteDoWhatsapp, ProcessoExterno, programa_da_ponte,
};
use sbf_mensageiros::whatsapp::sessao::SessaoCifrada;
use sbf_mensageiros::{Erro, FonteDeMensagens as _};

use crate::{componentes, tema};

/// O aviso do ADR-017, palavra por palavra do que o CEO aceitou, em linguagem do app.
pub const AVISO_DO_WHATSAPP: &str = "O WhatsApp não oferece acesso oficial às suas conversas. \
O SBF entra como um aparelho conectado (como o WhatsApp Web), só lê e nunca envia nada. Mesmo \
assim, os termos do WhatsApp proíbem acesso automatizado não autorizado e preveem a suspensão da \
conta; você aceitou esse risco. Só funciona nesta estação Linux. Abra o WhatsApp no celular pelo \
menos a cada 14 dias para manter o vínculo, e não deixe esta estação desligada por mais de 30 \
dias, senão mensagens se perdem.";

type Lista = Result<Vec<Conversa>, String>;

/// O estado da seção Mensageiros.
#[derive(Default)]
pub struct Mensageiros {
    cofre: PathBuf,
    config: Config,
    carregada: bool,
    aviso: Option<String>,
    /// O usuário marcou "entendo o risco".
    entendeu_o_risco: bool,
    pareamento: Option<Pareamento<ProcessoExterno>>,
    qr: Option<String>,
    estado_do_pareamento: Option<String>,
    conversas_whatsapp: Vec<Conversa>,
    conversas_telegram: Vec<Conversa>,
    listando: Option<(Plataforma, Receiver<Lista>)>,
    importando: Option<Receiver<Result<String, String>>>,
    export_texto: String,
    ponte_texto: String,
}

impl std::fmt::Debug for Mensageiros {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mensageiros")
            .field("ligado", &self.config.ligado)
            .finish_non_exhaustive()
    }
}

fn pasta_de_dados(cofre: &Path) -> Option<PathBuf> {
    let pasta = sbf_chaveiro::pasta_de_dados_do_app().ok()?;
    sbf_chaveiro::garantir_fora_do_cofre(&pasta, cofre).ok()?;
    Some(pasta)
}

impl Mensageiros {
    /// Estado inicial; a configuração vem de [`Mensageiros::carregar`].
    #[must_use]
    pub fn nova(cofre: PathBuf) -> Self {
        Self {
            cofre,
            ..Self::default()
        }
    }

    fn arquivo(&self) -> Option<PathBuf> {
        Some(pasta_de_dados(&self.cofre)?.join(ARQUIVO_DA_CONFIG))
    }

    /// Lê `mensageiros.json` (sem arquivo: tudo desligado, nenhuma conversa incluída).
    pub fn carregar(&mut self) {
        self.carregada = true;
        if let Some(a) = self.arquivo() {
            match Config::carregar(&a) {
                Ok(c) => self.config = c,
                Err(_) => {
                    self.aviso =
                        Some("O arquivo de mensageiros está ilegível; usando o padrão.".into())
                }
            }
        }
        self.export_texto = self
            .config
            .export_do_telegram
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.ponte_texto = self
            .config
            .ponte_do_whatsapp
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }

    fn salvar(&mut self) {
        let Some(a) = self.arquivo() else {
            self.aviso = Some("Sem pasta de dados do app (ou ela fica dentro do cofre).".into());
            return;
        };
        if let Err(e) = self.config.salvar(&a) {
            self.aviso = Some(format!("Não deu para guardar as opções: {e}"));
        }
    }

    fn sessao(&self, chaveiro: Option<&Chaveiro>) -> Option<SessaoCifrada> {
        Some(SessaoCifrada::nova(
            &pasta_de_dados(&self.cofre)?,
            chaveiro?.clone(),
        ))
    }

    fn parear(&mut self, chaveiro: Option<&Chaveiro>) {
        let (Some(sessao), Some(ponte)) = (
            self.sessao(chaveiro),
            programa_da_ponte(self.config.ponte_do_whatsapp.as_ref()),
        ) else {
            self.aviso =
                Some("Precisa do chaveiro aberto e do executável da ponte do WhatsApp.".into());
            return;
        };
        let r = ProcessoExterno::subir(&ponte).and_then(|c| Pareamento::iniciar(c, sessao));
        match r {
            Ok(p) => {
                self.pareamento = Some(p);
                self.qr = None;
                self.estado_do_pareamento = Some("Aguardando o QR…".into());
                self.aviso = None;
            }
            Err(e) => self.aviso = Some(format!("Não deu para iniciar o pareamento: {e}")),
        }
    }

    fn sondar_pareamento(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pareamento.as_mut() else {
            return;
        };
        for passo in p.sondar() {
            match passo {
                Passo::Qr(c) => {
                    self.qr = Some(c);
                    self.estado_do_pareamento = Some(
                        "Escaneie em WhatsApp › Aparelhos conectados › Conectar um aparelho."
                            .into(),
                    );
                }
                Passo::Pareado => {
                    self.qr = None;
                    self.estado_do_pareamento =
                        Some("Pareado. A sessão está guardada cifrada.".into());
                }
                Passo::Falhou(m) => {
                    self.qr = None;
                    self.aviso = Some(format!("O pareamento não terminou: {m}"));
                    self.estado_do_pareamento = None;
                }
            }
        }
        if p.terminou() {
            self.pareamento = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn desparear(&mut self, chaveiro: Option<&Chaveiro>) {
        self.pareamento = None;
        self.qr = None;
        let Some(sessao) = self.sessao(chaveiro) else {
            self.aviso = Some("Precisa do chaveiro aberto para desparear.".into());
            return;
        };
        let canal = programa_da_ponte(self.config.ponte_do_whatsapp.as_ref())
            .and_then(|p| ProcessoExterno::subir(&p).ok());
        let r = match canal {
            Some(c) => PonteDoWhatsapp::nova(c, sessao).desparear(),
            None => sessao.apagar(),
        };
        match r {
            Ok(()) => {
                self.conversas_whatsapp.clear();
                self.estado_do_pareamento = Some(
                    "Despareado. Remova também o aparelho no celular, se ele ainda aparecer."
                        .into(),
                );
            }
            Err(e) => self.aviso = Some(format!("Não deu para desparear: {e}")),
        }
    }

    fn listar_whatsapp(&mut self, chaveiro: Option<&Chaveiro>) {
        let (Some(sessao), Some(ponte)) = (
            self.sessao(chaveiro),
            programa_da_ponte(self.config.ponte_do_whatsapp.as_ref()),
        ) else {
            self.aviso =
                Some("Precisa do chaveiro aberto e do executável da ponte do WhatsApp.".into());
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = ProcessoExterno::subir(&ponte)
                .and_then(|c| PonteDoWhatsapp::nova(c, sessao).listar_conversas())
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.listando = Some((Plataforma::Whatsapp, rx));
    }

    fn abrir_export(&mut self) {
        let pasta = PathBuf::from(self.export_texto.trim());
        self.config.export_do_telegram = Some(pasta.clone());
        self.salvar();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = ExportDoTelegram::abrir(&pasta)
                .and_then(|mut e| e.listar_conversas())
                .map_err(|e| match e {
                    Erro::NaoPronta => "Não achei o result.json nessa pasta.".to_owned(),
                    Erro::Formato(_) => "Esse result.json não é um export do Telegram.".to_owned(),
                    e => e.to_string(),
                });
            let _ = tx.send(r);
        });
        self.listando = Some((Plataforma::Telegram, rx));
    }

    fn importar_agora(&mut self, chaveiro: Option<&Chaveiro>) {
        let Some(dados) = pasta_de_dados(&self.cofre) else {
            self.aviso = Some("Sem pasta de dados do app.".into());
            return;
        };
        let cofre = self.cofre.clone();
        let chaveiro = chaveiro.cloned();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let abrir = || {
                chaveiro
                    .clone()
                    .ok_or(Erro::Chaveiro("chaveiro fechado".into()))
            };
            let r = rodar_no_cofre(&dados, &cofre, &abrir, &AtomicBool::new(false))
                .map(|r| {
                    format!(
                        "{} mensagens vistas, {} itens novos no inventário{}.",
                        r.mensagens,
                        r.novos,
                        if r.falhas > 0 {
                            format!(", {} fonte(s) com falha", r.falhas)
                        } else {
                            String::new()
                        }
                    )
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.importando = Some(rx);
    }

    fn espiar_threads(&mut self, ctx: &egui::Context) {
        if let Some((plataforma, rx)) = &self.listando {
            let plataforma = *plataforma;
            match rx.try_recv() {
                Ok(Ok(l)) => {
                    match plataforma {
                        Plataforma::Whatsapp => self.conversas_whatsapp = l,
                        Plataforma::Telegram => self.conversas_telegram = l,
                    }
                    self.listando = None;
                    self.aviso = None;
                }
                Ok(Err(e)) => {
                    self.listando = None;
                    self.aviso = Some(format!("Não deu para listar as conversas: {e}"));
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => self.listando = None,
            }
        }
        if let Some(rx) = &self.importando {
            match rx.try_recv() {
                Ok(r) => {
                    self.importando = None;
                    self.aviso = Some(match r {
                        Ok(t) => t,
                        Err(e) => format!("A importação falhou: {e}"),
                    });
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => self.importando = None,
            }
        }
    }

    /// A seção Opções › Mensageiros.
    pub fn ui(&mut self, ui: &mut Ui, chaveiro: Option<&Chaveiro>) {
        let p = tema::paleta(ui.ctx());
        if !self.carregada {
            self.carregar();
        }
        self.sondar_pareamento(ui.ctx());
        self.espiar_threads(ui.ctx());
        componentes::rotulo_de_secao(ui, "Mensageiros");
        let mut mudou = false;
        mudou |= ui
            .checkbox(
                &mut self.config.ligado,
                "Coletar mensagens do WhatsApp e do Telegram (o worker faz isso com a janela fechada)",
            )
            .changed();
        ui.label(
            RichText::new("Só entram as conversas que você marcar abaixo. Mídias e documentos viram achados; textos entram quando parecem financeiros. Nada é enviado, apagado nem marcado como lido.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.add_space(8.0);

        // WhatsApp
        ui.label(RichText::new("WhatsApp").strong());
        ui.colored_label(p.aviso, AVISO_DO_WHATSAPP);
        let linux = cfg!(target_os = "linux");
        if !linux {
            ui.colored_label(
                p.aviso,
                "O WhatsApp só é suportado em estação Linux por enquanto.",
            );
        }
        let pareado = self.sessao(chaveiro).is_some_and(|s| s.existe());
        ui.label(if pareado {
            "Estado: pareado."
        } else {
            "Estado: não pareado."
        });
        ui.horizontal(|ui| {
            ui.label("Ponte (executável):");
            if ui.text_edit_singleline(&mut self.ponte_texto).lost_focus() {
                let t = self.ponte_texto.trim();
                self.config.ponte_do_whatsapp = (!t.is_empty()).then(|| PathBuf::from(t));
                mudou = true;
            }
        });
        ui.checkbox(
            &mut self.entendeu_o_risco,
            "Entendo o risco para a minha conta",
        );
        ui.horizontal(|ui| {
            let livre = linux && self.pareamento.is_none() && self.entendeu_o_risco;
            if ui
                .add_enabled(livre && !pareado, egui::Button::new("Parear por QR"))
                .clicked()
            {
                self.parear(chaveiro);
            }
            if ui
                .add_enabled(
                    pareado || self.pareamento.is_some(),
                    egui::Button::new("Desparear"),
                )
                .clicked()
            {
                self.desparear(chaveiro);
            }
            if ui
                .add_enabled(
                    pareado && self.listando.is_none(),
                    egui::Button::new("Carregar conversas"),
                )
                .clicked()
            {
                self.listar_whatsapp(chaveiro);
            }
        });
        if let Some(t) = &self.estado_do_pareamento {
            ui.label(t);
        }
        if let Some(codigo) = &self.qr {
            desenhar_qr(ui, codigo);
        }
        mudou |= escolha_de_conversas(
            ui,
            "whatsapp",
            &self.conversas_whatsapp,
            &mut self.config,
            Plataforma::Whatsapp,
        );
        ui.add_space(10.0);

        // Telegram
        ui.label(RichText::new("Telegram (export do Telegram Desktop)").strong());
        ui.label(
            RichText::new("No Telegram Desktop: Configurações › Avançado › Exportar dados do Telegram, formato JSON, com mídia. Aponte para a pasta que tem o result.json.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.export_texto);
            if ui
                .add_enabled(
                    self.listando.is_none() && !self.export_texto.trim().is_empty(),
                    egui::Button::new("Abrir export"),
                )
                .clicked()
            {
                self.abrir_export();
            }
        });
        mudou |= escolha_de_conversas(
            ui,
            "telegram",
            &self.conversas_telegram,
            &mut self.config,
            Plataforma::Telegram,
        );
        ui.add_space(6.0);
        if self.importando.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Importando…");
            });
        } else if ui
            .add_enabled(self.config.ligado, egui::Button::new("Importar agora"))
            .clicked()
        {
            self.importar_agora(chaveiro);
        }
        if let Some(a) = &self.aviso {
            ui.colored_label(p.aviso, a);
        }
        if mudou {
            self.config.whatsapp.ligada = true;
            self.config.telegram.ligada = true;
            self.salvar();
        }
    }
}

/// A lista de conversas com uma caixa por conversa. Devolve se algo mudou.
fn escolha_de_conversas(
    ui: &mut Ui,
    id: &str,
    conversas: &[Conversa],
    config: &mut Config,
    plataforma: Plataforma,
) -> bool {
    let mut mudou = false;
    if conversas.is_empty() {
        let n = config.escolha(plataforma).incluidas.len();
        ui.label(format!(
            "{n} conversa(s) incluída(s). Carregue a lista para escolher."
        ));
        return false;
    }
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(180.0)
        .show(ui, |ui| {
            for c in conversas {
                let mut marcada = config.escolha(plataforma).incluidas.contains(&c.id);
                let rotulo = if c.grupo {
                    format!("{} (grupo)", c.nome)
                } else {
                    c.nome.clone()
                };
                if ui.checkbox(&mut marcada, rotulo).changed() {
                    config.escolha_mut(plataforma).incluir(&c.id, marcada);
                    mudou = true;
                }
            }
        });
    mudou
}

/// Desenha o QR (módulos escuros sobre fundo claro, com a margem de silêncio).
fn desenhar_qr(ui: &mut Ui, codigo: &str) {
    let Ok(qr) = qrcode::QrCode::new(codigo.as_bytes()) else {
        ui.label("O QR veio ilegível; aguarde o próximo.");
        return;
    };
    let n = qr.width();
    let cores = qr.to_colors();
    let modulo = 4.0_f32;
    let margem = 4.0 * modulo;
    let lado = n as f32 * modulo + 2.0 * margem;
    let (resp, pintor) = ui.allocate_painter(Vec2::splat(lado), egui::Sense::hover());
    pintor.rect_filled(resp.rect, 0.0, Color32::WHITE);
    for (i, c) in cores.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            let min = resp.rect.min + Vec2::new(margem + x * modulo, margem + y * modulo);
            pintor.rect_filled(
                egui::Rect::from_min_size(min, Vec2::splat(modulo)),
                0.0,
                Color32::BLACK,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    #[test]
    fn estado_inicial_nao_inclui_nada_e_esta_desligado() {
        let m = Mensageiros::nova(PathBuf::from("/tmp/cofre-ficticio"));
        assert!(!m.config.ligado);
        assert!(m.config.whatsapp.incluidas.is_empty() && m.config.telegram.incluidas.is_empty());
    }

    #[test]
    fn o_aviso_do_risco_diz_o_que_o_adr_017_diz() {
        for trecho in [
            "nunca envia",
            "termos do WhatsApp",
            "suspensão",
            "Linux",
            "14 dias",
            "30",
        ] {
            assert!(AVISO_DO_WHATSAPP.contains(trecho), "falta: {trecho}");
        }
    }

    #[test]
    fn qr_do_pareamento_tem_matriz() {
        let qr = qrcode::QrCode::new(b"2@abcdef,ghijkl,mnopqr==").unwrap();
        assert!(qr.width() >= 21);
    }
}
