/**
 * @sbf/worker-ingestion — Entry point
 *
 * Consome a fila de jobs do próprio Postgres: reivindica um job com
 * `FOR UPDATE SKIP LOCKED`, processa, e volta a dormir escutando
 * `NOTIFY sbf_jobs` (com polling de reserva). Não há porta aberta, nem
 * chamada HTTP, nem SSH — a Vercel grava a linha, este processo a pega.
 *
 * O que mudou em relação ao poll loop anterior:
 *
 * - **Concorrência real.** Antes, dois processos deste worker liam os mesmos
 *   jobs do mesmo `SELECT ... LIMIT 10` e processavam tudo em duplicidade.
 *   Agora o claim é atômico e um job pertence a um worker de cada vez.
 * - **Retentativa de verdade.** `retry_count`/`max_retries` existiam sem
 *   ninguém os consultar: qualquer erro transitório aposentava o documento.
 *   Agora a fila devolve o job com backoff exponencial até esgotar.
 * - **Latência.** O poll de 5 s era o piso do tempo de resposta mesmo com a
 *   fila vazia e um job recém-criado; o `NOTIFY` derruba isso para o tempo
 *   de ida e volta ao banco.
 *
 * Capacidades: este worker roda na VPS, onde o `ocrmypdf` está instalado, e
 * por isso anuncia `ocr` por padrão. `WORKER_CAPABILITIES` sobrescreve —
 * numa máquina sem OCR, rodar com `WORKER_CAPABILITIES=` faz o worker
 * simplesmente não reivindicar os jobs que exigem OCR, em vez de falhar neles.
 */
import { createShutdownSignal, getDb, resolveCapabilities, runWorkerLoop } from "@sbf/db";
import { processJob } from "./processor";

export { isValidTransition, getValidNextStates } from "./state-machine";
export { processJob } from "./processor";
export { getIngestionBucket, INGESTION_BUCKET } from "./storage";

/** Capacidades padrão do worker de ingestão na VPS. */
export const DEFAULT_CAPABILITIES = ["ocr"] as const;

const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS ?? 5000);
/**
 * Um documento pode passar por OCR e duas chamadas de IA — minutos, não
 * segundos. O reap precisa de folga bem acima disso, senão devolve à fila
 * trabalho que ainda está em andamento e o documento é processado duas vezes.
 */
const STALE_AFTER_MS = Number(process.env.JOB_STALE_AFTER_MS ?? 30 * 60_000);

async function main(): Promise<void> {
  const db = getDb();

  await runWorkerLoop({
    name: "ingestion",
    capabilities: resolveCapabilities(DEFAULT_CAPABILITIES),
    signal: createShutdownSignal(),
    pollIntervalMs: POLL_INTERVAL_MS,
    staleAfterMs: STALE_AFTER_MS,
    db,
    handle: async (job) => {
      await processJob(db, job);
      // O processador já avançou a máquina de estados até onde deu; a fila só
      // solta o lock. Sobrescrever o status aqui apagaria essa informação.
    },
  });
}

// Só executa o loop se este arquivo for o entry point
if (import.meta.main) {
  main().catch((err: unknown) => {
    console.error("[WORKER] Falha fatal:", err);
    process.exit(1);
  });
}
