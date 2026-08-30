import type { CSSProperties } from "react";
import { Button } from "../primitives/Button";
import { Icon } from "../primitives/Icon";

export interface UpcomingCommitmentProps {
  /** Etiqueta do dia — `Hoje`, `Amanha`, `Seg`. */
  dayLabel: string;
  /** Data curta — `22/08`. */
  dateLabel: string;
  /** Nome do compromisso. */
  name: string;
  /** Conta ou cartao provavel, categoria — o contexto da linha. */
  meta?: string;
  /** Valor formatado. */
  amount: string;
  /**
   * - `pending` — mostra a ação "Paguei".
   * - `paid` — mostra a confirmação verde.
   * - `overdue` — vencido; a linha inteira ganha fundo de risco.
   */
  status?: "pending" | "paid" | "overdue";
  onPay?: () => void;
  style?: CSSProperties;
}

/**
 * Uma linha da fila de compromissos.
 *
 * `overdue` e o único estado que pinta a linha de vermelho — atraso e risco
 * real. Um compromisso que só está chegando fica neutro; o produto não trata
 * gasto previsto como problema.
 *
 * A ação "Paguei" fica na própria linha de proposito: marcar pagamento e a
 * tarefa mais repetida da tela Hoje e não deve custar uma navegação.
 */
export function UpcomingCommitment({
  dayLabel,
  dateLabel,
  name,
  meta,
  amount,
  status = "pending",
  onPay,
  style,
}: UpcomingCommitmentProps) {
  const overdue = status === "overdue";
  const paid = status === "paid";

  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        gap: 13,
        alignItems: "flex-start",
        padding: 13,
        borderRadius: "var(--sbf-radius-lg)",
        background: overdue ? "var(--warn-bg)" : paid ? "var(--paid-bg)" : "var(--surface)",
        border: `1px solid ${overdue ? "var(--warn-line)" : "var(--line-soft)"}`,
        color: "var(--text)",
        ...style,
      }}
    >
      <div style={{ flex: "0 0 54px", textAlign: "center", paddingTop: 2 }}>
        <div
          style={{
            fontSize: 11,
            fontWeight: 700,
            textTransform: "uppercase",
            letterSpacing: ".06em",
            color: overdue ? "var(--danger)" : "var(--primary-ink)",
          }}
        >
          {dayLabel}
        </div>
        <div style={{ fontSize: 12, color: "var(--text-3)" }}>{dateLabel}</div>
      </div>

      <div style={{ flex: "1 1 auto", minWidth: 0 }}>
        <div style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em" }}>{name}</div>
        {meta ? (
          <div style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>{meta}</div>
        ) : null}
      </div>

      <div
        style={{
          flex: "0 0 auto",
          textAlign: "right",
          display: "flex",
          flexDirection: "column",
          gap: 7,
          alignItems: "flex-end",
        }}
      >
        <div style={{ fontSize: 15, fontWeight: 800, fontVariantNumeric: "tabular-nums" }}>
          {amount}
        </div>
        {paid ? (
          <span
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              fontSize: 12,
              fontWeight: 700,
              color: "var(--success)",
            }}
          >
            <Icon name="check_circle" size={17} />
            Pago
          </span>
        ) : (
          <Button variant="soft" size="sm" onClick={onPay}>
            Paguei
          </Button>
        )}
      </div>
    </div>
  );
}
