/**
 * @sbf/worker-ingestion — Entry point
 * Poll loop: busca jobs pendentes e processa sequencialmente.
 * Roda como processo Bun standalone.
 */
import { IngestionJobStatus } from "@sbf/ingestion-types";
import { getSupabaseClient } from "./supabase";
import { processJob } from "./processor";

export { isValidTransition, getValidNextStates } from "./state-machine";
export { processJob } from "./processor";

const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS ?? 5000);
const BATCH_SIZE = Number(process.env.BATCH_SIZE ?? 10);

let running = true;

async function pollAndProcess(): Promise<number> {
  const supabase = getSupabaseClient();

  // Buscar jobs que precisam de processamento
  const { data: jobs, error } = await supabase
    .from("ingestion_jobs")
    .select("id, run_id, user_id, source_document_id, status, retry_count, max_retries, metadata")
    .in("status", [
      IngestionJobStatus.DISCOVERED,
      IngestionJobStatus.DOWNLOADED,
      IngestionJobStatus.HASHED,
      IngestionJobStatus.QUEUED,
      IngestionJobStatus.PARSED,
    ])
    .order("created_at", { ascending: true })
    .limit(BATCH_SIZE);

  if (error) {
    console.error("[WORKER] Poll error:", error.message);
    return 0;
  }

  if (!jobs || jobs.length === 0) return 0;

  console.log(`[WORKER] Processing ${jobs.length} jobs...`);

  let processed = 0;
  for (const job of jobs) {
    if (!running) break;
    await processJob(supabase, job);
    processed++;
  }

  return processed;
}

async function main(): Promise<void> {
  console.log("[WORKER] Ingestion worker starting...");
  console.log(`[WORKER] Poll interval: ${POLL_INTERVAL_MS}ms, batch size: ${BATCH_SIZE}`);

  // Graceful shutdown
  process.on("SIGTERM", () => {
    console.log("[WORKER] SIGTERM received, shutting down...");
    running = false;
  });
  process.on("SIGINT", () => {
    console.log("[WORKER] SIGINT received, shutting down...");
    running = false;
  });

  while (running) {
    try {
      const count = await pollAndProcess();
      if (count === 0) {
        await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
      }
    } catch (err) {
      console.error("[WORKER] Unexpected error:", err);
      await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
    }
  }

  console.log("[WORKER] Worker stopped.");
}

// Só executa o loop se este arquivo for o entry point
if (import.meta.main) {
  main();
}
