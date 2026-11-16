//! Importação da planilha do controle mensal (spec 11).
//!
//! - [`leitura`]: lê `.xlsm`/`.xlsx` só pelos valores em cache, nunca executa macro.
//! - [`importacao`]: prévia (sem gravar) e confirmação, tudo por `Planejamento::aplicar`.
//! - [`ocultos`]: recorrentes achados no inventário (Pluggy) que a planilha não traz.
//! - [`conciliacao`]: sugestões de conciliação com as transações do inventário.
//! - [`saldos`]: o último saldo de cada conta, do bruto da Pluggy guardado no cofre.
//!
//! Regras: dinheiro só em centavos; erro de fórmula nunca vira zero; nenhum valor, nome
//! ou descrição vai a log (só contagens).

pub mod conciliacao;
pub mod importacao;
pub mod leitura;
pub mod mapa;
pub mod ocultos;
pub mod saldos;
pub mod texto;

/// Implementação em memória do trait, só para testes (deste crate e do CLI).
#[doc(hidden)]
pub mod falso;
#[cfg(test)]
mod fixtura;

use sbf_planejamento::ErroDoPlanejamento;

#[derive(Debug, thiserror::Error)]
pub enum ErroPlanilha {
    #[error("não deu para abrir a planilha: {0}")]
    Abrir(String),
    #[error("a aba {0} não existe na planilha")]
    AbaInexistente(String),
    #[error("mapa inválido: {0}")]
    Mapa(String),
    #[error("inventário: {0}")]
    Inventario(String),
    #[error("saldos: {0}")]
    Saldos(String),
    #[error(transparent)]
    Planejamento(#[from] ErroDoPlanejamento),
}

pub type Resultado<T> = Result<T, ErroPlanilha>;
