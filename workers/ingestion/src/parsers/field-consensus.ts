export type ConsensusSource = "deterministic_parser" | "supplier_template" | "boleto_utils";

export interface FieldCandidate {
  field: string;
  value: unknown;
  confidence: number;
  source: ConsensusSource;
  reason?: string;
}

export interface FieldDecision {
  value: unknown;
  confidence: number;
  source: ConsensusSource;
  reason?: string;
  contenders: Array<{
    source: ConsensusSource;
    confidence: number;
    value: unknown;
    reason?: string;
  }>;
}

export interface ConsensusResult {
  values: Record<string, unknown>;
  confidencePerField: Record<string, number>;
  sourcePerField: Record<string, ConsensusSource>;
  decisions: Record<string, FieldDecision>;
  overallConfidence: number;
}

const SOURCE_PRIORITY: Record<ConsensusSource, number> = {
  boleto_utils: 3,
  supplier_template: 2,
  deterministic_parser: 1,
};

export const DEFAULT_CONSENSUS_FIELDS = [
  "total_amount",
  "due_date",
  "supplier_name_raw",
  "competence_date",
  "supplier_cnpj",
  "document_number",
  "barcode_digitable_line",
] as const;

function hasMeaningfulValue(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return true;
}

function clampConfidence(value: number): number {
  if (!Number.isFinite(value)) return 0;
  if (value < 0) return 0;
  if (value > 1) return 1;
  return value;
}

function pickBestCandidate(candidates: FieldCandidate[]): FieldCandidate {
  const sorted = [...candidates].sort((a, b) => {
    const pa = SOURCE_PRIORITY[a.source];
    const pb = SOURCE_PRIORITY[b.source];
    if (pa !== pb) return pb - pa;
    if (a.confidence !== b.confidence) return b.confidence - a.confidence;
    return String(b.value ?? "").length - String(a.value ?? "").length;
  });
  return sorted[0]!;
}

export function resolveFieldConsensus(
  allCandidates: FieldCandidate[],
  fields: readonly string[] = DEFAULT_CONSENSUS_FIELDS,
): ConsensusResult {
  const values: Record<string, unknown> = {};
  const confidencePerField: Record<string, number> = {};
  const sourcePerField: Record<string, ConsensusSource> = {};
  const decisions: Record<string, FieldDecision> = {};

  const grouped = new Map<string, FieldCandidate[]>();
  for (const candidate of allCandidates) {
    if (!fields.includes(candidate.field)) continue;
    if (!hasMeaningfulValue(candidate.value)) continue;

    const safeCandidate: FieldCandidate = {
      ...candidate,
      confidence: clampConfidence(candidate.confidence),
    };

    const current = grouped.get(candidate.field) ?? [];
    current.push(safeCandidate);
    grouped.set(candidate.field, current);
  }

  let confidenceAccumulator = 0;
  let populatedFields = 0;

  for (const field of fields) {
    const candidates = grouped.get(field) ?? [];
    if (candidates.length === 0) continue;

    const winner = pickBestCandidate(candidates);
    values[field] = winner.value;
    confidencePerField[field] = winner.confidence;
    sourcePerField[field] = winner.source;
    decisions[field] = {
      value: winner.value,
      confidence: winner.confidence,
      source: winner.source,
      reason: winner.reason,
      contenders: candidates.map((c) => ({
        source: c.source,
        confidence: c.confidence,
        value: c.value,
        reason: c.reason,
      })),
    };

    confidenceAccumulator += winner.confidence;
    populatedFields += 1;
  }

  const overallConfidence =
    populatedFields > 0 ? Number((confidenceAccumulator / populatedFields).toFixed(4)) : 0;

  return {
    values,
    confidencePerField,
    sourcePerField,
    decisions,
    overallConfidence,
  };
}

export function toFieldCandidates(
  source: ConsensusSource,
  confidence: number,
  values: Record<string, unknown>,
  reason?: string,
): FieldCandidate[] {
  const list: FieldCandidate[] = [];
  for (const [field, value] of Object.entries(values)) {
    if (!hasMeaningfulValue(value)) continue;
    list.push({
      field,
      value,
      confidence: clampConfidence(confidence),
      source,
      reason,
    });
  }
  return list;
}
