import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { MetricValue } from "../primitives/MetricValue";
import { Pill } from "../primitives/Pill";
import { ProgressBar } from "../primitives/ProgressBar";
import type { SbfTone } from "../tokens";

export interface CreditCardSummaryProps {
  /** Nome e final — `Cartao Nubank ·4567`. */
  name: string;
  /** Fatura atual, formatada. */
  invoice: string;
  /** Limite livre, formatado. */
  available: string;
  /** Limite total, formatado. */
  limit: string;
  /** Fracao do limite já usada, de 0 a 1. */
  /**
   * Fração do limite já usada, de 0 a 1 — **não** porcentagem. Vai direto para
   * o `ProgressBar`, que satura em 1; passar `75` para "75%" pinta a barra
   * inteira em vez de três quartos, sem erro nenhum a denunciar.
   */
  used: number;
  /** Aviso de vencimento — `vence amanha`. */
  dueLabel?: string;
  /** Tom do aviso. Ambar por padrão: proximidade não e risco. */
  dueTone?: SbfTone;
  style?: CSSProperties;
}

/**
 * Cartao de credito como compromisso futuro, não como conta.
 *
 * A distincao e de dominio: o saldo de uma conta e dinheiro que existe; a
 * fatura de um cartao e divida já assumida. Por isso o número em destaque e a
 * fatura, e o limite livre aparece como barra — útil, mas subordinado.
 *
 * A barra usa azul ardosia (`--secondary`), não verde: consumir limite não e
 * progresso.
 */
export function CreditCardSummary({
  name,
  invoice,
  available,
  limit,
  used,
  dueLabel,
  dueTone = "warn",
  style,
}: CreditCardSummaryProps) {
  return (
    <Card padding={18} style={{ minHeight: 150, gap: 12, ...style }}>
      <div
        style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10 }}
      >
        <div style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>{name}</div>
        {dueLabel ? (
          <Pill tone={dueTone} size="xs">
            {dueLabel}
          </Pill>
        ) : null}
      </div>

      <div>
        <div style={{ fontSize: 11.5, color: "var(--text-3)", fontWeight: 600 }}>Fatura atual</div>
        <MetricValue size="strong">{invoice}</MetricValue>
      </div>

      <div style={{ marginTop: "auto" }}>
        <ProgressBar value={used} fill="var(--secondary)" />
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 10,
            fontSize: 11.5,
            color: "var(--text-3)",
            marginTop: 6,
            fontFamily: "var(--sbf-font-sans)",
          }}
        >
          <span>Limite livre {available}</span>
          <span>de {limit}</span>
        </div>
      </div>
    </Card>
  );
}
