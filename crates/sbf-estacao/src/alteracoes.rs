//! A fila de alterações sobre o cache (grill Q6 e Q7): propor só grava na fila local, com a
//! versão de base lida do cache; a sincronização envia; a resposta do servidor vira Aceita
//! (as linhas novas entram no cache) ou Recusada (lado a lado "Sua versão" × "No servidor
//! agora"); Refazer cria outra sobre a versão do servidor; Descartar encerra.
//!
//! Nesta fatia a única Ação é **mudar a categoria de uma movimentação**.

use crate::cache::{CacheDoLivro, ErroDoCache, RegistroDeAlteracao};
use crate::texto::{data_br, dia_e_mes, dinheiro};
use sbf_contrato::{
    Acao, Categoria, Colecao, ItemAlterado, LinhaAgora, Movimentacao, PropostaDeAlteracao,
    RespostaDaAlteracao, VersaoDeBase,
};
use sbf_inventario::alteracao::{
    Alteracao, CampoLadoALado, ErroDeAlteracao, EstadoDaAlteracao, OpcaoDeCategoria,
    PedidoDeAlteracao, Recusa, ResultadoDeAlteracao,
};
use sbf_inventario::referencia::{Ref, Tipo};
use uuid::Uuid;

/// O texto de uma categoria ausente.
pub const SEM_CATEGORIA: &str = "Sem categoria";

impl From<ErroDoCache> for ErroDeAlteracao {
    fn from(e: ErroDoCache) -> Self {
        ErroDeAlteracao::Local(e.0)
    }
}

fn id_de(alvo: &Ref, tipo: Tipo) -> ResultadoDeAlteracao<Uuid> {
    if alvo.tipo != tipo {
        return Err(ErroDeAlteracao::NaoSuportado(format!(
            "esperava {tipo:?}, veio {:?}",
            alvo.tipo
        )));
    }
    Uuid::parse_str(&alvo.chave).map_err(|_| ErroDeAlteracao::NaoEncontrado(alvo.clone()))
}

fn nome_da_categoria(cache: &CacheDoLivro, id: Option<Uuid>) -> ResultadoDeAlteracao<String> {
    let Some(id) = id else {
        return Ok(SEM_CATEGORIA.to_owned());
    };
    Ok(cache
        .linha::<Categoria>(Colecao::Categorias, &id.to_string())?
        .map_or_else(|| "(categoria desconhecida)".to_owned(), |c| c.nome))
}

/// "Mercado Delta · 04/11".
pub fn rotulo_da_movimentacao(m: &Movimentacao) -> String {
    let descricao = m
        .descricao
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or("Movimentação");
    format!("{descricao} · {}", dia_e_mes(&m.data))
}

/// Grava a alteração na fila, como Aguardando, com a versão de base do cache.
///
/// # Errors
/// Item ou categoria que não existe no cache, nada que mude, ou falha do cache.
pub fn propor(
    cache: &mut CacheDoLivro,
    pedido: &PedidoDeAlteracao,
    estacao: Ref,
    agora: &str,
) -> ResultadoDeAlteracao<Alteracao> {
    let PedidoDeAlteracao::MudarCategoria {
        movimentacao,
        categoria,
    } = pedido;
    let id = id_de(movimentacao, Tipo::Movimentacao)?;
    let atual: Movimentacao = cache
        .linha(Colecao::Movimentacoes, &id.to_string())?
        .ok_or_else(|| ErroDeAlteracao::NaoEncontrado(movimentacao.clone()))?;
    let nova = categoria
        .as_ref()
        .map(|c| id_de(c, Tipo::Categoria))
        .transpose()?;
    if let (Some(c), Some(alvo)) = (nova, categoria.as_ref())
        && cache
            .linha::<Categoria>(Colecao::Categorias, &c.to_string())?
            .is_none()
    {
        return Err(ErroDeAlteracao::NaoEncontrado(alvo.clone()));
    }
    if nova == atual.categoria {
        return Err(ErroDeAlteracao::NadaMuda);
    }
    let alteracao = Alteracao {
        id: Uuid::new_v4().to_string(),
        pedido: pedido.clone(),
        rotulo_do_alvo: rotulo_da_movimentacao(&atual),
        area: "Movimentações".to_owned(),
        antes: nome_da_categoria(cache, atual.categoria)?,
        depois: nome_da_categoria(cache, nova)?,
        feita_em: agora.to_owned(),
        estacao,
        estado: EstadoDaAlteracao::Aguardando,
        versoes_de_base: vec![(movimentacao.clone(), atual.versao.0.clone())],
        recusa: None,
        refeita_em: None,
    };
    let proposta = proposta_de(&alteracao, id, nova, &atual)?;
    cache.gravar_alteracao(&RegistroDeAlteracao {
        alteracao: alteracao.clone(),
        proposta,
        no_servidor_agora: None,
    })?;
    Ok(alteracao)
}

fn proposta_de(
    alteracao: &Alteracao,
    movimentacao: Uuid,
    categoria: Option<Uuid>,
    base: &Movimentacao,
) -> ResultadoDeAlteracao<PropostaDeAlteracao> {
    let id = Uuid::parse_str(&alteracao.id)
        .map_err(|_| ErroDeAlteracao::Local("id de alteração inválido".to_owned()))?;
    Ok(PropostaDeAlteracao {
        id,
        acao: Acao::MudarCategoriaDaMovimentacao {
            movimentacao,
            categoria,
        },
        item: ItemAlterado {
            colecao: Colecao::Movimentacoes,
            id: movimentacao,
        },
        versoes_de_base: vec![VersaoDeBase {
            colecao: Colecao::Movimentacoes,
            id: movimentacao,
            versao: base.versao.clone(),
        }],
        feita_em: alteracao.feita_em.clone(),
    })
}

fn campos_da_movimentacao(
    cache: &CacheDoLivro,
    m: &Movimentacao,
) -> ResultadoDeAlteracao<Vec<(String, String)>> {
    Ok(vec![
        (
            "Descrição".to_owned(),
            m.descricao.clone().unwrap_or_default(),
        ),
        ("Data".to_owned(), data_br(&m.data)),
        ("Valor".to_owned(), dinheiro(m.valor)),
        (
            "Categoria".to_owned(),
            nome_da_categoria(cache, m.categoria)?,
        ),
        (
            "Confirmada".to_owned(),
            if m.confirmada { "Sim" } else { "Não" }.to_owned(),
        ),
    ])
}

/// O lado a lado do item inteiro: a sua versão (a base com a Ação aplicada) e a do servidor
/// agora ("(não existe mais)" se o item sumiu).
fn lado_a_lado(
    cache: &CacheDoLivro,
    proposta: &PropostaDeAlteracao,
    sua_base: Option<&Movimentacao>,
    no_servidor: Option<&Movimentacao>,
) -> ResultadoDeAlteracao<Vec<CampoLadoALado>> {
    let Acao::MudarCategoriaDaMovimentacao { categoria, .. } = &proposta.acao else {
        // A fila antiga só guarda a Ação de categoria.
        return Ok(Vec::new());
    };
    let sua = sua_base.or(no_servidor).map(|m| Movimentacao {
        categoria: *categoria,
        ..m.clone()
    });
    let sua = sua
        .as_ref()
        .map(|m| campos_da_movimentacao(cache, m))
        .transpose()?;
    let servidor = no_servidor
        .map(|m| campos_da_movimentacao(cache, m))
        .transpose()?;
    let rotulos = sua
        .as_ref()
        .or(servidor.as_ref())
        .map(|c| c.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(rotulos
        .iter()
        .enumerate()
        .map(|(i, rotulo)| {
            let valor = |lado: &Option<Vec<(String, String)>>| {
                lado.as_ref().map_or_else(
                    || "(não existe mais)".to_owned(),
                    |c| c.get(i).map(|(_, v)| v.clone()).unwrap_or_default(),
                )
            };
            CampoLadoALado::novo(rotulo.clone(), valor(&sua), valor(&servidor))
        })
        .collect())
}

fn movimentacao_agora(linhas: &[LinhaAgora]) -> Option<&Movimentacao> {
    linhas.iter().find_map(|l| match l {
        LinhaAgora::Movimentacoes(m) => Some(m.as_ref()),
        LinhaAgora::Rateios(_) | LinhaAgora::LotesDeRascunhos(_) | LinhaAgora::Rascunhos(_) => None,
    })
}

/// Aplica a resposta do servidor a uma alteração enviada. Devolve se foi aceita.
///
/// # Errors
/// Falha do cache.
pub fn aplicar_resposta(
    cache: &mut CacheDoLivro,
    mut registro: RegistroDeAlteracao,
    resposta: RespostaDaAlteracao,
    agora: &str,
) -> ResultadoDeAlteracao<bool> {
    match resposta {
        RespostaDaAlteracao::Aceita { linhas, .. } => {
            cache.gravar_linhas(&linhas)?;
            registro.alteracao.estado = EstadoDaAlteracao::Aceita;
            cache.gravar_alteracao(&registro)?;
            Ok(true)
        }
        RespostaDaAlteracao::Recusada {
            mensagem,
            no_servidor_agora,
            ..
        } => {
            let item = registro.proposta.item.id.to_string();
            let base: Option<Movimentacao> = cache.linha(Colecao::Movimentacoes, &item)?;
            let servidor = movimentacao_agora(&no_servidor_agora);
            let campos = lado_a_lado(cache, &registro.proposta, base.as_ref(), servidor)?;
            registro.alteracao.estado = EstadoDaAlteracao::Recusada;
            registro.alteracao.recusa = Some(Recusa {
                motivo: mensagem,
                quando: agora.to_owned(),
                campos,
            });
            registro.no_servidor_agora = Some(no_servidor_agora.clone());
            // O servidor é a autoridade: a cópia passa a ter a linha como ele a tem.
            cache.gravar_linhas(&no_servidor_agora)?;
            cache.gravar_alteracao(&registro)?;
            Ok(false)
        }
    }
}

fn so_recusada(registro: &RegistroDeAlteracao) -> ResultadoDeAlteracao<()> {
    if registro.alteracao.estado == EstadoDaAlteracao::Recusada {
        Ok(())
    } else {
        Err(ErroDeAlteracao::NaoEstaRecusada {
            id: registro.alteracao.id.clone(),
            estado: registro.alteracao.estado,
        })
    }
}

/// "Refazer sobre a nova": a recusada vira Refeita e nasce outra, Aguardando, com o mesmo
/// pedido sobre a versão que o servidor tem agora.
///
/// # Errors
/// Alteração que não existe ou não está recusada, item que sumiu, nada que mude.
pub fn refazer(cache: &mut CacheDoLivro, id: &str, agora: &str) -> ResultadoDeAlteracao<Alteracao> {
    let mut antiga = cache
        .alteracao(id)?
        .ok_or_else(|| ErroDeAlteracao::AlteracaoInexistente(id.to_owned()))?;
    so_recusada(&antiga)?;
    let PedidoDeAlteracao::MudarCategoria {
        movimentacao,
        categoria,
    } = &antiga.alteracao.pedido;
    let mov_id = id_de(movimentacao, Tipo::Movimentacao)?;
    let base: Movimentacao = antiga
        .no_servidor_agora
        .as_deref()
        .and_then(movimentacao_agora)
        .cloned()
        .or(cache.linha(Colecao::Movimentacoes, &mov_id.to_string())?)
        .ok_or_else(|| ErroDeAlteracao::NaoEncontrado(movimentacao.clone()))?;
    let nova_categoria = categoria
        .as_ref()
        .map(|c| id_de(c, Tipo::Categoria))
        .transpose()?;
    if nova_categoria == base.categoria {
        return Err(ErroDeAlteracao::NadaMuda);
    }
    let nova = Alteracao {
        id: Uuid::new_v4().to_string(),
        rotulo_do_alvo: rotulo_da_movimentacao(&base),
        antes: nome_da_categoria(cache, base.categoria)?,
        feita_em: agora.to_owned(),
        estado: EstadoDaAlteracao::Aguardando,
        versoes_de_base: vec![(movimentacao.clone(), base.versao.0.clone())],
        recusa: None,
        refeita_em: None,
        ..antiga.alteracao.clone()
    };
    let proposta = proposta_de(&nova, mov_id, nova_categoria, &base)?;
    antiga.alteracao.estado = EstadoDaAlteracao::Refeita;
    antiga.alteracao.refeita_em = Some(nova.id.clone());
    cache.gravar_alteracoes(&[
        antiga,
        RegistroDeAlteracao {
            alteracao: nova.clone(),
            proposta,
            no_servidor_agora: None,
        },
    ])?;
    Ok(nova)
}

/// "Descartar": a recusada vira Descartada e vale a versão do servidor.
///
/// # Errors
/// Alteração que não existe ou não está recusada.
pub fn descartar(cache: &mut CacheDoLivro, id: &str) -> ResultadoDeAlteracao<()> {
    let mut registro = cache
        .alteracao(id)?
        .ok_or_else(|| ErroDeAlteracao::AlteracaoInexistente(id.to_owned()))?;
    so_recusada(&registro)?;
    registro.alteracao.estado = EstadoDaAlteracao::Descartada;
    cache.gravar_alteracao(&registro)?;
    Ok(())
}

/// As categorias ativas, em ordem de nome; o grupo é o nome da categoria-pai.
///
/// # Errors
/// Falha do cache.
pub fn categorias(cache: &CacheDoLivro) -> ResultadoDeAlteracao<Vec<OpcaoDeCategoria>> {
    let todas: Vec<Categoria> = cache.linhas(Colecao::Categorias)?;
    let mut opcoes: Vec<OpcaoDeCategoria> = todas
        .iter()
        .filter(|c| c.ativa)
        .map(|c| OpcaoDeCategoria {
            alvo: Ref::nova(Tipo::Categoria, c.id.to_string()),
            nome: c.nome.clone(),
            grupo: c
                .pai
                .and_then(|p| todas.iter().find(|x| x.id == p))
                .map(|p| p.nome.clone()),
        })
        .collect();
    opcoes.sort_by_key(|a| a.nome.to_lowercase());
    Ok(opcoes)
}
