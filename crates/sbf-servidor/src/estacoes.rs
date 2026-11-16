//! As estações: autenticação de todo pedido, pareamento, lista e revogação.
//!
//! Todo filtro é pelo **dono** (`estacoes.user_id`), nunca pelo ator: o admin de
//! emergência que pareia uma estação a pareia para os dados do dono.

use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use sbf_contrato::{CodigoDeErro, Estacao, Estacoes, Pareamento, PedidoDePareamento};
use tokio_postgres::{GenericClient, Row};
use uuid::Uuid;

use crate::EstadoDoServidor;
use crate::banco::{ErroDeLeitura, Leitor, sql_instante};
use crate::credencial::{Credencial, do_cabecalho};
use crate::http::{ContratoDoPedido, ErroHttp};

/// A estação autenticada do pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstacaoAutenticada {
    pub id: Uuid,
    /// O dono dos dados: o filtro de toda consulta.
    pub dono: Uuid,
}

impl FromRequestParts<EstadoDoServidor> for EstacaoAutenticada {
    type Rejection = ErroHttp;

    async fn from_request_parts(
        partes: &mut Parts,
        estado: &EstadoDoServidor,
    ) -> Result<Self, Self::Rejection> {
        let token = partes
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(do_cabecalho)
            .ok_or_else(ErroHttp::nao_autenticado)?;
        let credencial = Credencial::ler(token).ok_or_else(ErroHttp::nao_autenticado)?;
        let contrato = partes
            .extensions
            .get::<ContratoDoPedido>()
            .map_or(sbf_contrato::VERSAO_DO_CONTRATO, |c| c.0);
        let cliente = estado.banco.get().await?;
        autenticar(&**cliente, &credencial, contrato).await
    }
}

/// Confere a credencial e registra o contato. Revogada → [`ErroHttp::estacao_revogada`];
/// desconhecida ou segredo errado → [`ErroHttp::nao_autenticado`], sem detalhe.
pub async fn autenticar<C: GenericClient>(
    cliente: &C,
    credencial: &Credencial,
    contrato: u32,
) -> Result<EstacaoAutenticada, ErroHttp> {
    let linha = cliente
        .query_opt(
            "SELECT user_id, credencial_sha256, revogada_em IS NOT NULL AS revogada \
             FROM public.estacoes WHERE id = $1",
            &[&credencial.estacao],
        )
        .await?;
    let Some(linha) = linha else {
        return Err(ErroHttp::nao_autenticado());
    };
    let dono: Uuid = linha.try_get("user_id")?;
    let revogada: bool = linha.try_get("revogada")?;
    if revogada {
        return Err(ErroHttp::estacao_revogada());
    }
    let hash: Option<Vec<u8>> = linha.try_get("credencial_sha256")?;
    if !hash.is_some_and(|h| credencial.confere(&h)) {
        return Err(ErroHttp::nao_autenticado());
    }
    let contrato = i32::try_from(contrato).unwrap_or(i32::MAX);
    cliente
        .execute(
            "UPDATE public.estacoes SET ultimo_contato = now(), contrato = $2 WHERE id = $1",
            &[&credencial.estacao, &contrato],
        )
        .await?;
    Ok(EstacaoAutenticada {
        id: credencial.estacao,
        dono,
    })
}

const SQL_ESTACAO: &str = concat!(
    "SELECT id, nome, sistema, ",
    sql_instante!("pareada_em"),
    " AS pareada_em, ",
    sql_instante!("ultimo_contato"),
    " AS ultimo_contato, revogada_em IS NOT NULL AS revogada, ",
    sql_instante!("revogada_em"),
    " AS revogada_em FROM public.estacoes"
);

fn estacao(r: &Row) -> Result<Estacao, ErroDeLeitura> {
    let l = Leitor(r);
    Ok(Estacao {
        id: l.id("id")?,
        nome: l.texto("nome")?,
        sistema: l.texto("sistema")?,
        pareada_em: l.texto("pareada_em")?,
        ultimo_contato: l.texto_opc("ultimo_contato")?,
        revogada: l.booleano("revogada")?,
        revogada_em: l.texto_opc("revogada_em")?,
    })
}

/// Uma estação do dono.
pub async fn ler_estacao<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    id: Uuid,
) -> Result<Option<Estacao>, ErroHttp> {
    let sql = format!("{SQL_ESTACAO} WHERE user_id = $1 AND id = $2");
    let linha = cliente.query_opt(sql.as_str(), &[&dono, &id]).await?;
    Ok(linha.as_ref().map(estacao).transpose()?)
}

/// `GET /estacoes`.
pub async fn listar(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
) -> Result<Json<Estacoes>, ErroHttp> {
    let cliente = estado.banco.get().await?;
    let sql = format!("{SQL_ESTACAO} WHERE user_id = $1 ORDER BY pareada_em, id");
    let linhas = cliente.query(sql.as_str(), &[&quem.dono]).await?;
    let estacoes = linhas.iter().map(estacao).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(Estacoes {
        estacoes,
        esta: quem.id,
    }))
}

/// `POST /estacoes/{id}/revogar`. Qualquer estação ativa do dono revoga qualquer outra
/// (ou a si mesma). Revogar de novo devolve a estação como está.
pub async fn revogar(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    Path((_versao, id)): Path<(String, String)>,
) -> Result<Json<Estacao>, ErroHttp> {
    let id = Uuid::try_parse(&id).map_err(|_| ErroHttp::nao_encontrado("estação desconhecida"))?;
    let cliente = estado.banco.get().await?;
    revogar_estacao(&**cliente, Some(quem.dono), id)
        .await?
        .map(Json)
        .ok_or_else(|| ErroHttp::nao_encontrado("estação desconhecida"))
}

/// Revoga: apaga o hash e marca a hora. `dono: None` é o comando de administração na
/// VPS (`sbf servidor revogar`), que não tem estação para se autenticar.
pub async fn revogar_estacao<C: GenericClient>(
    cliente: &C,
    dono: Option<Uuid>,
    id: Uuid,
) -> Result<Option<Estacao>, ErroHttp> {
    let linha = cliente
        .query_opt(
            "UPDATE public.estacoes SET credencial_sha256 = NULL, revogada_em = now() \
             WHERE id = $1 AND ($2::uuid IS NULL OR user_id = $2) AND revogada_em IS NULL \
             RETURNING user_id",
            &[&id, &dono],
        )
        .await?;
    let dono_de_fato: Option<Uuid> = match (linha, dono) {
        (Some(l), _) => Some(l.try_get("user_id")?),
        (None, Some(d)) => Some(d),
        (None, None) => cliente
            .query_opt("SELECT user_id FROM public.estacoes WHERE id = $1", &[&id])
            .await?
            .map(|l| l.try_get("user_id"))
            .transpose()?,
    };
    match dono_de_fato {
        Some(d) => ler_estacao(cliente, d, id).await,
        None => Ok(None),
    }
}

/// Todas as estações, de todos os donos (comando de administração na VPS).
pub async fn listar_todas<C: GenericClient>(cliente: &C) -> Result<Vec<Estacao>, ErroHttp> {
    let sql = format!("{SQL_ESTACAO} ORDER BY pareada_em, id");
    let linhas = cliente.query(sql.as_str(), &[]).await?;
    Ok(linhas.iter().map(estacao).collect::<Result<Vec<_>, _>>()?)
}

fn recusar_pareamento(mensagem: &str) -> ErroHttp {
    ErroHttp::novo(
        StatusCode::FORBIDDEN,
        CodigoDeErro::PareamentoRecusado,
        mensagem,
    )
}

fn validar_nome_e_sistema<'a>(
    nome: &'a str,
    sistema: &'a str,
) -> Result<(&'a str, &'a str), ErroHttp> {
    let nome = nome.trim();
    let sistema = sistema.trim();
    if nome.is_empty() || nome.chars().count() > 120 {
        return Err(ErroHttp::pedido_invalido(
            "o nome da estação precisa ter de 1 a 120 caracteres",
        ));
    }
    if sistema.is_empty() || sistema.chars().count() > 60 {
        return Err(ErroHttp::pedido_invalido(
            "o sistema precisa ter de 1 a 60 caracteres",
        ));
    }
    Ok((nome, sistema))
}

/// O usuário do e-mail (sem diferenciar maiúsculas), travado para a transação.
async fn buscar_usuario<C: GenericClient>(
    cliente: &C,
    email: &str,
) -> Result<Option<Row>, ErroHttp> {
    Ok(cliente
        .query_opt(
            "SELECT id, google_sub, COALESCE(owner_id, id) AS dono FROM public.users \
             WHERE lower(email) = $1 FOR UPDATE",
            &[&email],
        )
        .await?)
}

/// Cria a linha da estação com uma credencial nova; só o hash vai para o banco.
async fn inserir_estacao<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    ator: Uuid,
    nome: &str,
    sistema: &str,
    contrato: u32,
) -> Result<(Estacao, Credencial), ErroHttp> {
    let id = Uuid::new_v4();
    let credencial = Credencial::nova(id).map_err(ErroHttp::interno)?;
    let hash = credencial.hash().to_vec();
    let contrato = i32::try_from(contrato).unwrap_or(i32::MAX);
    cliente
        .execute(
            "INSERT INTO public.estacoes (id, user_id, pareada_por, nome, sistema, \
             credencial_sha256, contrato) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[&id, &dono, &ator, &nome, &sistema, &hash, &contrato],
        )
        .await?;
    let estacao = ler_estacao(cliente, dono, id)
        .await?
        .ok_or_else(|| ErroHttp::interno("a estação recém-criada sumiu"))?;
    Ok((estacao, credencial))
}

/// Pareamento administrativo (`sbf servidor emitir-credencial`), sem Google: o e-mail
/// precisa ser de um usuário existente, como no pareamento normal. Devolve a estação e o
/// texto da credencial.
pub async fn emitir_credencial(
    cliente: &mut deadpool_postgres::Object,
    email: &str,
    nome: &str,
    sistema: &str,
) -> Result<(Estacao, zeroize::Zeroizing<String>), ErroHttp> {
    let (nome, sistema) = validar_nome_e_sistema(nome, sistema)?;
    let transacao = cliente.transaction().await?;
    let email = email.trim().to_lowercase();
    let Some(usuario) = buscar_usuario(&*transacao, &email).await? else {
        return Err(recusar_pareamento(
            "este e-mail não tem cadastro no Seu Bolso Feliz",
        ));
    };
    let ator: Uuid = usuario.try_get("id")?;
    let dono: Uuid = usuario.try_get("dono")?;
    let (estacao, credencial) = inserir_estacao(
        &*transacao,
        dono,
        ator,
        nome,
        sistema,
        sbf_contrato::VERSAO_DO_CONTRATO,
    )
    .await?;
    transacao.commit().await?;
    Ok((estacao, credencial.texto()))
}

/// `POST /estacoes/pareamento`: login Google válido de usuário conhecido → credencial.
pub async fn parear(
    State(estado): State<EstadoDoServidor>,
    Extension(contrato): Extension<ContratoDoPedido>,
    corpo: Bytes,
) -> Result<impl IntoResponse, ErroHttp> {
    let pedido: PedidoDePareamento = serde_json::from_slice(&corpo)
        .map_err(|_| ErroHttp::pedido_invalido("o corpo do pareamento não segue o contrato"))?;
    let (nome, sistema) = validar_nome_e_sistema(&pedido.nome, &pedido.sistema)?;
    let identidade = match estado
        .google
        .verificar(&pedido.id_token, &pedido.nonce)
        .await
    {
        Ok(i) => i,
        Err(crate::google::ErroDoGoogle::Chaves(e)) => {
            return Err(ErroHttp::interno(format_args!("chaves do Google: {e}")));
        }
        Err(e) => {
            eprintln!("sbf servidor: pareamento recusado: {e}");
            return Err(recusar_pareamento("o login Google não foi aceito"));
        }
    };

    let mut cliente = estado.banco.get().await?;
    let transacao = cliente.transaction().await?;
    let Some(usuario) = buscar_usuario(&*transacao, &identidade.email).await? else {
        eprintln!("sbf servidor: pareamento recusado: e-mail sem cadastro");
        return Err(recusar_pareamento(
            "esta conta Google não tem acesso ao Seu Bolso Feliz",
        ));
    };
    let ator: Uuid = usuario.try_get("id")?;
    let dono: Uuid = usuario.try_get("dono")?;
    let sub: Option<String> = usuario.try_get("google_sub")?;
    match sub {
        Some(s) if s != identidade.sub => {
            eprintln!("sbf servidor: pareamento recusado: conta Google diferente da vinculada");
            return Err(recusar_pareamento(
                "esta conta Google não é a que está vinculada a este e-mail",
            ));
        }
        Some(_) => {}
        None => {
            // Como o web: o primeiro login pelo Google vincula a conta.
            transacao
                .execute(
                    "UPDATE public.users SET google_sub = $2, last_login_at = now() WHERE id = $1",
                    &[&ator, &identidade.sub],
                )
                .await?;
        }
    }

    let (estacao, credencial) =
        inserir_estacao(&*transacao, dono, ator, nome, sistema, contrato.0).await?;
    transacao.commit().await?;
    let resposta = Pareamento {
        estacao,
        credencial: credencial.texto().to_string(),
    };
    Ok((StatusCode::CREATED, Json(resposta)))
}
