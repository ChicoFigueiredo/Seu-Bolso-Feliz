/**
 * Orquestrador do pipeline de parsing.
 * Coordena: download → extração de texto → detecção de parser → parsing → persistência.
 * Implementa os itens 4.1-4.7 do checklist.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionLogLevel, ParserType } from "@sbf/ingestion-types";
import { extractText, PdfPasswordRequiredError } from "./text-extractor";
import { findPdfPassword } from "./secret-lookup";
import { isCemig, parseCemig } from "./cemig-parser";
import { parseBoleto } from "./boleto-parser";
import { extractWithBoletoUtils } from "./boleto-utils-extractor";
import { applySupplierTemplate } from "./supplier-templates";
import { findMatchingPattern } from "./pattern-matcher";
import { resolveFieldConsensus, toFieldCandidates, type FieldCandidate } from "./field-consensus";
import { writeLog, type LogContext } from "../logger";

interface ParseContext {
  supabase: SupabaseClient;
  ctx: LogContext;
  userId: string;
  sourceDocumentId: string;
  mimeType: string;
}

export interface ParseResult {
  parsedVersionId: string;
  extractionResultId: string | null;
  parserType: string;
  confidence: number;
  success: boolean;
}

interface DetectionDiagnostics {
  criticalDetected: number;
  criticalTotal: number;
  criticalCoverage: number;
  missingCriticalFields: string[];
  suspiciousFields: string[];
  rawConfidence: number;
  normalizedConfidence: number;
}

const CRITICAL_KEYS = ["total_amount", "due_date", "supplier_name_raw"] as const;

function hasMeaningfulValue(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return true;
}

function buildExtractedFieldsLogDetails(
  extractionData: Record<string, unknown>,
  parserType: string,
  confidence: number,
  diagnostics: DetectionDiagnostics | null,
): Record<string, unknown> {
  const { confidence: extractedConfidence, ...fields } = extractionData;

  return {
    parser_type: parserType,
    confidence,
    extracted_confidence: extractedConfidence,
    found_fields: Object.entries(fields)
      .filter(([, value]) => hasMeaningfulValue(value))
      .map(([key]) => key),
    fields,
    detection_diagnostics: diagnostics,
  };
}

function normalizeExtractedFields(data: Record<string, unknown>): Record<string, unknown> {
  return {
    total_amount: data.totalAmount ?? data.total_amount ?? null,
    due_date: data.dueDate ?? data.due_date ?? null,
    supplier_name_raw: data.supplierNameRaw ?? data.supplier_name_raw ?? null,
  };
}

function fieldMissing(field: (typeof CRITICAL_KEYS)[number], value: unknown): boolean {
  if (value === null || value === undefined) return true;
  if (typeof value === "string") return value.trim().length === 0;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return true;
    if (field === "total_amount") return value <= 0;
  }
  return false;
}

function fieldSuspicious(field: (typeof CRITICAL_KEYS)[number], value: unknown): boolean {
  if (fieldMissing(field, value)) return true;

  if (field === "supplier_name_raw" && typeof value === "string") {
    const cleaned = value.trim();
    return cleaned.length < 4 || !/[A-Za-zÀ-ÖØ-öø-ÿ]/.test(cleaned);
  }

  if (field === "due_date" && typeof value === "string") {
    return !/^\d{4}-\d{2}-\d{2}$/.test(value);
  }

  if (field === "total_amount" && typeof value === "number") {
    return value <= 0 || value > 10_000_000;
  }

  return false;
}

function evaluateDetectionConfidence(
  rawConfidence: number,
  extractionData: Record<string, unknown> | null,
): { normalizedConfidence: number; diagnostics: DetectionDiagnostics | null } {
  if (!extractionData) {
    return { normalizedConfidence: rawConfidence, diagnostics: null };
  }

  const normalized = normalizeExtractedFields(extractionData);
  const missingCriticalFields: string[] = [];
  const suspiciousFields: string[] = [];
  let criticalDetected = 0;

  for (const field of CRITICAL_KEYS) {
    const value = normalized[field];
    if (fieldMissing(field, value)) {
      missingCriticalFields.push(field);
      continue;
    }
    criticalDetected += 1;
    if (fieldSuspicious(field, value)) suspiciousFields.push(field);
  }

  const criticalCoverage = criticalDetected / CRITICAL_KEYS.length;

  // Cap de confiança por cobertura crítica:
  // 0/3 => 0.35 | 1/3 => 0.57 | 2/3 => 0.78 | 3/3 => 1.0
  const coverageCap = 0.35 + criticalCoverage * 0.65;
  const suspiciousPenalty = suspiciousFields.length > 0 ? 0.1 : 0;
  const normalizedConfidence = Math.max(
    0,
    Math.min(rawConfidence, coverageCap - suspiciousPenalty),
  );

  return {
    normalizedConfidence,
    diagnostics: {
      criticalDetected,
      criticalTotal: CRITICAL_KEYS.length,
      criticalCoverage,
      missingCriticalFields,
      suspiciousFields,
      rawConfidence,
      normalizedConfidence,
    },
  };
}

/**
 * Pipeline completo de parsing para um documento.
 * 1. Download do storage
 * 2. Tentativa de extração de texto (com retry + senha se necessário)
 * 3. Detecção de parser adequado
 * 4. Parsing e extração estruturada
 * 5. Persistência em parsed_document_versions + extraction_results
 */
export async function parseDocument(
  pctx: ParseContext,
  fileData: ArrayBuffer,
): Promise<ParseResult> {
  const { supabase, ctx, userId, sourceDocumentId, mimeType } = pctx;

  // ── 1. Extração de texto ──
  let text: string;
  let pages: number;
  let wasProtected: boolean;
  let extractionMethod: string;
  let ocrApplied: boolean;

  try {
    const result = await extractText(fileData, mimeType);
    text = result.text;
    pages = result.pages;
    wasProtected = result.wasProtected;
    extractionMethod = result.extractionMethod;
    ocrApplied = result.ocrApplied;
  } catch (err) {
    if (err instanceof PdfPasswordRequiredError) {
      await writeLog(supabase, ctx, IngestionLogLevel.INFO, "PDF protegido. Buscando senha...");

      const secret = await findPdfPassword(supabase, userId);
      if (!secret) {
        await writeLog(
          supabase,
          ctx,
          IngestionLogLevel.WARN,
          "Sem senha encontrada para PDF protegido",
        );
        return saveRawOnlyVersion(pctx, "", 0, ParserType.LOCAL_TEXT, 0);
      }

      const retryResult = await extractText(fileData, mimeType, secret.value);
      text = retryResult.text;
      pages = retryResult.pages;
      wasProtected = true;
      extractionMethod = retryResult.extractionMethod;
      ocrApplied = retryResult.ocrApplied;
    } else {
      throw err;
    }
  }

  if (!text || text.trim().length === 0) {
    await writeLog(supabase, ctx, IngestionLogLevel.WARN, "Texto vazio após extração");
    return saveRawOnlyVersion(pctx, "", pages ?? 0, ParserType.LOCAL_TEXT, 0);
  }

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `Texto extraído: ${text.length} chars, ${pages} páginas${wasProtected ? " (PDF protegido)" : ""}`,
    {
      extraction_method: extractionMethod,
      ocr_applied: ocrApplied,
    },
  );

  // ── 2. Detecção de parser e extração ──
  let parserType: string = ParserType.LOCAL_TEXT;
  let extractionData: Record<string, unknown> | null = null;
  let confidence = 0.3; // Confiança base para texto bruto

  if (isCemig(text)) {
    parserType = ParserType.LOCAL_REGEX;
    const cemigResult = parseCemig(text);
    extractionData = cemigResult as unknown as Record<string, unknown>;
    confidence = cemigResult.confidence;
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      `Parser CEMIG detectado (confiança: ${(confidence * 100).toFixed(0)}%)`,
    );
  } else {
    // Tentar parser genérico de boleto
    const boletoResult = parseBoleto(text);
    if (boletoResult.confidence > 0.2) {
      parserType = ParserType.LOCAL_REGEX;
      extractionData = boletoResult as unknown as Record<string, unknown>;
      confidence = boletoResult.confidence;
      await writeLog(
        supabase,
        ctx,
        IngestionLogLevel.INFO,
        `Parser boleto genérico (confiança: ${(confidence * 100).toFixed(0)}%)`,
      );
    }
  }

  const { normalizedConfidence, diagnostics } = evaluateDetectionConfidence(
    confidence,
    extractionData,
  );
  confidence = normalizedConfidence;

  if (diagnostics) {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      `Detecção determinística: cobertura crítica ${diagnostics.criticalDetected}/${diagnostics.criticalTotal}, confiança normalizada ${(diagnostics.normalizedConfidence * 100).toFixed(0)}%`,
      diagnostics as unknown as Record<string, unknown>,
    );
  }

  // ── 2b. Busca de padrão do usuário (ADR-006) ──
  // Tenta identificar document_type a partir da extração atual para buscar padrão
  if (extractionData) {
    const detectedType = detectDocumentType(extractionData);
    const patternMatch = await findMatchingPattern(
      supabase,
      userId,
      detectedType,
      null, // supplier_id resolvido na fase de draft — ainda não disponível aqui
      extractionData,
      confidence,
    );

    if (patternMatch) {
      // Enriquece os dados com os campos mapeados pelo padrão
      extractionData = { ...extractionData, ...patternMatch.mappedFields };
      // Se o padrão aumentou a confiança, aplica
      if (patternMatch.confidence > confidence) {
        confidence = patternMatch.confidence;
      }
      await writeLog(
        supabase,
        ctx,
        IngestionLogLevel.INFO,
        `Padrão aplicado: "${patternMatch.patternName}" v${patternMatch.version} ` +
          `(confiança: ${(patternMatch.confidence * 100).toFixed(0)}%, auto: ${patternMatch.autoApplied})`,
      );
    }
  }

  if (extractionData) {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      "Campos extraídos do documento",
      buildExtractedFieldsLogDetails(extractionData, parserType, confidence, diagnostics),
    );
  }

  // ── 2c. Enriquecimento determinístico com templates + boleto-utils e consenso por campo ──
  const consensusCandidates: FieldCandidate[] = [];
  const normalizedFromParser = extractionData
    ? toNormalizedExtractionFields(extractionData)
    : ({} as Record<string, unknown>);
  if (extractionData) {
    consensusCandidates.push(
      ...toFieldCandidates(
        "deterministic_parser",
        confidence,
        normalizedFromParser,
        `parser:${parserType}`,
      ),
    );
  }

  const templateMatch = applySupplierTemplate(text);
  if (templateMatch) {
    consensusCandidates.push(
      ...toFieldCandidates(
        "supplier_template",
        templateMatch.confidence,
        {
          supplier_name_raw: templateMatch.supplierNameRaw,
          supplier_cnpj: templateMatch.supplierCnpj,
          total_amount: templateMatch.totalAmount,
          due_date: templateMatch.dueDate,
          competence_date: templateMatch.competenceDate,
          document_number: templateMatch.documentNumber,
        },
        `template:${templateMatch.templateId}`,
      ),
    );

    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      `Template local aplicado: ${templateMatch.templateId} (confiança ${(templateMatch.confidence * 100).toFixed(0)}%)`,
      {
        matched_rules: templateMatch.matchedRules,
        template_fields: {
          supplier_name_raw: templateMatch.supplierNameRaw,
          supplier_cnpj: templateMatch.supplierCnpj,
          total_amount: templateMatch.totalAmount,
          due_date: templateMatch.dueDate,
          competence_date: templateMatch.competenceDate,
          document_number: templateMatch.documentNumber,
        },
      },
    );
  }

  const boletoUtilsMatch = await extractWithBoletoUtils(text);
  if (boletoUtilsMatch) {
    consensusCandidates.push(
      ...toFieldCandidates(
        "boleto_utils",
        boletoUtilsMatch.confidence,
        {
          supplier_name_raw: boletoUtilsMatch.supplierNameRaw,
          total_amount: boletoUtilsMatch.totalAmount,
          due_date: boletoUtilsMatch.dueDate,
          barcode_digitable_line: boletoUtilsMatch.barcodeDigitableLine,
          document_type: boletoUtilsMatch.boletoType,
        },
        boletoUtilsMatch.reason,
      ),
    );

    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      `Boleto validado por boleto-utils (tipo: ${boletoUtilsMatch.boletoType ?? "desconhecido"})`,
      {
        boleto_utils_fields: {
          due_date: boletoUtilsMatch.dueDate,
          total_amount: boletoUtilsMatch.totalAmount,
          barcode_digitable_line: boletoUtilsMatch.barcodeDigitableLine,
          supplier_name_raw: boletoUtilsMatch.supplierNameRaw,
        },
      },
    );
  }

  let consensusMeta: {
    sourcePerField: Record<string, string>;
    confidencePerField: Record<string, number>;
    decisions: Record<string, unknown>;
    overallConfidence: number;
  } | null = null;

  if (consensusCandidates.length > 0) {
    const consensus = resolveFieldConsensus(consensusCandidates);
    const legacyConsensus = toLegacyExtractionFields(consensus.values);

    extractionData = {
      ...(extractionData ?? {}),
      ...legacyConsensus,
      confidence:
        confidence > 0
          ? Math.max(consensus.overallConfidence, confidence)
          : consensus.overallConfidence,
    };

    confidence = (extractionData.confidence as number) ?? confidence;
    consensusMeta = {
      sourcePerField: consensus.sourcePerField,
      confidencePerField: consensus.confidencePerField,
      decisions: consensus.decisions,
      overallConfidence: consensus.overallConfidence,
    };

    await writeLog(supabase, ctx, IngestionLogLevel.INFO, "Consenso por campo aplicado", {
      consensus_overall_confidence: consensus.overallConfidence,
      source_per_field: consensus.sourcePerField,
      confidence_per_field: consensus.confidencePerField,
    });
  }

  // ── 3. Persistir parsed_document_version ──
  const { data: lastVersion } = await supabase
    .from("parsed_document_versions")
    .select("version_number")
    .eq("source_document_id", sourceDocumentId)
    .order("version_number", { ascending: false })
    .limit(1)
    .single();

  const versionNumber = (lastVersion?.version_number ?? 0) + 1;

  const { data: parsedVersion, error: pvError } = await supabase
    .from("parsed_document_versions")
    .insert({
      source_document_id: sourceDocumentId,
      user_id: userId,
      version_number: versionNumber,
      parser_type: parserType,
      parser_version: "1.0.0",
      raw_text: text,
      structured_data: extractionData,
      confidence_score: confidence,
      metadata: {
        pages,
        was_protected: wasProtected,
        text_length: text.length,
        extraction_method: extractionMethod,
        ocr_applied: ocrApplied,
        detection_diagnostics: diagnostics,
        field_consensus: consensusMeta,
      },
    })
    .select("id")
    .single();

  if (pvError || !parsedVersion) {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.ERROR,
      `Falha ao salvar parsed_version: ${pvError?.message}`,
    );
    throw new Error(`Falha ao salvar parsed_version: ${pvError?.message}`);
  }

  const parsedVersionId = parsedVersion.id;

  // ── 4. Persistir extraction_result (se houve extração estruturada) ──
  let extractionResultId: string | null = null;

  if (extractionData && confidence > 0.2) {
    const ed = extractionData as Record<string, unknown>;
    const consumption = ed.consumption as { kwh?: number; days?: number } | undefined;

    const { data: exResult, error: exError } = await supabase
      .from("extraction_results")
      .insert({
        parsed_version_id: parsedVersionId,
        user_id: userId,
        supplier_name_raw: (ed.supplierNameRaw as string) ?? null,
        competence_date: (ed.competenceDate as string) ?? null,
        due_date: (ed.dueDate as string) ?? null,
        total_amount: (ed.totalAmount as number) ?? null,
        document_number: (ed.documentNumber as string) ?? null,
        contract_identifier: (ed.contractIdentifier as string) ?? null,
        consumption_data: consumption ? { kwh: consumption.kwh, days: consumption.days } : null,
        breakdown: (ed.breakdown as Record<string, number>) ?? null,
        category_suggestion: suggestCategory(ed),
        tags_suggestion: suggestTags(ed),
        priority_suggestion: "alta",
        metadata: {
          parser_type: parserType,
          parser_version: "1.0.0",
          detection_diagnostics: diagnostics,
          source_per_field: consensusMeta?.sourcePerField ?? null,
          confidence_per_field: consensusMeta?.confidencePerField ?? null,
          field_decisions: consensusMeta?.decisions ?? null,
          // Campos sem coluna dedicada — disponíveis para a camada de IA
          deterministic_extras: {
            supplier_cnpj: (ed.supplierCnpj as string) ?? null,
            barcode_digitable_line: (ed.barcodeDigitableLine as string) ?? null,
          },
        },
      })
      .select("id")
      .single();

    if (exError) {
      await writeLog(
        supabase,
        ctx,
        IngestionLogLevel.WARN,
        `Falha ao salvar extraction_result: ${exError.message}`,
      );
    } else {
      extractionResultId = exResult?.id ?? null;
    }
  }

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `Parsing v${versionNumber} concluído: ${parserType}, confiança ${(confidence * 100).toFixed(0)}%`,
  );

  return {
    parsedVersionId,
    extractionResultId,
    parserType,
    confidence,
    success: true,
  };
}

/** Salva versão com texto vazio (PDF não extraído, imagem, etc.) */
async function saveRawOnlyVersion(
  pctx: ParseContext,
  rawText: string,
  pages: number,
  parserType: string,
  confidence: number,
): Promise<ParseResult> {
  const { supabase, userId, sourceDocumentId } = pctx;

  const { data: lastVersion } = await supabase
    .from("parsed_document_versions")
    .select("version_number")
    .eq("source_document_id", sourceDocumentId)
    .order("version_number", { ascending: false })
    .limit(1)
    .single();

  const versionNumber = (lastVersion?.version_number ?? 0) + 1;

  const { data: pv, error } = await supabase
    .from("parsed_document_versions")
    .insert({
      source_document_id: sourceDocumentId,
      user_id: userId,
      version_number: versionNumber,
      parser_type: parserType,
      parser_version: "1.0.0",
      raw_text: rawText,
      confidence_score: confidence,
      metadata: { pages, needs_manual_review: true },
    })
    .select("id")
    .single();

  if (error || !pv) throw new Error(`Falha ao salvar parsed_version: ${error?.message}`);

  return {
    parsedVersionId: pv.id,
    extractionResultId: null,
    parserType,
    confidence,
    success: rawText.length > 0,
  };
}

function suggestCategory(data: Record<string, unknown>): string | null {
  const name = ((data.supplierNameRaw as string) ?? "").toUpperCase();
  if (
    name.includes("CEMIG") ||
    name.includes("CPFL") ||
    name.includes("ENEL") ||
    name.includes("ENERGI")
  ) {
    return "energia_eletrica";
  }
  if (
    name.includes("COPASA") ||
    name.includes("SABESP") ||
    name.includes("SANEPAR") ||
    name.includes("ÁGUA")
  ) {
    return "agua_saneamento";
  }
  if (
    name.includes("VIVO") ||
    name.includes("CLARO") ||
    name.includes("TIM") ||
    name.includes("OI ")
  ) {
    return "telecomunicacoes";
  }
  return null;
}

function suggestTags(data: Record<string, unknown>): string[] {
  const tags: string[] = ["essencial", "moradia"];
  const name = ((data.supplierNameRaw as string) ?? "").toUpperCase();
  if (name.includes("CEMIG") || name.includes("ENERGI")) {
    tags.push("energia");
  }
  return tags;
}

function toNormalizedExtractionFields(data: Record<string, unknown>): Record<string, unknown> {
  return {
    total_amount: data.totalAmount ?? data.total_amount ?? null,
    due_date: data.dueDate ?? data.due_date ?? null,
    supplier_name_raw: data.supplierNameRaw ?? data.supplier_name_raw ?? null,
    competence_date: data.competenceDate ?? data.competence_date ?? null,
    supplier_cnpj: data.supplierCnpj ?? data.supplier_cnpj ?? null,
    document_number: data.documentNumber ?? data.document_number ?? null,
    barcode_digitable_line: data.barcodeDigitableLine ?? data.barcode_digitable_line ?? null,
    document_type: data.documentType ?? data.document_type ?? null,
  };
}

function toLegacyExtractionFields(data: Record<string, unknown>): Record<string, unknown> {
  return {
    totalAmount: data.total_amount ?? null,
    dueDate: data.due_date ?? null,
    supplierNameRaw: data.supplier_name_raw ?? null,
    competenceDate: data.competence_date ?? null,
    supplierCnpj: data.supplier_cnpj ?? null,
    documentNumber: data.document_number ?? null,
    barcodeDigitableLine: data.barcode_digitable_line ?? null,
    documentType: data.document_type ?? null,
  };
}

/**
 * Detecta o tipo de documento a partir dos dados extraídos.
 * Usado para buscar padrões cadastrados pelo usuário (ADR-006).
 */
function detectDocumentType(data: Record<string, unknown>): string {
  // documento já pode indicar seu tipo explicitamente
  if (typeof data.documentType === "string" && data.documentType) {
    return data.documentType;
  }

  const name = ((data.supplierNameRaw as string) ?? "").toUpperCase();

  if (name.includes("CEMIG") || name.includes("CPFL") || name.includes("ENEL")) {
    return "conta_energia";
  }
  if (name.includes("COPASA") || name.includes("SABESP") || name.includes("SANEPAR")) {
    return "conta_agua";
  }
  if (name.includes("VIVO") || name.includes("CLARO") || name.includes("TIM")) {
    return "conta_telefone";
  }

  // Boleto genérico quando há código de barras / vencimento identificados
  if (data.barCode || data.dueDate) {
    return "boleto_generico";
  }

  return "outros";
}
