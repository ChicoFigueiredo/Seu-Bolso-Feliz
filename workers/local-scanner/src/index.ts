/**
 * @sbf/worker-local-scanner — Entry point
 *
 * Escaneia diretório local por documentos financeiros, envia os arquivos para
 * o storage S3-compatível e cria source_documents + ingestion_jobs.
 *
 * Uso:
 *   bun run workers/local-scanner/src/index.ts --dir ./inbox --scan-once
 *   bun run workers/local-scanner/src/index.ts --dir ./inbox --watch
 *   bun run workers/local-scanner/src/index.ts --dir ./inbox --recursive --dry-run
 */
import { createShutdownSignal, getDb, resolveCapabilities, sleep } from "@sbf/db";
import { scanDirectory } from "./scanner";

export { scanDirectory } from "./scanner";

/**
 * Capacidades do host quando `WORKER_CAPABILITIES` não diz nada.
 *
 * Descreve a máquina WSL do dia a dia, que é onde a pasta vigiada está
 * montada: ela tem o `ocrmypdf` instalado, enxerga o diretório local e conhece
 * as senhas de PDF. É o padrão certo justamente porque quem roda o scanner
 * local é, quase sempre, quem também processa o que ele descobre.
 */
const CAPACIDADES_PADRAO = ["ocr", "pasta-local", "senha-pdf"] as const;

// ─── CLI Options ──────────────────────────────────────────────

interface LocalScanOptions {
  dir: string;
  recursive: boolean;
  scanOnce: boolean;
  watch: boolean;
  intervalMs: number;
  extensions: string;
  dryRun: boolean;
  verbose: boolean;
  moveProcessedTo: string;
  userId: string;
}

function parseArgs(): LocalScanOptions {
  const args = process.argv.slice(2);
  const opts: LocalScanOptions = {
    dir: process.env.WATCH_DIR ?? "./inbox",
    recursive: false,
    scanOnce: false,
    watch: false,
    intervalMs: Number(process.env.SCAN_INTERVAL_MS ?? 30000),
    extensions: "",
    dryRun: false,
    verbose: false,
    moveProcessedTo: "",
    userId: "",
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--dir":
        opts.dir = args[++i] ?? opts.dir;
        break;
      case "--recursive":
        opts.recursive = true;
        break;
      case "--scan-once":
        opts.scanOnce = true;
        break;
      case "--watch":
        opts.watch = true;
        break;
      case "--interval-ms":
        opts.intervalMs = Number(args[++i]) || 30000;
        break;
      case "--extensions":
        opts.extensions = args[++i] ?? "";
        break;
      case "--dry-run":
        opts.dryRun = true;
        break;
      case "--verbose":
        opts.verbose = true;
        break;
      case "--move-processed-to":
        opts.moveProcessedTo = args[++i] ?? "";
        break;
      case "--user-id":
        opts.userId = args[++i] ?? "";
        break;
      case "--help":
        printHelp();
        process.exit(0);
    }
  }

  // Default: scan-once if neither watch nor scan-once specified
  if (!opts.watch && !opts.scanOnce) {
    opts.scanOnce = true;
  }

  return opts;
}

function printHelp(): void {
  console.log(`
Local Scanner — Escaneia diretório local e cria jobs de ingestão

Uso:
  bun run workers/local-scanner/src/index.ts [opções]

Opções:
  --dir <caminho>          Diretório a escanear (padrão: ./inbox ou WATCH_DIR)
  --recursive              Varrer subdiretórios
  --scan-once              Escanear uma vez e sair (padrão quando sem --watch)
  --watch                  Modo watch: loop contínuo
  --interval-ms <ms>       Intervalo no modo watch (padrão: 30000)
  --extensions <list>      Extensões aceitas separadas por vírgula (ex: .pdf,.csv)
  --dry-run                Listar arquivos sem criar registros
  --verbose                Saída detalhada por arquivo
  --move-processed-to <p>  Mover arquivos processados para este diretório
  --user-id <uuid>         ID do usuário (sobrescreve LOCAL_USER_ID)
  --help                   Exibir esta ajuda
`);
}

// ─── Main ─────────────────────────────────────────────────────

async function runScan(opts: LocalScanOptions, capabilities: readonly string[]): Promise<number> {
  const db = getDb();

  const extensions =
    opts.extensions.length > 0
      ? new Set(
          opts.extensions
            .split(",")
            .map((e) => (e.startsWith(".") ? e.toLowerCase() : `.${e.toLowerCase()}`)),
        )
      : undefined;

  const count = await scanDirectory(db, opts.dir, {
    recursive: opts.recursive,
    extensions,
    dryRun: opts.dryRun,
    verbose: opts.verbose,
    moveProcessedTo: opts.moveProcessedTo || undefined,
    capabilities,
  });

  return count;
}

async function main(): Promise<void> {
  const opts = parseArgs();

  // Override LOCAL_USER_ID if provided via CLI
  if (opts.userId) {
    process.env.LOCAL_USER_ID = opts.userId;
  }

  console.log(`[SCANNER] Local scanner — dir: ${opts.dir}`);
  console.log(`[SCANNER] Modo: ${opts.dryRun ? "DRY RUN" : opts.watch ? "WATCH" : "SCAN-ONCE"}`);

  // As capacidades deste host decidem se os jobs criados aqui têm quem os
  // reivindique — por isso vão para o log logo de cara, antes de qualquer scan.
  const capabilities = resolveCapabilities(CAPACIDADES_PADRAO);
  console.log(`[SCANNER] Capacidades do host: [${capabilities.join(", ")}]`);
  if (opts.recursive) console.log(`[SCANNER] Recursivo: sim`);
  if (opts.extensions) console.log(`[SCANNER] Extensões: ${opts.extensions}`);
  if (opts.moveProcessedTo) console.log(`[SCANNER] Mover para: ${opts.moveProcessedTo}`);

  if (opts.watch) {
    // `createShutdownSignal` também acorda o `sleep` do intervalo: com os
    // handlers manuais anteriores, um SIGTERM logo depois de um scan só era
    // notado ao fim da espera inteira (até 30 s parado sem motivo).
    const signal = createShutdownSignal();

    while (!signal.aborted) {
      try {
        const count = await runScan(opts, capabilities);
        if (count > 0 || opts.verbose) {
          console.log(`[SCANNER] Descobertos: ${count} documento(s)`);
        }
      } catch (err) {
        console.error("[SCANNER] Erro no scan:", err instanceof Error ? err.message : err);
      }
      await sleep(opts.intervalMs, signal);
    }
    console.log("[SCANNER] Scanner encerrado.");
  } else {
    // scan-once (default)
    const count = await runScan(opts, capabilities);
    console.log(`[SCANNER] Concluído: ${count} documento(s) descoberto(s).`);
  }
}

if (import.meta.main) {
  main();
}
