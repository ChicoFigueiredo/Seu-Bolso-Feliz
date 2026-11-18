//! Telegram pela conta do usuário (MTProto, `grammers`), **só leitura** (ADR-017).
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`modelo`] | conversa, pedido, mídia |
//! | [`fonte`] | o trait [`fonte::FonteTelegram`] (listar conversas, baixar mídias desde um id) |
//! | [`config`] | `telegram.json`: as conversas seguidas |
//! | [`estado`] | `telegram-estado.json`: checkpoint por conversa e o que a janela mostra |
//! | [`coleta`] | o ciclo: baixa para `entrada/telegram/<conversa>/`, registra, reconhece |
//! | [`sessao`] | a sessão MTProto, serializada e guardada no chaveiro |
//! | [`credenciais`] | `TELEGRAM_API_ID` e `TELEGRAM_API_HASH` do ambiente ou do `.env` |
//! | [`cliente`] | a fonte real, sobre `grammers-client`, e o login |

mod arquivos;
pub mod cliente;
pub mod coleta;
pub mod config;
pub mod credenciais;
pub mod erro;
pub mod estado;
pub mod fonte;
pub mod modelo;
pub mod sessao;

pub use erro::{Erro, Resultado};
pub use fonte::FonteTelegram;
