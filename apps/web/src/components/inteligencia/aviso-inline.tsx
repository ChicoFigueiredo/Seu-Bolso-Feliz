import type { ReactNode } from "react";
import { Icon } from "@sbf/design-system";

/**
 * Aviso discreto de falha parcial numa ação em lote — mesma linguagem visual
 * (tokens `--warn-*`) do card de erro de `ConnectionStatusCard` no
 * design-system, para o vocabulário de "algo deu errado" não variar de tela
 * para tela mesmo sem um primitivo de alerta exportado.
 */
export function AvisoInline({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        gap: 8,
        alignItems: "flex-start",
        background: "var(--warn-bg)",
        border: "1px solid var(--warn-line)",
        borderRadius: "var(--sbf-radius-sm)",
        padding: "10px 12px",
        fontSize: 12.5,
        color: "var(--warn)",
        lineHeight: 1.4,
        fontFamily: "var(--sbf-font-sans)",
      }}
    >
      <Icon name="warning" size={17} />
      <span>{children}</span>
    </div>
  );
}
