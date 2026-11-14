//! A costura entre o mensageiro e o inventário. **Só leitura**: o trait não tem nenhum método
//! que envie, apague ou marque como lida (ADR-017: "nunca envia").

use std::path::Path;

use crate::Resultado;
use crate::modelo::{Checkpoint, Conversa, Mensagem, Plataforma};

/// Uma fonte de mensagens: a ponte do WhatsApp, o export do Telegram ou uma falsa de teste.
pub trait FonteDeMensagens {
    /// De qual mensageiro.
    fn plataforma(&self) -> Plataforma;

    /// Todas as conversas que a fonte conhece (o usuário escolhe quais entram).
    ///
    /// # Errors
    /// A fonte não está pronta ou falhou.
    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>>;

    /// Mensagens da conversa posteriores a `desde`, da mais antiga para a mais nova, até
    /// `limite`. A fonte nunca devolve a mesma mensagem duas vezes para o mesmo checkpoint.
    ///
    /// # Errors
    /// A fonte não está pronta ou falhou.
    fn mensagens_novas(
        &mut self,
        conversa: &str,
        desde: &Checkpoint,
        limite: usize,
    ) -> Resultado<Vec<Mensagem>>;

    /// Grava o anexo da mensagem em `destino` (arquivo novo, que o chamador apaga depois).
    ///
    /// # Errors
    /// A mídia expirou, não existe ou a fonte falhou.
    fn baixar_anexo(&mut self, mensagem: &Mensagem, destino: &Path) -> Resultado<()>;
}
