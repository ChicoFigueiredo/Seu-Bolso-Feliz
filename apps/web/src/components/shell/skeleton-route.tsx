import { EmptyState } from "@sbf/design-system";

export interface SkeletonRouteProps {
  /** Glifo Material Symbols — em geral, o mesmo ícone do item de navegação. */
  icon: string;
  title?: string;
  /** Uma frase dizendo o que vem por aí, na linguagem do CONTEXT.md — nunca
   * jargão de pipeline ("ingestion job", "draft batch"). */
  description: string;
}

/**
 * Corpo padrão das rotas da arquitetura de informação nova que ainda não
 * ganharam conteúdo — outro time preenche depois. O shell (sidebar/TopBar)
 * já mostra o nome da seção; isto só confirma que a área existe e adianta o
 * que vem por aí, em vez de um "nada aqui" sem contexto.
 */
export function SkeletonRoute({ icon, title = "Em construção", description }: SkeletonRouteProps) {
  return (
    <div
      style={{
        display: "flex",
        minHeight: "100%",
        alignItems: "center",
        justifyContent: "center",
        padding: "var(--sbf-space-6) 0",
      }}
    >
      <EmptyState icon={icon} title={title} description={description} />
    </div>
  );
}
