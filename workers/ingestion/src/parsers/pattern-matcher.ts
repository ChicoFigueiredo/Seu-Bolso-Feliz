/**
 * Pattern Matcher — busca padrões de extração do usuário e aplica field_mappings.
 * Integrado no parse-orchestrator após extração de texto e antes de persistência.
 *
 * Fluxo (ADR-006 §3):
 *   1. Busca padrões ativos por supplier_id + document_type (ou só document_type)
 *   2. Para cada match: aplica extraction_rules e field_mappings
 *   3. Se confiança >= confidence_threshold → marca como auto-sugestão
 *   4. Retorna campos enriquecidos ou null se nenhum match
 */

import { and, desc, eq } from "drizzle-orm";
import { document_patterns, type Database } from "@sbf/db";

export interface PatternMatch {
  patternId: string;
  patternName: string;
  version: number;
  mappedFields: Record<string, unknown>;
  confidence: number;
  autoApplied: boolean; // true se confiança >= threshold
}

interface DocumentPattern {
  id: string;
  name: string;
  version: number;
  document_type: string;
  supplier_id: string | null;
  extraction_rules: Record<string, unknown>;
  field_mappings: Record<string, unknown>;
  confidence_threshold: number;
}

/**
 * Busca padrões compatíveis para o documento e aplica mapeamentos.
 *
 * @param db - Instância Drizzle
 * @param userId   - ID do usuário
 * @param documentType - Tipo do documento (ex: "utility_bill", "credit_card_statement")
 * @param supplierId   - ID do fornecedor (nullable)
 * @param extractedFields - Campos já extraídos pelo parser determinístico
 * @param currentConfidence - Confiança atual da extração
 */
export async function findMatchingPattern(
  db: Database,
  userId: string,
  documentType: string | null,
  supplierId: string | null,
  extractedFields: Record<string, unknown>,
  currentConfidence: number,
): Promise<PatternMatch | null> {
  if (!documentType) return null;

  // Busca padrões ativos do usuário — preferência: supplier_id match primeiro
  let patterns: DocumentPattern[];
  try {
    const rows = await db
      .select({
        id: document_patterns.id,
        name: document_patterns.name,
        version: document_patterns.version,
        document_type: document_patterns.document_type,
        supplier_id: document_patterns.supplier_id,
        extraction_rules: document_patterns.extraction_rules,
        field_mappings: document_patterns.field_mappings,
        confidence_threshold: document_patterns.confidence_threshold,
      })
      .from(document_patterns)
      .where(
        and(
          eq(document_patterns.user_id, userId),
          eq(document_patterns.is_active, true),
          eq(document_patterns.document_type, documentType),
        ),
      )
      .orderBy(desc(document_patterns.version))
      .limit(10);
    patterns = rows as unknown as DocumentPattern[];
  } catch {
    // Padrão é enriquecimento opcional: não achar (ou falhar ao buscar) só
    // significa seguir com o que o parser determinístico já extraiu.
    return null;
  }

  if (patterns.length === 0) return null;

  // Prioriza padrões com supplier_id correspondente, depois os globais (supplier_id = null)
  const ranked: DocumentPattern[] = [
    ...patterns.filter((p) => supplierId !== null && p.supplier_id === supplierId),
    ...patterns.filter((p) => p.supplier_id === null),
  ];

  if (ranked.length === 0) return null;

  const best = ranked[0]!;

  // Aplica field_mappings: mapeia campos extraídos para nomes padronizados
  const mappedFields = applyFieldMappings(extractedFields, best.field_mappings);

  // Calcula confiança: usa a atual se não tiver como melhorar
  const mappingCoverage = computeMappingCoverage(mappedFields, best.field_mappings);
  const appliedConfidence = Math.max(
    currentConfidence,
    currentConfidence * (1 + mappingCoverage * 0.2),
  );
  const capped = Math.min(appliedConfidence, 1.0);

  return {
    patternId: best.id,
    patternName: best.name,
    version: best.version,
    mappedFields,
    confidence: capped,
    autoApplied: capped >= best.confidence_threshold,
  };
}

/**
 * Aplica os field_mappings do padrão sobre os campos extraídos.
 * field_mappings: { campo_origem: campo_destino } ou { campo_origem: { target, transform } }
 */
function applyFieldMappings(
  extracted: Record<string, unknown>,
  mappings: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...extracted };

  for (const [sourceField, mapping] of Object.entries(mappings)) {
    if (!(sourceField in extracted)) continue;
    const value = extracted[sourceField];

    if (typeof mapping === "string") {
      // { supplierNameRaw: "supplier_name" }
      result[mapping] = value;
    } else if (typeof mapping === "object" && mapping !== null) {
      const m = mapping as { target?: string; transform?: string };
      if (m.target) {
        result[m.target] = applyTransform(value, m.transform);
      }
    }
  }

  return result;
}

/** Transforma valor conforme instrução simples */
function applyTransform(value: unknown, transform?: string): unknown {
  if (!transform) return value;
  if (typeof value !== "string") return value;

  switch (transform) {
    case "uppercase":
      return value.toUpperCase();
    case "lowercase":
      return value.toLowerCase();
    case "trim":
      return value.trim();
    case "parse_number": {
      const n = parseFloat(value.replace(/[^0-9.,]/g, "").replace(",", "."));
      return isNaN(n) ? value : n;
    }
    default:
      return value;
  }
}

/**
 * Calcula cobertura do mapeamento: quantos campos mapeados foram encontrados.
 * Retorna 0.0 a 1.0.
 */
function computeMappingCoverage(
  mappedFields: Record<string, unknown>,
  mappings: Record<string, unknown>,
): number {
  const total = Object.keys(mappings).length;
  if (total === 0) return 0;

  let found = 0;
  for (const sourceField of Object.keys(mappings)) {
    // O campo alvo deve existir e ter valor não-nulo
    const mapping = mappings[sourceField];
    const targetField =
      typeof mapping === "string" ? mapping : (mapping as { target?: string }).target;
    if (targetField && targetField in mappedFields && mappedFields[targetField] !== null) {
      found++;
    }
  }

  return found / total;
}
