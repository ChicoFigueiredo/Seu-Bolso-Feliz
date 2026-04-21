"use client";

/**
 * ChatContext — contexto compartilhado para o assistente de IA.
 *
 * ⚠️ Scaffolding intencional: este provider é criado no Sprint 1 mas consumido
 * integralmente apenas no Sprint 4 (telas de documentos com contexto inline).
 * Não remover por "parece sem uso" — é referenciado em ai-chat-drawer.tsx e
 * será usado em /dashboard/documents/[id] para injetar documentId/documentType
 * automaticamente na conversa com a IA.
 */

import { createContext, useContext, useState } from "react";
import type { ReactNode } from "react";

export interface ChatContextValue {
  context: Record<string, unknown>;
  setContext: (ctx: Record<string, unknown>) => void;
}

const ChatContext = createContext<ChatContextValue | null>(null);

export function ChatContextProvider({ children }: { children: ReactNode }) {
  const [context, setContext] = useState<Record<string, unknown>>({});

  return <ChatContext.Provider value={{ context, setContext }}>{children}</ChatContext.Provider>;
}

export function useChatContext(): ChatContextValue {
  const ctx = useContext(ChatContext);
  if (!ctx) {
    throw new Error("useChatContext deve ser usado dentro de ChatContextProvider");
  }
  return ctx;
}
