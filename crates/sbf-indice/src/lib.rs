//! Índice local de busca por semelhança (ADR-015 item 2, spec 10 frente C2).
//!
//! Embeddings e índice são calculados e guardados **na estação**: o destinatário (o motor de
//! IA) só recebe os trechos recuperados, no nível de confiança dele.
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`embedder`] | o trait [`Embedder`], o [`EmbedderFalso`] determinístico dos testes e, com a feature `modelo-real`, o FastEmbed |
//! | [`pedacos`] | corta texto longo em pedaços de ~800 caracteres com sobreposição |
//! | [`indice`] | `indice.sqlite`: vetores como BLOB `f32`, busca por força bruta (cosseno), atualização incremental por hash |
//! | [`fontes`] | os trechos do inventário (achados, registros) e do cache do livro-caixa (movimentações) |
//! | [`busca`] | o trait [`Busca`] que a triagem e o MCP usam |
//! | [`perguntar`] | a área Perguntar: pergunta, k trechos, só eles vão ao motor, resposta com citações |
//!
//! Nenhum teste baixa modelo: o [`EmbedderFalso`] faz o papel dele.

pub mod busca;
pub mod embedder;
pub mod erro;
pub mod fontes;
pub mod indice;
pub mod pedacos;
pub mod perguntar;

pub use busca::{Busca, BuscaLocal};
pub use embedder::{Embedder, EmbedderFalso};
#[cfg(feature = "modelo-real")]
pub use embedder::{FastEmbedder, modelo_baixado};
pub use erro::{ErroDoIndice, Resultado};
pub use indice::{Indice, ItemRef, Parecido, Relatorio, Trecho};
