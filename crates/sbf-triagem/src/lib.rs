//! Triagem do inventário por IA (spec 09). A IA **sugere**; o usuário confirma em lote, e
//! cada sugestão aceita vira uma decisão sua no log. Uma sugestão nunca é decisão e nunca
//! passa por cima de decisão do usuário.
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`fila`] | seleciona o que vai à IA e resolve o que as regras gratuitas já sabem (IA-01, IA-02) |
//! | [`entrada`] | monta o texto de cada lote no nível Completo e trunca texto longo |
//! | [`prompt`] | o prompt versionado e o JSON Schema da resposta |
//! | [`resposta`] | valida a resposta do modelo, item a item |
//! | [`sugestoes`] | grava e lista `sugestao_ia`, `rodada_ia` |
//! | [`formato`] | o `Formato` da fila do motor: entrada do lote e confiança por item |
//! | [`rodada`] | roda a fila do motor sobre a fila da triagem e grava as sugestões (IA-03) |
//! | [`aceite`] | aceitar e recusar em lote, uma decisão por item (IA-07, IA-08) |
//! | [`piloto`] | o acerto contra as decisões já tomadas (IA-09) |
//! | [`agente`] | o agente contínuo: regras aprendidas, exemplos no prompt, calibração, painel |
//! | [`servico`] | o trait pequeno que a janela usa (iniciar, pausar, retomar, progresso) |
//!
//! O motor (provedores, cache, ritmo, registro do que saiu) é do `sbf-ia`: esta crate só
//! recebe um `&dyn Provedor`, que o coordenador liga ao motor real. Nada aqui chama rede.

pub mod aceite;
pub mod agente;
pub mod entrada;
pub mod erro;
pub mod fila;
pub mod formato;
pub mod normalizar;
pub mod piloto;
pub mod prompt;
pub mod resposta;
pub mod rodada;
pub mod servico;
pub mod sugestoes;
pub mod tipos;

pub use erro::ErroDaTriagem;
pub use tipos::{Alvo, Item, ItemDeAchado, ItemDeRegistro};
