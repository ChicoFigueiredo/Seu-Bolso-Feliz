//! O comparador do corte (`sbf coletor-vps comparar`): só LÊ. Durante a semana em paralelo
//! (worker Bun e coletor Rust rodando juntos) mostra, dia a dia, o que cada um viu, para o
//! dono decidir se desliga o Bun.
//!
//! Quem criou cada execução do Gmail se distingue por `ingestion_runs.metadata.origem`
//! (`coletor-vps` é o Rust; ausente é o Bun). Como a deduplicação é a mesma, o que vale
//! comparar é o que cada um ENCONTROU (`attachmentsFound`), não o que criou: quem roda
//! primeiro cria, o outro só pula.
//!
//! A Pluggy não tem registro de execução; ali a comparação é o total de drafts por dia de
//! criação e o `sbf coletor-vps pluggy --simular`, que conta o que o Rust criaria agora (se
//! o Bun já trouxe tudo, dá zero).
//!
//! A transação é `READ ONLY`: o banco recusa qualquer escrita, mesmo por engano.

use std::fmt;

use deadpool_postgres::Pool;
use uuid::Uuid;

use crate::erro::ErroDeColeta;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinhaDeExecucao {
    pub dia: String,
    pub origem: String,
    pub execucoes: i64,
    pub mensagens: i64,
    pub anexos_encontrados: i64,
    pub jobs_criados: i64,
    pub ja_processados: i64,
    pub erros: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinhaDeCheckpoint {
    pub origem: String,
    pub escopo: String,
    pub situacao: String,
    pub cursor: String,
    pub vistos: i32,
    pub criados: i32,
    pub erros: i32,
    pub atualizado_em: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relatorio {
    pub dias: i64,
    pub gmail_execucoes: Vec<LinhaDeExecucao>,
    pub gmail_documentos: i64,
    pub gmail_documentos_sem_job: i64,
    /// `(dia, drafts)` dos drafts `pluggy:txn_%`.
    pub pluggy_drafts_por_dia: Vec<(String, i64)>,
    pub pluggy_drafts: i64,
    pub checkpoints: Vec<LinhaDeCheckpoint>,
    /// `(conexão, última sincronização)`.
    pub conexoes: Vec<(String, String)>,
}

impl Relatorio {
    /// Os dias em que Bun e Rust rodaram, com a diferença de anexos encontrados.
    pub fn divergencias_do_gmail(&self) -> Vec<(String, i64, i64)> {
        let mut saida = Vec::new();
        for l in self.gmail_execucoes.iter().filter(|l| l.origem == "bun") {
            if let Some(r) = self
                .gmail_execucoes
                .iter()
                .find(|r| r.origem == "rust" && r.dia == l.dia)
                && r.anexos_encontrados != l.anexos_encontrados
            {
                saida.push((l.dia.clone(), l.anexos_encontrados, r.anexos_encontrados));
            }
        }
        saida
    }
}

impl fmt::Display for Relatorio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Comparação das coletas (últimos {} dias; somente leitura)",
            self.dias
        )?;
        writeln!(f, "\n== Gmail: execuções por dia e origem ==")?;
        writeln!(
            f,
            "dia         origem  execuções  mensagens  anexos_achados  jobs_criados  já_processados  erros"
        )?;
        for l in &self.gmail_execucoes {
            writeln!(
                f,
                "{}  {:<6}  {:>9}  {:>9}  {:>14}  {:>12}  {:>14}  {:>5}",
                l.dia,
                l.origem,
                l.execucoes,
                l.mensagens,
                l.anexos_encontrados,
                l.jobs_criados,
                l.ja_processados,
                l.erros
            )?;
        }
        let divergencias = self.divergencias_do_gmail();
        if divergencias.is_empty() {
            writeln!(
                f,
                "anexos achados: sem divergência nos dias em que os dois rodaram"
            )?;
        }
        for (dia, bun, rust) in &divergencias {
            writeln!(
                f,
                "DIVERGE em {dia}: bun achou {bun} anexo(s), rust achou {rust}"
            )?;
        }
        writeln!(f, "documentos do Gmail no banco: {}", self.gmail_documentos)?;
        writeln!(
            f,
            "documentos do Gmail sem job: {}",
            self.gmail_documentos_sem_job
        )?;
        writeln!(f, "\n== Pluggy: drafts criados por dia ==")?;
        for (dia, n) in &self.pluggy_drafts_por_dia {
            writeln!(f, "{dia}  {n}")?;
        }
        writeln!(f, "drafts da Pluggy no total: {}", self.pluggy_drafts)?;
        writeln!(f, "\n== Checkpoints ==")?;
        for c in &self.checkpoints {
            writeln!(
                f,
                "{}  {}  {}  cursor={}  vistos={} criados={} erros={}  em {}",
                c.origem,
                c.escopo,
                c.situacao,
                c.cursor,
                c.vistos,
                c.criados,
                c.erros,
                c.atualizado_em
            )?;
        }
        writeln!(f, "\n== Conexões Pluggy: última sincronização ==")?;
        for (conexao, quando) in &self.conexoes {
            writeln!(f, "{conexao}  {quando}")?;
        }
        Ok(())
    }
}

/// Monta o relatório dos últimos `dias` dias do dono.
pub async fn comparar(banco: &Pool, dono: Uuid, dias: i64) -> Result<Relatorio, ErroDeColeta> {
    let mut cliente = banco.get().await?;
    let tx = cliente.build_transaction().read_only(true).start().await?;
    let mut r = Relatorio {
        dias,
        ..Default::default()
    };
    let numero = |l: &tokio_postgres::Row, c: &str| -> Result<i64, ErroDeColeta> {
        Ok(l.try_get::<_, i64>(c)?)
    };
    for l in tx
        .query(
            "SELECT to_char(started_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS dia, \
             CASE WHEN metadata->>'origem' = 'coletor-vps' THEN 'rust' ELSE 'bun' END AS origem, \
             count(*) AS execucoes, \
             coalesce(sum((stats->>'messagesListed')::bigint), 0)::bigint AS mensagens, \
             coalesce(sum((stats->>'attachmentsFound')::bigint), 0)::bigint AS achados, \
             coalesce(sum((stats->>'jobsCreated')::bigint), 0)::bigint AS jobs, \
             coalesce(sum((stats->>'skippedAlreadyProcessed')::bigint), 0)::bigint AS pulados, \
             coalesce(sum((stats->>'errors')::bigint), 0)::bigint AS erros \
             FROM public.ingestion_runs \
             WHERE user_id = $1 AND source_type = 'gmail' \
               AND started_at >= now() - ($2::bigint * interval '1 day') \
             GROUP BY 1, 2 ORDER BY 1, 2",
            &[&dono, &dias],
        )
        .await?
    {
        r.gmail_execucoes.push(LinhaDeExecucao {
            dia: l.try_get("dia")?,
            origem: l.try_get("origem")?,
            execucoes: numero(&l, "execucoes")?,
            mensagens: numero(&l, "mensagens")?,
            anexos_encontrados: numero(&l, "achados")?,
            jobs_criados: numero(&l, "jobs")?,
            ja_processados: numero(&l, "pulados")?,
            erros: numero(&l, "erros")?,
        });
    }
    let l = tx
        .query_one(
            "SELECT count(*) AS docs, count(*) FILTER (WHERE NOT EXISTS \
             (SELECT 1 FROM public.ingestion_jobs j WHERE j.source_document_id = d.id)) AS sem_job \
             FROM public.source_documents d WHERE d.user_id = $1 AND d.origin_type = 'gmail'",
            &[&dono],
        )
        .await?;
    r.gmail_documentos = numero(&l, "docs")?;
    r.gmail_documentos_sem_job = numero(&l, "sem_job")?;
    for l in tx
        .query(
            "SELECT to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS dia, count(*) AS n \
             FROM public.draft_records WHERE user_id = $1 AND external_ref LIKE 'pluggy:txn\\_%' \
               AND created_at >= now() - ($2::bigint * interval '1 day') GROUP BY 1 ORDER BY 1",
            &[&dono, &dias],
        )
        .await?
    {
        r.pluggy_drafts_por_dia
            .push((l.try_get("dia")?, numero(&l, "n")?));
    }
    r.pluggy_drafts = numero(
        &tx.query_one(
            "SELECT count(*) AS n FROM public.draft_records WHERE user_id = $1 AND external_ref LIKE 'pluggy:txn\\_%'",
            &[&dono],
        )
        .await?,
        "n",
    )?;
    for l in tx
        .query(
            "SELECT source_type, scope_key, status, coalesce(cursor_value, '-') AS cursor, \
             messages_seen, documents_created, errors, \
             to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS em \
             FROM public.ingestion_checkpoints WHERE user_id = $1 AND source_type IN ('gmail', 'pluggy') \
             ORDER BY source_type, scope_key",
            &[&dono],
        )
        .await?
    {
        r.checkpoints.push(LinhaDeCheckpoint {
            origem: l.try_get("source_type")?,
            escopo: l.try_get("scope_key")?,
            situacao: l.try_get("status")?,
            cursor: l.try_get("cursor")?,
            vistos: l.try_get("messages_seen")?,
            criados: l.try_get("documents_created")?,
            erros: l.try_get("errors")?,
            atualizado_em: l.try_get("em")?,
        });
    }
    for l in tx
        .query(
            "SELECT external_item_id, \
             coalesce(to_char(last_synced_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), 'nunca') AS quando \
             FROM public.provider_connections WHERE user_id = $1 AND provider = 'pluggy' ORDER BY created_at, id",
            &[&dono],
        )
        .await?
    {
        r.conexoes.push((l.try_get("external_item_id")?, l.try_get("quando")?));
    }
    tx.rollback().await?;
    Ok(r)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linha(dia: &str, origem: &str, achados: i64) -> LinhaDeExecucao {
        LinhaDeExecucao {
            dia: dia.into(),
            origem: origem.into(),
            anexos_encontrados: achados,
            ..Default::default()
        }
    }

    #[test]
    fn diverge_so_quando_os_dois_rodaram_e_discordam() {
        let r = Relatorio {
            gmail_execucoes: vec![
                linha("2026-11-14", "bun", 5),
                linha("2026-11-14", "rust", 5),
                linha("2026-11-15", "bun", 7),
                linha("2026-11-15", "rust", 6),
                linha("2026-11-16", "bun", 3),
            ],
            ..Default::default()
        };
        assert_eq!(
            r.divergencias_do_gmail(),
            vec![("2026-11-15".to_owned(), 7, 6)]
        );
        let texto = r.to_string();
        assert!(texto.contains("DIVERGE em 2026-11-15: bun achou 7 anexo(s), rust achou 6"));
    }
}
