"use client";

/**
 * A folha inferior desta seção, posicionada na tela.
 *
 * O `Sheet` do @sbf/design-system desenha o cartão, a alça e o botão de
 * fechar, mas é `position: relative` — ele se ancora na base do container em
 * que for colocado, não na base da janela. Este componente é esse container:
 * o véu escuro clicável, a tecla Escape, o travamento da rolagem de fundo e o
 * corte de altura para o botão de salvar nunca ficar fora do alcance numa
 * tela baixa.
 *
 * O véu é `<button>` e não `<div onClick>` de propósito: assim tocar fora
 * fecha, e quem navega por teclado também alcança essa saída.
 */

import { useEffect, type ReactNode } from "react";
import { Sheet } from "@sbf/design-system";
import styles from "./centros.module.css";

export interface FolhaProps {
  titulo: string;
  aberta: boolean;
  onFechar: () => void;
  children: ReactNode;
}

export function Folha({ titulo, aberta, onFechar, children }: FolhaProps) {
  useEffect(() => {
    if (!aberta) return undefined;

    const aoTeclar = (evento: KeyboardEvent) => {
      if (evento.key === "Escape") onFechar();
    };
    document.addEventListener("keydown", aoTeclar);

    const overflowAnterior = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      document.removeEventListener("keydown", aoTeclar);
      document.body.style.overflow = overflowAnterior;
    };
  }, [aberta, onFechar]);

  if (!aberta) return null;

  return (
    <div className={styles.overlayFolha}>
      <button
        type="button"
        aria-label="Fechar"
        onClick={onFechar}
        style={{
          position: "absolute",
          inset: 0,
          border: 0,
          padding: 0,
          background: "rgba(24,35,31,.35)",
          cursor: "pointer",
        }}
      />
      {/* `withScrim={false}`: o véu acima é o nosso, para poder ser clicável. */}
      <Sheet title={titulo} onClose={onFechar} withScrim={false} style={{ zIndex: 1 }}>
        <div className={styles.corpoFolha} style={{ display: "grid", gap: 14 }}>
          {children}
        </div>
      </Sheet>
    </div>
  );
}
