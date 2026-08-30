import "../../../actions/test-db-setup";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import type { NextRequest } from "next/server";
import {
  cleanupInstitution,
  seedFinancialProduct,
  seedInstitution,
} from "../../../actions/__tests__/fixtures";
import { GET, PATCH } from "./route";

// Sessão corrente. `id` = ator, `ownerId` = dono dos dados.
let sessao: { id: string; ownerId: string } | null = null;

vi.mock("@/lib/auth", () => ({
  getUserOrNull: async () =>
    sessao ? { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null } : null,
  requireUser: async () => {
    if (!sessao) throw new Error("NEXT_REDIRECT");
    return { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null };
  },
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";
const ADMIN = "00000000-0000-0000-0000-00000000ad11";

/** O handler só chama `.json()`; um Request comum basta. */
function patchRequest(body: unknown): NextRequest {
  return new Request("http://localhost/api/reconciliation/x", {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  }) as unknown as NextRequest;
}

function getRequest(): NextRequest {
  return new Request("http://localhost/api/reconciliation/x") as unknown as NextRequest;
}

function rota(draftId: string) {
  return { params: Promise.resolve({ draftId }) };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("/api/reconciliation/[draftId]", () => {
  const draftIds: string[] = [];
  const institutionIds: string[] = [];

  beforeEach(() => {
    sessao = { id: USER_A, ownerId: USER_A };
  });

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0)) {
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    }
    for (const id of institutionIds.splice(0)) await cleanupInstitution(id);
  });

  async function seedDraft(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.draft_records)
      .values({
        user_id: userId,
        draft_type: "transaction",
        status: "pending_review",
        draft_data: { amount: 100, due_date: "2026-03-10" },
        materialization_key: `test::${crypto.randomUUID()}`,
      })
      .returning();
    if (!row) throw new Error("Falha ao semear draft_record de teste");
    draftIds.push(row.id);
    return row;
  }

  async function seedTransaction(userId: string) {
    const institution = await seedInstitution(userId);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(userId, institution.id, "checking_account");

    const db = getDb();
    const [row] = await db
      .insert(schema.transactions)
      .values({
        user_id: userId,
        financial_product_id: product.id,
        type: "expense",
        amount: 100,
        event_date: "2026-03-10",
      })
      .returning();
    if (!row) throw new Error("Falha ao semear transaction de teste");
    return row;
  }

  async function lerDraft(id: string) {
    const db = getDb();
    const [row] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, id));
    return row;
  }

  // ── GET ────────────────────────────────────────────────────────────────

  it("GET responde 401 sem sessão", async () => {
    sessao = null;
    const res = await GET(getRequest(), rota(crypto.randomUUID()));
    expect(res.status).toBe(401);
  });

  it("GET não enxerga draft de outro usuário", async () => {
    const draftB = await seedDraft(USER_B);

    const res = await GET(getRequest(), rota(draftB.id));
    expect(res.status).toBe(404);

    // E não deixou rastro de reconciliação no draft alheio.
    expect((await lerDraft(draftB.id))?.reconciliation_status).toBe("not_checked");
  });

  it("GET roda a reconciliação do próprio draft e persiste o resultado", async () => {
    const draft = await seedDraft(USER_A);

    const res = await GET(getRequest(), rota(draft.id));
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toMatchObject({ draftId: draft.id, cached: false, status: "no_match" });

    const persistido = await lerDraft(draft.id);
    expect(persistido?.reconciliation_status).toBe("no_match");
    expect(persistido?.reconciled_at).not.toBeNull();
  });

  it("GET segue o ownerId da sessão, não o id do ator", async () => {
    const draftDoDono = await seedDraft(USER_A);
    const draftDoAtor = await seedDraft(ADMIN);

    sessao = { id: ADMIN, ownerId: USER_A };

    expect((await GET(getRequest(), rota(draftDoDono.id))).status).toBe(200);
    expect((await GET(getRequest(), rota(draftDoAtor.id))).status).toBe(404);
  });

  // ── PATCH ──────────────────────────────────────────────────────────────

  it("PATCH recusa ação desconhecida", async () => {
    const draft = await seedDraft(USER_A);
    const res = await PATCH(patchRequest({ action: "confirmed_whatever" }), rota(draft.id));
    expect(res.status).toBe(400);
  });

  it("PATCH confirma duplicata apontando para transação própria", async () => {
    const draft = await seedDraft(USER_A);
    const txn = await seedTransaction(USER_A);

    const res = await PATCH(
      patchRequest({ action: "confirmed_duplicate", transactionId: txn.id }),
      rota(draft.id),
    );
    expect(res.status).toBe(200);

    const atualizado = await lerDraft(draft.id);
    expect(atualizado?.reconciliation_status).toBe("confirmed_duplicate");
    expect(atualizado?.reconciled_transaction_id).toBe(txn.id);
  });

  // IDOR fechado nesta migração: `transactionId` vem do cliente e virava a FK
  // `reconciled_transaction_id` sem checagem de posse.
  it("PATCH recusa transactionId de outro usuário e não grava a FK", async () => {
    const draft = await seedDraft(USER_A);
    const txnDeB = await seedTransaction(USER_B);

    const res = await PATCH(
      patchRequest({ action: "confirmed_duplicate", transactionId: txnDeB.id }),
      rota(draft.id),
    );
    expect(res.status).toBe(404);

    const intocado = await lerDraft(draft.id);
    expect(intocado?.reconciled_transaction_id).toBeNull();
    // E o status também não mudou: a recusa acontece ANTES do update.
    expect(intocado?.reconciliation_status).toBe("not_checked");
  });

  it("PATCH em draft de outro usuário devolve 404 e não altera nada", async () => {
    const draftB = await seedDraft(USER_B);

    const res = await PATCH(patchRequest({ action: "confirmed_new" }), rota(draftB.id));
    expect(res.status).toBe(404);
    expect((await lerDraft(draftB.id))?.reconciliation_status).toBe("not_checked");
  });
});
