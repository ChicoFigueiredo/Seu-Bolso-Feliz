"use client";

/**
 * Entradas de formulário desta seção.
 *
 * O `Field` do @sbf/design-system é **leitura** — rótulo, valor já extraído e
 * a pílula de confiança da IA. Ele não tem `onChange` nem `<input>`. Estes
 * componentes são a contraparte editável, com o mesmo desenho (rótulo pequeno
 * acima, alvo de 44px, foco visível) para as duas coisas não parecerem de
 * sistemas diferentes.
 */

import { useId, type ReactNode } from "react";
import { Icon } from "@sbf/design-system";
import styles from "./centros.module.css";

interface BaseProps {
  label: string;
  /** Explicação curta abaixo do campo. Some quando há erro. */
  ajuda?: string;
  /** Mensagem de erro. Quando presente, o campo ganha borda de alerta. */
  erro?: string | null;
}

function Envelope({
  label,
  ajuda,
  erro,
  htmlFor,
  children,
}: BaseProps & { htmlFor: string; children: ReactNode }) {
  return (
    <div style={{ display: "grid", gap: 6, minWidth: 0 }}>
      <label
        htmlFor={htmlFor}
        style={{
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 12.5,
          fontWeight: 700,
          color: "var(--text-3)",
        }}
      >
        {label}
      </label>
      {children}
      {erro ? (
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            fontFamily: "var(--sbf-font-sans)",
            fontSize: 12,
            color: "var(--danger)",
            fontWeight: 600,
          }}
        >
          <Icon name="error" size={14} />
          {erro}
        </span>
      ) : ajuda ? (
        <span style={{ fontFamily: "var(--sbf-font-sans)", fontSize: 12, color: "var(--text-3)" }}>
          {ajuda}
        </span>
      ) : null}
    </div>
  );
}

export interface CampoTextoProps extends BaseProps {
  valor: string;
  onChange: (valor: string) => void;
  placeholder?: string;
  /** Mais de uma linha — usado na descrição do centro. */
  linhas?: number;
  maxLength?: number;
  autoFocus?: boolean;
}

export function CampoTexto({
  label,
  ajuda,
  erro,
  valor,
  onChange,
  placeholder,
  linhas,
  maxLength,
  autoFocus,
}: CampoTextoProps) {
  const id = useId();
  const classe = `${styles.campo} ${erro ? styles.campoComProblema : ""}`;
  return (
    <Envelope label={label} ajuda={ajuda} erro={erro} htmlFor={id}>
      {linhas && linhas > 1 ? (
        <textarea
          id={id}
          className={classe}
          value={valor}
          rows={linhas}
          maxLength={maxLength}
          placeholder={placeholder}
          onChange={(evento) => onChange(evento.target.value)}
          style={{ resize: "vertical", lineHeight: 1.45, fontWeight: 500 }}
        />
      ) : (
        <input
          id={id}
          className={classe}
          type="text"
          value={valor}
          maxLength={maxLength}
          placeholder={placeholder}
          autoFocus={autoFocus}
          onChange={(evento) => onChange(evento.target.value)}
        />
      )}
    </Envelope>
  );
}

export interface CampoValorProps extends BaseProps {
  /** Texto cru do campo, em reais. Nunca number: `1,` é um estado válido de digitação. */
  valor: string;
  onChange: (valor: string) => void;
  placeholder?: string;
  "aria-label"?: string;
}

/**
 * Campo de dinheiro em reais.
 *
 * Guarda **texto**, não número: enquanto o usuário digita "12," o valor não é
 * um número válido, e converter a cada tecla apagaria a vírgula que ele
 * acabou de escrever. A conversão para centavos acontece em
 * `rateio-aritmetica.ts`, num ponto só.
 *
 * `inputMode="decimal"` abre o teclado numérico do celular — que oferece
 * ponto, não vírgula; por isso o leitor aceita os dois.
 */
export function CampoValor({
  label,
  ajuda,
  erro,
  valor,
  onChange,
  placeholder = "0,00",
  ...rest
}: CampoValorProps) {
  const id = useId();
  return (
    <Envelope label={label} ajuda={ajuda} erro={erro} htmlFor={id}>
      <div style={{ position: "relative", display: "flex", alignItems: "center" }}>
        <span
          aria-hidden="true"
          style={{
            position: "absolute",
            left: 12,
            fontFamily: "var(--sbf-font-sans)",
            fontSize: 13,
            fontWeight: 700,
            color: "var(--text-4)",
            pointerEvents: "none",
          }}
        >
          R$
        </span>
        <input
          id={id}
          className={`${styles.campo} ${styles.campoValor} ${erro ? styles.campoComProblema : ""}`}
          type="text"
          inputMode="decimal"
          value={valor}
          placeholder={placeholder}
          aria-label={rest["aria-label"]}
          onChange={(evento) => onChange(evento.target.value)}
          style={{ paddingLeft: 36 }}
        />
      </div>
    </Envelope>
  );
}

export interface OpcaoSelecao {
  valor: string;
  rotulo: string;
  desabilitada?: boolean;
}

export interface CampoSelecaoProps extends BaseProps {
  valor: string;
  onChange: (valor: string) => void;
  opcoes: readonly OpcaoSelecao[];
  /** Primeira linha, sem valor — "Escolha um centro". */
  placeholder?: string;
  "aria-label"?: string;
}

/**
 * Seleção nativa.
 *
 * `<select>` de propósito: no celular ele vira a roleta do sistema, que é
 * maior que qualquer lista customizada, já sabe rolar e já é acessível a
 * leitor de tela sem uma linha de ARIA.
 */
export function CampoSelecao({
  label,
  ajuda,
  erro,
  valor,
  onChange,
  opcoes,
  placeholder,
  ...rest
}: CampoSelecaoProps) {
  const id = useId();
  return (
    <Envelope label={label} ajuda={ajuda} erro={erro} htmlFor={id}>
      <select
        id={id}
        className={`${styles.campo} ${erro ? styles.campoComProblema : ""}`}
        value={valor}
        aria-label={rest["aria-label"]}
        onChange={(evento) => onChange(evento.target.value)}
        style={{ cursor: "pointer" }}
      >
        {placeholder ? <option value="">{placeholder}</option> : null}
        {opcoes.map((opcao) => (
          <option key={opcao.valor} value={opcao.valor} disabled={opcao.desabilitada}>
            {opcao.rotulo}
          </option>
        ))}
      </select>
    </Envelope>
  );
}
