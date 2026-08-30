/**
 * Testes de integração: findReconciliationCandidates (workers/ingestion/src/reconciliation).
 *
 * Motor usado pelo pipeline de documento inteiro (não só Pluggy) — nunca
 * tinha sido exercitado contra o schema real. As Regras 2 e 3 referenciavam
 * colunas que não existem em `transactions`/`recurring_instances`
 * (transaction_date/supplier_name/category, due_date/template_id), sob casts
 * `as never`/`as unknown as typeof supabase` que escondiam o erro de tipo —
 * a query falhava silenciosamente (erro do Supabase nunca checado) e a Regra
 * 2 (match_exact/match_fuzzy, a regra principal) sempre voltava zero
 * candidatos em produção. Hoje o motor fala Drizzle: um nome de coluna errado
 * não compila, e um erro de banco vira exceção em vez de resultado vazio.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import {
  financial_products,
  institutions,
  recurring_instances,
  recurring_templates,
  suppliers,
  transactions,
  type Database,
} from "@sbf/db";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { getTestDb, limparPorUsuario, semBancoDeTeste } from "../helpers/db";

const USER_ID = "50000000-0000-4000-8000-000000000001";

const TABELAS_DE_CADA_TESTE = ["recurring_instances", "recurring_templates", "transactions"];
const TABELAS_DO_ARQUIVO = [
  ...TABELAS_DE_CADA_TESTE,
  "suppliers",
  "financial_products",
  "institutions",
];

let db: Database;
let financialProductId: string;
let supplierId: string;

describe.skipIf(semBancoDeTeste)("reconciliação", () => {
  beforeAll(async () => {
    db = getTestDb();
    await limparPorUsuario(db, TABELAS_DO_ARQUIVO, USER_ID);

    const [institution] = await db
      .insert(institutions)
      .values({ user_id: USER_ID, name: "Banco de Teste" })
      .returning({ id: institutions.id });

    const [product] = await db
      .insert(financial_products)
      .values({
        user_id: USER_ID,
        institution_id: institution!.id,
        name: "Conta de Teste",
        type: "checking_account",
      })
      .returning({ id: financial_products.id });
    financialProductId = product!.id;

    const [supplier] = await db
      .insert(suppliers)
      .values({ user_id: USER_ID, name: "Fornecedor de Teste" })
      .returning({ id: suppliers.id });
    supplierId = supplier!.id;
  });

  beforeEach(async () => {
    await limparPorUsuario(db, TABELAS_DE_CADA_TESTE, USER_ID);
  });

  afterAll(async () => {
    await limparPorUsuario(db, TABELAS_DO_ARQUIVO, USER_ID);
  });

  describe("Regra 2 — match por supplier_id + amount + due_date", () => {
    it("acha match_exact quando existe transação com mesmo fornecedor, valor e data (±7 dias)", async () => {
      await db.insert(transactions).values({
        user_id: USER_ID,
        financial_product_id: financialProductId,
        supplier_id: supplierId,
        type: "expense",
        amount: 150.0,
        event_date: "2026-08-10",
      });

      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        { supplier_id: supplierId, amount: 150.0, due_date: "2026-08-12" },
        null,
      );

      expect(result.status).toBe("match_exact");
      expect(result.candidates[0]?.score).toBe(0.95);
    });

    it("acha match_fuzzy quando o valor bate mas a data diverge mais de 7 dias", async () => {
      await db.insert(transactions).values({
        user_id: USER_ID,
        financial_product_id: financialProductId,
        supplier_id: supplierId,
        type: "expense",
        amount: 200.0,
        event_date: "2026-08-01",
      });

      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        { supplier_id: supplierId, amount: 200.0, due_date: "2026-08-20" },
        null,
      );

      expect(result.status).toBe("match_fuzzy");
    });
  });

  describe("Regra 3 — match por recorrência", () => {
    it("reconhece instância existente no mês de competência (score 0.85)", async () => {
      const [template] = await db
        .insert(recurring_templates)
        .values({
          user_id: USER_ID,
          supplier_id: supplierId,
          name: "Conta recorrente de teste",
          type: "expense",
          amount: 100.0,
          frequency: "monthly",
        })
        .returning({ id: recurring_templates.id });

      await db.insert(recurring_instances).values({
        user_id: USER_ID,
        recurring_template_id: template!.id,
        expected_date: "2026-08-15",
        expected_amount: 100.0,
        status: "pending",
      });

      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        { supplier_id: supplierId, amount: 100.0, competence_date: "2026-08-15" },
        null,
      );

      const recurrenceCandidate = result.candidates.find((c) => c.matchType === "match_recurrence");
      expect(recurrenceCandidate?.score).toBe(0.85);
      expect(recurrenceCandidate?.description).toContain("instância prevista");
    });
  });
});
