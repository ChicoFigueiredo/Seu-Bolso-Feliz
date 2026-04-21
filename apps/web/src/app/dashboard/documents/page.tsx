"use client";

import { useState, useEffect, useTransition } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import { FileUp, FileText, Loader2, ChevronRight } from "lucide-react";
import Link from "next/link";
import { getSourceDocuments, uploadDocument } from "@/app/actions/ingestion";
import type { SourceDocument } from "@sbf/shared-types";

// ─── Helpers de exibição ────────────────────────────────────────────────────

const STATUS_LABEL: Record<string, string> = {
  new: "Novo",
  queued: "Na fila",
  processing: "Processando",
  processed: "Processado",
  pending_review: "Aguardando revisão",
  approved: "Aprovado",
  failed: "Erro",
};

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  new: "outline",
  queued: "secondary",
  processing: "secondary",
  processed: "default",
  pending_review: "secondary",
  approved: "default",
  failed: "destructive",
};

const ORIGIN_LABEL: Record<string, string> = {
  gmail: "Gmail",
  local_file: "Arquivo local",
  manual_upload: "Upload manual",
};

function formatFileSize(bytes: number | null): string {
  if (!bytes) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ─── Componente ─────────────────────────────────────────────────────────────

export default function DocumentsPage() {
  const [docs, setDocs] = useState<SourceDocument[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [isPending, startTransition] = useTransition();

  // Filtros
  const [filterStatus, setFilterStatus] = useState("all");
  const [filterOrigin, setFilterOrigin] = useState("all");
  const [filterSearch, setFilterSearch] = useState("");

  useEffect(() => {
    loadDocs();
  }, [filterStatus, filterOrigin]);

  async function loadDocs() {
    setLoading(true);
    try {
      const result = await getSourceDocuments({
        status: filterStatus !== "all" ? filterStatus : undefined,
        originType: filterOrigin !== "all" ? filterOrigin : undefined,
        search: filterSearch || undefined,
        limit: 50,
      });
      setDocs(result.data);
      setTotal(result.count);
    } catch {
      toast.error("Erro ao carregar documentos");
    } finally {
      setLoading(false);
    }
  }

  function handleFilterSearch(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    loadDocs();
  }

  async function handleUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    startTransition(async () => {
      try {
        const formData = new FormData();
        formData.append("file", file);
        await uploadDocument(formData);
        toast.success("Documento enviado", {
          description: "O pipeline de ingestão foi iniciado.",
        });
        loadDocs();
      } catch (err) {
        toast.error("Erro no upload", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });

    e.target.value = "";
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Documentos</h1>
        <p className="text-muted-foreground">
          Todos os documentos ingeridos — Gmail, upload manual e arquivo local.
        </p>
      </div>

      {/* Upload manual */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileUp className="size-5" />
            Enviar Documento
          </CardTitle>
          <CardDescription>
            O arquivo será enviado para o pipeline de ingestão e processado automaticamente.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap items-end gap-4">
            <div className="space-y-2">
              <Label htmlFor="file-upload">Arquivo</Label>
              <Input
                id="file-upload"
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.xlsx,.csv,.doc,.docx,.ofx,.qif"
                onChange={handleUpload}
                disabled={isPending}
              />
            </div>
            {isPending && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                Enviando…
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Filtros */}
      <Card>
        <CardContent className="pt-4">
          <form onSubmit={handleFilterSearch} className="flex flex-wrap gap-3">
            <Select value={filterStatus} onValueChange={setFilterStatus}>
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder="Status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">Todos os status</SelectItem>
                <SelectItem value="new">Novo</SelectItem>
                <SelectItem value="queued">Na fila</SelectItem>
                <SelectItem value="processed">Processado</SelectItem>
                <SelectItem value="pending_review">Aguardando revisão</SelectItem>
                <SelectItem value="approved">Aprovado</SelectItem>
                <SelectItem value="failed">Erro</SelectItem>
              </SelectContent>
            </Select>

            <Select value={filterOrigin} onValueChange={setFilterOrigin}>
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder="Origem" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">Todas as origens</SelectItem>
                <SelectItem value="gmail">Gmail</SelectItem>
                <SelectItem value="manual_upload">Upload manual</SelectItem>
                <SelectItem value="local_file">Arquivo local</SelectItem>
              </SelectContent>
            </Select>

            <div className="flex flex-1 gap-2">
              <Input
                placeholder="Buscar por nome…"
                value={filterSearch}
                onChange={(e) => setFilterSearch(e.target.value)}
                className="max-w-xs"
              />
              <Button type="submit" variant="outline" size="sm">
                Buscar
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      {/* Lista */}
      <Card>
        <CardHeader>
          <CardTitle>Documentos</CardTitle>
          <CardDescription>{loading ? "Carregando…" : `${total} documento(s)`}</CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="size-8 animate-spin text-muted-foreground" />
            </div>
          ) : docs.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <FileText className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum documento encontrado.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Origem</TableHead>
                  <TableHead>Fornecedor</TableHead>
                  <TableHead>Status pipeline</TableHead>
                  <TableHead>Tamanho</TableHead>
                  <TableHead>Data</TableHead>
                  <TableHead className="w-8" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {docs.map((doc) => (
                  <TableRow key={doc.id} className="cursor-pointer hover:bg-muted/50">
                    {/* S2-005 — link para /dashboard/documents/[id] */}
                    <TableCell className="max-w-[220px]">
                      <Link
                        href={`/dashboard/documents/${doc.id}`}
                        className="block truncate font-medium hover:underline"
                      >
                        {doc.filename}
                      </Link>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {ORIGIN_LABEL[doc.origin_type] ?? doc.origin_type}
                    </TableCell>

                    {/* S2-006 — coluna Fornecedor */}
                    <TableCell className="text-sm">
                      {(doc as SourceDocument & { supplier_name_raw?: string })
                        .supplier_name_raw ?? (
                        <span className="text-muted-foreground">Não identificado</span>
                      )}
                    </TableCell>

                    {/* S2-006 — coluna Status pipeline */}
                    <TableCell>
                      <Badge variant={STATUS_VARIANT[doc.status] ?? "outline"}>
                        {STATUS_LABEL[doc.status] ?? doc.status}
                      </Badge>
                    </TableCell>

                    <TableCell className="text-sm">
                      {formatFileSize(doc.file_size_bytes ?? null)}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(doc.created_at).toLocaleDateString("pt-BR")}
                    </TableCell>
                    <TableCell>
                      <Link href={`/dashboard/documents/${doc.id}`}>
                        <ChevronRight className="size-4 text-muted-foreground" />
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
