import "../../../../actions/test-db-setup";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { GET } from "./route";

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

function rota(draftId: string) {
  return { params: Promise.resolve({ draftId }) };
}

function pedido(): Request {
  return new Request("http://localhost/api/reconciliation/x/progress");
}

describe.skipIf(!process.env.TEST_DATABASE_URL)(
  "GET /api/reconciliation/[draftId]/progress",
  () => {
    const batchIds: string[] = [];
    const draftIds: string[] = [];

    beforeEach(() => {
      sessao = { id: USER_A, ownerId: USER_A };
    });

    afterEach(async () => {
      const db = getDb();
      for (const id of draftIds.splice(0))
        await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
      for (const id of batchIds.splice(0))
        await db.delete(schema.draft_batches).where(eq(schema.draft_batches.id, id));
    });

    async function seedBatch(userId: string, statuses: string[]) {
      const db = getDb();
      const [batch] = await db
        .insert(schema.draft_batches)
        .values({ user_id: userId, name: "Batch de teste", status: "open" })
        .returning();
      if (!batch) throw new Error("Falha ao semear draft_batch de teste");
      batchIds.push(batch.id);

      for (const status of statuses) {
        const [draft] = await db
          .insert(schema.draft_records)
          .values({
            user_id: userId,
            batch_id: batch.id,
            draft_type: "transaction",
            status,
            draft_data: { amount: 1 },
            materialization_key: `test::${crypto.randomUUID()}`,
          })
          .returning();
        if (!draft) throw new Error("Falha ao semear draft_record de teste");
        draftIds.push(draft.id);
      }
      return batch;
    }

    it("responde 401 sem sessão", async () => {
      sessao = null;
      const res = await GET(pedido(), rota(crypto.randomUUID()));
      expect(res.status).toBe(401);
    });

    // `fn_reconciliation_progress` é SECURITY DEFINER e só recebe o batch_id:
    // ela conta drafts de QUALQUER dono. Quem autoriza é a rota.
    it("não vaza o progresso de um batch de outro usuário", async () => {
      const batchDeB = await seedBatch(USER_B, ["approved", "pending_review"]);

      const res = await GET(pedido(), rota(batchDeB.id));
      expect(res.status).toBe(404);

      const corpo = await res.json();
      expect(corpo).not.toHaveProperty("total_count");
    });

    it("devolve a contagem do próprio batch como número", async () => {
      const batch = await seedBatch(USER_A, ["approved", "approved", "pending_review", "rejected"]);

      const res = await GET(pedido(), rota(batch.id));
      expect(res.status).toBe(200);

      const corpo = await res.json();
      expect(corpo).toEqual({ total_count: 4, reconciled_count: 2, progress_pct: 50 });
      // `bigint`/`numeric` voltam do driver como string — a UI monta uma barra
      // de progresso com esses valores e precisa de número.
      expect(typeof corpo.total_count).toBe("number");
      expect(typeof corpo.progress_pct).toBe("number");
    });

    it("segue o ownerId da sessão, não o id do ator", async () => {
      const batchDoDono = await seedBatch(USER_A, ["approved"]);
      const batchDoAtor = await seedBatch(ADMIN, ["approved"]);

      sessao = { id: ADMIN, ownerId: USER_A };

      expect((await GET(pedido(), rota(batchDoDono.id))).status).toBe(200);
      expect((await GET(pedido(), rota(batchDoAtor.id))).status).toBe(404);
    });
  },
);
