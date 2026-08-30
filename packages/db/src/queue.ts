/**
 * Fila de jobs no próprio Postgres — o canal entre a app na Vercel e os
 * workers.
 *
 * Não há HTTP nem SSH entre os dois lados: a app grava uma linha em
 * `public.ingestion_jobs` e os workers a reivindicam com `FOR UPDATE SKIP
 * LOCKED`. Nenhuma porta é exposta, e o único endereço que qualquer processo
 * precisa conhecer é o `DATABASE_URL`.
 *
 * Três mecanismos sustentam isso:
 *
 * 1. **Claim atômico.** `claimJobs` é UMA instrução `UPDATE ... FROM (SELECT
 *    ... FOR UPDATE SKIP LOCKED)`. Dois workers disparando ao mesmo tempo
 *    nunca pegam o mesmo job: o segundo pula a linha travada pelo primeiro
 *    em vez de bloquear nela.
 * 2. **Reap.** Um worker que morre (OOM, `kill -9`, queda da VPS) deixa o
 *    job com `locked_by` preenchido para sempre. `reapStaleJobs` devolve à
 *    fila tudo que está travado há mais tempo que o limite.
 * 3. **Acordar sem polling apertado.** `waitForJob` escuta `NOTIFY sbf_jobs`
 *    (disparado por um trigger `AFTER INSERT`) numa conexão direta, e cai
 *    para polling quando a notificação não chega. O fallback não é
 *    pessimismo gratuito: o pooler da Neon não entrega notificação de forma
 *    confiável, então a conexão de LISTEN precisa ser a direta
 *    (`DATABASE_URL_UNPOOLED`) via driver `pg` — e se ela cair, o polling
 *    segura o worker de pé até a reconexão.
 *
 * As capacidades (`requires` no job, `capabilities` no worker) resolvem o
 * roteamento: um job que precisa de OCR ou de ler a pasta local só pode ser
 * reivindicado por um worker que anuncia essa capacidade. O teste é de
 * subconjunto (`requires <@ capabilities`), então `requires = '{}'` é
 * elegível para qualquer worker.
 */
import { sql } from "drizzle-orm";
import { Client } from "pg";
import { getDb } from "./client";

// ---------------------------------------------------------------------------
// Tipos
// ---------------------------------------------------------------------------

/**
 * A instância do Drizzle devolvida por {@link getDb}.
 *
 * Vive aqui porque todo worker precisa nomear esse tipo em assinatura de
 * função, e `ReturnType<typeof getDb>` espalhado por dezenas de arquivos
 * envelhece mal.
 */
export type Database = ReturnType<typeof getDb>;

/** Subconjunto do Drizzle que a fila usa — só SQL bruto, sem query builder. */
export type QueueDb = Pick<Database, "execute">;

export interface QueueJob {
  id: string;
  runId: string;
  userId: string;
  sourceDocumentId: string | null;
  status: string;
  errorMessage: string | null;
  retryCount: number;
  maxRetries: number;
  metadata: Record<string, unknown>;
  /** Capacidades exigidas para processar este job (ex.: `["ocr"]`). */
  requires: string[];
  lockedBy: string | null;
  lockedAt: string | null;
  availableAt: string;
  createdAt: string | null;
  updatedAt: string | null;
}

/** Nome do canal do `NOTIFY` disparado pelo trigger de INSERT. */
export const JOB_CHANNEL = "sbf_jobs";

/**
 * Status em que um job ainda tem trabalho pendente no pipeline.
 *
 * Espelha o poll original do worker de ingestão: os estados intermediários
 * (`parsing`, `classified`, …) são de posse de quem já está processando, e
 * `posted`/`failed` são terminais.
 *
 * LACUNA CONHECIDA (herdada do poll antigo, não introduzida aqui): um worker
 * que morra no meio de um passo deixa o job parado num estado intermediário.
 * `reapStaleJobs` devolve o lock, mas o status continua fora desta lista e
 * nenhum claim volta a enxergá-lo. Fechar isso exige o reap saber para onde
 * cada estado intermediário deve retroceder (`parsing` → `queued`,
 * `ai_lite_enriching` → `parsed`, …) — conhecimento da máquina de estados do
 * worker de ingestão, não da fila. Enquanto não existir, esses jobs precisam
 * de reset manual, e o sintoma é um job com `locked_by` nulo e status
 * intermediário parado há horas.
 */
export const CLAIMABLE_STATUSES: readonly string[] = [
  "discovered",
  "downloaded",
  "hashed",
  "queued",
  "parsed",
];

/** Colunas devolvidas por toda operação da fila. */
const JOB_COLUMNS = sql`id, run_id, user_id, source_document_id, status, error_message,
  retry_count, max_retries, metadata, requires, locked_by, locked_at,
  available_at, created_at, updated_at`;

/**
 * As mesmas colunas qualificadas por `j`.
 *
 * O `UPDATE ... FROM (subquery)` do claim traz uma segunda relação para o
 * escopo, e um `returning id` sem prefixo é rejeitado como ambíguo pelo
 * Postgres.
 */
const JOB_COLUMNS_J = sql`j.id, j.run_id, j.user_id, j.source_document_id, j.status,
  j.error_message, j.retry_count, j.max_retries, j.metadata, j.requires,
  j.locked_by, j.locked_at, j.available_at, j.created_at, j.updated_at`;

// ---------------------------------------------------------------------------
// Normalização de linha
// ---------------------------------------------------------------------------

/**
 * Serializa um array JS no literal de array do Postgres (`{a,b}`).
 *
 * O driver HTTP da Neon e o driver `pg` não concordam sobre como serializar
 * um array JS passado como parâmetro; passar o literal já pronto como `text`
 * com cast explícito (`$1::text[]`) funciona igual nos dois.
 */
export function formatPgTextArray(values: readonly string[]): string {
  if (values.length === 0) return "{}";
  const escaped = values.map((v) => `"${v.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`);
  return `{${escaped.join(",")}}`;
}

/** Aceita tanto o array já parseado quanto o literal `{a,b}` cru. */
export function parsePgTextArray(value: unknown): string[] {
  if (Array.isArray(value)) return value.map((v) => String(v));
  if (typeof value !== "string") return [];
  const trimmed = value.trim();
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) return [];
  const inner = trimmed.slice(1, -1);
  if (inner.length === 0) return [];
  const out: string[] = [];
  let current = "";
  let quoted = false;
  let escaping = false;
  for (const ch of inner) {
    if (escaping) {
      current += ch;
      escaping = false;
    } else if (ch === "\\") {
      escaping = true;
    } else if (ch === '"') {
      quoted = !quoted;
    } else if (ch === "," && !quoted) {
      out.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  out.push(current);
  return out;
}

function toIsoOrNull(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (value instanceof Date) return value.toISOString();
  return String(value);
}

function toJsonObject(value: unknown): Record<string, unknown> {
  if (value === null || value === undefined) return {};
  if (typeof value === "string") {
    try {
      const parsed: unknown = JSON.parse(value);
      return typeof parsed === "object" && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  }
  return typeof value === "object" ? (value as Record<string, unknown>) : {};
}

function mapRow(row: Record<string, unknown>): QueueJob {
  return {
    id: String(row.id),
    runId: String(row.run_id),
    userId: String(row.user_id),
    sourceDocumentId: row.source_document_id === null ? null : String(row.source_document_id),
    status: String(row.status),
    errorMessage: row.error_message === null ? null : String(row.error_message ?? ""),
    retryCount: Number(row.retry_count ?? 0),
    maxRetries: Number(row.max_retries ?? 3),
    metadata: toJsonObject(row.metadata),
    requires: parsePgTextArray(row.requires),
    lockedBy: row.locked_by === null ? null : String(row.locked_by),
    lockedAt: toIsoOrNull(row.locked_at),
    availableAt: toIsoOrNull(row.available_at) ?? new Date().toISOString(),
    createdAt: toIsoOrNull(row.created_at),
    updatedAt: toIsoOrNull(row.updated_at),
  };
}

/**
 * `db.execute()` devolve `{ rows }` no driver HTTP da Neon e no `pg`, mas os
 * tipos declarados divergem; um acesso normalizado evita espalhar cast.
 */
async function rowsOf(db: QueueDb, query: Parameters<QueueDb["execute"]>[0]) {
  const result = (await db.execute(query)) as unknown as { rows?: unknown } | unknown[];
  const raw = Array.isArray(result) ? result : (result.rows ?? []);
  return (Array.isArray(raw) ? raw : []) as Record<string, unknown>[];
}

// ---------------------------------------------------------------------------
// Backoff
// ---------------------------------------------------------------------------

export interface BackoffOptions {
  /** Espera da primeira retentativa, em segundos. Padrão: 5. */
  baseSeconds?: number;
  /** Multiplicador por tentativa. Padrão: 2. */
  factor?: number;
  /** Teto da espera, em segundos. Padrão: 3600 (1 h). */
  maxSeconds?: number;
}

const DEFAULT_BACKOFF: Required<BackoffOptions> = {
  baseSeconds: 5,
  factor: 2,
  maxSeconds: 3600,
};

/**
 * Espera antes da próxima tentativa, em segundos.
 *
 * `retryCount` é o número de falhas JÁ registradas antes desta — 0 na
 * primeira falha, que portanto espera exatamente `baseSeconds`.
 *
 * A mesma curva é calculada dentro do SQL de `failJob` (para manter a
 * operação numa única instrução atômica). O teste de integração
 * `backoff SQL e JS concordam` existe para impedir que as duas divirjam.
 */
export function computeBackoffSeconds(retryCount: number, options: BackoffOptions = {}): number {
  const { baseSeconds, factor, maxSeconds } = { ...DEFAULT_BACKOFF, ...options };
  const raw = baseSeconds * Math.pow(factor, Math.max(0, retryCount));
  return Math.min(maxSeconds, raw);
}

// ---------------------------------------------------------------------------
// enqueue
// ---------------------------------------------------------------------------

export interface EnqueueJobInput {
  runId: string;
  userId: string;
  sourceDocumentId?: string | null;
  /** Padrão: `"discovered"`. */
  status?: string;
  /** Capacidades exigidas. Padrão: `[]` (qualquer worker atende). */
  requires?: readonly string[];
  metadata?: Record<string, unknown>;
  maxRetries?: number;
  /** Agenda o job para o futuro. Padrão: disponível imediatamente. */
  availableAt?: Date | string | null;
  db?: QueueDb;
}

export async function enqueueJob(input: EnqueueJobInput): Promise<QueueJob> {
  const db = input.db ?? getDb();
  const rows = await rowsOf(
    db,
    sql`
      insert into public.ingestion_jobs
        (run_id, user_id, source_document_id, status, requires, metadata, max_retries, available_at)
      values (
        ${input.runId}::uuid,
        ${input.userId}::uuid,
        ${input.sourceDocumentId ?? null}::uuid,
        ${input.status ?? "discovered"},
        ${formatPgTextArray(input.requires ?? [])}::text[],
        ${JSON.stringify(input.metadata ?? {})}::jsonb,
        coalesce(${input.maxRetries ?? null}::int, 3),
        coalesce(${normalizeTimestamp(input.availableAt)}::timestamptz, now())
      )
      returning ${JOB_COLUMNS}
    `,
  );
  const row = rows[0];
  if (!row) throw new Error("enqueueJob: insert não devolveu linha");
  return mapRow(row);
}

function normalizeTimestamp(value: Date | string | null | undefined): string | null {
  if (value === null || value === undefined) return null;
  return value instanceof Date ? value.toISOString() : value;
}

// ---------------------------------------------------------------------------
// claim
// ---------------------------------------------------------------------------

export interface ClaimJobOptions {
  /** Identidade do worker, gravada em `locked_by`. */
  workerId: string;
  /** Capacidades oferecidas. Padrão: `[]`. */
  capabilities?: readonly string[];
  /** Status elegíveis. Padrão: {@link CLAIMABLE_STATUSES}. */
  statuses?: readonly string[];
  /**
   * Status a gravar no ato do claim. Padrão: mantém o status atual — quem
   * avança a máquina de estados é o processamento, não a fila.
   */
  status?: string | null;
  db?: QueueDb;
}

export interface ClaimJobsOptions extends ClaimJobOptions {
  /** Quantos jobs reivindicar de uma vez. Padrão: 1. */
  limit?: number;
}

/**
 * Reivindica até `limit` jobs numa única instrução atômica.
 *
 * O `SKIP LOCKED` é o que torna isto seguro sob concorrência: workers
 * simultâneos nunca disputam a mesma linha e nenhum deles bloqueia esperando
 * o outro. O lock lógico (`locked_by`/`locked_at`) sobrevive ao commit — é
 * ele, não o lock de linha, que impede um segundo claim enquanto o job está
 * sendo processado.
 */
export async function claimJobs(options: ClaimJobsOptions): Promise<QueueJob[]> {
  const db = options.db ?? getDb();
  const statuses = options.statuses ?? CLAIMABLE_STATUSES;
  const capabilities = options.capabilities ?? [];
  const limit = Math.max(1, options.limit ?? 1);

  const rows = await rowsOf(
    db,
    sql`
      update public.ingestion_jobs as j
      set locked_by = ${options.workerId},
          locked_at = now(),
          status = coalesce(${options.status ?? null}::text, j.status),
          updated_at = now()
      from (
        select id as claimed_id
        from public.ingestion_jobs
        where locked_by is null
          and available_at <= now()
          and status = any(${formatPgTextArray([...statuses])}::text[])
          and requires <@ ${formatPgTextArray([...capabilities])}::text[]
        order by available_at asc, created_at asc
        limit ${limit}
        for update skip locked
      ) as c
      where j.id = c.claimed_id
      returning ${JOB_COLUMNS_J}
    `,
  );
  return rows.map(mapRow);
}

/** Conveniência: reivindica um job, ou `null` se a fila não tem trabalho. */
export async function claimJob(options: ClaimJobOptions): Promise<QueueJob | null> {
  const [job] = await claimJobs({ ...options, limit: 1 });
  return job ?? null;
}

// ---------------------------------------------------------------------------
// complete / fail / release
// ---------------------------------------------------------------------------

export interface CompleteJobOptions {
  jobId: string;
  /** Status final. Padrão: mantém o status atual. */
  status?: string | null;
  /**
   * Quando informado, a escrita só acontece se o job ainda estiver travado
   * por este worker. Protege contra um worker "zumbi" (já reapeado, o job já
   * reivindicado por outro) sobrescrever o trabalho de quem o substituiu.
   */
  expectedWorkerId?: string | null;
  db?: QueueDb;
}

/** Encerra o job com sucesso e devolve o lock. */
export async function completeJob(options: CompleteJobOptions): Promise<QueueJob | null> {
  const db = options.db ?? getDb();
  const rows = await rowsOf(
    db,
    sql`
      update public.ingestion_jobs
      set status = coalesce(${options.status ?? null}::text, status),
          locked_by = null,
          locked_at = null,
          error_message = null,
          updated_at = now()
      where id = ${options.jobId}::uuid
        and (${options.expectedWorkerId ?? null}::text is null
             or locked_by = ${options.expectedWorkerId ?? null}::text)
      returning ${JOB_COLUMNS}
    `,
  );
  const row = rows[0];
  return row ? mapRow(row) : null;
}

export interface FailJobOptions {
  jobId: string;
  /** Mensagem gravada em `error_message`. */
  error: string | Error;
  /** Detalhes estruturados gravados em `error_details`. */
  details?: Record<string, unknown> | null;
  /**
   * `true` (padrão) devolve o job à fila enquanto houver tentativas
   * sobrando; `false` marca `failed` de imediato (erro definitivo, tipo
   * documento inválido — retentar não muda o resultado).
   */
  retry?: boolean;
  /** Status a restaurar ao retentar. Padrão: mantém o status atual. */
  retryStatus?: string | null;
  expectedWorkerId?: string | null;
  backoff?: BackoffOptions;
  db?: QueueDb;
}

export interface FailJobResult {
  job: QueueJob;
  /** `true` quando ainda havia tentativa sobrando e o job voltou à fila. */
  willRetry: boolean;
  /** Espera aplicada até `available_at`, em segundos (0 quando não retenta). */
  backoffSeconds: number;
}

/**
 * Registra a falha do job: devolve à fila com backoff exponencial enquanto
 * `retry_count < max_retries`, e marca `failed` quando as tentativas acabam.
 *
 * Tudo numa instrução — o `CASE` decide retentar ou falhar a partir do
 * `retry_count` da própria linha, sem um SELECT anterior que abriria janela
 * para um reap concorrente.
 */
export async function failJob(options: FailJobOptions): Promise<FailJobResult | null> {
  const db = options.db ?? getDb();
  const retry = options.retry ?? true;
  const { baseSeconds, factor, maxSeconds } = { ...DEFAULT_BACKOFF, ...options.backoff };
  const message = options.error instanceof Error ? options.error.message : String(options.error);

  const rows = await rowsOf(
    db,
    sql`
      update public.ingestion_jobs
      set retry_count = coalesce(retry_count, 0) + 1,
          status = case
            when ${retry}
             and coalesce(retry_count, 0) + 1 <= coalesce(max_retries, 3)
            then coalesce(${options.retryStatus ?? null}::text, status)
            else 'failed'
          end,
          available_at = case
            when ${retry}
             and coalesce(retry_count, 0) + 1 <= coalesce(max_retries, 3)
            then now() + make_interval(secs => least(
              ${maxSeconds}::double precision,
              ${baseSeconds}::double precision
                * power(${factor}::double precision, coalesce(retry_count, 0))
            ))
            else available_at
          end,
          locked_by = null,
          locked_at = null,
          error_message = ${message},
          error_details = ${options.details ? JSON.stringify(options.details) : null}::jsonb,
          updated_at = now()
      where id = ${options.jobId}::uuid
        and (${options.expectedWorkerId ?? null}::text is null
             or locked_by = ${options.expectedWorkerId ?? null}::text)
      returning ${JOB_COLUMNS},
        (retry_count <= coalesce(max_retries, 3)) as will_retry
    `,
  );
  const row = rows[0];
  if (!row) return null;
  const job = mapRow(row);
  const willRetry = retry && row.will_retry === true;
  return {
    job,
    willRetry,
    backoffSeconds: willRetry
      ? computeBackoffSeconds(job.retryCount - 1, { baseSeconds, factor, maxSeconds })
      : 0,
  };
}

export interface ReleaseJobOptions {
  jobId: string;
  expectedWorkerId?: string | null;
  /** Quando devolver o job à disponibilidade. Padrão: agora. */
  availableAt?: Date | string | null;
  /** Status a restaurar. Padrão: mantém o atual. */
  status?: string | null;
  db?: QueueDb;
}

/**
 * Devolve o job à fila sem contá-lo como falha.
 *
 * É o que o shutdown limpo usa: um SIGTERM no meio do processamento não é um
 * erro do documento, e consumir uma tentativa por causa de um deploy seria
 * castigar o job pelo que aconteceu com o worker.
 */
export async function releaseJob(options: ReleaseJobOptions): Promise<QueueJob | null> {
  const db = options.db ?? getDb();
  const rows = await rowsOf(
    db,
    sql`
      update public.ingestion_jobs
      set locked_by = null,
          locked_at = null,
          status = coalesce(${options.status ?? null}::text, status),
          available_at = coalesce(${normalizeTimestamp(options.availableAt)}::timestamptz, now()),
          updated_at = now()
      where id = ${options.jobId}::uuid
        and (${options.expectedWorkerId ?? null}::text is null
             or locked_by = ${options.expectedWorkerId ?? null}::text)
      returning ${JOB_COLUMNS}
    `,
  );
  const row = rows[0];
  return row ? mapRow(row) : null;
}

// ---------------------------------------------------------------------------
// reap
// ---------------------------------------------------------------------------

export interface ReapStaleJobsOptions {
  /**
   * Idade mínima do lock para considerá-lo abandonado. Aceita milissegundos
   * ou uma duração. Padrão: 10 minutos.
   */
  olderThanMs?: number;
  db?: QueueDb;
}

/**
 * Devolve à fila jobs cujo worker morreu sem liberar o lock.
 *
 * Sem isto, um `kill -9` no meio do processamento tira o job de circulação
 * para sempre: `locked_by` continua preenchido e nenhum claim futuro o
 * enxerga. O limite precisa ser confortavelmente maior que o job mais lento
 * do pipeline, senão o reap rouba trabalho de quem ainda está vivo.
 */
export async function reapStaleJobs(options: ReapStaleJobsOptions = {}): Promise<QueueJob[]> {
  const db = options.db ?? getDb();
  const seconds = (options.olderThanMs ?? 10 * 60_000) / 1000;
  const rows = await rowsOf(
    db,
    sql`
      update public.ingestion_jobs
      set locked_by = null,
          locked_at = null,
          available_at = now(),
          updated_at = now()
      where locked_by is not null
        and locked_at is not null
        and locked_at < now() - make_interval(secs => ${seconds}::double precision)
      returning ${JOB_COLUMNS}
    `,
  );
  return rows.map(mapRow);
}

// ---------------------------------------------------------------------------
// LISTEN / NOTIFY
// ---------------------------------------------------------------------------

export interface JobNotifierOptions {
  /** Canal a escutar. Padrão: {@link JOB_CHANNEL}. */
  channel?: string;
  /**
   * Conexão direta. Padrão: `DATABASE_URL_UNPOOLED` e, na falta dela,
   * `DATABASE_URL`. O pooler não entrega notificação de forma confiável.
   */
  connectionString?: string;
  /** Espera entre tentativas de reconexão. Padrão: 2 s. */
  reconnectDelayMs?: number;
  onError?: (error: Error) => void;
}

type Waiter = { resolve: (value: "notified") => void };

/**
 * Conexão dedicada de `LISTEN`, com reconexão automática.
 *
 * Vive fora do pool do Drizzle de propósito: uma conexão em LISTEN fica
 * ocupada indefinidamente, e emprestá-la do pool de queries estrangularia o
 * worker. Toda falha aqui é degradação, nunca interrupção — quem espera
 * acorda pelo timeout de polling de qualquer forma.
 */
export class JobNotifier {
  private readonly channel: string;
  private readonly connectionString: string | undefined;
  private readonly reconnectDelayMs: number;
  private readonly onError: ((error: Error) => void) | undefined;
  private readonly waiters = new Set<Waiter>();
  private client: Client | null = null;
  private connecting: Promise<void> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  /** Exposto para diagnóstico/teste: a conexão de LISTEN está de pé? */
  public listening = false;

  constructor(options: JobNotifierOptions = {}) {
    this.channel = options.channel ?? JOB_CHANNEL;
    this.connectionString =
      options.connectionString ??
      process.env.DATABASE_URL_UNPOOLED ??
      process.env.DATABASE_URL ??
      undefined;
    this.reconnectDelayMs = options.reconnectDelayMs ?? 2000;
    this.onError = options.onError;
  }

  /** Abre a conexão de LISTEN. Falha silenciosamente — o polling cobre. */
  async start(): Promise<void> {
    if (this.closed || !this.connectionString) return;
    if (this.client || this.connecting) return this.connecting ?? undefined;

    const connectionString = this.connectionString;
    this.connecting = (async () => {
      const client = new Client({ connectionString });
      client.on("notification", (msg) => {
        if (msg.channel === this.channel) this.wake();
      });
      client.on("error", (err: Error) => {
        this.handleDisconnect(err);
      });
      client.on("end", () => {
        this.handleDisconnect(new Error("conexão de LISTEN encerrada"));
      });
      await client.connect();
      await client.query(`LISTEN ${quoteIdentifier(this.channel)}`);
      this.client = client;
      this.listening = true;
    })()
      .catch((err: unknown) => {
        this.listening = false;
        this.client = null;
        this.report(err);
        this.scheduleReconnect();
      })
      .finally(() => {
        this.connecting = null;
      });

    return this.connecting;
  }

  /**
   * Registra a espera ANTES de o chamador tentar o claim.
   *
   * A ordem importa: armar depois do claim perde a notificação que chegar no
   * meio, e o worker dormiria até o timeout de polling com trabalho parado
   * na fila.
   */
  arm(): { promise: Promise<"notified" | "timeout" | "aborted">; cancel: () => void } {
    let waiter: Waiter | null = null;
    let settle: ((value: "notified" | "timeout" | "aborted") => void) | null = null;
    const promise = new Promise<"notified" | "timeout" | "aborted">((resolve) => {
      settle = resolve;
      waiter = { resolve };
      this.waiters.add(waiter);
    });
    const cancel = () => {
      if (waiter) this.waiters.delete(waiter);
    };
    return {
      promise,
      cancel: () => {
        cancel();
        settle?.("aborted");
      },
    };
  }

  /**
   * Espera por notificação, com teto de tempo (o fallback de polling) e
   * cancelamento por `AbortSignal`.
   */
  async wait(options: {
    timeoutMs: number;
    signal?: AbortSignal;
  }): Promise<"notified" | "timeout" | "aborted"> {
    const armed = this.arm();
    return this.settle(armed, options);
  }

  /** Resolve uma espera já armada, aplicando timeout e abort. */
  async settle(
    armed: { promise: Promise<"notified" | "timeout" | "aborted">; cancel: () => void },
    options: { timeoutMs: number; signal?: AbortSignal },
  ): Promise<"notified" | "timeout" | "aborted"> {
    if (options.signal?.aborted) {
      armed.cancel();
      return "aborted";
    }
    void this.start();

    let timer: ReturnType<typeof setTimeout> | null = null;
    let onAbort: (() => void) | null = null;
    try {
      const timeout = new Promise<"timeout">((resolve) => {
        timer = setTimeout(() => resolve("timeout"), options.timeoutMs);
      });
      const aborted = new Promise<"aborted">((resolve) => {
        if (!options.signal) return;
        onAbort = () => resolve("aborted");
        options.signal.addEventListener("abort", onAbort, { once: true });
      });
      return await Promise.race([armed.promise, timeout, aborted]);
    } finally {
      if (timer) clearTimeout(timer);
      if (onAbort && options.signal) options.signal.removeEventListener("abort", onAbort);
      armed.cancel();
    }
  }

  /** Acorda todos os que esperam. Público para permitir testar sem banco. */
  wake(): void {
    for (const waiter of [...this.waiters]) {
      this.waiters.delete(waiter);
      waiter.resolve("notified");
    }
  }

  async close(): Promise<void> {
    this.closed = true;
    this.listening = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    const client = this.client;
    this.client = null;
    this.wake();
    if (client) {
      client.removeAllListeners();
      await client.end().catch(() => undefined);
    }
  }

  private handleDisconnect(err: Error): void {
    if (this.closed) return;
    this.listening = false;
    const client = this.client;
    this.client = null;
    if (client) {
      client.removeAllListeners();
      void client.end().catch(() => undefined);
    }
    this.report(err);
    // Acorda quem espera: sem a conexão, o polling é o único caminho, e
    // deixar o worker dormindo até o timeout atrasaria o job à toa.
    this.wake();
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.start();
    }, this.reconnectDelayMs);
    // Não segurar o processo vivo só por causa do timer de reconexão.
    this.reconnectTimer.unref?.();
  }

  private report(err: unknown): void {
    const error = err instanceof Error ? err : new Error(String(err));
    if (this.onError) this.onError(error);
  }
}

function quoteIdentifier(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

// ---------------------------------------------------------------------------
// waitForJob
// ---------------------------------------------------------------------------

export interface WaitForJobOptions extends ClaimJobOptions {
  /** Aborta a espera (shutdown). */
  signal?: AbortSignal;
  /** Teto entre tentativas quando a notificação não chega. Padrão: 5 s. */
  pollIntervalMs?: number;
  /** Reaproveita um notifier já aberto (o loop do worker mantém um). */
  notifier?: JobNotifier;
}

/**
 * Bloqueia até haver um job reivindicável, e o reivindica.
 *
 * Devolve `null` só quando o `signal` aborta — é assim que o loop do worker
 * sai do laço no shutdown.
 */
export async function waitForJob(options: WaitForJobOptions): Promise<QueueJob | null> {
  const notifier = options.notifier ?? new JobNotifier();
  const ownsNotifier = !options.notifier;
  const pollIntervalMs = options.pollIntervalMs ?? 5000;

  try {
    void notifier.start();
    for (;;) {
      if (options.signal?.aborted) return null;

      // Armar antes de reivindicar: se um job entrar entre o claim vazio e o
      // início da espera, a notificação já está capturada.
      const armed = notifier.arm();
      const job = await claimJob(options);
      if (job) {
        armed.cancel();
        return job;
      }

      const outcome = await notifier.settle(armed, {
        timeoutMs: pollIntervalMs,
        signal: options.signal,
      });
      if (outcome === "aborted") return null;
    }
  } finally {
    if (ownsNotifier) await notifier.close();
  }
}

// ---------------------------------------------------------------------------
// Capacidades
// ---------------------------------------------------------------------------

/**
 * Lê as capacidades do worker de `WORKER_CAPABILITIES` (lista separada por
 * vírgula), caindo no padrão do próprio worker quando a variável não existe.
 */
export function resolveCapabilities(
  fallback: readonly string[] = [],
  env: Record<string, string | undefined> = process.env,
): string[] {
  const raw = env.WORKER_CAPABILITIES;
  if (raw === undefined) return [...fallback];
  const parsed = raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  // `WORKER_CAPABILITIES=""` é uma declaração legítima de "nenhuma
  // capacidade", não uma variável ausente — respeitá-la é o que permite
  // rodar um worker deliberadamente restrito sem editar código.
  return parsed;
}

// ---------------------------------------------------------------------------
// Loop de worker
// ---------------------------------------------------------------------------

export interface WorkerLogger {
  info: (message: string, details?: unknown) => void;
  error: (message: string, details?: unknown) => void;
}

const defaultLogger: WorkerLogger = {
  info: (message, details) => console.log(message, details ?? ""),
  error: (message, details) => console.error(message, details ?? ""),
};

export interface JobOutcome {
  /** Status final a gravar. Padrão: o handler já cuidou do status. */
  status?: string | null;
  /** `false` marca `failed` sem consumir retentativas restantes. */
  retry?: boolean;
}

export interface RunWorkerLoopOptions {
  /** Identidade do worker. Padrão: `${nome}@${hostname}:${pid}`. */
  workerId?: string;
  /** Nome do worker, só para log e para compor o `workerId` padrão. */
  name: string;
  /** Capacidades oferecidas. */
  capabilities?: readonly string[];
  /** Status elegíveis para claim. */
  statuses?: readonly string[];
  /** Status a gravar no claim. */
  claimStatus?: string | null;
  /** Processa um job. Lançar erro faz o job voltar à fila com backoff. */
  handle: (job: QueueJob) => Promise<JobOutcome | void>;
  signal: AbortSignal;
  pollIntervalMs?: number;
  /** Frequência do reap. Padrão: 60 s. `0` desliga. */
  reapIntervalMs?: number;
  /** Idade do lock considerada abandono. Padrão: 10 min. */
  staleAfterMs?: number;
  /** Espera máxima pelo job em andamento no shutdown. Padrão: 30 s. */
  shutdownGraceMs?: number;
  backoff?: BackoffOptions;
  logger?: WorkerLogger;
  db?: QueueDb;
  notifier?: JobNotifier;
}

/**
 * `while (true) { claim() ?? await waitForJob(); processa; complete/fail }`.
 *
 * O shutdown é o ponto delicado: um SIGTERM no meio do processamento espera
 * o job terminar até `shutdownGraceMs` e, se estourar, devolve o job à fila
 * em vez de deixá-lo travado com `locked_by` preenchido — o reap acabaria
 * resolvendo, mas só depois de minutos de atraso desnecessário.
 */
export async function runWorkerLoop(options: RunWorkerLoopOptions): Promise<void> {
  const log = options.logger ?? defaultLogger;
  const workerId = options.workerId ?? defaultWorkerId(options.name);
  const capabilities = options.capabilities ?? [];
  const pollIntervalMs = options.pollIntervalMs ?? 5000;
  const reapIntervalMs = options.reapIntervalMs ?? 60_000;
  const staleAfterMs = options.staleAfterMs ?? 10 * 60_000;
  const shutdownGraceMs = options.shutdownGraceMs ?? 30_000;
  const signal = options.signal;
  const notifier =
    options.notifier ??
    new JobNotifier({ onError: (err) => log.error(`[${options.name}] LISTEN`, err.message) });
  const ownsNotifier = !options.notifier;

  const claimOptions: ClaimJobOptions = {
    workerId,
    capabilities,
    statuses: options.statuses,
    status: options.claimStatus,
    db: options.db,
  };

  log.info(
    `[${options.name}] iniciando — worker=${workerId} capacidades=[${capabilities.join(", ")}]`,
  );

  let lastReap = 0;
  /** Job em processamento, para o shutdown saber o que devolver. */
  let inFlight: string | null = null;
  let finished = false;

  /** Timer do prazo de shutdown, para poder ser cancelado no caminho feliz. */
  let graceTimer: ReturnType<typeof setTimeout> | null = null;

  const onAbort = () => {
    if (finished) return;
    graceTimer = setTimeout(() => {
      graceTimer = null;
      if (finished || !inFlight) return;
      const jobId = inFlight;
      log.error(`[${options.name}] shutdown estourou o prazo — devolvendo job ${jobId} à fila`);
      void releaseJob({ jobId, expectedWorkerId: workerId, db: options.db }).catch((err: unknown) =>
        log.error(`[${options.name}] falha ao devolver job`, err),
      );
    }, shutdownGraceMs);
    // `unref` para o prazo de shutdown não virar o motivo de o processo
    // continuar vivo: quando o job termina antes, ninguém deveria esperar o
    // relógio zerar só porque um timer ficou pendurado no event loop.
    graceTimer.unref?.();
  };
  signal.addEventListener("abort", onAbort, { once: true });

  try {
    while (!signal.aborted) {
      if (reapIntervalMs > 0 && Date.now() - lastReap >= reapIntervalMs) {
        lastReap = Date.now();
        try {
          const reaped = await reapStaleJobs({ olderThanMs: staleAfterMs, db: options.db });
          if (reaped.length > 0) {
            log.info(`[${options.name}] ${reaped.length} job(s) travado(s) devolvido(s) à fila`);
          }
        } catch (err) {
          log.error(`[${options.name}] reap falhou`, err);
        }
      }

      let job: QueueJob | null = null;
      try {
        job =
          (await claimJob(claimOptions)) ??
          (await waitForJob({ ...claimOptions, signal, pollIntervalMs, notifier }));
      } catch (err) {
        log.error(`[${options.name}] claim falhou`, err);
        await sleep(pollIntervalMs, signal);
        continue;
      }
      if (!job) break;

      inFlight = job.id;
      try {
        const outcome = (await options.handle(job)) ?? {};
        await completeJob({
          jobId: job.id,
          status: outcome.status ?? null,
          expectedWorkerId: workerId,
          db: options.db,
        });
      } catch (err) {
        const result = await failJob({
          jobId: job.id,
          error: err instanceof Error ? err : String(err),
          details: err instanceof Error && err.stack ? { stack: err.stack } : null,
          expectedWorkerId: workerId,
          backoff: options.backoff,
          db: options.db,
        }).catch((failErr: unknown) => {
          log.error(`[${options.name}] falha ao registrar erro do job ${job?.id}`, failErr);
          return null;
        });
        const suffix = result?.willRetry
          ? `retentativa em ${result.backoffSeconds}s (${result.job.retryCount}/${result.job.maxRetries})`
          : "sem retentativas restantes";
        log.error(`[${options.name}] job ${job.id} falhou — ${suffix}`, err);
      } finally {
        inFlight = null;
      }
    }
  } finally {
    finished = true;
    if (graceTimer) clearTimeout(graceTimer);
    signal.removeEventListener("abort", onAbort);
    if (ownsNotifier) await notifier.close();
    log.info(`[${options.name}] encerrado.`);
  }
}

function defaultWorkerId(name: string): string {
  const host = process.env.HOSTNAME ?? process.env.COMPUTERNAME ?? "local";
  return `${name}@${host}:${process.pid}`;
}

/** `setTimeout` que também acorda no abort — nunca prende o shutdown. */
export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms);
    function done() {
      clearTimeout(timer);
      signal?.removeEventListener("abort", done);
      resolve();
    }
    signal?.addEventListener("abort", done, { once: true });
  });
}

/**
 * `AbortSignal` que dispara em SIGTERM/SIGINT.
 *
 * Um segundo sinal encerra na marra: se o operador insiste, esperar o job
 * terminar deixou de ser o que ele quer.
 */
export function createShutdownSignal(logger: WorkerLogger = defaultLogger): AbortSignal {
  const controller = new AbortController();
  let asked = false;
  for (const sig of ["SIGTERM", "SIGINT"] as const) {
    process.on(sig, () => {
      if (asked) {
        logger.error(`${sig} novamente — encerrando imediatamente.`);
        process.exit(130);
      }
      asked = true;
      logger.info(`${sig} recebido — encerrando com elegância...`);
      controller.abort();
    });
  }
  return controller.signal;
}
