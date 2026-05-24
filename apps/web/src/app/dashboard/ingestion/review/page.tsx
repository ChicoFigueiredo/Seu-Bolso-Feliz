import { getDraftBatches } from "@/app/actions/ingestion";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/status-badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { FileSearch } from "lucide-react";
import Link from "next/link";

function formatDate(dateStr: string | null): string {
  if (!dateStr) return "—";
  return new Date(dateStr).toLocaleDateString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default async function ReviewPage() {
  const batches = await getDraftBatches({ limit: 50 });

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Revisão de Rascunhos</h1>
        <p className="text-muted-foreground">
          Revise e aprove os rascunhos gerados pelo pipeline de ingestão
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Lotes de Rascunhos</CardTitle>
          <CardDescription>{batches.length} lote(s)</CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {batches.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <FileSearch className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum lote de rascunhos encontrado.</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Os lotes são criados automaticamente quando documentos são ingeridos.
              </p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Total</TableHead>
                  <TableHead>Aprovados</TableHead>
                  <TableHead>Rejeitados</TableHead>
                  <TableHead>Criado em</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {batches.map((batch) => (
                  <TableRow key={batch.id}>
                    <TableCell className="font-medium">
                      {batch.name ?? `Lote ${batch.id.slice(0, 8)}`}
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={batch.status} className="text-xs" />
                    </TableCell>
                    <TableCell>{batch.total_drafts ?? 0}</TableCell>
                    <TableCell className="text-green-600">{batch.approved_count ?? 0}</TableCell>
                    <TableCell className="text-red-600">{batch.rejected_count ?? 0}</TableCell>
                    <TableCell className="text-sm">{formatDate(batch.created_at)}</TableCell>
                    <TableCell className="text-right">
                      <Link
                        href={`/dashboard/ingestion/review/${batch.id}`}
                        className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                      >
                        Revisar
                      </Link>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
