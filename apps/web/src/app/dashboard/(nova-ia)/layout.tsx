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
      {children}
    </SbfTheme>
  );
}
