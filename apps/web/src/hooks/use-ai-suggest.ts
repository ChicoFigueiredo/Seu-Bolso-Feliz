"use client";

/**
 * useAISuggest — S4-001
 *
 * Faz POST /api/ai-suggest com toolName + params e retorna JSON direto.
 * NÃO abre o drawer. NÃO usa streaming.
 * Destinado a sugestões pontuais inline: campos de metadados, splits, fornecedor, etc.
 *
 * Uso:
 *   const { suggest, data, loading, error } = useAISuggest<SuggestReconciliationResult>();
 *   await suggest("suggest_reconciliation", { document_id: doc.id });
 */

import { useState, useCallback } from "react";

export interface AISuggestState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  suggest: (toolName: string, params: Record<string, unknown>) => Promise<T | null>;
  reset: () => void;
}

export function useAISuggest<T = unknown>(): AISuggestState<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const suggest = useCallback(
    async (toolName: string, params: Record<string, unknown>): Promise<T | null> => {
      setLoading(true);
      setError(null);
      setData(null);

      try {
        const res = await fetch("/api/ai-suggest", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ toolName, params }),
        });

        if (!res.ok) {
          const body = await res.json().catch(() => ({}));
          const msg = (body as { error?: string }).error ?? `Erro ${res.status}`;
          setError(msg);
          return null;
        }

        const result = (await res.json()) as T;
        setData(result);
        return result;
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Erro desconhecido";
        setError(msg);
        return null;
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const reset = useCallback(() => {
    setData(null);
    setError(null);
    setLoading(false);
  }, []);

  return { data, loading, error, suggest, reset };
}
