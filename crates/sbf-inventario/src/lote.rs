//! Dono: agente do núcleo (spec 06, "Lotes"). Ver `lib.rs`.
//!
//! - **Montar:** criar com nome e adicionar achados em estado Documento e registros.
//! - **Aprovar:** grava `aprovado_em` e muda os registros do lote para `aprovado`, cada
//!   mudança como uma decisão (com o motivo [`motivo_da_aprovacao`]).
//! - **Desaprovar:** a única forma de mudar um lote aprovado. Limpa `aprovado_em` e desfaz
//!   as aprovações de registro que a aprovação do lote fez (as que ainda estão em vigor).
//! - **Enviar ao servidor** não existe nesta fatia (fica para a fatia 3, INV-10).
//!
//! Cada passo é uma decisão com `alvo_tipo = 'lote'` (spec 06, tabela das decisões).

use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::json;

use crate::comando::{ErroDeComando, IdDecisao, ResultadoDeComando};
use crate::decisao::{self, Efeito};
use crate::projecao::{AlvoTipo, campo, erro_de_banco};
use crate::referencia::{Ref, Tipo};

/// Começo do motivo das aprovações de registro feitas por um lote.
pub const PREFIXO_DO_MOTIVO: &str = "aprovação do lote ";

/// Por que o Enviar ao servidor está desabilitado nesta fatia (INV-10).
pub const MOTIVO_DO_ENVIO_DESABILITADO: &str =
    "Chega na fatia 3, com a Caixa de entrada: um lote aprovado ainda não é promovido.";

/// Enviar ao servidor: desabilitado na fatia 1. `Err` traz o motivo para a janela mostrar.
///
/// # Errors
/// Sempre, nesta fatia.
pub fn enviar_ao_servidor(_lote: &Ref) -> Result<(), &'static str> {
    Err(MOTIVO_DO_ENVIO_DESABILITADO)
}

/// O motivo gravado em cada aprovação de registro feita pelo lote `id`.
pub fn motivo_da_aprovacao(id: &str) -> String {
    format!("{PREFIXO_DO_MOTIVO}{id}")
}

fn aprovado_em(tx: &Transaction<'_>, lote: &Ref) -> ResultadoDeComando<Option<String>> {
    if lote.tipo != Tipo::Lote {
        return Err(ErroDeComando::NaoEncontrado(lote.clone()));
    }
    let linha: Option<Option<String>> = tx
        .query_row(
            "SELECT aprovado_em FROM lote WHERE id = ?1",
            [&lote.chave],
            |l| l.get(0),
        )
        .optional()?;
    linha.ok_or_else(|| ErroDeComando::NaoEncontrado(lote.clone()))
}

/// Erro [`ErroDeComando::LoteAprovado`] se o lote está aprovado.
pub(crate) fn exigir_editavel(tx: &Transaction<'_>, lote: &Ref) -> ResultadoDeComando<()> {
    match aprovado_em(tx, lote) {
        Ok(Some(_)) => Err(ErroDeComando::LoteAprovado(lote.clone())),
        Ok(None) => Ok(()),
        // O lote de uma decisão antiga pode já ter sido desfeito: nada a proteger.
        Err(ErroDeComando::NaoEncontrado(_)) => Ok(()),
        Err(outro) => Err(outro),
    }
}

pub(crate) fn tem_itens(tx: &Transaction<'_>, lote: &str) -> ResultadoDeComando<bool> {
    Ok(tx
        .prepare_cached("SELECT 1 FROM lote_item WHERE lote_id = ?1")?
        .exists([lote])?)
}

/// Cria o lote. O id nunca reaproveita o de um lote desfeito (o log ainda fala dele).
pub(crate) fn criar(tx: &Transaction<'_>, nome: &str) -> ResultadoDeComando<Ref> {
    let nome = nome.trim();
    if nome.is_empty() {
        return Err(ErroDeComando::NaoSeAplica {
            acao: "Novo lote (sem nome)".into(),
            alvo: Ref::nova(Tipo::Lote, "novo"),
        });
    }
    let em = decisao::agora(tx)?;
    let id: i64 = tx.query_row(
        "SELECT MAX(COALESCE((SELECT MAX(id) FROM lote), 0),
                    COALESCE((SELECT MAX(CAST(alvo_id AS INTEGER)) FROM decisao
                               WHERE alvo_tipo = 'lote'), 0)) + 1",
        [],
        |l| l.get(0),
    )?;
    let efeito = Efeito::novo(
        AlvoTipo::Lote,
        id.to_string(),
        campo::CRIADO,
        json!({"nome": nome, "criado_em": em}),
    );
    decisao::gravar(tx, &efeito, None, None, &em)?;
    Ok(Ref::nova(Tipo::Lote, id.to_string()))
}

fn item_do_lote(tx: &Transaction<'_>, item: &Ref) -> ResultadoDeComando<(&'static str, String)> {
    match decisao::resolver_alvo(tx, item)? {
        AlvoTipo::Achado => {
            let estado: String = tx.query_row(
                "SELECT estado FROM achado WHERE content_hash = ?1",
                [&item.chave],
                |l| l.get(0),
            )?;
            if estado != "documento" {
                return Err(ErroDeComando::NaoSeAplica {
                    acao: "Adicionar ao lote (o achado não está em estado Documento)".into(),
                    alvo: item.clone(),
                });
            }
            Ok(("achado", item.chave.clone()))
        }
        AlvoTipo::Registro => Ok(("registro", item.chave.clone())),
        _ => Err(ErroDeComando::NaoSeAplica {
            acao: "Adicionar ao lote".into(),
            alvo: item.clone(),
        }),
    }
}

fn mudar_itens(
    tx: &Transaction<'_>,
    lote: &Ref,
    itens: &[Ref],
    no_lote: bool,
) -> ResultadoDeComando<()> {
    exigir_editavel(tx, lote)?;
    aprovado_em(tx, lote)?;
    let em = decisao::agora(tx)?;
    for item in itens {
        let (tipo, id) = if no_lote {
            item_do_lote(tx, item)?
        } else {
            let tipo = match item.tipo {
                Tipo::Achado | Tipo::Documento => "achado",
                Tipo::Registro => "registro",
                _ => {
                    return Err(ErroDeComando::NaoSeAplica {
                        acao: "Remover do lote".into(),
                        alvo: item.clone(),
                    });
                }
            };
            (tipo, item.chave.clone())
        };
        let ja_esta = tx
            .prepare_cached(
                "SELECT 1 FROM lote_item WHERE lote_id = ?1 AND alvo_tipo = ?2 AND alvo_id = ?3",
            )?
            .exists(params![lote.chave, tipo, id])?;
        if ja_esta == no_lote {
            continue;
        }
        let efeito = Efeito::novo(
            AlvoTipo::Lote,
            lote.chave.clone(),
            campo::ITEM,
            json!({"tipo": tipo, "id": id, "no_lote": no_lote}),
        );
        decisao::gravar(tx, &efeito, None, None, &em)?;
    }
    Ok(())
}

pub(crate) fn adicionar(tx: &Transaction<'_>, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
    mudar_itens(tx, lote, itens, true)
}

pub(crate) fn remover(tx: &Transaction<'_>, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
    mudar_itens(tx, lote, itens, false)
}

fn registros_do_lote(tx: &Transaction<'_>, lote: &str) -> ResultadoDeComando<Vec<String>> {
    let mut consulta = tx.prepare_cached(
        "SELECT alvo_id FROM lote_item WHERE lote_id = ?1 AND alvo_tipo = 'registro'
          ORDER BY CAST(alvo_id AS INTEGER)",
    )?;
    Ok(consulta
        .query_map([lote], |l| l.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Aprova o lote: cada registro ainda não aprovado vira `aprovado` (uma decisão cada) e,
/// por último, o lote ganha `aprovado_em`. `desfaz` é preenchido quando a aprovação
/// desfaz uma desaprovação.
pub(crate) fn aprovar(
    tx: &Transaction<'_>,
    lote: &Ref,
    em: &str,
    desfaz: Option<IdDecisao>,
) -> ResultadoDeComando<IdDecisao> {
    if aprovado_em(tx, lote)?.is_some() {
        return Err(ErroDeComando::LoteAprovado(lote.clone()));
    }
    let motivo = motivo_da_aprovacao(&lote.chave);
    for registro in registros_do_lote(tx, &lote.chave)? {
        let estado: Option<String> = tx
            .query_row(
                "SELECT estado FROM registro WHERE id = ?1",
                [&registro],
                |l| l.get(0),
            )
            .optional()?;
        if matches!(estado.as_deref(), None | Some("aprovado" | "promovido")) {
            continue;
        }
        let efeito = Efeito::novo(
            AlvoTipo::Registro,
            registro,
            campo::ESTADO,
            json!({"estado": "aprovado"}),
        );
        decisao::gravar(tx, &efeito, Some(&motivo), None, em)?;
    }
    let efeito = Efeito::novo(
        AlvoTipo::Lote,
        lote.chave.clone(),
        campo::APROVADO_EM,
        json!({"aprovado_em": em}),
    );
    decisao::gravar(tx, &efeito, None, desfaz, em)
}

/// Desaprova o lote: desfaz as aprovações de registro desta aprovação que ainda valem e
/// limpa `aprovado_em`. `desfaz` é preenchido quando vem de "Desfazer" da aprovação.
pub(crate) fn desaprovar(
    tx: &Transaction<'_>,
    lote: &Ref,
    em: &str,
    desfaz: Option<IdDecisao>,
) -> ResultadoDeComando<IdDecisao> {
    if aprovado_em(tx, lote)?.is_none() {
        return Err(ErroDeComando::NaoSeAplica {
            acao: "Desaprovar (o lote não está aprovado)".into(),
            alvo: lote.clone(),
        });
    }
    let motivo = motivo_da_aprovacao(&lote.chave);
    for registro in registros_do_lote(tx, &lote.chave)? {
        let estado: Option<String> = tx
            .query_row(
                "SELECT estado FROM registro WHERE id = ?1",
                [&registro],
                |l| l.get(0),
            )
            .optional()?;
        if estado.as_deref() != Some("aprovado") {
            continue;
        }
        let ultima: Option<IdDecisao> = tx
            .query_row(
                "SELECT d.id FROM decisao d
                  WHERE d.alvo_tipo = 'registro' AND d.alvo_id = ?1 AND d.campo = 'estado'
                  ORDER BY d.id DESC LIMIT 1",
                [&registro],
                |l| l.get(0),
            )
            .optional()?;
        let Some(ultima) = ultima else { continue };
        let d = decisao::ler_decisao(tx, ultima)?
            .ok_or_else(|| erro_de_banco("decisão sumiu do log"))?;
        // Só a aprovação feita por este lote, e só se for a última palavra sobre o estado.
        if d.motivo.as_deref() == Some(motivo.as_str()) && d.desfaz.is_none() {
            decisao::desfazer_simples(tx, &d, AlvoTipo::Registro, em)?;
        }
    }
    let efeito = Efeito::novo(
        AlvoTipo::Lote,
        lote.chave.clone(),
        campo::APROVADO_EM,
        json!({"aprovado_em": null}),
    );
    decisao::gravar(tx, &efeito, None, desfaz, em)
}
