/**
 * Fila de jobs no Postgres — testes contra um banco real.
 *
 * `SKIP LOCKED`, `NOTIFY` e aritmética de intervalo não têm como ser
 * exercitados com mock: o comportamento que importa (dois workers disputando
 * a mesma linha, uma notificação atravessando conexões, um lock envelhecendo)
 * mora no servidor, não no cliente. Então este arquivo exige um Postgres
 * descartável:
 *
 *   docker run -d --name sbf-queue-test \
 *     -e POSTGRES_PASSWORD=queue -e POSTGRES_USER=queue -e POSTGRES_DB=queue \
 *     -p 55432:5432 postgres:16-alpine
 *
 *   QUEUE_TEST_DATABASE_URL=postgresql://queue:queue@127.0.0.1:55432/queue \
 *     bun run test:integration -- queue
 *
 * A variável é separada de `DATABASE_URL` de propósito: o setup faz DROP e
 * CREATE da tabela, e apontar isso para o Neon de trabalho apagaria a fila
 * real. Sem a variável, o arquivo inteiro é pulado (o CI não provisiona
 * Postgres para esta suíte).
 */
import { describe, it, expect, beforeAll, beforeEach } from "vitest";
import { sql, type SQL } from "drizzle-orm";
import {
  CLAIMABLE_STATUSES,
  JOB_CHANNEL,
  JobNotifier,
  claimJob,
  claimJobs,
  completeJob,
  computeBackoffSeconds,
  enqueueJob,
  failJob,
  reapStaleJobs,
  releaseJob,
  getDb,
  runWorkerLoop,
  waitForJob,
  type QueueDb,
  type QueueJob,
} from "@sbf/db";

const TEST_URL = process.env.QUEUE_TEST_DATABASE_URL;
const RUN_ID = "11111111-1111-4111-8111-111111111111";
const USER_ID = "22222222-2222-4222-8222-222222222222";

// getDb() é um singleton lazy que lê DATABASE_URL na primeira chamada, e o
// arquivo roda em um worker isolado do Vitest — apontá-lo para o banco
// descartável aqui não vaza para nenhuma outra suíte.
if (TEST_URL) process.env.DATABASE_URL = TEST_URL;

let db: QueueDb;

/** Atalho para SQL bruto: os dois drivers devolvem `{ rows }`. */
async function exec(query: SQL): Promise<Record<string, unknown>[]> {
  const result = (await db.execute(query)) as unknown as { rows: Record<string, unknown>[] };
  return result.rows ?? [];
}

const SCHEMA = `
  drop table if exists public.ingestion_jobs cascade;
  create table public.ingestion_jobs (
    id uuid primary key default gen_random_uuid(),
    run_id uuid not null,
    user_id uuid not null,
    source_document_id uuid,
    status text not null default 'discovered',
    error_message text,
    error_details jsonb,
    retry_count integer default 0,
    max_retries integer default 3,
    metadata jsonb default '{}'::jsonb,
    created_at timestamptz default now(),
    updated_at timestamptz default now(),
    needs_full_ai_review boolean default false,
    locked_by text,
    locked_at timestamptz,
    requires text[] not null default '{}',
    available_at timestamptz not null default now()
  );

  create or replace function public.notify_sbf_jobs() returns trigger
  language plpgsql as $fn$
  begin
    perform pg_notify('${JOB_CHANNEL}', new.id::text);
    return new;
  end;
  $fn$;

  create trigger trg_notify_sbf_jobs
    after insert on public.ingestion_jobs
    for each row execute function public.notify_sbf_jobs();
`;

describe.skipIf(!TEST_URL)("fila de jobs no Postgres", () => {
  beforeAll(async () => {
    db = getDb();
    await db.execute(sql.raw(SCHEMA));
  });

  beforeEach(async () => {
    await db.execute(sql`truncate table public.ingestion_jobs`);
  });

  async function seed(
    overrides: Partial<Parameters<typeof enqueueJob>[0]> = {},
  ): Promise<QueueJob> {
    return enqueueJob({ runId: RUN_ID, userId: USER_ID, db, ...overrides });
  }

  async function readJob(id: string): Promise<Record<string, unknown>> {
    const rows = await exec(sql`select * from public.ingestion_jobs where id = ${id}::uuid`);
    return rows[0]!;
  }

  describe("enqueueJob", () => {
    it("insere com os padrões da fila", async () => {
      const job = await seed();
      expect(job.status).toBe("discovered");
      expect(job.requires).toEqual([]);
      expect(job.retryCount).toBe(0);
      expect(job.maxRetries).toBe(3);
      expect(job.lockedBy).toBeNull();
      expect(job.metadata).toEqual({});
    });

    it("preserva requires, metadata e status informados", async () => {
      const job = await seed({
        status: "queued",
        requires: ["ocr", "senha-pdf"],
        metadata: { origem: "gmail", tentativa: 1 },
        maxRetries: 7,
      });
      expect(job.status).toBe("queued");
      expect(job.requires).toEqual(["ocr", "senha-pdf"]);
      expect(job.metadata).toEqual({ origem: "gmail", tentativa: 1 });
      expect(job.maxRetries).toBe(7);
    });

    it("aceita agendamento futuro em available_at", async () => {
      const future = new Date(Date.now() + 60_000);
      const job = await seed({ availableAt: future });
      expect(new Date(job.availableAt).getTime()).toBeGreaterThan(Date.now() + 30_000);
    });
  });

  describe("claim atômico", () => {
    it("dois workers, um job: só um pega", async () => {
      const job = await seed();

      const [a, b] = await Promise.all([
        claimJob({ workerId: "worker-a", db }),
        claimJob({ workerId: "worker-b", db }),
      ]);

      const winners = [a, b].filter((j): j is QueueJob => j !== null);
      expect(winners).toHaveLength(1);
      expect(winners[0]!.id).toBe(job.id);
      expect(["worker-a", "worker-b"]).toContain(winners[0]!.lockedBy);

      const row = await readJob(job.id);
      expect(row.locked_by).toBe(winners[0]!.lockedBy);
      expect(row.locked_at).not.toBeNull();
    });

    it("cinco workers disputando um job produzem exatamente um vencedor", async () => {
      await seed();

      const results = await Promise.all(
        Array.from({ length: 5 }, (_, i) => claimJob({ workerId: `worker-${i}`, db })),
      );

      expect(results.filter((j) => j !== null)).toHaveLength(1);
    });

    it("dez jobs entre dois workers em lote: nenhum job em duplicidade", async () => {
      const ids = new Set<string>();
      for (let i = 0; i < 10; i++) ids.add((await seed()).id);

      const [batchA, batchB] = await Promise.all([
        claimJobs({ workerId: "worker-a", limit: 10, db }),
        claimJobs({ workerId: "worker-b", limit: 10, db }),
      ]);

      const claimed = [...batchA, ...batchB].map((j) => j.id);
      expect(claimed).toHaveLength(10);
      expect(new Set(claimed).size).toBe(10);
      expect(new Set(claimed)).toEqual(ids);
    });

    it("um job já reivindicado não é reivindicado de novo", async () => {
      await seed();
      expect(await claimJob({ workerId: "worker-a", db })).not.toBeNull();
      expect(await claimJob({ workerId: "worker-b", db })).toBeNull();
    });

    it("respeita a ordem da fila (mais antigo primeiro)", async () => {
      const first = await seed();
      await new Promise((r) => setTimeout(r, 15));
      const second = await seed();

      expect((await claimJob({ workerId: "w", db }))!.id).toBe(first.id);
      expect((await claimJob({ workerId: "w", db }))!.id).toBe(second.id);
    });

    it("grava o status do claim quando pedido, e mantém o atual quando não", async () => {
      const a = await seed({ status: "queued" });
      const claimedA = await claimJob({ workerId: "w", db, status: "parsing" });
      expect(claimedA!.id).toBe(a.id);
      expect(claimedA!.status).toBe("parsing");

      const b = await seed({ status: "queued" });
      const claimedB = await claimJob({ workerId: "w", db });
      expect(claimedB!.id).toBe(b.id);
      expect(claimedB!.status).toBe("queued");
    });
  });

  describe("elegibilidade", () => {
    it("job agendado para o futuro não é reivindicado", async () => {
      await seed({ availableAt: new Date(Date.now() + 60_000) });
      expect(await claimJob({ workerId: "w", db })).toBeNull();
    });

    it("status fora da lista de claim é ignorado", async () => {
      await seed({ status: "posted" });
      expect(await claimJob({ workerId: "w", db })).toBeNull();
      expect(await claimJob({ workerId: "w", statuses: ["posted"], db })).not.toBeNull();
    });

    it("todos os CLAIMABLE_STATUSES são de fato reivindicáveis", async () => {
      for (const status of CLAIMABLE_STATUSES) await seed({ status });
      const claimed = await claimJobs({ workerId: "w", limit: 50, db });
      expect(claimed).toHaveLength(CLAIMABLE_STATUSES.length);
    });
  });

  describe("capacidades", () => {
    it("worker sem a capacidade não enxerga o job", async () => {
      await seed({ requires: ["ocr"] });
      expect(await claimJob({ workerId: "vercel", capabilities: [], db })).toBeNull();
    });

    it("worker com a capacidade exata pega", async () => {
      const job = await seed({ requires: ["ocr"] });
      const claimed = await claimJob({ workerId: "vps", capabilities: ["ocr"], db });
      expect(claimed?.id).toBe(job.id);
      expect(claimed?.requires).toEqual(["ocr"]);
    });

    it("worker com superset de capacidades pega", async () => {
      const job = await seed({ requires: ["ocr", "senha-pdf"] });
      const claimed = await claimJob({
        workerId: "wsl",
        capabilities: ["ocr", "pasta-local", "senha-pdf"],
        db,
      });
      expect(claimed?.id).toBe(job.id);
    });

    it("job sem requires é elegível para qualquer worker", async () => {
      const job = await seed();
      expect((await claimJob({ workerId: "pluggy", capabilities: [], db }))?.id).toBe(job.id);
    });

    it("roteia: o job de OCR vai para a VPS, o de pasta local para o WSL", async () => {
      const ocrJob = await seed({ requires: ["ocr"] });
      const localJob = await seed({ requires: ["pasta-local"] });

      const vps = await claimJob({ workerId: "vps", capabilities: ["ocr"], db });
      const wsl = await claimJob({
        workerId: "wsl",
        capabilities: ["ocr", "pasta-local", "senha-pdf"],
        db,
      });

      expect(vps?.id).toBe(ocrJob.id);
      expect(wsl?.id).toBe(localJob.id);
    });
  });

  describe("completeJob", () => {
    it("libera o lock e grava o status final", async () => {
      const job = await seed();
      await claimJob({ workerId: "w", db });
      const done = await completeJob({ jobId: job.id, status: "posted", db });

      expect(done?.status).toBe("posted");
      expect(done?.lockedBy).toBeNull();
      expect(done?.lockedAt).toBeNull();
    });

    it("não escreve quando o lock é de outro worker", async () => {
      const job = await seed();
      await claimJob({ workerId: "worker-vivo", db });

      const result = await completeJob({
        jobId: job.id,
        status: "posted",
        expectedWorkerId: "worker-zumbi",
        db,
      });

      expect(result).toBeNull();
      expect((await readJob(job.id)).locked_by).toBe("worker-vivo");
    });
  });

  describe("failJob", () => {
    it("devolve à fila com backoff exponencial enquanto há tentativas", async () => {
      const job = await seed({ maxRetries: 3 });
      await claimJob({ workerId: "w", db });

      const before = Date.now();
      const result = await failJob({ jobId: job.id, error: new Error("timeout no OCR"), db });

      expect(result?.willRetry).toBe(true);
      expect(result?.job.retryCount).toBe(1);
      expect(result?.job.status).toBe("discovered");
      expect(result?.job.lockedBy).toBeNull();
      expect(result?.job.errorMessage).toBe("timeout no OCR");
      expect(result?.backoffSeconds).toBe(5);
      expect(new Date(result!.job.availableAt).getTime()).toBeGreaterThan(before + 4000);
    });

    it("o job em backoff não é reivindicado antes da hora", async () => {
      const job = await seed();
      await claimJob({ workerId: "w", db });
      await failJob({ jobId: job.id, error: "falhou", db });

      expect(await claimJob({ workerId: "w", db })).toBeNull();
    });

    it("marca failed quando as tentativas acabam", async () => {
      const job = await seed({ maxRetries: 1 });

      await claimJob({ workerId: "w", db });
      const first = await failJob({ jobId: job.id, error: "1", db });
      expect(first?.willRetry).toBe(true);

      await exec(sql`update public.ingestion_jobs set available_at = now()`);
      await claimJob({ workerId: "w", db });
      const second = await failJob({ jobId: job.id, error: "2", db });

      expect(second?.willRetry).toBe(false);
      expect(second?.job.status).toBe("failed");
      expect(second?.job.retryCount).toBe(2);
    });

    it("retry:false falha de imediato, sem gastar as tentativas restantes", async () => {
      const job = await seed({ maxRetries: 10 });
      await claimJob({ workerId: "w", db });

      const result = await failJob({ jobId: job.id, error: "PDF corrompido", retry: false, db });

      expect(result?.willRetry).toBe(false);
      expect(result?.job.status).toBe("failed");
    });

    it("grava error_details e permite restaurar o status na retentativa", async () => {
      const job = await seed({ status: "parsed" });
      await claimJob({ workerId: "w", db, status: "parsing" });

      const result = await failJob({
        jobId: job.id,
        error: "erro",
        details: { etapa: "ocr", pagina: 3 },
        retryStatus: "parsed",
        db,
      });

      expect(result?.job.status).toBe("parsed");
      expect((await readJob(job.id)).error_details).toEqual({ etapa: "ocr", pagina: 3 });
    });

    it("não escreve quando o lock é de outro worker", async () => {
      const job = await seed();
      await claimJob({ workerId: "worker-vivo", db });

      const result = await failJob({
        jobId: job.id,
        error: "erro",
        expectedWorkerId: "worker-zumbi",
        db,
      });

      expect(result).toBeNull();
      expect((await readJob(job.id)).retry_count).toBe(0);
    });

    it("o backoff calculado no SQL concorda com computeBackoffSeconds", async () => {
      // Guarda contra a única duplicação de lógica da fila: a curva vive no
      // CASE de failJob (para manter a operação numa instrução só) e em
      // computeBackoffSeconds (para o chamador poder prever a espera).
      const job = await seed({ maxRetries: 10 });

      for (let attempt = 0; attempt < 4; attempt++) {
        await exec(sql`update public.ingestion_jobs set available_at = now()`);
        await claimJob({ workerId: "w", db });
        const before = Date.now();
        const result = await failJob({ jobId: job.id, error: `tentativa ${attempt}`, db });

        const esperado = computeBackoffSeconds(attempt);
        expect(result?.backoffSeconds).toBe(esperado);

        const deltaMs = new Date(result!.job.availableAt).getTime() - before;
        expect(deltaMs).toBeGreaterThan(esperado * 1000 - 2000);
        expect(deltaMs).toBeLessThan(esperado * 1000 + 2000);
      }
    });
  });

  describe("releaseJob", () => {
    it("devolve o job sem contar como falha", async () => {
      const job = await seed();
      await claimJob({ workerId: "w", db });

      const released = await releaseJob({ jobId: job.id, expectedWorkerId: "w", db });

      expect(released?.lockedBy).toBeNull();
      expect(released?.retryCount).toBe(0);
      expect((await claimJob({ workerId: "outro", db }))?.id).toBe(job.id);
    });
  });

  describe("reapStaleJobs", () => {
    it("devolve à fila o job de um worker que morreu", async () => {
      const job = await seed();
      await claimJob({ workerId: "worker-morto", db });
      await exec(sql`update public.ingestion_jobs set locked_at = now() - interval '30 minutes'`);

      const reaped = await reapStaleJobs({ olderThanMs: 10 * 60_000, db });

      expect(reaped.map((j) => j.id)).toEqual([job.id]);
      expect(reaped[0]!.lockedBy).toBeNull();
      expect((await claimJob({ workerId: "worker-novo", db }))?.id).toBe(job.id);
    });

    it("não rouba job de worker ainda vivo", async () => {
      await seed();
      await claimJob({ workerId: "worker-vivo", db });

      expect(await reapStaleJobs({ olderThanMs: 10 * 60_000, db })).toEqual([]);
    });

    it("ignora jobs que nunca foram reivindicados", async () => {
      await seed();
      expect(await reapStaleJobs({ olderThanMs: 0, db })).toEqual([]);
    });

    it("não zera retry_count nem status ao devolver", async () => {
      const job = await seed({ status: "parsed" });
      await claimJob({ workerId: "morto", db });
      await exec(
        sql`update public.ingestion_jobs set locked_at = now() - interval '1 hour', retry_count = 2`,
      );

      const [reaped] = await reapStaleJobs({ olderThanMs: 60_000, db });

      expect(reaped?.id).toBe(job.id);
      expect(reaped?.retryCount).toBe(2);
      expect(reaped?.status).toBe("parsed");
    });
  });

  describe("waitForJob (LISTEN/NOTIFY com fallback)", () => {
    it("acorda com a notificação do trigger, sem esperar o polling", async () => {
      const notifier = new JobNotifier({ connectionString: TEST_URL, reconnectDelayMs: 100 });
      await notifier.start();
      expect(notifier.listening).toBe(true);

      const controller = new AbortController();
      const started = Date.now();
      // pollIntervalMs alto de propósito: se o teste passar dentro do prazo,
      // foi o NOTIFY que acordou o worker, não o polling.
      const pending = waitForJob({
        workerId: "w",
        db,
        notifier,
        signal: controller.signal,
        pollIntervalMs: 60_000,
      });

      await new Promise((r) => setTimeout(r, 150));
      const job = await seed();

      const claimed = await pending;
      expect(claimed?.id).toBe(job.id);
      expect(Date.now() - started).toBeLessThan(10_000);

      controller.abort();
      await notifier.close();
    });

    it("pega o job pelo polling mesmo sem conexão de LISTEN", async () => {
      // Sem connectionString não há LISTEN — é o cenário de pooler que não
      // entrega notificação, e o worker precisa continuar avançando.
      const notifier = new JobNotifier({ connectionString: undefined });
      const controller = new AbortController();

      const pending = waitForJob({
        workerId: "w",
        db,
        notifier,
        signal: controller.signal,
        pollIntervalMs: 50,
      });

      await new Promise((r) => setTimeout(r, 120));
      const job = await seed();

      expect((await pending)?.id).toBe(job.id);
      controller.abort();
      await notifier.close();
    });

    it("devolve null quando o sinal aborta", async () => {
      const notifier = new JobNotifier({ connectionString: undefined });
      const controller = new AbortController();
      const pending = waitForJob({
        workerId: "w",
        db,
        notifier,
        signal: controller.signal,
        pollIntervalMs: 10_000,
      });

      setTimeout(() => controller.abort(), 50);
      expect(await pending).toBeNull();
      await notifier.close();
    });

    it("reconecta sozinho depois de a conexão de LISTEN cair", async () => {
      const notifier = new JobNotifier({ connectionString: TEST_URL, reconnectDelayMs: 50 });
      await notifier.start();
      expect(notifier.listening).toBe(true);

      // Derruba a conexão de LISTEN pelo lado do servidor.
      await exec(
        sql`select pg_terminate_backend(pid) from pg_stat_activity
            where query like 'LISTEN%' and pid <> pg_backend_pid()`,
      );

      const deadline = Date.now() + 10_000;
      while (!notifier.listening && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 100));
      }
      expect(notifier.listening).toBe(true);

      // E continua entregando notificação depois de reconectar.
      const armed = notifier.arm();
      await seed();
      await expect(
        Promise.race([armed.promise, new Promise((r) => setTimeout(() => r("timeout"), 5000))]),
      ).resolves.toBe("notified");

      await notifier.close();
    });
  });

  describe("runWorkerLoop", () => {
    it("processa a fila e encerra no abort", async () => {
      await seed();
      await seed();
      const processados: string[] = [];
      const controller = new AbortController();

      const loop = runWorkerLoop({
        name: "teste",
        workerId: "w",
        signal: controller.signal,
        db,
        pollIntervalMs: 30,
        reapIntervalMs: 0,
        notifier: new JobNotifier({ connectionString: undefined }),
        logger: { info: () => {}, error: () => {} },
        handle: async (job) => {
          processados.push(job.id);
          if (processados.length === 2) controller.abort();
          return { status: "posted" };
        },
      });

      await loop;
      expect(processados).toHaveLength(2);

      const rows = await exec(sql`select status, locked_by from public.ingestion_jobs`);
      expect(rows.every((r) => r.status === "posted" && r.locked_by === null)).toBe(true);
    });

    it("um handler que lança devolve o job à fila com backoff", async () => {
      const job = await seed();
      const controller = new AbortController();

      await runWorkerLoop({
        name: "teste",
        workerId: "w",
        signal: controller.signal,
        db,
        pollIntervalMs: 30,
        reapIntervalMs: 0,
        notifier: new JobNotifier({ connectionString: undefined }),
        logger: { info: () => {}, error: () => {} },
        handle: async () => {
          controller.abort();
          throw new Error("estourou");
        },
      });

      const row = await readJob(job.id);
      expect(row.retry_count).toBe(1);
      expect(row.locked_by).toBeNull();
      expect(row.error_message).toBe("estourou");
      expect(new Date(row.available_at as string).getTime()).toBeGreaterThan(Date.now() + 1000);
    });

    it("reapeia jobs travados no início do ciclo", async () => {
      const orfao = await seed();
      await claimJob({ workerId: "worker-morto", db });
      await exec(sql`update public.ingestion_jobs set locked_at = now() - interval '1 hour'`);

      const processados: string[] = [];
      const controller = new AbortController();

      await runWorkerLoop({
        name: "teste",
        workerId: "w",
        signal: controller.signal,
        db,
        pollIntervalMs: 30,
        reapIntervalMs: 1,
        staleAfterMs: 60_000,
        notifier: new JobNotifier({ connectionString: undefined }),
        logger: { info: () => {}, error: () => {} },
        handle: async (job) => {
          processados.push(job.id);
          controller.abort();
        },
      });

      expect(processados).toEqual([orfao.id]);
    });

    it("shutdown que estoura o prazo devolve o job à fila em vez de travá-lo", async () => {
      const job = await seed();
      const controller = new AbortController();
      let liberar: (() => void) | null = null;
      const travado = new Promise<void>((resolve) => {
        liberar = resolve;
      });
      let sinalizarInicio: (() => void) | null = null;
      const comecou = new Promise<void>((resolve) => {
        sinalizarInicio = resolve;
      });

      const loop = runWorkerLoop({
        name: "teste",
        workerId: "w",
        signal: controller.signal,
        db,
        pollIntervalMs: 30,
        reapIntervalMs: 0,
        // Prazo curtíssimo: o handler abaixo trava de propósito, então o
        // shutdown precisa desistir de esperar e devolver o job.
        shutdownGraceMs: 50,
        notifier: new JobNotifier({ connectionString: undefined }),
        logger: { info: () => {}, error: () => {} },
        handle: async () => {
          sinalizarInicio?.();
          controller.abort();
          await travado;
        },
      });

      // Só faz sentido observar o release DEPOIS de o job ter sido
      // reivindicado — sem isso o teste passaria trivialmente lendo a linha
      // antes do claim.
      await comecou;
      const claimado = await readJob(job.id);
      expect(claimado.locked_by).toBe("w");

      // Espera o release acontecer pelo caminho do prazo estourado.
      const deadline = Date.now() + 5000;
      let row = await readJob(job.id);
      while (row.locked_by !== null && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 25));
        row = await readJob(job.id);
      }

      expect(row.locked_by).toBeNull();
      expect(row.locked_at).toBeNull();
      // Devolver não é falhar: a tentativa não foi consumida.
      expect(row.retry_count).toBe(0);
      expect(row.status).toBe("discovered");

      // E o job volta a ser reivindicável por outro worker imediatamente.
      expect((await claimJob({ workerId: "outro", db }))?.id).toBe(job.id);

      liberar?.();
      await loop;
    });

    it("respeita as capacidades declaradas", async () => {
      await seed({ requires: ["ocr"] });
      const semOcr = await seed();
      const processados: string[] = [];
      const controller = new AbortController();

      await runWorkerLoop({
        name: "teste",
        workerId: "w",
        capabilities: [],
        signal: controller.signal,
        db,
        pollIntervalMs: 30,
        reapIntervalMs: 0,
        notifier: new JobNotifier({ connectionString: undefined }),
        logger: { info: () => {}, error: () => {} },
        handle: async (job) => {
          processados.push(job.id);
          controller.abort();
        },
      });

      expect(processados).toEqual([semOcr.id]);
      expect((await readJob(semOcr.id)).locked_by).toBeNull();
    });
  });

  describe("sanidade do canal", () => {
    it("o trigger publica no canal que a fila escuta", async () => {
      const rows = await exec(sql`select prosrc from pg_proc where proname = 'notify_sbf_jobs'`);
      expect(String(rows[0]?.prosrc)).toContain(JOB_CHANNEL);
    });

    it("a coluna requires aceita o literal gerado pelo cliente", async () => {
      const job = await seed({ requires: ['com "aspas"', "com,virgula"] });
      const row = await readJob(job.id);
      expect(row.requires).toEqual(['com "aspas"', "com,virgula"]);
    });
  });
});
