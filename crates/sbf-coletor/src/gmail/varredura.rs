//! A varredura de um rótulo do Gmail (`index.ts` do gmail-scanner): lista as mensagens,
//! baixa os anexos aceitos, guarda os bytes no armazenamento e cria `source_documents` +
//! `ingestion_jobs` em `discovered`, no formato que o worker de ingestão já lê.
//!
//! Deduplicação em duas fases, como no Bun:
//! 1. antes de baixar: já existe `source_documents` do Gmail com a mesma mensagem e o mesmo nome;
//! 2. depois de baixar: o SHA-256 do conteúdo já está em `document_fingerprints` (arquivo
//!    idêntico vindo de outra mensagem ou de outra origem).
//!
//! Diferenças deliberadas em relação ao Bun (ver `docs/desktop/coletas-vps.md`):
//! - `source_documents` e `ingestion_jobs` entram na MESMA transação (o Bun deixava um
//!   documento sem job se o job falhasse, e a fase 1 o escondia para sempre);
//! - o `ingestion_runs` só nasce quando há o primeiro documento novo (um coletor a cada
//!   poucos minutos não pode encher a tabela de execuções vazias);
//! - a chave no armazenamento é `{dono}/{sha256}/{nome}` (como o upload do desktop), não
//!   `{dono}/{uuid}/{nome}`: repetir o envio regrava os mesmos bytes, sem blob órfão;
//! - falha ao ler UMA mensagem conta como erro e a varredura segue (o Bun abortava tudo);
//! - o checkpoint em `ingestion_checkpoints` (`source_type = gmail`) estreita a consulta com
//!   `after:` nas varreduras incrementais.

use deadpool_postgres::Pool;
use sbf_servidor::armazenamento::Armazenamento;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::cliente::ClienteGmail;
use super::consulta::{PartesDaConsulta, montar_consulta};
use super::mensagem::{
    Anexo, Metadados, data_do_cabecalho, exigencias_do_anexo, nome_seguro, processar,
};
use crate::erro::ErroDeColeta;

/// A folga que o `after:` incremental dá antes da última data vista (dois dias), para
/// mensagens que chegaram fora de ordem.
const FOLGA_DO_CHECKPOINT_MS: i64 = 2 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct OpcoesDaVarredura {
    pub rotulo: String,
    pub consulta: String,
    pub de: Option<String>,
    pub ate: Option<String>,
    /// No máximo N mensagens; `None` é todas.
    pub limite: Option<usize>,
    pub tamanho_do_lote: u32,
    /// Só lista, sem baixar, gravar nem criar jobs.
    pub simulacao: bool,
    /// Ignora o checkpoint: lista o rótulo inteiro (pega mensagens rotuladas depois).
    pub completa: bool,
}

impl Default for OpcoesDaVarredura {
    fn default() -> Self {
        Self {
            rotulo: "Comprovantes".to_owned(),
            consulta: String::new(),
            de: None,
            ate: None,
            limite: None,
            tamanho_do_lote: 50,
            simulacao: false,
            completa: false,
        }
    }
}

/// Os contadores (mesmos nomes do `ScanStats` do Bun, que vão para `ingestion_runs.stats`).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Estatisticas {
    pub messages_listed: u32,
    pub messages_with_attachments: u32,
    pub attachments_found: u32,
    pub attachments_uploaded: u32,
    pub jobs_created: u32,
    pub bodies_processed: u32,
    pub bodies_skipped: u32,
    pub skipped_already_processed: u32,
    pub skipped_by_msg_filename: u32,
    pub skipped_by_content_hash: u32,
    pub errors: u32,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn formatar_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

struct Varredor<'a> {
    gmail: &'a ClienteGmail,
    banco: &'a Pool,
    armazenamento: &'a dyn Armazenamento,
    dono: Uuid,
    execucao: Option<Uuid>,
    estat: Estatisticas,
    metadados_da_execucao: serde_json::Value,
}

/// Faz a varredura. Devolve os contadores; erros isolados de anexo viram contagem, não abortam.
pub async fn varrer(
    gmail: &ClienteGmail,
    banco: &Pool,
    armazenamento: &dyn Armazenamento,
    dono: Uuid,
    opcoes: &OpcoesDaVarredura,
) -> Result<Estatisticas, ErroDeColeta> {
    let consulta = montar_consulta(&PartesDaConsulta {
        consulta: Some(&opcoes.consulta),
        de: opcoes.de.as_deref(),
        ate: opcoes.ate.as_deref(),
    });
    let rotulo_id = match gmail.achar_rotulo(&opcoes.rotulo).await? {
        Some(id) => id,
        None => {
            let nomes: Vec<String> = gmail
                .listar_rotulos()
                .await?
                .into_iter()
                .map(|r| r.name)
                .collect();
            return Err(ErroDeColeta::Entrada(format!(
                "rótulo \"{}\" não encontrado no Gmail; rótulos disponíveis: {}",
                opcoes.rotulo,
                nomes.join(", ")
            )));
        }
    };

    // O checkpoint só estreita a consulta quando a varredura é incremental e sem período.
    let escopo = format!("rotulo:{}|consulta:{consulta}", opcoes.rotulo);
    let estreitar = !opcoes.completa && !opcoes.simulacao && opcoes.de.is_none();
    let marca_anterior = if estreitar {
        ler_marca(banco, dono, &escopo).await?
    } else {
        None
    };
    let consulta_efetiva = match marca_anterior {
        Some(ms) => {
            let desde = ((ms - FOLGA_DO_CHECKPOINT_MS) / 1000).max(0);
            if consulta.is_empty() {
                format!("after:{desde}")
            } else {
                format!("{consulta} after:{desde}")
            }
        }
        None => consulta.clone(),
    };

    let mut v = Varredor {
        gmail,
        banco,
        armazenamento,
        dono,
        execucao: None,
        estat: Estatisticas::default(),
        metadados_da_execucao: json!({
            "label": opcoes.rotulo,
            "label_id": rotulo_id,
            "query": if consulta_efetiva.is_empty() { None } else { Some(&consulta_efetiva) },
            "from_date": opcoes.de,
            "to_date": opcoes.ate,
            "limit": opcoes.limite,
            "origem": "coletor-vps",
        }),
    };

    let mut pagina: Option<String> = None;
    let mut processadas: usize = 0;
    let mut maior_data: i64 = 0;
    let mut terminou_inteira = true;
    let resultado: Result<(), ErroDeColeta> = async {
        loop {
            let restante = opcoes
                .limite
                .map_or(usize::MAX, |l| l.saturating_sub(processadas));
            if restante == 0 {
                terminou_inteira = false;
                break;
            }
            let lote = u32::try_from(restante.min(opcoes.tamanho_do_lote as usize)).unwrap_or(500);
            let lista = gmail
                .listar_mensagens(
                    Some(&rotulo_id),
                    Some(&consulta_efetiva),
                    lote.clamp(1, 500),
                    pagina.as_deref(),
                )
                .await?;
            if lista.messages.is_empty() {
                break;
            }
            for referencia in &lista.messages {
                if opcoes.limite.is_some_and(|l| processadas >= l) {
                    terminou_inteira = false;
                    break;
                }
                v.estat.messages_listed += 1;
                processadas += 1;
                let mensagem = match gmail.mensagem(&referencia.id).await {
                    Ok(m) => m,
                    Err(e) => {
                        v.estat.errors += 1;
                        eprintln!("erro ao ler a mensagem {}: {e}", referencia.id);
                        continue;
                    }
                };
                if let Ok(ms) = mensagem.internal_date.parse::<i64>() {
                    maior_data = maior_data.max(ms);
                }
                let processada = processar(&mensagem);
                if processada.anexos.is_empty() {
                    continue;
                }
                v.estat.messages_with_attachments += 1;
                v.estat.attachments_found +=
                    u32::try_from(processada.anexos.len()).unwrap_or(u32::MAX);
                println!(
                    "mensagem {processadas}: {} | anexos: {}",
                    corte(&processada.metadados.assunto, 60),
                    processada.anexos.len()
                );
                for anexo in &processada.anexos {
                    if opcoes.simulacao {
                        println!(
                            "  anexo {} ({}, {})",
                            anexo.nome,
                            anexo.tipo_mime,
                            formatar_bytes(usize::try_from(anexo.tamanho).unwrap_or(0))
                        );
                        continue;
                    }
                    if let Err(e) = v.anexo(&processada.metadados, anexo).await {
                        v.estat.errors += 1;
                        eprintln!("  erro em {}: {e}", anexo.nome);
                    }
                }
            }
            pagina = lista.next_page_token;
            if pagina.is_none() || opcoes.limite.is_some_and(|l| processadas >= l) {
                if pagina.is_some() {
                    terminou_inteira = false;
                }
                break;
            }
        }
        Ok(())
    }
    .await;

    if !opcoes.simulacao {
        v.finalizar_execucao().await;
        let limpa = resultado.is_ok() && terminou_inteira && v.estat.errors == 0;
        if let Err(e) = gravar_marca(
            banco,
            dono,
            &escopo,
            &v.estat,
            limpa.then_some(maior_data).filter(|m| *m > 0),
            resultado.is_ok() && v.estat.errors == 0,
        )
        .await
        {
            eprintln!("não deu para gravar o checkpoint do gmail: {e}");
        }
    }
    resultado.map(|()| v.estat)
}

fn corte(texto: &str, n: usize) -> String {
    texto.chars().take(n).collect()
}

impl Varredor<'_> {
    /// O `ingestion_runs` da varredura, criado na primeira necessidade.
    async fn execucao(&mut self) -> Result<Uuid, ErroDeColeta> {
        if let Some(id) = self.execucao {
            return Ok(id);
        }
        let cliente = self.banco.get().await?;
        let linha = cliente
            .query_one(
                "INSERT INTO public.ingestion_runs (user_id, source_type, status, metadata) \
                 VALUES ($1, 'gmail', 'running', $2) RETURNING id",
                &[&self.dono, &self.metadados_da_execucao],
            )
            .await?;
        let id: Uuid = linha.try_get("id")?;
        self.execucao = Some(id);
        Ok(id)
    }

    async fn finalizar_execucao(&mut self) {
        let Some(id) = self.execucao else { return };
        let falhou = self.estat.errors > 0 && self.estat.jobs_created == 0;
        let estatisticas = serde_json::to_value(&self.estat).unwrap_or_default();
        let r = async {
            let cliente = self.banco.get().await?;
            cliente
                .execute(
                    "UPDATE public.ingestion_runs SET status = $2, completed_at = now(), stats = $3 \
                     WHERE id = $1",
                    &[&id, &if falhou { "failed" } else { "completed" }, &estatisticas],
                )
                .await?;
            Ok::<_, ErroDeColeta>(())
        }
        .await;
        if let Err(e) = r {
            // O scan já aconteceu; só o registro do desfecho se perde.
            eprintln!("não deu para finalizar o ingestion_run: {e}");
        }
    }

    async fn anexo(&mut self, m: &Metadados, a: &Anexo) -> Result<(), ErroDeColeta> {
        let dono = self.dono;
        // Fase 1: antes de baixar.
        {
            let cliente = self.banco.get().await?;
            let ja = cliente
                .query_opt(
                    "SELECT id FROM public.source_documents WHERE user_id = $1 AND \
                     origin_type = 'gmail' AND gmail_message_id = $2 AND filename = $3 LIMIT 1",
                    &[&dono, &m.id_da_mensagem, &a.nome],
                )
                .await?;
            if ja.is_some() {
                self.estat.skipped_already_processed += 1;
                self.estat.skipped_by_msg_filename += 1;
                return Ok(());
            }
        }
        let bytes = self.gmail.anexo(&m.id_da_mensagem, &a.id_do_anexo).await?;
        let sha = hex(&Sha256::digest(&bytes));
        // Fase 2: o mesmo conteúdo, de qualquer origem.
        {
            let cliente = self.banco.get().await?;
            let ja = cliente
                .query_opt(
                    "SELECT id FROM public.document_fingerprints WHERE user_id = $1 AND \
                     content_hash = $2 LIMIT 1",
                    &[&dono, &sha],
                )
                .await?;
            if ja.is_some() {
                self.estat.skipped_already_processed += 1;
                self.estat.skipped_by_content_hash += 1;
                println!("  duplicado por conteúdo: {} [{}]", a.nome, &sha[..12]);
                return Ok(());
            }
        }
        let chave_de_origem = format!("gmail:{}:{sha}", m.id_da_mensagem);
        let seguro = nome_seguro(&a.nome);
        let chave = format!(
            "{dono}/{sha}/{}",
            if seguro.is_empty() { "anexo" } else { &seguro }
        );
        self.armazenamento
            .gravar(&chave, &bytes, &a.tipo_mime)
            .await
            .map_err(|e| ErroDeColeta::Armazenamento(e.to_string()))?;
        self.estat.attachments_uploaded += 1;

        let execucao = self.execucao().await?;
        let data = data_do_cabecalho(&m.data);
        let data_iso = data.map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
        let tamanho = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        let rotulos = m.rotulos.join(",");
        let requires = exigencias_do_anexo(&a.nome, &a.tipo_mime);
        let metadados_do_job = json!({
            "filename": a.nome,
            "origin": "gmail_scanner",
            "gmail_subject": m.assunto,
            "gmail_from": m.de,
            "gmail_date": data_iso,
        });

        let mut cliente = self.banco.get().await?;
        let transacao = cliente.transaction().await?;
        let documento = transacao
            .query_opt(
                "INSERT INTO public.source_documents \
                 (user_id, origin_type, origin_key, gmail_message_id, gmail_thread_id, \
                  gmail_attachment_id, gmail_label, gmail_date, gmail_from, gmail_subject, \
                  filename, mime_type, file_size_bytes, storage_path, content_hash, status) \
                 VALUES ($1, 'gmail', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'active') \
                 ON CONFLICT DO NOTHING RETURNING id",
                &[
                    &dono,
                    &chave_de_origem,
                    &m.id_da_mensagem,
                    &m.id_da_conversa,
                    &a.id_do_anexo,
                    &rotulos,
                    &data,
                    &m.de,
                    &m.assunto,
                    &a.nome,
                    &a.tipo_mime,
                    &tamanho,
                    &chave,
                    &sha,
                ],
            )
            .await?;
        let Some(documento) = documento else {
            // Outro processo (o Bun em paralelo) criou o mesmo documento entre a fase 1 e agora.
            transacao.rollback().await?;
            self.estat.skipped_already_processed += 1;
            self.estat.skipped_by_msg_filename += 1;
            return Ok(());
        };
        let documento: Uuid = documento.try_get("id")?;
        transacao
            .execute(
                "INSERT INTO public.ingestion_jobs \
                 (run_id, user_id, source_document_id, status, requires, metadata, max_retries, available_at) \
                 VALUES ($1, $2, $3, 'discovered', $4, $5, 3, now())",
                &[&execucao, &dono, &documento, &requires, &metadados_do_job],
            )
            .await?;
        transacao.commit().await?;
        self.estat.jobs_created += 1;

        // O fingerprint não invalida o documento: sem ele só a dedup por hash fica cega.
        if let Err(e) = cliente
            .execute(
                "INSERT INTO public.document_fingerprints \
                 (source_document_id, user_id, content_hash, hash_algorithm) VALUES ($1, $2, $3, 'sha256')",
                &[&documento, &dono, &sha],
            )
            .await
        {
            eprintln!("  aviso: fingerprint de {} não gravado: {}", a.nome, ErroDeColeta::from(e));
        }
        println!(
            "  ok {} ({}) [{}]",
            a.nome,
            formatar_bytes(bytes.len()),
            &sha[..12]
        );
        Ok(())
    }
}

/// A última data interna (ms) que uma varredura limpa viu neste escopo.
async fn ler_marca(banco: &Pool, dono: Uuid, escopo: &str) -> Result<Option<i64>, ErroDeColeta> {
    let cliente = banco.get().await?;
    let linha = cliente
        .query_opt(
            "SELECT cursor_value FROM public.ingestion_checkpoints \
             WHERE user_id = $1 AND source_type = 'gmail' AND scope_key = $2",
            &[&dono, &escopo],
        )
        .await?;
    let Some(l) = linha else { return Ok(None) };
    let valor: Option<String> = l.try_get("cursor_value")?;
    Ok(valor.and_then(|v| v.parse::<i64>().ok()))
}

/// Soma os contadores e, numa varredura limpa e inteira, avança a data interna.
async fn gravar_marca(
    banco: &Pool,
    dono: Uuid,
    escopo: &str,
    e: &Estatisticas,
    nova_marca: Option<i64>,
    sem_falha: bool,
) -> Result<(), ErroDeColeta> {
    let cliente = banco.get().await?;
    let marca = nova_marca.map(|m| m.to_string());
    let vistos = i32::try_from(e.messages_listed).unwrap_or(i32::MAX);
    let criados = i32::try_from(e.jobs_created).unwrap_or(i32::MAX);
    let duplicados = i32::try_from(e.skipped_already_processed).unwrap_or(i32::MAX);
    let erros = i32::try_from(e.errors).unwrap_or(i32::MAX);
    cliente
        .execute(
            "INSERT INTO public.ingestion_checkpoints \
             (user_id, source_type, scope_key, cursor_kind, cursor_value, status, messages_seen, \
              documents_created, duplicates_skipped, errors) \
             VALUES ($1, 'gmail', $2, 'internal_date', $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (user_id, source_type, scope_key) DO UPDATE SET \
               cursor_value = CASE WHEN EXCLUDED.cursor_value IS NULL THEN ingestion_checkpoints.cursor_value \
                 ELSE GREATEST(EXCLUDED.cursor_value::bigint, COALESCE(ingestion_checkpoints.cursor_value::bigint, 0))::text END, \
               status = EXCLUDED.status, \
               messages_seen = ingestion_checkpoints.messages_seen + EXCLUDED.messages_seen, \
               documents_created = ingestion_checkpoints.documents_created + EXCLUDED.documents_created, \
               duplicates_skipped = ingestion_checkpoints.duplicates_skipped + EXCLUDED.duplicates_skipped, \
               errors = ingestion_checkpoints.errors + EXCLUDED.errors",
            &[
                &dono,
                &escopo,
                &marca,
                &if sem_falha { "completed" } else { "failed" },
                &vistos,
                &criados,
                &duplicados,
                &erros,
            ],
        )
        .await?;
    Ok(())
}
