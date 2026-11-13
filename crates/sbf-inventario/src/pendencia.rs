//! Dono: agente do núcleo (spec 06, "Como cada decisão do §10 cai no banco"). Ver `lib.rs`.
//!
//! Sinalizar abre uma pendência sobre qualquer item; Responder grava `resposta` e
//! `resolvida_em`. Desfazer a sinalização **fecha** a pendência (com
//! [`RESPOSTA_DE_SINALIZACAO_DESFEITA`](crate::projecao::RESPOSTA_DE_SINALIZACAO_DESFEITA))
//! em vez de apagá-la: os scripts também criam pendências, e um id apagado poderia ser
//! reaproveitado por eles.

use rusqlite::Transaction;
use serde_json::json;

use crate::comando::ResultadoDeComando;
use crate::decisao::Efeito;
use crate::projecao::{AlvoTipo, campo};
use crate::referencia::Ref;

/// `pendencia.etapa` das pendências abertas na janela.
pub const ETAPA_DA_JANELA: &str = "desktop";
/// `pendencia.tipo` das pendências abertas por Sinalizar.
pub const TIPO_SINALIZADA: &str = "sinalizada";

/// A decisão que abre uma pendência sobre `alvo`, com um id novo.
pub(crate) fn sinalizar(
    tx: &Transaction<'_>,
    alvo: &Ref,
    pergunta: &str,
    em: &str,
) -> ResultadoDeComando<Efeito> {
    let nova: i64 = tx.query_row(
        "SELECT MAX(COALESCE((SELECT MAX(id) FROM pendencia), 0),
                    COALESCE((SELECT MAX(CAST(alvo_id AS INTEGER)) FROM decisao
                               WHERE alvo_tipo = 'pendencia'), 0)) + 1",
        [],
        |l| l.get(0),
    )?;
    Ok(Efeito::novo(
        AlvoTipo::Pendencia,
        nova.to_string(),
        campo::CRIADA,
        json!({
            "etapa": ETAPA_DA_JANELA,
            "tipo": TIPO_SINALIZADA,
            "referencia": alvo.to_string(),
            "pergunta": pergunta,
            "criada_em": em,
            "resolvida_em": null,
            "resposta": null,
        }),
    ))
}

/// A decisão que responde a pendência `id`.
pub(crate) fn responder(id: &str, resposta: &str, em: &str) -> Efeito {
    Efeito::novo(
        AlvoTipo::Pendencia,
        id,
        campo::RESPOSTA,
        json!({"resposta": resposta, "resolvida_em": em}),
    )
}
