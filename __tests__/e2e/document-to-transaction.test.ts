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
 * Por isso o E2E mira a RPC diretamente, que é onde a garantia mora.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { scanDirectory } from "@sbf/worker-local-scanner";
import { processJob } from "@sbf/worker-ingestion";
import { POSTABLE_SCHEMAS, TO_INSERT, TARGET_TABLES, parseDraftPayload } from "@sbf/contracts";
import { boletoCemig } from "../fixtures/documents/boleto-cemig";

const SUPABASE_URL = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY =
  process.env.SUPABASE_SECRET_KEY ??
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImV4cCI6MTk4MzgxMjk5Nn0.EGIM96RAZx35lJzdJsyH-qQwv8Hdp7fsn3W0YpN81IU";

const INBOX = join(process.cwd(), "__tests__/e2e/.e2e-inbox");

let supabase: SupabaseClient;
let userId: string;
let productId: string;

async function drain(): Promise<void> {
  for (let round = 0; round < 20; round++) {
    const { data: jobs } = await supabase
      .from("ingestion_jobs")
      .select("id, run_id, user_id, source_document_id, status, retry_count, max_retries, metadata")
      .eq("user_id", userId)
      .in("status", ["discovered", "downloaded", "hashed", "queued", "parsed"])
      .order("created_at", { ascending: true });

    if (!jobs || jobs.length === 0) return;
    for (const job of jobs) await processJob(supabase, job);
  }
}

async function materialize(draftId: string, payload: unknown, table: string) {
  return supabase.rpc("fn_materialize_draft_record", {
    p_user_id: userId,
    p_draft_id: draftId,
    p_target_table: table,
    p_insert_payload: payload as never,
    p_actor: "e2e",
  });
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const email = "test-e2e-cycle@sbf.local";
  const created = await supabase.auth.admin.createUser({
    email,
    password: "TestPass123!",
    email_confirm: true,
  });
  if (created.data?.user?.id) {
    userId = created.data.user.id;
  } else {
    const probe = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
      auth: { persistSession: false, autoRefreshToken: false },
    });
    const s = await probe.auth.signInWithPassword({ email, password: "TestPass123!" });
    userId = s.data!.user!.id;
  }

  process.env.LOCAL_USER_ID = userId;
  process.env.SUPABASE_URL = SUPABASE_URL;
  process.env.SUPABASE_SECRET_KEY = SUPABASE_SERVICE_KEY;

  await cleanup();

  // A conta é obrigatória: transactions.financial_product_id é NOT NULL e
  // nada no pipeline consegue inferi-la a partir do documento.
  const { data: inst } = await supabase
    .from("institutions")
    .insert({ user_id: userId, name: "Banco E2E" })
    .select("id")
    .single();

  const { data: prod, error } = await supabase
    .from("financial_products")
    .insert({
      user_id: userId,
      institution_id: inst!.id,
      name: "Conta Corrente E2E",
      type: "checking_account",
    })
    .select("id")
    .single();
  if (error) throw new Error(`financial_products: ${error.message}`);
  productId = prod!.id as string;

  // Recriar do zero: arquivos remanescentes de uma execução anterior seriam
  // redescobertos e quebrariam as contagens.
  await rm(INBOX, { recursive: true, force: true });
  await mkdir(INBOX, { recursive: true });
});

/**
 * Ordem importa: as tabelas filhas saem antes das pais.
 *
 * `parsed_document_versions` estava faltando numa primeira versão desta função
 * e, por ser referenciada em cascata a partir de `source_documents`, fazia o
 * delete dos documentos violar FK — silenciosamente, porque `.delete()` sem
 * checagem de erro não reclama. O resultado eram documentos acumulando entre
 * execuções e contagens erradas. Por isso agora todo delete é verificado.
 */
async function cleanup() {
  const tables = [
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
    // audit_logs fica de fora de propósito: a tabela é imutável por política
    // do banco (DELETE é recusado). É a propriedade certa para uma trilha de
    // auditoria, então o teste se adapta a ela em vez de contorná-la.
  ];

  for (const table of tables) {
    const { error } = await supabase.from(table).delete().eq("user_id", userId);
    if (error) throw new Error(`cleanup ${table}: ${error.message}`);
  }
}

afterAll(async () => {
  await cleanup();
  await supabase.from("financial_products").delete().eq("user_id", userId);
  await supabase.from("institutions").delete().eq("user_id", userId);
  await rm(INBOX, { recursive: true, force: true });
});

describe("Marco 1 — um documento fecha o ciclo", () => {
  it("PDF real percorre ingestão → obrigação → draft → aprovação → transação", async () => {
    // ── 1. Ingestão de um PDF de verdade ─────────────────────────────────
    // Nenhum teste do repositório exercitava o caminho do pdf-parse: toda
    // entrada era uma string sintética escrita num .csv.
    await writeFile(join(INBOX, boletoCemig.filename), boletoCemig.bytes());

    const discovered = await scanDirectory(supabase, INBOX);
    expect(discovered).toBe(1);

    await drain();

    const { data: docs } = await supabase
      .from("source_documents")
      .select("id, filename, mime_type")
      .eq("user_id", userId);
    expect(docs).toHaveLength(1);
    expect(docs![0]!.mime_type).toBe("application/pdf");

    // ── 2. Extração leu o PDF ────────────────────────────────────────────
    const { data: extractions } = await supabase
      .from("extraction_results")
      .select("total_amount, due_date, supplier_name_raw")
      .eq("user_id", userId);
    expect(extractions!.length).toBeGreaterThanOrEqual(1);

    // ── 3. Draft gerado no contrato v1 ───────────────────────────────────
    const { data: drafts } = await supabase
      .from("draft_records")
      .select("id, draft_type, status, draft_data, draft_schema_version, obligation_id")
      .eq("user_id", userId);

    expect(drafts!.length).toBeGreaterThanOrEqual(1);
    const draft = drafts!.find((d) => d.draft_type === "transaction")!;
    expect(draft).toBeDefined();
    expect(draft.status).toBe("pending_review");
    expect(draft.draft_schema_version).toBe(1);

    // O payload não pode mais conter o contrato antigo.
    const raw = JSON.stringify(draft.draft_data);
    expect(raw).not.toContain("despesa");
    expect((draft.draft_data as Record<string, unknown>).direction).toBe("outflow");

    // ── 4. GUARDA: pending_review não vira dinheiro ──────────────────────
    const guard = await materialize(draft.id, { financial_product_id: productId }, "transactions");
    expect(guard.error).not.toBeNull();
    expect(guard.error!.message).toMatch(/pending_review/);

    const { count: zero } = await supabase
      .from("transactions")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);
    expect(zero).toBe(0);

    // ── 5. Humano completa o que a extração não podia saber ──────────────
    const completed = {
      ...(draft.draft_data as Record<string, unknown>),
      financial_product_id: productId,
      transaction_type: "expense",
      event_date: "2026-04-15",
    };

    await supabase
      .from("draft_records")
      .update({ draft_data: completed as never, status: "corrected" })
      .eq("id", draft.id);

    // ── 6. Aprovação NÃO lança nada ──────────────────────────────────────
    await supabase
      .from("draft_records")
      .update({ status: "approved", approved_at: new Date().toISOString() })
      .eq("id", draft.id);

    const { data: afterApproval } = await supabase
      .from("draft_records")
      .select("status, posted_record_id")
      .eq("id", draft.id)
      .single();

    expect(afterApproval!.status).toBe("approved");
    expect(afterApproval!.posted_record_id).toBeNull();

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
    expect((posted.data as Record<string, unknown>).status).toBe("posted");

    // ── 8. A transação existe, com os valores certos ─────────────────────
    const postedId = (posted.data as Record<string, string>).posted_record_id;
    const { data: tx } = await supabase
      .from("transactions")
      .select("amount, type, event_date, origin_type, is_confirmed")
      .eq("id", postedId)
      .single();

    expect(Number(tx!.amount)).toBe(boletoCemig.expected.amountCents / 100);
    expect(tx!.type).toBe("expense");
    expect(tx!.event_date).toBe("2026-04-15");
    expect(tx!.origin_type).toBe("import");

    const { data: finalDraft } = await supabase
      .from("draft_records")
      .select("status, posted_record_id, posted_at")
      .eq("id", draft.id)
      .single();
    expect(finalDraft!.status).toBe("posted");
    expect(finalDraft!.posted_record_id).toBe(postedId);
    expect(finalDraft!.posted_at).not.toBeNull();

    // ── 9. Auditoria gravou (antes escrevia em colunas inexistentes) ─────
    const { data: audit } = await supabase
      .from("audit_logs")
      .select("action, entity_id")
      .eq("user_id", userId)
      .eq("action", "draft_materialized");
    expect(audit!.length).toBeGreaterThanOrEqual(1);

    // ── 10. Replay: lançar de novo não duplica ───────────────────────────
    const replay = await materialize(draft.id, insert, TARGET_TABLES.transaction);
    expect((replay.data as Record<string, unknown>).status).toBe("already_posted");

    const { count: stillOne } = await supabase
      .from("transactions")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);
    expect(stillOne).toBe(1);
  });

  it("o mesmo documento por outro canal não vira segunda despesa", async () => {
    // Simula o boleto chegando também pelo Gmail: outro origin_key, mesmo
    // conteúdo. Deve anexar evidência à obrigação existente, sem novo lote.
    const before = await supabase
      .from("financial_obligations")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);

    const otherChannel = join(INBOX, "copia-gmail.pdf");
    await writeFile(otherChannel, boletoCemig.bytes());
    await scanDirectory(supabase, INBOX);
    await drain();

    const after = await supabase
      .from("financial_obligations")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);

    // Nenhuma obrigação nova: a deduplicação por content_hash e a convergência
    // por identity key impedem que o mesmo fato vire duas despesas.
    expect(after.count).toBe(before.count);

    const { count: txCount } = await supabase
      .from("transactions")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);
    expect(txCount).toBe(1);
  });
});
