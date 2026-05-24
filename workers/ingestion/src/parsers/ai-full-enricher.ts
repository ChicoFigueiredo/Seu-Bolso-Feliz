/**
 * @sbf/worker-ingestion — AI Full Enricher (Visão)
 *
 * Camada 3 do pipeline híbrido: analisa o documento original via OpenAI com
 * visão computacional (gpt-4o). Usado quando:
 *   - O documento é uma imagem (MIME image/*)
 *   - O PDF é escaneado (texto vazio ou muito curto)
 *   - A camada lite não resolveu campos críticos (needsFullReview = true)
 *   - O usuário solicita explicitamente via UI ou MCP tool
 *
 * Custo estimado: ~1.500–4.000 tokens com imagem (≈ $0.02–0.06 / doc).
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionLogLevel } from "@sbf/ingestion-types";
import { writeLog, type LogContext } from "../logger";

// ── Tipos ───────────────────────────────────────────────────────────────────

export interface AiFullInput {
  extractionResultId: string | null;
  sourceDocumentId: string;
  storagePath: string;
  mimeType: string;
  rawText: string; // pode ser vazio para imagens
}

export interface AiFullOutput {
  enriched: boolean;
  fields: {
    total_amount: number | null;
    due_date: string | null;
    supplier_name_raw: string | null;
    competence_date: string | null;
    document_number: string | null;
    document_type: string | null;
    financial_intent:
      | "transaction"
      | "recurring_expense"
      | "metric"
      | "liability_payment"
      | "unknown";
  };
  confidencePerField: Record<string, number>;
  reasoning: string | null;
  overallConfidence: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function buildVisionPrompt(): string {
  return `Você é um parser financeiro especializado em documentos brasileiros (boletos, contas, faturas, comprovantes).

Analise a imagem deste documento financeiro e extraia os dados solicitados.

Responda APENAS com JSON válido, sem texto adicional, sem markdown, sem backticks:
{
  "total_amount": <número decimal ou null>,
  "due_date": "<YYYY-MM-DD ou null>",
  "competence_date": "<YYYY-MM-DD ou null>",
  "supplier_name_raw": "<nome do fornecedor/emissor ou null>",
  "document_number": "<número do documento ou null>",
  "document_type": "<boleto|conta_energia|conta_agua|conta_telefone|condominio|nfe|recibo|outros>",
  "financial_intent": "<transaction|recurring_expense|metric|liability_payment|unknown>",
  "confidence_per_field": {
    "total_amount": <0.0-1.0>,
    "due_date": <0.0-1.0>,
    "supplier_name_raw": <0.0-1.0>,
    "financial_intent": <0.0-1.0>
  },
  "reasoning": "<frase curta explicando a classificação>"
}`;
}

function buildTextFallbackPrompt(rawText: string): string {
  const excerpt = rawText.slice(0, 3000);
  return `Você é um parser financeiro especializado em documentos brasileiros (boletos, contas, faturas).

Texto extraído do documento (pode estar incompleto ou com problemas de formatação):
"""
${excerpt}
"""

Analise o texto e extraia os dados financeiros.

Responda APENAS com JSON válido, sem texto adicional, sem markdown, sem backticks:
{
  "total_amount": <número decimal ou null>,
  "due_date": "<YYYY-MM-DD ou null>",
  "competence_date": "<YYYY-MM-DD ou null>",
  "supplier_name_raw": "<nome do fornecedor/emissor ou null>",
  "document_number": "<número do documento ou null>",
  "document_type": "<boleto|conta_energia|conta_agua|conta_telefone|condominio|nfe|recibo|outros>",
  "financial_intent": "<transaction|recurring_expense|metric|liability_payment|unknown>",
  "confidence_per_field": {
    "total_amount": <0.0-1.0>,
    "due_date": <0.0-1.0>,
    "supplier_name_raw": <0.0-1.0>,
    "financial_intent": <0.0-1.0>
  },
  "reasoning": "<frase curta explicando a classificação>"
}`;
}

async function callOpenAiVision(
  imageBase64: string,
  mimeType: string,
): Promise<Record<string, unknown>> {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    throw new Error("OPENAI_API_KEY não configurada — enriquecimento full não disponível");
  }

  const response = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: process.env.OPENAI_FULL_MODEL ?? "gpt-4o",
      messages: [
        {
          role: "user",
          content: [
            {
              type: "image_url",
              image_url: { url: `data:${mimeType};base64,${imageBase64}`, detail: "high" },
            },
            { type: "text", text: buildVisionPrompt() },
          ],
        },
      ],
      max_tokens: 600,
      temperature: 0,
      response_format: { type: "json_object" },
    }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`OpenAI Vision API error ${response.status}: ${body.slice(0, 200)}`);
  }

  const data = (await response.json()) as {
    choices: Array<{ message: { content: string } }>;
  };

  return JSON.parse(data.choices[0]?.message?.content ?? "{}") as Record<string, unknown>;
}

async function callOpenAiText(rawText: string): Promise<Record<string, unknown>> {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    throw new Error("OPENAI_API_KEY não configurada — enriquecimento full não disponível");
  }

  const response = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: process.env.OPENAI_FULL_MODEL ?? "gpt-4o",
      messages: [{ role: "user", content: buildTextFallbackPrompt(rawText) }],
      max_tokens: 600,
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

  return JSON.parse(data.choices[0]?.message?.content ?? "{}") as Record<string, unknown>;
}

// ── Função principal ─────────────────────────────────────────────────────────

/**
 * Enriquece um extraction_result via análise full de IA (gpt-4o com visão).
 * Baixa o arquivo do Storage, converte para base64 e envia ao modelo.
 * Para PDFs com texto disponível usa análise de texto (mais barato).
 */
export async function enrichWithAiFull(
  supabase: SupabaseClient,
  ctx: LogContext,
  input: AiFullInput,
): Promise<AiFullOutput> {
  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `IA full: iniciando análise via visão para ${input.mimeType}`,
  );

  const isImage = input.mimeType.startsWith("image/");
  const hasText = input.rawText.trim().length > 50;

  let parsed: Record<string, unknown>;

  try {
    if (isImage || !hasText) {
      // Baixar arquivo do Storage para análise de visão
      const { data: blob, error: dlError } = await supabase.storage
        .from("ingestion-originals")
        .download(input.storagePath);

      if (dlError || !blob) {
        throw new Error(`Falha ao baixar arquivo para análise full: ${dlError?.message}`);
      }

      const arrayBuffer = await blob.arrayBuffer();
      const base64 = Buffer.from(arrayBuffer).toString("base64");
      parsed = await callOpenAiVision(base64, input.mimeType);
    } else {
      // PDF com texto extraído — usar análise de texto (mais barato)
      parsed = await callOpenAiText(input.rawText);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    await writeLog(supabase, ctx, IngestionLogLevel.WARN, `IA full falhou: ${msg}`);
    return {
      enriched: false,
      fields: {
        total_amount: null,
        due_date: null,
        supplier_name_raw: null,
        competence_date: null,
        document_number: null,
        document_type: null,
        financial_intent: "unknown",
      },
      confidencePerField: {},
      reasoning: null,
      overallConfidence: 0,
    };
  }

  // Extrair campos
  const fields: AiFullOutput["fields"] = {
    total_amount: typeof parsed.total_amount === "number" ? parsed.total_amount : null,
    due_date: typeof parsed.due_date === "string" ? parsed.due_date : null,
    supplier_name_raw:
      typeof parsed.supplier_name_raw === "string" ? parsed.supplier_name_raw : null,
    competence_date: typeof parsed.competence_date === "string" ? parsed.competence_date : null,
    document_number: typeof parsed.document_number === "string" ? parsed.document_number : null,
    document_type: typeof parsed.document_type === "string" ? parsed.document_type : null,
    financial_intent: ([
      "transaction",
      "recurring_expense",
      "metric",
      "liability_payment",
      "unknown",
    ].includes(parsed.financial_intent as string)
      ? parsed.financial_intent
      : "unknown") as AiFullOutput["fields"]["financial_intent"],
  };

  const confidencePerField = (parsed.confidence_per_field as Record<string, number>) ?? {};
  const reasoning = typeof parsed.reasoning === "string" ? parsed.reasoning : null;

  const criticalConfs = ["total_amount", "due_date", "supplier_name_raw"].map(
    (f) => confidencePerField[f] ?? 0,
  );
  const overallConfidence = criticalConfs.reduce((a, b) => a + b, 0) / criticalConfs.length;

  // Persistir no banco — atualiza ou cria extraction_result
  if (input.extractionResultId) {
    await supabase
      .from("extraction_results")
      .update({
        supplier_name_raw: fields.supplier_name_raw,
        due_date: fields.due_date,
        total_amount: fields.total_amount,
        competence_date: fields.competence_date,
        document_number: fields.document_number,
        ai_enrichment_type: "full",
        ai_enrichment_at: new Date().toISOString(),
        confidence_per_field: confidencePerField,
        reasoning,
        financial_intent: fields.financial_intent,
      })
      .eq("id", input.extractionResultId);
  }

  // Resetar flag needs_full_ai_review
  await supabase
    .from("ingestion_jobs")
    .update({ needs_full_ai_review: false })
    .eq("source_document_id", input.sourceDocumentId)
    .eq("needs_full_ai_review", true);

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `IA full concluída: intent=${fields.financial_intent}, confiança=${(overallConfidence * 100).toFixed(0)}%`,
    {
      fields,
      confidence_per_field: confidencePerField,
      reasoning,
    },
  );

  return { enriched: true, fields, confidencePerField, reasoning, overallConfidence };
}
