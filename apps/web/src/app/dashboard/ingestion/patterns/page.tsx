import { listPatterns } from "@/app/actions/patterns";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { BookOpen, CheckCircle2, XCircle, AlertTriangle, Plus } from "lucide-react";
import Link from "next/link";

function successRate(success: number, feedback: number): number {
  const total = success + feedback;
  if (total === 0) return 100;
  return Math.round((success / total) * 100);
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  });
}

export default async function PatternsPage() {
  const { data: patterns, count } = await listPatterns({ limit: 100 });

  const active = patterns.filter((p) => p.is_active);
  const inactive = patterns.filter((p) => !p.is_active);
  const atRisk = active.filter(
    (p) => p.feedback_count > 1 && successRate(p.success_count, p.feedback_count) < 60,
  );

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Padrões Documentais</h1>
          <p className="text-muted-foreground">Padrões de extração aprendidos — {count} no total</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/ingestion/patterns/new">
            <Plus className="mr-2 size-4" />
            Novo Padrão
          </Link>
        </Button>
      </div>

      {/* Resumo */}
      <div className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Padrões Ativos</CardTitle>
            <CheckCircle2 className="size-4 text-green-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{active.length}</div>
            <p className="text-xs text-muted-foreground">em uso no pipeline</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Inativos</CardTitle>
            <XCircle className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{inactive.length}</div>
            <p className="text-xs text-muted-foreground">desativados ou versionados</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Em Risco</CardTitle>
            <AlertTriangle className="size-4 text-yellow-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{atRisk.length}</div>
            <p className="text-xs text-muted-foreground">taxa de sucesso {"<"} 60%</p>
          </CardContent>
        </Card>
      </div>

      {/* Tabela */}
      {patterns.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16 text-center">
            <BookOpen className="mb-4 size-12 text-muted-foreground/40" />
            <p className="text-sm font-medium">Nenhum padrão cadastrado</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Padrões são criados automaticamente quando você corrige uma extração, ou manualmente.
            </p>
            <Button asChild className="mt-4" variant="outline" size="sm">
              <Link href="/dashboard/ingestion/patterns/new">Criar primeiro padrão</Link>
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Nome</TableHead>
                <TableHead>Tipo</TableHead>
                <TableHead>Versão</TableHead>
                <TableHead className="text-center">Acertos</TableHead>
                <TableHead className="text-center">Correções</TableHead>
                <TableHead className="text-center">Taxa</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Atualizado</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {patterns.map((pattern) => {
                const rate = successRate(pattern.success_count, pattern.feedback_count);
                const isAtRisk = pattern.is_active && pattern.feedback_count > 1 && rate < 60;

                return (
                  <TableRow key={pattern.id}>
                    <TableCell className="font-medium">
                      <Link
                        href={`/dashboard/ingestion/patterns/${pattern.id}`}
                        className="hover:underline"
                      >
                        {pattern.name}
                      </Link>
                    </TableCell>
                    <TableCell>
                      <code className="rounded bg-muted px-1 py-0.5 text-xs">
                        {pattern.document_type}
                      </code>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className="text-xs">
                        v{pattern.version}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-center text-sm text-green-600">
                      {pattern.success_count}
                    </TableCell>
                    <TableCell className="text-center text-sm text-orange-600">
                      {pattern.feedback_count}
                    </TableCell>
                    <TableCell className="text-center">
                      <span
                        className={`text-sm font-medium ${
                          rate >= 80
                            ? "text-green-600"
                            : rate >= 60
                              ? "text-yellow-600"
                              : "text-red-600"
                        }`}
                      >
                        {rate}%
                      </span>
                    </TableCell>
                    <TableCell>
                      {pattern.is_active ? (
                        <Badge variant={isAtRisk ? "destructive" : "default"} className="text-xs">
                          {isAtRisk ? "Em risco" : "Ativo"}
                        </Badge>
                      ) : (
                        <Badge variant="secondary" className="text-xs">
                          Inativo
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatDate(pattern.updated_at)}
                    </TableCell>
                    <TableCell>
                      <Button asChild variant="ghost" size="sm">
                        <Link href={`/dashboard/ingestion/patterns/${pattern.id}`}>Ver</Link>
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </Card>
      )}
    </div>
  );
}
