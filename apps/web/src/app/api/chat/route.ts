import { streamText } from "ai";
import { and, eq, sql } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { iaConfigurada, modeloDeConversa } from "@/lib/ai/provider";
import { getUserOrNull } from "@/lib/auth";
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

/**
 * Confere que a sessão de chat pertence ao dono dos dados.
 *
 * `sessionId` chega no corpo da requisição, e `ai_chat_messages.session_id` é
 * uma FK: sem esta checagem, o usuário A conseguiria injetar mensagens (e
 * inflar contadores) na conversa do usuário B só chutando/vazando um UUID.
 * Mesmo padrão de validação de FK de `app/actions/transactions.ts`.
 */
async function sessaoPertenceAoDono(sessionId: string, ownerId: string): Promise<boolean> {
  const db = getDb();
  const [row] = await db
    .select({ id: schema.ai_chat_sessions.id })
    .from(schema.ai_chat_sessions)
    .where(
      and(eq(schema.ai_chat_sessions.id, sessionId), eq(schema.ai_chat_sessions.user_id, ownerId)),
    );
  return Boolean(row);
}

export async function POST(req: Request) {
  // Validate auth — Route Handler: devolve 401, não redireciona.
  const user = await getUserOrNull();

  if (!user) {
    return new Response(JSON.stringify({ error: "Não autenticado" }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Rate limit por ATOR (`user.id`), não por dono: se um administrador de
  // emergência estiver operando a conta, a cota dele é dele. Este é o único
  // ponto do arquivo em que `id` é o valor certo — todo acesso a dados abaixo
  // usa `ownerId`.
  const rateCheck = checkRateLimit(user.id);
  if (!rateCheck.allowed) {
    return new Response(JSON.stringify({ error: rateCheck.error }), {
      status: 429,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Validate API key is configured
  if (!iaConfigurada()) {
    return new Response(
      JSON.stringify({
        error: "IA não configurada. Defina OPENAI_API_KEY ou OPENROUTER_API_KEY no .env",
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

  // A posse é conferida UMA vez, antes de qualquer escrita: `onFinish` roda
  // depois que o stream começou e não teria como recusar a requisição.
  const sessaoValida =
    typeof sessionId === "string" && sessionId.length > 0
      ? await sessaoPertenceAoDono(sessionId, user.ownerId)
      : false;

  if (sessionId && !sessaoValida) {
    return new Response(JSON.stringify({ error: "Sessão de chat não encontrada" }), {
      status: 404,
      headers: { "Content-Type": "application/json" },
    });
  }

  const db = getDb();

  // Log the interaction
  if (sessaoValida) {
    await db.insert(schema.ai_chat_messages).values({
      session_id: sessionId,
      user_id: user.ownerId,
      role: "user",
      content: messages[messages.length - 1]?.content ?? "",
    });
  }

  const result = streamText({
    model: modeloDeConversa(),
    system: SBF_SYSTEM_PROMPT,
    messages,
    tools: sbfTools,
    maxTokens: MAX_TOKENS_PER_REQUEST,
    maxSteps: 5,
    onFinish: async ({ text, usage, toolCalls, toolResults }) => {
      if (!sessaoValida) return;

      // Log assistant response
      if (text) {
        await db.insert(schema.ai_chat_messages).values({
          session_id: sessionId,
          user_id: user.ownerId,
          role: "assistant",
          content: text,
          tokens_used: usage?.totalTokens,
        });
      }

      // Log tool calls for audit
      if (toolCalls && toolCalls.length > 0) {
        const toolEntries = toolCalls.map((call, i) => ({
          session_id: sessionId as string,
          user_id: user.ownerId,
          role: "tool" as const,
          content: JSON.stringify({
            tool_name: call.toolName,
            args: call.args,
            result: toolResults?.[i]?.result ?? null,
          }),
          tokens_used: 0,
        }));

        await db.insert(schema.ai_chat_messages).values(toolEntries);
      }

      // Update session token counter.
      //
      // Antes isto era a RPC `increment_session_tokens`, que NÃO existe no
      // Neon: ela descobria o usuário por `auth.uid()` e foi descartada na
      // migração (ver a NOTA nº 4 no topo de `packages/db/schema.sql`). O
      // incremento é feito aqui, no SQL, e não com um read-modify-write em
      // TypeScript — duas requisições concorrentes na mesma sessão
      // perderiam contagem.
      const msgCount = 2 + (toolCalls?.length ?? 0);
      await db
        .update(schema.ai_chat_sessions)
        .set({
          total_tokens_used: sql`COALESCE(${schema.ai_chat_sessions.total_tokens_used}, 0) + ${usage?.totalTokens ?? 0}`,
          message_count: sql`COALESCE(${schema.ai_chat_sessions.message_count}, 0) + ${msgCount}`,
          updated_at: new Date().toISOString(),
        })
        .where(
          and(
            eq(schema.ai_chat_sessions.id, sessionId),
            eq(schema.ai_chat_sessions.user_id, user.ownerId),
          ),
        );
    },
  });

  return result.toDataStreamResponse();
}
