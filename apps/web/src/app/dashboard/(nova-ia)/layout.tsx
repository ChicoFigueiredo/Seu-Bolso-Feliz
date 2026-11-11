import type { ReactNode } from "react";
import { SbfTheme } from "@sbf/design-system";

/**
 * Layout compartilhado das rotas novas da arquitetura de informação (Hoje,
 * Movimentações, Contas & Cartões, Dívidas, Recorrentes, Projetos & Centros,
 * Documentos, Inteligência, Conexões).
 *
 * Só existe para dar a estas rotas o escopo de tokens do @sbf/design-system
 * via <SbfTheme>. As rotas antigas, irmãs destas sob /dashboard (transactions,
 * liabilities, ...), ficam fora do grupo e continuam em shadcn puro — por
 * isso o <SbfTheme> vive aqui, e não no dashboard/layout.tsx, que é
 * compartilhado por todo mundo. `(nova-ia)` é um route group: organiza o app
 * router sem entrar na URL, então `/dashboard/hoje` continua `/dashboard/hoje`.
 */
export default function NovaIaLayout({ children }: { children: ReactNode }) {
  return (
    <SbfTheme theme="salvia" style={{ display: "flex", flex: "1 1 auto", minHeight: "100%" }}>
      {/*
        `minWidth: 0` não é enfeite. O <SbfTheme> é `display: flex`, então a
        página vira item de flex — e item de flex nasce com `min-width: auto`,
        que o impede de encolher abaixo do próprio conteúdo. O resultado é a
        página empurrar a largura do celular e a tela inteira ganhar rolagem
        horizontal: foi o que aconteceu em Movimentações, onde quatro cards de
        210px forçaram 508px dentro de uma coluna de 358px.
        Vale para as nove telas — nas outras só não aparecia porque o conteúdo
        cabia por acaso. Corrigido aqui, num lugar só, para que tela nova não
        tenha de lembrar disso.
      */}
      <div style={{ flex: "1 1 auto", minWidth: 0 }}>{children}</div>
    </SbfTheme>
  );
}
