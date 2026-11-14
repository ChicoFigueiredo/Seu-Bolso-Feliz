//! A sincronização de uma conexão Pluggy (`sync-runner.ts` e o `index.ts` do worker): lista as
//! transações de cada conta mapeada, cria `draft_records` (mesmo destino do pipeline de
//! documentos), roda a reconciliação e nunca duplica uma transação já sincronizada
//! (`draft_records.external_ref`, único por usuário).
//!
//! Dois modos, a mesma função: **incremental** (janela curta, sem checkpoint) e **backfill**
//! (365 dias, com checkpoint por conta em `ingestion_checkpoints`, retomável entre execuções).
//! Os dois NUNCA compartilham checkpoint: um incremental no meio de um backfill não herda a
//! página onde o backfill parou.
//!
//! Diferenças deliberadas do Bun (ver `docs/desktop/coletas-vps.md`):
//! - o `draft_batches` só nasce quando há o primeiro draft novo (um coletor a cada poucos
//!   minutos não pode encher a tabela de lotes vazios);
//! - erro de uma conexão não derruba as seguintes: conta como falha e o processo sai com erro
//!   no fim (o Bun abortava todas);
//! - draft com data fora de `AAAA-MM-DD` conta como erro de item (o Bun lançava uma exceção
//!   que interrompia a conexão inteira).

use std::collections::HashMap;

use chrono::{Days, NaiveDate, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::error::SqlState;
use uuid::Uuid;

use super::categorias::nome_de_categoria;
use super::cliente::ClientePluggy;
use super::fornecedor::{Apelido, resolver_fornecedor};
use super::rascunho::{
    VERSAO_DO_ESQUEMA_DO_RASCUNHO, Vinculos, montar_draft_data, referencia_externa,
};
use super::reconciliacao::{DadosDoDraft, encontrar_candidatos};
use super::saldos::{Mapeamento, planejar_saldos};
use crate::checkpoint::{self, Progresso};
use crate::erro::ErroDeColeta;

const DIAS_DO_INCREMENTAL: u64 = 7;
const DIAS_DO_BACKFILL: u64 = 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    Incremental,
    Backfill,
}

#[derive(Debug, Clone, Default)]
pub struct OpcoesDaSincronizacao {
    pub de: Option<String>,
    pub ate: Option<String>,
    pub backfill: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultadoDaConexao {
    pub lote: Option<Uuid>,
    pub contas_sincronizadas: u32,
    pub criados: u32,
    pub ja_sincronizados: u32,
    pub ignoradas: u32,
    pub erros_de_item: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resumo {
    pub conexoes: u32,
    pub conexoes_com_falha: u32,
    pub criados: u32,
    pub ja_sincronizados: u32,
    pub erros_de_item: u32,
}

fn ler_data(texto: &str, nome: &str) -> Result<NaiveDate, ErroDeColeta> {
    NaiveDate::parse_from_str(texto.trim(), "%Y-%m-%d")
        .map_err(|_| ErroDeColeta::Entrada(format!("{nome}: use AAAA-MM-DD")))
}

/// Sincroniza todas as conexões Pluggy ativas do dono.
pub async fn sincronizar(
    banco: &Pool,
    pluggy: &ClientePluggy,
    dono: Uuid,
    opcoes: &OpcoesDaSincronizacao,
) -> Result<Resumo, ErroDeColeta> {
    let hoje = Utc::now().date_naive();
    sincronizar_em(banco, pluggy, dono, opcoes, hoje).await
}

/// Como [`sincronizar`], com o "hoje" escolhido (os testes fixam a data).
pub async fn sincronizar_em(
    banco: &Pool,
    pluggy: &ClientePluggy,
    dono: Uuid,
    opcoes: &OpcoesDaSincronizacao,
    hoje: NaiveDate,
) -> Result<Resumo, ErroDeColeta> {
    let de_pedido = opcoes
        .de
        .as_deref()
        .map(|d| ler_data(d, "--de"))
        .transpose()?;
    let ate_pedido = opcoes
        .ate
        .as_deref()
        .map(|d| ler_data(d, "--ate"))
        .transpose()?;
    let conexoes = {
        let c = banco.get().await?;
        c.query(
            "SELECT id, external_item_id, last_synced_at FROM public.provider_connections \
             WHERE user_id = $1 AND provider = 'pluggy' AND status = 'active' ORDER BY created_at, id",
            &[&dono],
        )
        .await?
    };
    let mut resumo = Resumo::default();
    if conexoes.is_empty() {
        println!("Nenhuma conexão Pluggy ativa.");
        return Ok(resumo);
    }
    for conexao in &conexoes {
        let id: Uuid = conexao.try_get("id")?;
        let item: String = conexao.try_get("external_item_id")?;
        let ultima: Option<chrono::DateTime<Utc>> = conexao.try_get("last_synced_at")?;
        resumo.conexoes += 1;
        let mapeamentos = match carregar_mapeamentos(banco, id).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("falha ao listar as contas de {id}: {e}");
                resumo.conexoes_com_falha += 1;
                continue;
            }
        };
        let de = de_pedido.unwrap_or_else(|| {
            if opcoes.backfill {
                hoje - Days::new(DIAS_DO_BACKFILL)
            } else {
                ultima.map_or(hoje - Days::new(DIAS_DO_INCREMENTAL), |u| u.date_naive())
            }
        });
        let ate = ate_pedido.unwrap_or(hoje);
        let modo = if opcoes.backfill {
            Modo::Backfill
        } else {
            Modo::Incremental
        };
        let r = match sincronizar_conexao(banco, pluggy, dono, &item, &mapeamentos, de, ate, modo)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("falha ao sincronizar a conexão {id}: {e}");
                resumo.conexoes_com_falha += 1;
                continue;
            }
        };
        println!(
            "Conexão {id}: {} draft(s) criado(s), {} já sincronizado(s), {} ignorado(s), {} erro(s).",
            r.criados, r.ja_sincronizados, r.ignoradas, r.erros_de_item
        );
        resumo.criados += r.criados;
        resumo.ja_sincronizados += r.ja_sincronizados;
        resumo.erros_de_item += r.erros_de_item;

        // Saldo é posição, não fluxo: as transações nunca o atualizam. Falha aqui não derruba o
        // sync (as transações já estão gravadas e valem sozinhas).
        match atualizar_saldos(banco, pluggy, dono, &item, &mapeamentos).await {
            Ok(0) => {}
            Ok(n) => println!("Conexão {id}: saldo atualizado em {n} conta(s)."),
            Err(e) => eprintln!("falha ao atualizar os saldos de {id}: {e}"),
        }
        let c = banco.get().await?;
        c.execute(
            "UPDATE public.provider_connections SET last_synced_at = now() WHERE id = $1",
            &[&id],
        )
        .await?;
    }
    Ok(resumo)
}

async fn carregar_mapeamentos(
    banco: &Pool,
    conexao: Uuid,
) -> Result<Vec<Mapeamento>, ErroDeColeta> {
    let c = banco.get().await?;
    let linhas = c
        .query(
            "SELECT external_account_id, financial_product_id, status \
             FROM public.external_account_mappings WHERE provider_connection_id = $1 \
             ORDER BY created_at, id",
            &[&conexao],
        )
        .await?;
    linhas
        .iter()
        .map(|l| {
            Ok(Mapeamento {
                id_externo_da_conta: l.try_get("external_account_id")?,
                produto: l.try_get("financial_product_id")?,
                situacao: l.try_get("status")?,
            })
        })
        .collect()
}

async fn atualizar_saldos(
    banco: &Pool,
    pluggy: &ClientePluggy,
    dono: Uuid,
    item: &str,
    mapeamentos: &[Mapeamento],
) -> Result<usize, ErroDeColeta> {
    let contas = pluggy.listar_contas(item).await?;
    let plano = planejar_saldos(&contas, mapeamentos);
    let c = banco.get().await?;
    for a in &plano {
        // Centavos inteiros dividem por 100 no próprio banco: exato, sem passar por float.
        c.execute(
            "UPDATE public.financial_products SET current_balance = ($1::bigint)::numeric / 100, \
             updated_at = now() WHERE id = $2 AND user_id = $3",
            &[&a.centavos, &a.produto, &dono],
        )
        .await?;
    }
    Ok(plano.len())
}

struct Contexto<'a> {
    banco: &'a Pool,
    dono: Uuid,
    lote: Option<Uuid>,
    apelidos: Vec<Apelido>,
    categorias: HashMap<String, Uuid>,
}

impl Contexto<'_> {
    async fn lote(&mut self) -> Result<Uuid, ErroDeColeta> {
        if let Some(l) = self.lote {
            return Ok(l);
        }
        let c = self.banco.get().await?;
        let nome = format!(
            "Sincronização Pluggy {}",
            Utc::now().date_naive().format("%Y-%m-%d")
        );
        let linha = c
            .query_one(
                "INSERT INTO public.draft_batches (user_id, name, status) VALUES ($1, $2, 'open') RETURNING id",
                &[&self.dono, &nome],
            )
            .await?;
        let id: Uuid = linha.try_get("id")?;
        self.lote = Some(id);
        Ok(id)
    }

    /// O id da categoria da sugestão da Pluggy, criando-a na primeira vez. Falha ao criar não
    /// derruba a importação: a transação entra sem categoria, que é recuperável depois.
    async fn categoria(&mut self, sugestao: Option<&str>) -> Option<Uuid> {
        let nome = nome_de_categoria(sugestao)?;
        if let Some(id) = self.categorias.get(&nome) {
            return Some(*id);
        }
        let r = async {
            let c = self.banco.get().await?;
            let l = c
                .query_one(
                    "INSERT INTO public.categories (user_id, name) VALUES ($1, $2) RETURNING id",
                    &[&self.dono, &nome],
                )
                .await?;
            Ok::<Uuid, ErroDeColeta>(l.try_get("id")?)
        }
        .await;
        match r {
            Ok(id) => {
                self.categorias.insert(nome, id);
                Some(id)
            }
            Err(e) => {
                eprintln!("falha ao criar a categoria \"{nome}\" (não crítico): {e}");
                None
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn sincronizar_conexao(
    banco: &Pool,
    pluggy: &ClientePluggy,
    dono: Uuid,
    item: &str,
    mapeamentos: &[Mapeamento],
    de: NaiveDate,
    ate: NaiveDate,
    modo: Modo,
) -> Result<ResultadoDaConexao, ErroDeColeta> {
    let ativos: Vec<&Mapeamento> = mapeamentos
        .iter()
        .filter(|m| m.situacao != "ignored")
        .collect();
    let mut r = ResultadoDaConexao {
        ignoradas: u32::try_from(mapeamentos.len() - ativos.len()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    if ativos.is_empty() {
        return Ok(r);
    }
    let mut ctx = Contexto {
        banco,
        dono,
        lote: None,
        apelidos: Vec::new(),
        categorias: HashMap::new(),
    };
    // Carregados uma vez por sync, não por transação. Falha aqui não é crítica.
    match banco.get().await {
        Ok(c) => {
            match c
                .query(
                    "SELECT supplier_id, alias_name FROM public.supplier_aliases WHERE user_id = $1 AND is_active = true \
                     ORDER BY created_at, id",
                    &[&dono],
                )
                .await
            {
                Ok(linhas) => {
                    for l in &linhas {
                        ctx.apelidos.push(Apelido {
                            fornecedor: l.try_get("supplier_id")?,
                            nome: l.try_get("alias_name")?,
                        });
                    }
                }
                Err(e) => eprintln!("falha ao buscar supplier_aliases (não crítico): {}", ErroDeColeta::from(e)),
            }
            match c
                .query(
                    "SELECT id, name FROM public.categories WHERE user_id = $1 AND is_active = true",
                    &[&dono],
                )
                .await
            {
                Ok(linhas) => {
                    for l in &linhas {
                        ctx.categorias.insert(l.try_get("name")?, l.try_get("id")?);
                    }
                }
                Err(e) => eprintln!("falha ao buscar categorias (não crítico): {}", ErroDeColeta::from(e)),
            }
        }
        Err(e) => eprintln!("sem conexão para ler apelidos e categorias (não crítico): {e}"),
    }

    for mapeamento in ativos {
        let janela = if modo == Modo::Backfill {
            let c = banco.get().await?;
            Some(
                checkpoint::iniciar_ou_retomar(
                    &**c,
                    dono,
                    "pluggy",
                    "page_number",
                    &format!("{item}:{}", mapeamento.id_externo_da_conta),
                    (de, ate),
                )
                .await?,
            )
        } else {
            None
        };
        let (de_efetivo, ate_efetivo) = janela.as_ref().map_or((de, ate), |j| (j.de, j.ate));
        let mut cursor: Option<String> = janela.as_ref().and_then(|j| j.cursor_inicial.clone());
        loop {
            let de_texto = de_efetivo.format("%Y-%m-%d").to_string();
            let ate_texto = ate_efetivo.format("%Y-%m-%d").to_string();
            let pagina = pluggy
                .listar_transacoes(
                    &mapeamento.id_externo_da_conta,
                    Some(&de_texto),
                    Some(&ate_texto),
                    cursor.as_deref(),
                )
                .await?;
            let mut progresso = Progresso {
                vistos: i32::try_from(pagina.transacoes.len()).unwrap_or(i32::MAX),
                ..Default::default()
            };
            for t in &pagina.transacoes {
                match gravar_transacao(&mut ctx, mapeamento, t).await {
                    Ok(Desfecho::Criado) => progresso.criados += 1,
                    Ok(Desfecho::JaExistia) => progresso.duplicados += 1,
                    Err(e) => {
                        progresso.erros += 1;
                        eprintln!("falha ao inserir o draft (txn {}): {e}", t.id_externo);
                    }
                }
            }
            r.criados += u32::try_from(progresso.criados).unwrap_or(0);
            r.ja_sincronizados += u32::try_from(progresso.duplicados).unwrap_or(0);
            r.erros_de_item += u32::try_from(progresso.erros).unwrap_or(0);
            // O cursor gravado é o da PRÓXIMA página: é de onde uma execução interrompida
            // recomeça. Gravar o atual repetiria a página que acabou de ser processada.
            cursor = pagina.proximo_cursor;
            progresso.proximo_cursor = cursor.clone();
            if let Some(j) = &janela {
                let c = banco.get().await?;
                checkpoint::registrar_pagina(&**c, j.id, &progresso).await?;
            }
            if cursor.is_none() {
                break;
            }
        }
        if let Some(j) = &janela {
            let c = banco.get().await?;
            checkpoint::concluir(&**c, j.id).await?;
        }
        r.contas_sincronizadas += 1;
    }
    if let Some(lote) = ctx.lote {
        let c = banco.get().await?;
        let criados = i32::try_from(r.criados).unwrap_or(i32::MAX);
        c.execute(
            "UPDATE public.draft_batches SET total_drafts = $2 WHERE id = $1",
            &[&lote, &criados],
        )
        .await?;
    }
    r.lote = ctx.lote;
    Ok(r)
}

enum Desfecho {
    Criado,
    JaExistia,
}

async fn gravar_transacao(
    ctx: &mut Contexto<'_>,
    mapeamento: &Mapeamento,
    t: &super::cliente::TransacaoNormalizada,
) -> Result<Desfecho, ErroDeColeta> {
    let referencia = referencia_externa(&t.id_externo);
    {
        let c = ctx.banco.get().await?;
        let ja = c
            .query_opt(
                "SELECT 1 FROM public.draft_records WHERE user_id = $1 AND external_ref = $2",
                &[&ctx.dono, &referencia],
            )
            .await?;
        if ja.is_some() {
            return Ok(Desfecho::JaExistia);
        }
    }
    let fornecedor = resolver_fornecedor(
        t.nome_do_comerciante.as_deref().unwrap_or(""),
        &ctx.apelidos,
    );
    let categoria = ctx.categoria(t.categoria.as_deref()).await;
    let dados = montar_draft_data(
        t,
        Vinculos {
            produto: mapeamento.produto,
            fornecedor,
            categoria,
        },
    )?;
    let lote = ctx.lote().await?;
    let c = ctx.banco.get().await?;
    let inserido = c
        .query_one(
            "INSERT INTO public.draft_records \
             (batch_id, user_id, source_document_id, extraction_result_id, draft_type, status, \
              draft_data, draft_schema_version, obligation_id, confidence_score, external_ref) \
             VALUES ($1, $2, NULL, NULL, 'transaction', 'pending_review', $3, $4, NULL, 1.00, $5) \
             RETURNING id",
            &[
                &lote,
                &ctx.dono,
                &dados,
                &VERSAO_DO_ESQUEMA_DO_RASCUNHO,
                &referencia,
            ],
        )
        .await;
    let draft: Uuid = match inserido {
        Ok(l) => l.try_get("id")?,
        // Unique de `external_ref`: uma corrida com outro coletor. Não é erro.
        Err(e) if e.code() == Some(&SqlState::UNIQUE_VIOLATION) => return Ok(Desfecho::JaExistia),
        Err(e) => return Err(e.into()),
    };

    // A reconciliação é uma anotação sobre um draft já criado: falhar aqui não o desfaz.
    let data = NaiveDate::parse_from_str(&t.data, "%Y-%m-%d").unwrap_or_default();
    let r = async {
        let resultado = encontrar_candidatos(
            &**c,
            ctx.dono,
            &DadosDoDraft {
                valor: t.centavos as f64 / 100.0,
                data,
                fornecedor,
                nome_do_fornecedor: t.nome_do_comerciante.as_deref(),
            },
        )
        .await?;
        c.execute(
            "UPDATE public.draft_records SET reconciliation_status = $2, \
             reconciliation_candidates = $3, reconciled_at = now() WHERE id = $1",
            &[
                &draft,
                &resultado.situacao,
                &serde_json::Value::Array(resultado.candidatos),
            ],
        )
        .await?;
        Ok::<(), ErroDeColeta>(())
    }
    .await;
    if let Err(e) = r {
        eprintln!("reconciliação falhou no draft {draft} (não crítico): {e}");
    }
    Ok(Desfecho::Criado)
}
