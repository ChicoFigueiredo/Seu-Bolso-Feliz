import type { CSSProperties, ReactNode } from "react";
import { SBF_DARK_THEMES, type SbfThemeName, baseType } from "./tokens";

export interface SbfThemeProps {
  /** Tema aplicado. Padrão `salvia`, o claro canonico. */
  theme?: SbfThemeName;
  /**
   * Preenche a altura da viewport e pinta o fundo do tema.
   * Ligue na raiz do app; deixe desligado ao envolver um trecho isolado.
   */
  fullBleed?: boolean;
  className?: string;
  style?: CSSProperties;
  children?: ReactNode;
}

/**
 * Raiz do design system: declara o tema e liga a tipografia.
 *
 * Todo componente do Seu Bolso Feliz le cor de `var(--token)`, e quem define
 * esses tokens e o atributo `data-sbf-theme` que este componente escreve.
 * Sem ele os componentes ainda renderizam (o `:root` de tokens.css aplica
 * salvia como padrão), mas trocar de tema deixa de funcionar.
 *
 * Importe `@sbf/design-system/tokens.css` uma vez no app.
 *
 * ```tsx
 * <SbfTheme theme="salvia" fullBleed>
 *   <AppSidebar ... />
 * </SbfTheme>
 * ```
 */
export function SbfTheme({
  theme = "salvia",
  fullBleed = false,
  className,
  style,
  children,
}: SbfThemeProps) {
  return (
    <div
      data-sbf-theme={theme}
      className={["sbf-root", className].filter(Boolean).join(" ")}
      style={{
        ...baseType,
        colorScheme: SBF_DARK_THEMES.includes(theme) ? "dark" : "light",
        background: "var(--bg)",
        fontSize: 14,
        lineHeight: 1.45,
        ...(fullBleed ? { minHeight: "100vh", width: "100%" } : {}),
        ...style,
      }}
    >
      {children}
    </div>
  );
}
