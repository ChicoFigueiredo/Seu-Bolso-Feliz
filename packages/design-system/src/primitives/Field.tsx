import type { CSSProperties } from "react";
import { Pill } from "./Pill";
import type { SbfTone } from "../tokens";

export interface FieldProps {
  label: string;
  /** Valor já extraido — texto, nunca campo editavel nesta leitura. */
  value: string;
  /**
   * Quanto a IA confia no valor. `alta` sai em verde, `provavel` em azul,
   * `baixa` em ambar. Omitir significa "informado por você", sem pilula.
   */
  confidence?: "alta" | "provavel" | "baixa";
  /**
   * Linha divisoria abaixo do campo. Desligue no último de uma sequencia —
   * um traco solto no pe do card le como lista que ficou pela metade.
   */
  divider?: boolean;
  style?: CSSProperties;
}

const tones: Record<string, SbfTone> = { alta: "primary", provavel: "secondary", baixa: "warn" };

/** O prop e ASCII para não complicar a API; o rotulo visível leva acento. */
const labels: Record<string, string> = { alta: "alta", provavel: "provável", baixa: "baixa" };

/**
 * Linha de campo extraido, com a confiança da IA ao lado.
 *
 * E o componente que cumpre a regra de produto "nenhum dado estimado se
 * apresenta como fato": a pilula de confiança fica no mesmo nivel visual do
 * valor, não escondida num rodape.
 */
export function Field({ label, value, confidence, divider = true, style }: FieldProps) {
  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        alignItems: "baseline",
        justifyContent: "space-between",
        gap: 12,
        borderBottom: divider ? "1px solid var(--line-soft)" : "none",
        paddingBottom: divider ? 11 : 0,
        ...style,
      }}
    >
      <span style={{ fontSize: 12.5, color: "var(--text-3)", fontWeight: 600 }}>{label}</span>
      <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
        <span style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)", textAlign: "right" }}>
          {value}
        </span>
        {confidence ? (
          <Pill tone={tones[confidence]} size="xs">
            {labels[confidence]}
          </Pill>
        ) : null}
      </span>
    </div>
  );
}
