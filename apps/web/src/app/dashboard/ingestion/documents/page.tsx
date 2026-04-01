"use client";

import { useState, useEffect, useCallback } from "react";
import { getSourceDocuments } from "@/app/actions/ingestion";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { FileText, Search, ChevronLeft, ChevronRight, Filter, FileSearch } from "lucide-react";
import Link from "next/link";
import type { SourceDocument } from "@sbf/shared-types";

const PAGE_SIZE = 20;

const STATUS_OPTIONS = [
  { value: "", label: "Todos os status" },
  { value: "new", label: "Novo" },
  { value: "processing", label: "Processando" },
  { value: "processed", label: "Processado" },
  { value: "failed", label: "Erro" },
];

const ORIGIN_OPTIONS = [
  { value: "", label: "Todas as origens" },
  { value: "manual_upload", label: "Upload manual" },
  { value: "gmail", label: "Gmail" },
  { value: "local_file", label: "Arquivo local" },
];

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

function formatFileSize(bytes: number | null): string {
  if (!bytes) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function DocumentsListPage() {
  const [documents, setDocuments] = useState<SourceDocument[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(true);

  // Filters
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [originType, setOriginType] = useState("");
  const [showFilters, setShowFilters] = useState(false);

  const fetchDocuments = useCallback(async () => {
    setLoading(true);
    try {
      const result = await getSourceDocuments({
        search: search || undefined,
        status: status || undefined,
        originType: originType || undefined,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      });
      setDocuments(result.data);
      setTotalCount(result.count);
    } finally {
      setLoading(false);
    }
  }, [search, status, originType, page]);

  useEffect(() => {
    fetchDocuments();
  }, [fetchDocuments]);

  function handleSearch() {
    setPage(0);
    fetchDocuments();
  }

  const totalPages = Math.ceil(totalCount / PAGE_SIZE);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Documentos</h1>
          <p className="text-muted-foreground">{totalCount} documento(s) no sistema</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => setShowFilters(!showFilters)}>
          <Filter className="mr-2 size-4" />
          Filtros
        </Button>
      </div>

      {/* Filters */}
      {showFilters && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium">Filtros</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-3">
              <div className="flex-1 min-w-[200px]">
                <Input
                  placeholder="Buscar por nome do arquivo..."
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                />
              </div>
              <Select
                value={status}
                onValueChange={(v) => {
                  setStatus(v);
                  setPage(0);
                }}
              >
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="Status" />
                </SelectTrigger>
                <SelectContent>
                  {STATUS_OPTIONS.map((opt) => (
                    <SelectItem key={opt.value || "__all"} value={opt.value || "__all"}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                value={originType}
                onValueChange={(v) => {
                  setOriginType(v);
                  setPage(0);
                }}
              >
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="Origem" />
                </SelectTrigger>
                <SelectContent>
                  {ORIGIN_OPTIONS.map((opt) => (
                    <SelectItem key={opt.value || "__all"} value={opt.value || "__all"}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button variant="secondary" onClick={handleSearch}>
                <Search className="mr-2 size-4" />
                Buscar
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Documents Table */}
      <Card>
        <CardContent className="p-0">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <div className="size-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
            </div>
          ) : documents.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <FileSearch className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum documento encontrado.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Arquivo</TableHead>
                  <TableHead>Origem</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Tamanho</TableHead>
                  <TableHead>Data</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {documents.map((doc) => (
                  <TableRow key={doc.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <FileText className="size-4 text-muted-foreground shrink-0" />
                        <span className="truncate max-w-[300px] font-medium">{doc.filename}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <span className="text-xs capitalize">
                        {doc.origin_type?.replace("_", " ") ?? "—"}
                      </span>
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={doc.status} className="text-xs" />
                    </TableCell>
                    <TableCell className="text-sm">{formatFileSize(doc.file_size_bytes)}</TableCell>
                    <TableCell className="text-sm">{formatDate(doc.created_at)}</TableCell>
                    <TableCell className="text-right">
                      <Link
                        href={`/dashboard/ingestion/documents/${doc.id}`}
                        className="text-sm font-medium text-primary hover:underline"
                      >
                        Ver detalhes
                      </Link>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            Página {page + 1} de {totalPages}
          </p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={page === 0}
              onClick={() => setPage((p) => p - 1)}
            >
              <ChevronLeft className="mr-1 size-4" />
              Anterior
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={page >= totalPages - 1}
              onClick={() => setPage((p) => p + 1)}
            >
              Próxima
              <ChevronRight className="ml-1 size-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
