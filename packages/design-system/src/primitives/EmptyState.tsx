import type { CSSProperties, ReactNode } from "react";
import { Icon } from "./Icon";

export interface EmptyStateProps {
  /** Glifo Material Symbols. Padrão `inbox`. */
  icon?: string;
  title: string;
  /** Uma frase dizendo o que fazer, nunca só "nada aqui". */
  description?: string;
  children?: ReactNode;
  style?: CSSProperties;
}

/**
 * Estado vazio.
 *
 * O sistema trata vazio como informação, não como falha: a descricao diz o que
 * o usuário pode fazer, e o tom evita qualquer sugestao de que ele errou.
 */
export function EmptyState({
  icon = "inbox",
  title,
  description,
  children,
  style,
}: EmptyStateProps) {
  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        textAlign: "center",
        gap: 8,
        padding: "34px 20px",
        color: "var(--text)",
        ...style,
      }}
    >
      <Icon name={icon} size={30} style={{ color: "var(--text-4)" }} />
      <div style={{ fontSize: 15, fontWeight: 700 }}>{title}</div>
      {description ? (
        <div style={{ fontSize: 13, color: "var(--text-3)", lineHeight: 1.45, maxWidth: 340 }}>
          {description}
        </div>
      ) : null}
      {children ? <div style={{ marginTop: 6 }}>{children}</div> : null}
    </div>
  );
}
