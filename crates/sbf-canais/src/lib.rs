//! O modo `sbf canais` (ADR-014, ADR-015, ADR-017): o bot do Telegram e os alertas, que
//! rodam 24 h na VPS junto do `sbf servidor` e do `sbf coletor-vps`.
//!
//! - [`bot`]: long polling (`getUpdates`), só de chats autorizados; documento ou foto vira
//!   `source_documents` + `ingestion_jobs` pelo caminho do servidor, direto na caixa de entrada;
//! - [`alertas`]: as regras (conta a vencer e vencida, cobrança duplicada, gasto fora do
//!   padrão) e o ponto de extensão [`alertas::FonteDeAlertas`];
//! - [`agenda`]: horários configuráveis, silêncio noturno, resumo diário;
//! - [`confianca`] e [`texto`]: o nível de confiança do destinatário (ADR-015), mascarado
//!   por padrão; [`repo`] registra no Postgres o que saiu (nunca o texto).
//!
//! É um crate à parte do `sbf-coletor` de propósito: o coletor puxa dados de fora em
//! intervalos; o canal só avisa e recebe, tem outro segredo (o token do bot), um laço
//! contínuo de long polling e deve reiniciar sem atrasar as coletas.

pub mod agenda;
pub mod alertas;
pub mod bot;
pub mod cli;
pub mod confianca;
pub mod config;
pub mod envio;
pub mod erro;
pub mod pontos;
pub mod repo;
pub mod telegram;
pub mod texto;

pub use erro::ErroDeCanal;
