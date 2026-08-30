import type { CSSProperties, ReactNode } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { SectionTitle } from "../primitives/SectionTitle";

export interface FinancialTimelineProps {
  /** Título do bloco. Padrão `Próximos dias`. */
  title?: string;
  actionLabel?: string;
  onAction?: () => void;
  /** As linhas — normalmente `UpcomingCommitment`. */
  children?: ReactNode;
  /**
   * Fecho do bloco, em dourado: quanto ainda falta para fechar o período.
   * E a frase que transforma uma lista em resposta.
   */
  footnote?: ReactNode;
  /** Glifo do fecho. Padrão `savings`. */
  footnoteIcon?: string;
  style?: CSSProperties;
}

/**
 * A fila de compromissos da tela Hoje.
 *
 * O rodape dourado não e enfeite: e onde a lista vira conclusao — "ate 19/09
 * você ainda precisa de R$ X para fechar o período". Sem ele o usuário tem
 * dados e nenhuma resposta.
 */
export function FinancialTimeline({
  title = "Próximos dias",
  actionLabel,
  onAction,
  children,
  footnote,
  footnoteIcon = "savings",
  style,
}: FinancialTimelineProps) {
  return (
    <Card style={{ gap: 14, ...style }}>
      <SectionTitle actionLabel={actionLabel} onAction={onAction}>
        {title}
      </SectionTitle>

      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>{children}</div>

      {footnote ? (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            background: "var(--accent-soft)",
            borderRadius: "var(--sbf-radius-lg)",
            padding: 14,
            fontFamily: "var(--sbf-font-sans)",
          }}
        >
          <Icon name={footnoteIcon} size={21} style={{ color: "var(--accent-ink)" }} />
          <div style={{ fontSize: 13, color: "var(--accent-ink-2)", lineHeight: 1.4 }}>
            {footnote}
          </div>
        </div>
      ) : null}
    </Card>
  );
}
