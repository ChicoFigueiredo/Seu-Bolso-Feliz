/**
 * Leitura tolerante de `draft_records.draft_data`.
 *
 * Despacha por `draft_schema_version`: v1 é validado direto, v0 (ou ausente)
 * passa por `migrateV0ToV1` em memória. Isso é o que torna um backfill parcial
 * um estado seguro em vez de uma tela de revisão quebrada — linhas antigas
 * continuam legíveis para sempre, mesmo que o script nunca termine.
 */
import type { z } from "zod";
import { DRAFT_SCHEMA_VERSION, type DraftType, type Provenance } from "./common";
import { DRAFT_SCHEMAS, type DraftPayloadV1 } from "./index";
import { migrateV0ToV1 } from "./migrate-v0";

export interface DraftRow {
  draft_type: string;
  draft_data: unknown;
  draft_schema_version?: number | null;
}

export type ParseDraftResult =
  | { ok: true; payload: DraftPayloadV1; migrated: boolean }
  | { ok: false; errors: string[]; migrated: boolean };

/** Formata issues do zod como `caminho: mensagem`, o formato que a UI já exibe. */
export function formatIssues(error: z.ZodError): string[] {
  return error.errors.map((e) => {
    const path = e.path.join(".");
    return path ? `${path}: ${e.message}` : e.message;
  });
}

export function parseDraftPayload(row: DraftRow, provenance?: Provenance): ParseDraftResult {
  const draftType = row.draft_type as DraftType;
  const schema = DRAFT_SCHEMAS[draftType];
  if (!schema) {
    return { ok: false, errors: [`draft_type desconhecido: ${row.draft_type}`], migrated: false };
  }

  const version = row.draft_schema_version ?? 0;
  const migrated = version < DRAFT_SCHEMA_VERSION;
  const candidate = migrated
    ? migrateV0ToV1(draftType, row.draft_data, provenance)
    : row.draft_data;

  const parsed = schema.safeParse(candidate);
  if (!parsed.success) {
    return { ok: false, errors: formatIssues(parsed.error), migrated };
  }
  return { ok: true, payload: parsed.data as DraftPayloadV1, migrated };
}
