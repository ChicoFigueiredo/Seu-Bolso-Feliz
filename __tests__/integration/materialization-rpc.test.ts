/* eslint-disable @typescript-eslint/no-explicit-any */
/**
 * Testes de integração: fn_materialize_draft_record.
 *
 * Esta RPC é a única porta pela qual um draft vira dinheiro no ledger. Os
 * testes abaixo cobrem exatamente os modos de falha que o desenho anterior
 * tinha e não conseguia evitar do lado do cliente:
 *
 *   - lançamento duplo sob concorrência (TOCTOU entre SELECT e UPDATE);
 *   - transação órfã quando a marcação do draft falha depois do INSERT;
 *   - materialização de draft ainda em pending_review;
 *   - travessia de usuário.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";

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
let otherUserId: string;
let productId: string;

/** Cria um draft aprovado pronto para lançar. */
async function createDraft(
  opts: {
    status?: string;
    draftType?: string;
    owner?: string;
  } = {},
): Promise<string> {
  const { data, error } = await supabase
    .from("draft_records")
    .insert({
      user_id: opts.owner ?? userId,
      draft_type: opts.draftType ?? "transaction",
      status: opts.status ?? "approved",
      draft_data: {},
      draft_schema_version: 1,
      materialization_key: `test:${crypto.randomUUID()}`,
    })
    .select("id")
    .single();
  if (error) throw new Error(`createDraft: ${error.message}`);
  return data!.id as string;
}

function transactionPayload(overrides: Record<string, unknown> = {}) {
  return {
    financial_product_id: productId,
    type: "expense",
    amount: 245.5,
    description: "CEMIG março",
    event_date: "2026-04-15",
    competence_date: "2026-03-01",
    category_id: null,
    priority: null,
    notes: null,
    metadata: {},
    ...overrides,
  };
}

async function materialize(draftId: string, payload: unknown, asUser = userId) {
  return supabase.rpc("fn_materialize_draft_record", {
    p_user_id: asUser,
    p_draft_id: draftId,
    p_target_table: "transactions",
    p_insert_payload: payload,
    p_actor: "test",
  });
}

async function countTransactions(): Promise<number> {
  const { count } = await supabase
    .from("transactions")
    .select("id", { count: "exact", head: true })
    .eq("user_id", userId);
  return count ?? 0;
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  // Deliberadamente sem auth.admin.listUsers(): contra o GoTrue local ele
  // devolve zero usuários com um objeto de erro vazio, mesmo quando o usuário
  // existe. Login com senha não depende dessa API e devolve o id real.
  const ensureUser = async (email: string): Promise<string> => {
    const created = await supabase.auth.admin.createUser({
      email,
      password: "TestPass123!",
      email_confirm: true,
    });
    if (created.data?.user?.id) return created.data.user.id;

    // Cliente descartável: signInWithPassword grava a sessão no cliente que o
    // executa, e fazer isso no cliente compartilhado o rebaixaria de
    // service_role para usuário comum, quebrando todos os inserts seguintes.
    const probe = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
      auth: { persistSession: false, autoRefreshToken: false },
    });
    const signedIn = await probe.auth.signInWithPassword({ email, password: "TestPass123!" });
    if (signedIn.data?.user?.id) return signedIn.data.user.id;
    throw new Error(
      `não consegui resolver o usuário ${email}: ${created.error?.message ?? ""} / ${signedIn.error?.message ?? ""}`,
    );
  };

  userId = await ensureUser("test-materialization@sbf.local");
  otherUserId = await ensureUser("test-materialization-other@sbf.local");

  const { data: inst } = await supabase
    .from("institutions")
    .insert({ user_id: userId, name: "Banco Teste" })
    .select("id")
    .single();

  const { data: prod, error } = await supabase
    .from("financial_products")
    .insert({
      user_id: userId,
      institution_id: inst!.id,
      name: "Conta Corrente Teste",
      type: "checking_account",
    })
    .select("id")
    .single();
  if (error) throw new Error(`financial_products: ${error.message}`);
  productId = prod!.id as string;
});

beforeEach(async () => {
  await supabase.from("transactions").delete().eq("user_id", userId);
  await supabase.from("draft_records").delete().eq("user_id", userId);
  await supabase.from("draft_records").delete().eq("user_id", otherUserId);
});

afterAll(async () => {
  await supabase.from("transactions").delete().eq("user_id", userId);
  await supabase.from("draft_records").delete().eq("user_id", userId);
  await supabase.from("draft_records").delete().eq("user_id", otherUserId);
  await supabase.from("audit_logs").delete().eq("user_id", userId);
  await supabase.from("financial_products").delete().eq("user_id", userId);
  await supabase.from("institutions").delete().eq("user_id", userId);
});

describe("caminho feliz", () => {
  it("insere a transação e marca o draft como posted", async () => {
    const draftId = await createDraft();
    const { data, error } = await materialize(draftId, transactionPayload());

    expect(error).toBeNull();
    expect((data as any).status).toBe("posted");

    const { data: draft } = await supabase
      .from("draft_records")
      .select("status, posted_record_id, posted_record_type, posted_at")
      .eq("id", draftId)
      .single();

    expect(draft!.status).toBe("posted");
    expect(draft!.posted_record_id).toBe((data as any).posted_record_id);
    expect(draft!.posted_record_type).toBe("transactions");
    expect(draft!.posted_at).not.toBeNull();

    const { data: tx } = await supabase
      .from("transactions")
      .select("amount, type, event_date, origin_type, is_confirmed")
      .eq("id", (data as any).posted_record_id)
      .single();

    expect(Number(tx!.amount)).toBe(245.5);
    expect(tx!.type).toBe("expense");
    expect(tx!.event_date).toBe("2026-04-15");
    // origin_type tem DEFAULT 'manual'; se um jsonb_populate_record tivesse
    // sido usado, a chave ausente sobrescreveria o default com NULL.
    expect(tx!.origin_type).toBe("import");
  });

  it("grava a trilha de auditoria dentro da mesma transação", async () => {
    const draftId = await createDraft();
    const { data } = await materialize(draftId, transactionPayload());

    const { data: logs } = await supabase
      .from("audit_logs")
      .select("action, entity_type, entity_id, new_values")
      .eq("user_id", userId)
      .eq("action", "draft_materialized")
      .eq("entity_id", (data as any).posted_record_id);

    expect(logs).toHaveLength(1);
    expect(logs![0]!.entity_type).toBe("transactions");
    expect((logs![0]!.new_values as any).draft_id).toBe(draftId);
  });
});

describe("governança de status", () => {
  it("recusa materializar um draft em pending_review", async () => {
    const draftId = await createDraft({ status: "pending_review" });
    const { error } = await materialize(draftId, transactionPayload());

    expect(error).not.toBeNull();
    expect(error!.message).toMatch(/pending_review/);
    expect(await countTransactions()).toBe(0);
  });

  it("aceita corrected, além de approved", async () => {
    const draftId = await createDraft({ status: "corrected" });
    const { error } = await materialize(draftId, transactionPayload());
    expect(error).toBeNull();
  });

  it.each(["rejected", "archived", "posted"])("recusa status %s", async (status) => {
    const draftId = await createDraft({ status });
    const { error } = await materialize(draftId, transactionPayload());
    expect(error).not.toBeNull();
    expect(await countTransactions()).toBe(0);
  });
});

describe("idempotência", () => {
  it("segunda chamada devolve already_posted sem inserir de novo", async () => {
    const draftId = await createDraft();
    const first = await materialize(draftId, transactionPayload());
    const second = await materialize(draftId, transactionPayload());

    expect((first.data as any).status).toBe("posted");
    expect((second.data as any).status).toBe("already_posted");
    expect((second.data as any).posted_record_id).toBe((first.data as any).posted_record_id);
    expect(await countTransactions()).toBe(1);
  });

  it("dupla aprovação concorrente lança exatamente uma vez", async () => {
    // Este é o caso que o SELECT-depois-UPDATE do cliente não conseguia
    // evitar: ambas as chamadas liam posted_record_id NULL e ambas inseriam.
    const draftId = await createDraft();

    const results = await Promise.all([
      materialize(draftId, transactionPayload()),
      materialize(draftId, transactionPayload()),
      materialize(draftId, transactionPayload()),
    ]);

    const statuses = results.map((r) => (r.data as any)?.status).filter(Boolean);
    expect(statuses.filter((s) => s === "posted")).toHaveLength(1);
    expect(statuses.filter((s) => s === "already_posted")).toHaveLength(2);
    expect(await countTransactions()).toBe(1);
  });
});

describe("isolamento e validação de entrada", () => {
  it("não materializa draft de outro usuário", async () => {
    const draftId = await createDraft({ owner: otherUserId });
    const { error } = await materialize(draftId, transactionPayload(), userId);
    expect(error).not.toBeNull();
    expect(error!.message).toMatch(/not found|not owned/i);
  });

  it("recusa tabela alvo fora da allowlist", async () => {
    const draftId = await createDraft();
    const { error } = await supabase.rpc("fn_materialize_draft_record", {
      p_user_id: userId,
      p_draft_id: draftId,
      p_target_table: "auth.users",
      p_insert_payload: transactionPayload(),
      p_actor: "test",
    });
    expect(error).not.toBeNull();
    expect(error!.message).toMatch(/not allowed/i);
  });
});

describe("atomicidade", () => {
  it("insert inválido não deixa órfão: draft continua approved", async () => {
    const draftId = await createDraft();
    const { error } = await materialize(
      draftId,
      transactionPayload({ financial_product_id: "99999999-9999-4999-8999-999999999999" }),
    );

    expect(error).not.toBeNull();
    expect(await countTransactions()).toBe(0);

    const { data: draft } = await supabase
      .from("draft_records")
      .select("status, posted_record_id")
      .eq("id", draftId)
      .single();

    // Toda a transação reverteu: nada foi marcado, e o draft segue retentável.
    expect(draft!.status).toBe("approved");
    expect(draft!.posted_record_id).toBeNull();
  });

  it("o índice único impede que dois drafts reivindiquem o mesmo registro", async () => {
    const draftA = await createDraft();
    const { data } = await materialize(draftA, transactionPayload());
    const postedId = (data as any).posted_record_id;

    const draftB = await createDraft();
    const { error } = await supabase
      .from("draft_records")
      .update({ posted_record_id: postedId, posted_record_type: "transactions" })
      .eq("id", draftB);

    // Backstop de banco: vale mesmo que o código da aplicação erre.
    expect(error).not.toBeNull();
    expect(error!.message).toMatch(/duplicate key|unique/i);
  });
});
