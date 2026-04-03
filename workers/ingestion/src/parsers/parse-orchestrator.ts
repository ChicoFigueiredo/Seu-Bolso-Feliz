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
import { findMatchingPattern } from "./pattern-matcher";
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

  try {
    const result = await extractText(fileData, mimeType);
    text = result.text;
    pages = result.pages;
    wasProtected = result.wasProtected;
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
  );

  // ── 2. Detecção de parser e extração ──
  let parserType = ParserType.LOCAL_TEXT;
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
        metadata: { parser_type: parserType, parser_version: "1.0.0" },
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
