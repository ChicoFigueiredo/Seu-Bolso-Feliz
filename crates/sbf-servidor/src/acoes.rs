//! As Ações de uma alteração (grill Q6/Q7 e spec 08), uma transação por proposta:
//!
//! 1. confere a versão de base de **cada** linha tocada contra a linha travada
//!    (`FOR UPDATE`); qualquer divergência recusa a proposta inteira;
//! 2. aplica a Ação dentro de um *savepoint*: se uma regra do banco ou do lançamento a
//!    reprova, o savepoint desfaz tudo o que ela fez e a proposta é **recusada** com
//!    [`CodigoDeErro::RegraViolada`], com as linhas como estão agora;
//! 3. um pedido incoerente (soma de rateio que não bate, dados fora do formato) não é
//!    recusa: é pedido inválido (HTTP 422) e nada se grava.

use sbf_contrato::{
    Acao, CodigoDeErro, Colecao, EdicaoDaMovimentacao, ItemAlterado, LinhaAgora, NovoRateio,
    PropostaDeAlteracao, RespostaDaAlteracao,
};
use tokio_postgres::{GenericClient, Transaction};
use uuid::Uuid;

use crate::banco::sql_versao;
use crate::dinheiro::{centavos_do_numeric, numeric_de_centavos};
use crate::http::ErroHttp;
use crate::livro::{ler_lote, ler_movimentacao, ler_rascunho, ler_rateios_da_movimentacao};
use crate::rascunhos::{conferir_correcao, dados_v1, payload_de_lancamento};

/// O que pode dar errado dentro de uma Ação.
#[derive(Debug)]
pub enum Falha {
    /// Falha do servidor ou pedido inválido: aborta tudo, nada é gravado.
    Http(ErroHttp),
    /// Uma regra do banco ou do lançamento reprovou: o savepoint desfaz e a proposta é
    /// recusada com esta mensagem.
    Regra(String),
}

impl From<ErroHttp> for Falha {
    fn from(e: ErroHttp) -> Self {
        Falha::Http(e)
    }
}

impl From<crate::banco::ErroDeLeitura> for Falha {
    fn from(e: crate::banco::ErroDeLeitura) -> Self {
        Falha::Http(e.into())
    }
}

impl From<tokio_postgres::Error> for Falha {
    fn from(e: tokio_postgres::Error) -> Self {
        let regra = e.code().is_some_and(|c| {
            let c = c.code();
            c.starts_with("22") || c.starts_with("23") || c == "P0001" || c == "P0002"
        });
        if regra {
            let mensagem = e.as_db_error().map_or_else(
                || "o banco recusou a alteração".to_owned(),
                |d| d.message().to_owned(),
            );
            Falha::Regra(mensagem)
        } else {
            Falha::Http(e.into())
        }
    }
}

type Res<T> = Result<T, Falha>;

enum Saida {
    Aceita {
        linhas: Vec<LinhaAgora>,
        excluidas: Vec<ItemAlterado>,
    },
    Recusa {
        motivo: CodigoDeErro,
        mensagem: String,
    },
}

fn recusa(motivo: CodigoDeErro, mensagem: impl Into<String>) -> Res<Saida> {
    Ok(Saida::Recusa {
        motivo,
        mensagem: mensagem.into(),
    })
}

fn pedido_invalido<T>(mensagem: impl Into<String>) -> Res<T> {
    Err(Falha::Http(ErroHttp::pedido_invalido(mensagem)))
}

pub fn recusada(
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

// ───────────────────────────── a forma da proposta ─────────────────────────────

/// O item e as linhas que a Ação toca, na forma que a estação tem de mandar. `None` nas
/// bases: o conjunto de bases é livre (só o lote de rascunhos).
fn forma(acao: &Acao) -> (ItemAlterado, Option<Vec<(Colecao, Uuid)>>) {
    let mov = |id| (Colecao::Movimentacoes, id);
    let rasc = |id| (Colecao::Rascunhos, id);
    let item = |(colecao, id)| ItemAlterado { colecao, id };
    match acao {
        Acao::MudarCategoriaDaMovimentacao { movimentacao, .. }
        | Acao::EditarMovimentacao { movimentacao, .. }
        | Acao::ExcluirMovimentacao { movimentacao }
        | Acao::RatearMovimentacao { movimentacao, .. } => {
            (item(mov(*movimentacao)), Some(vec![mov(*movimentacao)]))
        }
        Acao::CorrigirRascunho { rascunho, .. }
        | Acao::AprovarRascunho { rascunho }
        | Acao::RecusarRascunho { rascunho, .. } => {
            (item(rasc(*rascunho)), Some(vec![rasc(*rascunho)]))
        }
        Acao::ConciliarRascunho {
            rascunho,
            movimentacao,
        } => (
            item(rasc(*rascunho)),
            Some(vec![rasc(*rascunho), mov(*movimentacao)]),
        ),
        Acao::AprovarLoteDeRascunhos { lote } => (item((Colecao::LotesDeRascunhos, *lote)), None),
    }
}

/// A proposta é coerente: o item é o da Ação, há versão de base para cada linha tocada (e
/// só para elas) e os campos da Ação fazem sentido por si.
pub fn validar(proposta: &PropostaDeAlteracao) -> Result<(), ErroHttp> {
    let (item, esperadas) = forma(&proposta.acao);
    if proposta.item != item {
        return Err(ErroHttp::pedido_invalido(
            "o item da alteração não é o da ação",
        ));
    }
    let bases: Vec<(Colecao, Uuid)> = proposta
        .versoes_de_base
        .iter()
        .map(|b| (b.colecao, b.id))
        .collect();
    let mut sem_repetir = bases.clone();
    sem_repetir.sort_by_key(|(c, i)| (c.caminho(), *i));
    sem_repetir.dedup();
    if sem_repetir.len() != bases.len() {
        return Err(ErroHttp::pedido_invalido(
            "há versão de base repetida na alteração",
        ));
    }
    match esperadas {
        Some(mut esperadas) => {
            esperadas.sort_by_key(|(c, i)| (c.caminho(), *i));
            if sem_repetir != esperadas {
                return Err(ErroHttp::pedido_invalido(
                    "mande uma versão de base para cada linha que a ação toca, e só para elas",
                ));
            }
        }
        None => {
            let Acao::AprovarLoteDeRascunhos { lote } = &proposta.acao else {
                return Err(ErroHttp::interno("forma sem bases"));
            };
            let ok = bases
                .iter()
                .filter(|(c, i)| *c == Colecao::LotesDeRascunhos && i == lote)
                .count()
                == 1
                && bases.iter().all(|(c, i)| {
                    *c == Colecao::Rascunhos || (*c == Colecao::LotesDeRascunhos && i == lote)
                });
            if !ok {
                return Err(ErroHttp::pedido_invalido(
                    "aprovar o lote toca o lote e os rascunhos pendentes dele: mande a base de cada um",
                ));
            }
        }
    }
    validar_campos(&proposta.acao)
}

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

fn validar_campos(acao: &Acao) -> Result<(), ErroHttp> {
    match acao {
        Acao::EditarMovimentacao { edicao, .. } => {
            if !data_valida(&edicao.data) {
                return Err(ErroHttp::pedido_invalido(
                    "a data precisa estar em AAAA-MM-DD",
                ));
            }
            if edicao.valor <= 0 {
                return Err(ErroHttp::pedido_invalido(
                    "o valor precisa ser maior que zero",
                ));
            }
        }
        Acao::RatearMovimentacao { rateios, .. } => {
            if rateios.is_empty() {
                return Err(ErroHttp::pedido_invalido(
                    "o rateio precisa de ao menos um centro",
                ));
            }
            let mut centros: Vec<_> = rateios.iter().map(|r| r.centro).collect();
            centros.sort();
            centros.dedup();
            let mut ids: Vec<_> = rateios.iter().map(|r| r.id).collect();
            ids.sort();
            ids.dedup();
            if centros.len() != rateios.len() || ids.len() != rateios.len() {
                return Err(ErroHttp::pedido_invalido(
                    "o rateio repete um centro ou um id",
                ));
            }
            if rateios.iter().any(|r| r.valor <= 0) {
                return Err(ErroHttp::pedido_invalido(
                    "cada rateio precisa ter valor maior que zero",
                ));
            }
        }
        Acao::CorrigirRascunho { dados, .. } if !dados.is_object() => {
            return Err(ErroHttp::pedido_invalido(
                "os dados do rascunho precisam ser um objeto",
            ));
        }
        _ => {}
    }
    Ok(())
}

// ─────────────────────────────── a aplicação ───────────────────────────────

/// Aplica a proposta numa transação já aberta: aceita ou recusa, nunca pela metade.
pub async fn aplicar(
    transacao: &mut Transaction<'_>,
    dono: Uuid,
    proposta: &PropostaDeAlteracao,
) -> Result<RespostaDaAlteracao, ErroHttp> {
    // O gatilho de rateio é diferido até o `COMMIT`; imediato, o erro dele cai dentro do
    // savepoint e vira recusa em vez de um 500 na hora de gravar.
    transacao
        .batch_execute("SET CONSTRAINTS transaction_allocations_valida IMMEDIATE")
        .await?;

    if let Some((motivo, mensagem)) = conferir_bases(transacao, dono, proposta).await? {
        let agora = linhas_agora(transacao, dono, proposta).await?;
        return Ok(recusada(proposta, motivo, &mensagem, agora));
    }

    let ponto = transacao.savepoint("acao").await?;
    let resultado = executar(&ponto, dono, proposta).await;
    match resultado {
        Ok(Saida::Aceita { linhas, excluidas }) => {
            ponto.commit().await?;
            Ok(RespostaDaAlteracao::Aceita {
                id: proposta.id,
                linhas,
                excluidas,
            })
        }
        Ok(Saida::Recusa { motivo, mensagem }) => {
            ponto.rollback().await?;
            let agora = linhas_agora(transacao, dono, proposta).await?;
            Ok(recusada(proposta, motivo, &mensagem, agora))
        }
        Err(Falha::Regra(mensagem)) => {
            ponto.rollback().await?;
            let agora = linhas_agora(transacao, dono, proposta).await?;
            Ok(recusada(
                proposta,
                CodigoDeErro::RegraViolada,
                &mensagem,
                agora,
            ))
        }
        Err(Falha::Http(e)) => {
            let _ = ponto.rollback().await;
            Err(e)
        }
    }
}

fn tabela_de(colecao: Colecao) -> Option<&'static str> {
    match colecao {
        Colecao::Movimentacoes => Some("transactions"),
        Colecao::Rascunhos => Some("draft_records"),
        Colecao::LotesDeRascunhos => Some("draft_batches"),
        _ => None,
    }
}

/// Trava cada linha de base e compara a versão. `Some` é a recusa (motivo e mensagem).
async fn conferir_bases(
    transacao: &Transaction<'_>,
    dono: Uuid,
    proposta: &PropostaDeAlteracao,
) -> Result<Option<(CodigoDeErro, String)>, ErroHttp> {
    let mut bases: Vec<_> = proposta.versoes_de_base.iter().collect();
    bases.sort_by_key(|b| (b.colecao.caminho(), b.id));
    for base in bases {
        let Some(tabela) = tabela_de(base.colecao) else {
            return Err(ErroHttp::pedido_invalido("coleção sem versão de base"));
        };
        let sql = format!(
            "SELECT {} AS versao FROM public.{tabela} WHERE id = $1 AND user_id = $2 FOR UPDATE",
            sql_versao!("updated_at")
        );
        let linha = transacao
            .query_opt(sql.as_str(), &[&base.id, &dono])
            .await?;
        let Some(linha) = linha else {
            return Ok(Some((
                CodigoDeErro::ItemNaoEncontrado,
                "uma linha da alteração não existe mais no servidor".into(),
            )));
        };
        let agora: String = linha.try_get("versao")?;
        if base.versao.0 != agora {
            return Ok(Some((
                CodigoDeErro::VersaoDeBaseDivergente,
                "uma linha da alteração mudou no servidor depois da sua cópia".into(),
            )));
        }
    }
    if let Acao::AprovarLoteDeRascunhos { lote } = &proposta.acao {
        let agora = rascunhos_lancaveis(transacao, dono, *lote).await?;
        let mut dele: Vec<Uuid> = proposta
            .versoes_de_base
            .iter()
            .filter(|b| b.colecao == Colecao::Rascunhos)
            .map(|b| b.id)
            .collect();
        dele.sort();
        let mut no_servidor = agora;
        no_servidor.sort();
        if dele != no_servidor {
            return Ok(Some((
                CodigoDeErro::VersaoDeBaseDivergente,
                "o lote ganhou ou perdeu rascunhos pendentes depois da sua cópia".into(),
            )));
        }
    }
    Ok(None)
}

/// Os rascunhos do lote que ainda esperam uma decisão ou um lançamento.
async fn rascunhos_lancaveis<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    lote: Uuid,
) -> Result<Vec<Uuid>, ErroHttp> {
    let linhas = cliente
        .query(
            "SELECT id FROM public.draft_records WHERE batch_id = $1 AND user_id = $2 \
             AND status IN ('pending_review', 'corrected', 'approved') AND posted_record_id IS NULL \
             ORDER BY created_at, id FOR UPDATE",
            &[&lote, &dono],
        )
        .await?;
    linhas
        .iter()
        .map(|l| l.try_get::<_, Uuid>("id").map_err(ErroHttp::from))
        .collect()
}

/// Cada linha tocada como está no servidor agora (vazio se sumiu).
async fn linhas_agora<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    proposta: &PropostaDeAlteracao,
) -> Result<Vec<LinhaAgora>, ErroHttp> {
    let mut saida = Vec::new();
    for base in &proposta.versoes_de_base {
        match base.colecao {
            Colecao::Movimentacoes => {
                if let Some(m) = ler_movimentacao(cliente, dono, base.id).await? {
                    saida.push(LinhaAgora::Movimentacoes(Box::new(m)));
                    for r in ler_rateios_da_movimentacao(cliente, dono, base.id).await? {
                        saida.push(LinhaAgora::Rateios(r));
                    }
                }
            }
            Colecao::Rascunhos => {
                if let Some(r) = ler_rascunho(cliente, dono, base.id).await? {
                    saida.push(LinhaAgora::Rascunhos(Box::new(r)));
                }
            }
            Colecao::LotesDeRascunhos => {
                if let Some(l) = ler_lote(cliente, dono, base.id).await? {
                    saida.push(LinhaAgora::LotesDeRascunhos(l));
                }
            }
            _ => {}
        }
    }
    Ok(saida)
}

async fn executar(t: &Transaction<'_>, dono: Uuid, proposta: &PropostaDeAlteracao) -> Res<Saida> {
    match &proposta.acao {
        Acao::MudarCategoriaDaMovimentacao {
            movimentacao,
            categoria,
        } => mudar_categoria(t, dono, *movimentacao, *categoria).await,
        Acao::CorrigirRascunho { rascunho, dados } => corrigir(t, dono, *rascunho, dados).await,
        Acao::AprovarRascunho { rascunho } => aprovar(t, dono, *rascunho).await,
        Acao::RecusarRascunho { rascunho, motivo } => recusar(t, dono, *rascunho, motivo).await,
        Acao::AprovarLoteDeRascunhos { lote } => aprovar_lote(t, dono, *lote).await,
        Acao::EditarMovimentacao {
            movimentacao,
            edicao,
        } => editar(t, dono, *movimentacao, edicao).await,
        Acao::ExcluirMovimentacao { movimentacao } => excluir(t, dono, *movimentacao).await,
        Acao::RatearMovimentacao {
            movimentacao,
            rateios,
        } => ratear(t, dono, *movimentacao, rateios).await,
        Acao::ConciliarRascunho {
            rascunho,
            movimentacao,
        } => conciliar(t, dono, *rascunho, *movimentacao).await,
    }
}

async fn existe<C: GenericClient>(
    cliente: &C,
    tabela: &str,
    dono: Uuid,
    id: Uuid,
) -> Result<bool, tokio_postgres::Error> {
    let sql = format!("SELECT 1 FROM public.{tabela} WHERE id = $1 AND user_id = $2");
    Ok(cliente
        .query_opt(sql.as_str(), &[&id, &dono])
        .await?
        .is_some())
}

async fn movimentacao_agora(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<LinhaAgora> {
    let m = ler_movimentacao(t, dono, id)
        .await?
        .ok_or_else(|| ErroHttp::interno("a movimentação alterada sumiu"))?;
    Ok(LinhaAgora::Movimentacoes(Box::new(m)))
}

async fn rascunho_agora(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<LinhaAgora> {
    let r = ler_rascunho(t, dono, id)
        .await?
        .ok_or_else(|| ErroHttp::interno("o rascunho alterado sumiu"))?;
    Ok(LinhaAgora::Rascunhos(Box::new(r)))
}

async fn lote_agora(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<LinhaAgora> {
    let l = ler_lote(t, dono, id)
        .await?
        .ok_or_else(|| ErroHttp::interno("o lote alterado sumiu"))?;
    Ok(LinhaAgora::LotesDeRascunhos(l))
}

// ───────────────────────────── movimentações ─────────────────────────────

async fn mudar_categoria(
    t: &Transaction<'_>,
    dono: Uuid,
    movimentacao: Uuid,
    categoria: Option<Uuid>,
) -> Res<Saida> {
    if let Some(c) = categoria
        && !existe(t, "categories", dono, c).await?
    {
        return recusa(
            CodigoDeErro::ReferenciaNaoEncontrada,
            "a categoria escolhida não existe no servidor",
        );
    }
    // O gatilho `trg_transactions_updated_at` muda `updated_at`, e com ele a versão.
    t.execute(
        "UPDATE public.transactions SET category_id = $3 WHERE id = $1 AND user_id = $2",
        &[&movimentacao, &dono, &categoria],
    )
    .await?;
    Ok(Saida::Aceita {
        linhas: vec![movimentacao_agora(t, dono, movimentacao).await?],
        excluidas: vec![],
    })
}

async fn tipo_e_valor(t: &Transaction<'_>, dono: Uuid, mov: Uuid) -> Res<(String, i64)> {
    let l = t
        .query_one(
            "SELECT type, abs(amount)::text AS valor FROM public.transactions WHERE id = $1 AND user_id = $2",
            &[&mov, &dono],
        )
        .await?;
    let texto: String = l.try_get("valor")?;
    let valor = centavos_do_numeric(&texto).map_err(ErroHttp::interno)?;
    Ok((l.try_get("type")?, valor))
}

async fn editar(
    t: &Transaction<'_>,
    dono: Uuid,
    movimentacao: Uuid,
    e: &EdicaoDaMovimentacao,
) -> Res<Saida> {
    if !existe(t, "financial_products", dono, e.conta).await? {
        return recusa(
            CodigoDeErro::ReferenciaNaoEncontrada,
            "a conta escolhida não existe no servidor",
        );
    }
    if let Some(f) = e.fornecedor
        && !existe(t, "suppliers", dono, f).await?
    {
        return recusa(
            CodigoDeErro::ReferenciaNaoEncontrada,
            "o fornecedor escolhido não existe no servidor",
        );
    }
    if let Some(c) = e.centro
        && !existe(t, "financial_centers", dono, c).await?
    {
        return recusa(
            CodigoDeErro::ReferenciaNaoEncontrada,
            "o centro escolhido não existe no servidor",
        );
    }
    let antigos = ler_rateios_da_movimentacao(t, dono, movimentacao).await?;
    let (tipo, _) = tipo_e_valor(t, dono, movimentacao).await?;
    if e.centro.is_some() && tipo == "statement_payment" {
        return Err(Falha::Regra(
            "pagamento de fatura não aceita rateio: as compras da fatura já foram rateadas".into(),
        ));
    }
    if e.centro.is_none() {
        let somado: i64 = antigos.iter().map(|r| r.valor).sum();
        if somado > e.valor {
            return Err(Falha::Regra(
                "os rateios somam mais que o novo valor: rateie de novo".into(),
            ));
        }
    }
    let valor = numeric_de_centavos(e.valor);
    t.execute(
        "UPDATE public.transactions SET description = $3, event_date = $4::text::date, \
         amount = $5::text::numeric, financial_product_id = $6, supplier_id = $7, notes = $8 \
         WHERE id = $1 AND user_id = $2",
        &[
            &movimentacao,
            &dono,
            &e.descricao,
            &e.data,
            &valor,
            &e.conta,
            &e.fornecedor,
            &e.notas,
        ],
    )
    .await?;
    let mut excluidas = Vec::new();
    if let Some(centro) = e.centro {
        t.execute(
            "DELETE FROM public.transaction_allocations WHERE transaction_id = $1 AND user_id = $2",
            &[&movimentacao, &dono],
        )
        .await?;
        t.execute(
            "INSERT INTO public.transaction_allocations (user_id, transaction_id, financial_center_id, amount) \
             VALUES ($1, $2, $3, $4::text::numeric)",
            &[&dono, &movimentacao, &centro, &valor],
        )
        .await?;
        excluidas = antigos
            .iter()
            .map(|r| ItemAlterado {
                colecao: Colecao::Rateios,
                id: r.id,
            })
            .collect();
    }
    let mut linhas = vec![movimentacao_agora(t, dono, movimentacao).await?];
    if e.centro.is_some() {
        linhas.extend(
            ler_rateios_da_movimentacao(t, dono, movimentacao)
                .await?
                .into_iter()
                .map(LinhaAgora::Rateios),
        );
    }
    Ok(Saida::Aceita { linhas, excluidas })
}

async fn excluir(t: &Transaction<'_>, dono: Uuid, movimentacao: Uuid) -> Res<Saida> {
    let rateios = ler_rateios_da_movimentacao(t, dono, movimentacao).await?;
    t.execute(
        "DELETE FROM public.transactions WHERE id = $1 AND user_id = $2",
        &[&movimentacao, &dono],
    )
    .await?;
    let mut excluidas = vec![ItemAlterado {
        colecao: Colecao::Movimentacoes,
        id: movimentacao,
    }];
    excluidas.extend(rateios.iter().map(|r| ItemAlterado {
        colecao: Colecao::Rateios,
        id: r.id,
    }));
    Ok(Saida::Aceita {
        linhas: vec![],
        excluidas,
    })
}

async fn ratear(
    t: &Transaction<'_>,
    dono: Uuid,
    movimentacao: Uuid,
    novos: &[NovoRateio],
) -> Res<Saida> {
    let (tipo, valor) = tipo_e_valor(t, dono, movimentacao).await?;
    let soma: i64 = novos.iter().map(|r| r.valor).sum();
    if soma != valor {
        return pedido_invalido(format!(
            "a soma dos rateios ({}) tem de ser igual ao valor da movimentação ({})",
            numeric_de_centavos(soma),
            numeric_de_centavos(valor)
        ));
    }
    if tipo == "statement_payment" {
        return Err(Falha::Regra(
            "pagamento de fatura não aceita rateio: as compras da fatura já foram rateadas".into(),
        ));
    }
    for r in novos {
        if !existe(t, "financial_centers", dono, r.centro).await? {
            return recusa(
                CodigoDeErro::ReferenciaNaoEncontrada,
                "um dos centros escolhidos não existe no servidor",
            );
        }
    }
    let antigos = ler_rateios_da_movimentacao(t, dono, movimentacao).await?;
    t.execute(
        "DELETE FROM public.transaction_allocations WHERE transaction_id = $1 AND user_id = $2",
        &[&movimentacao, &dono],
    )
    .await?;
    for r in novos {
        let texto = numeric_de_centavos(r.valor);
        t.execute(
            "INSERT INTO public.transaction_allocations (id, user_id, transaction_id, financial_center_id, amount, note) \
             VALUES ($1, $2, $3, $4, $5::text::numeric, $6)",
            &[&r.id, &dono, &movimentacao, &r.centro, &texto, &r.nota],
        )
        .await?;
    }
    let mut linhas = vec![movimentacao_agora(t, dono, movimentacao).await?];
    linhas.extend(
        ler_rateios_da_movimentacao(t, dono, movimentacao)
            .await?
            .into_iter()
            .map(LinhaAgora::Rateios),
    );
    let excluidas = antigos
        .iter()
        .filter(|a| !novos.iter().any(|n| n.id == a.id))
        .map(|r| ItemAlterado {
            colecao: Colecao::Rateios,
            id: r.id,
        })
        .collect();
    Ok(Saida::Aceita { linhas, excluidas })
}

// ─────────────────────────────── rascunhos ───────────────────────────────

struct DadosDoRascunho {
    estado: String,
    tipo: String,
    dados: serde_json::Value,
    versao_do_esquema: i16,
}

async fn dados_do_rascunho(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<DadosDoRascunho> {
    let l = t
        .query_one(
            "SELECT status, draft_type, draft_data, draft_schema_version FROM public.draft_records \
             WHERE id = $1 AND user_id = $2",
            &[&id, &dono],
        )
        .await?;
    Ok(DadosDoRascunho {
        estado: l.try_get("status")?,
        tipo: l.try_get("draft_type")?,
        dados: l.try_get("draft_data")?,
        versao_do_esquema: l.try_get("draft_schema_version")?,
    })
}

async fn lote_do_rascunho(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<Option<Uuid>> {
    Ok(t.query_one(
        "SELECT batch_id FROM public.draft_records WHERE id = $1 AND user_id = $2",
        &[&id, &dono],
    )
    .await?
    .try_get("batch_id")?)
}

/// Refaz as contagens e o estado do lote a partir dos rascunhos dele.
async fn recalcular_lote(t: &Transaction<'_>, dono: Uuid, lote: Uuid) -> Res<()> {
    t.execute(
        "UPDATE public.draft_batches b SET total_drafts = s.total, approved_count = s.aprovados, \
         rejected_count = s.recusados, status = CASE \
           WHEN s.pendentes > 0 THEN CASE WHEN s.aprovados + s.recusados > 0 THEN 'reviewing' ELSE 'open' END \
           WHEN s.recusados = 0 THEN 'approved' \
           WHEN s.aprovados = 0 THEN 'rejected' \
           ELSE 'partial' END \
         FROM (SELECT count(*)::int AS total, \
                 (count(*) FILTER (WHERE status IN ('approved', 'posted')))::int AS aprovados, \
                 (count(*) FILTER (WHERE status = 'rejected'))::int AS recusados, \
                 (count(*) FILTER (WHERE status IN ('pending_review', 'corrected')))::int AS pendentes \
               FROM public.draft_records WHERE batch_id = $1 AND user_id = $2) s \
         WHERE b.id = $1 AND b.user_id = $2",
        &[&lote, &dono],
    )
    .await?;
    Ok(())
}

async fn corrigir(
    t: &Transaction<'_>,
    dono: Uuid,
    id: Uuid,
    dados: &serde_json::Value,
) -> Res<Saida> {
    let atual = dados_do_rascunho(t, dono, id).await?;
    if !matches!(
        atual.estado.as_str(),
        "pending_review" | "corrected" | "approved"
    ) {
        return recusa(
            CodigoDeErro::RegraViolada,
            format!(
                "o rascunho está {} e não pode mais ser corrigido",
                atual.estado
            ),
        );
    }
    if let Err(mensagem) = conferir_correcao(&atual.tipo, dados) {
        return pedido_invalido(mensagem);
    }
    t.execute(
        "UPDATE public.draft_records SET draft_data = $3, draft_schema_version = 1, status = 'corrected', \
         corrections = jsonb_build_object('manual_edit', true, 'edited_at', now()), \
         materialization_error = NULL WHERE id = $1 AND user_id = $2",
        &[&id, &dono, dados],
    )
    .await?;
    Ok(Saida::Aceita {
        linhas: vec![rascunho_agora(t, dono, id).await?],
        excluidas: vec![],
    })
}

/// Aprova e lança um rascunho. `Ok(Err(mensagem))`: o rascunho não pode virar movimentação.
async fn aprovar_um(
    t: &Transaction<'_>,
    dono: Uuid,
    id: Uuid,
) -> Res<Result<Option<Uuid>, String>> {
    let atual = dados_do_rascunho(t, dono, id).await?;
    match atual.estado.as_str() {
        "posted" => return Ok(Ok(None)),
        "pending_review" | "corrected" | "approved" => {}
        outro => {
            return Ok(Err(format!(
                "o rascunho está {outro} e não pode ser lançado"
            )));
        }
    }
    let dados = dados_v1(&atual.tipo, &atual.dados, atual.versao_do_esquema);
    let payload = match payload_de_lancamento(&atual.tipo, &dados) {
        Ok(p) => p,
        Err(erros) => return Ok(Err(erros.join("; "))),
    };
    t.execute(
        "UPDATE public.draft_records SET status = 'approved', approved_at = now(), approved_by = $3 \
         WHERE id = $1 AND user_id = $2",
        &[&id, &dono, &dono],
    )
    .await?;
    let resultado = t
        .query_one(
            "SELECT public.fn_materialize_draft_record($1, $2, 'transactions', $3, 'desktop') AS resultado",
            &[&dono, &id, &payload],
        )
        .await?;
    let resultado: serde_json::Value = resultado.try_get("resultado")?;
    let lancado = resultado
        .get("posted_record_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    Ok(Ok(lancado))
}

async fn aprovar(t: &Transaction<'_>, dono: Uuid, id: Uuid) -> Res<Saida> {
    let lote = lote_do_rascunho(t, dono, id).await?;
    let lancado = match aprovar_um(t, dono, id).await? {
        Ok(l) => l,
        Err(mensagem) => return recusa(CodigoDeErro::RegraViolada, mensagem),
    };
    let mut linhas = vec![rascunho_agora(t, dono, id).await?];
    if let Some(mov) = lancado {
        linhas.push(movimentacao_agora(t, dono, mov).await?);
    }
    if let Some(lote) = lote {
        recalcular_lote(t, dono, lote).await?;
        linhas.push(lote_agora(t, dono, lote).await?);
    }
    Ok(Saida::Aceita {
        linhas,
        excluidas: vec![],
    })
}

async fn aprovar_lote(t: &Transaction<'_>, dono: Uuid, lote: Uuid) -> Res<Saida> {
    let ids = rascunhos_lancaveis(t, dono, lote).await?;
    let mut linhas = Vec::new();
    for id in &ids {
        match aprovar_um(t, dono, *id).await? {
            Ok(lancado) => {
                linhas.push(rascunho_agora(t, dono, *id).await?);
                if let Some(mov) = lancado {
                    linhas.push(movimentacao_agora(t, dono, mov).await?);
                }
            }
            Err(mensagem) => {
                return recusa(
                    CodigoDeErro::RegraViolada,
                    format!(
                        "o rascunho {id} não pode ser lançado, então o lote inteiro ficou como estava: {mensagem}"
                    ),
                );
            }
        }
    }
    recalcular_lote(t, dono, lote).await?;
    linhas.push(lote_agora(t, dono, lote).await?);
    Ok(Saida::Aceita {
        linhas,
        excluidas: vec![],
    })
}

async fn recusar(t: &Transaction<'_>, dono: Uuid, id: Uuid, motivo: &str) -> Res<Saida> {
    let atual = dados_do_rascunho(t, dono, id).await?;
    if !matches!(
        atual.estado.as_str(),
        "pending_review" | "corrected" | "approved"
    ) {
        return recusa(
            CodigoDeErro::RegraViolada,
            format!("o rascunho está {} e não pode ser recusado", atual.estado),
        );
    }
    let lote = lote_do_rascunho(t, dono, id).await?;
    t.execute(
        "UPDATE public.draft_records SET status = 'rejected', \
         corrections = jsonb_build_object('rejection_reason', $3::text) WHERE id = $1 AND user_id = $2",
        &[&id, &dono, &motivo],
    )
    .await?;
    let mut linhas = vec![rascunho_agora(t, dono, id).await?];
    if let Some(lote) = lote {
        recalcular_lote(t, dono, lote).await?;
        linhas.push(lote_agora(t, dono, lote).await?);
    }
    Ok(Saida::Aceita {
        linhas,
        excluidas: vec![],
    })
}

async fn conciliar(
    t: &Transaction<'_>,
    dono: Uuid,
    rascunho: Uuid,
    movimentacao: Uuid,
) -> Res<Saida> {
    t.execute(
        "UPDATE public.draft_records SET reconciliation_status = 'confirmed_duplicate', \
         reconciled_transaction_id = $3, reconciled_at = now() WHERE id = $1 AND user_id = $2",
        &[&rascunho, &dono, &movimentacao],
    )
    .await?;
    Ok(Saida::Aceita {
        linhas: vec![rascunho_agora(t, dono, rascunho).await?],
        excluidas: vec![],
    })
}
