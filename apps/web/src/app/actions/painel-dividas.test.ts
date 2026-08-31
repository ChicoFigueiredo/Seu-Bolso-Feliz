import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import { createLiability, createLiabilityInstallment } from "./liabilities";
import {
  carregarDetalheDivida,
  carregarPainelDividas,
  gerarCronogramaAutomatico,
  marcarParcelaComoPaga,
} from "./painel-dividas";
import { hojeIso, somaDias } from "@/lib/datas";

// Mesmo mock de apps/web/src/app/actions/liabilities.test.ts: a identidade
// vem do nosso próprio `requireUser()`, sem provedor externo.
let currentUserId = "";
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

const USER_A = "00000000-0000-0000-0000-0000000000d1";
const USER_B = "00000000-0000-0000-0000-0000000000d2";

// Mesmo relógio que o código sob teste — ver `@/lib/datas`.
function somaDiasIso(dias: number): string {
  return somaDias(hojeIso(), dias);
}

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("painel-dividas", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/liabilities/liability_installments
    }
  });

  describe("carregarPainelDividas", () => {
    it("isola dívidas por usuário e soma só as ativas na dívida total", async () => {
      currentUserId = USER_A;
      const seedA = await seedProductFor(USER_A);
      institutionIds.push(seedA.institutionId);
      await createLiability({
        financial_product_id: seedA.productId,
        name: "Empréstimo ativo",
        type: "personal_loan",
        original_amount: 10000,
        outstanding_balance: 8000,
        status: "active",
      });
      await createLiability({
        financial_product_id: seedA.productId,
        name: "Já quitado",
        type: "personal_loan",
        original_amount: 500,
        outstanding_balance: 0,
        status: "paid_off",
      });

      currentUserId = USER_B;
      const seedB = await seedProductFor(USER_B);
      institutionIds.push(seedB.institutionId);
      await createLiability({
        financial_product_id: seedB.productId,
        name: "Dívida de B",
        type: "personal_loan",
        original_amount: 999,
        outstanding_balance: 999,
        status: "active",
      });

      currentUserId = USER_A;
      const painel = await carregarPainelDividas();
      expect(painel.dividaTotal).toBe(8000);
      expect(painel.ativas.map((d) => d.nome)).toEqual(["Empréstimo ativo"]);
      expect(painel.quitadas.map((d) => d.nome)).toEqual(["Já quitado"]);
      expect(painel.ativas.some((d) => d.nome === "Dívida de B")).toBe(false);
    });

    it("usa a próxima parcela lançada como fato e cai pra estimativa saldo/parcelas restantes quando não há nenhuma", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);

      const comParcela = await createLiability({
        financial_product_id: seed.productId,
        name: "Com parcela lançada",
        type: "personal_loan",
        original_amount: 1200,
        outstanding_balance: 1200,
        total_installments: 12,
        paid_installments: 0,
        status: "active",
      });
      await createLiabilityInstallment({
        liability_id: comParcela.id,
        installment_number: 1,
        due_date: somaDiasIso(10),
        total_amount: 111.11,
      });

      await createLiability({
        financial_product_id: seed.productId,
        name: "Sem parcela lançada",
        type: "personal_loan",
        original_amount: 1000,
        outstanding_balance: 1000,
        total_installments: 10,
        paid_installments: 0,
        status: "active",
      });

      const painel = await carregarPainelDividas();
      const a = painel.ativas.find((d) => d.nome === "Com parcela lançada");
      const b = painel.ativas.find((d) => d.nome === "Sem parcela lançada");
      expect(a).toBeDefined();
      expect(b).toBeDefined();

      expect(a!.valorParcela).toBeCloseTo(111.11, 2);
      expect(a!.valorParcelaEstimado).toBe(false);

      expect(b!.valorParcela).toBeCloseTo(100, 2); // 1000 / 10 parcelas restantes
      expect(b!.valorParcelaEstimado).toBe(true);

      expect(painel.compromissoMensalEstimado).toBe(true);
    });
  });

  describe("carregarDetalheDivida", () => {
    it("devolve null pra dívida de outro usuário", async () => {
      currentUserId = USER_B;
      const seedB = await seedProductFor(USER_B);
      institutionIds.push(seedB.institutionId);
      const b = await createLiability({
        financial_product_id: seedB.productId,
        name: "Dívida B",
        type: "personal_loan",
        original_amount: 100,
        outstanding_balance: 100,
        status: "active",
      });

      currentUserId = USER_A;
      expect(await carregarDetalheDivida(b.id)).toBeNull();
    });

    it("classifica parcelas em paga / atrasada / a_vencer", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Com cronograma",
        type: "installment_plan",
        original_amount: 300,
        outstanding_balance: 300,
        status: "active",
      });
      await createLiabilityInstallment({
        liability_id: divida.id,
        installment_number: 1,
        due_date: somaDiasIso(-30),
        total_amount: 100,
        status: "paid",
        paid_amount: 100,
        paid_date: somaDiasIso(-30),
      });
      await createLiabilityInstallment({
        liability_id: divida.id,
        installment_number: 2,
        due_date: somaDiasIso(-5),
        total_amount: 100,
      });
      await createLiabilityInstallment({
        liability_id: divida.id,
        installment_number: 3,
        due_date: somaDiasIso(20),
        total_amount: 100,
      });

      const detalhe = await carregarDetalheDivida(divida.id);
      expect(detalhe).not.toBeNull();
      expect(detalhe!.parcelas.map((p) => p.situacao)).toEqual(["paga", "atrasada", "a_vencer"]);
    });

    it("monta uma prévia do cronograma quando a dívida tem termos completos mas nenhuma parcela lançada", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Termos completos, sem parcela",
        type: "personal_loan",
        original_amount: 1200,
        outstanding_balance: 1200,
        interest_rate: 0.01,
        rate_type: "monthly",
        amortization_system: "price",
        total_installments: 12,
        start_date: "2026-01-15",
        status: "active",
      });

      const detalhe = await carregarDetalheDivida(divida.id);
      expect(detalhe!.previaCronograma).not.toBeNull();
      expect(detalhe!.previaCronograma!.parcelas).toBe(12);
      expect(detalhe!.previaCronograma!.primeiraParcela).toBeGreaterThan(0);
    });
  });

  describe("gerarCronogramaAutomatico", () => {
    it("gera as parcelas com a matemática de @sbf/domain e recusa gerar de novo", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Financiamento com termos completos",
        type: "personal_loan",
        original_amount: 1200,
        outstanding_balance: 1200,
        interest_rate: 0.01,
        rate_type: "monthly",
        amortization_system: "price",
        total_installments: 12,
        start_date: "2026-01-15",
        status: "active",
      });

      const resultado = await gerarCronogramaAutomatico(divida.id);
      expect(resultado.criadas).toBe(12);

      const db = getDb();
      const parcelas = await db
        .select()
        .from(schema.liability_installments)
        .where(eq(schema.liability_installments.liability_id, divida.id));
      expect(parcelas).toHaveLength(12);
      expect(parcelas.map((p) => p.installment_number).sort((x, y) => x - y)).toEqual(
        Array.from({ length: 12 }, (_, i) => i + 1),
      );

      const primeira = parcelas.find((p) => p.installment_number === 1);
      const ultima = parcelas.find((p) => p.installment_number === 12);
      expect(primeira?.due_date).toBe("2026-01-15");
      expect(ultima?.due_date).toBe("2026-12-15");

      // Price: parcela fixa — todas as parcelas têm o mesmo total.
      const totais = new Set(parcelas.map((p) => Number(p.total_amount)));
      expect(totais.size).toBe(1);

      await expect(gerarCronogramaAutomatico(divida.id)).rejects.toThrow();
    });

    it("recusa gerar sem total de parcelas nem data de início", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Sem termos",
        type: "overdraft",
        original_amount: 500,
        outstanding_balance: 500,
        status: "active",
      });
      await expect(gerarCronogramaAutomatico(divida.id)).rejects.toThrow();
    });

    it("lança erro ao gerar cronograma pra dívida de outro usuário", async () => {
      currentUserId = USER_B;
      const seedB = await seedProductFor(USER_B);
      institutionIds.push(seedB.institutionId);
      const b = await createLiability({
        financial_product_id: seedB.productId,
        name: "Dívida B",
        type: "personal_loan",
        original_amount: 100,
        outstanding_balance: 100,
        total_installments: 2,
        start_date: "2026-01-01",
        status: "active",
      });

      currentUserId = USER_A;
      await expect(gerarCronogramaAutomatico(b.id)).rejects.toThrow();
    });
  });

  describe("marcarParcelaComoPaga", () => {
    it("abate o saldo devedor, soma parcelas pagas, e é idempotente", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Pagando aos poucos",
        type: "personal_loan",
        original_amount: 1000,
        outstanding_balance: 1000,
        total_installments: 10,
        paid_installments: 0,
        status: "active",
      });
      const parcela = await createLiabilityInstallment({
        liability_id: divida.id,
        installment_number: 1,
        due_date: somaDiasIso(5),
        total_amount: 100,
        principal_amount: 90,
        interest_amount: 10,
      });

      await marcarParcelaComoPaga(parcela.id);

      const db = getDb();
      const [dividaAtualizada] = await db
        .select()
        .from(schema.liabilities)
        .where(eq(schema.liabilities.id, divida.id));
      expect(Number(dividaAtualizada?.outstanding_balance)).toBe(910);
      expect(dividaAtualizada?.paid_installments).toBe(1);
      expect(dividaAtualizada?.status).toBe("active");

      const [parcelaAtualizada] = await db
        .select()
        .from(schema.liability_installments)
        .where(eq(schema.liability_installments.id, parcela.id));
      expect(parcelaAtualizada?.status).toBe("paid");
      expect(Number(parcelaAtualizada?.paid_amount)).toBe(100);

      // Idempotente: chamar de novo numa parcela já paga não abate o saldo outra vez.
      await marcarParcelaComoPaga(parcela.id);
      const [dividaSegunda] = await db
        .select()
        .from(schema.liabilities)
        .where(eq(schema.liabilities.id, divida.id));
      expect(Number(dividaSegunda?.outstanding_balance)).toBe(910);
      expect(dividaSegunda?.paid_installments).toBe(1);
    });

    it("marca a dívida como quitada quando a parcela paga zera o saldo", async () => {
      currentUserId = USER_A;
      const seed = await seedProductFor(USER_A);
      institutionIds.push(seed.institutionId);
      const divida = await createLiability({
        financial_product_id: seed.productId,
        name: "Última parcela",
        type: "personal_loan",
        original_amount: 100,
        outstanding_balance: 100,
        total_installments: 1,
        paid_installments: 0,
        status: "active",
      });
      const parcela = await createLiabilityInstallment({
        liability_id: divida.id,
        installment_number: 1,
        due_date: somaDiasIso(1),
        total_amount: 100,
      });

      await marcarParcelaComoPaga(parcela.id);

      const db = getDb();
      const [dividaAtualizada] = await db
        .select()
        .from(schema.liabilities)
        .where(eq(schema.liabilities.id, divida.id));
      expect(Number(dividaAtualizada?.outstanding_balance)).toBe(0);
      expect(dividaAtualizada?.status).toBe("paid_off");
    });

    it("lança erro ao marcar como paga uma parcela de outro usuário", async () => {
      currentUserId = USER_B;
      const seedB = await seedProductFor(USER_B);
      institutionIds.push(seedB.institutionId);
      const dividaB = await createLiability({
        financial_product_id: seedB.productId,
        name: "Dívida B",
        type: "personal_loan",
        original_amount: 100,
        outstanding_balance: 100,
        status: "active",
      });
      const parcelaB = await createLiabilityInstallment({
        liability_id: dividaB.id,
        installment_number: 1,
        due_date: somaDiasIso(1),
        total_amount: 50,
      });

      currentUserId = USER_A;
      await expect(marcarParcelaComoPaga(parcelaB.id)).rejects.toThrow();
    });
  });
});
