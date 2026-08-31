import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const execute = vi.fn();
vi.mock("./client", () => ({ getDb: () => ({ execute }) }));

import { resolveOwnerId, resetOwnerCache } from "./owner";

const OWNER = "5b7682b4-9f0c-4475-91e2-6a8996fb2330";
const ORFAO = "8e2ee153-eee7-4287-b630-ef6c60469000";

beforeEach(() => {
  execute.mockReset();
  resetOwnerCache();
  delete process.env.LOCAL_USER_ID;
});
afterEach(() => {
  delete process.env.LOCAL_USER_ID;
});

describe("resolveOwnerId", () => {
  it("sem LOCAL_USER_ID, pega o dono do banco", async () => {
    execute.mockResolvedValue({ rows: [{ id: OWNER }] });
    await expect(resolveOwnerId()).resolves.toBe(OWNER);
  });

  it("com LOCAL_USER_ID válido, respeita o override", async () => {
    process.env.LOCAL_USER_ID = OWNER;
    execute.mockResolvedValue({ rows: [{ id: OWNER }] });
    await expect(resolveOwnerId()).resolves.toBe(OWNER);
  });

  // O caso que motivou este módulo: o banco foi recriado, os ids mudaram, e o
  // `.env` ficou com o antigo. Antes isto passava batido e os workers gravavam
  // linhas que a aplicação nunca mostraria.
  it("com LOCAL_USER_ID apontando para usuário inexistente, falha na largada", async () => {
    process.env.LOCAL_USER_ID = ORFAO;
    execute.mockResolvedValue({ rows: [] });
    await expect(resolveOwnerId()).rejects.toThrow(/não corresponde a nenhum usuário/);
  });

  it("mensagem do erro nomeia o valor culpado, para o operador achar no .env", async () => {
    process.env.LOCAL_USER_ID = ORFAO;
    execute.mockResolvedValue({ rows: [] });
    await expect(resolveOwnerId()).rejects.toThrow(ORFAO);
  });

  it("sem dono no banco, diz que faltam as migrations", async () => {
    execute.mockResolvedValue({ rows: [] });
    await expect(resolveOwnerId()).rejects.toThrow(/migrations/);
  });

  it("consulta o banco uma vez só — o id não muda durante o processo", async () => {
    execute.mockResolvedValue({ rows: [{ id: OWNER }] });
    await resolveOwnerId();
    await resolveOwnerId();
    await resolveOwnerId();
    expect(execute).toHaveBeenCalledTimes(1);
  });

  it("string vazia em LOCAL_USER_ID não conta como override", async () => {
    process.env.LOCAL_USER_ID = "   ";
    execute.mockResolvedValue({ rows: [{ id: OWNER }] });
    await expect(resolveOwnerId()).resolves.toBe(OWNER);
    expect(execute.mock.calls).toHaveLength(1);
  });
});
