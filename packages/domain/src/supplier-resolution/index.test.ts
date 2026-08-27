/**
 * Resolver de fornecedor por alias exato — Fase 4 (Pluggy).
 *
 * Escopo mínimo: casar o nome bruto da transação com um alias já cadastrado
 * (normalizado, comparação exata). Sem match exato, cai para `needs_review` —
 * o motor completo de scoring (CPF/CNPJ/NSU) fica para depois.
 */
import { describe, it, expect } from "vitest";
import { resolveSupplierForTransaction, type SupplierAliasCandidate } from "./index";

describe("resolveSupplierForTransaction", () => {
  it("matches when raw name normalizes to the same value as an active alias", () => {
    const candidates: SupplierAliasCandidate[] = [
      { supplierId: "supplier-1", aliasName: "Cemig Distribuição S/A", isActive: true },
    ];

    const result = resolveSupplierForTransaction("CEMIG DISTRIBUICAO S.A.", candidates);

    expect(result).toEqual({ status: "matched", supplierId: "supplier-1" });
  });

  it("falls back to needs_review when no alias matches", () => {
    const candidates: SupplierAliasCandidate[] = [
      { supplierId: "supplier-1", aliasName: "Cemig Distribuição S/A", isActive: true },
    ];

    const result = resolveSupplierForTransaction("Posto Ipiranga Centro", candidates);

    expect(result).toEqual({
      status: "needs_review",
      reason: "no_exact_alias_match",
    });
  });

  it("falls back to needs_review when there are no candidates at all", () => {
    const result = resolveSupplierForTransaction("Qualquer Nome", []);

    expect(result).toEqual({
      status: "needs_review",
      reason: "no_exact_alias_match",
    });
  });

  it("ignores inactive aliases", () => {
    const candidates: SupplierAliasCandidate[] = [
      { supplierId: "supplier-1", aliasName: "Cemig Distribuição S/A", isActive: false },
    ];

    const result = resolveSupplierForTransaction("CEMIG DISTRIBUICAO S.A.", candidates);

    expect(result).toEqual({
      status: "needs_review",
      reason: "no_exact_alias_match",
    });
  });

  it("falls back to needs_review when the raw name is empty", () => {
    const candidates: SupplierAliasCandidate[] = [
      { supplierId: "supplier-1", aliasName: "Cemig Distribuição S/A", isActive: true },
    ];

    const result = resolveSupplierForTransaction("", candidates);

    expect(result).toEqual({
      status: "needs_review",
      reason: "no_exact_alias_match",
    });
  });
});
