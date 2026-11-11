import { beforeEach, describe, expect, it, vi } from "vitest";

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

// O `vitest.config.ts` do repo não tem alias para `@/` — todo import `@/...`
// da rota precisa estar mockado. Nenhum deles participa do que se testa aqui
// (autenticação e a lista de ferramentas permitidas).
vi.mock("@/lib/ai/provider", () => ({
  iaConfigurada: () => true,
  modeloDeSugestao: () => ({ modelId: "fake" }),
  modeloDeConversa: () => ({ modelId: "fake" }),
}));
vi.mock("@/lib/ai/tools", () => ({ sbfTools: {} }));

const { POST } = await import("./route");

const USER_A = "00000000-0000-0000-0000-0000000000a1";

function pedido(body: unknown): Request {
  return new Request("http://localhost/api/ai-suggest", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

// Esta rota não toca no banco: as consultas ficam nas ferramentas de
// `lib/ai/tools.ts` (testadas em lib/ai/tools.test.ts). Aqui só valem o
// portão de autenticação e a lista de ferramentas permitidas.
describe("POST /api/ai-suggest", () => {
  beforeEach(() => {
    sessao = { id: USER_A, ownerId: USER_A };
  });

  it("responde 401 em JSON quando não há sessão", async () => {
    sessao = null;
    const res = await POST(pedido({ toolName: "suggest_splits", params: {} }));

    expect(res.status).toBe(401);
    await expect(res.json()).resolves.toMatchObject({ error: expect.any(String) });
  });

  it("recusa com 403 uma ferramenta fora da lista permitida", async () => {
    // `approve_draft` existe no registro de ferramentas do chat, mas escreve
    // no banco — não pode ser disparada por um endpoint de sugestão inline.
    const res = await POST(pedido({ toolName: "approve_draft", params: { draftId: "x" } }));

    expect(res.status).toBe(403);
  });

  it("recusa com 400 quando toolName não vem", async () => {
    const res = await POST(pedido({ params: {} }));
    expect(res.status).toBe(400);
  });
});
