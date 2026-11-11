import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createFinancialPeriod,
  deleteFinancialPeriod,
  generateFinancialPeriods,
  getCurrentFinancialPeriod,
  getFinancialPeriodForDate,
  getFinancialPeriods,
  updateFinancialPeriod,
} from "./financial-periods";

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

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-periods actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db.delete(schema.financial_periods).where(eq(schema.financial_periods.id, id));
    }
  });

  it("isola períodos por usuário", async () => {
    currentUserId = USER_A;
    const a = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getFinancialPeriods();
    expect(listA.map((p) => p.id)).toContain(a.id);
    expect(listA.map((p) => p.id)).not.toContain(b.id);
  });

  it("cria, marca como atual, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createFinancialPeriod({
      start_date: "2026-02-01",
      end_date: "2026-02-28",
      is_current: true,
    });
    createdIds.push(created.id);

    const current = await getCurrentFinancialPeriod();
    expect(current?.id).toBe(created.id);

    const updated = await updateFinancialPeriod(created.id, { is_current: false });
    expect(updated.is_current).toBe(false);

    await deleteFinancialPeriod(created.id);
    createdIds.length = 0;
  });

  it("lança erro ao atualizar id de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-03-01",
      end_date: "2026-03-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(updateFinancialPeriod(b.id, { is_current: true })).rejects.toThrow();
  });
});

// ══════════════════════════════════════════════════════════════
// generateFinancialPeriods / getFinancialPeriodForDate
// ══════════════════════════════════════════════════════════════
//
// Eram RPCs (`generate_financial_periods`, `get_financial_period_for_date`)
// que filtravam por `auth.uid()` e não existem no Neon. Usuários próprios
// deste bloco: as duas actions varrem TODOS os períodos do dono (o UPDATE de
// `is_current` inclusive), então compartilhar USER_A/USER_B com os testes
// acima faria um teste mexer nas linhas do outro.

const USER_G1 = "00000000-0000-0000-0000-0000000000f1";
const USER_G2 = "00000000-0000-0000-0000-0000000000f2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-periods — geração e busca", () => {
  afterEach(async () => {
    const db = getDb();
    for (const userId of [USER_G1, USER_G2]) {
      await db.delete(schema.financial_periods).where(eq(schema.financial_periods.user_id, userId));
    }
  });

  it("gera os períodos do ciclo, com rótulo e limites corretos", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(20, 3, "2026-03-25");

    const periodos = await getFinancialPeriods();
    expect(periodos).toHaveLength(3);
    // Âncora 25/03 é DEPOIS do dia 20, então o primeiro ciclo começa em 20/03
    // (e não recua para fevereiro).
    expect(periodos[0]?.start_date).toBe("2026-03-20");
    expect(periodos[0]?.end_date).toBe("2026-04-19");
    expect(periodos[0]?.label).toBe("20/03/2026 a 19/04/2026");
    expect(periodos[1]?.start_date).toBe("2026-04-20");
    expect(periodos[2]?.start_date).toBe("2026-05-20");
  });

  it("recua um mês quando a âncora cai antes do dia de início", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(20, 1, "2026-03-05");

    const [periodo] = await getFinancialPeriods();
    expect(periodo?.start_date).toBe("2026-02-20");
    expect(periodo?.end_date).toBe("2026-03-19");
  });

  it("limita o dia de início ao último dia de meses curtos", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(31, 2, "2026-01-31");

    const periodos = await getFinancialPeriods();
    expect(periodos[0]?.start_date).toBe("2026-01-31");
    // Fevereiro de 2026 tem 28 dias: o ciclo seguinte começa em 28/02, logo o
    // primeiro termina em 27/02.
    expect(periodos[0]?.end_date).toBe("2026-02-27");
    expect(periodos[1]?.start_date).toBe("2026-02-28");
  });

  it("é idempotente: chamar de novo não duplica períodos", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(10, 4, "2026-06-15");
    const primeira = await getFinancialPeriods();

    await generateFinancialPeriods(10, 4, "2026-06-15");
    const segunda = await getFinancialPeriods();

    expect(segunda).toHaveLength(primeira.length);
    expect(segunda.map((p) => p.id).sort()).toEqual(primeira.map((p) => p.id).sort());
  });

  it("marca como atual exatamente o período que contém a data de hoje", async () => {
    currentUserId = USER_G1;
    // Âncora = hoje, 3 períodos: o primeiro necessariamente contém hoje.
    await generateFinancialPeriods(15, 3);

    const periodos = await getFinancialPeriods();
    const atuais = periodos.filter((p) => p.is_current);
    expect(atuais).toHaveLength(1);

    const hoje = new Date();
    const hojeIso = `${hoje.getFullYear()}-${String(hoje.getMonth() + 1).padStart(2, "0")}-${String(
      hoje.getDate(),
    ).padStart(2, "0")}`;
    expect(atuais[0]!.start_date <= hojeIso).toBe(true);
    expect(atuais[0]!.end_date >= hojeIso).toBe(true);
  });

  it("gera só para o dono: o outro usuário continua sem períodos", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(5, 2, "2026-09-10");

    currentUserId = USER_G2;
    expect(await getFinancialPeriods()).toHaveLength(0);
  });

  it("rejeita dia de início fora de 1..31 e meses não positivos", async () => {
    currentUserId = USER_G1;
    await expect(generateFinancialPeriods(0, 3)).rejects.toThrow();
    await expect(generateFinancialPeriods(32, 3)).rejects.toThrow();
    await expect(generateFinancialPeriods(10, 0)).rejects.toThrow();
    expect(await getFinancialPeriods()).toHaveLength(0);
  });

  it("getFinancialPeriodForDate acha o período que contém a data e null fora dele", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(20, 2, "2026-03-25");

    const dentro = await getFinancialPeriodForDate("2026-04-01");
    expect(dentro?.start_date).toBe("2026-03-20");
    expect(dentro?.end_date).toBe("2026-04-19");

    expect(await getFinancialPeriodForDate("2020-01-01")).toBeNull();
  });

  it("getFinancialPeriodForDate não enxerga período de outro usuário", async () => {
    currentUserId = USER_G1;
    await generateFinancialPeriods(20, 2, "2026-03-25");
    expect(await getFinancialPeriodForDate("2026-04-01")).not.toBeNull();

    currentUserId = USER_G2;
    expect(await getFinancialPeriodForDate("2026-04-01")).toBeNull();
  });
});
