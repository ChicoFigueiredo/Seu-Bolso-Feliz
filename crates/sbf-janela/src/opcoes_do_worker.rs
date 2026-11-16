//! Opções › Worker: liga e desliga o agente contínuo de triagem, define o horário e lista,
//! adiciona e remove as pastas vigiadas, tudo em `worker.json` (crate `sbf-worker`). A
//! escrita é atômica. O worker lê o arquivo a cada volta: o que muda aqui vale na próxima
//! volta dele.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use sbf_worker::config::Config;

use crate::{componentes, tema};

/// O aviso mostrado depois de cada gravação.
pub const VALE_NA_PROXIMA_VOLTA: &str = "Vale na próxima volta do worker.";

/// O estado da seção Worker de Opções.
#[derive(Debug)]
pub struct OpcoesDoWorker {
    arquivo: Option<PathBuf>,
    cofre: PathBuf,
    /// A configuração em edição.
    pub config: Config,
    /// O texto da pasta a adicionar.
    pub pasta_nova: String,
    /// Último resultado: confirmação ou o motivo da recusa.
    pub aviso: Option<String>,
}

impl OpcoesDoWorker {
    /// `arquivo`: o `worker.json` (`None` sem pasta de dados do app); `cofre`: nunca vigiado.
    #[must_use]
    pub fn nova(arquivo: Option<PathBuf>, cofre: PathBuf) -> Self {
        Self {
            arquivo,
            cofre,
            config: Config::default(),
            pasta_nova: String::new(),
            aviso: None,
        }
    }

    /// Lê o `worker.json` (ausente: padrão).
    pub fn carregar(&mut self) {
        let Some(a) = &self.arquivo else { return };
        match Config::carregar(a) {
            Ok(c) => self.config = c,
            Err(e) => self.aviso = Some(format!("Não deu para ler o worker.json: {e}")),
        }
    }

    fn gravar(&mut self) {
        let Some(a) = &self.arquivo else {
            self.aviso = Some("Sem pasta de dados do app para guardar o worker.json.".to_owned());
            return;
        };
        self.aviso = Some(match self.config.salvar(a) {
            Ok(()) => VALE_NA_PROXIMA_VOLTA.to_owned(),
            Err(e) => format!("Não deu para guardar o worker.json: {e}"),
        });
    }

    /// Liga ou desliga o agente contínuo de triagem.
    pub fn definir_triagem(&mut self, ligada: bool) {
        self.config.triagem.ligada = ligada;
        self.gravar();
    }

    /// O horário de trabalho do agente (horas locais, 0 a 24).
    pub fn definir_horario(&mut self, inicial: u32, final_: u32) {
        self.config.triagem.hora_inicial = inicial.min(23);
        self.config.triagem.hora_final = final_.min(24);
        self.gravar();
    }

    /// Valida e acrescenta a pasta vigiada. A recusa vira o aviso e nada é gravado.
    pub fn adicionar_pasta(&mut self, texto: &str) {
        let raiz = Path::new(texto.trim());
        match self.config.adicionar_pasta(raiz, &self.cofre) {
            Ok(()) => {
                self.pasta_nova.clear();
                self.gravar();
            }
            Err(e) => self.aviso = Some(e.to_string()),
        }
    }

    /// Tira a pasta de `indice` da lista.
    pub fn remover_pasta(&mut self, indice: usize) {
        if indice < self.config.pastas.len() {
            self.config.pastas.remove(indice);
            self.gravar();
        }
    }

    /// Desenha a seção.
    pub fn ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        componentes::rotulo_de_secao(ui, "Worker (agente contínuo e pastas vigiadas)");
        let mut ligada = self.config.triagem.ligada;
        if ui
            .checkbox(&mut ligada, "Triagem contínua por IA (gasta a assinatura)")
            .changed()
        {
            self.definir_triagem(ligada);
        }
        let (mut i, mut f) = (
            self.config.triagem.hora_inicial,
            self.config.triagem.hora_final,
        );
        ui.horizontal(|ui| {
            ui.label("Trabalhar das");
            let a = ui.add(egui::DragValue::new(&mut i).range(0..=23).suffix(" h"));
            ui.label("às");
            let b = ui.add(egui::DragValue::new(&mut f).range(0..=24).suffix(" h"));
            if a.changed() || b.changed() {
                self.definir_horario(i, f);
            }
        });
        ui.label(RichText::new("Pastas vigiadas").strong());
        let mut remover = None;
        for (n, pasta) in self.config.pastas.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(pasta.raiz.to_string_lossy());
                if ui.small_button("Remover").clicked() {
                    remover = Some(n);
                }
            });
        }
        if self.config.pastas.is_empty() {
            ui.label(RichText::new("Nenhuma pasta vigiada.").color(p.tinta_suave));
        }
        if let Some(n) = remover {
            self.remover_pasta(n);
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.pasta_nova).hint_text("/caminho/da/pasta"));
            if ui.button("Adicionar pasta").clicked() {
                let texto = self.pasta_nova.clone();
                self.adicionar_pasta(&texto);
            }
        });
        if let Some(a) = &self.aviso {
            ui.label(RichText::new(a).color(p.tinta_suave));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn nova() -> (tempfile::TempDir, OpcoesDoWorker) {
        let d = tempfile::tempdir().unwrap();
        let cofre = d.path().join("cofre");
        std::fs::create_dir_all(&cofre).unwrap();
        let o = OpcoesDoWorker::nova(Some(d.path().join("worker.json")), cofre);
        (d, o)
    }

    #[test]
    fn liga_triagem_define_horario_e_grava_com_aviso() {
        let (d, mut o) = nova();
        o.definir_triagem(true);
        o.definir_horario(22, 6);
        assert_eq!(o.aviso.as_deref(), Some(VALE_NA_PROXIMA_VOLTA));
        let lida = Config::carregar(&d.path().join("worker.json")).unwrap();
        assert!(lida.triagem.ligada);
        assert_eq!(
            (lida.triagem.hora_inicial, lida.triagem.hora_final),
            (22, 6)
        );
    }

    #[test]
    fn adiciona_e_remove_pasta_e_recusa_cofre_sem_gravar() {
        let (d, mut o) = nova();
        let boa = d.path().join("Downloads");
        o.adicionar_pasta(&boa.to_string_lossy());
        assert_eq!(o.config.pastas.len(), 1);
        let cofre = d.path().join("cofre/acervo");
        o.adicionar_pasta(&cofre.to_string_lossy());
        assert_eq!(o.config.pastas.len(), 1);
        assert!(o.aviso.as_deref().unwrap().contains("cofre"));
        let lida = Config::carregar(&d.path().join("worker.json")).unwrap();
        assert_eq!(lida.pastas.len(), 1);
        o.remover_pasta(0);
        let lida = Config::carregar(&d.path().join("worker.json")).unwrap();
        assert!(lida.pastas.is_empty());
    }
}
