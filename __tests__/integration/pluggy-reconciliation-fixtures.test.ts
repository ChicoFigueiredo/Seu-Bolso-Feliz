/**
 * Testes de integração: critérios de aceite de reconciliação (§37,
 * docs/prompts/2026-08-24-prompt-integracao-pluggy.md).
 *
 * Semeia o Postgres de teste com o dataset sintético de
 * __tests__/fixtures/pluggy-reconciliation/ (100 transações + 40 emails +
 * 20 PDFs — 15 recorrência/10 parcelamento/10 conflito/10 ambíguo cruzando
 * o pool) e roda findReconciliationCandidates de verdade contra cada
 * critério: correspondência exata, aproximada, não-correspondência,
 * ambiguidade, prevenção de falso positivo e preservação de evidência.
 *
 * "Ambiguidade" não é um status nativo do motor (decisão registrada:
 * estender reconciliation.ts pra isso está fora do raio de ação da Fase 4
 * Pluggy) — aqui ela é detectada no nível do teste via
 * `candidates.length > 1` com scores próximos.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { sql } from "drizzle-orm";
import {
  draft_records,
  financial_products,
  institutions,
  recurring_instances,
  recurring_templates,
  source_documents,
  suppliers,
  transactions,
  type Database,
} from "@sbf/db";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { computeContentHash } from "@sbf/operations";
import { getTestDb, limparPorUsuario, linha, linhas, semBancoDeTeste } from "../helpers/db";
import {
  SYNTHETIC_TRANSACTIONS,
  RECURRENCE_TRANSACTIONS,
  INSTALLMENT_TRANSACTIONS,
  CONFLICT_TRANSACTIONS,
  AMBIGUOUS_TRANSACTIONS,
  type SyntheticTransaction,
} from "../fixtures/pluggy-reconciliation/transactions";
import {
  PRECISE_EMAILS,
  OFFSET_DATE_EMAILS,
  NO_DUE_DATE_EMAILS,
  UNKNOWN_EMAILS,
} from "../fixtures/pluggy-reconciliation/emails";
import {
  UNIQUE_DOCUMENTS,
  DUPLICATE_DOCUMENTS,
  documentBytes,
} from "../fixtures/pluggy-reconciliation/documents";

const USER_ID = "60000000-0000-4000-8000-000000000001";

/** Filhas antes das pais. */
const TABELAS_DO_DATASET = [
  "draft_records",
  "source_documents",
  "recurring_instances",
  "recurring_templates",
  "transactions",
  "suppliers",
];

let db: Database;
let financialProductId: string;
const supplierIdByName = new Map<string, string>();

function amountReaisFromCents(cents: number): number {
  return cents / 100;
}

/** Todos os nomes de fornecedor citados nas três origens do dataset. */
function collectSupplierNames(): string[] {
  const names = new Set<string>();
  for (const t of SYNTHETIC_TRANSACTIONS) names.add(t.supplierName);
  for (const e of [...PRECISE_EMAILS, ...OFFSET_DATE_EMAILS, ...NO_DUE_DATE_EMAILS]) {
    names.add(e.supplierNameRaw);
  }
  for (const d of UNIQUE_DOCUMENTS) names.add(d.supplierName);
  for (const d of DUPLICATE_DOCUMENTS) names.add(d.supplierName);
  // UNKNOWN_EMAILS é deliberadamente excluído: o objetivo é não existir.
  return [...names];
}

async function seedSuppliers() {
  const names = collectSupplierNames();
  const inseridos = await db
    .insert(suppliers)
    .values(names.map((name) => ({ user_id: USER_ID, name })))
    .returning({ id: suppliers.id, name: suppliers.name });
  for (const row of inseridos) supplierIdByName.set(row.name, row.id);
}

async function seedTransactionsLedger() {
  await db.insert(transactions).values(
    SYNTHETIC_TRANSACTIONS.map((t) => ({
      user_id: USER_ID,
      financial_product_id: financialProductId,
      supplier_id: supplierIdByName.get(t.supplierName) ?? null,
      type: "expense",
      amount: amountReaisFromCents(t.amountCents),
      event_date: t.eventDate,
    })),
  );
}

/** Um recurring_template por grupo de recorrência + uma instance por mês. */
async function seedRecurringTemplatesAndInstances() {
  const groups = new Map<string, SyntheticTransaction[]>();
  for (const t of RECURRENCE_TRANSACTIONS) {
    const list = groups.get(t.scenarioGroup!) ?? [];
    list.push(t);
    groups.set(t.scenarioGroup!, list);
  }

  for (const [supplierName, txns] of groups) {
    const [template] = await db
      .insert(recurring_templates)
      .values({
        user_id: USER_ID,
        supplier_id: supplierIdByName.get(supplierName),
        name: `Recorrência sintética — ${supplierName}`,
        type: "expense",
        amount: amountReaisFromCents(txns[0]!.amountCents),
        frequency: "monthly",
      })
      .returning({ id: recurring_templates.id });

    await db.insert(recurring_instances).values(
      txns.map((t) => ({
        user_id: USER_ID,
        recurring_template_id: template!.id,
        expected_date: t.eventDate,
        expected_amount: amountReaisFromCents(t.amountCents),
        status: "pending",
      })),
    );
  }
}

/** source_documents pros 20 PDFs — os pares duplicados ganham um draft aprovado no primeiro. */
async function seedDocuments() {
  for (const doc of UNIQUE_DOCUMENTS) {
    await db.insert(source_documents).values({
      user_id: USER_ID,
      origin_type: "local_file",
      origin_key: doc.filename,
      filename: doc.filename,
      mime_type: "application/pdf",
      status: "active",
      supplier_id: supplierIdByName.get(doc.supplierName) ?? null,
      supplier_name_raw: doc.supplierName,
      content_hash: await computeContentHash(documentBytes(doc)),
    });
  }

  for (const doc of DUPLICATE_DOCUMENTS) {
    const [inserted] = await db
      .insert(source_documents)
      .values({
        user_id: USER_ID,
        origin_type: "local_file",
        origin_key: doc.filename,
        filename: doc.filename,
        mime_type: "application/pdf",
        status: "active",
        supplier_id: supplierIdByName.get(doc.supplierName) ?? null,
        supplier_name_raw: doc.supplierName,
        content_hash: await computeContentHash(documentBytes(doc)),
      })
      .returning({ id: source_documents.id });

    // Só a primeira cópia de cada par ganha draft aprovado — é o documento
    // "processado antes" que a segunda cópia deveria reconhecer como duplicata.
    if (doc.label.endsWith("copia-0")) {
      await db.insert(draft_records).values({
        user_id: USER_ID,
        source_document_id: inserted!.id,
        draft_type: "transaction",
        status: "approved",
        draft_data: {
          amount: amountReaisFromCents(doc.amountCents),
          due_date: doc.dueDate,
          supplier_name: doc.supplierName,
        },
        confidence_score: 1.0,
      });
    }
  }
}

describe.skipIf(semBancoDeTeste)("fixtures de reconciliação (Pluggy §37)", () => {
  beforeAll(async () => {
    db = getTestDb();
    await limparPorUsuario(
      db,
      [...TABELAS_DO_DATASET, "financial_products", "institutions"],
      USER_ID,
    );

    const [institution] = await db
      .insert(institutions)
      .values({ user_id: USER_ID, name: "Banco Fixture Pluggy" })
      .returning({ id: institutions.id });

    const [product] = await db
      .insert(financial_products)
      .values({
        user_id: USER_ID,
        institution_id: institution!.id,
        name: "Conta Fixture Pluggy",
        type: "checking_account",
      })
      .returning({ id: financial_products.id });
    financialProductId = product!.id;

    // Semeado uma vez só — os 52 testes abaixo só LEEM esse dataset via
    // findReconciliationCandidates (nenhum escreve nele), então recriar tudo
    // antes de cada teste era puro desperdício.
    await seedSuppliers();
    await seedTransactionsLedger();
    await seedRecurringTemplatesAndInstances();
    await seedDocuments();
  }, 30_000);

  afterAll(async () => {
    await limparPorUsuario(
      db,
      [...TABELAS_DO_DATASET, "financial_products", "institutions"],
      USER_ID,
    );
  });

  describe("correspondências exatas", () => {
    it.each(PRECISE_EMAILS)("$label: fornecedor+valor+data exatos → match_exact", async (email) => {
      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        {
          supplier_id: supplierIdByName.get(email.supplierNameRaw),
          amount: amountReaisFromCents(email.amountCents!),
          due_date: email.dueDate,
        },
        null,
      );

      expect(result.status).toBe("match_exact");
      expect(result.candidates[0]?.score).toBe(0.95);
    });
  });

  describe("correspondências aproximadas", () => {
    it.each(OFFSET_DATE_EMAILS)(
      "$label: valor certo, data fora da janela de 7 dias → match_fuzzy",
      async (email) => {
        const result = await findReconciliationCandidates(
          db,
          USER_ID,
          {
            supplier_id: supplierIdByName.get(email.supplierNameRaw),
            amount: amountReaisFromCents(email.amountCents!),
            due_date: email.dueDate,
          },
          null,
        );

        expect(result.status).toBe("match_fuzzy");
      },
    );
  });

  describe("não correspondência quando o score é insuficiente", () => {
    it.each(UNKNOWN_EMAILS)(
      "$label: fornecedor/valor sem correspondência no extrato → no_match",
      async (email) => {
        const result = await findReconciliationCandidates(
          db,
          USER_ID,
          {
            supplier_id: supplierIdByName.get(email.supplierNameRaw) ?? null,
            amount: amountReaisFromCents(email.amountCents!),
            due_date: email.dueDate,
          },
          null,
        );

        expect(result.status).toBe("no_match");
        expect(result.candidates).toHaveLength(0);
      },
    );

    it.each(NO_DUE_DATE_EMAILS)(
      "$label: fornecedor conhecido mas sem due_date → no_match (regra não roda sem data)",
      async (email) => {
        const result = await findReconciliationCandidates(
          db,
          USER_ID,
          {
            supplier_id: supplierIdByName.get(email.supplierNameRaw),
            amount: amountReaisFromCents(email.amountCents!),
            due_date: email.dueDate,
          },
          null,
        );

        expect(result.status).toBe("no_match");
      },
    );
  });

  describe("detecção de ambiguidade", () => {
    it.each(
      [...new Set(AMBIGUOUS_TRANSACTIONS.map((t) => t.scenarioGroup!))].map((group) => ({
        group,
        transactions: AMBIGUOUS_TRANSACTIONS.filter((t) => t.scenarioGroup === group),
      })),
    )(
      "$group: duas transações do mesmo fornecedor, valor e data próximos → múltiplos candidatos",
      async ({ transactions: doGrupo }) => {
        const first = doGrupo[0]!;
        const result = await findReconciliationCandidates(
          db,
          USER_ID,
          {
            supplier_id: supplierIdByName.get(first.supplierName),
            amount: amountReaisFromCents(first.amountCents),
            due_date: first.eventDate,
          },
          null,
        );

        // Ambíguo = mais de um candidato plausível com score próximo — o
        // motor não escolhe sozinho, decisão explícita de ficar pro humano.
        expect(result.candidates.length).toBeGreaterThan(1);
        const scores = result.candidates.map((c) => c.score);
        expect(Math.max(...scores) - Math.min(...scores)).toBeLessThanOrEqual(0.25);
      },
    );
  });

  describe("prevenção de falso positivo", () => {
    it.each(CONFLICT_TRANSACTIONS.filter((_, idx) => idx % 2 === 0))(
      "$label: mesmo valor/data que $scenarioGroup, fornecedor diferente → nunca vira candidato",
      async (conflictA) => {
        const conflictB = CONFLICT_TRANSACTIONS.find(
          (t) => t.scenarioGroup === conflictA.scenarioGroup && t.label !== conflictA.label,
        )!;

        const result = await findReconciliationCandidates(
          db,
          USER_ID,
          {
            supplier_id: supplierIdByName.get(conflictA.supplierName),
            amount: amountReaisFromCents(conflictA.amountCents),
            due_date: conflictA.eventDate,
          },
          null,
        );

        // O candidato do fornecedor B (mesmo valor/data) não pode aparecer —
        // supplier_id é o filtro que impede o falso positivo.
        expect(
          result.candidates.some((c) => c.candidateData.supplierName === conflictB.supplierName),
        ).toBe(false);
      },
    );

    it("parcelas da mesma compra: a parcela do dia certo vem como match_exact, as outras só como fuzzy (nunca no mesmo nível de confiança)", async () => {
      const group = INSTALLMENT_TRANSACTIONS.filter(
        (t) => t.scenarioGroup === INSTALLMENT_TRANSACTIONS[0]!.scenarioGroup,
      );
      const target = group[0]!;

      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        {
          supplier_id: supplierIdByName.get(target.supplierName),
          amount: amountReaisFromCents(target.amountCents),
          due_date: target.eventDate,
        },
        null,
      );

      expect(result.status).toBe("match_exact");
      const exactMatches = result.candidates.filter((c) => c.matchType === "match_exact");
      const fuzzyMatches = result.candidates.filter((c) => c.matchType === "match_fuzzy");
      expect(exactMatches).toHaveLength(1);
      expect(fuzzyMatches.length).toBeGreaterThan(0);
    });
  });

  describe("preservação de evidências", () => {
    it("documento duplicado (mesmo content_hash) → match_duplicate, e as duas evidências continuam existindo", async () => {
      const [first, second] = DUPLICATE_DOCUMENTS;
      expect(first!.duplicateGroup).toBe(second!.duplicateGroup);

      const secondDoc = await linha<{ id: string }>(
        db,
        sql`select id from public.source_documents
             where user_id = ${USER_ID}::uuid and origin_key = ${second!.filename}`,
      );
      expect(secondDoc).toBeDefined();

      const result = await findReconciliationCandidates(
        db,
        USER_ID,
        {
          amount: amountReaisFromCents(second!.amountCents),
          due_date: second!.dueDate,
        },
        secondDoc!.id,
      );

      expect(result.status).toBe("match_duplicate");
      expect(result.candidates[0]?.score).toBe(1.0);

      // Nenhuma das duas entradas foi descartada — a detecção de duplicata
      // não apaga evidência, só evita que a SEGUNDA vire uma obrigação nova.
      const bothDocs = await linhas(
        db,
        sql`select id from public.source_documents
             where user_id = ${USER_ID}::uuid
               and origin_key in (${first!.filename}, ${second!.filename})`,
      );
      expect(bothDocs).toHaveLength(2);
    });
  });
});
