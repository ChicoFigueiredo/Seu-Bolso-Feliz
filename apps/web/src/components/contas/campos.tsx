/**
 * Campos de formulário para as folhas de Contas & Cartões.
 *
 * `@sbf/design-system` ainda não tem uma primitiva de campo editável — `Field`
 * de lá é só leitura (valor já extraído + confiança da IA, nunca um input).
 * Em vez de adicionar uma ao pacote compartilhado (fora do escopo desta
 * tela), estes campos ficam locais, com `<select>`/`<input>` nativos e o
 * mesmo estilo visual dos outros primitivos (tokens de cor, raio, tipografia).
 * `<select>` nativo é deliberado — mesma razão de
 * `components/movimentacoes/folha-filtros.tsx`: no celular abre o seletor do
 * próprio sistema, mais rápido e mais acessível do que uma lista desenhada
 * à mão.
 */
import type { CSSProperties } from "react";

export interface OpcaoCampo {
  value: string;
  label: string;
}

const rotuloEstilo: CSSProperties = {
  fontSize: 12.5,
  fontWeight: 700,
  color: "var(--text-3)",
  marginBottom: 6,
  display: "block",
};

// minHeight 44 cobre o alvo de toque mínimo pedido para a tela (mobile-first,
// 390px).
const campoEstilo: CSSProperties = {
  fontFamily: "var(--sbf-font-sans)",
  width: "100%",
  minHeight: 44,
  padding: "10px 12px",
  fontSize: 14,
  fontWeight: 600,
  color: "var(--text)",
  background: "var(--surface)",
  border: "1px solid var(--line)",
  borderRadius: "var(--sbf-radius-md)",
  boxSizing: "border-box",
};

interface CampoBaseProps {
  rotulo: string;
  obrigatorio?: boolean;
  style?: CSSProperties;
}

export function CampoTexto({
  rotulo,
  obrigatorio,
  value,
  onChange,
  placeholder,
  maxLength,
  autoFocus,
  style,
}: CampoBaseProps & {
  value: string;
  onChange: (valor: string) => void;
  placeholder?: string;
  maxLength?: number;
  autoFocus?: boolean;
}) {
  return (
    <label style={{ display: "block", ...style }}>
      <span style={rotuloEstilo}>
        {rotulo}
        {obrigatorio ? " *" : ""}
      </span>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        maxLength={maxLength}
        autoFocus={autoFocus}
        style={campoEstilo}
      />
    </label>
  );
}

export function CampoNumero({
  rotulo,
  obrigatorio,
  value,
  onChange,
  placeholder,
  min,
  max,
  step,
  style,
}: CampoBaseProps & {
  /** Texto cru, não número: controlar como número reposiciona o cursor
   * enquanto o usuário digita "1", "1,", "1,5". A conversão só acontece ao
   * salvar, via `numeroOuNulo` (mesmo padrão de
   * apps/web/src/app/dashboard/cards/card-form.tsx). */
  value: string;
  onChange: (valor: string) => void;
  placeholder?: string;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <label style={{ display: "block", ...style }}>
      <span style={rotuloEstilo}>
        {rotulo}
        {obrigatorio ? " *" : ""}
      </span>
      <input
        type="number"
        inputMode="decimal"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        min={min}
        max={max}
        step={step}
        style={campoEstilo}
      />
    </label>
  );
}

export function CampoSelecao({
  rotulo,
  obrigatorio,
  value,
  onChange,
  opcoes,
  placeholder,
  style,
}: CampoBaseProps & {
  value: string;
  onChange: (valor: string) => void;
  opcoes: readonly OpcaoCampo[];
  placeholder?: string;
}) {
  return (
    <label style={{ display: "block", ...style }}>
      <span style={rotuloEstilo}>
        {rotulo}
        {obrigatorio ? " *" : ""}
      </span>
      <select value={value} onChange={(e) => onChange(e.target.value)} style={campoEstilo}>
        {placeholder ? <option value="">{placeholder}</option> : null}
        {opcoes.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function CampoAlternavel({
  rotulo,
  checked,
  onChange,
}: {
  rotulo: string;
  checked: boolean;
  onChange: (valor: boolean) => void;
}) {
  return (
    <label
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        fontSize: 13.5,
        fontWeight: 600,
        color: "var(--text)",
        fontFamily: "var(--sbf-font-sans)",
        minHeight: 44,
        cursor: "pointer",
      }}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        style={{ width: 20, height: 20 }}
      />
      {rotulo}
    </label>
  );
}

/** Campo vazio vira `null`, não `0` nem `""` — "não informado" é um estado
 * (mesmo padrão de apps/web/src/app/dashboard/cards/card-form.tsx). */
export function numeroOuNulo(valor: string): number | null {
  const texto = valor.trim();
  if (texto === "") return null;
  const n = Number(texto.replace(",", "."));
  return Number.isFinite(n) ? n : null;
}

export function textoOuNulo(valor: string): string | null {
  const texto = valor.trim();
  return texto === "" ? null : texto;
}
