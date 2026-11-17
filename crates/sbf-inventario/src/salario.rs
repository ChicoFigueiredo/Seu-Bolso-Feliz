//! O registro do tipo salário que um contracheque declara (spec 12, D1; CONTEXT.md,
//! Contracheque): o líquido na data de pagamento, com cada rubrica como sub-registro, e o
//! elo com o crédito do banco (o mesmo fato) quando o Salário é conciliado.
//!
//! Costura de 2026-11-17: o agente do inventário implementa; a janela e o reconhecimento
//! chamam estas assinaturas.

use rusqlite::{Connection, OptionalExtension};

use crate::comando::{Acao, ErroDeComando};
use crate::decisao;
use crate::projecao::ciclo;
use crate::referencia::{Ref, Tipo};

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

/// A fonte dos contracheques que não têm origem conhecida (tipo `rh`).
const FONTE_DO_CONTRACHEQUE: &str = "rh:contracheque";

/// Cria, uma vez só (pelo sha), o registro `salario` do contracheque com as rubricas como
/// sub-registros. Devolve o id do registro (o existente, se já havia).
///
/// Convenções: o registro é `tipo = 'salario'`, `data_tipo = 'pagamento'`,
/// `extraido_por = 'contracheque@1'`, com o líquido (positivo, entrou). A fonte é a da
/// primeira origem do achado (a mesma dos outros registros de documento); sem origem, a
/// fonte `rh:contracheque`. Cada rubrica é um sub-registro `tipo = 'rubrica'` com o **mesmo
/// sinal do registro**: provento positivo, desconto negativo (a soma das rubricas dá o
/// líquido).
///
/// # Errors
/// Falha do banco ou achado inexistente.
pub fn registrar_salario(conn: &Connection, s: &SalarioDoContracheque) -> Result<i64, String> {
    let erro = |e: rusqlite::Error| format!("erro do banco: {e}");
    let tx = conn.unchecked_transaction().map_err(erro)?;
    let achado: bool = tx
        .query_row(
            "SELECT 1 FROM achado WHERE content_hash = ?1",
            [&s.sha256],
            |_| Ok(()),
        )
        .optional()
        .map_err(erro)?
        .is_some();
    if !achado {
        return Err("achado inexistente".to_owned());
    }
    let existente: Option<i64> = tx
        .query_row(
            "SELECT id FROM registro WHERE content_hash = ?1 AND extraido_por = 'contracheque@1'
              ORDER BY id LIMIT 1",
            [&s.sha256],
            |l| l.get(0),
        )
        .optional()
        .map_err(erro)?;
    if let Some(id) = existente {
        return Ok(id);
    }
    let fonte: Option<String> = tx
        .query_row(
            "SELECT fonte_id FROM origem WHERE content_hash = ?1 ORDER BY id LIMIT 1",
            [&s.sha256],
            |l| l.get(0),
        )
        .optional()
        .map_err(erro)?;
    let fonte = match fonte {
        Some(f) => f,
        None => {
            tx.execute(
                "INSERT OR IGNORE INTO fonte (id, tipo, descricao) VALUES (?1, 'rh', 'Contracheques')",
                [FONTE_DO_CONTRACHEQUE],
            )
            .map_err(erro)?;
            FONTE_DO_CONTRACHEQUE.to_owned()
        }
    };
    let ciclo = ciclo::ciclo_de(&s.data);
    if let Some(c) = ciclo.as_deref().and_then(ciclo::limites_do_ciclo) {
        tx.execute(
            "INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
            rusqlite::params![c.id, c.inicio, c.fim],
        )
        .map_err(erro)?;
    }
    tx.execute(
        "INSERT INTO registro (content_hash, fonte_id, tipo, data, data_tipo, ciclo_id,
                               valor_centavos, descricao_original, extraido_por)
         VALUES (?1, ?2, 'salario', ?3, 'pagamento', ?4, ?5, 'Contracheque', 'contracheque@1')",
        rusqlite::params![s.sha256, fonte, s.data, ciclo, s.liquido_centavos],
    )
    .map_err(erro)?;
    let id = tx.last_insert_rowid();
    for r in &s.rubricas {
        let valor = if r.desconto {
            -r.valor_centavos.abs()
        } else {
            r.valor_centavos.abs()
        };
        tx.execute(
            "INSERT INTO sub_registro (registro_id, tipo, codigo, descricao_original,
                                       valor_centavos, parcela_numero)
             VALUES (?1, 'rubrica', ?2, ?3, ?4, ?5)",
            rusqlite::params![id, r.codigo, r.descricao, valor, r.parcela],
        )
        .map_err(erro)?;
    }
    tx.commit().map_err(erro)?;
    Ok(id)
}

/// Liga o registro do contracheque (`sha256`) ao crédito do banco (`external_ref`) como
/// conciliação e muda o crédito para o tipo `salario`, pelo log de decisões (com Rastro e
/// desfazer). Idempotente. `Ok(false)` quando falta um dos dois registros.
///
/// As decisões são as mesmas das Ações do §10 (`UnirConciliados` e `CorrigirTipo`), na
/// mesma transação; só as que ainda não valem são gravadas.
///
/// # Errors
/// Falha do banco.
pub fn ligar_salario_ao_credito(
    conn: &Connection,
    sha256: &str,
    external_ref: &str,
) -> Result<bool, String> {
    let erro = |e: rusqlite::Error| format!("erro do banco: {e}");
    let tx = conn.unchecked_transaction().map_err(erro)?;
    let contracheque: Option<i64> = tx
        .query_row(
            "SELECT id FROM registro WHERE content_hash = ?1 AND extraido_por = 'contracheque@1'
              ORDER BY id LIMIT 1",
            [sha256],
            |l| l.get(0),
        )
        .optional()
        .map_err(erro)?;
    let credito: Option<(i64, String)> = tx
        .query_row(
            "SELECT id, tipo FROM registro WHERE external_ref = ?1",
            [external_ref],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()
        .map_err(erro)?;
    let (Some(contracheque), Some((credito, tipo))) = (contracheque, credito) else {
        return Ok(false);
    };
    if contracheque == credito {
        return Ok(false);
    }
    let ligados = tx
        .query_row(
            "SELECT 1 FROM relacao_registro WHERE tipo = 'conciliacao'
               AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1))",
            [contracheque, credito],
            |_| Ok(()),
        )
        .optional()
        .map_err(erro)?
        .is_some();
    let alvo = Ref::nova(Tipo::Registro, credito.to_string());
    let mut acoes = Vec::new();
    if !ligados {
        acoes.push(Acao::UnirConciliados {
            com: Ref::nova(Tipo::Registro, contracheque.to_string()),
        });
    }
    if tipo != "salario" {
        acoes.push(Acao::CorrigirTipo("salario".to_owned()));
    }
    let de_comando = |e: ErroDeComando| e.to_string();
    let em = decisao::agora(&tx).map_err(de_comando)?;
    for acao in &acoes {
        for efeito in decisao::efeitos_da_acao(&tx, acao, &alvo, None, &em).map_err(de_comando)? {
            decisao::gravar(&tx, &efeito, None, None, &em).map_err(de_comando)?;
        }
    }
    tx.commit().map_err(erro)?;
    Ok(true)
}
