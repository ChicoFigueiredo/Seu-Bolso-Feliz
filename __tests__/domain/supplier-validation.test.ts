import { describe, it, expect } from "vitest";
import {
  createSupplierSchema,
  updateSupplierSchema,
  createSupplierAliasSchema,
  createSupplierContractSchema,
  createConsumptionMetricSchema,
} from "@sbf/validation";

const UUID = "00000000-0000-0000-0000-000000000001";

describe("Supplier Validation — T18-T24, T-ALIAS, T-METRIC", () => {
  // ── T18: CRUD Supplier ───────────────────────────────────────

  describe("T18: CRUD de fornecedor", () => {
    it("cria fornecedor com nome, tipo e documento", () => {
      const result = createSupplierSchema.safeParse({
        name: "Neoenergia",
        type: "utility",
        document_number: "12.345.678/0001-90",
      });

      expect(result.success).toBe(true);
    });

    it("rejeita fornecedor sem nome", () => {
      const result = createSupplierSchema.safeParse({
        name: "",
        type: "company",
      });

      expect(result.success).toBe(false);
    });

    it("update parcial aceita apenas nome", () => {
      const result = updateSupplierSchema.safeParse({
        name: "Nova Neoenergia",
      });

      expect(result.success).toBe(true);
    });

    it("tipo padrão é company", () => {
      const result = createSupplierSchema.parse({
        name: "Empresa X",
      });

      expect(result.type).toBe("company");
    });

    it("rejeita tipo inválido", () => {
      const result = createSupplierSchema.safeParse({
        name: "Empresa",
        type: "invalid_type",
      });

      expect(result.success).toBe(false);
    });
  });

  // ── T-ALIAS: Alias Governance ────────────────────────────────

  describe("T-ALIAS: Governança de aliases", () => {
    it("T-ALIAS-01: cria alias com campos obrigatórios", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: UUID,
        alias_name: "CELPE",
        alias_type: "trade_name",
      });

      expect(result.success).toBe(true);
    });

    it("T-ALIAS-02: alias requer supplier_id válido (UUID)", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: "not-a-uuid",
        alias_name: "CELPE",
      });

      expect(result.success).toBe(false);
    });

    it("T-ALIAS-03: alias requer nome não vazio", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: UUID,
        alias_name: "",
      });

      expect(result.success).toBe(false);
    });

    it("T-ALIAS-04: tipo padrão de alias é trade_name", () => {
      const result = createSupplierAliasSchema.parse({
        supplier_id: UUID,
        alias_name: "CELPE",
      });

      expect(result.alias_type).toBe("trade_name");
    });

    it("T-ALIAS-05: alias aceita validade temporal (valid_from/valid_until)", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: UUID,
        alias_name: "CELPE",
        valid_from: "2020-01-01",
        valid_until: "2025-12-31",
      });

      expect(result.success).toBe(true);
    });

    it("T-ALIAS-06: validade temporal com datas válidas passa", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: UUID,
        alias_name: "CELPE",
        valid_from: "2020-01-01",
      });

      expect(result.success).toBe(true);
    });

    it("T-ALIAS-07: aceita todos os tipos de alias válidos", () => {
      const types = ["former_name", "abbreviation", "trade_name", "billing_name", "other"];

      for (const type of types) {
        const result = createSupplierAliasSchema.safeParse({
          supplier_id: UUID,
          alias_name: "Alias",
          alias_type: type,
        });
        expect(result.success).toBe(true);
      }
    });

    it("T-ALIAS-08: rejeita tipo de alias inválido", () => {
      const result = createSupplierAliasSchema.safeParse({
        supplier_id: UUID,
        alias_name: "Alias",
        alias_type: "invalid",
      });

      expect(result.success).toBe(false);
    });
  });

  // ── Supplier Contract ────────────────────────────────────────

  describe("Supplier Contract", () => {
    it("cria contrato com campos obrigatórios", () => {
      const result = createSupplierContractSchema.safeParse({
        supplier_id: UUID,
        contract_type: "subscription",
        description: "Netflix",
        monthly_amount: 55.9,
      });

      expect(result.success).toBe(true);
    });

    it("rejeita tipo de contrato inválido", () => {
      const result = createSupplierContractSchema.safeParse({
        supplier_id: UUID,
        contract_type: "invalid",
      });

      expect(result.success).toBe(false);
    });

    it("aceita todos os tipos de contrato válidos", () => {
      const types = ["subscription", "installment", "on_demand", "prepaid", "other"];

      for (const type of types) {
        const result = createSupplierContractSchema.safeParse({
          supplier_id: UUID,
          contract_type: type,
        });
        expect(result.success).toBe(true);
      }
    });
  });

  // ── T25-T26 + T-METRIC: Consumption Metrics ──────────────────

  describe("T25-T26 + T-METRIC: Métricas de consumo", () => {
    it("T-METRIC-01: aceita métrica com quantity + unit", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        quantity: 320,
        unit: "kWh",
        unit_price: 0.85,
      });

      expect(result.success).toBe(true);
    });

    it("T-METRIC-02: rejeita quantity sem unit", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        quantity: 320,
        // sem unit
      });

      expect(result.success).toBe(false);
    });

    it("T-METRIC-03: aceita métrica tipo attribute (metadata.type='attribute')", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        metadata: { type: "attribute", value: "100Mbps", attribute_name: "velocidade" },
      });

      expect(result.success).toBe(true);
    });

    it("T-METRIC-04: rejeita métrica sem quantity e sem attribute", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        // sem quantity, sem metadata.type='attribute'
      });

      expect(result.success).toBe(false);
    });

    it("T-METRIC-05: rejeita quantity negativa", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        quantity: -10,
        unit: "kWh",
      });

      expect(result.success).toBe(false);
    });

    it("T25: registro de 320 kWh com quantidade e unidade", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-01-01",
        quantity: 320,
        unit: "kWh",
        unit_price: 0.75,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.quantity).toBe(320);
        expect(result.data.unit).toBe("kWh");
      }
    });

    it("T26: série temporal de métricas (múltiplos meses)", () => {
      const months = ["2025-01-01", "2025-02-01", "2025-03-01"];
      const quantities = [320, 280, 350];

      for (let i = 0; i < months.length; i++) {
        const result = createConsumptionMetricSchema.safeParse({
          supplier_id: UUID,
          reference_date: months[i],
          quantity: quantities[i],
          unit: "kWh",
        });
        expect(result.success).toBe(true);
      }
    });

    it("aceita document_id e transaction_id opcionais", () => {
      const result = createConsumptionMetricSchema.safeParse({
        supplier_id: UUID,
        reference_date: "2025-03-01",
        quantity: 100,
        unit: "m³",
        transaction_id: UUID,
        document_id: UUID,
      });

      expect(result.success).toBe(true);
    });
  });
});
