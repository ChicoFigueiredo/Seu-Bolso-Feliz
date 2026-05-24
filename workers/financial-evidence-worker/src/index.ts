/**
 * @sbf/worker-financial-evidence — Orquestrador de evidências financeiras
 *
 * CLI unificada que orquestra adapters de Gmail e pasta local,
 * reutilizando workers existentes sem duplicar código.
 *
 * Uso:
 *   bun run workers/financial-evidence-worker/src/index.ts --gmail --label Comprovantes
 *   bun run workers/financial-evidence-worker/src/index.ts --local --dir ./inbox
 *   bun run workers/financial-evidence-worker/src/index.ts --gmail --local --dry-run
 *   bun run workers/financial-evidence-worker/src/index.ts --help
 */

import { spawnSync } from "bun";
import { resolve } from "node:path";

// Caminho raiz do monorepo (3 níveis acima deste arquivo)
const REPO_ROOT = resolve(import.meta.dir, "../../../");

interface OrchestratorOptions {
  // Fontes
  gmail: boolean;
  local: boolean;

  // Gmail
  gmailLabel: string;
  gmailQuery: string;
  gmailFromDate: string;
  gmailToDate: string;
  gmailLimit: number;
  gmailBatchSize: number;
  includeBody: boolean;
  includeAttachments: boolean;

  // Local
  localDir: string;
  localRecursive: boolean;
  localScanOnce: boolean;
  localWatch: boolean;
  localIntervalMs: number;
  localExtensions: string;
  moveProcessedTo: string;

  // Compartilhado
  dryRun: boolean;
  process: boolean;
  aiMode: "auto" | "skip" | "lite" | "full";
  userId: string;
  verbose: boolean;
}

function parseArgs(): OrchestratorOptions {
  const args = process.argv.slice(2);
  const opts: OrchestratorOptions = {
    gmail: false,
    local: false,
    gmailLabel: "",
    gmailQuery: "",
    gmailFromDate: "",
    gmailToDate: "",
    gmailLimit: 0,
    gmailBatchSize: 50,
    includeBody: false,
    includeAttachments: true,
    localDir: "./inbox",
    localRecursive: false,
    localScanOnce: false,
    localWatch: false,
    localIntervalMs: 30000,
    localExtensions: "",
    moveProcessedTo: "",
    dryRun: false,
    process: false,
    aiMode: "auto",
    userId: "",
    verbose: false,
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--gmail":
        opts.gmail = true;
        break;
      case "--local":
        opts.local = true;
        break;
      case "--label":
        opts.gmailLabel = args[++i] ?? "";
        break;
      case "--query":
        opts.gmailQuery = args[++i] ?? "";
        break;
      case "--from-date":
        opts.gmailFromDate = args[++i] ?? "";
        break;
      case "--to-date":
        opts.gmailToDate = args[++i] ?? "";
        break;
      case "--limit":
        opts.gmailLimit = Number(args[++i]) || 0;
        break;
      case "--batch-size":
        opts.gmailBatchSize = Number(args[++i]) || 50;
        break;
      case "--include-body":
        opts.includeBody = true;
        break;
      case "--include-attachments":
        opts.includeAttachments = true;
        break;
      case "--dir":
        opts.localDir = args[++i] ?? "./inbox";
        break;
      case "--recursive":
        opts.localRecursive = true;
        break;
      case "--scan-once":
        opts.localScanOnce = true;
        break;
      case "--watch":
        opts.localWatch = true;
        break;
      case "--interval-ms":
        opts.localIntervalMs = Number(args[++i]) || 30000;
        break;
      case "--extensions":
        opts.localExtensions = args[++i] ?? "";
        break;
      case "--move-processed-to":
        opts.moveProcessedTo = args[++i] ?? "";
        break;
      case "--dry-run":
        opts.dryRun = true;
        break;
      case "--process":
        opts.process = true;
        break;
      case "--ai-mode":
        opts.aiMode = (args[++i] ?? "auto") as OrchestratorOptions["aiMode"];
        break;
      case "--user-id":
        opts.userId = args[++i] ?? "";
        break;
      case "--verbose":
        opts.verbose = true;
        break;
      case "--help":
        printHelp();
        process.exit(0);
    }
  }

  // Se nenhuma fonte especificada, ativar ambas
  if (!opts.gmail && !opts.local) {
    opts.gmail = true;
    opts.local = true;
  }

  return opts;
}

function printHelp(): void {
  console.log(`
Financial Evidence Worker — Orquestrador unificado de evidências financeiras

Uso:
  bun run workers/financial-evidence-worker/src/index.ts [opções]

Fontes:
  --gmail                Escanear Gmail
  --local                Escanear pasta local
  (sem --gmail/--local)  Escanear ambas

Gmail:
  --label <nome>         Label do Gmail (ex: Comprovantes)
  --query <q>            Query Gmail (ex: 'from:nubank newer_than:30d')
  --from-date <data>     Data inicial (YYYY-MM-DD)
  --to-date <data>       Data final (YYYY-MM-DD)
  --limit <n>            Máximo de mensagens
  --batch-size <n>       Tamanho do batch (padrão: 50)
  --include-body         Processar corpo dos emails (além de anexos)
  --include-attachments  Processar anexos (padrão: true)

Local:
  --dir <caminho>        Diretório a escanear (padrão: ./inbox)
  --recursive            Varrer subdiretórios
  --scan-once            Escanear uma vez e sair
  --watch                Modo watch (loop contínuo)
  --interval-ms <ms>     Intervalo no modo watch (padrão: 30000)
  --extensions <list>    Extensões aceitas (ex: .pdf,.csv)
  --move-processed-to    Mover arquivos após sucesso

Compartilhado:
  --dry-run              Simular sem criar registros
  --process              Processar jobs após scan
  --ai-mode auto|skip|lite|full  Modo de IA (padrão: auto)
  --user-id <uuid>       ID do usuário (sobrescreve LOCAL_USER_ID)
  --verbose              Saída detalhada
  --help                 Exibir esta ajuda
`);
}

function buildGmailArgs(opts: OrchestratorOptions): string[] {
  const args: string[] = [];
  if (opts.gmailLabel) args.push("--label", opts.gmailLabel);
  if (opts.gmailQuery) args.push("--query", opts.gmailQuery);
  if (opts.gmailFromDate) args.push("--from-date", opts.gmailFromDate);
  if (opts.gmailToDate) args.push("--to-date", opts.gmailToDate);
  if (opts.gmailLimit > 0) args.push("--limit", String(opts.gmailLimit));
  if (opts.gmailBatchSize !== 50) args.push("--batch-size", String(opts.gmailBatchSize));
  if (opts.includeBody) args.push("--include-body");
  if (opts.dryRun) args.push("--dry-run");
  if (opts.verbose) args.push("--verbose");
  return args;
}

function buildLocalArgs(opts: OrchestratorOptions): string[] {
  const args: string[] = [];
  args.push("--dir", opts.localDir);
  if (opts.localRecursive) args.push("--recursive");
  if (opts.localScanOnce) args.push("--scan-once");
  if (opts.localWatch) args.push("--watch");
  if (opts.localIntervalMs !== 30000) args.push("--interval-ms", String(opts.localIntervalMs));
  if (opts.localExtensions) args.push("--extensions", opts.localExtensions);
  if (opts.moveProcessedTo) args.push("--move-processed-to", opts.moveProcessedTo);
  if (opts.dryRun) args.push("--dry-run");
  if (opts.process) args.push("--process");
  if (opts.verbose) args.push("--verbose");
  return args;
}

function runWorker(scriptPath: string, args: string[], label: string): boolean {
  console.log(`\n${"═".repeat(60)}`);
  console.log(`▶  ${label}`);
  console.log(`${"═".repeat(60)}\n`);

  const result = spawnSync(["bun", "run", scriptPath, ...args], {
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
    cwd: REPO_ROOT,
  });

  const success = result.exitCode === 0;
  if (!success) {
    console.error(`\n❌ ${label} terminou com código ${result.exitCode}`);
  }
  return success;
}

async function main(): Promise<void> {
  const opts = parseArgs();

  console.log("\n🏦 Financial Evidence Worker");
  console.log(`   Modo: ${opts.dryRun ? "DRY RUN" : "REAL"}`);
  console.log(
    `   Fontes: ${[opts.gmail && "Gmail", opts.local && "Local"].filter(Boolean).join(", ")}`,
  );

  const gmailScript = resolve(REPO_ROOT, "workers/gmail-scanner/src/index.ts");
  const localScript = resolve(REPO_ROOT, "workers/local-scanner/src/index.ts");

  let overallSuccess = true;

  if (opts.gmail) {
    const args = buildGmailArgs(opts);
    const ok = runWorker(gmailScript, args, "Gmail Scanner");
    if (!ok) overallSuccess = false;
  }

  if (opts.local) {
    const args = buildLocalArgs(opts);
    const ok = runWorker(localScript, args, "Local Scanner");
    if (!ok) overallSuccess = false;
  }

  console.log(`\n${"═".repeat(60)}`);
  if (overallSuccess) {
    console.log("✅ Financial Evidence Worker concluído com sucesso");
  } else {
    console.log("⚠️  Financial Evidence Worker concluído com erros");
    process.exit(1);
  }
}

main();
