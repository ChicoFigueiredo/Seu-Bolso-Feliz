"use client";

/**
 * A identidade visual de um Centro, repetida igual em toda parte.
 *
 * O ponto colorido e o ícone são o que fazem "Edulingo" ser reconhecido no
 * cartão da lista, na linha de rateio e no gráfico sem ler o nome. Se cada
 * tela desenhasse o seu, a leitura deixaria de ser a mesma — por isso vivem
 * num componente só.
 */

import { Icon } from "@sbf/design-system";
import { corDoCentro, iconeDoCentro } from "./paleta";

export function PontoDoCentro({ cor, tamanho = 10 }: { cor: string | null; tamanho?: number }) {
  return (
    <span
      aria-hidden="true"
      style={{
        width: tamanho,
        height: tamanho,
        borderRadius: Math.max(3, Math.round(tamanho * 0.4)),
        background: corDoCentro(cor),
        flex: `0 0 ${String(tamanho)}px`,
      }}
    />
  );
}

export interface SeloDoCentroProps {
  cor: string | null;
  icone: string | null;
  tamanho?: number;
}

/** Quadrado arredondado com o ícone do centro sobre a cor dele, esmaecida. */
export function SeloDoCentro({ cor, icone, tamanho = 38 }: SeloDoCentroProps) {
  const pigmento = corDoCentro(cor);
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: tamanho,
        height: tamanho,
        flex: `0 0 ${String(tamanho)}px`,
        borderRadius: "var(--sbf-radius-md)",
        // A cor do centro em 18% de opacidade: identidade sem virar bloco
        // saturado ao lado de outros oito centros.
        background: `color-mix(in srgb, ${pigmento} 18%, transparent)`,
        color: pigmento,
      }}
    >
      <Icon name={iconeDoCentro(icone)} size={Math.round(tamanho * 0.55)} />
    </span>
  );
}
