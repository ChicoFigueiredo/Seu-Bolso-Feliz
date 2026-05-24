import { describe, expect, it } from "vitest";
import {
  DEFAULT_CONSENSUS_FIELDS,
  resolveFieldConsensus,
  toFieldCandidates,
} from "../../workers/ingestion/src/parsers/field-consensus";

describe("field-consensus", () => {
  it("prioriza boleto_utils sobre parser determinístico para campos críticos", () => {
    const candidates = [
      ...toFieldCandidates("deterministic_parser", 0.75, {
        total_amount: 103.21,
        due_date: "2026-05-15",
      }),
      ...toFieldCandidates("boleto_utils", 0.99, {
        total_amount: 99.9,
        due_date: "2026-05-20",
      }),
    ];

    const result = resolveFieldConsensus(candidates);

    expect(result.values.total_amount).toBe(99.9);
    expect(result.values.due_date).toBe("2026-05-20");
    expect(result.sourcePerField.total_amount).toBe("boleto_utils");
  });

  it("prioriza template sobre parser quando boleto_utils não existe", () => {
    const candidates = [
      ...toFieldCandidates("deterministic_parser", 0.8, {
        supplier_name_raw: "Vivo",
      }),
      ...toFieldCandidates("supplier_template", 0.87, {
        supplier_name_raw: "VIVO S.A.",
      }),
    ];

    const result = resolveFieldConsensus(candidates);

    expect(result.values.supplier_name_raw).toBe("VIVO S.A.");
    expect(result.sourcePerField.supplier_name_raw).toBe("supplier_template");
  });

  it("usa confiança para desempate entre mesma fonte", () => {
    const result = resolveFieldConsensus([
      {
        field: "document_number",
        value: "A-1",
        confidence: 0.6,
        source: "deterministic_parser",
      },
      {
        field: "document_number",
        value: "A-2",
        confidence: 0.9,
        source: "deterministic_parser",
      },
    ]);

    expect(result.values.document_number).toBe("A-2");
  });

  it("ignora candidatos sem valor significativo", () => {
    const result = resolveFieldConsensus([
      {
        field: "supplier_name_raw",
        value: "   ",
        confidence: 0.99,
        source: "boleto_utils",
      },
    ]);

    expect(result.values.supplier_name_raw).toBeUndefined();
    expect(result.overallConfidence).toBe(0);
  });

  it("calcula overallConfidence pela média dos campos selecionados", () => {
    const result = resolveFieldConsensus([
      ...toFieldCandidates("supplier_template", 0.8, {
        supplier_name_raw: "CEMIG DISTRIBUICAO S.A.",
      }),
      ...toFieldCandidates("deterministic_parser", 0.6, {
        total_amount: 240.55,
      }),
      ...toFieldCandidates("boleto_utils", 1, {
        due_date: "2026-04-20",
      }),
    ]);

    // (0.8 + 0.6 + 1.0) / 3 = 0.8
    expect(result.overallConfidence).toBe(0.8);
  });

  it("respeita lista customizada de campos", () => {
    const result = resolveFieldConsensus(
      [
        ...toFieldCandidates("deterministic_parser", 0.7, {
          total_amount: 10,
          due_date: "2026-01-10",
        }),
      ],
      ["total_amount"],
    );

    expect(result.values.total_amount).toBe(10);
    expect(result.values.due_date).toBeUndefined();
  });

  it("expõe decisões e contenders para auditoria", () => {
    const result = resolveFieldConsensus([
      ...toFieldCandidates("deterministic_parser", 0.65, {
        total_amount: 120,
      }),
      ...toFieldCandidates("supplier_template", 0.8, {
        total_amount: 100,
      }),
      ...toFieldCandidates("boleto_utils", 0.99, {
        total_amount: 95,
      }),
    ]);

    const decision = result.decisions.total_amount;
    expect(decision).toBeTruthy();
    expect(decision?.contenders.length).toBe(3);
    expect(decision?.source).toBe("boleto_utils");
  });

  it("exporta campos padrão para cobertura dos críticos", () => {
    expect(DEFAULT_CONSENSUS_FIELDS.includes("total_amount")).toBe(true);
    expect(DEFAULT_CONSENSUS_FIELDS.includes("due_date")).toBe(true);
    expect(DEFAULT_CONSENSUS_FIELDS.includes("supplier_name_raw")).toBe(true);
  });
});
