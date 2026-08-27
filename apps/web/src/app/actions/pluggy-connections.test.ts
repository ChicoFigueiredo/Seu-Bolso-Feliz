import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  getProviderConnections,
  revokeProviderConnection,
  updateExternalAccountMapping,
} from "./pluggy-connections";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("@sbf/financial-connectors/pluggy", () => ({ PluggyProvider: { fromEnv: () => ({}) } }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("pluggy-connections actions", () => {
  const createdConnectionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdConnectionIds.splice(0)) {
      await db
        .delete(schema.external_account_mappings)
        .where(eq(schema.external_account_mappings.provider_connection_id, id));
      await db.delete(schema.provider_connections).where(eq(schema.provider_connections.id, id));
    }
  });

  it("isola conexões por usuário", async () => {
    const db = getDb();
    const [connA] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_A, provider: "pluggy", external_item_id: "item-a", status: "active" })
      .returning();
    if (!connA) throw new Error("Falha ao semear conexão A");
    createdConnectionIds.push(connA.id);
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-b", status: "active" })
      .returning();
    if (!connB) throw new Error("Falha ao semear conexão B");
    createdConnectionIds.push(connB.id);

    currentUserId = USER_A;
    const listA = await getProviderConnections();
    expect(listA.map((c) => c.id)).toContain(connA.id);
    expect(listA.map((c) => c.id)).not.toContain(connB.id);
  });

  it("lança erro ao revogar conexão de outro usuário e não afeta a linha", async () => {
    const db = getDb();
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-c", status: "active" })
      .returning();
    if (!connB) throw new Error("Falha ao semear conexão B");
    createdConnectionIds.push(connB.id);

    currentUserId = USER_A;
    // Divergência deliberada do brief (ver ruling da revisão da Task 4,
    // consistente com deleteInstitution na Task 2): 0 linhas afetadas agora
    // lança erro, em vez de falhar em silêncio.
    await expect(revokeProviderConnection(connB.id)).rejects.toThrow();
    const [stillActive] = await db
      .select()
      .from(schema.provider_connections)
      .where(eq(schema.provider_connections.id, connB.id));
    if (!stillActive) throw new Error("Conexão B não encontrada após revoke");
    expect(stillActive.status).toBe("active");
  });

  it("lança erro ao atualizar mapeamento de outro usuário", async () => {
    const db = getDb();
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-d", status: "active" })
      .returning();
    if (!connB) throw new Error("Falha ao semear conexão B");
    createdConnectionIds.push(connB.id);
    const [mapping] = await db
      .insert(schema.external_account_mappings)
      .values({
        user_id: USER_B,
        provider_connection_id: connB.id,
        external_account_id: "acc-1",
        external_account_name: "Conta",
        external_account_type: "checking",
      })
      .returning();
    if (!mapping) throw new Error("Falha ao semear mapeamento");

    currentUserId = USER_A;
    await expect(
      updateExternalAccountMapping(mapping.id, { external_account_name: "Hackeado" }),
    ).rejects.toThrow();
  });
});
