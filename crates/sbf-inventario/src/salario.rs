//! O registro do tipo salário que um contracheque declara (spec 12, D1; CONTEXT.md,
//! Contracheque): o líquido na data de pagamento, com cada rubrica como sub-registro, e o
//! elo com o crédito do banco (o mesmo fato) quando o Salário é conciliado.
//!
//! Costura de 2026-11-17: o agente do inventário implementa; a janela e o reconhecimento
//! chamam estas assinaturas.

use rusqlite::Connection;

/// Uma rubrica do contracheque, como sub-registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubricaDoContracheque {
    pub codigo: Option<String>,
    pub descricao: String,
    pub valor_centavos: i64,
    pub desconto: bool,
    /// A parcela atual, nos descontos de contrato.
    pub parcela: Option<u32>,
}

/// O que o contracheque declara.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalarioDoContracheque {
    /// O sha256 do PDF (o achado).
    pub sha256: String,
    /// `AAAA-MM-DD`: a data de pagamento.
    pub data: String,
    pub liquido_centavos: i64,
    pub rubricas: Vec<RubricaDoContracheque>,
}

/// Cria, uma vez só (pelo sha), o registro `salario` do contracheque com as rubricas como
/// sub-registros. Devolve o id do registro (o existente, se já havia).
///
/// # Errors
/// Falha do banco ou achado inexistente.
pub fn registrar_salario(conn: &Connection, s: &SalarioDoContracheque) -> Result<i64, String> {
    let _ = (conn, s);
    Err("o registro do salário ainda não é gravado".to_owned())
}

/// Liga o registro do contracheque (`sha256`) ao crédito do banco (`external_ref`) como
/// conciliação e muda o crédito para o tipo `salario`, pelo log de decisões (com Rastro e
/// desfazer). Idempotente. `Ok(false)` quando falta um dos dois registros.
///
/// # Errors
/// Falha do banco.
pub fn ligar_salario_ao_credito(
    conn: &Connection,
    sha256: &str,
    external_ref: &str,
) -> Result<bool, String> {
    let _ = (conn, sha256, external_ref);
    Err("o elo do salário ainda não é gravado".to_owned())
}
