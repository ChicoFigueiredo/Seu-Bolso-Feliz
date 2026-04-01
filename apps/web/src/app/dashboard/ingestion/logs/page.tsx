"use client";

import { useState, useEffect, useCallback } from "react";
import { getIngestionLogs } from "@/app/actions/ingestion";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { Activity, ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import type { IngestionLog } from "@sbf/shared-types";

const PAGE_SIZE = 50;

const levelVariant: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  debug: "outline",
  info: "secondary",
  warn: "default",
  error: "destructive",
};

const levelEmoji: Record<string, string> = {
  debug: "🔍",
  info: "ℹ️",
  warn: "⚠️",
  error: "❌",
};

function formatDate(dateStr: string | null): string {
  if (!dateStr) return "—";
  return new Date(dateStr).toLocaleDateString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export default function IngestionLogsPage() {
  const [logs, setLogs] = useState<IngestionLog[]>([]);
  const [count, setCount] = useState(0);
  const [level, setLevel] = useState<string>("all");
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const result = await getIngestionLogs({
        level: level === "all" ? undefined : level,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      });
      setLogs(result.data);
      setCount(result.count);
    } finally {
      setLoading(false);
    }
  }, [level, page]);

  useEffect(() => {
    loadLogs();
  }, [loadLogs]);

  const totalPages = Math.ceil(count / PAGE_SIZE);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Logs de Ingestão</h1>
        <p className="text-muted-foreground">Registros de execução do pipeline de ingestão</p>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Registros</CardTitle>
              <CardDescription>{count} log(s) encontrado(s)</CardDescription>
            </div>
            <div className="flex items-center gap-3">
              <Select
                value={level}
                onValueChange={(v) => {
                  setLevel(v);
                  setPage(0);
                }}
              >
                <SelectTrigger className="w-[140px]">
                  <SelectValue placeholder="Nível" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">Todos</SelectItem>
                  <SelectItem value="debug">Debug</SelectItem>
                  <SelectItem value="info">Info</SelectItem>
                  <SelectItem value="warn">Warn</SelectItem>
                  <SelectItem value="error">Error</SelectItem>
                </SelectContent>
              </Select>
              <Button variant="outline" size="sm" onClick={loadLogs} disabled={loading}>
                <RefreshCw className={`size-4 ${loading ? "animate-spin" : ""}`} />
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {logs.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <Activity className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">
                {loading ? "Carregando..." : "Nenhum log encontrado."}
              </p>
            </div>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-[100px]">Nível</TableHead>
                    <TableHead>Mensagem</TableHead>
                    <TableHead className="w-[100px]">Job</TableHead>
                    <TableHead className="w-[100px]">Run</TableHead>
                    <TableHead className="w-[150px]">Data</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {logs.map((log) => (
                    <>
                      <TableRow
                        key={log.id}
                        className="cursor-pointer"
                        onClick={() => setExpandedId(expandedId === log.id ? null : log.id)}
                      >
                        <TableCell>
                          <Badge variant={levelVariant[log.level] ?? "outline"} className="text-xs">
                            {levelEmoji[log.level]} {log.level}
                          </Badge>
                        </TableCell>
                        <TableCell className="max-w-[400px] truncate text-sm">
                          {log.message}
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {log.job_id ? log.job_id.slice(0, 8) + "…" : "—"}
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {log.run_id ? log.run_id.slice(0, 8) + "…" : "—"}
                        </TableCell>
                        <TableCell className="text-sm">{formatDate(log.created_at)}</TableCell>
                      </TableRow>
                      {expandedId === log.id && log.details && (
                        <TableRow key={`${log.id}-details`}>
                          <TableCell colSpan={5}>
                            <pre className="max-h-60 overflow-auto rounded bg-muted p-3 text-xs">
                              {JSON.stringify(log.details, null, 2)}
                            </pre>
                          </TableCell>
                        </TableRow>
                      )}
                    </>
                  ))}
                </TableBody>
              </Table>

              {/* Pagination */}
              {totalPages > 1 && (
                <div className="flex items-center justify-between border-t px-4 py-3">
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
                      <ChevronLeft className="size-4" />
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={page >= totalPages - 1}
                      onClick={() => setPage((p) => p + 1)}
                    >
                      <ChevronRight className="size-4" />
                    </Button>
                  </div>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
