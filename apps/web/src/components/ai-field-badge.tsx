"use client";

/**
 * AIFieldBadge — S4-002
 *
 * Badge inline exibido em campos de metadados extraídos por IA quando a
 * confiança for menor que CONFIDENCE_THRESHOLD (padrão 0.8).
 *
 * Comportamento:
 * - Mostra chip colorido com nível de confiança (baixa/média)
 * - Tooltip com fonte da extração e valor exato de confiança
 * - Botão "Por que?" que abre o drawer de IA com a explicação pré-injetada
 * - Callbacks onAccept/onReject opcionais para ações inline
 */

import { HelpCircle, CheckCircle2, XCircle, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { useChatContext } from "@/contexts/chat-context";

export const CONFIDENCE_THRESHOLD = 0.8;

export interface AIFieldBadgeProps {
  /** Nome legível do campo (ex.: "Fornecedor", "Valor Total") */
  fieldLabel: string;
  /** Confiança da extração (0–1). Badge só aparece quando < CONFIDENCE_THRESHOLD. */
  confidence: number;
  /** Origem da extração (ex.: "OCR", "regex", "gpt-4o") */
  source?: string;
  /** ID do documento para contextualizar a explicação no chat */
  documentId?: string;
  /** Valor extraído pelo campo (para exibição no tooltip) */
  value?: string;
  /** Callback chamado ao aceitar a sugestão da IA */
  onAccept?: () => void;
  /** Callback chamado ao rejeitar a sugestão da IA */
  onReject?: () => void;
  /** Exibir botões de aceitar/rejeitar além do "Por que?" */
  showActions?: boolean;
}

function confidenceLabel(conf: number): string {
  if (conf < 0.5) return "Confiança baixa";
  if (conf < 0.7) return "Confiança moderada";
  return "Revisar";
}

function confidenceVariant(conf: number): "destructive" | "secondary" | "outline" {
  if (conf < 0.5) return "destructive";
  if (conf < 0.7) return "secondary";
  return "outline";
}

export function AIFieldBadge({
  fieldLabel,
  confidence,
  source,
  documentId,
  value,
  onAccept,
  onReject,
  showActions = false,
}: AIFieldBadgeProps) {
  // Não renderizar se confiança for suficiente
  if (confidence >= CONFIDENCE_THRESHOLD) return null;

  const { setDrawerOpen, setPendingMessage } = useChatContext();

  function handleExplain() {
    const parts: string[] = [
      `Por que o campo "${fieldLabel}" foi extraído com confiança ${Math.round(confidence * 100)}%?`,
    ];
    if (value) parts.push(`O valor extraído foi: "${value}".`);
    if (source) parts.push(`Fonte de extração: ${source}.`);
    if (documentId) parts.push(`ID do documento: ${documentId}.`);
    parts.push(
      "Explique o que pode ter causado baixa confiança e o que devo verificar manualmente.",
    );

    setPendingMessage(parts.join(" "));
    setDrawerOpen(true);
  }

  return (
    <TooltipProvider delayDuration={300}>
      <span className="inline-flex items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              variant={confidenceVariant(confidence)}
              className="cursor-default gap-1 text-xs font-normal"
            >
              <Sparkles className="h-3 w-3" />
              {confidenceLabel(confidence)} ({Math.round(confidence * 100)}%)
            </Badge>
          </TooltipTrigger>
          <TooltipContent side="top" className="max-w-xs text-sm">
            <p className="font-semibold">Extraído por IA</p>
            {value && (
              <p className="text-muted-foreground mt-0.5">
                Valor: <span className="font-mono">{value}</span>
              </p>
            )}
            {source && (
              <p className="text-muted-foreground mt-0.5">
                Fonte: <span className="font-mono">{source}</span>
              </p>
            )}
            <p className="text-muted-foreground mt-1 text-xs">
              Confiança abaixo de {Math.round(CONFIDENCE_THRESHOLD * 100)}% — revise o valor.
            </p>
          </TooltipContent>
        </Tooltip>

        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-5 w-5"
          title={`Por que "${fieldLabel}" tem baixa confiança?`}
          onClick={handleExplain}
        >
          <HelpCircle className="h-3.5 w-3.5 text-muted-foreground" />
        </Button>

        {showActions && onAccept && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-5 w-5 text-green-600"
            title="Aceitar sugestão"
            onClick={onAccept}
          >
            <CheckCircle2 className="h-3.5 w-3.5" />
          </Button>
        )}

        {showActions && onReject && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-5 w-5 text-red-600"
            title="Rejeitar sugestão"
            onClick={onReject}
          >
            <XCircle className="h-3.5 w-3.5" />
          </Button>
        )}
      </span>
    </TooltipProvider>
  );
}
