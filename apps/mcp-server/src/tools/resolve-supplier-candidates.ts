/**
 * @sbf/mcp-server — Tool: resolve_supplier_candidates
 * Busca candidatos de fornecedor com base em texto/CNPJ do documento.
 */
import type { SupabaseClient } from "@supabase/supabase-js";

export interface SupplierCandidate {
  id: string;
  name: string;
  canonical_name: string | null;
  document_number: string | null;
  category: string | null;
  match_type: "exact_cnpj" | "name_similarity";
}

export async function resolveSupplierCandidates(
  supabase: SupabaseClient,
  userId: string,
  query: { text?: string; cnpj?: string },
): Promise<SupplierCandidate[]> {
  const results: SupplierCandidate[] = [];

  // Busca por CNPJ exato
  if (query.cnpj) {
    const cleaned = query.cnpj.replace(/\D/g, "");
    const { data } = await supabase
      .from("suppliers")
      .select("id, name, canonical_name, document_number, category")
      .eq("user_id", userId)
      .eq("document_number", cleaned);

    if (data) {
      results.push(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        ...data.map((s: any) => ({
          ...s,
          match_type: "exact_cnpj" as const,
        })),
      );
    }
  }

  // Busca por texto (ilike no name e aliases)
  if (query.text) {
    const searchTerm = `%${query.text}%`;
    const { data } = await supabase
      .from("suppliers")
      .select("id, name, canonical_name, document_number, category")
      .eq("user_id", userId)
      .or(`name.ilike.${searchTerm},canonical_name.ilike.${searchTerm}`)
      .limit(10);

    if (data) {
      const existingIds = new Set(results.map((r) => r.id));
      results.push(
        ...data
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .filter((s: any) => !existingIds.has(s.id))
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .map((s: any) => ({
            ...s,
            match_type: "name_similarity" as const,
          })),
      );
    }
  }

  return results;
}
