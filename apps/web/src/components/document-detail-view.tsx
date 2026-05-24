"use client";

import { useState } from "react";
import {
  approveDraftRecord,
  rejectDraftRecord,
  updateDraftData,
  reprocessDocument,
} from "@/app/actions/ingestion";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/status-badge";
import { ConfidenceIndicator } from "@/components/confidence-indicator";
import { toast } from "sonner";
import {
  Check,
  X,
  RefreshCw,
  Pencil,
  Save,
  FileText,
  AlertTriangle,
  Lock,
  Loader2,
  Info,
} from "lucide-react";
import type { SourceDocument, IngestionJob, DraftRecord } from "@sbf/shared-types";

interface DocumentDetailViewProps {
  document: SourceDocument;
  jobs: IngestionJob[];
  drafts: DraftRecord[];
  fileUrl: string | null;
}

export function DocumentDetailView({
  document,
  jobs,
  drafts: initialDrafts,
  fileUrl,
}: DocumentDetailViewProps) {
  const [drafts, setDrafts] = useState(initialDrafts);
  const [loading, setLoading] = useState<string | null>(null);
  const [editingDraft, setEditingDraft] = useState<string | null>(null);
  const [editedData, setEditedData] = useState<Record<string, string>>({});
  const [rejectReason, setRejectReason] = useState("");
  const [showRejectFor, setShowRejectFor] = useState<string | null>(null);
  const [reprocessing, setReprocessing] = useState(false);

  const latestJob = jobs[0] ?? null;
  const isPdf = document.mime_type?.includes("pdf");
  const isImage = document.mime_type?.startsWith("image/");

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
      const updated = await rejectDraftRecord(id, rejectReason);
      setDrafts((prev) => prev.map((d) => (d.id === id ? { ...d, ...updated } : d)));
      setShowRejectFor(null);
      setRejectReason("");
      toast.success("Rascunho rejeitado");
    } catch (err) {
      toast.error("Erro ao rejeitar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  function startEditing(draft: DraftRecord) {
    const data = draft.draft_data as Record<string, unknown> | null;
    if (!data) return;
    const stringified: Record<string, string> = {};
    for (const [k, v] of Object.entries(data)) {
      stringified[k] = String(v ?? "");
    }
    setEditedData(stringified);
    setEditingDraft(draft.id);
  }

  async function handleSaveEdit(id: string) {
    setLoading(id);
    try {
      const updated = await updateDraftData(id, editedData);
      setDrafts((prev) => prev.map((d) => (d.id === id ? { ...d, ...updated } : d)));
      setEditingDraft(null);
      toast.success("Dados atualizados");
    } catch (err) {
      toast.error("Erro ao salvar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  async function handleReprocess() {
    setReprocessing(true);
    try {
      await reprocessDocument(document.id);
      toast.success("Documento enviado para reprocessamento");
      // Refresh page to reflect changes
      window.location.reload();
    } catch (err) {
      toast.error("Erro ao reprocessar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
      setReprocessing(false);
    }
  }

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      {/* LEFT: Document Viewer */}
      <div className="space-y-4">
        <Card className="overflow-hidden">
          <CardHeader className="pb-2">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium">Documento Original</CardTitle>
              <Button variant="outline" size="sm" onClick={handleReprocess} disabled={reprocessing}>
                {reprocessing ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <RefreshCw className="mr-1 size-3" />
                )}
                Reprocessar
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {fileUrl ? (
              <div className="rounded-lg border bg-muted/50 overflow-hidden">
                {isPdf ? (
                  <iframe
                    src={fileUrl}
                    className="w-full h-[600px]"
                    title="Visualização do documento"
                  />
                ) : isImage ? (
                  <img
                    src={fileUrl}
                    alt={document.filename}
                    className="w-full max-h-[600px] object-contain"
                  />
                ) : (
                  <div className="flex flex-col items-center justify-center py-12">
                    <FileText className="mb-2 size-8 text-muted-foreground" />
                    <p className="text-sm text-muted-foreground">Pré-visualização não disponível</p>
                    <a
                      href={fileUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="mt-2 text-sm text-primary hover:underline"
                    >
                      Baixar arquivo
                    </a>
                  </div>
                )}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center py-12 rounded-lg border bg-muted/50">
                <Lock className="mb-2 size-8 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">Arquivo não disponível no Storage</p>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Document Metadata */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Metadados</CardTitle>
          </CardHeader>
          <CardContent>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
              <dt className="font-medium text-muted-foreground">Origem</dt>
              <dd className="capitalize">{document.origin_type?.replace("_", " ") ?? "—"}</dd>
              <dt className="font-medium text-muted-foreground">MIME Type</dt>
              <dd>{document.mime_type ?? "—"}</dd>
              <dt className="font-medium text-muted-foreground">Tamanho</dt>
              <dd>
                {document.file_size_bytes
                  ? `${(document.file_size_bytes / 1024).toFixed(1)} KB`
                  : "—"}
              </dd>
              <dt className="font-medium text-muted-foreground">Status</dt>
              <dd>
                <StatusBadge status={document.status} className="text-xs" />
              </dd>
              {document.gmail_from && (
                <>
                  <dt className="font-medium text-muted-foreground">Gmail De</dt>
                  <dd className="truncate">{document.gmail_from}</dd>
                </>
              )}
              {document.gmail_subject && (
                <>
                  <dt className="font-medium text-muted-foreground">Assunto</dt>
                  <dd className="truncate">{document.gmail_subject}</dd>
                </>
              )}
              <dt className="font-medium text-muted-foreground">Criado em</dt>
              <dd>
                {document.created_at ? new Date(document.created_at).toLocaleString("pt-BR") : "—"}
              </dd>
            </dl>
          </CardContent>
        </Card>

        {/* Jobs */}
        {jobs.length > 0 && (
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">
                Jobs de Processamento ({jobs.length})
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {jobs.map((job) => (
                <div
                  key={job.id}
                  className="flex items-center justify-between rounded-md border p-2 text-sm"
                >
                  <StatusBadge status={job.status} className="text-xs" />
                  {job.error_message && (
                    <span className="flex items-center gap-1 text-xs text-destructive">
                      <AlertTriangle className="size-3" />
                      {job.error_message.slice(0, 80)}
                    </span>
                  )}
                  <span className="text-xs text-muted-foreground">
                    {job.updated_at ? new Date(job.updated_at).toLocaleString("pt-BR") : ""}
                  </span>
                </div>
              ))}
            </CardContent>
          </Card>
        )}
      </div>

      {/* RIGHT: Drafts / Review */}
      <div className="space-y-4">
        {drafts.length === 0 ? (
          <Card>
            <CardContent className="py-12">
              <div className="flex flex-col items-center text-center">
                <Info className="mb-2 size-8 text-muted-foreground" />
                <p className="font-medium">Nenhum rascunho gerado</p>
                <p className="mt-1 text-sm text-muted-foreground">
                  {latestJob?.status === "failed"
                    ? "O processamento falhou. Tente reprocessar o documento."
                    : "O documento ainda está sendo processado."}
                </p>
              </div>
            </CardContent>
          </Card>
        ) : (
          drafts.map((draft) => {
            const data = draft.draft_data as Record<string, unknown> | null;
            const isEditing = editingDraft === draft.id;
            const isLoading = loading === draft.id;

            return (
              <Card key={draft.id}>
                <CardHeader className="pb-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm font-medium">
                      {draft.draft_type} — {draft.id.slice(0, 8)}
                    </CardTitle>
                    <StatusBadge status={draft.status} className="text-xs" />
                  </div>
                  {draft.confidence_score != null && (
                    <ConfidenceIndicator value={draft.confidence_score} className="mt-1" />
                  )}
                </CardHeader>
                <CardContent className="space-y-3">
                  {/* Draft Data — Editable or Read-only */}
                  {data && (
                    <div className="rounded-md border p-3">
                      {isEditing ? (
                        <div className="space-y-2">
                          {Object.entries(editedData).map(([key, value]) => (
                            <div key={key} className="flex items-center gap-2">
                              <label className="w-32 shrink-0 text-xs font-medium text-muted-foreground">
                                {key}
                              </label>
                              <Input
                                value={value}
                                onChange={(e) =>
                                  setEditedData((prev) => ({
                                    ...prev,
                                    [key]: e.target.value,
                                  }))
                                }
                                className="h-8 text-sm"
                              />
                            </div>
                          ))}
                          <div className="flex gap-2 pt-2">
                            <Button
                              size="sm"
                              onClick={() => handleSaveEdit(draft.id)}
                              disabled={isLoading}
                            >
                              {isLoading ? (
                                <Loader2 className="mr-1 size-3 animate-spin" />
                              ) : (
                                <Save className="mr-1 size-3" />
                              )}
                              Salvar
                            </Button>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => setEditingDraft(null)}
                            >
                              Cancelar
                            </Button>
                          </div>
                        </div>
                      ) : (
                        <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
                          {Object.entries(data).map(([key, value]) => (
                            <div key={key} className="contents">
                              <dt className="font-medium text-muted-foreground">{key}</dt>
                              <dd className="truncate">{String(value ?? "—")}</dd>
                            </div>
                          ))}
                        </dl>
                      )}
                    </div>
                  )}

                  {/* Action Buttons */}
                  {(draft.status === "pending_review" || draft.status === "corrected") && (
                    <div className="flex items-center gap-2 pt-1">
                      <Button
                        size="sm"
                        onClick={() => handleApprove(draft.id)}
                        disabled={isLoading}
                      >
                        {isLoading ? (
                          <Loader2 className="mr-1 size-3 animate-spin" />
                        ) : (
                          <Check className="mr-1 size-3" />
                        )}
                        Aprovar
                      </Button>
                      {!isEditing && (
                        <Button size="sm" variant="secondary" onClick={() => startEditing(draft)}>
                          <Pencil className="mr-1 size-3" />
                          Editar
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="destructive"
                        onClick={() =>
                          setShowRejectFor(showRejectFor === draft.id ? null : draft.id)
                        }
                        disabled={isLoading}
                      >
                        <X className="mr-1 size-3" />
                        Rejeitar
                      </Button>
                    </div>
                  )}

                  {/* Reject Reason */}
                  {showRejectFor === draft.id && (
                    <div className="space-y-2 rounded-md border border-destructive/20 bg-destructive/5 p-3">
                      <Textarea
                        placeholder="Motivo da rejeição (opcional)"
                        value={rejectReason}
                        onChange={(e) => setRejectReason(e.target.value)}
                        rows={2}
                      />
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          variant="destructive"
                          onClick={() => handleReject(draft.id)}
                          disabled={isLoading}
                        >
                          Confirmar Rejeição
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => {
                            setShowRejectFor(null);
                            setRejectReason("");
                          }}
                        >
                          Cancelar
                        </Button>
                      </div>
                    </div>
                  )}

                  {/* Corrections display */}
                  {draft.corrections && typeof draft.corrections === "object" && (
                    <div className="rounded-md border bg-muted/50 p-3">
                      <p className="mb-1 text-xs font-medium text-muted-foreground">Correções:</p>
                      <pre className="text-xs whitespace-pre-wrap">
                        {JSON.stringify(draft.corrections, null, 2)}
                      </pre>
                    </div>
                  )}
                </CardContent>
              </Card>
            );
          })
        )}
      </div>
    </div>
  );
}
