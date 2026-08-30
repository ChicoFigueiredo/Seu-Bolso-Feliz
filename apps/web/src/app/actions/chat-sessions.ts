"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";

type ChatSession = typeof schema.ai_chat_sessions.$inferSelect;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

// Nota: apesar do nome, esta função sempre CRIA uma sessão nova — nunca
// busca uma existente para reaproveitar. Idêntico ao comportamento anterior
// (createSession() em ai-chat-drawer.tsx); o componente só chama isso quando
// ainda não há sessionId. O nome vem da spec de design, não implica busca.
export async function getOrCreateChatSession(): Promise<ChatSession> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.ai_chat_sessions)
    .values({ user_id: userId, title: "Nova conversa", context_type: "general" })
    .returning();

  if (!row) throw new Error("Falha ao criar sessão de chat");
  return row;
}
