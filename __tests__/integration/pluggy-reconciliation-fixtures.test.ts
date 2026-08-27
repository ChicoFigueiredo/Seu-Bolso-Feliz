/**
 * Testes de integração: critérios de aceite de reconciliação (§37,
 * docs/prompts/2026-08-24-prompt-integracao-pluggy.md).
 *
 * Semeia Supabase local com o dataset sintético de
 * __tests__/fixtures/pluggy-reconciliation/ (100 transações + 40 emails +
 * 20 PDFs — 15 recorrência/10 parcelamento/10 conflito/10 ambíguo cruzando
 * o pool) e roda findReconciliationCandidates de verdade contra cada
 * critério: correspondência exata, aproximada, não-correspondência,
 * ambiguidade, prevenção de falso positivo e preservação de evidência.
 *
 * "Ambiguidade" não é um status nativo do motor (decisão registrada:
 * estender reconciliation.ts pra isso está fora do raio de ação da Fase 4
 * Pluggy) — aqui ela é detectada no nível do teste via
 * `candidates.length > 1` com scores próximos.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { computeContentHash } from "@sbf/operations";
import {
  SYNTHETIC_TRANSACTIONS,
  RECURRENCE_TRANSACTIONS,
  INSTALLMENT_TRANSACTIONS,
  CONFLICT_TRANSACTIONS,
  AMBIGUOUS_TRANSACTIONS,
  type SyntheticTransaction,
} from "../fixtures/pluggy-reconciliation/transactions";
import {
  PRECISE_EMAILS,
  OFFSET_DATE_EMAILS,
  NO_DUE_DATE_EMAILS,
  UNKNOWN_EMAILS,
} from "../fixtures/pluggy-reconciliation/emails";
import {
  UNIQUE_DOCUMENTS,
  DUPLICATE_DOCUMENTS,
  documentBytes,
} from "../fixtures/pluggy-reconciliation/documents";

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
const supplierIdByName = new Map<string, string>();

function amountReaisFromCents(cents: number): number {
  return cents / 100;
}

/** Todos os nomes de fornecedor citados nas três origens do dataset. */
function collectSupplierNames(): string[] {
  const names = new Set<string>();
  for (const t of SYNTHETIC_TRANSACTIONS) names.add(t.supplierName);
  for (const e of [...PRECISE_EMAILS, ...OFFSET_DATE_EMAILS, ...NO_DUE_DATE_EMAILS]) {
    names.add(e.supplierNameRaw);
  }
  for (const d of UNIQUE_DOCUMENTS) names.add(d.supplierName);
  for (const d of DUPLICATE_DOCUMENTS) names.add(d.supplierName);
  // UNKNOWN_EMAILS é deliberadamente excluído: o objetivo é não existir.
  return [...names];
}

async function seedSuppliers() {
  const names = collectSupplierNames();
  const rows = names.map((name) => ({ user_id: userId, name }));
  const { data, error } = await supabase.from("suppliers").insert(rows).select("id, name");
  if (error) throw new Error(`seedSuppliers: ${error.message}`);
  for (const row of data ?? []) {
    supplierIdByName.set(row.name as string, row.id as string);
  }
}

async function seedTransactionsLedger() {
  const rows = SYNTHETIC_TRANSACTIONS.map((t) => ({
    user_id: userId,
    financial_product_id: financialProductId,
    supplier_id: supplierIdByName.get(t.supplierName) ?? null,
    type: "expense",
    amount: amountReaisFromCents(t.amountCents),
    event_date: t.eventDate,
  }));
  const { error } = await supabase.from("transactions").insert(rows);
  if (error) throw new Error(`seedTransactionsLedger: ${error.message}`);
}

/** Um recurring_template por grupo de recorrência + uma instance por mês. */
async function seedRecurringTemplatesAndInstances() {
  const groups = new Map<string, SyntheticTransaction[]>();
  for (const t of RECURRENCE_TRANSACTIONS) {
    const list = groups.get(t.scenarioGroup!) ?? [];
    list.push(t);
    groups.set(t.scenarioGroup!, list);
  }

  for (const [supplierName, txns] of groups) {
    const { data: template, error: templateError } = await supabase
      .from("recurring_templates")
      .insert({
        user_id: userId,
        supplier_id: supplierIdByName.get(supplierName),
        name: `Recorrência sintética — ${supplierName}`,
        type: "expense",
        amount: amountReaisFromCents(txns[0]!.amountCents),
        frequency: "monthly",
      })
      .select("id")
      .single();
    if (templateError)
      throw new Error(`seedRecurringTemplatesAndInstances: ${templateError.message}`);

    const instanceRows = txns.map((t) => ({
      user_id: userId,
      recurring_template_id: template!.id as string,
      expected_date: t.eventDate,
      expected_amount: amountReaisFromCents(t.amountCents),
      status: "pending",
    }));
    const { error: instancesError } = await supabase
      .from("recurring_instances")
      .insert(instanceRows);
    if (instancesError)
      throw new Error(`seedRecurringTemplatesAndInstances: ${instancesError.message}`);
  }
}

/** source_documents pros 20 PDFs — os pares duplicados ganham um draft aprovado no primeiro. */
async function seedDocuments() {
  for (const doc of UNIQUE_DOCUMENTS) {
    await supabase.from("source_documents").insert({
      user_id: userId,
      origin_type: "local_file",
      origin_key: doc.filename,
      filename: doc.filename,
      mime_type: "application/pdf",
      status: "active",
      supplier_id: supplierIdByName.get(doc.supplierName) ?? null,
      supplier_name_raw: doc.supplierName,
      content_hash: await computeContentHash(documentBytes(doc)),
    });
  }

  for (const doc of DUPLICATE_DOCUMENTS) {
    const { data: inserted, error } = await supabase
      .from("source_documents")
      .insert({
        user_id: userId,
        origin_type: "local_file",
        origin_key: doc.filename,
        filename: doc.filename,
        mime_type: "application/pdf",
        status: "active",
        supplier_id: supplierIdByName.get(doc.supplierName) ?? null,
        supplier_name_raw: doc.supplierName,
        content_hash: await computeContentHash(documentBytes(doc)),
      })
      .select("id")
      .single();
    if (error) throw new Error(`seedDocuments (duplicado): ${error.message}`);

    // Só a primeira cópia de cada par ganha draft aprovado — é o documento
    // "processado antes" que a segunda cópia deveria reconhecer como duplicata.
    if (doc.label.endsWith("copia-0")) {
      await supabase.from("draft_records").insert({
        user_id: userId,
        source_document_id: inserted!.id as string,
        draft_type: "transaction",
        status: "approved",
        draft_data: {
          amount: amountReaisFromCents(doc.amountCents),
          due_date: doc.dueDate,
          supplier_name: doc.supplierName,
        },
        confidence_score: 1.0,
      });
    }
  }
}

async function seedAll() {
  await seedSuppliers();
  await seedTransactionsLedger();
  await seedRecurringTemplatesAndInstances();
  await seedDocuments();
}

async function cleanup() {
  await supabase.from("draft_records").delete().eq("user_id", userId);
  await supabase.from("source_documents").delete().eq("user_id", userId);
  await supabase.from("recurring_instances").delete().eq("user_id", userId);
  await supabase.from("recurring_templates").delete().eq("user_id", userId);
  await supabase.from("transactions").delete().eq("user_id", userId);
  await supabase.from("suppliers").delete().eq("user_id", userId);
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const email = "test-pluggy-reconciliation-fixtures@sbf.local";
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
    .insert({ user_id: userId, name: "Banco Fixture Pluggy" })
    .select("id")
    .single();
  if (institutionError) throw new Error(`institution: ${institutionError.message}`);
  institutionId = institution!.id as string;

  const { data: product, error: productError } = await supabase
    .from("financial_products")
    .insert({
      user_id: userId,
      institution_id: institutionId,
      name: "Conta Fixture Pluggy",
      type: "checking_account",
    })
    .select("id")
    .single();
  if (productError) throw new Error(`financial_product: ${productError.message}`);
  financialProductId = product!.id as string;

  // Semeado uma vez só — os 52 testes abaixo só LEEM esse dataset via
  // findReconciliationCandidates (nenhum escreve nele), então recriar tudo
  // antes de cada teste era puro desperdício (e, sob carga concorrente dos
  // outros arquivos de integração no mesmo Supabase local, uma fonte real
  // de flakiness: muitas idas e vindas redundantes disputando conexão).
  await seedAll();
}, 30_000);

afterAll(async () => {
  await cleanup();
  await supabase.from("financial_products").delete().eq("user_id", userId);
  await supabase.from("institutions").delete().eq("user_id", userId);
});

describe("correspondências exatas", () => {
  it.each(PRECISE_EMAILS)("$label: fornecedor+valor+data exatos → match_exact", async (email) => {
    const result = await findReconciliationCandidates(
      supabase,
      userId,
      {
        supplier_id: supplierIdByName.get(email.supplierNameRaw),
        amount: amountReaisFromCents(email.amountCents!),
        due_date: email.dueDate,
      },
      null,
    );

    expect(result.status).toBe("match_exact");
    expect(result.candidates[0]?.score).toBe(0.95);
  });
});

describe("correspondências aproximadas", () => {
  it.each(OFFSET_DATE_EMAILS)(
    "$label: valor certo, data fora da janela de 7 dias → match_fuzzy",
    async (email) => {
      const result = await findReconciliationCandidates(
        supabase,
        userId,
        {
          supplier_id: supplierIdByName.get(email.supplierNameRaw),
          amount: amountReaisFromCents(email.amountCents!),
          due_date: email.dueDate,
        },
        null,
      );

      expect(result.status).toBe("match_fuzzy");
    },
  );
});

describe("não correspondência quando o score é insuficiente", () => {
  it.each(UNKNOWN_EMAILS)(
    "$label: fornecedor/valor sem correspondência no extrato → no_match",
    async (email) => {
      const result = await findReconciliationCandidates(
        supabase,
        userId,
        {
          supplier_id: supplierIdByName.get(email.supplierNameRaw) ?? null,
          amount: amountReaisFromCents(email.amountCents!),
          due_date: email.dueDate,
        },
        null,
      );

      expect(result.status).toBe("no_match");
      expect(result.candidates).toHaveLength(0);
    },
  );

  it.each(NO_DUE_DATE_EMAILS)(
    "$label: fornecedor conhecido mas sem due_date → no_match (regra não roda sem data)",
    async (email) => {
      const result = await findReconciliationCandidates(
        supabase,
        userId,
        {
          supplier_id: supplierIdByName.get(email.supplierNameRaw),
          amount: amountReaisFromCents(email.amountCents!),
          due_date: email.dueDate,
        },
        null,
      );

      expect(result.status).toBe("no_match");
    },
  );
});

describe("detecção de ambiguidade", () => {
  it.each(
    [...new Set(AMBIGUOUS_TRANSACTIONS.map((t) => t.scenarioGroup!))].map((group) => ({
      group,
      transactions: AMBIGUOUS_TRANSACTIONS.filter((t) => t.scenarioGroup === group),
    })),
  )(
    "$group: duas transações do mesmo fornecedor, valor e data próximos → múltiplos candidatos",
    async ({ transactions }) => {
      const first = transactions[0]!;
      const result = await findReconciliationCandidates(
        supabase,
        userId,
        {
          supplier_id: supplierIdByName.get(first.supplierName),
          amount: amountReaisFromCents(first.amountCents),
          due_date: first.eventDate,
        },
        null,
      );

      // Ambíguo = mais de um candidato plausível com score próximo — o
      // motor não escolhe sozinho, decisão explícita de ficar pro humano.
      expect(result.candidates.length).toBeGreaterThan(1);
      const scores = result.candidates.map((c) => c.score);
      expect(Math.max(...scores) - Math.min(...scores)).toBeLessThanOrEqual(0.25);
    },
  );
});

describe("prevenção de falso positivo", () => {
  it.each(CONFLICT_TRANSACTIONS.filter((_, idx) => idx % 2 === 0))(
    "$label: mesmo valor/data que $scenarioGroup, fornecedor diferente → nunca vira candidato",
    async (conflictA) => {
      const conflictB = CONFLICT_TRANSACTIONS.find(
        (t) => t.scenarioGroup === conflictA.scenarioGroup && t.label !== conflictA.label,
      )!;

      const result = await findReconciliationCandidates(
        supabase,
        userId,
        {
          supplier_id: supplierIdByName.get(conflictA.supplierName),
          amount: amountReaisFromCents(conflictA.amountCents),
          due_date: conflictA.eventDate,
        },
        null,
      );

      // O candidato do fornecedor B (mesmo valor/data) não pode aparecer —
      // supplier_id é o filtro que impede o falso positivo.
      expect(
        result.candidates.some((c) => c.candidateData.supplierName === conflictB.supplierName),
      ).toBe(false);
    },
  );

  it("parcelas da mesma compra: a parcela do dia certo vem como match_exact, as outras só como fuzzy (nunca no mesmo nível de confiança)", async () => {
    const group = INSTALLMENT_TRANSACTIONS.filter(
      (t) => t.scenarioGroup === INSTALLMENT_TRANSACTIONS[0]!.scenarioGroup,
    );
    const target = group[0]!;

    const result = await findReconciliationCandidates(
      supabase,
      userId,
      {
        supplier_id: supplierIdByName.get(target.supplierName),
        amount: amountReaisFromCents(target.amountCents),
        due_date: target.eventDate,
      },
      null,
    );

    expect(result.status).toBe("match_exact");
    const exactMatches = result.candidates.filter((c) => c.matchType === "match_exact");
    const fuzzyMatches = result.candidates.filter((c) => c.matchType === "match_fuzzy");
    expect(exactMatches).toHaveLength(1);
    expect(fuzzyMatches.length).toBeGreaterThan(0);
  });
});

describe("preservação de evidências", () => {
  it("documento duplicado (mesmo content_hash) → match_duplicate, e as duas evidências continuam existindo", async () => {
    const [first, second] = DUPLICATE_DOCUMENTS;
    expect(first!.duplicateGroup).toBe(second!.duplicateGroup);

    const { data: secondDoc, error } = await supabase
      .from("source_documents")
      .select("id")
      .eq("user_id", userId)
      .eq("origin_key", second!.filename)
      .single();
    if (error) throw new Error(error.message);

    const result = await findReconciliationCandidates(
      supabase,
      userId,
      {
        amount: amountReaisFromCents(second!.amountCents),
        due_date: second!.dueDate,
      },
      secondDoc!.id as string,
    );

    expect(result.status).toBe("match_duplicate");
    expect(result.candidates[0]?.score).toBe(1.0);

    // Nenhuma das duas entradas foi descartada — a detecção de duplicata
    // não apaga evidência, só evita que a SEGUNDA vire uma obrigação nova.
    const { data: bothDocs } = await supabase
      .from("source_documents")
      .select("id")
      .eq("user_id", userId)
      .in("origin_key", [first!.filename, second!.filename]);
    expect(bothDocs).toHaveLength(2);
  });
});
