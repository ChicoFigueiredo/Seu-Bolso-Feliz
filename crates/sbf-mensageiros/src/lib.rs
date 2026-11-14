//! Fontes em massa de mensageiros (WhatsApp e Telegram), que entram pelo inventário
//! (ADR-014) e rodam no `sbf worker`. Só leitura (ADR-017): nenhum tipo deste crate envia
//! mensagem.
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`modelo`] | conversa, mensagem, anexo, checkpoint |
//! | [`fonte`] | o trait [`fonte::FonteDeMensagens`] |
//! | [`mapeamento`] | mensagem → anexo/texto → origem `whatsapp:<chat>/<msg>` |
//! | [`gravador`] | achado por sha256 e origem, idempotente |
//! | [`ciclo`] | o que o worker roda: Telegram (export) e WhatsApp (ponte), conversas incluídas |
//! | [`ciclo`] | o que o worker roda: Telegram (export) e WhatsApp (ponte), conversas incluídas |
//! | [`ciclo`] | o que o worker roda: Telegram (export) e WhatsApp (ponte), conversas incluídas |
//! | [`coletor`] | o ciclo: conversas incluídas, checkpoint, cancelamento |
//! | [`config`] | `mensageiros.json` (padrão: nenhuma conversa) e checkpoints |
//! | [`telegram`] | importação do export JSON do Telegram Desktop |
//! | [`whatsapp`] | a ponte (processo externo, JSON por stdio) e a sessão cifrada |

pub mod ciclo;
pub mod coletor;
pub mod config;
pub mod erro;
pub mod fonte;
pub mod gravador;
pub mod mapeamento;
pub mod modelo;
pub mod telegram;
pub mod whatsapp;

pub use erro::{Erro, Resultado};
pub use fonte::FonteDeMensagens;
