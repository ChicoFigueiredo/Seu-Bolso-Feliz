import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { carregarPainelConexoes } from "./painel-conexoes";

let currentUserId = "";
// Mesmo mock de identidade dos outros testes de action: `ownerId === id`
// porque estes casos exercitam o dono. A distinção ator/dono tem cobertura
// própria em apps/web/src/app/api/**.
vi.mock("@/lib/auth", () => {
  const sessao = () =>
    currentUserId
      ? {
          id: currentUserId,
          ownerId: currentUserId,
          email: "teste@exemplo.com",
          role: "owner" as const,
          name: "Teste",
        }
      : null;
  return {
    requireUser: async () => {
      const s = sessao();
      if (!s) throw new Error("Não autenticado");
      return s;
    },
    getUser: async () => sessao(),
    getUserOrNull: async () => sessao(),
  };
});

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("carregarPainelConexoes", () => {
  const conexoesCriadas: string[] = [];
  const checkpointsCriados: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of conexoesCriadas.splice(0)) {
      await db
        .delete(schema.external_account_mappings)
        .where(eq(schema.external_account_mappings.provider_connection_id, id));
      await db.delete(schema.provider_connections).where(eq(schema.provider_connections.id, id));
    }
    for (const id of checkpointsCriados.splice(0)) {
      await db.delete(schema.ingestion_checkpoints).where(eq(schema.ingestion_checkpoints.id, id));
    }
  });

  async function semearConexao(
    userId: string,
    itemId: string,
    status = "active",
  ): Promise<{ id: string }> {
    const db = getDb();
    const [linha] = await db
      .insert(schema.provider_connections)
      .values({ user_id: userId, provider: "pluggy", external_item_id: itemId, status })
      .returning({ id: schema.provider_connections.id });
    if (!linha) throw new Error("Falha ao semear conexão");
    conexoesCriadas.push(linha.id);
    return linha;
  }

  async function semearContaExterna(
    userId: string,
    conexaoId: string,
    externalAccountId: string,
    status = "pending_mapping",
  ): Promise<void> {
    const db = getDb();
    await db.insert(schema.external_account_mappings).values({
      user_id: userId,
      provider_connection_id: conexaoId,
      external_account_id: externalAccountId,
      external_account_name: "Conta",
      external_account_type: "BANK",
      status,
    });
  }

  it("não devolve conexão de outro usuário", async () => {
    const conexaoA = await semearConexao(USER_A, "painel-item-a");
    const conexaoB = await semearConexao(USER_B, "painel-item-b");

    currentUserId = USER_A;
    const painel = await carregarPainelConexoes();
    const ids = painel.conexoes.map((c) => c.id);
    expect(ids).toContain(conexaoA.id);
    expect(ids).not.toContain(conexaoB.id);
  });

  it("não deixa vazar a conta externa de outro usuário na mesma conexão", async () => {
    // Defesa contra o erro clássico: filtrar mappings só por
    // provider_connection_id, sem o user_id junto.
    const conexaoA = await semearConexao(USER_A, "painel-item-c");
    await semearContaExterna(USER_A, conexaoA.id, "conta-do-a");
    await semearContaExterna(USER_B, conexaoA.id, "conta-do-b");

    currentUserId = USER_A;
    const painel = await carregarPainelConexoes();
    const conexao = painel.conexoes.find((c) => c.id === conexaoA.id);
    expect(conexao).toBeDefined();
    expect(conexao?.contas).toHaveLength(1);
  });

  it("conta como pendência só a conta sem vínculo de conexão viva", async () => {
    const viva = await semearConexao(USER_A, "painel-item-d");
    await semearContaExterna(USER_A, viva.id, "pendente-1");
    await semearContaExterna(USER_A, viva.id, "ignorada-1", "ignored");

    const revogada = await semearConexao(USER_A, "painel-item-e", "revoked");
    await semearContaExterna(USER_A, revogada.id, "pendente-2");

    currentUserId = USER_A;
    const painel = await carregarPainelConexoes();
    // Só "pendente-1": a ignorada já foi decidida, e a da conexão revogada não
    // sincroniza mais — cobrar vínculo dela seria pedir trabalho inútil.
    expect(painel.contasAguardandoVinculo).toBe(1);
  });

  it("marca importação de histórico a partir do checkpoint vivo do backfill", async () => {
    const db = getDb();
    const conexao = await semearConexao(USER_A, "painel-item-f");
    const [checkpoint] = await db
      .insert(schema.ingestion_checkpoints)
      .values({
        user_id: USER_A,
        source_type: "pluggy",
        // Mesmo formato gravado por workers/pluggy-sync/src/sync-runner.ts.
        scope_key: "painel-item-f:conta-123",
        cursor_kind: "page_number",
        status: "running",
      })
      .returning({ id: schema.ingestion_checkpoints.id });
    if (!checkpoint) throw new Error("Falha ao semear checkpoint");
    checkpointsCriados.push(checkpoint.id);

    currentUserId = USER_A;
    const painel = await carregarPainelConexoes();
    expect(painel.conexoes.find((c) => c.id === conexao.id)?.importandoHistorico).toBe(true);
  });

  it("informa se a credencial da Pluggy existe sem devolver o valor dela", async () => {
    const idOriginal = process.env.PLUGGY_CLIENT_ID;
    const segredoOriginal = process.env.PLUGGY_CLIENT_SECRET;
    currentUserId = USER_A;

    try {
      delete process.env.PLUGGY_CLIENT_ID;
      delete process.env.PLUGGY_CLIENT_SECRET;
      expect((await carregarPainelConexoes()).pluggyConfigurado).toBe(false);

      process.env.PLUGGY_CLIENT_ID = "id-de-teste";
      process.env.PLUGGY_CLIENT_SECRET = "segredo-de-teste";
      const painel = await carregarPainelConexoes();
      expect(painel.pluggyConfigurado).toBe(true);
      // O que atravessa é o booleano, nunca o segredo.
      expect(JSON.stringify(painel)).not.toContain("segredo-de-teste");
    } finally {
      if (idOriginal === undefined) delete process.env.PLUGGY_CLIENT_ID;
      else process.env.PLUGGY_CLIENT_ID = idOriginal;
      if (segredoOriginal === undefined) delete process.env.PLUGGY_CLIENT_SECRET;
      else process.env.PLUGGY_CLIENT_SECRET = segredoOriginal;
    }
  });
});
