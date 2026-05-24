import { Badge } from "@/components/ui/badge";

const statusConfig: Record<
  string,
  { label: string; variant: "default" | "secondary" | "destructive" | "outline" }
> = {
  // Ingestion job statuses
  discovered: { label: "Descoberto", variant: "outline" },
  downloaded: { label: "Baixado", variant: "outline" },
  hashed: { label: "Hash calculado", variant: "outline" },
  queued: { label: "Na fila", variant: "outline" },
  parsed: { label: "Parseado", variant: "secondary" },
  classified: { label: "Classificado", variant: "secondary" },
  reconciled: { label: "Conciliado", variant: "secondary" },
  drafted: { label: "Rascunho", variant: "default" },
  pending_review: { label: "Aguardando revisão", variant: "default" },
  approved: { label: "Aprovado", variant: "secondary" },
  posted: { label: "Lançado", variant: "secondary" },
  failed: { label: "Falhou", variant: "destructive" },
  skipped: { label: "Ignorado", variant: "outline" },
  // Draft record statuses
  pending: { label: "Pendente", variant: "default" },
  rejected: { label: "Rejeitado", variant: "destructive" },
  edited: { label: "Editado", variant: "outline" },
  // Draft batch statuses
  open: { label: "Aberto", variant: "outline" },
  reviewing: { label: "Em revisão", variant: "default" },
  partial: { label: "Parcial", variant: "outline" },
  // Ingestion run statuses
  running: { label: "Executando", variant: "default" },
  completed: { label: "Completo", variant: "secondary" },
};

export function StatusBadge({ status, className }: { status: string; className?: string }) {
  const config = statusConfig[status] ?? { label: status, variant: "outline" as const };
  return (
    <Badge variant={config.variant} className={className}>
      {config.label}
    </Badge>
  );
}
