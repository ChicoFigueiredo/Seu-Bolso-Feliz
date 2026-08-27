"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";

type ChatSession = typeof schema.ai_chat_sessions.$inferSelect;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
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
