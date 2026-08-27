import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { getOrCreateChatSession } from "./chat-sessions";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: {
      getUser: async () => ({ data: { user: currentUserId ? { id: currentUserId } : null } }),
    },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("chat-sessions actions", () => {
  const sessionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sessionIds.splice(0)) {
      await db.delete(schema.ai_chat_sessions).where(eq(schema.ai_chat_sessions.id, id));
    }
  });

  it("isola sessões de chat por usuário", async () => {
    currentUserId = USER_A;
    const sessionA = await getOrCreateChatSession();
    sessionIds.push(sessionA.id);

    currentUserId = USER_B;
    const sessionB = await getOrCreateChatSession();
    sessionIds.push(sessionB.id);

    const db = getDb();
    const rowsForA = await db
      .select()
      .from(schema.ai_chat_sessions)
      .where(eq(schema.ai_chat_sessions.user_id, USER_A));
    expect(rowsForA.map((r) => r.id)).toContain(sessionA.id);
    expect(rowsForA.map((r) => r.id)).not.toContain(sessionB.id);
  });

  it("cria sessão com título e contexto padrão (caminho feliz)", async () => {
    currentUserId = USER_A;
    const session = await getOrCreateChatSession();
    sessionIds.push(session.id);

    expect(session.title).toBe("Nova conversa");
    expect(session.context_type).toBe("general");
    expect(session.user_id).toBe(USER_A);
  });

  it("lança erro quando não autenticado", async () => {
    currentUserId = "";
    await expect(getOrCreateChatSession()).rejects.toThrow("Não autenticado");
  });
});
