import type { CSSProperties } from "react";

export interface IconProps {
  /**
   * Nome do glifo em Material Symbols Rounded — `savings`, `check_circle`,
   * `auto_awesome`, `chevron_right`, `expand_more`, `rule`, `photo_camera`.
   */
  name: string;
  /** Lado do glifo em px. Padrão 20. */
  size?: number;
  /** Cor. Padrão herda do contexto. */
  color?: string;
  /** Glifo preenchido em vez de contornado. */
  filled?: boolean;
  style?: CSSProperties;
}

/**
 * Ícone da familia Material Symbols Rounded.
 *
 * A fonte vem no tokens.css. O texto do span e o próprio nome do glifo, então
 * um nome inexistente aparece como palavra — se você ver "savings" escrito na
 * tela, a fonte de ícones não carregou.
 */
export function Icon({ name, size = 20, color, filled = false, style }: IconProps) {
  return (
    <span
      className="sbf-icon"
      aria-hidden="true"
      style={{
        fontFamily: "var(--sbf-font-icon)",
        fontSize: size,
        lineHeight: 1,
        color,
        fontVariationSettings: `'opsz' 24, 'wght' 400, 'FILL' ${filled ? 1 : 0}, 'GRAD' 0`,
        ...style,
      }}
    >
      {name}
    </span>
  );
}
