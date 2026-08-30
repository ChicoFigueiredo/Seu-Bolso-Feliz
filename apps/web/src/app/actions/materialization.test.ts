import "./test-db-setup";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { postApprovedDraftRecord } from "./materialization";

// Sessão corrente do teste. `id` é o ATOR e `ownerId` é o DONO dos dados —
// são campos diferentes de propósito (ver apps/web/src/lib/auth/session.ts), e
// os testes abaixo os mantêm DIFERENTES justamente para que trocar `ownerId`
// por `id` no código quebre a suíte em vez de passar despercebido.
let sessao: { id: string; ownerId: string } | null = null;

vi.mock("@/lib/auth", () => ({
  requireUser: async () => {
    // `requireUser()` não devolve null: sem sessão ele redireciona pro
    // /login, o que em runtime é um throw (NEXT_REDIRECT). O mock reproduz
    // esse contrato para o teste de "sem sessão" continuar valendo.
    if (!sessao) throw new Error("NEXT_REDIRECT");
    return { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null };
  },
  getUserOrNull: async () =>
    sessao ? { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null } : null,
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";
// Administrador de emergência: age em nome de USER_A, mas tem id próprio.
const ADMIN = "00000000-0000-0000-0000-00000000ad11";

describe.skipIf(!process.env.TEST_DATABASE_URL)("materialization actions", () => {
  const draftIds: string[] = [];

  beforeEach(() => {
    sessao = { id: USER_A, ownerId: USER_A };
  });

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0)) {
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    }
  });

  async function seedDraft(userId: string, status: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.draft_records)
      .values({
        user_id: userId,
        draft_type: "transaction",
        status,
        draft_data: { amount: 100 },
        materialization_key: `test::${crypto.randomUUID()}`,
      })
      .returning();
    if (!row) throw new Error("Falha ao criar draft_record de teste");
    draftIds.push(row.id);
    return row;
  }

  it("recusa lançar draft de outro usuário (draft não encontrado)", async () => {
    const draftB = await seedDraft(USER_B, "approved");

    sessao = { id: USER_A, ownerId: USER_A };
    const result = await postApprovedDraftRecord(draftB.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("não encontrado");
  });

  it("recusa lançar draft ainda pending_review", async () => {
    const draft = await seedDraft(USER_A, "pending_review");

    const result = await postApprovedDraftRecord(draft.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("Aprove primeiro");
  });

  // Os dois testes abaixo são o par que separa `ownerId` de `id`. Com um dos
  // dois sozinho, trocar um pelo outro no código ainda passaria (o dono
  // comum tem os dois iguais); juntos, qualquer troca quebra.
  it("admin operando por USER_A ENXERGA o draft do dono (filtro é ownerId)", async () => {
    const draftDoDono = await seedDraft(USER_A, "pending_review");

    sessao = { id: ADMIN, ownerId: USER_A };
    const result = await postApprovedDraftRecord(draftDoDono.id);

    // Achou o draft (passou pelo filtro de posse) e parou só na regra de
    // status. Se o código filtrasse por `id`, a mensagem seria "não
    // encontrado".
    expect(result.message).toContain("Aprove primeiro");
  });

  it("admin operando por USER_A NÃO enxerga draft do próprio id de ator", async () => {
    const draftDoAtor = await seedDraft(ADMIN, "pending_review");

    sessao = { id: ADMIN, ownerId: USER_A };
    const result = await postApprovedDraftRecord(draftDoAtor.id);

    expect(result.success).toBe(false);
    expect(result.message).toContain("não encontrado");
  });

  it("sem sessão, propaga o redirect do requireUser e não toca no draft", async () => {
    const draft = await seedDraft(USER_A, "approved");

    sessao = null;
    // Mudança de contrato desta migração: falta de sessão deixou de ser
    // `{ success: false, message: "Não autenticado" }` e passou a ser o
    // mesmo redirect do resto do app.
    await expect(postApprovedDraftRecord(draft.id)).rejects.toThrow("NEXT_REDIRECT");

    const db = getDb();
    const [intocado] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, draft.id));
    expect(intocado?.posted_record_id).toBeNull();
    expect(intocado?.status).toBe("approved");
  });
});
