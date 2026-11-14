//! As escritas do livro-caixa feitas nesta estação (spec 08): a fila de [`Acao`]s do contrato
//! v2 que sobrevive a fechar o app, a **aplicação otimista** no cache (a linha mostra o que o
//! usuário fez, marcada como pendente até o aceite), o aceite e a **guarda das recusas** ("Sua
//! versão" × "No servidor agora"), Refazer e Descartar.
//!
//! Tudo aqui roda dentro de uma transação do `livro-caixa.sqlite`: enfileirar grava a escrita
//! **e** o efeito otimista juntos, então um app que fecha no meio nunca deixa um efeito sem
//! fila nem uma fila sem efeito.
//!
//! A fila nova convive com a antiga ([`crate::alteracoes`], só mudar a categoria, que a janela
//! da fatia 2 usa): são duas tabelas, e a sincronização envia as duas, a antiga primeiro.

use std::collections::HashSet;

use rusqlite::{OptionalExtension, Transaction, params};
use sbf_contrato::{
    Acao, Categoria, Centavos, Centro, CodigoDeErro, Colecao, Conta, EstadoDaConciliacao,
    EstadoDoRascunho, Id, ItemAlterado, LinhaAgora, LoteDeRascunhos, Movimentacao,
    PropostaDeAlteracao, Rascunho, Rateio, VersaoDeBase,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cache::{CacheDoLivro, ErroDoCache, linha_agora};

/// Onde uma escrita está.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaEscrita {
    /// Feita aqui, ainda não aceita pelo servidor. Vai na próxima sincronização, em ordem.
    Aguardando,
    /// O servidor aplicou. Fica no histórico.
    Aceita,
    /// O servidor não aplicou. Fica visível até Refazer ou Descartar.
    Recusada,
    /// Recusada e refeita sobre a versão nova (a refeita é outra escrita).
    Refeita,
    /// Recusada e descartada: vale a versão do servidor.
    Descartada,
}

impl EstadoDaEscrita {
    /// Ainda pede atenção (Aguardando ou Recusada).
    pub fn em_aberto(self) -> bool {
        matches!(self, Self::Aguardando | Self::Recusada)
    }
}

/// Por que o servidor recusou, com as duas versões lado a lado: "Sua versão" é
/// [`Escrita::acao`] (e `proposta`, com as versões de base); "No servidor agora" é
/// `no_servidor_agora` (vazio: a linha nem existe mais).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecusaDaEscrita {
    pub motivo: CodigoDeErro,
    pub mensagem: String,
    /// Instante ISO 8601 da recusa.
    pub quando: String,
    pub no_servidor_agora: Vec<LinhaAgora>,
}

/// Uma escrita na fila.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Escrita {
    /// O id da proposta (o mesmo em todo reenvio).
    pub id: Id,
    pub acao: Acao,
    /// "Aprovar rascunho · Mercado Delta": o que a lista mostra.
    pub rotulo: String,
    /// Instante ISO 8601.
    pub feita_em: String,
    pub estado: EstadoDaEscrita,
    pub recusa: Option<RecusaDaEscrita>,
    /// Refeita: a escrita nova.
    pub refeita_em: Option<Id>,
    pub proposta: PropostaDeAlteracao,
}

/// O que pode dar errado ao escrever.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ErroDeEscrita {
    #[error("não está na cópia local: {0}")]
    NaoEncontrado(String),
    #[error("a escrita {0} não existe")]
    Inexistente(Id),
    #[error("a escrita {0} não está recusada: só uma recusada pode ser refeita ou descartada")]
    NaoEstaRecusada(Id),
    #[error("nada muda: o item já está assim")]
    NadaMuda,
    #[error("a soma dos rateios ({soma}) tem de ser igual ao valor da movimentação ({valor})")]
    SomaDoRateioNaoBate { soma: Centavos, valor: Centavos },
    #[error("pedido inválido: {0}")]
    Invalido(String),
    #[error("o servidor tem um contrato mais novo: atualize o app para escrever")]
    ContratoMaisNovo,
    #[error("esta estação foi revogada")]
    EstacaoRevogada,
    #[error("erro local: {0}")]
    Local(String),
}

impl From<ErroDoCache> for ErroDeEscrita {
    fn from(e: ErroDoCache) -> Self {
        Self::Local(e.0)
    }
}

impl From<rusqlite::Error> for ErroDeEscrita {
    fn from(e: rusqlite::Error) -> Self {
        Self::Local(e.to_string())
    }
}

impl From<serde_json::Error> for ErroDeEscrita {
    fn from(e: serde_json::Error) -> Self {
        Self::Local(format!("JSON: {e}"))
    }
}

pub type ResultadoDeEscrita<T> = Result<T, ErroDeEscrita>;

// ───────────────────────────── a API para a janela ─────────────────────────────

/// A caixa de entrada: o que o cache sabe dos rascunhos do pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDaCaixa {
    pub rascunho: Rascunho,
    /// Há uma escrita Aguardando sobre ele (marca visual de pendente).
    pub pendente: bool,
    /// Há uma escrita Recusada em aberto sobre ele.
    pub recusado: bool,
}

/// Os rascunhos de um lote (ou os sem lote, com `lote: None`).
#[derive(Debug, Clone, PartialEq)]
pub struct LoteNaCaixa {
    pub lote: Option<LoteDeRascunhos>,
    pub itens: Vec<ItemDaCaixa>,
}

/// A caixa de entrada e as escritas, para a janela. Implementado por [`crate::EstacaoLocal`].
pub trait Escritas {
    /// A caixa de entrada agrupada por lote, o mais novo primeiro. `so_a_decidir`: só os
    /// rascunhos que ainda esperam uma decisão (pendente de revisão, corrigido, aprovado e
    /// ainda não lançado).
    ///
    /// # Errors
    /// Falha do cache.
    fn caixa_de_entrada(&self, so_a_decidir: bool) -> ResultadoDeEscrita<Vec<LoteNaCaixa>>;

    /// Põe a Ação na fila, grava a base lida do cache e já mostra o efeito no cache, marcado
    /// pendente. Não fala com a rede: vai na próxima sincronização.
    ///
    /// # Errors
    /// [`ErroDeEscrita`]: item fora do cache, nada que mude, soma de rateio que não bate…
    fn enfileirar(&mut self, acao: Acao) -> ResultadoDeEscrita<Escrita>;

    /// As escritas, da mais antiga para a mais nova. `so_em_aberto`: Aguardando e Recusada.
    ///
    /// # Errors
    /// Falha do cache.
    fn escritas(&self, so_em_aberto: bool) -> ResultadoDeEscrita<Vec<Escrita>>;

    /// As recusadas em aberto, com "Sua versão" × "No servidor agora".
    ///
    /// # Errors
    /// Falha do cache.
    fn recusas(&self) -> ResultadoDeEscrita<Vec<Escrita>> {
        Ok(self
            .escritas(true)?
            .into_iter()
            .filter(|e| e.estado == EstadoDaEscrita::Recusada)
            .collect())
    }

    /// Os itens com escrita Aguardando: a janela os marca como pendentes.
    ///
    /// # Errors
    /// Falha do cache.
    fn pendentes(&self) -> ResultadoDeEscrita<HashSet<(Colecao, Id)>>;

    /// "Refazer sobre a nova": a recusada vira Refeita e nasce outra, Aguardando, com a mesma
    /// Ação sobre a versão que o servidor tem agora.
    ///
    /// # Errors
    /// [`ErroDeEscrita`].
    fn refazer_escrita(&mut self, id: Id) -> ResultadoDeEscrita<Escrita>;

    /// "Descartar": a recusada vira Descartada e vale a versão do servidor.
    ///
    /// # Errors
    /// [`ErroDeEscrita`].
    fn descartar_escrita(&mut self, id: Id) -> ResultadoDeEscrita<()>;
}

// ───────────────────────────── leitura do cache na transação ─────────────────────────────

fn ler<T: DeserializeOwned>(
    tx: &Transaction<'_>,
    colecao: Colecao,
    id: Id,
) -> ResultadoDeEscrita<Option<T>> {
    let texto: Option<String> = tx
        .query_row(
            "SELECT dados FROM linha WHERE colecao = ?1 AND id = ?2",
            params![colecao.caminho(), id.to_string()],
            |l| l.get(0),
        )
        .optional()?;
    Ok(texto.map(|t| serde_json::from_str(&t)).transpose()?)
}

fn ler_todas<T: DeserializeOwned>(
    tx: &Transaction<'_>,
    colecao: Colecao,
) -> ResultadoDeEscrita<Vec<T>> {
    let mut st = tx.prepare_cached("SELECT dados FROM linha WHERE colecao = ?1 ORDER BY id")?;
    let textos = st.query_map([colecao.caminho()], |l| l.get::<_, String>(0))?;
    let mut saida = Vec::new();
    for t in textos {
        saida.push(serde_json::from_str(&t?)?);
    }
    Ok(saida)
}

fn gravar_linhas(tx: &Transaction<'_>, linhas: &[LinhaAgora]) -> ResultadoDeEscrita<()> {
    for linha in linhas {
        let (colecao, id, versao, dados) = linha_agora(linha)?;
        tx.execute(
            "INSERT INTO linha (colecao, id, versao, dados) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (colecao, id) DO UPDATE SET versao = excluded.versao, dados = excluded.dados",
            params![colecao.caminho(), id, versao, dados],
        )?;
    }
    Ok(())
}

fn apagar(tx: &Transaction<'_>, itens: &[ItemAlterado]) -> ResultadoDeEscrita<()> {
    for i in itens {
        tx.execute(
            "DELETE FROM linha WHERE colecao = ?1 AND id = ?2",
            params![i.colecao.caminho(), i.id.to_string()],
        )?;
    }
    Ok(())
}

fn rateios_da(tx: &Transaction<'_>, movimentacao: Id) -> ResultadoDeEscrita<Vec<Rateio>> {
    Ok(ler_todas::<Rateio>(tx, Colecao::Rateios)?
        .into_iter()
        .filter(|r| r.movimentacao == movimentacao)
        .collect())
}

fn itens_dos_rateios(rateios: &[Rateio]) -> Vec<ItemAlterado> {
    rateios
        .iter()
        .map(|r| ItemAlterado {
            colecao: Colecao::Rateios,
            id: r.id,
        })
        .collect()
}

fn versao_da(linha: &LinhaAgora) -> (ItemAlterado, Option<String>) {
    match linha {
        LinhaAgora::Movimentacoes(m) => {
            (item(Colecao::Movimentacoes, m.id), Some(m.versao.0.clone()))
        }
        LinhaAgora::Rateios(r) => (item(Colecao::Rateios, r.id), None),
        LinhaAgora::LotesDeRascunhos(l) => (
            item(Colecao::LotesDeRascunhos, l.id),
            Some(l.versao.0.clone()),
        ),
        LinhaAgora::Rascunhos(r) => (item(Colecao::Rascunhos, r.id), Some(r.versao.0.clone())),
    }
}

fn item(colecao: Colecao, id: Id) -> ItemAlterado {
    ItemAlterado { colecao, id }
}

fn versao_no_cache(tx: &Transaction<'_>, i: &ItemAlterado) -> ResultadoDeEscrita<Option<String>> {
    let v: Option<Option<String>> = tx
        .query_row(
            "SELECT versao FROM linha WHERE colecao = ?1 AND id = ?2",
            params![i.colecao.caminho(), i.id.to_string()],
            |l| l.get(0),
        )
        .optional()?;
    Ok(v.flatten())
}

// ───────────────────────────── a fila (tabela `escrita`) ─────────────────────────────

fn gravar_escrita(tx: &Transaction<'_>, e: &Escrita) -> ResultadoDeEscrita<()> {
    let estado = serde_json::to_value(e.estado)?;
    tx.execute(
        "INSERT INTO escrita (id, estado, feita_em, dados) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (id) DO UPDATE SET estado = excluded.estado, dados = excluded.dados",
        params![
            e.id.to_string(),
            estado.as_str().unwrap_or_default(),
            e.feita_em,
            serde_json::to_string(e)?
        ],
    )?;
    Ok(())
}

fn todas_as_escritas(tx: &Transaction<'_>) -> ResultadoDeEscrita<Vec<Escrita>> {
    let mut st = tx.prepare_cached("SELECT dados FROM escrita ORDER BY rowid")?;
    let textos = st.query_map([], |l| l.get::<_, String>(0))?;
    let mut saida = Vec::new();
    for t in textos {
        saida.push(serde_json::from_str(&t?)?);
    }
    Ok(saida)
}

fn uma_escrita(tx: &Transaction<'_>, id: Id) -> ResultadoDeEscrita<Option<Escrita>> {
    let t: Option<String> = tx
        .query_row(
            "SELECT dados FROM escrita WHERE id = ?1",
            [id.to_string()],
            |l| l.get(0),
        )
        .optional()?;
    Ok(t.map(|t| serde_json::from_str(&t)).transpose()?)
}

// ───────────────────────────── montar a proposta ─────────────────────────────

fn data_valida(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
        && matches!(d[5..7].parse::<u8>(), Ok(1..=12))
        && matches!(d[8..10].parse::<u8>(), Ok(1..=31))
}

fn nao_achou(o_que: &str, id: Id) -> ErroDeEscrita {
    ErroDeEscrita::NaoEncontrado(format!("{o_que} {id}"))
}

fn movimentacao_do_cache(tx: &Transaction<'_>, id: Id) -> ResultadoDeEscrita<Movimentacao> {
    ler(tx, Colecao::Movimentacoes, id)?.ok_or_else(|| nao_achou("movimentação", id))
}

fn rascunho_do_cache(tx: &Transaction<'_>, id: Id) -> ResultadoDeEscrita<Rascunho> {
    ler(tx, Colecao::Rascunhos, id)?.ok_or_else(|| nao_achou("rascunho", id))
}

fn a_decidir(r: &Rascunho) -> bool {
    r.conciliacao != EstadoDaConciliacao::ConfirmadoDuplicata
        && (r.estado.pendente()
            || (r.estado == EstadoDoRascunho::Aprovado && r.lancado_como.is_none()))
}

fn base_de(colecao: Colecao, id: Id, versao: &str) -> VersaoDeBase {
    VersaoDeBase {
        colecao,
        id,
        versao: sbf_contrato::Versao(versao.to_owned()),
    }
}

fn texto_da_movimentacao(m: &Movimentacao) -> String {
    m.descricao
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or("Movimentação")
        .to_owned()
}

fn texto_do_rascunho(r: &Rascunho) -> String {
    r.dados
        .get("description")
        .and_then(|d| d.as_str())
        .or_else(|| r.dados.get("supplier_name_raw").and_then(|d| d.as_str()))
        .filter(|d| !d.trim().is_empty())
        .unwrap_or("Rascunho")
        .to_owned()
}

/// A proposta de uma Ação, com a versão de base de cada linha tocada lida do cache, e o
/// rótulo. Confere o que dá para conferir sem o servidor.
fn montar(
    tx: &Transaction<'_>,
    id: Id,
    acao: &Acao,
    agora: &str,
) -> ResultadoDeEscrita<(PropostaDeAlteracao, String)> {
    let mov_c = Colecao::Movimentacoes;
    let rasc_c = Colecao::Rascunhos;
    let (item_alterado, bases, rotulo) = match acao {
        Acao::MudarCategoriaDaMovimentacao {
            movimentacao,
            categoria,
        } => {
            let m = movimentacao_do_cache(tx, *movimentacao)?;
            if m.categoria == *categoria {
                return Err(ErroDeEscrita::NadaMuda);
            }
            if let Some(c) = categoria
                && ler::<Categoria>(tx, Colecao::Categorias, *c)?.is_none()
            {
                return Err(nao_achou("categoria", *c));
            }
            (
                item(mov_c, m.id),
                vec![base_de(mov_c, m.id, &m.versao.0)],
                format!("Categoria · {}", texto_da_movimentacao(&m)),
            )
        }
        Acao::EditarMovimentacao {
            movimentacao,
            edicao,
        } => {
            let m = movimentacao_do_cache(tx, *movimentacao)?;
            if !data_valida(&edicao.data) {
                return Err(ErroDeEscrita::Invalido(
                    "a data precisa estar em AAAA-MM-DD".into(),
                ));
            }
            if edicao.valor <= 0 {
                return Err(ErroDeEscrita::Invalido(
                    "o valor precisa ser maior que zero".into(),
                ));
            }
            if ler::<Conta>(tx, Colecao::Contas, edicao.conta)?.is_none() {
                return Err(nao_achou("conta", edicao.conta));
            }
            if let Some(c) = edicao.centro
                && ler::<Centro>(tx, Colecao::Centros, c)?.is_none()
            {
                return Err(nao_achou("centro", c));
            }
            (
                item(mov_c, m.id),
                vec![base_de(mov_c, m.id, &m.versao.0)],
                format!("Editar · {}", texto_da_movimentacao(&m)),
            )
        }
        Acao::ExcluirMovimentacao { movimentacao } => {
            let m = movimentacao_do_cache(tx, *movimentacao)?;
            (
                item(mov_c, m.id),
                vec![base_de(mov_c, m.id, &m.versao.0)],
                format!("Excluir · {}", texto_da_movimentacao(&m)),
            )
        }
        Acao::RatearMovimentacao {
            movimentacao,
            rateios,
        } => {
            let m = movimentacao_do_cache(tx, *movimentacao)?;
            let soma: Centavos = rateios.iter().map(|r| r.valor).sum();
            if rateios.is_empty() || rateios.iter().any(|r| r.valor <= 0) {
                return Err(ErroDeEscrita::Invalido(
                    "o rateio precisa de ao menos um centro, cada um com valor maior que zero"
                        .into(),
                ));
            }
            if soma != m.valor.abs() {
                return Err(ErroDeEscrita::SomaDoRateioNaoBate {
                    soma,
                    valor: m.valor.abs(),
                });
            }
            for r in rateios {
                if ler::<Centro>(tx, Colecao::Centros, r.centro)?.is_none() {
                    return Err(nao_achou("centro", r.centro));
                }
            }
            (
                item(mov_c, m.id),
                vec![base_de(mov_c, m.id, &m.versao.0)],
                format!("Ratear · {}", texto_da_movimentacao(&m)),
            )
        }
        Acao::CorrigirRascunho { rascunho, dados } => {
            let r = rascunho_do_cache(tx, *rascunho)?;
            if !dados.is_object() {
                return Err(ErroDeEscrita::Invalido(
                    "os dados do rascunho precisam ser um objeto".into(),
                ));
            }
            if !a_decidir(&r) {
                return Err(ErroDeEscrita::Invalido("o rascunho já foi decidido".into()));
            }
            (
                item(rasc_c, r.id),
                vec![base_de(rasc_c, r.id, &r.versao.0)],
                format!("Corrigir rascunho · {}", texto_do_rascunho(&r)),
            )
        }
        Acao::AprovarRascunho { rascunho } | Acao::RecusarRascunho { rascunho, .. } => {
            let r = rascunho_do_cache(tx, *rascunho)?;
            if !a_decidir(&r) {
                return Err(ErroDeEscrita::Invalido("o rascunho já foi decidido".into()));
            }
            let verbo = if matches!(acao, Acao::AprovarRascunho { .. }) {
                "Aprovar"
            } else {
                "Recusar"
            };
            (
                item(rasc_c, r.id),
                vec![base_de(rasc_c, r.id, &r.versao.0)],
                format!("{verbo} rascunho · {}", texto_do_rascunho(&r)),
            )
        }
        Acao::AprovarLoteDeRascunhos { lote } => {
            let l: LoteDeRascunhos = ler(tx, Colecao::LotesDeRascunhos, *lote)?
                .ok_or_else(|| nao_achou("lote", *lote))?;
            let mut bases = vec![base_de(Colecao::LotesDeRascunhos, l.id, &l.versao.0)];
            let mut quantos = 0;
            for r in ler_todas::<Rascunho>(tx, rasc_c)? {
                if r.lote == Some(l.id) && a_decidir(&r) {
                    bases.push(base_de(rasc_c, r.id, &r.versao.0));
                    quantos += 1;
                }
            }
            if quantos == 0 {
                return Err(ErroDeEscrita::NadaMuda);
            }
            (
                item(Colecao::LotesDeRascunhos, l.id),
                bases,
                format!(
                    "Aprovar lote · {} ({quantos} rascunhos)",
                    l.nome.as_deref().unwrap_or("Lote")
                ),
            )
        }
        Acao::ConciliarRascunho {
            rascunho,
            movimentacao,
        } => {
            let r = rascunho_do_cache(tx, *rascunho)?;
            let m = movimentacao_do_cache(tx, *movimentacao)?;
            (
                item(rasc_c, r.id),
                vec![
                    base_de(rasc_c, r.id, &r.versao.0),
                    base_de(mov_c, m.id, &m.versao.0),
                ],
                format!("Conciliar rascunho · {}", texto_do_rascunho(&r)),
            )
        }
    };
    Ok((
        PropostaDeAlteracao {
            id,
            acao: acao.clone(),
            item: item_alterado,
            versoes_de_base: bases,
            feita_em: agora.to_owned(),
        },
        rotulo,
    ))
}

// ───────────────────────────── aplicação otimista ─────────────────────────────

/// Mostra no cache o que a Ação faz. Não mexe na versão das linhas (ela continua sendo a do
/// servidor): é o que a guarda das recusas compara.
fn aplicar_otimista(tx: &Transaction<'_>, p: &PropostaDeAlteracao) -> ResultadoDeEscrita<()> {
    let mut linhas: Vec<LinhaAgora> = Vec::new();
    let mut apagados: Vec<ItemAlterado> = Vec::new();
    match &p.acao {
        Acao::MudarCategoriaDaMovimentacao {
            movimentacao,
            categoria,
        } => {
            if let Some(mut m) = ler::<Movimentacao>(tx, Colecao::Movimentacoes, *movimentacao)? {
                m.categoria = *categoria;
                linhas.push(LinhaAgora::Movimentacoes(Box::new(m)));
            }
        }
        Acao::EditarMovimentacao {
            movimentacao,
            edicao,
        } => {
            if let Some(mut m) = ler::<Movimentacao>(tx, Colecao::Movimentacoes, *movimentacao)? {
                m.descricao.clone_from(&edicao.descricao);
                m.data.clone_from(&edicao.data);
                m.valor = edicao.valor;
                m.conta = edicao.conta;
                m.fornecedor = edicao.fornecedor;
                m.notas.clone_from(&edicao.notas);
                linhas.push(LinhaAgora::Movimentacoes(Box::new(m)));
                if let Some(centro) = edicao.centro {
                    apagados.extend(itens_dos_rateios(&rateios_da(tx, *movimentacao)?));
                    linhas.push(LinhaAgora::Rateios(Rateio {
                        id: Uuid::new_v4(),
                        movimentacao: *movimentacao,
                        centro,
                        valor: edicao.valor,
                        nota: None,
                        criado_em: p.feita_em.clone(),
                    }));
                }
            }
        }
        Acao::ExcluirMovimentacao { movimentacao } => {
            apagados.extend(itens_dos_rateios(&rateios_da(tx, *movimentacao)?));
            apagados.push(item(Colecao::Movimentacoes, *movimentacao));
        }
        Acao::RatearMovimentacao {
            movimentacao,
            rateios,
        } => {
            apagados.extend(itens_dos_rateios(&rateios_da(tx, *movimentacao)?));
            for r in rateios {
                linhas.push(LinhaAgora::Rateios(Rateio {
                    id: r.id,
                    movimentacao: *movimentacao,
                    centro: r.centro,
                    valor: r.valor,
                    nota: r.nota.clone(),
                    criado_em: p.feita_em.clone(),
                }));
            }
        }
        Acao::CorrigirRascunho { rascunho, dados } => {
            if let Some(mut r) = ler::<Rascunho>(tx, Colecao::Rascunhos, *rascunho)? {
                r.dados.clone_from(dados);
                r.estado = EstadoDoRascunho::Corrigido;
                linhas.push(LinhaAgora::Rascunhos(Box::new(r)));
            }
        }
        Acao::AprovarRascunho { rascunho } => {
            if let Some(mut r) = ler::<Rascunho>(tx, Colecao::Rascunhos, *rascunho)? {
                r.estado = EstadoDoRascunho::Aprovado;
                linhas.push(LinhaAgora::Rascunhos(Box::new(r)));
            }
        }
        Acao::RecusarRascunho { rascunho, motivo } => {
            if let Some(mut r) = ler::<Rascunho>(tx, Colecao::Rascunhos, *rascunho)? {
                r.estado = EstadoDoRascunho::Recusado;
                r.correcoes = Some(serde_json::json!({ "rejection_reason": motivo }));
                linhas.push(LinhaAgora::Rascunhos(Box::new(r)));
            }
        }
        Acao::AprovarLoteDeRascunhos { lote } => {
            for mut r in ler_todas::<Rascunho>(tx, Colecao::Rascunhos)? {
                if r.lote == Some(*lote) && a_decidir(&r) {
                    r.estado = EstadoDoRascunho::Aprovado;
                    linhas.push(LinhaAgora::Rascunhos(Box::new(r)));
                }
            }
        }
        Acao::ConciliarRascunho {
            rascunho,
            movimentacao,
        } => {
            if let Some(mut r) = ler::<Rascunho>(tx, Colecao::Rascunhos, *rascunho)? {
                r.conciliacao = EstadoDaConciliacao::ConfirmadoDuplicata;
                r.estado = EstadoDoRascunho::Recusado;
                r.movimentacao_conciliada = Some(*movimentacao);
                linhas.push(LinhaAgora::Rascunhos(Box::new(r)));
            }
        }
    }
    // O que a Ação cria (rateios novos) não é apagado por ela mesma.
    apagados.retain(|a| {
        !linhas
            .iter()
            .any(|l| versao_da(l).0 == *a && a.colecao == Colecao::Rateios)
    });
    apagar(tx, &apagados)?;
    gravar_linhas(tx, &linhas)
}

/// Depois de uma troca ou de uma resposta: mostra de novo o efeito de cada escrita ainda
/// Aguardando, por cima do que o servidor mandou.
fn reaplicar_pendentes(tx: &Transaction<'_>) -> ResultadoDeEscrita<()> {
    for e in todas_as_escritas(tx)? {
        if e.estado == EstadoDaEscrita::Aguardando {
            aplicar_otimista(tx, &e.proposta)?;
        }
    }
    Ok(())
}

// ───────────────────────────── a resposta do servidor ─────────────────────────────

/// A escrita foi aceita: o cache passa a ter as linhas como o servidor as devolveu, as
/// escritas seguintes que partiram da mesma versão passam a partir da nova, e o efeito
/// otimista dos pendentes é refeito por cima.
fn aceitar(
    tx: &Transaction<'_>,
    mut e: Escrita,
    linhas: &[LinhaAgora],
    excluidas: &[ItemAlterado],
) -> ResultadoDeEscrita<()> {
    let itens: Vec<ItemAlterado> = linhas.iter().map(|l| versao_da(l).0).collect();
    let mut antes = Vec::with_capacity(itens.len());
    for i in &itens {
        antes.push((i.clone(), versao_no_cache(tx, i)?));
    }
    // O que o servidor devolve no lugar dos rateios substitui o efeito otimista deles.
    let mov_rateada = match &e.acao {
        Acao::EditarMovimentacao {
            movimentacao,
            edicao,
        } if edicao.centro.is_some() => Some(*movimentacao),
        Acao::RatearMovimentacao { movimentacao, .. } => Some(*movimentacao),
        _ => None,
    };
    if let Some(m) = mov_rateada {
        apagar(tx, &itens_dos_rateios(&rateios_da(tx, m)?))?;
    }
    apagar(tx, excluidas)?;
    gravar_linhas(tx, linhas)?;
    rebasear(tx, e.id, &antes, linhas)?;
    e.estado = EstadoDaEscrita::Aceita;
    e.recusa = None;
    gravar_escrita(tx, &e)?;
    reaplicar_pendentes(tx)
}

/// As escritas seguintes que partiram da versão que acabou de mudar partem da nova: elas
/// foram feitas em cima do efeito da anterior, não de uma edição de outra estação.
fn rebasear(
    tx: &Transaction<'_>,
    aceita: Id,
    antes: &[(ItemAlterado, Option<String>)],
    linhas: &[LinhaAgora],
) -> ResultadoDeEscrita<()> {
    let novas: Vec<(ItemAlterado, Option<String>)> = linhas.iter().map(versao_da).collect();
    for mut e in todas_as_escritas(tx)? {
        if e.id == aceita || e.estado != EstadoDaEscrita::Aguardando {
            continue;
        }
        let mut mudou = false;
        for base in &mut e.proposta.versoes_de_base {
            let alvo = item(base.colecao, base.id);
            let velha = antes
                .iter()
                .find(|(i, _)| *i == alvo)
                .and_then(|(_, v)| v.as_deref());
            let nova = novas
                .iter()
                .find(|(i, _)| *i == alvo)
                .and_then(|(_, v)| v.as_deref());
            if let (Some(velha), Some(nova)) = (velha, nova)
                && base.versao.0 == velha
                && velha != nova
            {
                base.versao = sbf_contrato::Versao(nova.to_owned());
                mudou = true;
            }
        }
        // Um lote aprovado depois de um rascunho já lançado não lista mais esse rascunho.
        if matches!(e.acao, Acao::AprovarLoteDeRascunhos { .. }) {
            let fora: Vec<Id> = linhas
                .iter()
                .filter_map(|l| match l {
                    LinhaAgora::Rascunhos(r) if !a_decidir(r) => Some(r.id),
                    _ => None,
                })
                .collect();
            let antes_len = e.proposta.versoes_de_base.len();
            e.proposta
                .versoes_de_base
                .retain(|b| !(b.colecao == Colecao::Rascunhos && fora.contains(&b.id)));
            mudou |= e.proposta.versoes_de_base.len() != antes_len;
        }
        if mudou {
            gravar_escrita(tx, &e)?;
        }
    }
    Ok(())
}

/// A escrita foi recusada: o cache passa a ter o que o servidor tem agora (e perde o que o
/// servidor não tem), e a recusa fica guardada para a área Sincronização.
fn recusar(
    tx: &Transaction<'_>,
    mut e: Escrita,
    motivo: CodigoDeErro,
    mensagem: &str,
    no_servidor_agora: Vec<LinhaAgora>,
    quando: &str,
) -> ResultadoDeEscrita<()> {
    let presentes: Vec<ItemAlterado> = no_servidor_agora.iter().map(|l| versao_da(l).0).collect();
    let mut sumiram: Vec<ItemAlterado> = Vec::new();
    for base in &e.proposta.versoes_de_base {
        let alvo = item(base.colecao, base.id);
        if !presentes.contains(&alvo) {
            if base.colecao == Colecao::Movimentacoes {
                sumiram.extend(itens_dos_rateios(&rateios_da(tx, base.id)?));
            }
            sumiram.push(alvo);
        } else if base.colecao == Colecao::Movimentacoes {
            // Os rateios que o servidor mandou substituem os otimistas.
            sumiram.extend(itens_dos_rateios(&rateios_da(tx, base.id)?));
        }
    }
    apagar(tx, &sumiram)?;
    gravar_linhas(tx, &no_servidor_agora)?;
    e.estado = EstadoDaEscrita::Recusada;
    e.recusa = Some(RecusaDaEscrita {
        motivo,
        mensagem: mensagem.to_owned(),
        quando: quando.to_owned(),
        no_servidor_agora,
    });
    gravar_escrita(tx, &e)?;
    reaplicar_pendentes(tx)
}

// ───────────────────────────── o que a sincronização usa ─────────────────────────────

impl CacheDoLivro {
    /// As escritas, da mais antiga para a mais nova.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn escritas(&self) -> Result<Vec<Escrita>, ErroDoCache> {
        let mut st = self
            .ler()
            .prepare("SELECT dados FROM escrita ORDER BY rowid")?;
        let textos = st.query_map([], |l| l.get::<_, String>(0))?;
        let mut saida = Vec::new();
        for t in textos {
            saida.push(serde_json::from_str(&t?)?);
        }
        Ok(saida)
    }

    /// Uma escrita pelo id.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn escrita(&self, id: Id) -> Result<Option<Escrita>, ErroDoCache> {
        Ok(self.escritas()?.into_iter().find(|e| e.id == id))
    }

    /// Põe a Ação na fila e mostra o efeito no cache, numa transação só.
    ///
    /// # Errors
    /// [`ErroDeEscrita`]; nada é gravado.
    pub fn enfileirar(&mut self, acao: Acao, agora: &str) -> ResultadoDeEscrita<Escrita> {
        let id = Uuid::new_v4();
        self.armazem.transacao_imediata(
            |tx| -> Result<ResultadoDeEscrita<Escrita>, ErroDoCache> {
                Ok((|| {
                    let (proposta, rotulo) = montar(tx, id, &acao, agora)?;
                    aplicar_otimista(tx, &proposta)?;
                    let e = Escrita {
                        id,
                        acao: acao.clone(),
                        rotulo,
                        feita_em: agora.to_owned(),
                        estado: EstadoDaEscrita::Aguardando,
                        recusa: None,
                        refeita_em: None,
                        proposta,
                    };
                    gravar_escrita(tx, &e)?;
                    Ok(e)
                })())
            },
        )?
    }

    /// Aplica a resposta do servidor a uma escrita enviada. Devolve se foi aceita.
    ///
    /// # Errors
    /// Falha do cache.
    pub fn aplicar_resposta_da_escrita(
        &mut self,
        id: Id,
        resposta: sbf_contrato::RespostaDaAlteracao,
        agora: &str,
    ) -> ResultadoDeEscrita<bool> {
        self.armazem
            .transacao_imediata(|tx| -> Result<ResultadoDeEscrita<bool>, ErroDoCache> {
                Ok((|| {
                    let e = uma_escrita(tx, id)?.ok_or(ErroDeEscrita::Inexistente(id))?;
                    match resposta {
                        sbf_contrato::RespostaDaAlteracao::Aceita {
                            linhas, excluidas, ..
                        } => {
                            aceitar(tx, e, &linhas, &excluidas)?;
                            Ok(true)
                        }
                        sbf_contrato::RespostaDaAlteracao::Recusada {
                            motivo,
                            mensagem,
                            no_servidor_agora,
                            ..
                        } => {
                            recusar(tx, e, motivo, &mensagem, no_servidor_agora, agora)?;
                            Ok(false)
                        }
                    }
                })())
            })?
    }

    /// O servidor disse que a proposta é inválida (HTTP 422): vira Recusada aqui mesmo, sem
    /// linhas do servidor, e as coleções tocadas voltam a ser baixadas inteiras (o efeito
    /// otimista some).
    ///
    /// # Errors
    /// Falha do cache.
    pub fn recusar_localmente(
        &mut self,
        id: Id,
        mensagem: &str,
        agora: &str,
    ) -> ResultadoDeEscrita<()> {
        self.armazem
            .transacao_imediata(|tx| -> Result<ResultadoDeEscrita<()>, ErroDoCache> {
                Ok((|| {
                    let mut e = uma_escrita(tx, id)?.ok_or(ErroDeEscrita::Inexistente(id))?;
                    e.estado = EstadoDaEscrita::Recusada;
                    e.recusa = Some(RecusaDaEscrita {
                        motivo: CodigoDeErro::PedidoInvalido,
                        mensagem: mensagem.to_owned(),
                        quando: agora.to_owned(),
                        no_servidor_agora: vec![],
                    });
                    gravar_escrita(tx, &e)?;
                    for c in [
                        Colecao::Movimentacoes,
                        Colecao::Rateios,
                        Colecao::Rascunhos,
                        Colecao::LotesDeRascunhos,
                    ] {
                        tx.execute(
                            "UPDATE colecao SET cursor = NULL, etag = NULL WHERE nome = ?1",
                            [c.caminho()],
                        )?;
                    }
                    Ok(())
                })())
            })?
    }

    /// Mostra de novo o efeito das escritas Aguardando por cima do que a troca trouxe.
    ///
    /// # Errors
    /// Falha do cache.
    pub fn reaplicar_escritas(&mut self) -> ResultadoDeEscrita<()> {
        self.armazem
            .transacao_imediata(|tx| -> Result<ResultadoDeEscrita<()>, ErroDoCache> {
                Ok(reaplicar_pendentes(tx))
            })?
    }

    /// "Refazer sobre a nova".
    ///
    /// # Errors
    /// [`ErroDeEscrita`].
    pub fn refazer_escrita(&mut self, id: Id, agora: &str) -> ResultadoDeEscrita<Escrita> {
        let nova_id = Uuid::new_v4();
        self.armazem.transacao_imediata(
            |tx| -> Result<ResultadoDeEscrita<Escrita>, ErroDoCache> {
                Ok((|| {
                    let mut antiga = uma_escrita(tx, id)?.ok_or(ErroDeEscrita::Inexistente(id))?;
                    if antiga.estado != EstadoDaEscrita::Recusada {
                        return Err(ErroDeEscrita::NaoEstaRecusada(id));
                    }
                    let (proposta, rotulo) = montar(tx, nova_id, &antiga.acao, agora)?;
                    aplicar_otimista(tx, &proposta)?;
                    let nova = Escrita {
                        id: nova_id,
                        acao: antiga.acao.clone(),
                        rotulo,
                        feita_em: agora.to_owned(),
                        estado: EstadoDaEscrita::Aguardando,
                        recusa: None,
                        refeita_em: None,
                        proposta,
                    };
                    gravar_escrita(tx, &nova)?;
                    antiga.estado = EstadoDaEscrita::Refeita;
                    antiga.refeita_em = Some(nova_id);
                    gravar_escrita(tx, &antiga)?;
                    Ok(nova)
                })())
            },
        )?
    }

    /// "Descartar".
    ///
    /// # Errors
    /// [`ErroDeEscrita`].
    pub fn descartar_escrita(&mut self, id: Id) -> ResultadoDeEscrita<()> {
        self.armazem
            .transacao_imediata(|tx| -> Result<ResultadoDeEscrita<()>, ErroDoCache> {
                Ok((|| {
                    let mut e = uma_escrita(tx, id)?.ok_or(ErroDeEscrita::Inexistente(id))?;
                    if e.estado != EstadoDaEscrita::Recusada {
                        return Err(ErroDeEscrita::NaoEstaRecusada(id));
                    }
                    e.estado = EstadoDaEscrita::Descartada;
                    gravar_escrita(tx, &e)
                })())
            })?
    }

    /// Os itens que têm escrita Aguardando.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn itens_pendentes(&self) -> Result<HashSet<(Colecao, Id)>, ErroDoCache> {
        let mut saida = HashSet::new();
        for e in self.escritas()? {
            if e.estado == EstadoDaEscrita::Aguardando {
                for b in &e.proposta.versoes_de_base {
                    saida.insert((b.colecao, b.id));
                }
                // Rascunhos aprovados com o lote, e rateios, pertencem à escrita também.
                if let Acao::RatearMovimentacao { rateios, .. } = &e.acao {
                    for r in rateios {
                        saida.insert((Colecao::Rateios, r.id));
                    }
                }
            }
        }
        Ok(saida)
    }

    /// A caixa de entrada: os rascunhos agrupados por lote (o mais novo primeiro).
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn caixa_de_entrada(&self, so_a_decidir: bool) -> Result<Vec<LoteNaCaixa>, ErroDoCache> {
        let lotes: Vec<LoteDeRascunhos> = self.linhas(Colecao::LotesDeRascunhos)?;
        let rascunhos: Vec<Rascunho> = self.linhas(Colecao::Rascunhos)?;
        let escritas = self.escritas()?;
        let marca = |r: &Rascunho| {
            let mexe = |e: &&Escrita, estado: EstadoDaEscrita| {
                e.estado == estado
                    && e.proposta
                        .versoes_de_base
                        .iter()
                        .any(|b| b.colecao == Colecao::Rascunhos && b.id == r.id)
            };
            (
                escritas
                    .iter()
                    .any(|e| mexe(&e, EstadoDaEscrita::Aguardando)),
                escritas.iter().any(|e| mexe(&e, EstadoDaEscrita::Recusada)),
            )
        };
        let mut grupos: Vec<LoteNaCaixa> = Vec::new();
        let mut ordenados = lotes;
        ordenados.sort_by(|a, b| b.criado_em.cmp(&a.criado_em).then(a.id.cmp(&b.id)));
        for lote in ordenados {
            let itens: Vec<ItemDaCaixa> = rascunhos
                .iter()
                .filter(|r| r.lote == Some(lote.id) && (!so_a_decidir || a_decidir(r)))
                .map(|r| {
                    let (pendente, recusado) = marca(r);
                    ItemDaCaixa {
                        rascunho: r.clone(),
                        pendente,
                        recusado,
                    }
                })
                .collect();
            if !itens.is_empty() || !so_a_decidir {
                grupos.push(LoteNaCaixa {
                    lote: Some(lote),
                    itens,
                });
            }
        }
        let soltos: Vec<ItemDaCaixa> = rascunhos
            .iter()
            .filter(|r| r.lote.is_none() && (!so_a_decidir || a_decidir(r)))
            .map(|r| {
                let (pendente, recusado) = marca(r);
                ItemDaCaixa {
                    rascunho: r.clone(),
                    pendente,
                    recusado,
                }
            })
            .collect();
        if !soltos.is_empty() {
            grupos.push(LoteNaCaixa {
                lote: None,
                itens: soltos,
            });
        }
        Ok(grupos)
    }
}
