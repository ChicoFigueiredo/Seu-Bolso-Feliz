//! Servidor MCP local do SBF (ADR-012, modo `sbf mcp`; ADR-015 item 4). O Claude Code ou o
//! Claude Desktop o iniciam como processo filho e falam por stdio (JSON-RPC).
//!
//! - [`ferramentas`]: o que cada ferramenta faz, em funções simples que devolvem JSON. Sem
//!   dependência do protocolo, para testar direto.
//! - [`servidor`]: a casca do `rmcp` (nomes, descrições, esquemas dos parâmetros).
//!
//! Regras (D15, 2026-11-18): o servidor tem **controle total**. As ferramentas de planejamento
//! e de regras **aplicam** direto (autor `mcp` no histórico, com Desfazer), e as `propor_*`
//! continuam só propondo. Antes da primeira escrita de cada banco na sessão há backup (módulo
//! [`backup`]); se ele falha, a escrita é recusada. A triagem do inventário segue só do
//! usuário: `sugerir` apenas grava sugestões (modelo `mcp`). O livro-caixa é só leitura.
//!
//! **Roteiros escritos** (spec 16, RE-19; módulo [`roteiros_escritos`]): o MCP só **propõe**
//! (rascunho com autor `mcp`) e consulta. Ao contrário das regras, não há ferramenta que
//! ative, desative, teste ou execute: um roteiro escrito comanda um navegador com credenciais.

pub mod backup;
pub mod ferramentas;
pub mod plano;
pub mod regras;
pub mod roteiros_escritos;
pub mod servidor;
pub mod telegram;

pub use ferramentas::{ColetorDoGmail, Nucleo, ParamsDeColetaDoGmail};
pub use plano::Fabrica;
pub use servidor::servir_stdio;
