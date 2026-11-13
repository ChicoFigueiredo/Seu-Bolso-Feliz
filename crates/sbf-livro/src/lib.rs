//! O domínio do livro-caixa do Seu Bolso Feliz, sem rede, sem banco e sem egui.
//!
//! Usado pelo servidor e pela estação: uma regra, um código (ADR-012). Cada regra foi
//! portada de um arquivo TS e tem um teste de ouro que compara a saída do Rust com a
//! saída do TS sobre os mesmos dados (`ouro/` gera as fixtures, `tests/` compara).
//!
//! Dinheiro é [`Centavos`] (`i64`) em todo lugar; nenhum valor de dinheiro passa por
//! ponto flutuante, salvo dentro da amortização, onde a taxa de juros é fracionária
//! (ver [`amortizacao`]).

pub mod ciclo;
pub mod data;
pub mod dinheiro;

pub use ciclo::{Ciclo, DiaDeCorte};
pub use data::Data;
pub use dinheiro::{Centavos, ErroDeValor, Taxa};
