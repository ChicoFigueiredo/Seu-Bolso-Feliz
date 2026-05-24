import { describe, expect, it } from "vitest";
import {
  computeCriticalCoverage,
  shouldActivateAiLite,
  shouldEscalateToAiFull,
} from "../../workers/ingestion/src/parsers/ai-lite-enricher";

// Campos completos para reuso nos testes
const fullFields = {
  total_amount: 123.45,
  due_date: "2026-05-20",
  supplier_name_raw: "Vivo Participações S.A.",
  competence_date: "2026-05-01",
  document_number: "INV-123",
  supplier_cnpj: "12345678000199",
  barcode_digitable_line: "10493.38128 60009.000023 00016.901019 3 93780000012345",
};

describe("AI Lite enricher heuristics", () => {
  it("ativa IA lite quando confiança está abaixo do limiar de 0.95", () => {
    // Com threshold em 0.95, qualquer confiança < 0.95 ativa IA lite
    const should = shouldActivateAiLite(0.8, {
      ...fullFields,
    });

    expect(should).toBe(true);
  });

  it("ativa IA lite com confiança 0.94 (abaixo do novo limiar)", () => {
    const should = shouldActivateAiLite(0.94, { ...fullFields });
    expect(should).toBe(true);
  });

  it("NÃO ativa IA lite apenas se confiança for >= 0.95 E todos os campos ok", () => {
    // Confiança exatamente no limiar com todos os campos completos
    const should = shouldActivateAiLite(0.95, { ...fullFields });
    // Todos os campos críticos presentes e válidos → pode pular IA
    expect(should).toBe(false);
  });

  it("ativa IA lite com confiança alta quando fornecedor é suspeito", () => {
    const should = shouldActivateAiLite(0.95, {
      ...fullFields,
      supplier_name_raw: "1234", // muito curto, sem letras suficientes
    });

    expect(should).toBe(true);
  });

  it("ativa IA lite quando campo crítico está ausente, independente da confiança", () => {
    const should = shouldActivateAiLite(1.0, {
      ...fullFields,
      due_date: null, // campo crítico ausente
    });

    expect(should).toBe(true);
  });

  it("calcula cobertura crítica corretamente com 2 de 3 campos presentes", () => {
    const coverage = computeCriticalCoverage({
      ...fullFields,
      due_date: null,
    });

    expect(coverage).toBeCloseTo(2 / 3, 5);
  });

  it("calcula cobertura crítica 0 quando todos os campos críticos ausentes", () => {
    const coverage = computeCriticalCoverage({
      ...fullFields,
      total_amount: null,
      due_date: null,
      supplier_name_raw: null,
    });

    expect(coverage).toBe(0);
  });

  it("escala para IA full quando confiança geral está abaixo de 0.95", () => {
    const should = shouldEscalateToAiFull(
      {
        total_amount: 99.9,
        due_date: "2026-06-10",
        competence_date: null,
        supplier_name_raw: "Cemig Distribuição S.A.",
        supplier_cnpj: null,
        document_number: null,
        barcode_digitable_line: null,
        document_type: "conta_energia",
        financial_intent: "recurring_expense",
        description: null,
      },
      { total_amount: 0.9, due_date: 0.9, supplier_name_raw: 0.9 },
      0.88, // abaixo de 0.95
    );

    expect(should).toBe(true);
  });

  it("escala para IA full quando confiança por campo crítico está baixa", () => {
    const should = shouldEscalateToAiFull(
      {
        total_amount: 99.9,
        due_date: "2026-06-10",
        competence_date: null,
        supplier_name_raw: "Cemig Distribuição S.A.",
        supplier_cnpj: null,
        document_number: null,
        barcode_digitable_line: null,
        document_type: "boleto",
        financial_intent: "recurring_expense",
        description: null,
      },
      {
        total_amount: 0.9,
        due_date: 0.4, // baixa confiança neste campo
        supplier_name_raw: 0.9,
      },
      0.97,
    );

    expect(should).toBe(true);
  });
});
