//! Opções → IA → "Chaves e contas": guardar, trocar e apagar a chave de cada provedor de IA
//! que usa chave (hoje só o OpenRouter) e mostrar o estado da assinatura do Claude Code.
//!
//! A chave digitada vai direto ao chaveiro e é zerada da memória da tela logo depois. A tela
//! nunca a reexibe: mostra só "guardada · termina em …abcd" (os 4 últimos, guardados de uma
//! vez ao abrir ou guardar, nunca a chave inteira) ou "não guardada". Nada vai a log.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use egui::{RichText, Ui};
use sbf_chaveiro::{Chaveiro, Segredo};
use zeroize::{Zeroize, Zeroizing};

use crate::componentes;
use crate::navegacao::Pedido;
use crate::tema;
use crate::triagem::AcaoDaTriagem;

/// Um provedor que usa chave guardada no chaveiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvedorComChave {
    /// O id no chaveiro (`ia:<id>`).
    pub id: &'static str,
    pub nome: &'static str,
}

/// Os provedores com chave. Só o OpenRouter hoje; não inventar outros.
pub const PROVEDORES_COM_CHAVE: &[ProvedorComChave] = &[ProvedorComChave {
    id: "openrouter",
    nome: "OpenRouter",
}];

/// O aviso depois de guardar ou apagar: o serviço de IA monta o provedor ao abrir o app.
pub const VALE_AO_REABRIR: &str = "Vale ao reabrir o app.";

/// Onde as chaves moram. O chaveiro de verdade ou, na demonstração, uma guarda em memória.
pub trait GuardaDeChaves {
    /// Os últimos 4 caracteres da chave guardada. `None`: não há chave. `Some("")`: há, mas
    /// é curta demais para mostrar o final.
    fn final_guardado(&self, provedor: &str) -> Option<String>;
    /// # Errors
    /// Texto para a tela (nunca o valor).
    fn guardar(&self, provedor: &str, chave: Segredo) -> Result<(), String>;
    /// # Errors
    /// Texto para a tela.
    fn apagar(&self, provedor: &str) -> Result<(), String>;
}

/// O final mostrável de uma chave: 4 caracteres, e só se a chave tiver 12 ou mais (numa chave
/// curta o final seria boa parte dela).
fn final_de(chave: &str) -> String {
    let n = chave.chars().count();
    if n < 12 {
        return String::new();
    }
    chave.chars().skip(n - 4).collect()
}

impl GuardaDeChaves for Chaveiro {
    fn final_guardado(&self, provedor: &str) -> Option<String> {
        if !self.tem_chave_de_ia(provedor) {
            return None;
        }
        Some(
            self.chave_de_ia(provedor)
                .map(|c| final_de(c.expor()))
                .unwrap_or_default(),
        )
    }

    fn guardar(&self, provedor: &str, chave: Segredo) -> Result<(), String> {
        self.guardar_chave_de_ia(provedor, chave)
            .map_err(|e| e.to_string())
    }

    fn apagar(&self, provedor: &str) -> Result<(), String> {
        self.apagar_chave_de_ia(provedor).map_err(|e| e.to_string())
    }
}

/// Uma guarda em memória para a demonstração: nada sai do processo.
#[derive(Debug, Default, Clone)]
pub struct GuardaFicticia {
    chaves: Rc<RefCell<Vec<(String, String)>>>,
}

impl GuardaFicticia {
    /// Já com uma chave fictícia guardada.
    pub fn com_chave(provedor: &str, chave: &str) -> Self {
        let g = Self::default();
        g.chaves
            .borrow_mut()
            .push((provedor.to_owned(), chave.to_owned()));
        g
    }
}

impl GuardaDeChaves for GuardaFicticia {
    fn final_guardado(&self, provedor: &str) -> Option<String> {
        self.chaves
            .borrow()
            .iter()
            .find(|(p, _)| p == provedor)
            .map(|(_, c)| final_de(c))
    }

    fn guardar(&self, provedor: &str, chave: Segredo) -> Result<(), String> {
        let mut v = self.chaves.borrow_mut();
        v.retain(|(p, _)| p != provedor);
        v.push((provedor.to_owned(), chave.expor().to_owned()));
        Ok(())
    }

    fn apagar(&self, provedor: &str) -> Result<(), String> {
        self.chaves.borrow_mut().retain(|(p, _)| p != provedor);
        Ok(())
    }
}

/// O estado de uma chave, para a tela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoDaChave {
    NaoGuardada,
    /// O final (4 caracteres) ou vazio se a chave é curta demais para mostrar.
    Guardada(String),
}

impl EstadoDaChave {
    /// O texto da máscara: nunca a chave.
    pub fn texto(&self) -> String {
        match self {
            Self::NaoGuardada => "não guardada".to_owned(),
            Self::Guardada(f) if f.is_empty() => "guardada".to_owned(),
            Self::Guardada(f) => format!("guardada · termina em …{f}"),
        }
    }

    pub fn guardada(&self) -> bool {
        matches!(self, Self::Guardada(_))
    }
}

/// O que a tela guarda de um provedor.
pub struct LinhaDaChave {
    pub provedor: ProvedorComChave,
    pub estado: EstadoDaChave,
    /// O que está sendo digitado. Zerado depois de guardar e ao sair do app.
    campo: Zeroizing<String>,
    /// Apagar pediu confirmação e espera o sim.
    pub confirmando_apagar: bool,
}

impl std::fmt::Debug for LinhaDaChave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinhaDaChave")
            .field("provedor", &self.provedor.id)
            .field("estado", &self.estado.texto())
            .finish_non_exhaustive()
    }
}

/// O view-model de "Chaves e contas".
#[derive(Default)]
pub struct ChavesDeIa {
    guarda: Option<Box<dyn GuardaDeChaves>>,
    linhas: Vec<LinhaDaChave>,
    /// O `claude` achado nesta máquina (o mesmo que o provedor usa).
    pub claude: Option<PathBuf>,
    /// O último aviso: `Ok` ou `Err`.
    pub aviso: Option<Result<String, String>>,
    /// Rola a página de Opções até esta seção nos próximos N quadros (o conteúdo cresce no quadro seguinte) (as capturas de tela usam).
    pub rolar_ate_aqui: u8,
}

impl std::fmt::Debug for ChavesDeIa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChavesDeIa")
            .field("ligada", &self.guarda.is_some())
            .field("linhas", &self.linhas)
            .finish_non_exhaustive()
    }
}

impl ChavesDeIa {
    pub fn nova(guarda: Option<Box<dyn GuardaDeChaves>>, claude: Option<PathBuf>) -> Self {
        let mut s = Self {
            guarda,
            linhas: Vec::new(),
            claude,
            aviso: None,
            rolar_ate_aqui: 0,
        };
        s.reler();
        s
    }

    /// Lê de novo o estado de cada provedor (só o final, nunca a chave).
    pub fn reler(&mut self) {
        let guarda = self.guarda.as_deref();
        self.linhas = PROVEDORES_COM_CHAVE
            .iter()
            .map(|p| {
                let estado = match guarda.and_then(|g| g.final_guardado(p.id)) {
                    Some(f) => EstadoDaChave::Guardada(f),
                    None => EstadoDaChave::NaoGuardada,
                };
                LinhaDaChave {
                    provedor: *p,
                    estado,
                    campo: Zeroizing::new(String::with_capacity(256)),
                    confirmando_apagar: false,
                }
            })
            .collect();
    }

    pub fn linhas(&self) -> &[LinhaDaChave] {
        &self.linhas
    }

    pub fn tem_guarda(&self) -> bool {
        self.guarda.is_some()
    }

    fn linha_mut(&mut self, id: &str) -> Option<&mut LinhaDaChave> {
        self.linhas.iter_mut().find(|l| l.provedor.id == id)
    }

    /// Digita no campo (para os testes; a tela edita o buffer direto).
    pub fn digitar(&mut self, id: &str, texto: &str) {
        if let Some(l) = self.linha_mut(id) {
            l.campo.push_str(texto);
        }
    }

    /// O campo tem algo que valha guardar.
    pub fn pode_guardar(&self, id: &str) -> bool {
        self.guarda.is_some()
            && self
                .linhas
                .iter()
                .find(|l| l.provedor.id == id)
                .is_some_and(|l| !l.campo.trim().is_empty())
    }

    /// Guarda (ou troca) a chave digitada e zera o campo, com sucesso ou não.
    pub fn guardar(&mut self, id: &str) {
        let Some(guarda) = self.guarda.as_deref() else {
            return;
        };
        let Some(l) = self.linhas.iter_mut().find(|l| l.provedor.id == id) else {
            return;
        };
        let digitado = l.campo.trim();
        if digitado.is_empty() {
            return;
        }
        let nome = l.provedor.nome;
        let resultado = guarda.guardar(id, Segredo::novo(digitado));
        l.campo.zeroize();
        l.confirmando_apagar = false;
        match resultado {
            Ok(()) => {
                l.estado = match guarda.final_guardado(id) {
                    Some(f) => EstadoDaChave::Guardada(f),
                    None => EstadoDaChave::NaoGuardada,
                };
                self.aviso = Some(Ok(format!("Chave do {nome} guardada. {VALE_AO_REABRIR}")));
            }
            Err(e) => {
                self.aviso = Some(Err(format!(
                    "Não foi possível guardar a chave do {nome}: {e}"
                )));
            }
        }
    }

    /// O primeiro clique em Apagar: pede confirmação.
    pub fn pedir_para_apagar(&mut self, id: &str) {
        if let Some(l) = self.linha_mut(id).filter(|l| l.estado.guardada()) {
            l.confirmando_apagar = true;
        }
    }

    pub fn cancelar_apagar(&mut self, id: &str) {
        if let Some(l) = self.linha_mut(id) {
            l.confirmando_apagar = false;
        }
    }

    /// Apaga de verdade; só vale depois de [`Self::pedir_para_apagar`].
    pub fn confirmar_apagar(&mut self, id: &str) {
        let Some(guarda) = self.guarda.as_deref() else {
            return;
        };
        let Some(l) = self
            .linhas
            .iter_mut()
            .find(|l| l.provedor.id == id && l.confirmando_apagar)
        else {
            return;
        };
        l.confirmando_apagar = false;
        l.campo.zeroize();
        let nome = l.provedor.nome;
        match guarda.apagar(id) {
            Ok(()) => {
                l.estado = EstadoDaChave::NaoGuardada;
                self.aviso = Some(Ok(format!("Chave do {nome} apagada. {VALE_AO_REABRIR}")));
            }
            Err(e) => {
                self.aviso = Some(Err(format!(
                    "Não foi possível apagar a chave do {nome}: {e}"
                )));
            }
        }
    }

    /// A linha de estado do `claude`.
    pub fn texto_do_claude(&self) -> String {
        match &self.claude {
            Some(c) => format!("Claude Code encontrado: {}", c.display()),
            None => "Claude Code (`claude`) não encontrado nesta máquina".to_owned(),
        }
    }
}

/// "Chaves e contas": uma linha por provedor com chave e o cartão da assinatura do Claude.
pub fn secao(
    ui: &mut Ui,
    chaves: &mut ChavesDeIa,
    testando: bool,
    teste: Option<&Result<String, String>>,
    pedidos: &mut Vec<Pedido>,
) {
    let cores = tema::paleta(ui.ctx());
    componentes::rotulo_de_secao(ui, "Chaves e contas");
    if !chaves.tem_guarda() {
        ui.label(
            RichText::new("O chaveiro não está aberto: não dá para guardar chaves agora.")
                .color(cores.tinta_suave),
        );
    }
    let mut guardar = None;
    let mut pedir = None;
    let mut confirmar = None;
    let mut cancelar = None;
    for l in &mut chaves.linhas {
        let id = l.provedor.id;
        ui.horizontal(|ui| {
            ui.label(RichText::new(l.provedor.nome).strong());
            let cor = if l.estado.guardada() {
                cores.sucesso
            } else {
                cores.tinta_suave
            };
            ui.label(RichText::new(l.estado.texto()).color(cor));
        });
        ui.horizontal(|ui| {
            ui.add_enabled(
                chaves.guarda.is_some(),
                egui::TextEdit::singleline(&mut *l.campo)
                    .password(true)
                    .hint_text("cole a chave aqui")
                    .desired_width(280.0),
            );
            let rotulo = if l.estado.guardada() {
                "Trocar"
            } else {
                "Guardar"
            };
            let pode = chaves.guarda.is_some() && !l.campo.trim().is_empty();
            if ui.add_enabled(pode, egui::Button::new(rotulo)).clicked() {
                guardar = Some(id);
            }
            if l.estado.guardada() && !l.confirmando_apagar && ui.button("Apagar").clicked() {
                pedir = Some(id);
            }
        });
        if l.confirmando_apagar {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Apagar a chave do {} do chaveiro?",
                        l.provedor.nome
                    ))
                    .color(cores.perigo),
                );
                if ui.button("Apagar mesmo").clicked() {
                    confirmar = Some(id);
                }
                if ui.button("Cancelar").clicked() {
                    cancelar = Some(id);
                }
            });
        }
        ui.add_space(4.0);
    }
    if let Some(id) = guardar {
        chaves.guardar(id);
    }
    if let Some(id) = pedir {
        chaves.pedir_para_apagar(id);
    }
    if let Some(id) = confirmar {
        chaves.confirmar_apagar(id);
    }
    if let Some(id) = cancelar {
        chaves.cancelar_apagar(id);
    }
    match &chaves.aviso {
        Some(Ok(t)) => {
            ui.label(RichText::new(t).color(cores.sucesso));
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e).color(cores.perigo));
        }
        None => {}
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Assinatura Claude").strong());
    let cor = if chaves.claude.is_some() {
        cores.sucesso
    } else {
        cores.perigo
    };
    ui.label(RichText::new(chaves.texto_do_claude()).color(cor));
    ui.label(
        RichText::new("Não usa chave: usa o login do Claude Code desta máquina. Para trocar de conta, rode `claude /login` no terminal.")
            .size(12.0)
            .color(cores.tinta_suave),
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!testando, egui::Button::new("Testar"))
            .clicked()
        {
            pedidos.push(Pedido::Triagem(AcaoDaTriagem::TestarConexao));
        }
        if testando {
            ui.spinner();
            ui.label(RichText::new("Testando…").color(cores.tinta_suave));
        }
    });
    match teste {
        Some(Ok(t)) => {
            ui.label(RichText::new(t).color(cores.sucesso));
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e).color(cores.perigo));
        }
        None => {}
    }
    if chaves.rolar_ate_aqui > 0 {
        chaves.rolar_ate_aqui -= 1;
        ui.scroll_with_delta(egui::vec2(0.0, -10_000.0));
    }
}

#[cfg(test)]
mod testes {
    use std::sync::Arc;

    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;

    use super::*;

    const CHAVE: &str = "sk-or-v1-ficticia-0000abcd";

    fn guarda_vazia() -> ChavesDeIa {
        ChavesDeIa::nova(Some(Box::new(GuardaFicticia::default())), None)
    }

    #[test]
    fn a_mascara_mostra_so_o_final() {
        assert_eq!(final_de(CHAVE), "abcd");
        assert_eq!(final_de("curta"), "");
        assert_eq!(
            EstadoDaChave::Guardada("abcd".into()).texto(),
            "guardada · termina em …abcd"
        );
        assert_eq!(EstadoDaChave::Guardada(String::new()).texto(), "guardada");
        assert_eq!(EstadoDaChave::NaoGuardada.texto(), "não guardada");
        assert!(
            !EstadoDaChave::Guardada("abcd".into())
                .texto()
                .contains("sk-or")
        );
    }

    #[test]
    fn guardar_zera_o_campo_e_nunca_expoe_a_chave() {
        let mut c = guarda_vazia();
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::NaoGuardada);
        assert!(!c.pode_guardar("openrouter"));
        c.digitar("openrouter", &format!("  {CHAVE}  "));
        assert!(c.pode_guardar("openrouter"));
        c.guardar("openrouter");
        assert!(c.linhas()[0].campo.is_empty());
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::Guardada("abcd".into()));
        let aviso = c.aviso.clone().unwrap().unwrap();
        assert!(aviso.contains(VALE_AO_REABRIR));
        assert!(!aviso.contains(CHAVE));
        assert!(!format!("{c:?}").contains(CHAVE));
    }

    #[test]
    fn trocar_substitui_e_so_espacos_nao_guarda() {
        let mut c = ChavesDeIa::nova(
            Some(Box::new(GuardaFicticia::com_chave("openrouter", CHAVE))),
            None,
        );
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::Guardada("abcd".into()));
        c.digitar("openrouter", "   ");
        assert!(!c.pode_guardar("openrouter"));
        c.guardar("openrouter");
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::Guardada("abcd".into()));
        assert!(c.aviso.is_none());
        c.linha_mut("openrouter").unwrap().campo.clear();
        c.digitar("openrouter", "sk-or-v1-outra-chave-wxyz");
        c.guardar("openrouter");
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::Guardada("wxyz".into()));
    }

    #[test]
    fn apagar_pede_confirmacao_e_cancelar_nao_apaga() {
        let mut c = ChavesDeIa::nova(
            Some(Box::new(GuardaFicticia::com_chave("openrouter", CHAVE))),
            None,
        );
        // Sem pedir, confirmar não faz nada.
        c.confirmar_apagar("openrouter");
        assert!(c.linhas()[0].estado.guardada());
        c.pedir_para_apagar("openrouter");
        assert!(c.linhas()[0].confirmando_apagar);
        c.cancelar_apagar("openrouter");
        c.confirmar_apagar("openrouter");
        assert!(c.linhas()[0].estado.guardada());
        c.pedir_para_apagar("openrouter");
        c.confirmar_apagar("openrouter");
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::NaoGuardada);
        assert!(c.aviso.clone().unwrap().unwrap().contains("apagada"));
        // Sem chave, não há o que apagar.
        c.pedir_para_apagar("openrouter");
        assert!(!c.linhas()[0].confirmando_apagar);
    }

    #[test]
    fn sem_chaveiro_nada_guarda() {
        let mut c = ChavesDeIa::nova(None, None);
        c.digitar("openrouter", CHAVE);
        assert!(!c.pode_guardar("openrouter"));
        c.guardar("openrouter");
        assert!(c.aviso.is_none());
    }

    #[test]
    fn o_claude_aparece_com_o_caminho_ou_como_ausente() {
        let mut c = guarda_vazia();
        assert!(c.texto_do_claude().contains("não encontrado"));
        c.claude = Some(PathBuf::from("/casa/fulano/.local/bin/claude"));
        assert_eq!(
            c.texto_do_claude(),
            "Claude Code encontrado: /casa/fulano/.local/bin/claude"
        );
    }

    #[test]
    fn o_chaveiro_de_verdade_guarda_troca_e_apaga() {
        let pasta = tempfile::tempdir().unwrap();
        let chaveiro = Chaveiro::abrir_com_chaveiro(
            pasta.path(),
            Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap();
        let mut c = ChavesDeIa::nova(Some(Box::new(chaveiro.clone())), None);
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::NaoGuardada);
        c.digitar("openrouter", CHAVE);
        c.guardar("openrouter");
        assert!(chaveiro.tem_chave_de_ia("openrouter"));
        assert_eq!(chaveiro.chave_de_ia("openrouter").unwrap().expor(), CHAVE);
        assert_eq!(c.linhas()[0].estado, EstadoDaChave::Guardada("abcd".into()));
        // Reabrir a tela relê do chaveiro.
        let c2 = ChavesDeIa::nova(Some(Box::new(chaveiro.clone())), None);
        assert_eq!(
            c2.linhas()[0].estado,
            EstadoDaChave::Guardada("abcd".into())
        );
        c.pedir_para_apagar("openrouter");
        c.confirmar_apagar("openrouter");
        assert!(!chaveiro.tem_chave_de_ia("openrouter"));
    }
}
