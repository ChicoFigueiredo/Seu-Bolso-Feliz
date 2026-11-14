//! O que os canais guardam no Postgres (migração `0006_canais.sql`): destinatários
//! vinculados, códigos de uso único, o último estado avisado de cada alerta, o registro do
//! que saiu (ADR-015) e um pouco de estado do serviço.

use std::collections::HashMap;

use deadpool_postgres::Pool;
use sbf_contrato::Colecao;
use sbf_livro::LivroCaixa;
use sbf_livro::contrato::livro_das_linhas;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::confianca::Nivel;
use crate::erro::ErroDeCanal;

pub const CANAL_TELEGRAM: &str = "telegram";

/// Quanto um código de vinculação vale.
pub const VALIDADE_DO_CODIGO_EM_MINUTOS: i64 = 10;

/// Sem 0/O/1/I/L: o código é digitado à mão no Telegram.
const ALFABETO: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const TAMANHO_DO_CODIGO: usize = 8;

fn sha(texto: &str) -> Vec<u8> {
    Sha256::digest(texto.trim().to_uppercase().as_bytes()).to_vec()
}

/// Um destinatário do canal, com o nível de confiança dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destinatario {
    pub dono: Uuid,
    pub chat: i64,
    pub nivel: Nivel,
}

/// Gera um código de uso único (8 caracteres) que vale 10 minutos. Só o hash fica no banco.
pub async fn gerar_codigo(banco: &Pool, dono: Uuid) -> Result<String, ErroDeCanal> {
    let mut sorteio = [0u8; TAMANHO_DO_CODIGO];
    getrandom::fill(&mut sorteio)
        .map_err(|_| ErroDeCanal::Configuracao("sem fonte de aleatoriedade do sistema".into()))?;
    let codigo: String = sorteio
        .iter()
        .map(|b| char::from(ALFABETO[usize::from(*b) % ALFABETO.len()]))
        .collect();
    let cliente = banco.get().await?;
    cliente
        .execute(
            "DELETE FROM public.canais_codigos WHERE expira_em < now() - interval '1 day'",
            &[],
        )
        .await?;
    cliente
        .execute(
            "INSERT INTO public.canais_codigos (codigo_sha256, user_id, canal, expira_em) \
             VALUES ($1, $2, $3, now() + make_interval(mins => $4::int))",
            &[
                &sha(&codigo),
                &dono,
                &CANAL_TELEGRAM,
                &(VALIDADE_DO_CODIGO_EM_MINUTOS as i32),
            ],
        )
        .await?;
    Ok(codigo)
}

/// Gasta um código: devolve o dono se o código existe, não expirou e ainda não foi usado.
pub async fn consumir_codigo(banco: &Pool, codigo: &str) -> Result<Option<Uuid>, ErroDeCanal> {
    let cliente = banco.get().await?;
    let linha = cliente
        .query_opt(
            "UPDATE public.canais_codigos SET usado_em = now() \
             WHERE codigo_sha256 = $1 AND canal = $2 AND usado_em IS NULL AND expira_em > now() \
             RETURNING user_id",
            &[&sha(codigo), &CANAL_TELEGRAM],
        )
        .await?;
    linha.map(|l| l.try_get(0)).transpose().map_err(Into::into)
}

/// Vincula um chat. Um chat que já estava vinculado e foi revogado volta mascarado.
pub async fn vincular(banco: &Pool, dono: Uuid, chat: i64) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    cliente
        .execute(
            "INSERT INTO public.canais_destinatarios (user_id, canal, identificador) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (canal, identificador) DO UPDATE SET \
               user_id = EXCLUDED.user_id, revogado_em = NULL, vinculado_em = now(), \
               nivel_de_confianca = CASE WHEN canais_destinatarios.revogado_em IS NULL \
                 AND canais_destinatarios.user_id = EXCLUDED.user_id \
                 THEN canais_destinatarios.nivel_de_confianca ELSE 'mascarado' END",
            &[&dono, &CANAL_TELEGRAM, &chat.to_string()],
        )
        .await?;
    Ok(())
}

pub async fn revogar(banco: &Pool, chat: i64) -> Result<bool, ErroDeCanal> {
    let cliente = banco.get().await?;
    let n = cliente
        .execute(
            "UPDATE public.canais_destinatarios SET revogado_em = now() \
             WHERE canal = $1 AND identificador = $2 AND revogado_em IS NULL",
            &[&CANAL_TELEGRAM, &chat.to_string()],
        )
        .await?;
    Ok(n > 0)
}

/// O vínculo ativo de um chat, se houver.
pub async fn destinatario_do_chat(
    banco: &Pool,
    chat: i64,
) -> Result<Option<Destinatario>, ErroDeCanal> {
    let cliente = banco.get().await?;
    let linha = cliente
        .query_opt(
            "SELECT user_id, nivel_de_confianca FROM public.canais_destinatarios \
             WHERE canal = $1 AND identificador = $2 AND revogado_em IS NULL",
            &[&CANAL_TELEGRAM, &chat.to_string()],
        )
        .await?;
    linha
        .map(|l| -> Result<Destinatario, ErroDeCanal> {
            let nivel: String = l.try_get(1)?;
            Ok(Destinatario {
                dono: l.try_get(0)?,
                chat,
                nivel: Nivel::de_codigo(&nivel)?,
            })
        })
        .transpose()
}

/// Todos os destinatários do dono: os vinculados (com o nível deles) e os chats da lista
/// configurada, que ainda não têm vínculo, mascarados.
pub async fn destinatarios(
    banco: &Pool,
    dono: Uuid,
    configurados: impl IntoIterator<Item = i64>,
) -> Result<Vec<Destinatario>, ErroDeCanal> {
    let cliente = banco.get().await?;
    let linhas = cliente
        .query(
            "SELECT identificador, nivel_de_confianca FROM public.canais_destinatarios \
             WHERE user_id = $1 AND canal = $2 AND revogado_em IS NULL ORDER BY vinculado_em",
            &[&dono, &CANAL_TELEGRAM],
        )
        .await?;
    let mut saida: Vec<Destinatario> = Vec::new();
    for l in linhas {
        let id: String = l.try_get(0)?;
        let nivel: String = l.try_get(1)?;
        if let Ok(chat) = id.parse::<i64>() {
            saida.push(Destinatario {
                dono,
                chat,
                nivel: Nivel::de_codigo(&nivel)?,
            });
        }
    }
    for chat in configurados {
        if saida.iter().all(|d| d.chat != chat) {
            saida.push(Destinatario {
                dono,
                chat,
                nivel: Nivel::Mascarado,
            });
        }
    }
    Ok(saida)
}

/// Muda o nível de um chat vinculado. Falha se o chat não está vinculado.
pub async fn definir_nivel(banco: &Pool, chat: i64, nivel: Nivel) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    let n = cliente
        .execute(
            "UPDATE public.canais_destinatarios SET nivel_de_confianca = $3 \
             WHERE canal = $1 AND identificador = $2 AND revogado_em IS NULL",
            &[&CANAL_TELEGRAM, &chat.to_string(), &nivel.codigo()],
        )
        .await?;
    if n == 0 {
        return Err(ErroDeCanal::Entrada(
            "esse chat não está vinculado (use vincular-telegram antes)".into(),
        ));
    }
    Ok(())
}

/// O último estado avisado de cada alerta a este chat.
pub async fn estados_avisados(
    banco: &Pool,
    chat: i64,
) -> Result<HashMap<String, String>, ErroDeCanal> {
    let cliente = banco.get().await?;
    let linhas = cliente
        .query(
            "SELECT chave, estado FROM public.canais_alertas_enviados \
             WHERE canal = $1 AND identificador = $2",
            &[&CANAL_TELEGRAM, &chat.to_string()],
        )
        .await?;
    linhas
        .iter()
        .map(|l| Ok((l.try_get(0)?, l.try_get(1)?)))
        .collect()
}

pub async fn gravar_avisado(
    banco: &Pool,
    dono: Uuid,
    chat: i64,
    chave: &str,
    estado: &str,
) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    cliente
        .execute(
            "INSERT INTO public.canais_alertas_enviados (user_id, canal, identificador, chave, estado) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (canal, identificador, chave) DO UPDATE SET estado = EXCLUDED.estado, \
               enviado_em = now(), user_id = EXCLUDED.user_id",
            &[&dono, &CANAL_TELEGRAM, &chat.to_string(), &chave, &estado],
        )
        .await?;
    Ok(())
}

/// Esquece os alertas que deixaram de valer: se voltarem, avisam de novo.
pub async fn esquecer_ausentes(
    banco: &Pool,
    chat: i64,
    chaves_ativas: &[String],
) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    cliente
        .execute(
            "DELETE FROM public.canais_alertas_enviados \
             WHERE canal = $1 AND identificador = $2 AND NOT (chave = ANY($3))",
            &[&CANAL_TELEGRAM, &chat.to_string(), &chaves_ativas],
        )
        .await?;
    Ok(())
}

/// Registra o que saiu (ADR-015): para quem, de que tipo, quanto e quando. Nunca o texto.
pub async fn registrar_saida(
    banco: &Pool,
    destinatario: &Destinatario,
    tipo: &str,
    tamanho_bytes: usize,
    entregue: bool,
) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    cliente
        .execute(
            "INSERT INTO public.canais_saidas \
             (user_id, canal, identificador, tipo, nivel_de_confianca, tamanho_bytes, entregue) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &destinatario.dono,
                &CANAL_TELEGRAM,
                &destinatario.chat.to_string(),
                &tipo,
                &destinatario.nivel.codigo(),
                &i32::try_from(tamanho_bytes).unwrap_or(i32::MAX),
                &entregue,
            ],
        )
        .await?;
    Ok(())
}

pub async fn ler_estado_do_servico(
    banco: &Pool,
    chave: &str,
) -> Result<Option<String>, ErroDeCanal> {
    let cliente = banco.get().await?;
    let linha = cliente
        .query_opt(
            "SELECT valor FROM public.canais_estado WHERE chave = $1",
            &[&chave],
        )
        .await?;
    linha.map(|l| l.try_get(0)).transpose().map_err(Into::into)
}

pub async fn gravar_estado_do_servico(
    banco: &Pool,
    chave: &str,
    valor: &str,
) -> Result<(), ErroDeCanal> {
    let cliente = banco.get().await?;
    cliente
        .execute(
            "INSERT INTO public.canais_estado (chave, valor) VALUES ($1, $2) \
             ON CONFLICT (chave) DO UPDATE SET valor = EXCLUDED.valor, atualizado_em = now()",
            &[&chave, &valor],
        )
        .await?;
    Ok(())
}

/// As coleções que as regras de alerta leem.
const COLECOES: [Colecao; 9] = [
    Colecao::Contas,
    Colecao::Cartoes,
    Colecao::Faturas,
    Colecao::ItensDeFatura,
    Colecao::Categorias,
    Colecao::Fornecedores,
    Colecao::Movimentacoes,
    Colecao::Recorrentes,
    Colecao::Ocorrencias,
];

/// O livro-caixa do dono, pelas mesmas leituras do servidor (sempre filtradas pelo dono).
pub async fn carregar_livro(banco: &Pool, dono: Uuid) -> Result<LivroCaixa, ErroDeCanal> {
    let cliente = banco.get().await?;
    let dia: Option<i32> = cliente
        .query_opt(
            "SELECT financial_cycle_start_day FROM public.user_financial_preferences \
             WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 1",
            &[&dono],
        )
        .await?
        .map(|l| l.try_get(0))
        .transpose()?
        .flatten();
    let dia = dia
        .and_then(|d| u8::try_from(d).ok())
        .filter(|d| (1..=31).contains(d))
        .unwrap_or(sbf_servidor::DIA_DE_CORTE_PADRAO);
    let mut linhas = Vec::new();
    for colecao in COLECOES {
        linhas.push(
            sbf_servidor::livro::ler_colecao(&**cliente, colecao, dono)
                .await
                .map_err(|e| ErroDeCanal::Livro(e.corpo.mensagem))?,
        );
    }
    livro_das_linhas(dia, &linhas).map_err(|e| ErroDeCanal::Livro(e.to_string()))
}
