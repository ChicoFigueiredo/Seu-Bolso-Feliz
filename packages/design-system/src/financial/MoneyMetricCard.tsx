import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { MetricLabel } from "../primitives/MetricLabel";
import { MetricValue } from "../primitives/MetricValue";
import { Pill } from "../primitives/Pill";

export interface MoneyMetricCardProps {
  /** Rotulo em versalete — `Disponível agora`, `Livre após compromissos`. */
  label: string;
  /** Valor já formatado em Real. Use `formatBRL`. */
  value: string;
  /**
   * `hero` pinta o card de verde profundo e usa a serifa maior. Reserve para
   * o número mais importante da tela; dois heros lado a lado se anulam.
   */
  variant?: "hero" | "surface";
  /** Linha de contexto abaixo do número — de onde ele veio, quando atualizou. */
  caption?: string;
  /**
   * Marca o número como estimativa. Obrigatorio sempre que o valor puder
   * mudar: o produto não apresenta estimativa como fato.
   */
  estimated?: boolean;
  /** Ressalva ao pe do card, com ícone de atenção. */
  note?: string;
  /** Rotulo do botao de procedencia — normalmente "Como chegamos neste valor?". */
  actionLabel?: string;
  onAction?: () => void;
  style?: CSSProperties;
}

/**
 * O card de um número de dinheiro.
 *
 * E o componente que responde a primeira pergunta do produto — "quanto eu
 * tenho agora?" — e por isso carrega dois compromissos que não são
 * decorativos: a pilula `estimado` quando o valor pode mudar, e o botao de
 * procedencia que abre a conta de como o número foi formado.
 *
 * ```tsx
 * <MoneyMetricCard
 *   variant="hero"
 *   label="Disponível agora"
 *   value={formatBRL(4350)}
 *   caption="Somando 3 contas correntes · atualizado hoje, 08:12"
 *   actionLabel="Como chegamos neste valor?"
 * />
 * ```
 */
export function MoneyMetricCard({
  label,
  value,
  variant = "surface",
  caption,
  estimated = false,
  note,
  actionLabel,
  onAction,
  style,
}: MoneyMetricCardProps) {
  const hero = variant === "hero";
  return (
    <Card
      variant={hero ? "hero" : "surface"}
      style={{ gap: hero ? 10 : 9, minHeight: 150, ...style }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <MetricLabel variant={hero ? "onHero" : "default"}>{label}</MetricLabel>
        {estimated ? (
          <Pill tone="secondary" size="xs">
            estimado
          </Pill>
        ) : null}
      </div>

      <MetricValue size={hero ? "hero" : "display"} color={hero ? "var(--hero-text)" : undefined}>
        {value}
      </MetricValue>

      {caption ? (
        <div
          style={{
            fontSize: 12.5,
            lineHeight: 1.4,
            color: hero ? "var(--hero-sub)" : "var(--text-3)",
          }}
        >
          {caption}
        </div>
      ) : null}

      {note ? (
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: 7,
            fontSize: 12.5,
            color: "var(--text-3)",
            lineHeight: 1.4,
            marginTop: "auto",
          }}
        >
          <Icon name="info" size={17} style={{ color: "var(--warn)", lineHeight: 1.1 }} />
          <span>{note}</span>
        </div>
      ) : null}

      {actionLabel ? (
        <button
          type="button"
          onClick={onAction}
          style={{
            marginTop: "auto",
            alignSelf: "flex-start",
            fontFamily: "var(--sbf-font-sans)",
            border: `1px solid ${hero ? "rgba(245,239,217,.35)" : "var(--line)"}`,
            background: "transparent",
            color: hero ? "var(--hero-accent)" : "var(--text-3)",
            borderRadius: "var(--sbf-radius-pill)",
            padding: "6px 12px",
            fontSize: 12,
            fontWeight: 700,
            cursor: "pointer",
          }}
        >
          {actionLabel}
        </button>
      ) : null}
    </Card>
  );
}
