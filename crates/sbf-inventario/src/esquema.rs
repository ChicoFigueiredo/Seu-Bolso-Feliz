//! O SQL das migrações, congelado. Dono: integrador.
//!
//! - **Migração 1** é o esquema do Bun na versão "2" (`workers/inventario/src/schema.sql`
//!   em 2026-11-13), copiado e congelado: o Bun pode evoluir, a migração 1 não.
//! - **Migração 2** é o log de decisões, os lotes, `decidido_por` e os índices
//!   (spec 06, "O banco"). O agente do núcleo pode completar a 2 enquanto ela não foi
//!   aplicada em nenhum banco real.

/// Migração 1: o esquema do Bun na versão "2".
pub const MIGRACAO_1_BASE_BUN: &str = include_str!("../sql/0001-base-bun-v2.sql");

/// Migração 2: decisões, lotes, `decidido_por` e índices. O trigger só de acréscimo
/// em `decisao` é instalado em Rust, via `bancada_armazem`.
pub const MIGRACAO_2_DECISOES_E_LOTES: &str = include_str!("../sql/0002-decisoes-e-lotes.sql");

/// Migração 3: a triagem por IA (spec 09): sugestões, rodadas, o que saiu e o cache.
pub const MIGRACAO_3_TRIAGEM: &str = include_str!("../sql/0003-triagem.sql");

/// Migração 4: a Caixa de entrada, a fila de reconhecimento dos documentos novos.
pub const MIGRACAO_4_CAIXA_DE_ENTRADA: &str = include_str!("../sql/0004-caixa-de-entrada.sql");
