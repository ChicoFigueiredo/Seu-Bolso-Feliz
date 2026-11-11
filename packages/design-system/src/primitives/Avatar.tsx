import type { CSSProperties } from "react";

export interface AvatarProps {
  /** Nome completo — as iniciais são derivadas dele. */
  name: string;
  size?: number;
  style?: CSSProperties;
}

function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/**
 * Iniciais do usuário em disco verde.
 *
 * Não aceita foto por escolha: o produto e de uso pessoal e um avatar
 * fotografico só acrescentaria ruido ao cabecalho.
 */
export function Avatar({ name, size = 40, style }: AvatarProps) {
  return (
    <div
      title={name}
      style={{
        fontFamily: "var(--sbf-font-sans)",
        width: size,
        height: size,
        flex: `0 0 ${size}px`,
        borderRadius: "var(--sbf-radius-pill)",
        background: "var(--hero-bg)",
        color: "var(--hero-accent)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontWeight: 800,
        fontSize: Math.round(size * 0.35),
        ...style,
      }}
    >
      {initialsOf(name)}
    </div>
  );
}
