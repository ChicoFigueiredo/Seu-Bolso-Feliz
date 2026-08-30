/**
 * Testes de integração: fn_materialize_draft_record.
 *
 * Esta função do banco é a única porta pela qual um draft vira dinheiro no
 * ledger. Os testes abaixo cobrem exatamente os modos de falha que o desenho
 * anterior tinha e não conseguia evitar do lado do cliente:
 *
 *   - lançamento duplo sob concorrência (TOCTOU entre SELECT e UPDATE);
 *   - transação órfã quando a marcação do draft falha depois do INSERT;
 *   - materialização de draft ainda em pending_review;
 *   - travessia de usuário.
 *
 * A última é a que mais mudou de peso: com RLS, o banco ainda teria uma
 * segunda linha de defesa. Sem RLS, `p_user_id` é a ÚNICA coisa que separa os
 * drafts de duas pessoas — o que torna este arquivo, e não a política, o
 * lugar onde o isolamento é provado.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { sql } from "drizzle-orm";
import { draft_records, financial_products, institutions, type Database } from "@sbf/db";
import { getTestDb, limparPorUsuario, linha, linhas, semBancoDeTeste, tentar } from "../helpers/db";

const USER_ID = "40000000-0000-4000-8000-000000000001";
const OUTRO_USER_ID = "40000000-0000-4000-8000-000000000002";

let db: Database;
let productId: string;

interface MaterializeResult {
  status: string;
  posted_record_id: string;
}

/** Cria um draft aprovado pronto para lançar. */
async function createDraft(
  opts: { status?: string; draftType?: string; owner?: string } = {},
): Promise<string> {
  const [draft] = await db
    .insert(draft_records)
    .values({
      user_id: opts.owner ?? USER_ID,
      draft_type: opts.draftType ?? "transaction",
      status: opts.status ?? "approved",
      draft_data: {},
      draft_schema_version: 1,
      materialization_key: `test:${crypto.randomUUID()}`,
    })
    .returning({ id: draft_records.id });
  return draft!.id;
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

/**
 * Chama a função e devolve `{ data, error }`.
 *
 * O formato veio do supabase-js e continua útil: metade das asserções deste
 * arquivo é sobre a MENSAGEM do erro que o banco levanta, não sobre uma
 * exceção genérica.
 */
async function materialize(
  draftId: string,
  payload: unknown,
  asUser = USER_ID,
  targetTable = "transactions",
) {
  return tentar(async () => {
    const row = await linha<{ resultado: MaterializeResult }>(
      db,
      sql`select public.fn_materialize_draft_record(
        ${asUser}::uuid,
        ${draftId}::uuid,
        ${targetTable}::text,
        ${JSON.stringify(payload)}::jsonb,
        'test'::text
      ) as resultado`,
    );
    return row!.resultado;
  });
}

async function countTransactions(): Promise<number> {
  const row = await linha<{ total: string }>(
    db,
    sql`select count(*)::text as total from public.transactions where user_id = ${USER_ID}::uuid`,
  );
  return Number(row!.total);
}

describe.skipIf(semBancoDeTeste)("materialização de draft", () => {
  beforeAll(async () => {
    db = getTestDb();
    await limparPorUsuario(
      db,
      ["transactions", "draft_records", "financial_products", "institutions"],
      USER_ID,
      OUTRO_USER_ID,
    );

    const [inst] = await db
      .insert(institutions)
      .values({ user_id: USER_ID, name: "Banco Teste" })
      .returning({ id: institutions.id });

    const [prod] = await db
      .insert(financial_products)
      .values({
        user_id: USER_ID,
        institution_id: inst!.id,
        name: "Conta Corrente Teste",
        type: "checking_account",
      })
      .returning({ id: financial_products.id });
    productId = prod!.id;
  });

  beforeEach(async () => {
    await limparPorUsuario(db, ["transactions", "draft_records"], USER_ID, OUTRO_USER_ID);
  });

  // `audit_logs` não entra no cleanup: é imutável por trigger. As asserções
  // sobre auditoria filtram por `entity_id`, então acúmulo entre execuções não
  // contamina nada.
  afterAll(async () => {
    await limparPorUsuario(
      db,
      ["transactions", "draft_records", "financial_products", "institutions"],
      USER_ID,
      OUTRO_USER_ID,
    );
  });

  describe("caminho feliz", () => {
    it("insere a transação e marca o draft como posted", async () => {
      const draftId = await createDraft();
      const { data, error } = await materialize(draftId, transactionPayload());

      expect(error).toBeNull();
      expect(data!.status).toBe("posted");

      const draft = await linha<{
        status: string;
        posted_record_id: string;
        posted_record_type: string;
        posted_at: string | null;
      }>(
        db,
        sql`select status, posted_record_id, posted_record_type, posted_at
              from public.draft_records where id = ${draftId}::uuid`,
      );

      expect(draft!.status).toBe("posted");
      expect(draft!.posted_record_id).toBe(data!.posted_record_id);
      expect(draft!.posted_record_type).toBe("transactions");
      expect(draft!.posted_at).not.toBeNull();

      const tx = await linha<{
        amount: string;
        type: string;
        event_date: string;
        origin_type: string;
      }>(
        db,
        sql`select amount, type, event_date::text as event_date, origin_type
              from public.transactions where id = ${data!.posted_record_id}::uuid`,
      );

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

      const logs = await linhas<{ entity_type: string; new_values: Record<string, unknown> }>(
        db,
        sql`select entity_type, new_values from public.audit_logs
             where user_id = ${USER_ID}::uuid
               and action = 'draft_materialized'
               and entity_id = ${data!.posted_record_id}::uuid`,
      );

      expect(logs).toHaveLength(1);
      expect(logs[0]!.entity_type).toBe("transactions");
      expect(logs[0]!.new_values.draft_id).toBe(draftId);
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

      expect(first.data!.status).toBe("posted");
      expect(second.data!.status).toBe("already_posted");
      expect(second.data!.posted_record_id).toBe(first.data!.posted_record_id);
      expect(await countTransactions()).toBe(1);
    });

    it("dupla aprovação concorrente lança exatamente uma vez", async () => {
      // Este é o caso que o SELECT-depois-UPDATE do cliente não conseguia
      // evitar: ambas as chamadas liam posted_record_id NULL e ambas inseriam.
      // O pool do `pg` abre conexões distintas, então a concorrência é real,
      // não simulada.
      const draftId = await createDraft();

      const results = await Promise.all([
        materialize(draftId, transactionPayload()),
        materialize(draftId, transactionPayload()),
        materialize(draftId, transactionPayload()),
      ]);

      const statuses = results.map((r) => r.data?.status).filter(Boolean);
      expect(statuses.filter((s) => s === "posted")).toHaveLength(1);
      expect(statuses.filter((s) => s === "already_posted")).toHaveLength(2);
      expect(await countTransactions()).toBe(1);
    });
  });

  describe("isolamento e validação de entrada", () => {
    it("não materializa draft de outro usuário", async () => {
      const draftId = await createDraft({ owner: OUTRO_USER_ID });
      const { error } = await materialize(draftId, transactionPayload(), USER_ID);
      expect(error).not.toBeNull();
      expect(error!.message).toMatch(/not found|not owned/i);
    });

    it("recusa tabela alvo fora da allowlist", async () => {
      const draftId = await createDraft();
      const { error } = await materialize(
        draftId,
        transactionPayload(),
        USER_ID,
        "public.users", // a tabela de identidade que substituiu auth.users
      );
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

      const draft = await linha<{ status: string; posted_record_id: string | null }>(
        db,
        sql`select status, posted_record_id from public.draft_records
             where id = ${draftId}::uuid`,
      );

      // Toda a transação reverteu: nada foi marcado, e o draft segue retentável.
      expect(draft!.status).toBe("approved");
      expect(draft!.posted_record_id).toBeNull();
    });

    it("o índice único impede que dois drafts reivindiquem o mesmo registro", async () => {
      const draftA = await createDraft();
      const { data } = await materialize(draftA, transactionPayload());
      const postedId = data!.posted_record_id;

      const draftB = await createDraft();
      const { error } = await tentar(() =>
        db.execute(sql`
          update public.draft_records
             set posted_record_id = ${postedId}::uuid, posted_record_type = 'transactions'
           where id = ${draftB}::uuid and user_id = ${USER_ID}::uuid
        `),
      );

      // Backstop de banco: vale mesmo que o código da aplicação erre.
      expect(error).not.toBeNull();
      expect(error!.message).toMatch(/duplicate key|unique/i);
    });
  });
});
