"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
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
import { FileText, Loader2, ChevronRight, Sparkles } from "lucide-react";
import Link from "next/link";
import { getSourceDocuments } from "@/app/actions/ingestion";
import type { SourceDocument } from "@sbf/shared-types";
import { useChatContext } from "@/contexts/chat-context";
import { CONFIDENCE_THRESHOLD } from "@/components/ai-field-badge";
import { DeleteDocumentButton } from "@/components/delete-document-button";
import { DocumentUploadDnD } from "@/components/document-upload-dnd";

// ─── Helpers de exibição ────────────────────────────────────────────────────

const STATUS_LABEL: Record<string, string> = {
  new: "Novo",
  queued: "Na fila",
  processing: "Processando",
  processed: "Processado",
  pending_review: "Aguardando revisão",
  approved: "Aprovado",
  deleted: "Removido",
  failed: "Erro",
};

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  new: "outline",
  queued: "secondary",
  processing: "secondary",
  processed: "default",
  pending_review: "secondary",
  approved: "default",
  deleted: "outline",
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

  // S4-009
  const { setDrawerOpen, setPendingMessage } = useChatContext();

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

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Documentos</h1>
        <p className="text-muted-foreground">
          Todos os documentos ingeridos — Gmail, upload manual e arquivo local.
        </p>
      </div>

      {/* Upload manual — DnD */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">Enviar Documento</CardTitle>
          <CardDescription>
            O arquivo será enviado para o pipeline de ingestão e processado automaticamente.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <DocumentUploadDnD onSuccess={loadDocs} />
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
                  <TableHead className="w-8">IA</TableHead>
                  <TableHead className="w-8" />
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
                      {doc.created_at ? new Date(doc.created_at).toLocaleDateString("pt-BR") : "—"}
                    </TableCell>
                    {/* S4-009: botão Explicar para docs com baixa confiança */}
                    <TableCell>
                      {(() => {
                        const meta = (doc.metadata ?? {}) as Record<string, unknown>;
                        const confidence = meta["confidence"] as number | undefined;
                        if (confidence == null || confidence >= CONFIDENCE_THRESHOLD) return null;
                        return (
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="h-7 px-2 text-xs text-primary"
                            title={`Confiança: ${Math.round(confidence * 100)}%`}
                            onClick={() => {
                              setPendingMessage(
                                `Explique por que o documento "${doc.filename}" foi classificado com confiança baixa ` +
                                  `(${Math.round(confidence * 100)}%). ID: ${doc.id}.`,
                              );
                              setDrawerOpen(true);
                            }}
                          >
                            <Sparkles className="mr-1 h-3 w-3" />
                            Explicar
                          </Button>
                        );
                      })()}
                    </TableCell>
                    <TableCell>
                      <Link href={`/dashboard/documents/${doc.id}`}>
                        <ChevronRight className="size-4 text-muted-foreground" />
                      </Link>
                    </TableCell>
                    <TableCell onClick={(e) => e.stopPropagation()}>
                      <DeleteDocumentButton
                        documentId={doc.id}
                        filename={doc.filename}
                        triggerVariant="icon"
                        onDeleted={loadDocs}
                      />
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
