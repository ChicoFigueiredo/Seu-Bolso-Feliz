//! De onde vêm os segredos que montam a senha candidata. O `sbf-chaveiro` implementa
//! [`FonteDeSegredos`] sobre o chaveiro; os testes, sobre valores fictícios.

use zeroize::Zeroizing;

/// Por que um segredo não veio. Nenhuma variante carrega valor nem rótulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum ErroDeFonte {
    /// Não há CPF no chaveiro.
    #[error("não há CPF no chaveiro")]
    SemCpf,
    /// O CPF guardado não tem 11 dígitos.
    #[error("o CPF guardado no chaveiro não tem 11 dígitos")]
    CpfInvalido,
    /// Não há senha própria com esse rótulo.
    #[error("não há senha própria com esse rótulo no chaveiro")]
    SemSenhaPropria,
    /// O chaveiro (arquivo de fallback) está trancado.
    #[error("o chaveiro está trancado: destranque com a senha-mestra")]
    Trancado,
    /// A estratégia pede um número de dígitos fora de 1 a 11 (montada sem o `FromStr`).
    #[error("a estratégia pede um número de dígitos do CPF fora de 1 a 11")]
    EstrategiaInvalida,
    /// Outra falha do chaveiro.
    #[error("o chaveiro não respondeu")]
    Indisponivel,
}

/// Quem entrega o CPF e as senhas próprias, em memória zerável.
pub trait FonteDeSegredos {
    /// O CPF como foi guardado (o que não é dígito é ignorado ao montar a candidata).
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte>;
    /// A senha própria do rótulo (a estratégia `senha_propria:<rotulo>`).
    fn senha_propria(&self, rotulo: &str) -> Result<Zeroizing<String>, ErroDeFonte>;
}
