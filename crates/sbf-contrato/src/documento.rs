//! O envio de documentos (spec 08): `POST /api/v2/documentos`, `multipart/form-data`.
//!
//! Campos, nesta ordem: [`CAMPO_SHA256`] (texto, 64 hexadecimais minúsculos),
//! [`CAMPO_NOME_ORIGINAL`] (texto), [`CAMPO_ID_NO_INVENTARIO`] (texto, opcional: o id do
//! registro no inventário, só para a estação conciliar a resposta) e [`CAMPO_ARQUIVO`] (os
//! bytes, com o `Content-Type` do arquivo). O servidor confere que o SHA-256 dos bytes é o
//! declarado. É idempotente pelo SHA-256 do dono: reenviar o mesmo conteúdo devolve o
//! mesmo `source_document_id` e o mesmo `job_id`, com `ja_existia: true`.

use serde::{Deserialize, Serialize};

use crate::Id;

pub const CAMPO_ARQUIVO: &str = "arquivo";
pub const CAMPO_SHA256: &str = "sha256";
pub const CAMPO_NOME_ORIGINAL: &str = "nome_original";
pub const CAMPO_ID_NO_INVENTARIO: &str = "id_no_inventario";

/// O maior documento que o servidor aceita (20 MiB).
pub const TAMANHO_MAXIMO_DO_DOCUMENTO: usize = 20 * 1024 * 1024;

/// A resposta de `POST /documentos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RespostaDoDocumento {
    pub source_document_id: Id,
    pub job_id: Id,
    /// O conteúdo já estava no servidor (reenvio): nada novo foi criado.
    pub ja_existia: bool,
    /// O mesmo SHA-256 que a estação mandou, ecoado.
    pub sha256: String,
    /// O `id_no_inventario` que a estação mandou, ecoado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_no_inventario: Option<String>,
}
