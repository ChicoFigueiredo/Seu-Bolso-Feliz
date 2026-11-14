//! Pontos de atenção (CONTEXT.md, spec 10): algo que a análise aponta como digno de ação,
//! em qualquer área. Traz a evidência (com links para os itens) e o impacto (sempre com a
//! estimativa marcada como estimativa), e fica aberto, resolvido ou ignorado.
//!
//! A análise é **por regras e sem IA**. Lê o inventário, o cache do livro-caixa (só leitura)
//! e `saida_ia`; grava só em `pontos.sqlite`. Dinheiro em centavos `i64`.
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`tipos`] | [`Ponto`], [`Candidato`], [`Impacto`], áreas e estados |
//! | [`armazem`] | `pontos.sqlite`: aplica candidatos sem repetir (PA-02), resolver, ignorar, avisos |
//! | [`regras`] | as regras de dinheiro, operação, custo de IA e tempo do usuário |
//! | [`servico`] | o [`ServicoDePontos`] que a janela usa |
//! | [`analise`] | [`analisar`] roda todas; [`rodar`] analisa e aplica |

pub mod analise;
pub mod armazem;
pub mod erro;
pub mod regras;
pub mod servico;
pub mod tipos;

pub use analise::{Analise, Contexto, Limites, analisar, rodar};
pub use armazem::{Armazem, Filtro, Resumo};
pub use erro::{ErroDosPontos, Resultado};
pub use servico::{PontosLocais, ServicoDePontos};
pub use tipos::{Area, Candidato, Estado, Evidencia, Impacto, Ponto, Unidade};
