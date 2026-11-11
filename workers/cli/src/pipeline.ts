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
import {
  CLAIMABLE_STATUSES,
  claimJobs,
  completeJob,
  failJob,
  getDb,
  releaseJob,
  resolveCapabilities,
  type Database,
  type QueueJob,
} from "@sbf/db";
import { processJob } from "@sbf/worker-ingestion";
import { scanDirectory } from "@sbf/worker-local-scanner";
import { scanGmailLabel } from "@sbf/worker-gmail-scanner";
import type { CliOptions } from "./args";

/** Status a partir dos quais um job ainda avança na máquina de estados. */
const PROCESSABLE: readonly string[] = CLAIMABLE_STATUSES;

/**
 * Capacidades do CLI.
 *
 * Ele roda na máquina do desenvolvedor (WSL), que é onde a pasta local, o
 * `ocrmypdf` e as senhas de PDF estão — logo, reivindica de tudo.
 */
export const DEFAULT_CLI_CAPABILITIES = ["ocr", "pasta-local", "senha-pdf"] as const;

export interface PipelineStats {
  gmailDiscovered: number;
  localDiscovered: number;
  jobsProcessed: number;
  errors: string[];
}

/**
 * Drena a fila até não sobrar job processável ou atingir o teto.
 *
 * O teto existe para que uma execução não vire um loop infinito quando um job
 * volta ao mesmo status por erro persistente.
 *
 * Diferente do loop antigo, cada job é REIVINDICADO antes de processar: rodar
 * o CLI enquanto o worker da VPS está de pé deixou de significar processar
 * tudo em duplicidade.
 */
export async function drainJobs(
  db: Database,
  opts: { maxJobs: number; batchSize: number; verbose: boolean },
): Promise<number> {
  const workerId = `cli:${process.pid}`;
  const capabilities = resolveCapabilities(DEFAULT_CLI_CAPABILITIES);
  let processed = 0;

  while (processed < opts.maxJobs) {
    const jobs: QueueJob[] = await claimJobs({
      workerId,
      capabilities,
      statuses: PROCESSABLE,
      limit: Math.min(opts.batchSize, opts.maxJobs - processed),
      db,
    });

    if (jobs.length === 0) break;

    for (const job of jobs) {
      if (processed >= opts.maxJobs) {
        // Sobrou job reivindicado que não vai ser processado nesta execução:
        // devolver é obrigatório, senão ele fica travado até o reap.
        await releaseJob({ jobId: job.id, expectedWorkerId: workerId, db });
        continue;
      }
      if (opts.verbose) console.log(`  [job] ${job.id} (${job.status})`);
      try {
        await processJob(db, job);
        await completeJob({ jobId: job.id, expectedWorkerId: workerId, db });
      } catch (err) {
        await failJob({
          jobId: job.id,
          error: err instanceof Error ? err : String(err),
          expectedWorkerId: workerId,
          db,
        });
        if (opts.verbose) {
          console.error(`  [job] ${job.id} falhou:`, err instanceof Error ? err.message : err);
        }
      }
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

  const db = getDb();

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
      stats.localDiscovered = await scanDirectory(db, opts.dir ?? "./inbox", {
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
    stats.jobsProcessed = await drainJobs(db, {
      maxJobs: opts.maxJobs,
      batchSize: opts.batchSize,
      verbose: opts.verbose,
    });
  }

  return stats;
}
