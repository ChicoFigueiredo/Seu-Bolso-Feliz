/**
 * POST /api/ai-suggest — S4-001
 *
 * Executa uma ferramenta de IA pontualmente e retorna JSON sem streaming.
 * Usado pelo hook useAISuggest para sugestões inline em campos de formulário.
 *
 * Body: { toolName: string, params: Record<string, unknown> }
 * Response: { result: unknown } ou { error: string }
 */

import { generateText } from "ai";
import { openai } from "@ai-sdk/openai";
import { createClient } from "@/lib/supabase/server";
import { sbfTools } from "@/lib/ai/tools";

// Rate limiting independente do chat — janela mais apertada (inline suggestions)
const rateLimits = new Map<string, { count: number; resetAt: number }>();
const RATE_LIMIT_PER_MINUTE = 20;

function checkRateLimit(userId: string): boolean {
  const now = Date.now();
  const entry = rateLimits.get(userId) ?? { count: 0, resetAt: now + 60_000 };
  if (now > entry.resetAt) {
    entry.count = 0;
    entry.resetAt = now + 60_000;
  }
  if (entry.count >= RATE_LIMIT_PER_MINUTE) return false;
  entry.count++;
  rateLimits.set(userId, entry);
  return true;
}

const ALLOWED_TOOLS = new Set([
  "suggest_reconciliation",
  "suggest_splits",
  "suggest_supplier_name",
  "explain_classification",
  "explain_extraction",
  "suggest_category_tags",
  "suggest_document_type",
  "suggest_supplier",
]);

export async function POST(req: Request) {
  // Auth
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) {
    return Response.json({ error: "Não autenticado" }, { status: 401 });
  }

  // Rate limit
  if (!checkRateLimit(user.id)) {
    return Response.json(
      { error: `Limite de ${RATE_LIMIT_PER_MINUTE} sugestões por minuto atingido.` },
      { status: 429 },
    );
  }

  // API key
  if (!process.env.OPENAI_API_KEY) {
    return Response.json({ error: "OPENAI_API_KEY não configurada." }, { status: 500 });
  }

  // Parse body
  let toolName: string;
  let params: Record<string, unknown>;
  try {
    const body = await req.json();
    toolName = body.toolName;
    params = body.params ?? {};
  } catch {
    return Response.json({ error: "Body inválido" }, { status: 400 });
  }

  if (!toolName || typeof toolName !== "string") {
    return Response.json({ error: "toolName é obrigatório" }, { status: 400 });
  }

  // Whitelist de ferramentas permitidas neste endpoint
  if (!ALLOWED_TOOLS.has(toolName)) {
    return Response.json(
      { error: `Ferramenta '${toolName}' não permitida neste endpoint.` },
      { status: 403 },
    );
  }

  // A ferramenta deve existir no sbfTools
  const tool = (sbfTools as Record<string, unknown>)[toolName];
  if (!tool) {
    return Response.json({ error: `Ferramenta '${toolName}' não encontrada.` }, { status: 404 });
  }

  try {
    // Executa via generateText com a única ferramenta solicitada + toolChoice forçado
    const { toolResults } = await generateText({
      model: openai("gpt-4o-mini"),
      tools: { [toolName]: tool } as typeof sbfTools,
      toolChoice: { type: "tool", toolName: toolName as keyof typeof sbfTools },
      maxSteps: 1,
      messages: [
        {
          role: "user",
          content: `Execute a ferramenta ${toolName} com os seguintes parâmetros: ${JSON.stringify(params)}`,
        },
      ],
    });

    const firstResult = toolResults?.[0];
    if (!firstResult) {
      return Response.json({ error: "Ferramenta não retornou resultado" }, { status: 500 });
    }

    return Response.json({ result: firstResult.result });
  } catch (e) {
    const msg = e instanceof Error ? e.message : "Erro interno";
    return Response.json({ error: msg }, { status: 500 });
  }
}
