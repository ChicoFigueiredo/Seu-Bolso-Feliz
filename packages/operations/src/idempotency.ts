/**
 * Política de idempotência para ingestão de documentos.
 * Conforme planejamento 005 seção 2.3.
 *
 * 1. content_hash existe → duplicata exata → rejeitar
 * 2. canonical_fingerprint existe → duplicata semântica → alertar
 * 3. origin_key existe → mesmo documento mesma fonte → atualizar se mudou
 * 4. force_reprocess → recria parsed_version, não duplica source_document
 */

export interface IdempotencyResult {
  action: "reject_exact" | "alert_semantic" | "update_origin" | "proceed";
  existingDocumentId?: string;
  reason?: string;
}

interface IdempotencyCheck {
  contentHash: string;
  canonicalFingerprint?: string;
  originKey: string;
  forceReprocess?: boolean;
}

interface ExistingFingerprints {
  byContentHash?: { sourceDocumentId: string };
  byCanonicalFingerprint?: { sourceDocumentId: string };
  byOriginKey?: { sourceDocumentId: string; contentHash: string };
}

export function checkIdempotency(
  check: IdempotencyCheck,
  existing: ExistingFingerprints,
): IdempotencyResult {
  if (check.forceReprocess && existing.byOriginKey) {
    return {
      action: "proceed",
      existingDocumentId: existing.byOriginKey.sourceDocumentId,
      reason: "force_reprocess",
    };
  }

  // 1. Duplicata exata por content_hash
  if (existing.byContentHash) {
    return {
      action: "reject_exact",
      existingDocumentId: existing.byContentHash.sourceDocumentId,
      reason: "Arquivo com hash idêntico já existe",
    };
  }

  // 2. Duplicata semântica por canonical_fingerprint
  if (check.canonicalFingerprint && existing.byCanonicalFingerprint) {
    return {
      action: "alert_semantic",
      existingDocumentId: existing.byCanonicalFingerprint.sourceDocumentId,
      reason: "Documento com conteúdo semanticamente equivalente encontrado",
    };
  }

  // 3. Mesmo origin_key com conteúdo diferente → atualizar
  if (existing.byOriginKey && existing.byOriginKey.contentHash !== check.contentHash) {
    return {
      action: "update_origin",
      existingDocumentId: existing.byOriginKey.sourceDocumentId,
      reason: "Mesmo documento da mesma fonte com conteúdo atualizado",
    };
  }

  // 3b. Mesmo origin_key com mesmo conteúdo → rejeitar
  if (existing.byOriginKey && existing.byOriginKey.contentHash === check.contentHash) {
    return {
      action: "reject_exact",
      existingDocumentId: existing.byOriginKey.sourceDocumentId,
      reason: "Documento idêntico da mesma fonte já processado",
    };
  }

  // 4. Documento novo
  return { action: "proceed" };
}
