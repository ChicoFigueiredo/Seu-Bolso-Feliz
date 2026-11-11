"use client";

import { ThemeProvider } from "next-themes";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";

/**
 * O produto é claro por decisão: o tema do design system está fixo em
 * `salvia`, e o manifest declara fundo claro.
 *
 * Deixar esta camada seguir o modo escuro do sistema não deixava o app
 * escuro — deixava só a **moldura** escura. O `<main>` tem 24px de padding, e
 * o fundo do `body` aparecia nessa faixa em volta do conteúdo claro. Era a
 * moldura preta que dava para ver em qualquer tela do painel.
 *
 * Quando existir modo escuro de verdade, ele troca o tema do design system
 * (`noite` ou `grafite`) e `forcedTheme` volta a ser `system`.
 */
export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider attribute="class" forcedTheme="light" disableTransitionOnChange>
      <TooltipProvider>
        {children}
        <Toaster richColors position="top-right" />
      </TooltipProvider>
    </ThemeProvider>
  );
}
