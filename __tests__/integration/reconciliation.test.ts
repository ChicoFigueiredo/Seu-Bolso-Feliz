/**
 * Testes de integração: findReconciliationCandidates (workers/ingestion/src/reconciliation).
 *
 * Motor usado pelo pipeline de documento inteiro (não só Pluggy) — nunca
 * tinha sido exercitado contra o schema real. As Regras 2 e 3 referenciavam
 * colunas que não existem em `transactions`/`recurring_instances`
 * (transaction_date/supplier_name/category, due_date/template_id), sob casts
 * `as never`/`as unknown as typeof supabase` que escondiam o erro de tipo —
 * a query falhava silenciosamente (erro do Supabase nunca checado) e a Regra
 * 2 (match_exact/match_fuzzy, a regra principal) sempre voltava zero
 * candidatos em produção.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";

const SUPABASE_URL = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY = exigirSupabaseSecretKey();

/**
 * A chave secreta local é fixa (mesmo valor em qualquer máquina, sai de
 * `supabase status`), mas não pode ficar hardcoded aqui: o padrão
 * `sb_secret_...` aciona o secret scanning do GitHub mesmo sendo local.
 * O CI exporta SUPABASE_SECRET_KEY logo após subir o Supabase local.
 */
function exigirSupabaseSecretKey(): string {
  const key = process.env.SUPABASE_SECRET_KEY;
  if (!key) {
    throw new Error(
      "SUPABASE_SECRET_KEY não definida. Rode `supabase status` e copie SECRET_KEY para .env.",
    );
  }
  return key;
}

let supabase: SupabaseClient;
let userId: string;
let institutionId: string;
let financialProductId: string;
let supplierId: string;

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const email = "test-reconciliation@sbf.local";
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
    const signedIn = await probe.auth.signInWithPassword({ email, password: "TestPass123!" });
    userId = signedIn.data!.user!.id;
  }

  const { data: institution, error: institutionError } = await supabase
    .from("institutions")
    .insert({ user_id: userId, name: "Banco de Teste" })
    .select("id")
    .single();
  if (institutionError) throw new Error(`institution: ${institutionError.message}`);
  institutionId = institution!.id as string;

  const { data: product, error: productError } = await supabase
    .from("financial_products")
    .insert({
      user_id: userId,
      institution_id: institutionId,
      name: "Conta de Teste",
      type: "checking_account",
    })
    .select("id")
    .single();
  if (productError) throw new Error(`financial_product: ${productError.message}`);
  financialProductId = product!.id as string;

  const { data: supplier, error: supplierError } = await supabase
    .from("suppliers")
    .insert({ user_id: userId, name: "Fornecedor de Teste" })
    .select("id")
    .single();
  if (supplierError) throw new Error(`supplier: ${supplierError.message}`);
  supplierId = supplier!.id as string;
});

beforeEach(async () => {
  await supabase.from("recurring_instances").delete().eq("user_id", userId);
  await supabase.from("recurring_templates").delete().eq("user_id", userId);
  await supabase.from("transactions").delete().eq("user_id", userId);
});

afterAll(async () => {
  await supabase.from("recurring_instances").delete().eq("user_id", userId);
  await supabase.from("recurring_templates").delete().eq("user_id", userId);
  await supabase.from("transactions").delete().eq("user_id", userId);
  await supabase.from("suppliers").delete().eq("user_id", userId);
  await supabase.from("financial_products").delete().eq("user_id", userId);
  await supabase.from("institutions").delete().eq("user_id", userId);
});

describe("Regra 2 — match por supplier_id + amount + due_date", () => {
  it("acha match_exact quando existe transação com mesmo fornecedor, valor e data (±7 dias)", async () => {
    await supabase.from("transactions").insert({
      user_id: userId,
      financial_product_id: financialProductId,
      supplier_id: supplierId,
      type: "expense",
      amount: 150.0,
      event_date: "2026-08-10",
    });

    const result = await findReconciliationCandidates(
      supabase,
      userId,
      { supplier_id: supplierId, amount: 150.0, due_date: "2026-08-12" },
      null,
    );

    expect(result.status).toBe("match_exact");
    expect(result.candidates[0]?.score).toBe(0.95);
  });

  it("acha match_fuzzy quando o valor bate mas a data diverge mais de 7 dias", async () => {
    await supabase.from("transactions").insert({
      user_id: userId,
      financial_product_id: financialProductId,
      supplier_id: supplierId,
      type: "expense",
      amount: 200.0,
      event_date: "2026-08-01",
    });

    const result = await findReconciliationCandidates(
      supabase,
      userId,
      { supplier_id: supplierId, amount: 200.0, due_date: "2026-08-20" },
      null,
    );

    expect(result.status).toBe("match_fuzzy");
  });
});

describe("Regra 3 — match por recorrência", () => {
  it("reconhece instância existente no mês de competência (score 0.85)", async () => {
    const { data: template } = await supabase
      .from("recurring_templates")
      .insert({
        user_id: userId,
        supplier_id: supplierId,
        name: "Conta recorrente de teste",
        type: "expense",
        amount: 100.0,
        frequency: "monthly",
      })
      .select("id")
      .single();

    await supabase.from("recurring_instances").insert({
      user_id: userId,
      recurring_template_id: template!.id as string,
      expected_date: "2026-08-15",
      expected_amount: 100.0,
      status: "pending",
    });

    const result = await findReconciliationCandidates(
      supabase,
      userId,
      { supplier_id: supplierId, amount: 100.0, competence_date: "2026-08-15" },
      null,
    );

    const recurrenceCandidate = result.candidates.find((c) => c.matchType === "match_recurrence");
    expect(recurrenceCandidate?.score).toBe(0.85);
    expect(recurrenceCandidate?.description).toContain("instância prevista");
  });
});
