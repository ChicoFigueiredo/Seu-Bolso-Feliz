//! A costura entre o Telegram e o inventário. **Só leitura**: o trait não tem método que
//! envie, apague ou marque como lida (ADR-017).

use std::path::Path;

use crate::Resultado;
use crate::modelo::{Avanco, Conversa, Midia, Pedido};

/// Uma fonte do Telegram: a conta do usuário por MTProto, ou uma falsa de teste.
pub trait FonteTelegram {
    /// Todas as conversas da conta (grupos, canais e conversas privadas).
    ///
    /// # Errors
    /// [`crate::Erro::PrecisaEntrar`] sem sessão válida; falha de rede.
    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>>;

    /// Baixa para `pasta`, da mais antiga para a mais nova, as fotos e os documentos das
    /// mensagens que o `pedido` descreve, e chama `ao_baixar` com cada um **depois** de
    /// gravado. Se `ao_baixar` der erro, a fonte para e devolve esse erro. Devolve até onde
    /// olhou (o checkpoint, se tudo foi tratado).
    ///
    /// # Errors
    /// Falha de rede, de disco, ou o erro de `ao_baixar`.
    fn baixar_midias(
        &mut self,
        pedido: &Pedido,
        pasta: &Path,
        ao_baixar: &mut dyn FnMut(Midia) -> Resultado<()>,
    ) -> Resultado<Avanco>;
}

/// Vale baixar este anexo? Foto e documento (PDF, imagem, planilha, texto, zip) sim; áudio,
/// vídeo e figurinha não; nada acima de [`LIMITE_DE_DOWNLOAD`].
#[must_use]
pub fn vale_baixar(nome: &str, mime: Option<&str>, tamanho: Option<u64>) -> bool {
    if tamanho.is_some_and(|t| t > LIMITE_DE_DOWNLOAD) {
        return false;
    }
    if let Some(m) = mime {
        let m = m.to_ascii_lowercase();
        if m.starts_with("audio/") || m.starts_with("video/") || m == "application/x-tgsticker" {
            return false;
        }
    }
    let ext = nome
        .rsplit('.')
        .next()
        .filter(|e| *e != nome)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    !matches!(
        ext.as_str(),
        "mp3"
            | "ogg"
            | "oga"
            | "opus"
            | "m4a"
            | "wav"
            | "mp4"
            | "mov"
            | "mkv"
            | "avi"
            | "webm"
            | "tgs"
    )
}

/// O maior arquivo que a coleta baixa (100 MB): boleto e comprovante não chegam perto.
pub const LIMITE_DE_DOWNLOAD: u64 = 100 * 1024 * 1024;

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn documento_e_foto_sim_audio_video_figurinha_nao() {
        assert!(vale_baixar(
            "boleto.pdf",
            Some("application/pdf"),
            Some(10_000)
        ));
        assert!(vale_baixar("foto.jpg", Some("image/jpeg"), None));
        assert!(vale_baixar("extrato.csv", None, None));
        assert!(!vale_baixar("voz.ogg", Some("audio/ogg"), Some(5)));
        assert!(!vale_baixar("video.mp4", Some("video/mp4"), None));
        assert!(!vale_baixar(
            "sticker.tgs",
            Some("application/x-tgsticker"),
            None
        ));
        assert!(!vale_baixar("PTT.opus", None, None));
    }

    #[test]
    fn arquivo_enorme_fica_de_fora() {
        assert!(!vale_baixar(
            "imagem.iso",
            None,
            Some(LIMITE_DE_DOWNLOAD + 1)
        ));
        assert!(vale_baixar("imagem.pdf", None, Some(LIMITE_DE_DOWNLOAD)));
    }
}
