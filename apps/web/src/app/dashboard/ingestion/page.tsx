import {
  getIngestionStats,
  getIngestionRuns,
  getSourceDocuments,
  getIngestionJobs,
} from "@/app/actions/ingestion";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/status-badge";
import { UploadDocuments } from "@/components/upload-documents";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { FileText, AlertTriangle, CheckCircle2, Clock, FileSearch, Activity } from "lucide-react";
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

export default async function IngestionPage() {
  const [stats, runs, { data: recentDocs }, recentJobs] = await Promise.all([
    getIngestionStats(),
    getIngestionRuns(5),
    getSourceDocuments({ limit: 10 }),
    getIngestionJobs({ limit: 10 }),
  ]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Ingestão</h1>
          <p className="text-muted-foreground">Visão geral do pipeline de ingestão de documentos</p>
        </div>
        <Link
          href="/dashboard/ingestion/logs"
          className="inline-flex items-center gap-2 rounded-md bg-secondary px-3 py-2 text-sm font-medium text-secondary-foreground hover:bg-secondary/80"
        >
          <Activity className="size-4" />
          Ver Logs
        </Link>
      </div>

      {/* Stats Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-5">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Documentos</CardTitle>
            <FileText className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.totalDocuments}</div>
            <p className="text-xs text-muted-foreground">total no sistema</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Aguardando Revisão</CardTitle>
            <Clock className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.pendingReview}</div>
            <p className="text-xs text-muted-foreground">jobs pendentes</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Rascunhos</CardTitle>
            <FileSearch className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.totalDrafts}</div>
            <p className="text-xs text-muted-foreground">total gerados</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Pendentes</CardTitle>
            <CheckCircle2 className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.pendingDrafts}</div>
            <p className="text-xs text-muted-foreground">drafts para revisar</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Erros (24h)</CardTitle>
            <AlertTriangle className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.recentErrors}</div>
            <p className="text-xs text-muted-foreground">últimas 24 horas</p>
          </CardContent>
        </Card>
      </div>

      {/* Upload Manual */}
      <UploadDocuments />

      {/* Recent Documents */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle>Documentos Recentes</CardTitle>
            <CardDescription>{recentDocs.length} documento(s) mais recentes</CardDescription>
          </div>
          <Link
            href="/dashboard/ingestion/documents"
            className="text-sm font-medium text-primary hover:underline"
          >
            Ver todos →
          </Link>
        </CardHeader>
        <CardContent className="p-0">
          {recentDocs.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <FileText className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum documento ingerido ainda.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Arquivo</TableHead>
                  <TableHead>Origem</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Data</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {recentDocs.map((doc) => (
                  <TableRow key={doc.id} className="cursor-pointer hover:bg-muted/50">
                    <TableCell className="max-w-[200px] truncate font-medium">
                      <Link href={`/dashboard/ingestion/documents/${doc.id}`}>{doc.filename}</Link>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      <span className="capitalize">
                        {doc.origin_type?.replace("_", " ") ?? "—"}
                      </span>
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={doc.status ?? ""} className="text-xs" />
                    </TableCell>
                    <TableCell className="text-sm">{formatDate(doc.created_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Recent Jobs */}
      <Card>
        <CardHeader>
          <CardTitle>Jobs Recentes</CardTitle>
          <CardDescription>Últimos {recentJobs.length} job(s) de ingestão</CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {recentJobs.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <Activity className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum job de ingestão executado.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>ID</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Etapa</TableHead>
                  <TableHead>Criado em</TableHead>
                  <TableHead>Atualizado em</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {recentJobs.map((job) => (
                  <TableRow key={job.id}>
                    <TableCell className="font-mono text-xs">{job.id.slice(0, 8)}…</TableCell>
                    <TableCell>
                      <StatusBadge status={job.status ?? ""} className="text-xs" />
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {job.status ?? "—"}
                    </TableCell>
                    <TableCell className="text-sm">{formatDate(job.created_at)}</TableCell>
                    <TableCell className="text-sm">{formatDate(job.updated_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Recent Runs */}
      {runs.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Execuções Recentes</CardTitle>
            <CardDescription>Últimas {runs.length} execução(ões) do pipeline</CardDescription>
          </CardHeader>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>ID</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Início</TableHead>
                  <TableHead>Fim</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {runs.map((run) => (
                  <TableRow key={run.id}>
                    <TableCell className="font-mono text-xs">{run.id.slice(0, 8)}…</TableCell>
                    <TableCell>
                      <StatusBadge status={run.status ?? ""} className="text-xs" />
                    </TableCell>
                    <TableCell className="text-sm">{formatDate(run.started_at)}</TableCell>
                    <TableCell className="text-sm">{formatDate(run.completed_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
