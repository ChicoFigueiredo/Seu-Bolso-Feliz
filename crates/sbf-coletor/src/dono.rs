//! Quem é o dono dos dados gravados, para um processo sem sessão (`resolveOwnerId` de
//! `@sbf/db`): o banco é a fonte da verdade (a linha com `role = 'owner'`) e `LOCAL_USER_ID`
//! é só um override **validado** — um valor velho falha na largada em vez de gravar linhas
//! órfãs em nome de quem não existe.

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::erro::ErroDeColeta;

/// Resolve o dono. `substituto` é o valor de `LOCAL_USER_ID`, se houver.
pub async fn resolver_dono<C: GenericClient>(
    cliente: &C,
    substituto: Option<&str>,
) -> Result<Uuid, ErroDeColeta> {
    if let Some(texto) = substituto.map(str::trim).filter(|t| !t.is_empty()) {
        let id = Uuid::try_parse(texto)
            .map_err(|_| ErroDeColeta::Configuracao("LOCAL_USER_ID não é um UUID".to_owned()))?;
        let achou = cliente
            .query_opt("SELECT id FROM public.users WHERE id = $1 LIMIT 1", &[&id])
            .await?;
        return match achou {
            Some(_) => Ok(id),
            None => Err(ErroDeColeta::Configuracao(
                "LOCAL_USER_ID não corresponde a nenhum usuário em public.users \
                 (valor velho, ou aponta para outro ambiente)"
                    .to_owned(),
            )),
        };
    }
    let linha = cliente
        .query_opt(
            "SELECT id FROM public.users WHERE role = 'owner' ORDER BY created_at LIMIT 1",
            &[],
        )
        .await?;
    match linha {
        Some(l) => Ok(l.try_get("id")?),
        None => Err(ErroDeColeta::Configuracao(
            "nenhum usuário com role='owner' em public.users".to_owned(),
        )),
    }
}
