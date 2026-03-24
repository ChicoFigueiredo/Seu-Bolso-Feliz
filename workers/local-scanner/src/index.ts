/**
 * @sbf/worker-local-scanner — Entry point
 * Escaneia diretório local por documentos financeiros (PDF, CSV, imagens),
 * faz upload para Supabase Storage e cria source_documents + ingestion_jobs.
 */
import { getSupabaseClient } from "./supabase";

export { scanDirectory } from "./scanner";

const SCAN_INTERVAL_MS = Number(process.env.SCAN_INTERVAL_MS ?? 30000);
const WATCH_DIR = process.env.WATCH_DIR ?? "./inbox";

let running = true;

async function main(): Promise<void> {
  const { scanDirectory } = await import("./scanner");

  console.log(`[SCANNER] Local scanner starting — watching: ${WATCH_DIR}`);
  console.log(`[SCANNER] Scan interval: ${SCAN_INTERVAL_MS}ms`);

  process.on("SIGTERM", () => {
    running = false;
  });
  process.on("SIGINT", () => {
    running = false;
  });

  while (running) {
    try {
      const supabase = getSupabaseClient();
      const count = await scanDirectory(supabase, WATCH_DIR);
      if (count > 0) {
        console.log(`[SCANNER] Discovered ${count} new documents`);
      }
    } catch (err) {
      console.error("[SCANNER] Scan error:", err);
    }
    await new Promise((resolve) => setTimeout(resolve, SCAN_INTERVAL_MS));
  }

  console.log("[SCANNER] Scanner stopped.");
}

if (import.meta.main) {
  main();
}
