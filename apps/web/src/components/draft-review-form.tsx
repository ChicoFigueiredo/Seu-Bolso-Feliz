"use client";

import { useState } from "react";
import { approveDraftRecord, rejectDraftRecord, approveDraftBatch } from "@/app/actions/ingestion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/status-badge";
import { ConfidenceIndicator } from "@/components/confidence-indicator";
import { toast } from "sonner";
import { Check, X, CheckCheck, Loader2 } from "lucide-react";
import type { DraftRecord, DraftBatch } from "@sbf/shared-types";

interface DraftReviewFormProps {
  batch: DraftBatch;
  drafts: DraftRecord[];
}

export function DraftReviewForm({ batch, drafts: initialDrafts }: DraftReviewFormProps) {
  const [drafts, setDrafts] = useState(initialDrafts);
  const [loading, setLoading] = useState<string | null>(null);
  const [rejectReasons, setRejectReasons] = useState<Record<string, string>>({});
  const [showRejectFor, setShowRejectFor] = useState<string | null>(null);

  const pendingCount = drafts.filter((d) => d.status === "pending").length;

  async function handleApprove(id: string) {
    setLoading(id);
    try {
      const updated = await approveDraftRecord(id);
      setDrafts((prev) => prev.map((d) => (d.id === id ? { ...d, ...updated } : d)));
      toast.success("Rascunho aprovado");
    } catch (err) {
      toast.error("Erro ao aprovar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  async function handleReject(id: string) {
    setLoading(id);
    try {
      const reason = rejectReasons[id];
      const updated = await rejectDraftRecord(id, reason);
      setDrafts((prev) => prev.map((d) => (d.id === id ? { ...d, ...updated } : d)));
      setShowRejectFor(null);
      toast.success("Rascunho rejeitado");
    } catch (err) {
      toast.error("Erro ao rejeitar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  async function handleApproveAll() {
    setLoading("batch");
    try {
      await approveDraftBatch(batch.id);
      setDrafts((prev) =>
        prev.map((d) =>
          d.status === "pending"
            ? { ...d, status: "approved", approved_at: new Date().toISOString() }
            : d,
        ),
      );
      toast.success(`${pendingCount} rascunho(s) aprovado(s)`);
    } catch (err) {
      toast.error("Erro ao aprovar lote", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  function renderDraftData(data: unknown) {
    if (!data || typeof data !== "object")
      return <p className="text-sm text-muted-foreground">Sem dados</p>;
    const obj = data as Record<string, unknown>;
    return (
      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
        {Object.entries(obj).map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="font-medium text-muted-foreground">{key}</dt>
            <dd className="truncate">{String(value ?? "—")}</dd>
          </div>
        ))}
      </dl>
    );
  }

  return (
    <div className="space-y-4">
      {/* Batch header */}
      <div className="flex items-center justify-between rounded-lg border p-4">
        <div>
          <h2 className="text-lg font-semibold">{batch.name ?? `Lote ${batch.id.slice(0, 8)}`}</h2>
          <p className="text-sm text-muted-foreground">
            {drafts.length} rascunho(s) — {pendingCount} pendente(s)
          </p>
        </div>
        {pendingCount > 0 && (
          <Button onClick={handleApproveAll} disabled={loading === "batch"}>
            {loading === "batch" ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <CheckCheck className="mr-2 size-4" />
            )}
            Aprovar Todos ({pendingCount})
          </Button>
        )}
      </div>

      {/* Draft cards */}
      {drafts.map((draft) => (
        <Card key={draft.id}>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">
                {draft.draft_type} — {draft.id.slice(0, 8)}
              </CardTitle>
              <StatusBadge status={draft.status} className="text-xs" />
            </div>
            {draft.confidence_score != null && (
              <ConfidenceIndicator value={draft.confidence_score} className="mt-1" />
            )}
          </CardHeader>
          <CardContent className="space-y-3">
            {/* Draft data */}
            {renderDraftData(draft.draft_data)}

            {/* Actions */}
            {draft.status === "pending" && (
              <div className="flex items-center gap-2 pt-2">
                <Button
                  size="sm"
                  onClick={() => handleApprove(draft.id)}
                  disabled={loading === draft.id}
                >
                  {loading === draft.id ? (
                    <Loader2 className="mr-1 size-3 animate-spin" />
                  ) : (
                    <Check className="mr-1 size-3" />
                  )}
                  Aprovar
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => setShowRejectFor(showRejectFor === draft.id ? null : draft.id)}
                  disabled={loading === draft.id}
                >
                  <X className="mr-1 size-3" />
                  Rejeitar
                </Button>
              </div>
            )}

            {/* Reject reason */}
            {showRejectFor === draft.id && (
              <div className="space-y-2 rounded-md border border-destructive/20 bg-destructive/5 p-3">
                <Textarea
                  placeholder="Motivo da rejeição (opcional)"
                  value={rejectReasons[draft.id] ?? ""}
                  onChange={(e) =>
                    setRejectReasons((prev) => ({ ...prev, [draft.id]: e.target.value }))
                  }
                  rows={2}
                />
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => handleReject(draft.id)}
                    disabled={loading === draft.id}
                  >
                    Confirmar Rejeição
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => setShowRejectFor(null)}>
                    Cancelar
                  </Button>
                </div>
              </div>
            )}

            {/* Corrections display */}
            {draft.corrections && (
              <div className="rounded-md border p-3">
                <p className="mb-1 text-xs font-medium text-muted-foreground">
                  Correções aplicadas:
                </p>
                <pre className="text-xs">{JSON.stringify(draft.corrections, null, 2)}</pre>
              </div>
            )}
          </CardContent>
        </Card>
      ))}

      {drafts.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
          <p>Nenhum rascunho neste lote.</p>
        </div>
      )}
    </div>
  );
}
