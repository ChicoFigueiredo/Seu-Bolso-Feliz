import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { MetricValue } from "../primitives/MetricValue";
import { Pill } from "../primitives/Pill";

export interface AccountBalanceCardProps {
  /** Nome da conta — `Conta Corrente Caixa`. */
  name: string;
  /** Saldo formatado. */
  balance: string;
  /** Glifo da instituicao. Padrão `account_balance`. */
  icon?: string;
  /** Último movimento, ou a data da conciliacao. */
  caption?: string;
  /** `Conciliado` quando o saldo bate com o extrato; ausente quando não bate. */
  reconciled?: boolean;
  /** Saldo fora do cálculo de dinheiro livre — reserva, poupança. */
  excluded?: boolean;
  onClick?: () => void;
  style?: CSSProperties;
}

/**
 * Saldo de uma conta bancaria.
 *
 * `excluded` existe para separar reserva de dinheiro do dia: a poupança
 * aparece, mas esmaecida e rotulada, para não inflar a leitura de "quanto eu
 * tenho agora".
 */
export function AccountBalanceCard({
  name,
  balance,
  icon = "account_balance",
  caption,
  reconciled = false,
  excluded = false,
  onClick,
  style,
}: AccountBalanceCardProps) {
  return (
    <Card padding={18} onClick={onClick} style={{ gap: 11, ...style }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 36,
            height: 36,
            flex: "0 0 36px",
            borderRadius: "var(--sbf-radius-md)",
            background: "var(--primary-soft)",
            color: "var(--primary-ink)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon name={icon} size={20} />
        </span>
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>{name}</div>
          {caption ? (
            <div style={{ fontSize: 12, color: "var(--text-3)", marginTop: 2 }}>{caption}</div>
          ) : null}
        </div>
        {reconciled ? (
          <Pill tone="primary" size="xs" icon="check_circle">
            conciliado
          </Pill>
        ) : null}
      </div>

      <MetricValue size="strong" color={excluded ? "var(--text-3)" : undefined}>
        {balance}
      </MetricValue>

      {excluded ? (
        <div style={{ fontSize: 11.5, color: "var(--text-3)", fontFamily: "var(--sbf-font-sans)" }}>
          Fora do cálculo de dinheiro livre
        </div>
      ) : null}
    </Card>
  );
}
