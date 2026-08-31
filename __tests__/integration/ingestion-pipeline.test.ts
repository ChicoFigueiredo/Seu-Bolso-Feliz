/**
 * Testes de integração: Pipeline de Ingestão
 *
 * 2.27 — Scanner local → Worker → Banco (fluxo completo)
 * 2.28 — Deduplicação: mesmo documento 2x não duplica
 * 2.29 — Reprocessamento forçado (force_reprocess)
 *
 * Roda contra Postgres real (ver `__tests__/helpers/db.ts`) e contra uma loja
 * de blobs em memória (ver `__tests__/helpers/fake-storage.ts`). O dublê fica
 * no SDK do Vercel Blob, e não no `@sbf/storage`, porque o worker não recebe
 * cliente de storage por parâmetro: ele resolve o dele por ambiente
 * (`getIngestionBucket()`), e injetar onde a produção não injeta testaria
 * outro caminho.
 *
 * MUDANÇA DE CONTRATO que este arquivo passou a exercer: `processJob` não
 * marca mais `failed` por conta própria quando algo dá errado. Ele registra o
 * contexto e **propaga** — quem decide entre retentar com backoff e aposentar
 * o documento é a fila (`failJob` em packages/db/src/queue.ts). O último
 * describe prova isso; até então, um erro transitório de storage aposentava o
 * documento para sempre e `retry_count`/`max_retries` eram colunas decorativas.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach, vi } from "vitest";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { sql } from "drizzle-orm";
import { failJob, type Database, type QueueJob } from "@sbf/db";
import { IngestionJobStatus, IngestionRunStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { scanDirectory } from "../../workers/local-scanner/src/scanner";
import { processJob } from "../../workers/ingestion/src/processor";
import { resetIngestionBucket } from "../../workers/ingestion/src/storage";
import {
  getTestDb,
  limparPorUsuario,
  linha,
  linhas,
  semBancoDeTeste,
  semearUsuarios,
  tentar,
} from "../helpers/db";
import { startFakeStorage, type FakeStorage } from "../helpers/fake-storage";
import { jobPorId, jobsDoUsuario } from "../helpers/jobs";

// Precisa vir antes de qualquer coisa que importe `@sbf/storage`: é o que põe
// a loja de blobs em memória no lugar do SDK do Vercel Blob.
vi.mock("../../packages/storage/src/blob-api", async () => {
  const { moduloBlobApiFalso } = await import("../helpers/fake-storage");
  return moduloBlobApiFalso();
});

const USER_ID = "70000000-0000-4000-8000-000000000001";
const INBOX_DIR = join(process.cwd(), "__tests__/integration/.test-inbox");

/** Filhas antes das pais. */
const TABELAS = [
  "draft_records",
  "draft_batches",
  "extraction_results",
  "parsed_document_versions",
  "ingestion_logs",
  "document_fingerprints",
  "ingestion_jobs",
  "source_documents",
  "ingestion_runs",
];

let db: Database;
let storage: FakeStorage;

async function createTestFile(name: string, content: string): Promise<string> {
  const path = join(INBOX_DIR, name);
  await writeFile(path, content);
  return path;
}

async function getRuns() {
  return linhas<{ status: string; source_type: string }>(
    db,
    sql`select status, source_type from public.ingestion_runs
         where user_id = ${USER_ID}::uuid order by created_at asc`,
  );
}

async function getDocuments() {
  return linhas<{
    id: string;
    filename: string;
    mime_type: string;
    storage_path: string;
    origin_key: string;
  }>(
    db,
    sql`select id, filename, mime_type, storage_path, origin_key
          from public.source_documents where user_id = ${USER_ID}::uuid
         order by created_at asc`,
  );
}

async function getFingerprints() {
  return linhas<{
    content_hash: string;
    canonical_fingerprint: string;
    source_document_id: string;
  }>(
    db,
    sql`select content_hash, canonical_fingerprint, source_document_id
          from public.document_fingerprints where user_id = ${USER_ID}::uuid
         order by created_at asc`,
  );
}

async function getLogs() {
  return linhas<{ job_id: string | null; message: string }>(
    db,
    sql`select job_id, message from public.ingestion_logs
         where user_id = ${USER_ID}::uuid order by created_at asc`,
  );
}

describe.skipIf(semBancoDeTeste)("pipeline de ingestão", () => {
  beforeAll(async () => {
    db = getTestDb();
    // Zera a loja em memória, põe o token de mentira no ambiente e descarta o
    // bucket já resolvido (o singleton do worker guarda o cliente construído
    // na primeira chamada).
    storage = await startFakeStorage();
    resetIngestionBucket();
    process.env.LOCAL_USER_ID = USER_ID;
    await semearUsuarios(db, USER_ID);
  });

  afterAll(async () => {
    await rm(INBOX_DIR, { recursive: true, force: true }).catch(() => {});
    await limparPorUsuario(db, TABELAS, USER_ID);
    resetIngestionBucket();
    await storage.close();
  });

  beforeEach(async () => {
    await rm(INBOX_DIR, { recursive: true, force: true }).catch(() => {});
    await mkdir(INBOX_DIR, { recursive: true });
    await limparPorUsuario(db, TABELAS, USER_ID);
  });

  // ══════════════════════════════════════════════════════════════
  // 2.27 — Scanner local → Worker → Banco (fluxo completo)
  // ══════════════════════════════════════════════════════════════

  describe("2.27: Scanner → Worker → Banco (fluxo integrado)", () => {
    it("escaneia arquivo, cria job, processa até 'pending_review'", async () => {
      await createTestFile("conta-luz.csv", "Conta de luz CEMIG - Março 2026 - R$ 245,50");

      // === ETAPA 1: Scanner escaneia o diretório ===
      const discovered = await scanDirectory(db, INBOX_DIR);
      expect(discovered).toBe(1);

      const runs = await getRuns();
      expect(runs).toHaveLength(1);
      expect(runs[0]!.status).toBe(IngestionRunStatus.COMPLETED);
      expect(runs[0]!.source_type).toBe(SourceDocumentOrigin.LOCAL_FILE);

      const docs = await getDocuments();
      expect(docs).toHaveLength(1);
      expect(docs[0]!.filename).toBe("conta-luz.csv");
      expect(docs[0]!.mime_type).toBe("text/csv");
      expect(docs[0]!.storage_path).toBeTruthy();
      expect(docs[0]!.origin_key).toContain("local:");

      // O arquivo subiu de verdade — o storage tem o objeto sob a chave que o
      // documento aponta.
      expect(storage.chaves()).toContain(`ingestion-originals/${docs[0]!.storage_path}`);

      const jobs = await jobsDoUsuario(db, USER_ID);
      expect(jobs).toHaveLength(1);
      expect(jobs[0]!.status).toBe(IngestionJobStatus.DISCOVERED);
      expect(jobs[0]!.sourceDocumentId).toBe(docs[0]!.id);

      // === ETAPA 2: Worker processa o job ===
      await processJob(db, jobs[0]!);

      const atualizados = await jobsDoUsuario(db, USER_ID);
      expect(atualizados[0]!.status).toBe(IngestionJobStatus.PENDING_REVIEW);

      const fps = await getFingerprints();
      expect(fps).toHaveLength(1);
      expect(fps[0]!.content_hash).toBeTruthy();
      expect(fps[0]!.canonical_fingerprint).toBeTruthy();
      expect(fps[0]!.source_document_id).toBe(docs[0]!.id);

      const logs = await getLogs();
      expect(logs.length).toBeGreaterThanOrEqual(2);
      const messages = logs.map((l) => l.message);
      expect(messages.some((m) => m.includes("Downloaded"))).toBe(true);
      expect(messages.some((m) => m.includes("queued") || m.includes("proceeding"))).toBe(true);
    });

    it("escaneia múltiplos arquivos e cria jobs independentes", async () => {
      await createTestFile("fatura-nubank.csv", "Fatura Nubank Abril 2026 R$1.200,00");
      await createTestFile(
        "boleto-internet.csv",
        "Boleto Vivo Internet R$99,90 Vencimento 15/04/2026",
      );

      const discovered = await scanDirectory(db, INBOX_DIR);
      expect(discovered).toBe(2);

      expect(await getDocuments()).toHaveLength(2);

      const jobs = await jobsDoUsuario(db, USER_ID);
      expect(jobs).toHaveLength(2);
      expect(jobs.every((j) => j.status === IngestionJobStatus.DISCOVERED)).toBe(true);

      for (const job of jobs) await processJob(db, job);

      const atualizados = await jobsDoUsuario(db, USER_ID);
      expect(atualizados.every((j) => j.status === IngestionJobStatus.PENDING_REVIEW)).toBe(true);

      expect(await getFingerprints()).toHaveLength(2);
    });

    it("ignora arquivos com extensão não aceita", async () => {
      await createTestFile("readme.txt", "Este arquivo não é aceito");
      await createTestFile("conta.csv", "CSV aceito");

      const discovered = await scanDirectory(db, INBOX_DIR);
      expect(discovered).toBe(1);

      const docs = await getDocuments();
      expect(docs).toHaveLength(1);
      expect(docs[0]!.filename).toBe("conta.csv");
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 2.28 — Deduplicação: mesmo documento 2x não duplica
  // ══════════════════════════════════════════════════════════════

  describe("2.28: Deduplicação — mesmo doc não duplica", () => {
    it("scanner ignora arquivo já processado (mesma origin_key)", async () => {
      await createTestFile("conta-cemig.csv", "CEMIG Março 2026");

      expect(await scanDirectory(db, INBOX_DIR)).toBe(1);
      // Mesmo arquivo, mesmo mtime → a origin_key coincide e o scanner pula.
      expect(await scanDirectory(db, INBOX_DIR)).toBe(0);

      expect(await getDocuments()).toHaveLength(1);
      expect(await jobsDoUsuario(db, USER_ID)).toHaveLength(1);
    });

    it("worker rejeita documento com hash idêntico no step de hash", async () => {
      const content = "Mesmo conteúdo exato para testar dedup";

      await createTestFile("doc-a.csv", content);
      await scanDirectory(db, INBOX_DIR);

      const [primeiro] = await jobsDoUsuario(db, USER_ID);
      await processJob(db, primeiro!);

      // Nome (e portanto origin_key) diferente, conteúdo igual: o scanner
      // aceita, e a rejeição tem que vir do hash, no worker.
      await createTestFile("doc-b.csv", content);
      expect(await scanDirectory(db, INBOX_DIR)).toBe(1);

      const segundo = (await jobsDoUsuario(db, USER_ID)).find(
        (j) => j.status === IngestionJobStatus.DISCOVERED,
      );
      expect(segundo).toBeTruthy();

      await processJob(db, segundo!);

      // Duplicata não é erro transitório: retentar não muda o resultado, então
      // o próprio processador encerra o job em `failed` — e é por isso que
      // este caminho não passa pela política de retentativa da fila.
      const processado = await jobPorId(db, segundo!.id);
      expect(processado.status).toBe(IngestionJobStatus.FAILED);
      expect(processado.errorMessage).toBe("Duplicate document detected");

      const logs = await getLogs();
      const dedupLog = logs.find(
        (l) =>
          l.job_id === segundo!.id &&
          (l.message.includes("reject_exact") ||
            l.message.includes("alert_semantic") ||
            l.message.includes("Duplicate")),
      );
      expect(dedupLog).toBeTruthy();
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 2.29 — Reprocessamento forçado (force_reprocess)
  // ══════════════════════════════════════════════════════════════

  describe("2.29: Reprocessamento forçado", () => {
    it("job com force_reprocess=true ignora dedup e vai até PENDING_REVIEW", async () => {
      const content = "Documento para reprocessar";

      await createTestFile("reprocess-me.csv", content);
      await scanDirectory(db, INBOX_DIR);

      const [primeiro] = await jobsDoUsuario(db, USER_ID);
      await processJob(db, primeiro!);
      expect((await jobPorId(db, primeiro!.id)).status).toBe(IngestionJobStatus.PENDING_REVIEW);

      await createTestFile("reprocess-me-v2.csv", content);
      await scanDirectory(db, INBOX_DIR);

      const novo = (await jobsDoUsuario(db, USER_ID)).find(
        (j) => j.status === IngestionJobStatus.DISCOVERED,
      );
      expect(novo).toBeTruthy();

      await db.execute(sql`
        update public.ingestion_jobs
           set metadata = coalesce(metadata, '{}'::jsonb) || '{"force_reprocess": true}'::jsonb
         where id = ${novo!.id}::uuid and user_id = ${USER_ID}::uuid
      `);

      await processJob(db, await jobPorId(db, novo!.id));

      // Passou pela dedup em vez de ser rejeitado por ela.
      expect((await jobPorId(db, novo!.id)).status).toBe(IngestionJobStatus.PENDING_REVIEW);
    });

    it("job FAILED pode ser retomado de DISCOVERED com retry", async () => {
      await createTestFile("retry-test.csv", "Documento de retry");
      await scanDirectory(db, INBOX_DIR);

      const [job] = await jobsDoUsuario(db, USER_ID);
      const jobId = job!.id;

      // Simula uma falha já registrada e o reset que um retry manual faria.
      await db.execute(sql`
        update public.ingestion_jobs
           set status = ${IngestionJobStatus.FAILED},
               error_message = 'Simulated failure for retry test',
               error_details = '{"previous_status": "discovered"}'::jsonb
         where id = ${jobId}::uuid and user_id = ${USER_ID}::uuid
      `);
      await db.execute(sql`
        update public.ingestion_jobs
           set status = ${IngestionJobStatus.DISCOVERED}, retry_count = 1,
               error_message = null, error_details = null
         where id = ${jobId}::uuid and user_id = ${USER_ID}::uuid
      `);

      await processJob(db, await jobPorId(db, jobId));

      expect((await jobPorId(db, jobId)).status).toBe(IngestionJobStatus.PENDING_REVIEW);
    });
  });

  // ══════════════════════════════════════════════════════════════
  // Contrato de falha: quem decide retentar é a fila
  // ══════════════════════════════════════════════════════════════

  describe("falha transitória: processJob propaga, a fila decide", () => {
    /** Deixa o documento apontando para uma chave que o storage não tem. */
    async function quebrarOStoragePath(job: QueueJob): Promise<void> {
      await db.execute(sql`
        update public.source_documents
           set storage_path = ${`${USER_ID}/inexistente/sumido.csv`}
         where id = ${job.sourceDocumentId!}::uuid and user_id = ${USER_ID}::uuid
      `);
    }

    it("propaga o erro em vez de marcar failed sozinho", async () => {
      await createTestFile("some-do-storage.csv", "Some do storage");
      await scanDirectory(db, INBOX_DIR);

      const [job] = await jobsDoUsuario(db, USER_ID);
      await quebrarOStoragePath(job!);

      const { error } = await tentar(() => processJob(db, job!));
      expect(error).not.toBeNull();
      expect(error!.message).toMatch(/Failed to download/);

      // O job continua exatamente onde estava: nada de `failed` unilateral.
      const depois = await jobPorId(db, job!.id);
      expect(depois.status).toBe(IngestionJobStatus.DISCOVERED);

      // E o documento não foi marcado como falho — ainda há tentativas.
      const doc = await linha<{ status: string }>(
        db,
        sql`select status from public.source_documents where id = ${job!.sourceDocumentId!}::uuid`,
      );
      expect(doc!.status).not.toBe("failed");
    });

    it("a fila devolve o job com backoff enquanto houver tentativa, e só então falha", async () => {
      await createTestFile("esgota-tentativas.csv", "Esgota tentativas");
      await scanDirectory(db, INBOX_DIR);

      const [inicial] = await jobsDoUsuario(db, USER_ID);
      await quebrarOStoragePath(inicial!);
      await db.execute(sql`
        update public.ingestion_jobs set max_retries = 2
         where id = ${inicial!.id}::uuid and user_id = ${USER_ID}::uuid
      `);

      const desfechos: { status: string; willRetry: boolean }[] = [];
      for (let tentativa = 0; tentativa < 3; tentativa++) {
        // Relê o job direto, sem respeitar `available_at`: o backoff é
        // asserido logo abaixo, e esperá-lo de verdade custaria dezenas de
        // segundos por uma informação que já está na resposta de `failJob`.
        const job = await jobPorId(db, inicial!.id);
        const { error } = await tentar(() => processJob(db, job));
        expect(error).not.toBeNull();

        const resultado = await failJob({ db, jobId: job.id, error: error!, retry: true });
        desfechos.push({ status: resultado!.job.status, willRetry: resultado!.willRetry });

        // O backoff é real: o job sai de circulação até `available_at`.
        if (resultado!.willRetry) expect(resultado!.backoffSeconds).toBeGreaterThan(0);
      }

      expect(desfechos.map((d) => d.willRetry)).toEqual([true, true, false]);
      expect(desfechos.map((d) => d.status)).toEqual([
        IngestionJobStatus.DISCOVERED,
        IngestionJobStatus.DISCOVERED,
        IngestionJobStatus.FAILED,
      ]);

      // Só na última tentativa o documento é dado como perdido para a interface.
      const doc = await linha<{ status: string }>(
        db,
        sql`select status from public.source_documents
             where id = ${inicial!.sourceDocumentId!}::uuid`,
      );
      expect(doc!.status).toBe("failed");
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 4.16 — Fornecedor e período sugeridos corretamente
  // ══════════════════════════════════════════════════════════════

  describe("4.16: Fornecedor e período sugeridos corretamente", () => {
    it("documento CEMIG gera extraction_result com categoria e tags", async () => {
      const cemigText = [
        "CEMIG DISTRIBUIÇÃO S.A.",
        "Mês Referência MARÇO/2026",
        "Valor a Pagar R$ 245,50",
        "Vencimento: 15/04/2026",
        "Nota Fiscal 123456",
        "Unidade Consumidora: 12345678",
        "Consumo kWh: 320",
        "Quantidade de dias: 30",
      ].join("\n");
      await createTestFile("conta-cemig-detalhada.csv", cemigText);

      await scanDirectory(db, INBOX_DIR);
      const [job] = await jobsDoUsuario(db, USER_ID);
      await processJob(db, job!);

      const results = await linhas<{
        supplier_name_raw: string;
        category_suggestion: string;
        tags_suggestion: string[];
        competence_date: string | null;
        total_amount: number;
      }>(
        db,
        sql`select supplier_name_raw, category_suggestion, tags_suggestion,
                   competence_date::text as competence_date, total_amount
              from public.extraction_results where user_id = ${USER_ID}::uuid`,
      );

      expect(results.length).toBeGreaterThanOrEqual(1);

      const er = results[0]!;
      expect(er.supplier_name_raw).toContain("CEMIG");
      expect(er.category_suggestion).toBe("energia_eletrica");
      expect(er.tags_suggestion).toContain("essencial");
      expect(er.tags_suggestion).toContain("energia");
      expect(er.competence_date).toBeTruthy();
      expect(Number(er.total_amount)).toBe(245.5);
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 4.17 — Drafts não poluem ledger principal
  // ══════════════════════════════════════════════════════════════

  describe("4.17: Drafts não poluem ledger principal", () => {
    it("processar documento até PENDING_REVIEW não cria transactions", async () => {
      await createTestFile("boleto-nao-polui.csv", "Boleto Vivo R$99,90 Vencimento 15/04/2026");

      await scanDirectory(db, INBOX_DIR);
      const [job] = await jobsDoUsuario(db, USER_ID);
      await processJob(db, job!);

      expect((await jobPorId(db, job!.id)).status).toBe(IngestionJobStatus.PENDING_REVIEW);

      const drafts = await linhas<{
        status: string;
        posted_record_id: string | null;
        posted_record_type: string | null;
      }>(
        db,
        sql`select status, posted_record_id, posted_record_type
              from public.draft_records where user_id = ${USER_ID}::uuid`,
      );
      expect(drafts.length).toBeGreaterThanOrEqual(1);
      expect(drafts[0]!.status).toBe("pending_review");

      // Nada foi lançado: o draft não aponta para registro nenhum...
      expect(drafts[0]!.posted_record_id).toBeNull();
      expect(drafts[0]!.posted_record_type).toBeNull();

      // ...e o ledger continua vazio. A versão anterior deste teste afirmava
      // isso só pelo draft, o que deixaria passar um pipeline que inserisse em
      // `transactions` por outro caminho.
      const total = await linha<{ total: string }>(
        db,
        sql`select count(*)::text as total from public.transactions
             where user_id = ${USER_ID}::uuid`,
      );
      expect(Number(total!.total)).toBe(0);
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 4.18 — Baixa confiança vai para revisão
  // ══════════════════════════════════════════════════════════════

  describe("4.18: Baixa confiança vai para revisão", () => {
    it("documento com texto genérico gera draft com status pending_review", async () => {
      // Texto sem padrão reconhecível → baixa confiança (0.3 = texto bruto)
      await createTestFile("generico.csv", "Algum texto qualquer sem padrão de boleto ou CEMIG");

      await scanDirectory(db, INBOX_DIR);
      const [job] = await jobsDoUsuario(db, USER_ID);
      await processJob(db, job!);

      expect((await jobPorId(db, job!.id)).status).toBe(IngestionJobStatus.PENDING_REVIEW);

      const drafts = await linhas<{ status: string; confidence_score: number }>(
        db,
        sql`select status, confidence_score from public.draft_records
             where user_id = ${USER_ID}::uuid`,
      );

      expect(drafts.length).toBeGreaterThanOrEqual(1);
      expect(drafts[0]!.status).toBe("pending_review");
      // Texto genérico não é documento financeiro reconhecível.
      expect(Number(drafts[0]!.confidence_score)).toBeLessThan(1.0);
    });
  });

  // ══════════════════════════════════════════════════════════════
  // 4.19 — Reprocessamento gera nova versão
  // ══════════════════════════════════════════════════════════════

  describe("4.19: Reprocessamento gera nova versão", () => {
    it("reprocessar documento cria parsed_document_versions v1 e v2", async () => {
      await createTestFile("multi-versao.csv", "CEMIG DISTRIBUIÇÃO Conta MARÇO/2026 R$ 100,00");

      await scanDirectory(db, INBOX_DIR);
      const [job] = await jobsDoUsuario(db, USER_ID);
      const jobId = job!.id;
      const sourceDocId = job!.sourceDocumentId!;

      await processJob(db, job!);

      const versoes = async () =>
        linhas<{ version_number: number }>(
          db,
          sql`select version_number from public.parsed_document_versions
               where source_document_id = ${sourceDocId}::uuid
                 and user_id = ${USER_ID}::uuid
               order by version_number asc`,
        );

      const v1 = await versoes();
      expect(v1).toHaveLength(1);
      expect(Number(v1[0]!.version_number)).toBe(1);

      // Reprocessamento: volta o job para QUEUED e limpa os drafts anteriores.
      await db.execute(sql`
        update public.ingestion_jobs
           set status = ${IngestionJobStatus.QUEUED}, retry_count = 1
         where id = ${jobId}::uuid and user_id = ${USER_ID}::uuid
      `);
      await limparPorUsuario(db, ["draft_records", "draft_batches"], USER_ID);

      await processJob(db, await jobPorId(db, jobId));

      const v2 = await versoes();
      expect(v2).toHaveLength(2);
      expect(v2.map((v) => Number(v.version_number))).toEqual([1, 2]);
    });
  });
});
