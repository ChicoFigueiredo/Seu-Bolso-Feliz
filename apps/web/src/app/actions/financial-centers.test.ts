import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  allocateTransaction,
  allocateTransactionEvenly,
  archiveFinancialCenter,
  createFinancialCenter,
  deleteFinancialCenter,
  getFinancialCenter,
  getTransactionAllocations,
  listFinancialCenters,
  listFinancialCentersWithBalance,
  migrateTagsToCenters,
  updateFinancialCenter,
} from "./financial-centers";

const USER_A = "00000000-0000-0000-0000-0000000000c1";
const USER_B = "00000000-0000-0000-0000-0000000000c2";

let usuarioAtual = USER_A;
vi.mock("@/lib/auth", () => ({
  // `ownerId` é o dono dos dados; `id` é o ator. Toda action filtra por
  // ownerId — o teste usa ids diferentes de propósito, para que um filtro
  // feito por `id` por engano apareça como falha.
  requireUser: async () => ({
    id: "00000000-0000-0000-0000-00000000ac70",
    ownerId: usuarioAtual,
    email: "teste@exemplo.com",
    role: "owner" as const,
    name: "Teste",
  }),
}));

const institutionIds: string[] = [];
const centerIds: string[] = [];
const tagIds: string[] = [];

async function comoUsuario<T>(userId: string, fn: () => Promise<T>): Promise<T> {
  const anterior = usuarioAtual;
  usuarioAtual = userId;
  try {
    return await fn();
  } finally {
    usuarioAtual = anterior;
  }
}

async function seedProduto(userId: string): Promise<string> {
  const institution = await seedInstitution(userId);
  institutionIds.push(institution.id);
  const produto = await seedFinancialProduct(userId, institution.id);
  return produto.id;
}

async function seedMovimentacao(
  userId: string,
  produtoId: string,
  input: { amount: number; type?: string; event_date?: string },
): Promise<string> {
  const db = getDb();
  const [linha] = await db
    .insert(schema.transactions)
    .values({
      user_id: userId,
      financial_product_id: produtoId,
      type: input.type ?? "expense",
      amount: input.amount,
      event_date: input.event_date ?? "2026-08-10",
    })
    .returning({ id: schema.transactions.id });
  if (!linha) throw new Error("Falha ao semear movimentação de teste");
  return linha.id;
}

async function seedTag(userId: string, name: string): Promise<string> {
  const db = getDb();
  const [linha] = await db
    .insert(schema.tags)
    .values({ user_id: userId, name })
    .returning({ id: schema.tags.id });
  if (!linha) throw new Error("Falha ao semear tag de teste");
  tagIds.push(linha.id);
  return linha.id;
}

async function marcaComTag(transactionId: string, tagId: string): Promise<void> {
  await getDb()
    .insert(schema.transaction_tags)
    .values({ transaction_id: transactionId, tag_id: tagId });
}

async function criaCentro(userId: string, name: string): Promise<string> {
  const centro = await comoUsuario(userId, () => createFinancialCenter({ name }));
  centerIds.push(centro.id);
  return centro.id;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-centers actions", () => {
  afterEach(async () => {
    const db = getDb();
    // Instituição em cascata leva produtos, transações e (por cascata da FK)
    // os rateios; os centros só saem depois, porque a FK é `restrict`.
    for (const id of institutionIds.splice(0)) await cleanupInstitution(id);
    for (const id of tagIds.splice(0)) {
      await db.delete(schema.tags).where(eq(schema.tags.id, id));
    }
    for (const id of centerIds.splice(0)) {
      await db.delete(schema.financial_centers).where(eq(schema.financial_centers.id, id));
    }
    // Centros criados pela migração não passam por `criaCentro`.
    for (const userId of [USER_A, USER_B]) {
      await db.delete(schema.financial_centers).where(eq(schema.financial_centers.user_id, userId));
    }
    usuarioAtual = USER_A;
  });

  // ── CRUD e isolamento ──

  it("isola centros por usuário", async () => {
    const centroA = await criaCentro(USER_A, "Edulingo");
    const centroB = await criaCentro(USER_B, "Imob Max");

    const listaA = await comoUsuario(USER_A, () => listFinancialCenters());
    expect(listaA.map((c) => c.id)).toContain(centroA);
    expect(listaA.map((c) => c.id)).not.toContain(centroB);
    expect(await comoUsuario(USER_A, () => getFinancialCenter(centroB))).toBeNull();
  });

  it("cria, atualiza, arquiva e exclui um centro", async () => {
    const id = await criaCentro(USER_A, "Seu Bolso Feliz");
    await comoUsuario(USER_A, async () => {
      const atualizado = await updateFinancialCenter(id, { description: "Produto próprio" });
      expect(atualizado.description).toBe("Produto próprio");
      expect((await archiveFinancialCenter(id)).status).toBe("archived");
      await deleteFinancialCenter(id);
      expect(await getFinancialCenter(id)).toBeNull();
    });
    centerIds.length = 0;
  });

  it("não deixa atualizar nem excluir centro de outro usuário", async () => {
    const centroB = await criaCentro(USER_B, "Daiane");
    await comoUsuario(USER_A, async () => {
      await expect(updateFinancialCenter(centroB, { name: "Roubado" })).rejects.toThrow();
      await expect(deleteFinancialCenter(centroB)).rejects.toThrow();
    });
  });

  it("recusa excluir centro que já tem rateio — o caminho é arquivar", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const centro = await criaCentro(USER_A, "Edulingo");

    await comoUsuario(USER_A, async () => {
      await allocateTransaction(movimentacao, [{ financialCenterId: centro, amount: 40 }]);
      await expect(deleteFinancialCenter(centro)).rejects.toThrow(/arquive/i);
    });
  });

  // ── Rateio ──

  it("grava o conjunto de rateios e devolve o não alocado", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 150 });
    const edulingo = await criaCentro(USER_A, "Edulingo");
    const imob = await criaCentro(USER_A, "Imob Max");

    const resumo = await comoUsuario(USER_A, () =>
      allocateTransaction(movimentacao, [
        { financialCenterId: edulingo, amount: 100 },
        { financialCenterId: imob, amount: 20 },
      ]),
    );

    expect(resumo.allocated).toBe(120);
    expect(resumo.unallocated).toBe(30);
    expect(resumo.allocations).toHaveLength(2);
  });

  it("substitui o conjunto inteiro a cada chamada", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const edulingo = await criaCentro(USER_A, "Edulingo");
    const imob = await criaCentro(USER_A, "Imob Max");

    await comoUsuario(USER_A, async () => {
      await allocateTransaction(movimentacao, [
        { financialCenterId: edulingo, amount: 60 },
        { financialCenterId: imob, amount: 40 },
      ]);
      const resumo = await allocateTransaction(movimentacao, [
        { financialCenterId: imob, amount: 10 },
      ]);
      expect(resumo.allocations).toHaveLength(1);
      expect(resumo.allocations[0]?.financial_center_id).toBe(imob);
      expect(resumo.unallocated).toBe(90);
    });
  });

  it("lista vazia apaga todos os rateios", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const centro = await criaCentro(USER_A, "Edulingo");

    await comoUsuario(USER_A, async () => {
      await allocateTransaction(movimentacao, [{ financialCenterId: centro, amount: 50 }]);
      const resumo = await allocateTransaction(movimentacao, []);
      expect(resumo.allocations).toHaveLength(0);
      expect(resumo.unallocated).toBe(100);
    });
  });

  it("recusa soma maior que a movimentação e não grava nada", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const edulingo = await criaCentro(USER_A, "Edulingo");
    const imob = await criaCentro(USER_A, "Imob Max");

    await comoUsuario(USER_A, async () => {
      await expect(
        allocateTransaction(movimentacao, [
          { financialCenterId: edulingo, amount: 99.99 },
          { financialCenterId: imob, amount: 0.02 },
        ]),
      ).rejects.toThrow(/maior que o valor/i);
      expect((await getTransactionAllocations(movimentacao)).allocations).toHaveLength(0);
    });
  });

  it("bloqueia rateio em pagamento de fatura", async () => {
    const produto = await seedProduto(USER_A);
    const fatura = await seedMovimentacao(USER_A, produto, {
      amount: 800,
      type: "statement_payment",
    });
    const centro = await criaCentro(USER_A, "Edulingo");

    await comoUsuario(USER_A, async () => {
      await expect(
        allocateTransaction(fatura, [{ financialCenterId: centro, amount: 10 }]),
      ).rejects.toThrow(/fatura/i);
    });
  });

  it("receita rateia igual a despesa", async () => {
    const produto = await seedProduto(USER_A);
    const receita = await seedMovimentacao(USER_A, produto, { amount: 5000, type: "income" });
    const centro = await criaCentro(USER_A, "Edulingo");

    const resumo = await comoUsuario(USER_A, () =>
      allocateTransaction(receita, [{ financialCenterId: centro, amount: 5000 }]),
    );
    expect(resumo.allocated).toBe(5000);
    expect(resumo.unallocated).toBe(0);
  });

  it("divide igualmente sem perder centavo, com o resto no primeiro centro", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const a = await criaCentro(USER_A, "Edulingo");
    const b = await criaCentro(USER_A, "Imob Max");
    const c = await criaCentro(USER_A, "Daiane");

    const resumo = await comoUsuario(USER_A, () =>
      allocateTransactionEvenly(movimentacao, [a, b, c]),
    );
    expect(resumo.allocated).toBe(100);
    expect(resumo.unallocated).toBe(0);
    const porCentro = new Map(resumo.allocations.map((r) => [r.financial_center_id, r.amount]));
    expect(porCentro.get(a)).toBe(33.34);
    expect(porCentro.get(b)).toBe(33.33);
    expect(porCentro.get(c)).toBe(33.33);
  });

  // ── IDOR ──

  it("recusa rateio em movimentação de outro usuário", async () => {
    const produtoB = await seedProduto(USER_B);
    const movimentacaoB = await seedMovimentacao(USER_B, produtoB, { amount: 100 });
    const centroA = await criaCentro(USER_A, "Edulingo");

    await comoUsuario(USER_A, async () => {
      await expect(
        allocateTransaction(movimentacaoB, [{ financialCenterId: centroA, amount: 10 }]),
      ).rejects.toThrow(/movimentação não encontrada/i);
    });
  });

  it("recusa rateio apontando para centro de outro usuário", async () => {
    const produtoA = await seedProduto(USER_A);
    const movimentacaoA = await seedMovimentacao(USER_A, produtoA, { amount: 100 });
    const centroB = await criaCentro(USER_B, "Imob Max");

    await comoUsuario(USER_A, async () => {
      await expect(
        allocateTransaction(movimentacaoA, [{ financialCenterId: centroB, amount: 10 }]),
      ).rejects.toThrow(/centros não foram encontrados/i);
      expect((await getTransactionAllocations(movimentacaoA)).allocations).toHaveLength(0);
    });
  });

  it("recusa ler os rateios de movimentação de outro usuário", async () => {
    const produtoB = await seedProduto(USER_B);
    const movimentacaoB = await seedMovimentacao(USER_B, produtoB, { amount: 100 });
    await comoUsuario(USER_A, async () => {
      await expect(getTransactionAllocations(movimentacaoB)).rejects.toThrow();
    });
  });

  it("recusa rateio em centro arquivado", async () => {
    const produto = await seedProduto(USER_A);
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    const centro = await criaCentro(USER_A, "Edulingo");

    await comoUsuario(USER_A, async () => {
      await archiveFinancialCenter(centro);
      await expect(
        allocateTransaction(movimentacao, [{ financialCenterId: centro, amount: 10 }]),
      ).rejects.toThrow(/arquivado/i);
    });
  });

  // ── Saldo ──

  it("calcula saldo, entradas, saídas e resultado do mês", async () => {
    const produto = await seedProduto(USER_A);
    const receita = await seedMovimentacao(USER_A, produto, {
      amount: 1000,
      type: "income",
      event_date: "2026-08-05",
    });
    const despesa = await seedMovimentacao(USER_A, produto, {
      amount: 300,
      type: "expense",
      event_date: "2026-08-20",
    });
    const despesaAntiga = await seedMovimentacao(USER_A, produto, {
      amount: 150,
      type: "expense",
      event_date: "2026-07-15",
    });
    const centro = await criaCentro(USER_A, "Edulingo");

    const saldo = await comoUsuario(USER_A, async () => {
      await allocateTransaction(receita, [{ financialCenterId: centro, amount: 1000 }]);
      await allocateTransaction(despesa, [{ financialCenterId: centro, amount: 300 }]);
      await allocateTransaction(despesaAntiga, [{ financialCenterId: centro, amount: 150 }]);
      return listFinancialCentersWithBalance({
        startDate: "2026-08-01",
        endDate: "2026-08-31",
      });
    });

    const linha = saldo.find((c) => c.id === centro);
    expect(linha).toBeDefined();
    expect(linha?.inflow).toBe(1000);
    expect(linha?.outflow).toBe(450);
    expect(linha?.balance).toBe(550);
    expect(linha?.monthInflow).toBe(1000);
    expect(linha?.monthOutflow).toBe(300);
    expect(linha?.monthResult).toBe(700);
  });

  it("centro sem movimento aparece zerado, não some da lista", async () => {
    const centro = await criaCentro(USER_A, "IA");
    const saldo = await comoUsuario(USER_A, () => listFinancialCentersWithBalance());
    const linha = saldo.find((c) => c.id === centro);
    expect(linha).toMatchObject({ inflow: 0, outflow: 0, balance: 0, monthResult: 0 });
  });

  it("saldo não vaza rateio de outro usuário", async () => {
    const produtoB = await seedProduto(USER_B);
    const movimentacaoB = await seedMovimentacao(USER_B, produtoB, { amount: 500 });
    const centroB = await criaCentro(USER_B, "Edulingo");
    await comoUsuario(USER_B, () =>
      allocateTransaction(movimentacaoB, [{ financialCenterId: centroB, amount: 500 }]),
    );

    const centroA = await criaCentro(USER_A, "Edulingo");
    const saldoA = await comoUsuario(USER_A, () => listFinancialCentersWithBalance());
    expect(saldoA.map((c) => c.id)).toEqual([centroA]);
    expect(saldoA[0]?.outflow).toBe(0);
  });

  // ── Migração ──

  it("converte as tags nomeadas em centros e rateia 100% do valor", async () => {
    const produto = await seedProduto(USER_A);
    const tagEdulingo = await seedTag(USER_A, "Edulingo");
    const tagOutra = await seedTag(USER_A, "Urgente");
    const comTag = await seedMovimentacao(USER_A, produto, { amount: 250 });
    const semTag = await seedMovimentacao(USER_A, produto, { amount: 99 });
    await marcaComTag(comTag, tagEdulingo);
    await marcaComTag(semTag, tagOutra);

    const resultado = await comoUsuario(USER_A, () => migrateTagsToCenters());
    expect(resultado.centers.map((c) => c.name)).toEqual(["Edulingo"]);
    expect(resultado.allocationsCreated).toBe(1);

    const resumo = await comoUsuario(USER_A, () => getTransactionAllocations(comTag));
    expect(resumo.allocated).toBe(250);
    expect(resumo.unallocated).toBe(0);
    expect(await comoUsuario(USER_A, () => getTransactionAllocations(semTag))).toMatchObject({
      allocated: 0,
    });
  });

  it("é idempotente: rodar duas vezes não duplica centro nem rateio", async () => {
    const produto = await seedProduto(USER_A);
    const tag = await seedTag(USER_A, "Imob Max");
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 80 });
    await marcaComTag(movimentacao, tag);

    await comoUsuario(USER_A, () => migrateTagsToCenters());
    const segunda = await comoUsuario(USER_A, () => migrateTagsToCenters());

    expect(segunda.allocationsCreated).toBe(0);
    expect(segunda.transactionsSkippedAlreadyAllocated).toBe(1);
    expect(segunda.centers[0]?.created).toBe(false);

    const centros = await comoUsuario(USER_A, () => listFinancialCenters());
    expect(centros.filter((c) => c.name === "Imob Max")).toHaveLength(1);
    expect(
      (await comoUsuario(USER_A, () => getTransactionAllocations(movimentacao))).allocations,
    ).toHaveLength(1);
  });

  it("casa o nome da tag ignorando caixa e espaço", async () => {
    const produto = await seedProduto(USER_A);
    const tag = await seedTag(USER_A, "  seu bolso feliz ");
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 10 });
    await marcaComTag(movimentacao, tag);

    const resultado = await comoUsuario(USER_A, () => migrateTagsToCenters());
    expect(resultado.centers.map((c) => c.name)).toEqual(["Seu Bolso Feliz"]);
  });

  it("divide o valor quando a movimentação tem duas das tags", async () => {
    const produto = await seedProduto(USER_A);
    const tagA = await seedTag(USER_A, "Edulingo");
    const tagB = await seedTag(USER_A, "IA");
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 100 });
    await marcaComTag(movimentacao, tagA);
    await marcaComTag(movimentacao, tagB);

    await comoUsuario(USER_A, () => migrateTagsToCenters());
    const resumo = await comoUsuario(USER_A, () => getTransactionAllocations(movimentacao));
    expect(resumo.allocated).toBe(100);
    expect(resumo.allocations.map((r) => r.amount).sort()).toEqual([50, 50]);
  });

  it("não rateia pagamento de fatura marcado com a tag", async () => {
    const produto = await seedProduto(USER_A);
    const tag = await seedTag(USER_A, "Daiane");
    const fatura = await seedMovimentacao(USER_A, produto, {
      amount: 700,
      type: "statement_payment",
    });
    await marcaComTag(fatura, tag);

    const resultado = await comoUsuario(USER_A, () => migrateTagsToCenters());
    expect(resultado.transactionsSkippedNotAllocatable).toBe(1);
    expect(resultado.allocationsCreated).toBe(0);
  });

  it("não toca em movimentação que já tinha rateio manual", async () => {
    const produto = await seedProduto(USER_A);
    const tag = await seedTag(USER_A, "Edulingo");
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 200 });
    await marcaComTag(movimentacao, tag);
    const outroCentro = await criaCentro(USER_A, "Daiane");

    await comoUsuario(USER_A, async () => {
      await allocateTransaction(movimentacao, [{ financialCenterId: outroCentro, amount: 30 }]);
      const resultado = await migrateTagsToCenters();
      expect(resultado.transactionsSkippedAlreadyAllocated).toBe(1);
      expect(resultado.allocationsCreated).toBe(0);

      const resumo = await getTransactionAllocations(movimentacao);
      expect(resumo.allocations).toHaveLength(1);
      expect(resumo.allocated).toBe(30);
    });
  });

  it("não enxerga tag nem movimentação de outro usuário", async () => {
    const produtoB = await seedProduto(USER_B);
    const tagB = await seedTag(USER_B, "Edulingo");
    const movimentacaoB = await seedMovimentacao(USER_B, produtoB, { amount: 400 });
    await marcaComTag(movimentacaoB, tagB);

    const resultado = await comoUsuario(USER_A, () => migrateTagsToCenters());
    expect(resultado.centers).toEqual([]);
    expect(resultado.allocationsCreated).toBe(0);
  });

  it("mantém as tags depois da migração — tag continua existindo", async () => {
    const produto = await seedProduto(USER_A);
    const tag = await seedTag(USER_A, "Edulingo");
    const movimentacao = await seedMovimentacao(USER_A, produto, { amount: 10 });
    await marcaComTag(movimentacao, tag);

    await comoUsuario(USER_A, () => migrateTagsToCenters());

    const db = getDb();
    const restantes = await db.select().from(schema.tags).where(eq(schema.tags.id, tag));
    expect(restantes).toHaveLength(1);
    const vinculos = await db
      .select()
      .from(schema.transaction_tags)
      .where(eq(schema.transaction_tags.transaction_id, movimentacao));
    expect(vinculos).toHaveLength(1);
  });
});
