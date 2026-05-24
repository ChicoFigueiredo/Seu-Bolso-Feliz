/**
 * @sbf/worker-ingestion — AI Lite Enricher
 *
 * Camada 2 do pipeline híbrido: enriquece extraction_results usando um modelo
 * de linguagem barato (gpt-4o-mini) quando o parser determinístico ficou abaixo
 * do limiar de confiança ou deixou campos críticos em branco.
 *
 * Regras de ativação:
 *   - confidence < 0.6 após camada 1, OU
 *   - qualquer campo crítico ausente: total_amount | due_date | supplier_name_raw
 *
 * Custo alvo: ~500 tokens entrada + 300 saída por documento (≈ $0.003–0.005 / doc).
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionLogLevel } from "@sbf/ingestion-types";
import { writeLog, type LogContext } from "../logger";

// ── Constantes ──────────────────────────────────────────────────────────────

/**
 * Limiar mínimo de confiança combinada para PULAR o enriquecimento lite.
 * Somente documentos com 95%+ de cobertura e qualidade passam sem IA.
 * Na prática, isso significa que QUASE TODOS os documentos passarão pela IA lite.
 */
export const AI_LITE_CONFIDENCE_THRESHOLD = 0.95;

/** Limiar mínimo para considerar um campo crítico como confiável após IA lite */
export const AI_LITE_CRITICAL_CONFIDENCE_THRESHOLD = 0.95;

/** Limiar mínimo de confiança geral para evitar escalonamento para IA full */
export const AI_LITE_OVERALL_CONFIDENCE_THRESHOLD = 0.95;

/** Campos críticos: ausência de qualquer um aciona a camada lite */
const CRITICAL_FIELDS = ["total_amount", "due_date", "supplier_name_raw"] as const;

/** Máximo de caracteres do trecho de texto enviado ao modelo */
const MAX_TEXT_EXCERPT_CHARS = 3000;

// ── Tipos ───────────────────────────────────────────────────────────────────

export interface AiLiteInput {
  extractionResultId: string;
  rawText: string;
  currentConfidence: number;
  extractedFields: {
    total_amount: number | null;
    due_date: string | null;
    supplier_name_raw: string | null;
    competence_date: string | null;
    document_number: string | null;
    supplier_cnpj: string | null;
    barcode_digitable_line: string | null;
  };
}

/**
 * Contrato completo de saída da camada lite.
 * A IA deve tentar preencher TODOS os campos — não só os críticos.
 */
export interface AiLiteOutput {
  enriched: boolean;
  needsFullReview: boolean;
  fields: {
    // Campos financeiros principais
    total_amount: number | null;
    due_date: string | null;
    competence_date: string | null;
    // Identificação do fornecedor
    supplier_name_raw: string | null;
    supplier_cnpj: string | null;
    // Identificação do documento
    document_number: string | null;
    barcode_digitable_line: string | null;
    document_type: string | null;
    // Classificação semântica
    financial_intent:
      | "transaction"
      | "recurring_expense"
      | "metric"
      | "liability_payment"
      | "unknown";
    // Descrição livre gerada pela IA
    description: string | null;
  };
  confidencePerField: Record<string, number>;
  reasoning: string | null;
  overallConfidence: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function hasLetters(value: string): boolean {
  return /[A-Za-zÀ-ÖØ-öø-ÿ]/.test(value);
}

function isIsoDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function isMissingField(field: (typeof CRITICAL_FIELDS)[number], value: unknown): boolean {
  if (value === null || value === undefined) return true;
  if (typeof value === "string") return value.trim().length === 0;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return true;
    if (field === "total_amount") return value <= 0;
  }
  return false;
}

function isSuspiciousField(field: (typeof CRITICAL_FIELDS)[number], value: unknown): boolean {
  if (isMissingField(field, value)) return true;

  if (field === "supplier_name_raw" && typeof value === "string") {
    const cleaned = value.trim();
    return cleaned.length < 4 || !hasLetters(cleaned);
  }

  if (field === "due_date" && typeof value === "string") {
    return !isIsoDate(value);
  }

  if (field === "total_amount" && typeof value === "number") {
    return value <= 0 || value > 10_000_000;
  }

  return false;
}

export function computeCriticalCoverage(fields: AiLiteInput["extractedFields"]): number {
  const present = CRITICAL_FIELDS.filter((f) => !isMissingField(f, fields[f])).length;
  return present / CRITICAL_FIELDS.length;
}

/**
 * Verifica se a ativação do modo lite é necessária.
 */
export function shouldActivateAiLite(
  confidence: number,
  fields: AiLiteInput["extractedFields"],
): boolean {
  if (confidence < AI_LITE_CONFIDENCE_THRESHOLD) return true;
  return CRITICAL_FIELDS.some((f) => isSuspiciousField(f, fields[f]));
}

export function shouldEscalateToAiFull(
  fields: AiLiteOutput["fields"],
  confidencePerField: Record<string, number>,
  overallConfidence: number,
): boolean {
  if (overallConfidence < AI_LITE_OVERALL_CONFIDENCE_THRESHOLD) return true;

  for (const field of CRITICAL_FIELDS) {
    const value =
      field === "total_amount"
        ? fields.total_amount
        : field === "due_date"
          ? fields.due_date
          : fields.supplier_name_raw;

    if (isSuspiciousField(field, value)) return true;

    const fieldConfidence = confidencePerField[field] ?? 0;
    if (fieldConfidence < AI_LITE_CRITICAL_CONFIDENCE_THRESHOLD) return true;
  }

  return false;
}

/**
 * Constrói o prompt enviado ao modelo.
 * Envia os primeiros MAX_TEXT_EXCERPT_CHARS chars do documento e pede extração COMPLETA.
 * O contrato é: extraia o máximo possível de dados do documento.
 */
function buildPrompt(input: AiLiteInput): string {
  const missingFields = CRITICAL_FIELDS.filter((f) => isMissingField(f, input.extractedFields[f]));

  const excerpt = input.rawText.slice(0, MAX_TEXT_EXCERPT_CHARS);

  const extractedSoFar = JSON.stringify(
    {
      total_amount: input.extractedFields.total_amount,
      due_date: input.extractedFields.due_date,
      competence_date: input.extractedFields.competence_date,
      supplier_name_raw: input.extractedFields.supplier_name_raw,
      supplier_cnpj: input.extractedFields.supplier_cnpj,
      document_number: input.extractedFields.document_number,
      barcode_digitable_line: input.extractedFields.barcode_digitable_line,
    },
    null,
    2,
  );

  return `Você é um parser financeiro especializado em documentos brasileiros (boletos, contas, faturas, carnês, NFe, recibos, comprovantes).

Sua tarefa é extrair o MÁXIMO de dados estruturados possível deste documento.

Dados já extraídos por regex (podem estar incorretos, incompletos ou ausentes — revise tudo):
${extractedSoFar}

Campos críticos ausentes ou com baixa confiança: ${missingFields.length > 0 ? missingFields.join(", ") : "nenhum — mas revise todos os campos acima"}

Texto completo do documento (primeiros ${MAX_TEXT_EXCERPT_CHARS} caracteres):
"""
${excerpt}
"""

INSTRUÇÕES:
- Extraia TODOS os campos abaixo com a maior precisão possível
- Se um campo já foi extraído por regex, CONFIRME ou CORRIJA o valor
- Para campos não encontrados, retorne null — nunca invente valores
- total_amount: valor monetário principal do documento (sem centavos extras) como número decimal
- due_date / competence_date: formato YYYY-MM-DD obrigatório
- supplier_name_raw: nome COMPLETO do fornecedor/emissor (ex: "CEMIG DISTRIBUIÇÃO S.A.", não apenas "CEMIG")
- supplier_cnpj: somente dígitos, sem pontuação (ex: "12345678000199")
- barcode_digitable_line: linha digitável do boleto (47 ou 48 dígitos + pontos/espaços)
- document_type: tipo específico do documento
- financial_intent: classify o propósito financeiro da transação
- description: descreva em 1 frase o que é este documento (ex: "Conta de energia elétrica CEMIG, março 2026")

Responda APENAS com JSON válido, sem texto adicional, sem markdown, sem backticks:
{
  "total_amount": <número decimal ou null>,
  "due_date": "<YYYY-MM-DD ou null>",
  "competence_date": "<YYYY-MM-DD ou null>",
  "supplier_name_raw": "<nome completo do fornecedor ou null>",
  "supplier_cnpj": "<somente dígitos ou null>",
  "document_number": "<número do documento ou null>",
  "barcode_digitable_line": "<linha digitável ou null>",
  "document_type": "<boleto|conta_energia|conta_agua|conta_telefone|condominio|nfe|recibo|carne|iptu|ipva|outros>",
  "financial_intent": "<transaction|recurring_expense|metric|liability_payment|unknown>",
  "description": "<descrição curta em português ou null>",
  "confidence_per_field": {
    "total_amount": <0.0-1.0>,
    "due_date": <0.0-1.0>,
    "supplier_name_raw": <0.0-1.0>,
    "supplier_cnpj": <0.0-1.0>,
    "financial_intent": <0.0-1.0>
  },
  "reasoning": "<frase curta explicando a classificação e eventuais correções>"
}`;
}

/**
 * Chama o OpenAI com retry simples (1 tentativa extra em caso de falha de parse).
 */
async function callOpenAi(prompt: string): Promise<Record<string, unknown>> {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    throw new Error("OPENAI_API_KEY não configurada — enriquecimento lite não disponível");
  }

  const response = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: process.env.OPENAI_LITE_MODEL ?? "gpt-4o-mini",
      messages: [{ role: "user", content: prompt }],
      max_tokens: 400,
      temperature: 0,
      response_format: { type: "json_object" },
    }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`OpenAI API error ${response.status}: ${body.slice(0, 200)}`);
  }

  const data = (await response.json()) as {
    choices: Array<{ message: { content: string } }>;
  };

  const content = data.choices[0]?.message?.content ?? "{}";
  return JSON.parse(content) as Record<string, unknown>;
}

// ── Função principal ─────────────────────────────────────────────────────────

/**
 * Enriquece um extraction_result com IA lite (gpt-4o-mini).
 * Persiste os campos enriquecidos diretamente no banco.
 *
 * @returns AiLiteOutput com os campos enriquecidos e flags de resultado
 */
export async function enrichWithAiLite(
  supabase: SupabaseClient,
  ctx: LogContext,
  input: AiLiteInput,
): Promise<AiLiteOutput> {
  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `IA lite: campos críticos ausentes ou confiança baixa (${(input.currentConfidence * 100).toFixed(0)}%). Iniciando enriquecimento...`,
  );

  const prompt = buildPrompt(input);

  let parsed: Record<string, unknown>;
  try {
    parsed = await callOpenAi(prompt);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    await writeLog(supabase, ctx, IngestionLogLevel.WARN, `IA lite falhou: ${msg}`);
    return {
      enriched: false,
      needsFullReview: true,
      fields: {
        total_amount: input.extractedFields.total_amount,
        due_date: input.extractedFields.due_date,
        competence_date: input.extractedFields.competence_date,
        supplier_name_raw: input.extractedFields.supplier_name_raw,
        supplier_cnpj: input.extractedFields.supplier_cnpj,
        document_number: input.extractedFields.document_number,
        barcode_digitable_line: input.extractedFields.barcode_digitable_line,
        document_type: null,
        financial_intent: "unknown",
        description: null,
      },
      confidencePerField: {},
      reasoning: null,
      overallConfidence: input.currentConfidence,
    };
  }

  // ── Extrair todos os campos da resposta da IA ──

  // Campos financeiros principais
  const aiAmount =
    typeof parsed.total_amount === "number"
      ? parsed.total_amount
      : input.extractedFields.total_amount;
  const aiDueDate =
    typeof parsed.due_date === "string" ? parsed.due_date : input.extractedFields.due_date;
  const aiCompetenceDate =
    typeof parsed.competence_date === "string"
      ? parsed.competence_date
      : input.extractedFields.competence_date;

  // Identificação do fornecedor
  const aiSupplier =
    typeof parsed.supplier_name_raw === "string"
      ? parsed.supplier_name_raw
      : input.extractedFields.supplier_name_raw;
  const aiCnpj =
    typeof parsed.supplier_cnpj === "string"
      ? parsed.supplier_cnpj
      : input.extractedFields.supplier_cnpj;

  // Identificação do documento
  const aiDocNumber =
    typeof parsed.document_number === "string"
      ? parsed.document_number
      : input.extractedFields.document_number;
  const aiBarcode =
    typeof parsed.barcode_digitable_line === "string"
      ? parsed.barcode_digitable_line
      : input.extractedFields.barcode_digitable_line;
  const aiDocType = typeof parsed.document_type === "string" ? parsed.document_type : null;

  // Classificação semântica
  const aiIntent = (
    ["transaction", "recurring_expense", "metric", "liability_payment", "unknown"].includes(
      parsed.financial_intent as string,
    )
      ? parsed.financial_intent
      : "unknown"
  ) as AiLiteOutput["fields"]["financial_intent"];

  const aiDescription = typeof parsed.description === "string" ? parsed.description : null;

  const aiConfPerField = (parsed.confidence_per_field as Record<string, number>) ?? {};
  const aiReasoning = typeof parsed.reasoning === "string" ? parsed.reasoning : null;

  // ── Confiança geral: média dos campos críticos ──
  const criticalConfs = CRITICAL_FIELDS.map((f) => aiConfPerField[f] ?? 0);
  const overallConfidence =
    criticalConfs.length > 0
      ? criticalConfs.reduce((a, b) => a + b, 0) / criticalConfs.length
      : input.currentConfidence;

  // ── Persistir enriquecimento no banco ──
  // Campos que têm colunas dedicadas em extraction_results
  // Campos sem coluna dedicada (supplier_cnpj, barcode_digitable_line, document_type, description)
  // são armazenados no metadata para não perder informação.
  const { error } = await supabase
    .from("extraction_results")
    .update({
      supplier_name_raw: aiSupplier ?? input.extractedFields.supplier_name_raw,
      due_date: aiDueDate ?? input.extractedFields.due_date,
      total_amount: aiAmount ?? input.extractedFields.total_amount,
      competence_date: aiCompetenceDate ?? input.extractedFields.competence_date,
      document_number: aiDocNumber ?? input.extractedFields.document_number,
      ai_enrichment_type: "lite",
      ai_enrichment_at: new Date().toISOString(),
      confidence_per_field: aiConfPerField,
      reasoning: aiReasoning,
      financial_intent: aiIntent,
      metadata: {
        ai_lite_extras: {
          supplier_cnpj: aiCnpj,
          barcode_digitable_line: aiBarcode,
          document_type: aiDocType,
          description: aiDescription,
        },
      },
    })
    .eq("id", input.extractionResultId);

  if (error) {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.WARN,
      `Falha ao persistir enriquecimento lite: ${error.message}`,
    );
  }

  // ── Escala para IA full quando ainda houver incerteza material ──
  const needsFullReview = shouldEscalateToAiFull(
    {
      total_amount: aiAmount,
      due_date: aiDueDate,
      competence_date: aiCompetenceDate,
      supplier_name_raw: aiSupplier,
      supplier_cnpj: aiCnpj,
      document_number: aiDocNumber,
      barcode_digitable_line: aiBarcode,
      document_type: aiDocType,
      financial_intent: aiIntent,
      description: aiDescription,
    },
    aiConfPerField,
    overallConfidence,
  );

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `IA lite concluída: type=${aiDocType}, intent=${aiIntent}, ` +
      `confiança=${(overallConfidence * 100).toFixed(0)}%, needs_full=${needsFullReview}`,
    {
      fields: {
        total_amount: aiAmount,
        due_date: aiDueDate,
        competence_date: aiCompetenceDate,
        supplier_name_raw: aiSupplier,
        supplier_cnpj: aiCnpj,
        document_number: aiDocNumber,
        barcode_digitable_line: aiBarcode,
        document_type: aiDocType,
        financial_intent: aiIntent,
        description: aiDescription,
      },
      confidence_per_field: aiConfPerField,
      reasoning: aiReasoning,
    },
  );

  return {
    enriched: true,
    needsFullReview,
    fields: {
      total_amount: aiAmount,
      due_date: aiDueDate,
      competence_date: aiCompetenceDate,
      supplier_name_raw: aiSupplier,
      supplier_cnpj: aiCnpj,
      document_number: aiDocNumber,
      barcode_digitable_line: aiBarcode,
      document_type: aiDocType,
      financial_intent: aiIntent,
      description: aiDescription,
    },
    confidencePerField: aiConfPerField,
    reasoning: aiReasoning,
    overallConfidence,
  };
}
