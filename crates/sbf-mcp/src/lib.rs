//! Servidor MCP local do SBF (ADR-012, modo `sbf mcp`; ADR-015 item 4). O Claude Code ou o
//! Claude Desktop o iniciam como processo filho e falam por stdio (JSON-RPC).
//!
//! - [`ferramentas`]: o que cada ferramenta faz, em funções simples que devolvem JSON. Sem
//!   dependência do protocolo, para testar direto.
//! - [`servidor`]: a casca do `rmcp` (nomes, descrições, esquemas dos parâmetros).
//!
//! Regras: o inventário é lido por conexão só de leitura; a **única** escrita é `sugerir`,
//! que grava em `sugestao_ia` e `rodada_ia` (modelo `mcp`). Nunca grava decisão: quem
//! confirma é o usuário, em Inventário › Triagem por IA. O livro-caixa é só leitura.

pub mod ferramentas;
pub mod plano;
pub mod servidor;

pub use ferramentas::Nucleo;
pub use plano::Fabrica;
pub use servidor::servir_stdio;
