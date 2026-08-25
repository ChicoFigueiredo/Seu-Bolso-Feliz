/**
 * Resolver de fornecedor por alias exato — Fase 4 (Pluggy).
 *
 * Escopo mínimo: casar o nome bruto da transação (ex.: descrição do extrato
 * Pluggy) com um alias ativo já cadastrado, por igualdade após normalizeName.
 * Sem match, cai para needs_review — o motor completo de scoring
 * (CPF/CNPJ/NSU) fica para uma fase futura.
 *
 * A busca dos candidatos (Supabase, filtrada por user_id) é responsabilidade
 * do chamador; esta função é pura para ficar testável sem infraestrutura.
 */
import { normalizeName } from "../financial-intent/identity-key";

export interface SupplierAliasCandidate {
  supplierId: string;
  aliasName: string;
  isActive: boolean;
}

export type SupplierResolutionResult =
  | { status: "matched"; supplierId: string }
  | { status: "needs_review"; reason: "no_exact_alias_match" };

export function resolveSupplierForTransaction(
  rawName: string,
  candidates: SupplierAliasCandidate[],
): SupplierResolutionResult {
  const normalizedRawName = normalizeName(rawName);

  if (normalizedRawName !== "") {
    const match = candidates.find(
      (candidate) => candidate.isActive && normalizeName(candidate.aliasName) === normalizedRawName,
    );

    if (match) {
      return { status: "matched", supplierId: match.supplierId };
    }
  }

  return { status: "needs_review", reason: "no_exact_alias_match" };
}
