//! WhatsApp como dispositivo vinculado, **só leitura** (ADR-017).
//!
//! O protocolo do WhatsApp roda **fora** deste processo, numa ponte: um executável à parte
//! que fala JSON, uma linha por mensagem, por stdin e stdout ([`protocolo`]). A ponte de
//! referência é o plano B do ADR-017, o `whatsmeow` (Go, MPL-2.0); o `whatsapp-rust` 0.7 (MIT)
//! compila aqui, mas só com um backend de armazenamento próprio (o padrão dele usa
//! `libsqlite3-sys` 0.37 e briga com o `rusqlite` do SBF) e não dá para validá-lo sem parear
//! uma conta real; ver `docs/desktop/mensageiros.md`.
//!
//! - [`protocolo`]: os comandos (sem nenhum que envie) e os eventos.
//! - [`ponte`]: a [`ponte::PonteDoWhatsapp`] (a [`crate::FonteDeMensagens`]) e o pareamento.
//! - [`sessao`]: a sessão guardada cifrada; a chave fica no chaveiro.

pub mod ponte;
pub mod protocolo;
pub mod sessao;
