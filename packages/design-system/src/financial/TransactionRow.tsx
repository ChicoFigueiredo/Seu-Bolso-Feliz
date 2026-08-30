import type { CSSProperties } from "react";
import { Icon } from "../primitives/Icon";
import { Pill } from "../primitives/Pill";

export interface TransactionRowProps {
  /** Descricao do lançamento. */
  description: string;
  /** Conta ou cartao, categoria — o contexto. */
  meta?: string;
  /** Valor formatado, com sinal. */
  amount: string;
  /** Entrada pinta o valor de verde; saida fica neutra. */
  direction?: "in" | "out";
  /** Centros aos quais o lançamento foi alocado. */
  centers?: string[];
  /** Ha comprovante vinculado. */
  hasDocument?: boolean;
  /** Ainda não confirmado pelo usuário. */
  pending?: boolean;
  onClick?: () => void;
  style?: CSSProperties;
}

/**
 * Uma linha da timeline de movimentacoes.
 *
 * Saida não e pintada de vermelho: gastar não e erro, e o sinal negativo com o
 * contexto já basta. Vermelho fica reservado para atraso e falha — se toda
 * despesa gritasse, nada gritaria.
 *
 * O clipe de documento e um afeto do dominio: um lançamento com comprovante
 * vinculado e um lançamento que o usuário consegue explicar depois.
 */
export function TransactionRow({
  description,
  meta,
  amount,
  direction = "out",
  centers = [],
  hasDocument = false,
  pending = false,
  onClick,
  style,
}: TransactionRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        alignItems: "center",
        gap: 12,
        width: "100%",
        padding: "13px 16px",
        border: 0,
        borderTop: "1px solid var(--line-soft)",
        background: "var(--surface)",
        cursor: "pointer",
        textAlign: "left",
        minHeight: 64,
        color: "var(--text)",
        ...style,
      }}
    >
      <span style={{ flex: "1 1 auto", minWidth: 0 }}>
        <span style={{ display: "flex", alignItems: "center", gap: 7 }}>
          <span style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em" }}>
            {description}
          </span>
          {hasDocument ? (
            <Icon name="attach_file" size={15} style={{ color: "var(--text-4)" }} />
          ) : null}
        </span>
        {meta ? (
          <span style={{ display: "block", fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>
            {meta}
          </span>
        ) : null}
        {centers.length > 0 ? (
          <span style={{ display: "flex", gap: 5, marginTop: 7, flexWrap: "wrap" }}>
            {centers.map((c) => (
              <Pill key={c} tone="neutral" size="xs">
                {c}
              </Pill>
            ))}
          </span>
        ) : null}
      </span>

      <span
        style={{
          flex: "0 0 auto",
          display: "flex",
          flexDirection: "column",
          alignItems: "flex-end",
          gap: 5,
        }}
      >
        <span
          style={{
            fontSize: 15,
            fontWeight: 800,
            fontVariantNumeric: "tabular-nums",
            color: direction === "in" ? "var(--success)" : "var(--text)",
          }}
        >
          {amount}
        </span>
        {pending ? (
          <Pill tone="warn" size="xs">
            nao confirmado
          </Pill>
        ) : null}
      </span>
    </button>
  );
}
