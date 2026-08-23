/**
 * Parser único de argumentos do pipeline.
 *
 * Existe porque `financial-evidence-worker` declarava --from-date, --to-date,
 * --process, --ai-mode e --user-id, e nenhum deles chegava ao destino: os dois
 * scanners filhos tinham `switch` sem `default`, então engoliam em silêncio
 * qualquer flag que não conhecessem. O `--help` anunciava suporte a tudo isso.
 *
 * Aqui o parser é puro e testado, e "a flag chega ao destino" vira asserção em
 * vez de esperança.
 */

export type Source = "gmail" | "local" | "both";
export type AiMode = "auto" | "lite" | "full" | "skip";

export interface CliOptions {
  source: Source;
  // Gmail
  label: string;
  query: string | null;
  fromDate: string | null;
  toDate: string | null;
  includeBody: boolean;
  includeAttachments: boolean;
  // Pasta local
  dir: string | null;
  recursive: boolean;
  moveProcessedTo: string | null;
  // Comuns
  limit: number;
  batchSize: number;
  dryRun: boolean;
  verbose: boolean;
  /** Drena os jobs após escanear, em processo. */
  process: boolean;
  maxJobs: number;
  aiMode: AiMode;
  userId: string | null;
  help: boolean;
}

export const DEFAULTS: CliOptions = {
  source: "both",
  label: "Comprovantes",
  query: null,
  fromDate: null,
  toDate: null,
  includeBody: false,
  includeAttachments: true,
  dir: "./inbox",
  recursive: false,
  moveProcessedTo: null,
  limit: Infinity,
  batchSize: 50,
  dryRun: false,
  verbose: false,
  process: false,
  maxJobs: 500,
  aiMode: "auto",
  userId: null,
  help: false,
};

const AI_MODES: AiMode[] = ["auto", "lite", "full", "skip"];

export class CliError extends Error {}

/**
 * Converte argv em opções.
 *
 * Lança `CliError` em flag desconhecida ou valor inválido. Falhar alto é o
 * ponto: silêncio foi exatamente o que permitiu que cinco flags parecessem
 * suportadas por meses.
 */
export function parseArgs(argv: string[]): CliOptions {
  const o: CliOptions = { ...DEFAULTS };

  const next = (i: number, flag: string): string => {
    const v = argv[i + 1];
    if (v === undefined || v.startsWith("--")) {
      throw new CliError(`${flag} exige um valor`);
    }
    return v;
  };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]!;
    switch (arg) {
      case "--gmail":
        o.source = o.source === "local" ? "both" : "gmail";
        break;
      case "--local":
        o.source = o.source === "gmail" ? "both" : "local";
        break;
      case "--both":
        o.source = "both";
        break;

      case "--label":
        o.label = next(i++, arg);
        break;
      case "--query":
        o.query = next(i++, arg);
        break;
      case "--from-date":
        o.fromDate = next(i++, arg);
        break;
      case "--to-date":
        o.toDate = next(i++, arg);
        break;
      case "--include-body":
        o.includeBody = true;
        break;
      case "--no-attachments":
        o.includeAttachments = false;
        break;

      case "--dir":
        o.dir = next(i++, arg);
        break;
      case "--recursive":
        o.recursive = true;
        break;
      case "--move-processed-to":
        o.moveProcessedTo = next(i++, arg);
        break;

      case "--limit": {
        const n = Number(next(i++, arg));
        if (!Number.isFinite(n) || n <= 0)
          throw new CliError("--limit deve ser um número positivo");
        o.limit = n;
        break;
      }
      case "--batch-size": {
        const n = Number(next(i++, arg));
        if (!Number.isFinite(n) || n <= 0)
          throw new CliError("--batch-size deve ser um número positivo");
        o.batchSize = n;
        break;
      }
      case "--max-jobs": {
        const n = Number(next(i++, arg));
        if (!Number.isFinite(n) || n <= 0)
          throw new CliError("--max-jobs deve ser um número positivo");
        o.maxJobs = n;
        break;
      }

      case "--dry-run":
        o.dryRun = true;
        break;
      case "--verbose":
        o.verbose = true;
        break;
      case "--process":
        o.process = true;
        break;
      case "--ai-mode": {
        const v = next(i++, arg) as AiMode;
        if (!AI_MODES.includes(v)) {
          throw new CliError(`--ai-mode deve ser um de: ${AI_MODES.join(", ")}`);
        }
        o.aiMode = v;
        break;
      }
      case "--user-id":
        o.userId = next(i++, arg);
        break;

      case "--help":
      case "-h":
        o.help = true;
        break;

      default:
        throw new CliError(`Flag desconhecida: ${arg}`);
    }
  }

  return o;
}

export const HELP = `
Seu Bolso Feliz — pipeline de ingestão

Uso: bun run cli -- [opções]

Fontes:
  --gmail                  Escanear o Gmail
  --local                  Escanear a pasta local
  --both                   Ambos (padrão)

Gmail:
  --label <nome>           Label do Gmail (padrão: Comprovantes)
  --query <q>              Query Gmail (ex: 'from:nubank has:attachment')
  --from-date <YYYY-MM-DD> Início do período (vira after:)
  --to-date <YYYY-MM-DD>   Fim do período, inclusivo (vira before:)
  --include-body           Processar também o corpo dos e-mails
  --no-attachments         Não processar anexos

Pasta local:
  --dir <caminho>          Diretório a escanear (padrão: ./inbox)
  --recursive              Descer em subdiretórios
  --move-processed-to <d>  Mover arquivos processados

Processamento:
  --process                Drenar os jobs após escanear, em processo
  --max-jobs <n>           Teto de jobs por execução (padrão: 500)
  --ai-mode <modo>         auto | lite | full | skip (padrão: auto)

Comuns:
  --limit <n>              Máximo de itens por fonte
  --batch-size <n>         Tamanho do lote de listagem (padrão: 50)
  --user-id <uuid>         Sobrescreve LOCAL_USER_ID
  --dry-run                Não grava nada
  --verbose                Log detalhado
  --help                   Esta ajuda
`;
