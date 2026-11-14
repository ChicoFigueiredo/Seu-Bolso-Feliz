//! `POST /alteracoes` (grill Q6/Q7; spec 08): numa transação só, confere a versão de base de
//! cada linha tocada com a linha travada (`FOR UPDATE`), aplica a Ação inteira ou recusa, e
//! grava a proposta com a resposta em `alteracoes` (reenviar o mesmo `id` devolve a mesma
//! resposta). As Ações estão em [`crate::acoes`].

use axum::Extension;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use sbf_contrato::{Acao, CodigoDeErro, PropostaDeAlteracao, RespostaDaAlteracao};
use uuid::Uuid;

use crate::EstadoDoServidor;
use crate::acoes;
use crate::estacoes::EstacaoAutenticada;
use crate::http::{ContratoDoPedido, ErroHttp};

pub use crate::acoes::validar;

/// `POST /alteracoes`.
pub async fn receber(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    Extension(contrato): Extension<ContratoDoPedido>,
    corpo: Bytes,
) -> Result<Json<RespostaDaAlteracao>, ErroHttp> {
    let proposta = ler_proposta(&corpo, contrato.0)?;
    validar(&proposta)?;
    let mut cliente = estado.banco.get().await?;

    // Já recebida? Devolve a mesma resposta.
    if let Some(resposta) = resposta_guardada(&cliente, quem.dono, proposta.id).await? {
        return Ok(Json(resposta));
    }

    let mut transacao = cliente.transaction().await?;
    let resposta = acoes::aplicar(&mut transacao, quem.dono, &proposta).await?;
    let resultado = match resposta {
        RespostaDaAlteracao::Aceita { .. } => "aceita",
        RespostaDaAlteracao::Recusada { .. } => "recusada",
    };
    let acao = proposta.acao.tipo();
    let proposta_json = serde_json::to_value(&proposta).map_err(ErroHttp::interno)?;
    let resposta_json = serde_json::to_value(&resposta).map_err(ErroHttp::interno)?;
    let gravadas = transacao
        .execute(
            "INSERT INTO public.alteracoes (id, user_id, estacao_id, acao, resultado, proposta, resposta) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
            &[
                &proposta.id,
                &quem.dono,
                &quem.id,
                &acao,
                &resultado,
                &proposta_json,
                &resposta_json,
            ],
        )
        .await?;
    if gravadas == 0 {
        // Outra cópia da mesma proposta chegou primeiro: desfaz e devolve a dela.
        transacao.rollback().await?;
        return resposta_guardada(&cliente, quem.dono, proposta.id)
            .await?
            .map(Json)
            .ok_or_else(|| ErroHttp::pedido_invalido("o id desta alteração já foi usado"));
    }
    transacao.commit().await?;
    Ok(Json(resposta))
}

/// Lê o corpo. Uma Ação com `tipo` que esta versão do contrato não conhece é
/// [`CodigoDeErro::AcaoAindaNaoAceita`]; o resto fora do contrato é pedido inválido.
pub fn ler_proposta(
    corpo: &[u8],
    versao_do_contrato: u32,
) -> Result<PropostaDeAlteracao, ErroHttp> {
    let valor: serde_json::Value = serde_json::from_slice(corpo)
        .map_err(|_| ErroHttp::pedido_invalido("o corpo não é JSON"))?;
    let aceitas: &[&str] = if versao_do_contrato >= 2 {
        &Acao::TIPOS
    } else {
        &Acao::TIPOS_DA_V1
    };
    if let Some(tipo) = valor
        .get("acao")
        .and_then(|a| a.get("tipo"))
        .and_then(|t| t.as_str())
        && !aceitas.contains(&tipo)
    {
        return Err(ErroHttp::novo(
            StatusCode::UNPROCESSABLE_ENTITY,
            CodigoDeErro::AcaoAindaNaoAceita,
            "esta ação ainda não é aceita pelo servidor",
        ));
    }
    serde_json::from_value(valor)
        .map_err(|_| ErroHttp::pedido_invalido("a alteração não segue o contrato"))
}

async fn resposta_guardada(
    cliente: &tokio_postgres::Client,
    dono: Uuid,
    id: Uuid,
) -> Result<Option<RespostaDaAlteracao>, ErroHttp> {
    let linha = cliente
        .query_opt(
            "SELECT user_id, resposta FROM public.alteracoes WHERE id = $1",
            &[&id],
        )
        .await?;
    let Some(linha) = linha else {
        return Ok(None);
    };
    let de_quem: Uuid = linha.try_get("user_id")?;
    if de_quem != dono {
        return Err(ErroHttp::pedido_invalido(
            "o id desta alteração já foi usado",
        ));
    }
    let resposta: serde_json::Value = linha.try_get("resposta")?;
    serde_json::from_value(resposta)
        .map(Some)
        .map_err(ErroHttp::interno)
}
