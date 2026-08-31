/**
 * E2E: um documento fecha o ciclo (Marco 1 do §18).
 *
 * Prova, com execução, que a informação percorre o fluxo inteiro e chega ao
 * domínio financeiro definitivo:
 *
 *   PDF real → ingestão → extração → obrigação canônica → draft
 *   → guarda de pending_review → aprovação → lançamento atômico
 *   → transação real → replay idempotente → duplicata cross-canal
 *
 * Antes deste ciclo, cada elo existia isoladamente e a corrente estava
 * partida em três pontos: contratos incompatíveis, materializador sem
 * chamadores e aprovação que não lançava nada.
 *
 * RESTRIÇÃO PLANEJADA, não descoberta no meio: Server Actions do Next
 * ("use server") não executam sob vitest — precisam de contexto de request.
 * Por isso o E2E mira a função do banco diretamente, que é onde a garantia
 * mora.
 *
 * Depois da ADR-009 o ciclo roda contra Postgres puro (sem RLS: todo filtro de
 * `user_id` é explícito) e contra uma loja de blobs em memória. O `drain` deixou
 * de ser um laço que chama `processJob` e engole o resultado: agora é o mesmo
 * contrato do worker de verdade — processa, e em caso de erro entrega a
 * decisão de retentar à fila.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll, vi } from "vitest";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { sql } from "drizzle-orm";
import {
  CLAIMABLE_STATUSES,
  completeJob,
  failJob,
  financial_products,
  institutions,
  type Database,
} from "@sbf/db";
import { scanDirectory } from "@sbf/worker-local-scanner";
import { processJob } from "@sbf/worker-ingestion";
import { resetIngestionBucket } from "../../workers/ingestion/src/storage";
import { POSTABLE_SCHEMAS, TO_INSERT, TARGET_TABLES, parseDraftPayload } from "@sbf/contracts";
import { boletoCemig } from "../fixtures/documents/boleto-cemig";
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
import { jobsDoUsuario } from "../helpers/jobs";

// Precisa vir antes de qualquer coisa que importe `@sbf/storage`: é o que põe
// a loja de blobs em memória no lugar do SDK do Vercel Blob.
vi.mock("../../packages/storage/src/blob-api", async () => {
  const { moduloBlobApiFalso } = await import("../helpers/fake-storage");
  return moduloBlobApiFalso();
});

const USER_ID = "80000000-0000-4000-8000-000000000001";
const INBOX = join(process.cwd(), "__tests__/e2e/.e2e-inbox");

/**
 * Ordem importa: as tabelas filhas saem antes das pais.
 *
 * `parsed_document_versions` estava faltando numa primeira versão desta lista
 * e, por ser referenciada em cascata a partir de `source_documents`, fazia o
 * delete dos documentos violar FK — silenciosamente, porque o `.delete()` do
 * driver anterior, sem checagem de erro, não reclamava. Hoje `limparPorUsuario`
 * propaga a violação, então uma lista errada quebra o teste em vez de deixar
 * documento acumulado entre execuções.
 *
 * `audit_logs` fica de fora de propósito: a tabela é imutável por trigger
 * (`trg_audit_logs_immutable`). É a propriedade certa para uma trilha de
 * auditoria, então o teste se adapta a ela em vez de contorná-la.
 */
const TABELAS = [
  "transactions",
  "draft_records",
  "draft_batches",
  "financial_obligation_evidences",
  "financial_obligation_identity_keys",
  "financial_obligations",
  "ingestion_logs",
  "ingestion_jobs",
  "extraction_results",
  "parsed_document_versions",
  "document_fingerprints",
  "source_documents",
  "ingestion_runs",
];

let db: Database;
let storage: FakeStorage;
let productId: string;

interface MaterializeResult {
  status: string;
  posted_record_id: string;
}

/**
 * Esvazia a fila do usuário como o worker faria.
 *
 * O laço lê só os jobs deste usuário (e não `claimJobs`, que por desenho
 * atende a fila inteira e roubaria trabalho de outro arquivo de teste), mas o
 * desfecho é o do worker real: sucesso solta o lock, erro vai para `failJob` e
 * é a fila — não o processador — que decide entre retentar com backoff e
 * aposentar o documento.
 */
async function drain(): Promise<void> {
  for (let rodada = 0; rodada < 20; rodada++) {
    const agora = Date.now();
    const pendentes = (await jobsDoUsuario(db, USER_ID)).filter(
      (j) => CLAIMABLE_STATUSES.includes(j.status) && Date.parse(j.availableAt) <= agora,
    );
    if (pendentes.length === 0) return;

    for (const job of pendentes) {
      const { error } = await tentar(() => processJob(db, job));
      if (error) {
        await failJob({ db, jobId: job.id, error, retry: true });
      } else {
        await completeJob({ db, jobId: job.id });
      }
    }
  }
}

async function materialize(draftId: string, payload: unknown, table: string) {
  return tentar(async () => {
    const row = await linha<{ resultado: MaterializeResult }>(
      db,
      sql`select public.fn_materialize_draft_record(
        ${USER_ID}::uuid,
        ${draftId}::uuid,
        ${table}::text,
        ${JSON.stringify(payload)}::jsonb,
        'e2e'::text
      ) as resultado`,
    );
    return row!.resultado;
  });
}

async function contar(tabela: string): Promise<number> {
  const row = await linha<{ total: string }>(
    db,
    sql`select count(*)::text as total from public.${sql.raw(tabela)}
         where user_id = ${USER_ID}::uuid`,
  );
  return Number(row!.total);
}

describe.skipIf(semBancoDeTeste)("Marco 1 — um documento fecha o ciclo", () => {
  beforeAll(async () => {
    db = getTestDb();
    storage = await startFakeStorage();
    resetIngestionBucket();
    process.env.LOCAL_USER_ID = USER_ID;
    await semearUsuarios(db, USER_ID);

    await limparPorUsuario(db, [...TABELAS, "financial_products", "institutions"], USER_ID);

    // A conta é obrigatória: transactions.financial_product_id é NOT NULL e
    // nada no pipeline consegue inferi-la a partir do documento.
    const [inst] = await db
      .insert(institutions)
      .values({ user_id: USER_ID, name: "Banco E2E" })
      .returning({ id: institutions.id });

    const [prod] = await db
      .insert(financial_products)
      .values({
        user_id: USER_ID,
        institution_id: inst!.id,
        name: "Conta Corrente E2E",
        type: "checking_account",
      })
      .returning({ id: financial_products.id });
    productId = prod!.id;

    // Recriar do zero: arquivos remanescentes de uma execução anterior seriam
    // redescobertos e quebrariam as contagens.
    await rm(INBOX, { recursive: true, force: true });
    await mkdir(INBOX, { recursive: true });
  });

  afterAll(async () => {
    await limparPorUsuario(db, [...TABELAS, "financial_products", "institutions"], USER_ID);
    await rm(INBOX, { recursive: true, force: true });
    resetIngestionBucket();
    await storage.close();
  });

  it("PDF real percorre ingestão → obrigação → draft → aprovação → transação", async () => {
    // ── 1. Ingestão de um PDF de verdade ─────────────────────────────────
    // Nenhum teste do repositório exercitava o caminho do pdf-parse: toda
    // entrada era uma string sintética escrita num .csv.
    await writeFile(join(INBOX, boletoCemig.filename), boletoCemig.bytes());

    const discovered = await scanDirectory(db, INBOX);
    expect(discovered).toBe(1);

    await drain();

    const docs = await linhas<{ id: string; filename: string; mime_type: string }>(
      db,
      sql`select id, filename, mime_type from public.source_documents
           where user_id = ${USER_ID}::uuid`,
    );
    expect(docs).toHaveLength(1);
    expect(docs[0]!.mime_type).toBe("application/pdf");

    // ── 2. Extração leu o PDF ────────────────────────────────────────────
    const extractions = await linhas(
      db,
      sql`select total_amount, due_date, supplier_name_raw from public.extraction_results
           where user_id = ${USER_ID}::uuid`,
    );
    expect(extractions.length).toBeGreaterThanOrEqual(1);

    // ── 3. Draft gerado no contrato v1 ───────────────────────────────────
    const drafts = await linhas<{
      id: string;
      draft_type: string;
      status: string;
      draft_data: Record<string, unknown>;
      draft_schema_version: number;
    }>(
      db,
      sql`select id, draft_type, status, draft_data, draft_schema_version
            from public.draft_records where user_id = ${USER_ID}::uuid`,
    );

    expect(drafts.length).toBeGreaterThanOrEqual(1);
    const draft = drafts.find((d) => d.draft_type === "transaction")!;
    expect(draft).toBeDefined();
    expect(draft.status).toBe("pending_review");
    expect(Number(draft.draft_schema_version)).toBe(1);

    // O payload não pode mais conter o contrato antigo.
    expect(JSON.stringify(draft.draft_data)).not.toContain("despesa");
    expect(draft.draft_data.direction).toBe("outflow");

    // ── 4. GUARDA: pending_review não vira dinheiro ──────────────────────
    const guard = await materialize(draft.id, { financial_product_id: productId }, "transactions");
    expect(guard.error).not.toBeNull();
    expect(guard.error!.message).toMatch(/pending_review/);

    expect(await contar("transactions")).toBe(0);

    // ── 5. Humano completa o que a extração não podia saber ──────────────
    const completed = {
      ...draft.draft_data,
      financial_product_id: productId,
      transaction_type: "expense",
      event_date: "2026-04-15",
    };

    await db.execute(sql`
      update public.draft_records
         set draft_data = ${JSON.stringify(completed)}::jsonb, status = 'corrected'
       where id = ${draft.id}::uuid and user_id = ${USER_ID}::uuid
    `);

    // ── 6. Aprovação NÃO lança nada ──────────────────────────────────────
    await db.execute(sql`
      update public.draft_records
         set status = 'approved', approved_at = now()
       where id = ${draft.id}::uuid and user_id = ${USER_ID}::uuid
    `);

    const aposAprovacao = await linha<{ status: string; posted_record_id: string | null }>(
      db,
      sql`select status, posted_record_id from public.draft_records
           where id = ${draft.id}::uuid and user_id = ${USER_ID}::uuid`,
    );

    expect(aposAprovacao!.status).toBe("approved");
    expect(aposAprovacao!.posted_record_id).toBeNull();

    // ── 7. Lançamento, pelo mesmo contrato que o gerador usou ────────────
    const parsed = parseDraftPayload({
      draft_type: "transaction",
      draft_data: completed,
      draft_schema_version: 1,
    });
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;

    const postable = POSTABLE_SCHEMAS.transaction.safeParse(parsed.payload);
    expect(postable.success).toBe(true);
    if (!postable.success) return;

    const insert = (TO_INSERT.transaction as (p: unknown) => unknown)(postable.data);
    const posted = await materialize(draft.id, insert, TARGET_TABLES.transaction);

    expect(posted.error).toBeNull();
    expect(posted.data!.status).toBe("posted");

    // ── 8. A transação existe, com os valores certos ─────────────────────
    const postedId = posted.data!.posted_record_id;
    const tx = await linha<{
      amount: string;
      type: string;
      event_date: string;
      origin_type: string;
    }>(
      db,
      sql`select amount, type, event_date::text as event_date, origin_type
            from public.transactions
           where id = ${postedId}::uuid and user_id = ${USER_ID}::uuid`,
    );

    expect(Number(tx!.amount)).toBe(boletoCemig.expected.amountCents / 100);
    expect(tx!.type).toBe("expense");
    expect(tx!.event_date).toBe("2026-04-15");
    expect(tx!.origin_type).toBe("import");

    const draftFinal = await linha<{
      status: string;
      posted_record_id: string;
      posted_at: string | null;
    }>(
      db,
      sql`select status, posted_record_id, posted_at from public.draft_records
           where id = ${draft.id}::uuid and user_id = ${USER_ID}::uuid`,
    );
    expect(draftFinal!.status).toBe("posted");
    expect(draftFinal!.posted_record_id).toBe(postedId);
    expect(draftFinal!.posted_at).not.toBeNull();

    // ── 9. Auditoria gravou (antes escrevia em colunas inexistentes) ─────
    const audit = await linhas(
      db,
      sql`select action, entity_id from public.audit_logs
           where user_id = ${USER_ID}::uuid and action = 'draft_materialized'
             and entity_id = ${postedId}::uuid`,
    );
    expect(audit.length).toBeGreaterThanOrEqual(1);

    // ── 10. Replay: lançar de novo não duplica ───────────────────────────
    const replay = await materialize(draft.id, insert, TARGET_TABLES.transaction);
    expect(replay.data!.status).toBe("already_posted");

    expect(await contar("transactions")).toBe(1);
  });

  it("o mesmo documento por outro canal não vira segunda despesa", async () => {
    // Simula o boleto chegando também pelo Gmail: outro origin_key, mesmo
    // conteúdo. Deve anexar evidência à obrigação existente, sem novo lote.
    const antes = await contar("financial_obligations");

    await writeFile(join(INBOX, "copia-gmail.pdf"), boletoCemig.bytes());
    await scanDirectory(db, INBOX);
    await drain();

    // Nenhuma obrigação nova: a deduplicação por content_hash e a convergência
    // por identity key impedem que o mesmo fato vire duas despesas.
    expect(await contar("financial_obligations")).toBe(antes);
    expect(await contar("transactions")).toBe(1);
  });
});
