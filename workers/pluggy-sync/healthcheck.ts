/**
 * HEALTHCHECK do container — confirma que o loop de docker-entrypoint.sh
 * ainda está vivo (heartbeat recente), não que o último sync teve sucesso.
 * Ver docker-entrypoint.sh para a decisão de design.
 */
import { statSync } from "node:fs";

const heartbeatFile = process.env.HEARTBEAT_FILE ?? "/tmp/pluggy-sync-heartbeat";
const syncIntervalSeconds = Number(process.env.SYNC_INTERVAL_SECONDS ?? 3600);
const maxAgeSeconds = Number(process.env.HEARTBEAT_MAX_AGE_SECONDS ?? syncIntervalSeconds * 2);

try {
  const { mtimeMs } = statSync(heartbeatFile);
  const ageSeconds = (Date.now() - mtimeMs) / 1000;
  if (ageSeconds > maxAgeSeconds) {
    console.error(`heartbeat obsoleto: ${Math.round(ageSeconds)}s (máximo ${maxAgeSeconds}s)`);
    process.exit(1);
  }
  console.log(`heartbeat ok: ${Math.round(ageSeconds)}s atrás`);
} catch (err) {
  console.error(`heartbeat ausente: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
