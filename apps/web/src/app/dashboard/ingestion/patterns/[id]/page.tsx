import { getPattern, deactivatePattern, reactivatePattern } from "@/app/actions/patterns";
import { notFound } from "next/navigation";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ArrowLeft, CheckCircle2, XCircle, AlertTriangle, Clock } from "lucide-react";
import Link from "next/link";

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function successRate(success: number, feedback: number): number {
  const total = success + feedback;
  if (total === 0) return 100;
  return Math.round((success / total) * 100);
}

const feedbackLabels: Record<string, { label: string; className: string }> = {
  correct: { label: "Correto", className: "text-green-600" },
  incorrect: { label: "Incorreto", className: "text-red-600" },
  partial: { label: "Parcial", className: "text-yellow-600" },
  improved: { label: "Melhorado", className: "text-blue-600" },
};

interface Props {
  params: Promise<{ id: string }>;
}

export default async function PatternDetailPage({ params }: Props) {
  const { id } = await params;
  const { pattern, feedback } = await getPattern(id);

  if (!pattern) notFound();

  const rate = successRate(pattern.success_count, pattern.feedback_count);
  const isAtRisk = pattern.is_active && pattern.feedback_count > 1 && rate < 60;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <Button variant="ghost" size="sm" asChild>
            <Link href="/dashboard/ingestion/patterns">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <div>
            <h1 className="text-2xl font-bold tracking-tight">{pattern.name}</h1>
            <div className="mt-1 flex items-center gap-2">
              <code className="rounded bg-muted px-1.5 py-0.5 text-xs">
                {pattern.document_type}
              </code>
              <Badge variant="outline" className="text-xs">
                v{pattern.version}
              </Badge>
              {pattern.is_active ? (
                <Badge variant={isAtRisk ? "destructive" : "default"} className="text-xs">
                  {isAtRisk ? "Em risco" : "Ativo"}
                </Badge>
              ) : (
                <Badge variant="secondary" className="text-xs">
                  Inativo
                </Badge>
              )}
            </div>
          </div>
        </div>

        {/* Ações */}
        <div className="flex gap-2">
          {pattern.is_active ? (
            <form
              action={async () => {
                "use server";
                await deactivatePattern(id);
              }}
            >
              <Button variant="outline" size="sm" type="submit">
                <XCircle className="mr-2 size-4" />
                Desativar
              </Button>
            </form>
          ) : (
            <form
              action={async () => {
                "use server";
                await reactivatePattern(id);
              }}
            >
              <Button variant="outline" size="sm" type="submit">
                <CheckCircle2 className="mr-2 size-4" />
                Reativar
              </Button>
            </form>
          )}
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-3">
        {/* Métricas */}
        <div className="space-y-4 lg:col-span-1">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium">Desempenho</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Taxa de acerto</span>
                <span
                  className={`text-lg font-bold ${
                    rate >= 80 ? "text-green-600" : rate >= 60 ? "text-yellow-600" : "text-red-600"
                  }`}
                >
                  {rate}%
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1 text-sm text-muted-foreground">
                  <CheckCircle2 className="size-3 text-green-500" /> Acertos
                </span>
                <span className="text-sm font-medium text-green-600">{pattern.success_count}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1 text-sm text-muted-foreground">
                  <AlertTriangle className="size-3 text-orange-500" /> Correções
                </span>
                <span className="text-sm font-medium text-orange-600">
                  {pattern.feedback_count}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Confiança mínima</span>
                <span className="text-sm font-medium">
                  {Math.round(pattern.confidence_threshold * 100)}%
                </span>
              </div>
            </CardContent>
          </Card>

          {isAtRisk && (
            <Card className="border-orange-200 bg-orange-50">
              <CardContent className="flex gap-2 pt-4 text-sm text-orange-800">
                <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                <p>
                  Taxa de acerto abaixo de 60%. Se atingir 4 correções com taxa inferior a 50%, este
                  padrão será desativado automaticamente.
                </p>
              </CardContent>
            </Card>
          )}

          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium">Informações</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Criado em</span>
                <span>{formatDate(pattern.created_at)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Atualizado</span>
                <span>{formatDate(pattern.updated_at)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Fingerprints</span>
                <span>{pattern.sample_fingerprints.length}</span>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Regras e Mapeamentos */}
        <div className="space-y-4 lg:col-span-2">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium">Regras de Extração</CardTitle>
            </CardHeader>
            <CardContent>
              {Object.keys(pattern.extraction_rules).length === 0 ? (
                <p className="text-sm text-muted-foreground">Nenhuma regra configurada.</p>
              ) : (
                <pre className="overflow-auto rounded bg-muted p-3 text-xs">
                  {JSON.stringify(pattern.extraction_rules, null, 2)}
                </pre>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium">Mapeamento de Campos</CardTitle>
            </CardHeader>
            <CardContent>
              {Object.keys(pattern.field_mappings).length === 0 ? (
                <p className="text-sm text-muted-foreground">Nenhum mapeamento configurado.</p>
              ) : (
                <pre className="overflow-auto rounded bg-muted p-3 text-xs">
                  {JSON.stringify(pattern.field_mappings, null, 2)}
                </pre>
              )}
            </CardContent>
          </Card>

          {/* Histórico de feedback */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium">
                Histórico de Feedback ({feedback.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              {feedback.length === 0 ? (
                <p className="text-sm text-muted-foreground">Nenhum feedback registrado ainda.</p>
              ) : (
                <div className="space-y-2">
                  {feedback.map((fb) => {
                    const typeInfo = feedbackLabels[fb.feedback_type] ?? {
                      label: fb.feedback_type,
                      className: "text-muted-foreground",
                    };
                    return (
                      <div
                        key={fb.id}
                        className="flex items-start justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
                      >
                        <div className="flex items-center gap-2">
                          <span className={`font-medium ${typeInfo.className}`}>
                            {typeInfo.label}
                          </span>
                          {Object.keys(fb.corrections).length > 0 && (
                            <Badge variant="outline" className="text-xs">
                              {Object.keys(fb.corrections).length} campo(s)
                            </Badge>
                          )}
                          {fb.notes && <span className="text-muted-foreground">{fb.notes}</span>}
                        </div>
                        <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                          <Clock className="size-3" />
                          {formatDate(fb.created_at)}
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
