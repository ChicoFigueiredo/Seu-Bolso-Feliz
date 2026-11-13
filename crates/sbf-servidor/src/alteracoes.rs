//! `POST /alteracoes` (grill Q6/Q7): a primeira escrita de ponta a ponta, mudar a
//! categoria de uma movimentação. Numa transação só: confere a versão de base de cada
//! linha tocada com a linha travada (`FOR UPDATE`), aplica inteira ou recusa, e grava a
//! proposta com a resposta em `alteracoes` (reenviar o mesmo `id` devolve a mesma
//! resposta).

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use sbf_contrato::{
    Acao, CodigoDeErro, Colecao, LinhaAgora, PropostaDeAlteracao, RespostaDaAlteracao,
};
use uuid::Uuid;

use crate::EstadoDoServidor;
use crate::banco::sql_versao;
use crate::estacoes::EstacaoAutenticada;
use crate::http::ErroHttp;
use crate::livro::ler_movimentacao;

/// As Ações que este servidor aplica, pelo `tipo` do fio.
const ACOES_ACEITAS: [&str; 1] = ["mudar_categoria_da_movimentacao"];

/// `POST /alteracoes`.
pub async fn receber(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    corpo: Bytes,
) -> Result<Json<RespostaDaAlteracao>, ErroHttp> {
    let proposta = ler_proposta(&corpo)?;
    validar(&proposta)?;
    let mut cliente = estado.banco.get().await?;

    // Já recebida? Devolve a mesma resposta.
    if let Some(resposta) = resposta_guardada(&cliente, quem.dono, proposta.id).await? {
        return Ok(Json(resposta));
    }

    let transacao = cliente.transaction().await?;
    let resposta = match &proposta.acao {
        Acao::MudarCategoriaDaMovimentacao {
            movimentacao,
            categoria,
        } => mudar_categoria(&transacao, quem.dono, &proposta, *movimentacao, *categoria).await?,
    };
    let (acao, resultado) = (
        tipo_da_acao(&proposta.acao),
        match resposta {
            RespostaDaAlteracao::Aceita { .. } => "aceita",
            RespostaDaAlteracao::Recusada { .. } => "recusada",
        },
    );
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

fn tipo_da_acao(acao: &Acao) -> &'static str {
    match acao {
        Acao::MudarCategoriaDaMovimentacao { .. } => "mudar_categoria_da_movimentacao",
    }
}

/// Lê o corpo. Uma Ação com `tipo` que o servidor não conhece é
/// [`CodigoDeErro::AcaoAindaNaoAceita`]; o resto fora do contrato é pedido inválido.
pub fn ler_proposta(corpo: &[u8]) -> Result<PropostaDeAlteracao, ErroHttp> {
    let valor: serde_json::Value = serde_json::from_slice(corpo)
        .map_err(|_| ErroHttp::pedido_invalido("o corpo não é JSON"))?;
    if let Some(tipo) = valor
        .get("acao")
        .and_then(|a| a.get("tipo"))
        .and_then(|t| t.as_str())
        && !ACOES_ACEITAS.contains(&tipo)
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

/// A proposta é coerente: o item é o da Ação e há versão de base para cada linha tocada
/// (e só para elas).
pub fn validar(proposta: &PropostaDeAlteracao) -> Result<(), ErroHttp> {
    match &proposta.acao {
        Acao::MudarCategoriaDaMovimentacao { movimentacao, .. } => {
            if proposta.item.colecao != Colecao::Movimentacoes || proposta.item.id != *movimentacao
            {
                return Err(ErroHttp::pedido_invalido(
                    "o item da alteração não é a movimentação da ação",
                ));
            }
            let [base] = proposta.versoes_de_base.as_slice() else {
                return Err(ErroHttp::pedido_invalido(
                    "mudar a categoria toca uma linha: mande uma versão de base",
                ));
            };
            if base.colecao != Colecao::Movimentacoes || base.id != *movimentacao {
                return Err(ErroHttp::pedido_invalido(
                    "a versão de base não é a da movimentação",
                ));
            }
        }
    }
    Ok(())
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

fn recusada(
    proposta: &PropostaDeAlteracao,
    motivo: CodigoDeErro,
    mensagem: &str,
    agora: Vec<LinhaAgora>,
) -> RespostaDaAlteracao {
    RespostaDaAlteracao::Recusada {
        id: proposta.id,
        motivo,
        mensagem: mensagem.into(),
        sua_versao: Box::new(proposta.clone()),
        no_servidor_agora: agora,
    }
}

const SQL_TRAVAR_MOVIMENTACAO: &str = concat!(
    "SELECT ",
    sql_versao!("updated_at"),
    " AS versao FROM public.transactions WHERE id = $1 AND user_id = $2 FOR UPDATE"
);

async fn mudar_categoria(
    transacao: &tokio_postgres::Transaction<'_>,
    dono: Uuid,
    proposta: &PropostaDeAlteracao,
    movimentacao: Uuid,
    categoria: Option<Uuid>,
) -> Result<RespostaDaAlteracao, ErroHttp> {
    let travada = transacao
        .query_opt(SQL_TRAVAR_MOVIMENTACAO, &[&movimentacao, &dono])
        .await?;
    let Some(travada) = travada else {
        return Ok(recusada(
            proposta,
            CodigoDeErro::ItemNaoEncontrado,
            "a movimentação não existe mais no servidor",
            vec![],
        ));
    };
    let versao_agora: String = travada.try_get("versao")?;
    let base = &proposta.versoes_de_base[0].versao;
    if base.0 != versao_agora {
        let agora = ler_movimentacao(transacao, dono, movimentacao)
            .await?
            .map(|m| LinhaAgora::Movimentacoes(Box::new(m)))
            .into_iter()
            .collect();
        return Ok(recusada(
            proposta,
            CodigoDeErro::VersaoDeBaseDivergente,
            "a movimentação mudou no servidor depois da sua cópia",
            agora,
        ));
    }
    if let Some(categoria) = categoria {
        let existe = transacao
            .query_opt(
                "SELECT 1 FROM public.categories WHERE id = $1 AND user_id = $2",
                &[&categoria, &dono],
            )
            .await?
            .is_some();
        if !existe {
            let agora = ler_movimentacao(transacao, dono, movimentacao)
                .await?
                .map(|m| LinhaAgora::Movimentacoes(Box::new(m)))
                .into_iter()
                .collect();
            return Ok(recusada(
                proposta,
                CodigoDeErro::ReferenciaNaoEncontrada,
                "a categoria escolhida não existe no servidor",
                agora,
            ));
        }
    }
    // O gatilho `trg_transactions_updated_at` muda `updated_at`, e com ele a versão.
    transacao
        .execute(
            "UPDATE public.transactions SET category_id = $3 WHERE id = $1 AND user_id = $2",
            &[&movimentacao, &dono, &categoria],
        )
        .await?;
    let linha = ler_movimentacao(transacao, dono, movimentacao)
        .await?
        .ok_or_else(|| ErroHttp::interno("a movimentação alterada sumiu"))?;
    Ok(RespostaDaAlteracao::Aceita {
        id: proposta.id,
        linhas: vec![LinhaAgora::Movimentacoes(Box::new(linha))],
    })
}
