import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver recurring.test.ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import { createRecurringTemplate } from "./recurring";
import {
  carregarPainelRecorrentes,
  equivalenteMensal,
  gerarInstanciasDoPeriodo,
} from "./painel-recorrentes";
import { paraIso } from "@/lib/datas";

let currentUserId = "";
// Mesmo mock de recurring.test.ts: `requireUser()` é a única porta de
// identidade desde a saída do Supabase.
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

const USER_A = "00000000-0000-0000-0000-0000000000c1";
const USER_B = "00000000-0000-0000-0000-0000000000c2";

function required<T>(value: T | undefined, message: string): T {
  if (value === undefined) throw new Error(message);
  return value;
}

async function seedCategory(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.categories)
    .values({ user_id: userId, name: `Categoria de teste ${crypto.randomUUID()}` })
    .returning();
  return required(row, "Falha ao semear category de teste");
}

async function seedSupplier(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.suppliers)
    .values({ user_id: userId, name: `Fornecedor de teste ${crypto.randomUUID()}` })
    .returning();
  return required(row, "Falha ao semear supplier de teste");
}

async function seedTransaction(userId: string) {
  const db = getDb();
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  const [row] = await db
    .insert(schema.transactions)
    .values({
      user_id: userId,
      financial_product_id: product.id,
      type: "expense",
      event_date: "2026-01-01",
      amount: 100,
    })
    .returning();
  return {
    institutionId: institution.id,
    transaction: required(row, "Falha ao semear transaction de teste"),
  };
}

async function seedRecurringInstance(
  userId: string,
  templateId: string,
  overrides: Partial<typeof schema.recurring_instances.$inferInsert> = {},
) {
  const db = getDb();
  const [row] = await db
    .insert(schema.recurring_instances)
    .values({
      user_id: userId,
      recurring_template_id: templateId,
      expected_date: "2026-01-01",
      expected_amount: 100,
      status: "pending",
      ...overrides,
    })
    .returning();
  return required(row, "Falha ao semear recurring_instance de teste");
}

// Mesmo relógio que o código sob teste: em UTC−3, depois das 21h o UTC já
// virou o dia seguinte, e 'amanhã' cairia fora do período consultado.
const iso = paraIso;

// ─── equivalenteMensal: função pura, roda sem banco ──────────────────────
describe("equivalenteMensal", () => {
  it("mensal: passa o valor direto", async () => {
    expect(await equivalenteMensal(100, "monthly", null)).toBe(100);
  });

  it("semanal: 52 semanas / 12 meses", async () => {
    expect(await equivalenteMensal(100, "weekly", null)).toBeCloseTo((100 * 52) / 12, 6);
  });

  it("quinzenal: 26 quinzenas / 12 meses", async () => {
    expect(await equivalenteMensal(100, "biweekly", null)).toBeCloseTo((100 * 26) / 12, 6);
  });

  it("trimestral: divide por 3", async () => {
    expect(await equivalenteMensal(300, "quarterly", null)).toBeCloseTo(100, 6);
  });

  it("anual: divide por 12", async () => {
    expect(await equivalenteMensal(1200, "annual", null)).toBeCloseTo(100, 6);
  });

  it("personalizada com intervalo: normaliza pela média de dias por mês", async () => {
    const resultado = await equivalenteMensal(100, "custom", 10);
    expect(resultado).toBeCloseTo((100 * 30.4368) / 10, 4);
  });

  it("personalizada sem intervalo cadastrado: não inventa um número", async () => {
    expect(await equivalenteMensal(100, "custom", null)).toBeNull();
  });

  it("personalizada com intervalo inválido (zero ou negativo): não inventa um número", async () => {
    expect(await equivalenteMensal(100, "custom", 0)).toBeNull();
    expect(await equivalenteMensal(100, "custom", -5)).toBeNull();
  });
});

// ─── carregarPainelRecorrentes / gerarInstanciasDoPeriodo: banco real ────
describe.skipIf(!process.env.TEST_DATABASE_URL)("painel-recorrentes (integração)", () => {
  const templateIds: string[] = [];
  const institutionIds: string[] = [];
  const categoryIds: string[] = [];
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of templateIds.splice(0)) {
      await db.delete(schema.recurring_templates).where(eq(schema.recurring_templates.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/transactions
    }
    for (const id of categoryIds.splice(0)) {
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
  });

  it("isola o painel por usuário", async () => {
    currentUserId = USER_A;
    const a = await createRecurringTemplate({
      name: "Aluguel A",
      type: "expense",
      frequency: "monthly",
      amount: 1000,
      is_active: true,
    });
    templateIds.push(a.id);

    currentUserId = USER_B;
    const b = await createRecurringTemplate({
      name: "Aluguel B",
      type: "expense",
      frequency: "monthly",
      amount: 2000,
      is_active: true,
    });
    templateIds.push(b.id);

    currentUserId = USER_A;
    const painel = await carregarPainelRecorrentes();
    const nomes = painel.recorrentes.map((r) => r.nome);
    expect(nomes).toContain("Aluguel A");
    expect(nomes).not.toContain("Aluguel B");
  });

  it("total mensal comprometido soma só ativos, exclui receita, e resolve nome de categoria/fornecedor", async () => {
    currentUserId = USER_A;
    const categoria = await seedCategory(USER_A);
    categoryIds.push(categoria.id);
    const fornecedor = await seedSupplier(USER_A);
    supplierIds.push(fornecedor.id);

    const despesaAtiva = await createRecurringTemplate({
      name: "Internet",
      type: "expense",
      frequency: "monthly",
      amount: 100,
      is_active: true,
      category_id: categoria.id,
      supplier_id: fornecedor.id,
    });
    templateIds.push(despesaAtiva.id);

    const receitaAtiva = await createRecurringTemplate({
      name: "Salário",
      type: "income",
      frequency: "monthly",
      amount: 5000,
      is_active: true,
    });
    templateIds.push(receitaAtiva.id);

    const despesaPausada = await createRecurringTemplate({
      name: "Academia (pausada)",
      type: "expense",
      frequency: "monthly",
      amount: 200,
      is_active: false,
    });
    templateIds.push(despesaPausada.id);

    const painel = await carregarPainelRecorrentes();

    // Só os R$100 da internet — nem o salário (receita), nem a academia
    // pausada (não compromete o mês enquanto pausada).
    expect(painel.totalMensalComprometido).toBeCloseTo(100, 6);
    expect(painel.totalMensalEstimado).toBe(false);

    const internet = painel.recorrentes.find((r) => r.id === despesaAtiva.id);
    expect(internet?.categoria).toBe(categoria.name);
    expect(internet?.fornecedor).toBe(fornecedor.name);
  });

  it("valor variável e valor ausente marcam o total como estimativa", async () => {
    currentUserId = USER_A;
    const variavel = await createRecurringTemplate({
      name: "Energia (CEMIG)",
      type: "expense",
      frequency: "monthly",
      amount: 80,
      is_variable_amount: true,
      is_active: true,
    });
    templateIds.push(variavel.id);

    const semValor = await createRecurringTemplate({
      name: "Sem valor ainda",
      type: "expense",
      frequency: "monthly",
      amount: null,
      is_active: true,
    });
    templateIds.push(semValor.id);

    const painel = await carregarPainelRecorrentes();

    expect(painel.totalMensalEstimado).toBe(true);
    // A variável entra pelo valor de referência (80); a sem-valor contribui
    // zero — não inventa um número que ela não tem.
    expect(painel.totalMensalComprometido).toBeCloseTo(80, 6);
  });

  it("instância conciliada (com transaction_id) nunca vira a 'próxima instância', e aparece marcada no período", async () => {
    currentUserId = USER_A;
    const template = await createRecurringTemplate({
      name: "Streaming",
      type: "expense",
      frequency: "monthly",
      amount: 50,
      is_active: true,
    });
    templateIds.push(template.id);

    const { institutionId, transaction } = await seedTransaction(USER_A);
    institutionIds.push(institutionId);

    const hoje = new Date();
    const amanha = new Date(hoje.getTime() + 86400000);

    // Mais antiga, mas já virou uma transação de verdade — não pode ser "a
    // próxima", senão o compromisso conta duas vezes (aqui e na transação).
    const conciliada = await seedRecurringInstance(USER_A, template.id, {
      expected_date: iso(hoje),
      transaction_id: transaction.id,
    });
    // Mais recente, ainda sem transação — esta é a próxima de verdade.
    const pendente = await seedRecurringInstance(USER_A, template.id, {
      expected_date: iso(amanha),
    });

    const painel = await carregarPainelRecorrentes();
    const resumo = painel.recorrentes.find((r) => r.id === template.id);

    expect(resumo?.proximaInstancia?.id).toBe(pendente.id);
    expect(resumo?.proximaInstancia?.id).not.toBe(conciliada.id);

    const instanciaConciliada = painel.instancias.find((i) => i.id === conciliada.id);
    expect(instanciaConciliada?.conciliada).toBe(true);
    const instanciaPendente = painel.instancias.find((i) => i.id === pendente.id);
    expect(instanciaPendente?.conciliada).toBe(false);
  });

  it("gerarInstanciasDoPeriodo é idempotente: rodar de novo sem nada novo não duplica", async () => {
    currentUserId = USER_A;
    const template = await createRecurringTemplate({
      name: "Assinatura mensal",
      type: "expense",
      frequency: "monthly",
      amount: 30,
      is_active: true,
    });
    templateIds.push(template.id);

    const primeira = await gerarInstanciasDoPeriodo();
    expect(primeira.falhas).toEqual([]);
    expect(primeira.criadas).toBeGreaterThanOrEqual(1);

    const segunda = await gerarInstanciasDoPeriodo();
    // Sem alterar o template nem avançar o relógio, não há mais nada a gerar
    // dentro do mesmo horizonte — clicar de novo não duplica instância.
    expect(segunda.criadas).toBe(0);
    expect(segunda.falhas).toEqual([]);
  });
});
