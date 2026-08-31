/**
 * @sbf/mcp-server — Tool: resolve_supplier_candidates
 * Busca candidatos de fornecedor com base em texto/CNPJ do documento.
 */
import { and, eq, ilike, or } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";

export interface SupplierCandidate {
  id: string;
  name: string;
  trade_name: string | null;
  legal_name: string | null;
  document_number: string | null;
  type: string;
  match_type: "exact_cnpj" | "name_similarity";
}

/**
 * `suppliers` nunca teve `canonical_name`/`category` — colunas assim não
 * existem no schema (nem existiram antes, a julgar pelos tipos
 * gerados antigos). O `.select("id, name, canonical_name, ...")` do
 * driver anterior não é checado estaticamente, então essa ferramenta vinha
 * quebrada em silêncio: a busca por texto (`.or("canonical_name.ilike...")`)
 * provavelmente sempre lançava erro no lado do Postgres (coluna
 * inexistente) e o resultado virava lista vazia. Corrigido aqui para as
 * colunas reais mais próximas em intenção — `trade_name`/`legal_name` como
 * nomes alternativos, `type` como categorização.
 */
const SUPPLIER_COLUMNS = {
  id: schema.suppliers.id,
  name: schema.suppliers.name,
  trade_name: schema.suppliers.trade_name,
  legal_name: schema.suppliers.legal_name,
  document_number: schema.suppliers.document_number,
  type: schema.suppliers.type,
};

export async function resolveSupplierCandidates(
  db: Database,
  userId: string,
  query: { text?: string; cnpj?: string },
): Promise<SupplierCandidate[]> {
  const results: SupplierCandidate[] = [];

  // Busca por CNPJ exato
  if (query.cnpj) {
    const cleaned = query.cnpj.replace(/\D/g, "");
    const rows = await db
      .select(SUPPLIER_COLUMNS)
      .from(schema.suppliers)
      .where(
        and(eq(schema.suppliers.user_id, userId), eq(schema.suppliers.document_number, cleaned)),
      );

    results.push(...rows.map((s) => ({ ...s, match_type: "exact_cnpj" as const })));
  }

  // Busca por texto (ilike no nome e nas variações de nome)
  if (query.text) {
    const searchTerm = `%${query.text}%`;
    const rows = await db
      .select(SUPPLIER_COLUMNS)
      .from(schema.suppliers)
      .where(
        and(
          eq(schema.suppliers.user_id, userId),
          or(
            ilike(schema.suppliers.name, searchTerm),
            ilike(schema.suppliers.trade_name, searchTerm),
            ilike(schema.suppliers.legal_name, searchTerm),
          ),
        ),
      )
      .limit(10);

    const existingIds = new Set(results.map((r) => r.id));
    results.push(
      ...rows
        .filter((s) => !existingIds.has(s.id))
        .map((s) => ({ ...s, match_type: "name_similarity" as const })),
    );
  }

  return results;
}
