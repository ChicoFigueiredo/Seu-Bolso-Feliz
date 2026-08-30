import type { CSSProperties } from "react";
import { Button } from "../primitives/Button";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { Pill } from "../primitives/Pill";

export interface AIReviewItemProps {
  /** O que a IA encontrou — `Cobranca da Hetzner subiu 34%`. */
  title: string;
  /** A explicacao, em linguagem humana. */
  body: string;
  /** Glifo do tipo de achado. Padrão `auto_awesome`. */
  icon?: string;
  /** Rotulo da ação principal — `Confirmar recorrência`, `Ver lançamentos`. */
  actionLabel: string;
  onAction?: () => void;
  /** Rotulo do descarte. Padrão `Dispensar`. */
  dismissLabel?: string;
  onDismiss?: () => void;
  /** Confiança da IA — `alta`, `média`, `82%`. Sempre visível. */
  confidence?: string;
  style?: CSSProperties;
}

/**
 * Um item da fila "Precisa da sua atenção".
 *
 * Duas regras do produto estão embutidas: a IA nunca grava nada material sem
 * confirmação humana — dai a ação explicita — e toda sugestao carrega a
 * própria confiança, para que o usuário saiba o peso do que está lendo.
 *
 * `Dispensar` e tao proeminente quanto aceitar, de proposito: uma fila de IA
 * que só oferece o caminho do "sim" vira ruido permanente.
 */
export function AIReviewItem({
  title,
  body,
  icon = "auto_awesome",
  actionLabel,
  onAction,
  dismissLabel = "Dispensar",
  onDismiss,
  confidence,
  style,
}: AIReviewItemProps) {
  return (
    <Card
      padding={17}
      style={{
        borderRadius: "var(--sbf-radius-lg)",
        flexDirection: "row",
        gap: 12,
        alignItems: "flex-start",
        ...style,
      }}
    >
      <span
        style={{
          width: 36,
          height: 36,
          flex: "0 0 36px",
          borderRadius: "var(--sbf-radius-md)",
          background: "var(--secondary-soft)",
          color: "var(--secondary-ink)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name={icon} size={20} />
      </span>

      <div style={{ flex: "1 1 auto", minWidth: 0, fontFamily: "var(--sbf-font-sans)" }}>
        <div
          style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em", color: "var(--text)" }}
        >
          {title}
        </div>
        <div style={{ fontSize: 13, color: "var(--text-3)", marginTop: 3, lineHeight: 1.45 }}>
          {body}
        </div>
        <div style={{ display: "flex", gap: 8, marginTop: 12, flexWrap: "wrap" }}>
          <Button variant="soft" size="md" onClick={onAction}>
            {actionLabel}
          </Button>
          <Button variant="outline" size="md" onClick={onDismiss}>
            {dismissLabel}
          </Button>
        </div>
      </div>

      {confidence ? (
        <Pill tone="neutral" size="xs">
          {confidence}
        </Pill>
      ) : null}
    </Card>
  );
}
