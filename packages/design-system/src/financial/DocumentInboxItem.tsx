import type { CSSProperties } from "react";
import { Icon } from "../primitives/Icon";
import { Pill } from "../primitives/Pill";
import type { SbfTone } from "../tokens";

/**
 * Estados que o usuário ve. São humanos de proposito: `PARSING`,
 * `RECONCILED` e o resto da maquina de estados da ingestao nunca chegam a UI.
 */
export type DocumentState =
  | "pronto"
  | "precisa-revisar"
  | "possivel-duplicidade"
  | "protegido-por-senha"
  | "nao-reconhecido"
  | "processando";

export interface DocumentInboxItemProps {
  /** Nome do arquivo ou do documento. */
  name: string;
  /** Origem e chegada — `Gmail (financeiro@) · 21/08 as 06:40`. */
  meta?: string;
  /** Valor extraido, formatado. Ausente quando ainda não se sabe. */
  value?: string;
  state: DocumentState;
  /** Glifo do tipo de arquivo. Padrão segue o estado. */
  icon?: string;
  onClick?: () => void;
  style?: CSSProperties;
}

const states: Record<DocumentState, { label: string; tone: SbfTone; icon: string }> = {
  pronto: { label: "Pronto", tone: "primary", icon: "description" },
  "precisa-revisar": { label: "Precisa revisar", tone: "warn", icon: "rate_review" },
  "possivel-duplicidade": { label: "Possível duplicidade", tone: "warn", icon: "content_copy" },
  "protegido-por-senha": { label: "Protegido por senha", tone: "secondary", icon: "lock" },
  "nao-reconhecido": { label: "Não reconhecido", tone: "danger", icon: "help" },
  processando: { label: "Processando", tone: "neutral", icon: "hourglass_top" },
};

/**
 * Uma linha da caixa de entrada financeira.
 *
 * Camera, upload, Gmail e pasta local desembocam na mesma lista — a origem
 * vira contexto (`meta`), não uma aba separada. O estado e sempre uma frase
 * que o usuário entende; termos do pipeline ficam do lado de dentro.
 *
 * Use dentro de um `<Card variant="flush">` para as linhas encostarem nas
 * bordas e dividirem o mesmo tracejado.
 */
export function DocumentInboxItem({
  name,
  meta,
  value,
  state,
  icon,
  onClick,
  style,
}: DocumentInboxItemProps) {
  const s = states[state];
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        width: "100%",
        gap: 12,
        alignItems: "center",
        padding: "15px 16px",
        border: 0,
        borderTop: "1px solid var(--line-soft)",
        background: "var(--surface)",
        cursor: "pointer",
        textAlign: "left",
        minHeight: 70,
        color: "var(--text)",
        ...style,
      }}
    >
      <span
        style={{
          width: 40,
          height: 40,
          flex: "0 0 40px",
          borderRadius: "var(--sbf-radius-md)",
          background: "var(--muted)",
          color: "var(--text-2)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name={icon ?? s.icon} size={21} />
      </span>

      <span style={{ flex: "1 1 auto", minWidth: 0 }}>
        <span
          style={{ display: "block", fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em" }}
        >
          {name}
        </span>
        {meta ? (
          <span style={{ display: "block", fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>
            {meta}
          </span>
        ) : null}
      </span>

      <span style={{ flex: "0 0 auto", display: "flex", alignItems: "center", gap: 10 }}>
        {value ? (
          <span style={{ fontSize: 14, fontWeight: 800, fontVariantNumeric: "tabular-nums" }}>
            {value}
          </span>
        ) : null}
        <Pill tone={s.tone} size="xs">
          {s.label}
        </Pill>
        <Icon name="chevron_right" size={20} style={{ color: "var(--text-5)" }} />
      </span>
    </button>
  );
}
