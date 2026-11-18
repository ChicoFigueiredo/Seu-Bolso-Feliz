//! O que a coleta pede ao reconhecimento, atrás de um trait (os testes da coleta usam um
//! de mentira; o real é o [`ReconhecedorDoCofre`] com as regras ligadas).

use std::path::Path;
use std::sync::atomic::AtomicBool;

use sbf_reconhecimento::regras::MensagemRecebida;
use sbf_reconhecimento::{ReconhecedorDoCofre, Reconhecido};
use sbf_regras::Canal;

/// O que o reconhecimento concluiu (só categorias; nada do conteúdo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Uma regra (ou um leitor) reconheceu; o nome dela.
    Reconhecido(String),
    /// Duas regras empatadas discordam: vai para "Precisa revisar".
    Ambiguo,
    /// Nada reconheceu.
    Nada,
}

/// O reconhecimento de um documento novo e de uma mensagem nova.
pub trait Reconhecimento {
    /// Um arquivo que já está no cofre e no inventário, vindo do Gmail (`id_externo` é o id
    /// da mensagem).
    ///
    /// # Errors
    /// Um motivo genérico, sem conteúdo.
    fn arquivo(&self, caminho: &Path, id_externo: &str) -> Result<Desfecho, String>;

    /// O corpo de um e-mail sem anexo.
    ///
    /// # Errors
    /// Um motivo genérico, sem conteúdo.
    fn mensagem(&self, mensagem: &MensagemRecebida<'_>) -> Result<Desfecho, String>;
}

/// O reconhecimento real, sobre o cofre.
#[derive(Debug)]
pub struct ReconhecimentoDoCofre {
    reconhecedor: ReconhecedorDoCofre,
}

impl ReconhecimentoDoCofre {
    /// `reconhecedor` já vem com as regras ligadas (`com_regras`), como o `sbf reconhecer`.
    #[must_use]
    pub fn novo(reconhecedor: ReconhecedorDoCofre) -> Self {
        Self { reconhecedor }
    }
}

fn desfecho(r: &Reconhecido) -> Desfecho {
    match r {
        Reconhecido::PelaRegra { reconhecimento } => {
            Desfecho::Reconhecido(reconhecimento.regra.clone())
        }
        Reconhecido::Contracheque { .. } => Desfecho::Reconhecido("contracheque".to_owned()),
        Reconhecido::PelaIa { .. } => Desfecho::Reconhecido("ia".to_owned()),
        Reconhecido::Ambiguo { .. } => Desfecho::Ambiguo,
        Reconhecido::Nada { .. } => Desfecho::Nada,
    }
}

impl Reconhecimento for ReconhecimentoDoCofre {
    fn arquivo(&self, caminho: &Path, id_externo: &str) -> Result<Desfecho, String> {
        let r = self
            .reconhecedor
            .reconhecer_de(
                Some((Canal::Gmail, id_externo.to_owned())),
                caminho,
                &mut |_| {},
                &AtomicBool::new(false),
            )
            .map_err(|e| e.to_string())?;
        Ok(desfecho(&r.reconhecido))
    }

    fn mensagem(&self, mensagem: &MensagemRecebida<'_>) -> Result<Desfecho, String> {
        let r = self
            .reconhecedor
            .reconhecer_mensagem(mensagem)
            .map_err(|e| e.to_string())?;
        Ok(desfecho(&r))
    }
}
