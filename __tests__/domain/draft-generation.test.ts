/**
 * Testes unitários: Geração de Drafts
 *
 * 4.8-4.14 — Classificação, geração de drafts, sugestões
 */
import { describe, it, expect } from "vitest";
import {
  classifyDraftTypes,
  buildTransactionDraft,
  buildRecurringTemplateDraft,
  buildConsumptionMetricDraft,
} from "../../workers/ingestion/src/drafts/draft-generator";

// ══════════════════════════════════════════════════════════════
// 4.12 — Classificação de tipos de draft
// ══════════════════════════════════════════════════════════════

describe("4.12: classifyDraftTypes", () => {
  it("retorna 'transaction' para dados null", () => {
    const types = classifyDraftTypes(null);
    expect(types).toEqual(["transaction"]);
  });

  it("retorna 'transaction' para dados com valor e vencimento", () => {
    const types = classifyDraftTypes({ totalAmount: 100, dueDate: "2026-04-15" });
    expect(types).toContain("transaction");
  });

  it("inclui 'consumption_metric' quando há dados de consumo kWh", () => {
    const types = classifyDraftTypes({
      totalAmount: 245.5,
      consumption: { kwh: 320, days: 30 },
    });
    expect(types).toContain("consumption_metric");
    expect(types).toContain("transaction");
  });

  it("inclui 'recurring_template' quando há fornecedor e competência", () => {
    const types = classifyDraftTypes({
      supplierNameRaw: "CEMIG",
      competenceDate: "2026-03-01",
      totalAmount: 245.5,
    });
    expect(types).toContain("recurring_template");
    expect(types).toContain("transaction");
  });

  it("gera 3 tipos para conta de energia completa", () => {
    const types = classifyDraftTypes({
      supplierNameRaw: "CEMIG",
      competenceDate: "2026-03-01",
      totalAmount: 245.5,
      dueDate: "2026-04-15",
      consumption: { kwh: 320, days: 30 },
    });
    expect(types).toContain("transaction");
    expect(types).toContain("consumption_metric");
    expect(types).toContain("recurring_template");
    expect(types).toHaveLength(3);
  });

  it("retorna 'transaction' como fallback para dados sem campos reconhecíveis", () => {
    const types = classifyDraftTypes({ random: true });
    expect(types).toEqual(["transaction"]);
  });

  it("aceita campos com underscore (formato DB)", () => {
    const types = classifyDraftTypes({
      total_amount: 100,
      due_date: "2026-04-15",
      supplier_name_raw: "Vivo",
      competence_date: "2026-03-01",
    });
    expect(types).toContain("transaction");
    expect(types).toContain("recurring_template");
  });
});

// ══════════════════════════════════════════════════════════════
// 4.8 — buildTransactionDraft
// ══════════════════════════════════════════════════════════════

describe("4.8: buildTransactionDraft", () => {
  it("gera draft de transação com todos os campos", () => {
    const draft = buildTransactionDraft({
      supplierNameRaw: "CEMIG",
      totalAmount: 245.5,
      dueDate: "2026-04-15",
      competenceDate: "2026-03-01",
      category_suggestion: "energia_eletrica",
      tags_suggestion: ["essencial", "moradia"],
      documentNumber: "123456",
      contractIdentifier: "12345678",
    });

    expect(draft.type).toBe("despesa");
    expect(draft.amount).toBe(245.5);
    expect(draft.due_date).toBe("2026-04-15");
    expect(draft.competence_date).toBe("2026-03-01");
    expect(draft.category).toBe("energia_eletrica");
    expect(draft.tags).toEqual(["essencial", "moradia"]);
    expect(draft.supplier_name).toBe("CEMIG");
    expect(draft.document_number).toBe("123456");
  });

  it("lida com campos underscore (formato DB)", () => {
    const draft = buildTransactionDraft({
      supplier_name_raw: "Vivo",
      total_amount: 99.9,
      due_date: "2026-04-20",
    });

    expect(draft.amount).toBe(99.9);
    expect(draft.supplier_name).toBe("Vivo");
    expect(draft.due_date).toBe("2026-04-20");
  });

  it("usa fallback para campos null", () => {
    const draft = buildTransactionDraft({});
    expect(draft.type).toBe("despesa");
    expect(draft.description).toBe("Documento importado");
    expect(draft.amount).toBeNull();
    expect(draft.currency).toBe("BRL");
  });
});

// ══════════════════════════════════════════════════════════════
// 4.9 — buildRecurringTemplateDraft
// ══════════════════════════════════════════════════════════════

describe("4.9: buildRecurringTemplateDraft", () => {
  it("gera template recorrente mensal", () => {
    const draft = buildRecurringTemplateDraft({
      supplierNameRaw: "CEMIG",
      totalAmount: 245.5,
      category_suggestion: "energia_eletrica",
    });

    expect(draft.name).toBe("CEMIG - mensal");
    expect(draft.recurrence).toBe("monthly");
    expect(draft.base_amount).toBe(245.5);
    expect(draft.category).toBe("energia_eletrica");
  });

  it("fallback para nome 'Conta' sem fornecedor", () => {
    const draft = buildRecurringTemplateDraft({});
    expect(draft.name).toBe("Conta - mensal");
  });
});

// ══════════════════════════════════════════════════════════════
// 4.10 — buildConsumptionMetricDraft
// ══════════════════════════════════════════════════════════════

describe("4.10: buildConsumptionMetricDraft", () => {
  it("gera métrica de consumo com kWh e dias", () => {
    const draft = buildConsumptionMetricDraft({
      supplierNameRaw: "CEMIG",
      competenceDate: "2026-03-01",
      consumption: { kwh: 320, days: 30 },
      totalAmount: 245.5,
      contractIdentifier: "12345678",
    });

    expect(draft.kwh).toBe(320);
    expect(draft.days).toBe(30);
    expect(draft.amount).toBe(245.5);
    expect(draft.supplier_name).toBe("CEMIG");
    expect(draft.contract_identifier).toBe("12345678");
  });

  it("aceita formato DB (consumption_data)", () => {
    const draft = buildConsumptionMetricDraft({
      consumption_data: { kwh: 200, days: 28 },
      total_amount: 180.0,
      supplier_name_raw: "CEMIG",
    });

    expect(draft.kwh).toBe(200);
    expect(draft.days).toBe(28);
  });

  it("null se sem dados de consumo", () => {
    const draft = buildConsumptionMetricDraft({});
    expect(draft.kwh).toBeNull();
    expect(draft.days).toBeNull();
  });
});
