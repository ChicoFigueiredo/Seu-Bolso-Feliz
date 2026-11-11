import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { and, eq, sql } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { deleteSecret, listSecrets, setSecret } from "./secrets";

let currentUserId = "";
// `ownerId` é o dono dos dados; `id` é o ator. Toda action filtra por
// `ownerId` — o mock usa ids DIFERENTES de propósito, para que um filtro
// feito por `id` por engano apareça como falha em vez de passar despercebido.
vi.mock("@/lib/auth", () => ({
  requireUser: async () => ({
    id: "00000000-0000-0000-0000-00000000ac70",
    ownerId: currentUserId,
    email: "teste@exemplo.com",
    role: "owner" as const,
    name: "Teste",
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("secrets actions", () => {
  const secretIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of secretIds.splice(0)) {
      await db.delete(schema.user_secrets).where(eq(schema.user_secrets.id, id));
    }
  });

  async function seedSecret(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.user_secrets)
      .values({
        user_id: userId,
        secret_type: "pdf_password",
        encrypted_value: "fake-encrypted-value",
      })
      .returning();
    if (!row) throw new Error("Falha ao semear user_secret de teste");
    secretIds.push(row.id);
    return row;
  }

  it("isola segredos por usuário (sem devolver o valor em claro)", async () => {
    currentUserId = USER_A;
    const a = await seedSecret(USER_A);
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    const listA = await listSecrets();
    expect(listA.map((s) => s.id)).toContain(a.id);
    expect(listA.map((s) => s.id)).not.toContain(b.id);
    // A linha PRÓPRIA, não `listA[0]`: se a lista viesse vazia, um índice fixo
    // faria `expect(undefined).not.toHaveProperty(...)` passar sem testar nada.
    const linhaDeA = listA.find((s) => s.id === a.id)!;
    expect(linhaDeA).toBeDefined();
    expect(linhaDeA).not.toHaveProperty("encryptedValue");
    expect(linhaDeA).not.toHaveProperty("encrypted_value");
  });

  it("lança erro ao excluir segredo de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    await expect(deleteSecret(b.id)).rejects.toThrow();
  });
});

// ══════════════════════════════════════════════════════════════
// setSecret (era a RPC fn_set_secret)
// ══════════════════════════════════════════════════════════════
//
// Estes testes exercem a criptografia de verdade: o banco de teste precisa
// ter o schema `private` (crypto_keys + get_crypto_key) como o Neon tem, ou
// `encrypt_secret` levanta "nenhuma chave de criptografia disponível".

describe.skipIf(!process.env.TEST_DATABASE_URL)("setSecret", () => {
  const secretIds: string[] = [];
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    secretIds.length = 0;
    // Limpeza por DONO, não por id coletado: vários testes aqui contam as
    // linhas de um usuário para provar que nada foi gravado. Se uma guarda de
    // posse regredir, o insert que o teste esperava ver recusado acontece — e
    // o id dele nunca chega a `secretIds`, deixando uma linha órfã que faria o
    // teste SEGUINTE falhar por contaminação em vez de pelo bug real.
    for (const userId of [USER_A, USER_B]) {
      await db.delete(schema.user_secrets).where(eq(schema.user_secrets.user_id, userId));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    // `audit_logs` não é limpo: o trigger `prevent_audit_log_mutation` proíbe
    // DELETE por design. Os testes que a inspecionam filtram pelo `entity_id`
    // do segredo daquele caso, nunca por contagem global.
  });

  async function seedSupplier(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.suppliers)
      .values({ user_id: userId, name: `Fornecedor ${crypto.randomUUID()}` })
      .returning();
    if (!row) throw new Error("Falha ao semear supplier de teste");
    supplierIds.push(row.id);
    return row;
  }

  async function linhaCrua(id: string) {
    const db = getDb();
    const [row] = await db.select().from(schema.user_secrets).where(eq(schema.user_secrets.id, id));
    return row!;
  }

  it("grava a senha CRIPTOGRAFADA e o round-trip devolve o texto original", async () => {
    currentUserId = USER_A;
    const claro = `senha-${crypto.randomUUID()}`;

    const { id } = await setSecret({ plaintext: claro, label: "Fatura do banco" });
    secretIds.push(id);

    const row = await linhaCrua(id);
    expect(row.user_id).toBe(USER_A);
    expect(row.label).toBe("Fatura do banco");
    expect(row.secret_type).toBe("pdf_password");
    expect(row.encryption_version).toBe(1);
    // O que está na coluna NÃO pode ser o texto em claro.
    expect(row.encrypted_value).not.toBe(claro);
    expect(row.encrypted_value).not.toContain(claro);

    // E `decrypt_secret` — a mesma função que o worker usa via fn_get_secrets
    // — devolve exatamente o que foi gravado.
    const db = getDb();
    const decifrado = await db.execute<{ valor: string }>(
      sql`SELECT public.decrypt_secret(${row.encrypted_value}::text, ${row.encryption_version}::int) AS valor`,
    );
    expect(decifrado.rows[0]?.valor).toBe(claro);
  });

  it("regravar no mesmo escopo substitui o valor em vez de duplicar a linha", async () => {
    currentUserId = USER_A;
    const primeira = await setSecret({ plaintext: "senha-antiga", label: "Rótulo original" });
    secretIds.push(primeira.id);

    const segunda = await setSecret({ plaintext: "senha-nova" });
    expect(segunda.id).toBe(primeira.id);

    const db = getDb();
    const todas = await db
      .select()
      .from(schema.user_secrets)
      .where(eq(schema.user_secrets.user_id, USER_A));
    expect(todas).toHaveLength(1);

    const row = await linhaCrua(primeira.id);
    // `label` novo é NULL → COALESCE preserva o antigo (comportamento da RPC).
    expect(row.label).toBe("Rótulo original");
    const decifrado = await db.execute<{ valor: string }>(
      sql`SELECT public.decrypt_secret(${row.encrypted_value}::text, ${row.encryption_version}::int) AS valor`,
    );
    expect(decifrado.rows[0]?.valor).toBe("senha-nova");
  });

  it("escopos diferentes convivem: global e por fornecedor são linhas distintas", async () => {
    currentUserId = USER_A;
    const fornecedor = await seedSupplier(USER_A);

    const global = await setSecret({ plaintext: "senha-global" });
    secretIds.push(global.id);
    const doFornecedor = await setSecret({
      plaintext: "senha-do-fornecedor",
      entityType: "supplier",
      entityId: fornecedor.id,
    });
    secretIds.push(doFornecedor.id);

    expect(doFornecedor.id).not.toBe(global.id);
    const lista = await listSecrets();
    expect(lista.map((s) => s.id).sort()).toEqual([global.id, doFornecedor.id].sort());
    expect(lista.find((s) => s.id === doFornecedor.id)?.entityId).toBe(fornecedor.id);
  });

  // IDOR: `user_secrets.entity_id` é polimórfico, não tem FK, e o trigger de
  // validação que a migration prometeu nunca chegou ao banco.
  it("recusa entityId de fornecedor de outro usuário, sem gravar nada", async () => {
    currentUserId = USER_B;
    const fornecedorB = await seedSupplier(USER_B);

    currentUserId = USER_A;
    await expect(
      setSecret({ plaintext: "invasao", entityType: "supplier", entityId: fornecedorB.id }),
    ).rejects.toThrow();

    const db = getDb();
    const linhas = await db
      .select()
      .from(schema.user_secrets)
      .where(eq(schema.user_secrets.user_id, USER_A));
    expect(linhas).toHaveLength(0);
  });

  it("recusa entityId sem entityType (não há tabela onde checar posse)", async () => {
    currentUserId = USER_A;
    const fornecedor = await seedSupplier(USER_A);
    await expect(setSecret({ plaintext: "x", entityId: fornecedor.id })).rejects.toThrow();
  });

  it("recusa entityType que não é entidade nenhuma (chave herdada de Object)", async () => {
    currentUserId = USER_A;
    const fornecedor = await seedSupplier(USER_A);
    for (const forjado of ["constructor", "toString", "__proto__", "supplier "]) {
      await expect(
        setSecret({
          plaintext: "x",
          entityType: forjado as unknown as "supplier",
          entityId: fornecedor.id,
        }),
      ).rejects.toThrow(/Tipo de entidade desconhecido/);
    }
    const db = getDb();
    const linhas = await db
      .select()
      .from(schema.user_secrets)
      .where(eq(schema.user_secrets.user_id, USER_A));
    expect(linhas).toHaveLength(0);
  });

  it("recusa senha vazia", async () => {
    currentUserId = USER_A;
    await expect(setSecret({ plaintext: "" })).rejects.toThrow("Informe a senha");
  });

  it("audita a gravação sem registrar o segredo em lugar algum", async () => {
    currentUserId = USER_A;
    const claro = `senha-auditada-${crypto.randomUUID()}`;
    const { id } = await setSecret({ plaintext: claro, label: "Auditada" });
    secretIds.push(id);

    const db = getDb();
    const logs = await db
      .select()
      .from(schema.audit_logs)
      .where(
        and(
          eq(schema.audit_logs.user_id, USER_A),
          eq(schema.audit_logs.action, "secret_written"),
          eq(schema.audit_logs.entity_id, id),
        ),
      );
    expect(logs).toHaveLength(1);
    expect(logs[0]?.entity_type).toBe("user_secrets");
    expect(logs[0]?.new_values).toEqual({ secret_type: "pdf_password", entity_type: null });
    expect(JSON.stringify(logs[0]?.new_values)).not.toContain(claro);
  });

  it("segredo gravado por um usuário não aparece para o outro", async () => {
    currentUserId = USER_A;
    const { id } = await setSecret({ plaintext: "so-do-A" });
    secretIds.push(id);

    currentUserId = USER_B;
    expect(await listSecrets()).toHaveLength(0);
    await expect(deleteSecret(id)).rejects.toThrow();
  });

  // Contrato cross-surface: quem consome o segredo é o worker de ingestão, via
  // `fn_get_secrets` (+ `fn_mark_secret_used`) — funções SECURITY DEFINER que
  // sobreviveram à migração e continuam no Neon. Verificar só
  // `decrypt_secret(encrypted_value)` provaria menos: é por esta porta que a
  // senha realmente sai.
  it("o worker lê de volta, pela porta dele, exatamente o que foi gravado", async () => {
    currentUserId = USER_A;
    const claro = `senha-do-worker-${crypto.randomUUID()}`;
    const { id } = await setSecret({ plaintext: claro, label: "Boleto protegido" });
    secretIds.push(id);

    const db = getDb();
    const lidos = await db.execute<{
      secret_id: string;
      value: string;
      label: string | null;
    }>(sql`SELECT * FROM public.fn_get_secrets(${USER_A}::uuid, ${"pdf_password"}::text, 20)`);

    const doTeste = lidos.rows.find((r) => r.secret_id === id);
    expect(doTeste).toBeDefined();
    expect(doTeste!.value).toBe(claro);
    expect(doTeste!.label).toBe("Boleto protegido");

    // `fn_get_secrets` ordena por uso; `fn_mark_secret_used` é o que alimenta
    // essa ordem depois que a senha abre um documento.
    await db.execute(sql`SELECT public.fn_mark_secret_used(${USER_A}::uuid, ${id}::uuid)`);
    const row = await linhaCrua(id);
    expect(row.success_count).toBe(1);
    expect(row.last_used_at).not.toBeNull();
  });

  it("fn_get_secrets não entrega o segredo de A para o worker pedindo por B", async () => {
    currentUserId = USER_A;
    const { id } = await setSecret({ plaintext: "so-do-A-de-novo" });
    secretIds.push(id);

    const db = getDb();
    const lidos = await db.execute<{ secret_id: string }>(
      sql`SELECT * FROM public.fn_get_secrets(${USER_B}::uuid, ${"pdf_password"}::text, 20)`,
    );
    expect(lidos.rows.map((r) => r.secret_id)).not.toContain(id);
  });
});
