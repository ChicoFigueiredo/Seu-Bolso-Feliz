//! O modo `sbf coletor-vps` (ADR-014, ADR-010, ADR-012): as fontes já filtradas, que
//! entregam direto na caixa de entrada e rodam na VPS 24 h. Porte dos workers Bun
//! `gmail-scanner` e `pluggy-sync`.
//!
//! - [`gmail`]: varredura por rótulo/consulta, OAuth2 com refresh token, anexos para
//!   `source_documents` + `ingestion_jobs` (o mesmo formato do Bun);
//! - [`pluggy`]: transações da Pluggy para `draft_records` (`external_ref = pluggy:txn_<id>`),
//!   incremental ou `--backfill` retomável de 365 dias;
//! - [`comparador`]: leitura só de contagens, para a semana em paralelo com o Bun;
//! - [`agenda`]: o laço que roda as duas coletas em intervalos.
//!
//! Segredos (refresh token, client secret, tokens) chegam só por variável de ambiente, vivem
//! em [`config::Segredo`] e nunca entram em log, `Debug` ou mensagem de erro.

pub mod agenda;
pub mod checkpoint;
pub mod cli;
pub mod comparador;
pub mod config;
pub mod dono;
pub mod erro;
pub mod gmail;
pub mod pluggy;

pub use erro::ErroDeColeta;
