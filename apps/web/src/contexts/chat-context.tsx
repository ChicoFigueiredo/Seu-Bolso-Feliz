"use client";

/**
 * ChatContext — contexto compartilhado para o assistente de IA.
 *
 * ⚠️ Scaffolding intencional: este provider é criado no Sprint 1 mas consumido
 * integralmente apenas no Sprint 4 (telas de documentos com contexto inline).
 * Não remover por "parece sem uso" — é referenciado em ai-chat-drawer.tsx e
 * será usado em /dashboard/documents/[id] para injetar documentId/documentType
 * automaticamente na conversa com a IA.
 *
 * Sprint 4: adicionados openDrawer, setOpenDrawer e pendingMessage para
 * permitir que AIFieldBadge abra o drawer e injete mensagem inicial.
 */

import { createContext, useContext, useState } from "react";
import type { ReactNode } from "react";

export interface ChatContextValue {
  context: Record<string, unknown>;
  setContext: (ctx: Record<string, unknown>) => void;
  /** Controle global do drawer (usado por ChatToggle + AIFieldBadge) */
  drawerOpen: boolean;
  setDrawerOpen: (open: boolean) => void;
  /** Mensagem pendente a ser injetada no chat quando o drawer abrir */
  pendingMessage: string | null;
  setPendingMessage: (msg: string | null) => void;
}

const ChatContext = createContext<ChatContextValue | null>(null);

export function ChatContextProvider({ children }: { children: ReactNode }) {
  const [context, setContext] = useState<Record<string, unknown>>({});
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [pendingMessage, setPendingMessage] = useState<string | null>(null);

  return (
    <ChatContext.Provider
      value={{ context, setContext, drawerOpen, setDrawerOpen, pendingMessage, setPendingMessage }}
    >
      {children}
    </ChatContext.Provider>
  );
}

export function useChatContext(): ChatContextValue {
  const ctx = useContext(ChatContext);
  if (!ctx) {
    throw new Error("useChatContext deve ser usado dentro de ChatContextProvider");
  }
  return ctx;
}
