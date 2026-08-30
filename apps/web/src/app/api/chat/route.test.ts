import "../../actions/test-db-setup";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { POST } from "./route";

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

// O `vitest.config.ts` do repo não tem alias para `@/` — só o TypeScript
// resolve esse prefixo. Todo import `@/...` da rota precisa, portanto, estar
// mockado aqui. Não é perda: nada do que estes dois módulos fazem influencia
// o que se testa (autorização e gravação no banco).
vi.mock("@/lib/ai/system-prompt", () => ({ SBF_SYSTEM_PROMPT: "prompt de teste" }));
vi.mock("@/lib/ai/tools", () => ({ sbfTools: {} }));

// Sem credencial de IA a rota devolveria 500 antes de chegar em qualquer
// consulta — e é justamente o comportamento de banco que se quer testar aqui.
vi.mock("@/lib/ai/provider", () => ({
  iaConfigurada: () => true,
  modeloDeConversa: () => ({ modelId: "fake" }),
  modeloDeSugestao: () => ({ modelId: "fake" }),
}));

interface FinishArgs {
  text: string;
  usage: { totalTokens: number };
  toolCalls: { toolName: string; args: unknown }[];
  toolResults: { result: unknown }[];
}

/** `onFinish` da última chamada a `streamText`, para o teste disparar à mão. */
let ultimoOnFinish: ((a: FinishArgs) => Promise<void>) | null = null;

// O `onFinish` real só roda quando o stream termina, o que não acontece com
// um modelo falso. `importOriginal` é obrigatório: `lib/ai/tools.ts` usa
// `tool()` deste mesmo pacote no topo do módulo, e um mock total quebraria a
// importação da rota.
vi.mock("ai", async (importOriginal) => {
  const actual = await importOriginal<typeof import("ai")>();
  return {
    ...actual,
    streamText: (opts: { onFinish?: (a: FinishArgs) => Promise<void> }) => {
      ultimoOnFinish = opts.onFinish ?? null;
      return { toDataStreamResponse: () => new Response("stream", { status: 200 }) };
    },
  };
});

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";
const ADMIN = "00000000-0000-0000-0000-00000000ad11";

function pedido(body: unknown): Request {
  return new Request("http://localhost/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("POST /api/chat", () => {
  const sessionIds: string[] = [];

  beforeEach(() => {
    sessao = { id: USER_A, ownerId: USER_A };
    ultimoOnFinish = null;
  });

  afterEach(async () => {
    const db = getDb();
    for (const id of sessionIds.splice(0)) {
      await db.delete(schema.ai_chat_messages).where(eq(schema.ai_chat_messages.session_id, id));
      await db.delete(schema.ai_chat_sessions).where(eq(schema.ai_chat_sessions.id, id));
    }
  });

  async function seedSession(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.ai_chat_sessions)
      .values({ user_id: userId, title: "Conversa de teste", context_type: "general" })
      .returning();
    if (!row) throw new Error("Falha ao semear ai_chat_session de teste");
    sessionIds.push(row.id);
    return row;
  }

  async function mensagensDaSessao(sessionId: string) {
    const db = getDb();
    return db
      .select()
      .from(schema.ai_chat_messages)
      .where(eq(schema.ai_chat_messages.session_id, sessionId));
  }

  it("responde 401 em JSON quando não há sessão", async () => {
    sessao = null;
    const res = await POST(pedido({ messages: [{ role: "user", content: "oi" }] }));

    expect(res.status).toBe(401);
    expect(res.headers.get("Content-Type")).toContain("application/json");
    await expect(res.json()).resolves.toMatchObject({ error: expect.any(String) });
  });

  // IDOR fechado nesta migração: `sessionId` vem do corpo da requisição e é
  // FK de ai_chat_messages. Sem a checagem de posse, o usuário A escreveria
  // dentro da conversa do usuário B.
  it("recusa sessionId de outro usuário e não grava mensagem nenhuma", async () => {
    const sessaoDeB = await seedSession(USER_B);

    sessao = { id: USER_A, ownerId: USER_A };
    const res = await POST(
      pedido({ messages: [{ role: "user", content: "injeção" }], sessionId: sessaoDeB.id }),
    );

    expect(res.status).toBe(404);
    expect(await mensagensDaSessao(sessaoDeB.id)).toHaveLength(0);
  });

  it("registra pergunta, resposta, tool calls e incrementa os contadores da sessão", async () => {
    const minhaSessao = await seedSession(USER_A);

    const res = await POST(
      pedido({
        messages: [{ role: "user", content: "quantos drafts?" }],
        sessionId: minhaSessao.id,
      }),
    );
    expect(res.status).toBe(200);

    const aposPergunta = await mensagensDaSessao(minhaSessao.id);
    expect(aposPergunta).toHaveLength(1);
    expect(aposPergunta[0]?.role).toBe("user");
    expect(aposPergunta[0]?.user_id).toBe(USER_A);
    expect(aposPergunta[0]?.content).toBe("quantos drafts?");

    // Fim do stream: é aqui que ficava a RPC `increment_session_tokens`, que
    // NÃO existe no Neon (ver NOTA nº 4 de packages/db/schema.sql). Este
    // trecho existe para provar que a substituição em SQL funciona.
    expect(ultimoOnFinish).toBeTypeOf("function");
    await ultimoOnFinish!({
      text: "resposta do assistente",
      usage: { totalTokens: 123 },
      toolCalls: [{ toolName: "list_drafts", args: { limit: 3 } }],
      toolResults: [{ result: { drafts: [] } }],
    });

    const todas = await mensagensDaSessao(minhaSessao.id);
    expect(todas.map((m) => m.role).sort()).toEqual(["assistant", "tool", "user"]);
    expect(todas.every((m) => m.user_id === USER_A)).toBe(true);

    const db = getDb();
    const [sessaoAtualizada] = await db
      .select()
      .from(schema.ai_chat_sessions)
      .where(eq(schema.ai_chat_sessions.id, minhaSessao.id));
    expect(sessaoAtualizada?.total_tokens_used).toBe(123);
    // 2 (pergunta + resposta) + 1 tool call
    expect(sessaoAtualizada?.message_count).toBe(3);
  });

  it("usa o ownerId da sessão, não o id do ator, ao gravar e ao autorizar", async () => {
    const sessaoDoDono = await seedSession(USER_A);
    const sessaoDoAtor = await seedSession(ADMIN);

    // Administrador de emergência operando a conta de USER_A.
    sessao = { id: ADMIN, ownerId: USER_A };

    const ok = await POST(
      pedido({ messages: [{ role: "user", content: "olá" }], sessionId: sessaoDoDono.id }),
    );
    expect(ok.status).toBe(200);

    const mensagens = await mensagensDaSessao(sessaoDoDono.id);
    expect(mensagens).toHaveLength(1);
    // A mensagem pertence ao DONO — é ele quem vê a conversa depois.
    expect(mensagens[0]?.user_id).toBe(USER_A);

    // E a sessão que pertence ao id do ATOR não é acessível.
    const recusado = await POST(
      pedido({ messages: [{ role: "user", content: "olá" }], sessionId: sessaoDoAtor.id }),
    );
    expect(recusado.status).toBe(404);
    expect(await mensagensDaSessao(sessaoDoAtor.id)).toHaveLength(0);
  });
});
