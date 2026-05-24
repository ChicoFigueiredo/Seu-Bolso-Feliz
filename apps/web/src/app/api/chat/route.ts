import { streamText } from "ai";
import { openai } from "@ai-sdk/openai";
import { createClient } from "@/lib/supabase/server";
import { untypedFrom, untypedRpc } from "@/lib/supabase/untyped";
import { SBF_SYSTEM_PROMPT } from "@/lib/ai/system-prompt";
import { sbfTools } from "@/lib/ai/tools";

// Rate limiting: in-memory store (per-instance, resets on deploy)
const rateLimits = new Map<
  string,
  { count: number; resetAt: number; dailyCount: number; dailyResetAt: number }
>();

const RATE_LIMIT_PER_MINUTE = 10;
const RATE_LIMIT_PER_DAY = 100;
const MAX_TOKENS_PER_REQUEST = 4096;

function checkRateLimit(userId: string): { allowed: boolean; error?: string } {
  const now = Date.now();
  const entry = rateLimits.get(userId) ?? {
    count: 0,
    resetAt: now + 60_000,
    dailyCount: 0,
    dailyResetAt: now + 86_400_000,
  };

  // Reset minute window
  if (now > entry.resetAt) {
    entry.count = 0;
    entry.resetAt = now + 60_000;
  }

  // Reset daily window
  if (now > entry.dailyResetAt) {
    entry.dailyCount = 0;
    entry.dailyResetAt = now + 86_400_000;
  }

  if (entry.count >= RATE_LIMIT_PER_MINUTE) {
    return {
      allowed: false,
      error: `Limite de ${RATE_LIMIT_PER_MINUTE} mensagens por minuto atingido. Aguarde.`,
    };
  }

  if (entry.dailyCount >= RATE_LIMIT_PER_DAY) {
    return {
      allowed: false,
      error: `Limite de ${RATE_LIMIT_PER_DAY} mensagens por dia atingido.`,
    };
  }

  entry.count++;
  entry.dailyCount++;
  rateLimits.set(userId, entry);
  return { allowed: true };
}

export async function POST(req: Request) {
  // Validate auth
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) {
    return new Response("Unauthorized", { status: 401 });
  }

  // Check rate limit
  const rateCheck = checkRateLimit(user.id);
  if (!rateCheck.allowed) {
    return new Response(JSON.stringify({ error: rateCheck.error }), {
      status: 429,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Validate API key is configured
  if (!process.env.OPENAI_API_KEY) {
    return new Response(
      JSON.stringify({
        error: "API key OpenAI não configurada. Adicione OPENAI_API_KEY ao .env",
      }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }

  const body = await req.json();
  const { messages, sessionId } = body;

  if (!messages || !Array.isArray(messages)) {
    return new Response(JSON.stringify({ error: "messages é obrigatório" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Log the interaction
  if (sessionId) {
    await untypedFrom(supabase, "ai_chat_messages")
      .insert({
        session_id: sessionId,
        user_id: user.id,
        role: "user",
        content: messages[messages.length - 1]?.content ?? "",
      })
      .then(() => {
        // Fire and forget - don't block on logging
      });
  }

  const result = streamText({
    model: openai("gpt-4o"),
    system: SBF_SYSTEM_PROMPT,
    messages,
    tools: sbfTools,
    maxTokens: MAX_TOKENS_PER_REQUEST,
    maxSteps: 5,
    onFinish: async ({ text, usage, toolCalls, toolResults }) => {
      // Log assistant response
      if (sessionId && text) {
        await untypedFrom(supabase, "ai_chat_messages").insert({
          session_id: sessionId,
          user_id: user.id,
          role: "assistant",
          content: text,
          tokens_used: usage?.totalTokens,
        });
      }

      // Log tool calls for audit
      if (sessionId && toolCalls && toolCalls.length > 0) {
        const toolEntries = toolCalls.map((call, i) => ({
          session_id: sessionId,
          user_id: user.id,
          role: "tool" as const,
          content: JSON.stringify({
            tool_name: call.toolName,
            args: call.args,
            result: toolResults?.[i]?.result ?? null,
          }),
          tokens_used: 0,
        }));

        await untypedFrom(supabase, "ai_chat_messages").insert(toolEntries);
      }

      // Update session token counter
      if (sessionId) {
        const msgCount = 2 + (toolCalls?.length ?? 0);
        await untypedRpc(supabase, "increment_session_tokens", {
          p_session_id: sessionId,
          p_tokens: usage?.totalTokens ?? 0,
          p_messages: msgCount,
        });
      }
    },
  });

  return result.toDataStreamResponse();
}
