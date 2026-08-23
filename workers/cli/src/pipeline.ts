/**
 * Orquestração do pipeline, em processo.
 *
 * A versão anterior era um wrapper de `spawnSync` sobre os scanners. Isso
 * descartava estatísticas estruturadas, impedia E2E em processo e — o pior —
 * criava uma fronteira de argv onde as flags se perdiam sem aviso.
 *
 * `--process` agora faz o que sempre prometeu: drena a fila de
 * `ingestion_jobs` chamando `processJob` diretamente.
 */
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus } from "@sbf/ingestion-types";
import { processJob } from "@sbf/worker-ingestion";
import { scanDirectory } from "@sbf/worker-local-scanner";
import { scanGmailLabel } from "@sbf/worker-gmail-scanner";
import type { CliOptions } from "./args";

/** Status a partir dos quais um job ainda avança na máquina de estados. */
const PROCESSABLE: string[] = [
  IngestionJobStatus.DISCOVERED,
  IngestionJobStatus.DOWNLOADED,
  IngestionJobStatus.HASHED,
  IngestionJobStatus.QUEUED,
  IngestionJobStatus.PARSED,
];

export interface PipelineStats {
  gmailDiscovered: number;
  localDiscovered: number;
  jobsProcessed: number;
  errors: string[];
}

export function createSupabase(): SupabaseClient {
  const url = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_SECRET_KEY;
  if (!url || !key) {
    throw new Error("SUPABASE_URL e SUPABASE_SECRET_KEY são obrigatórios");
  }
  return createClient(url, key, {
    auth: { autoRefreshToken: false, persistSession: false },
  });
}

/**
 * Drena a fila até não sobrar job processável ou atingir o teto.
 *
 * O teto existe para que uma execução não vire um loop infinito quando um job
 * volta ao mesmo status por erro persistente.
 */
export async function drainJobs(
  supabase: SupabaseClient,
  opts: { maxJobs: number; batchSize: number; verbose: boolean },
): Promise<number> {
  let processed = 0;

  while (processed < opts.maxJobs) {
    const { data: jobs, error } = await supabase
      .from("ingestion_jobs")
      .select("id, run_id, user_id, source_document_id, status, retry_count, max_retries, metadata")
      .in("status", PROCESSABLE)
      .order("created_at", { ascending: true })
      .limit(Math.min(opts.batchSize, opts.maxJobs - processed));

    if (error) throw new Error(`Falha ao ler a fila: ${error.message}`);
    if (!jobs || jobs.length === 0) break;

    for (const job of jobs) {
      if (processed >= opts.maxJobs) break;
      if (opts.verbose) console.log(`  [job] ${job.id} (${job.status})`);
      await processJob(supabase, job);
      processed++;
    }
  }

  return processed;
}

/** Executa scan e, se pedido, processamento. */
export async function runPipeline(opts: CliOptions): Promise<PipelineStats> {
  const stats: PipelineStats = {
    gmailDiscovered: 0,
    localDiscovered: 0,
    jobsProcessed: 0,
    errors: [],
  };

  if (opts.userId) process.env.LOCAL_USER_ID = opts.userId;

  const supabase = createSupabase();

  if (opts.source === "gmail" || opts.source === "both") {
    try {
      const result = await scanGmailLabel({
        label: opts.label,
        query: opts.query ?? "",
        fromDate: opts.fromDate,
        toDate: opts.toDate,
        limit: opts.limit,
        dryRun: opts.dryRun,
        batchSize: opts.batchSize,
        includeBody: opts.includeBody,
        includeAttachments: opts.includeAttachments,
      });
      stats.gmailDiscovered = result.jobsCreated ?? 0;
    } catch (err) {
      // Uma fonte que falha não deve impedir a outra de rodar.
      stats.errors.push(`gmail: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  if (opts.source === "local" || opts.source === "both") {
    try {
      stats.localDiscovered = await scanDirectory(supabase, opts.dir ?? "./inbox", {
        recursive: opts.recursive,
        dryRun: opts.dryRun,
        verbose: opts.verbose,
        moveProcessedTo: opts.moveProcessedTo ?? undefined,
      });
    } catch (err) {
      stats.errors.push(`local: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  if (opts.process && !opts.dryRun) {
    stats.jobsProcessed = await drainJobs(supabase, {
      maxJobs: opts.maxJobs,
      batchSize: opts.batchSize,
      verbose: opts.verbose,
    });
  }

  return stats;
}
