/**
 * Mapper extraction_results → drafts, e a regra de classificação.
 */
import { describe, it, expect } from "vitest";
import {
  classifyDraftTypes,
  buildDraftPayload,
  toConsumptionMetricDraft,
} from "./extraction-to-draft";
import { DRAFT_SCHEMAS, POSTABLE_SCHEMAS, TARGET_TABLES } from "../draft";

describe("classifyDraftTypes", () => {
  it("retorna transaction para extração nula", () => {
    expect(classifyDraftTypes(null)).toEqual(["transaction"]);
  });

  it("retorna transaction quando há valor e vencimento", () => {
    expect(classifyDraftTypes({ totalAmount: 100, dueDate: "2026-04-15" })).toContain(
      "transaction",
    );
  });

  it("inclui consumption_metric quando há kWh", () => {
    const types = classifyDraftTypes({ totalAmount: 245.5, consumption: { kwh: 320, days: 30 } });
    expect(types).toContain("consumption_metric");
    expect(types).toContain("transaction");
  });

  it("aceita campos com underscore (formato DB)", () => {
    expect(
      classifyDraftTypes({ total_amount: 100, due_date: "2026-04-15", supplier_name_raw: "Vivo" }),
    ).toContain("transaction");
  });

  // ── A inversão deliberada ────────────────────────────────────────────────
  // A regra antiga era: hasSupplier && hasCompetence → recurring_template.
  // Toda conta de consumo satisfaz as duas condições, então qualquer conta
  // mensal virava recorrência a partir de UM documento, com "monthly" cravado
  // no código. O §8 do plano mestre proíbe: a saída deve ser um candidato
  // inferido de histórico, nunca um template criado silenciosamente.

  it("não infere recorrência a partir de um único documento (plano mestre §8)", () => {
    const types = classifyDraftTypes({
      supplierNameRaw: "CEMIG",
      competenceDate: "2026-03-01",
      totalAmount: 245.5,
    });
    expect(types).not.toContain("recurring_template");
    expect(types).toEqual(["transaction"]);
  });

  it("gera 2 tipos, não 3, para uma conta de energia completa", () => {
    const types = classifyDraftTypes({
      supplierNameRaw: "CEMIG",
      competenceDate: "2026-03-01",
      totalAmount: 245.5,
      dueDate: "2026-04-15",
      consumption: { kwh: 320, days: 30 },
    });
    expect(types).toEqual(["transaction", "consumption_metric"]);
    expect(types).toHaveLength(2);
  });
});

describe("buildDraftPayload", () => {
  const CEMIG = {
    supplier_name_raw: "CEMIG",
    total_amount: 245.5,
    due_date: "2026-04-15",
    competence_date: "2026-03-01",
    consumption_data: { kwh: 320, days: 30 },
  };

  it("todo tipo classificado produz um payload válido no seu schema", () => {
    for (const type of classifyDraftTypes(CEMIG)) {
      const payload = buildDraftPayload(type, CEMIG);
      expect(DRAFT_SCHEMAS[type].safeParse(payload).success).toBe(true);
    }
  });

  it("carrega a proveniência recebida", () => {
    const payload = buildDraftPayload("transaction", CEMIG, {
      provenance: {
        source_document_id: "22222222-2222-4222-8222-222222222222",
        parser_type: "LOCAL_REGEX",
      },
    });
    expect(payload.provenance.source_document_id).toBe("22222222-2222-4222-8222-222222222222");
    expect(payload.provenance.parser_type).toBe("LOCAL_REGEX");
  });
});

describe("consumption_metric não é transação", () => {
  it("aponta para consumption_metrics, evitando a contagem dupla", () => {
    // O materializador antigo roteava consumption_metric para
    // materializeTransaction. Como uma conta de luz gera transaction E
    // consumption_metric, aprová-la lançava R$245,50 duas vezes.
    expect(TARGET_TABLES.consumption_metric).toBe("consumption_metrics");
    expect(TARGET_TABLES.transaction).toBe("transactions");
    expect(TARGET_TABLES.consumption_metric).not.toBe(TARGET_TABLES.transaction);
  });

  it("exige fornecedor canônico e período completo para lançar", () => {
    const draft = toConsumptionMetricDraft({
      supplier_name_raw: "CEMIG",
      competence_date: "2026-03-01",
      consumption_data: { kwh: 320 },
      total_amount: 245.5,
    });
    const res = POSTABLE_SCHEMAS.consumption_metric.safeParse(draft);
    expect(res.success).toBe(false);
    if (!res.success) {
      const paths = res.error.errors.map((e) => e.path.join("."));
      expect(paths).toContain("supplier_id");
      expect(paths).toContain("reference_period_end");
    }
  });
});

describe("cobertura de tipos", () => {
  it("todo draft_type tem schema, schema de lançamento e tabela alvo", () => {
    for (const type of [
      "transaction",
      "consumption_metric",
      "liability",
      "recurring_template",
    ] as const) {
      expect(DRAFT_SCHEMAS[type]).toBeDefined();
      expect(POSTABLE_SCHEMAS[type]).toBeDefined();
      expect(TARGET_TABLES[type]).toBeTruthy();
    }
  });
});
