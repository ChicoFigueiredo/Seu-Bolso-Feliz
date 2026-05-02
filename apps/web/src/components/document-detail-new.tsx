"use client";

/**
 * DocumentDetailViewNew — Telas 13 (generic) e 14 (statement)
 *
 * variant="generic"   → lg:grid-cols-[minmax(0,1fr)_340px]
 * variant="statement" → lg:grid-cols-[minmax(0,480px)_minmax(0,1fr)]
 *
 * S3-007/008/009 — Sprint 3
 */

import dynamic from "next/dynamic";
import { useState, useTransition, useEffect, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import { PlusCircle, Trash2, Link2, Unlink, Loader2, Check, Sparkles } from "lucide-react";
import type { SourceDocument, DraftRecord, DraftBatch } from "@sbf/shared-types";
import {
  listDocumentSplits,
  createDocumentSplit,
  deleteDocumentSplit,
  type DocumentSplit,
} from "@/app/actions/document-splits";
import {
  listLinkedTransactions,
  linkTransactionToDocument,
  unlinkTransactionFromDocument,
  type DocumentTransactionLinkWithTransaction,
} from "@/app/actions/document-transactions";
import { updateDocumentMetadata } from "@/app/actions/document-metadata";
import { getDraftRecords, approveDraftRecord, approveDraftBatch } from "@/app/actions/ingestion";
import { AIFieldBadge, CONFIDENCE_THRESHOLD } from "@/components/ai-field-badge";
import { useAISuggest } from "@/hooks/use-ai-suggest";
import { useChatContext } from "@/contexts/chat-context";

// react-pdf — sem SSR
const PDFPreview = dynamic(() => import("@/components/pdf-preview"), {
  ssr: false,
  loading: () => (
    <div className="flex min-h-[320px] items-center justify-center rounded-md border bg-muted/20 text-sm text-muted-foreground">
      Carregando visualizador PDF…
    </div>
  ),
});

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatCurrency(value: number | null | undefined): string {
  if (value == null) return "—";
  return new Intl.NumberFormat("pt-BR", { style: "currency", currency: "BRL" }).format(value);
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "—";
  return new Date(value).toLocaleDateString("pt-BR");
}

// ─── Props ───────────────────────────────────────────────────────────────────

export type DocumentDetailVariant = "generic" | "statement";

export interface DocumentDetailNewProps {
  doc: SourceDocument;
  signedUrl: string | null;
  variant: DocumentDetailVariant;
  draftBatch?: DraftBatch | null;
  ingestionMetadata?: {
    parserType?: string | null;
    confidence?: number | null;
    classification?: string[];
    draftCount?: number | null;
    jobStatus?: string | null;
  };
}

function formatConfidencePercentage(value: number | null | undefined): string {
  if (value == null || Number.isNaN(value)) return "—";
  const normalized = value <= 1 ? value * 100 : value;
  return `${Math.max(0, Math.min(100, normalized)).toFixed(0)}%`;
}

const JOB_STATUS_LABEL: Record<string, string> = {
  queued: "Na fila",
  processing: "Processando",
  parsed: "Parseado",
  normalized: "Normalizado",
  validated: "Validado",
  reconciled: "Reconciliado",
  drafted: "Rascunhado",
  pending_review: "Aguardando revisão",
  approved: "Aprovado",
  failed: "Falhou",
};

function IngestionInsightsCard({
  metadata,
}: {
  metadata?: DocumentDetailNewProps["ingestionMetadata"];
}) {
  const parserType = metadata?.parserType ?? null;
  const confidence = metadata?.confidence ?? null;
  const classification = metadata?.classification ?? [];
  const draftCount = metadata?.draftCount ?? null;
  const jobStatus = metadata?.jobStatus ?? null;

  if (!parserType && confidence == null && classification.length === 0 && draftCount == null) {
    return null;
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-semibold">Pipeline de ingestão</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground">Parser</span>
          <span className="font-medium">{parserType ?? "—"}</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground">Confiança</span>
          <span className="font-medium">{formatConfidencePercentage(confidence)}</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground">Status do job</span>
          <Badge variant="outline">
            {jobStatus ? (JOB_STATUS_LABEL[jobStatus] ?? jobStatus) : "—"}
          </Badge>
        </div>
        <div className="flex items-start justify-between gap-3">
          <span className="pt-0.5 text-muted-foreground">Classificação</span>
          <div className="flex flex-wrap justify-end gap-1">
            {classification.length > 0 ? (
              classification.map((item) => (
                <Badge key={item} variant="secondary" className="text-xs">
                  {item}
                </Badge>
              ))
            ) : (
              <span>—</span>
            )}
          </div>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground">Drafts gerados</span>
          <span className="font-medium">{draftCount ?? "—"}</span>
        </div>
      </CardContent>
    </Card>
  );
}

// ─── Card Metadados editável (S3-010) ─────────────────────────────────────────

function MetadataCard({ doc }: { doc: SourceDocument }) {
  const extDoc = doc as SourceDocument & { supplier_name_raw?: string };
  const meta = (doc.metadata ?? {}) as Record<string, unknown>;
  const overallConfidence = meta["confidence"] != null ? Number(meta["confidence"]) : 1;

  const [editing, setEditing] = useState(false);
  const [supplierName, setSupplierName] = useState(extDoc.supplier_name_raw ?? "");
  const [docDate, setDocDate] = useState((meta["date"] as string) ?? "");
  const [docAmountStr, setDocAmountStr] = useState(
    meta["amount"] != null ? String(meta["amount"]) : "",
  );
  const [isPending, startTransition] = useTransition();
  const [saveError, setSaveError] = useState<string | null>(null);

  function handleSave() {
    setSaveError(null);
    startTransition(async () => {
      try {
        await updateDocumentMetadata(doc.id, {
          supplier_name_raw: supplierName || undefined,
          metadata: {
            ...meta,
            date: docDate || undefined,
            amount: docAmountStr ? parseFloat(docAmountStr.replace(",", ".")) : undefined,
          },
        });
        setEditing(false);
      } catch (e: unknown) {
        setSaveError(e instanceof Error ? e.message : String(e));
      }
    });
  }

  const docAmount = meta["amount"] != null ? Number(meta["amount"]) : null;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-semibold">Metadados</CardTitle>
          {!editing && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() => setEditing(true)}
            >
              Editar
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent>
        {editing ? (
          <div className="space-y-3">
            <div className="space-y-1">
              <Label className="text-xs">Fornecedor</Label>
              <Input
                className="h-8 text-sm"
                value={supplierName}
                onChange={(e) => setSupplierName(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">Data do documento</Label>
              <Input
                type="date"
                className="h-8 text-sm"
                value={docDate}
                onChange={(e) => setDocDate(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">Valor (R$)</Label>
              <Input
                type="text"
                inputMode="decimal"
                placeholder="0,00"
                className="h-8 text-sm"
                value={docAmountStr}
                onChange={(e) => setDocAmountStr(e.target.value)}
              />
            </div>
            {saveError && <p className="text-xs text-destructive">{saveError}</p>}
            <div className="flex gap-2">
              <Button size="sm" onClick={handleSave} disabled={isPending} className="gap-1">
                {isPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Check className="h-3.5 w-3.5" />
                )}
                Salvar
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setEditing(false)}
                disabled={isPending}
              >
                Cancelar
              </Button>
            </div>
          </div>
        ) : (
          <dl className="space-y-2 text-sm">
            <div className="flex justify-between gap-2">
              <dt className="text-muted-foreground">Fornecedor</dt>
              <dd className="flex items-center gap-1 font-medium">
                {supplierName || "Não identificado"}
                {overallConfidence < CONFIDENCE_THRESHOLD && (
                  <AIFieldBadge
                    fieldLabel="Fornecedor"
                    confidence={overallConfidence}
                    source={(meta["extraction_source"] as string) ?? "ocr"}
                    documentId={doc.id}
                    value={supplierName || undefined}
                  />
                )}
              </dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-muted-foreground">Data</dt>
              <dd className="flex items-center gap-1">
                {formatDate(docDate || null)}
                {overallConfidence < CONFIDENCE_THRESHOLD && docDate && (
                  <AIFieldBadge
                    fieldLabel="Data do documento"
                    confidence={overallConfidence}
                    source={(meta["extraction_source"] as string) ?? "ocr"}
                    documentId={doc.id}
                    value={docDate}
                  />
                )}
              </dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-muted-foreground">Valor</dt>
              <dd className="flex items-center gap-1 font-medium">
                {formatCurrency(docAmount)}
                {overallConfidence < CONFIDENCE_THRESHOLD && docAmount != null && (
                  <AIFieldBadge
                    fieldLabel="Valor"
                    confidence={overallConfidence}
                    source={(meta["extraction_source"] as string) ?? "ocr"}
                    documentId={doc.id}
                    value={formatCurrency(docAmount)}
                  />
                )}
              </dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-muted-foreground">Tipo MIME</dt>
              <dd className="truncate text-xs text-muted-foreground">{doc.mime_type ?? "—"}</dd>
            </div>
          </dl>
        )}
      </CardContent>
    </Card>
  );
}

// ─── Card Rateios (S3-008) ────────────────────────────────────────────────────

function SplitsCard({
  sourceDocumentId,
  docAmount,
}: {
  sourceDocumentId: string;
  docAmount: number | null;
}) {
  const [splits, setSplits] = useState<DocumentSplit[]>([]);
  const [isPending, startTransition] = useTransition();
  const [amount, setAmount] = useState("");
  const [description, setDescription] = useState("");
  const [addError, setAddError] = useState<string | null>(null);

  // S4-005: sugestão de rateio via IA
  const { suggest: suggestSplits, loading: loadingSplits } = useAISuggest<{
    result: { suggestion?: string; message?: string };
  }>();

  const { setDrawerOpen, setPendingMessage } = useChatContext();

  async function handleSuggestSplits() {
    const res = await suggestSplits("suggest_splits", {
      document_id: sourceDocumentId,
      total_amount: docAmount ?? undefined,
    });
    if (res?.result?.suggestion) {
      setPendingMessage(
        `Sugira um rateio para este documento. ${res.result.suggestion} Valor total: ${formatCurrency(docAmount)}.`,
      );
      setDrawerOpen(true);
    }
  }

  const load = useCallback(() => {
    listDocumentSplits(sourceDocumentId)
      .then(setSplits)
      .catch(() => {});
  }, [sourceDocumentId]);

  useEffect(() => {
    load();
  }, [load]);

  const currentSum = splits.reduce((a, s) => a + s.amount, 0);
  const progressPct =
    docAmount && docAmount > 0 ? Math.min(100, (currentSum / docAmount) * 100) : null;

  function handleAdd() {
    const parsed = parseFloat(amount.replace(",", "."));
    if (isNaN(parsed) || parsed <= 0) {
      setAddError("Informe um valor válido maior que zero.");
      return;
    }
    setAddError(null);
    startTransition(async () => {
      try {
        await createDocumentSplit({
          source_document_id: sourceDocumentId,
          amount: parsed,
          description: description.trim() || undefined,
        });
        setAmount("");
        setDescription("");
        load();
      } catch (e: unknown) {
        setAddError(e instanceof Error ? e.message : String(e));
      }
    });
  }

  function handleDelete(id: string) {
    startTransition(async () => {
      await deleteDocumentSplit(id);
      load();
    });
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-semibold">Rateios</CardTitle>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleSuggestSplits}
            disabled={loadingSplits}
            title="Deixar a IA sugerir como ratear este documento"
          >
            {loadingSplits ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Sparkles className="h-3 w-3" />
            )}
            IA sugere rateio
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {docAmount != null && (
          <div className="space-y-1">
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>Soma: {formatCurrency(currentSum)}</span>
              <span>Total: {formatCurrency(docAmount)}</span>
            </div>
            <Progress value={progressPct ?? 0} className="h-2" />
          </div>
        )}

        {splits.length === 0 ? (
          <p className="text-xs text-muted-foreground">Nenhum rateio cadastrado.</p>
        ) : (
          <ul className="space-y-1.5">
            {splits.map((s) => (
              <li
                key={s.id}
                className="flex items-center justify-between gap-2 rounded-md border px-3 py-1.5 text-sm"
              >
                <span className="flex-1 truncate text-muted-foreground">
                  {s.description || "Sem descrição"}
                </span>
                <span className="font-medium tabular-nums">{formatCurrency(s.amount)}</span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 shrink-0 text-destructive hover:text-destructive"
                  onClick={() => handleDelete(s.id)}
                  disabled={isPending}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </li>
            ))}
          </ul>
        )}

        <Separator />
        <div className="space-y-2">
          <div className="space-y-1">
            <Label className="text-xs">Valor (R$)</Label>
            <Input
              type="text"
              inputMode="decimal"
              placeholder="0,00"
              className="h-8 text-sm"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
          </div>
          <div className="space-y-1">
            <Label className="text-xs">Descrição (opcional)</Label>
            <Input
              placeholder="Ex.: Materiais de escritório"
              className="h-8 text-sm"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          {addError && <p className="text-xs text-destructive">{addError}</p>}
          <Button size="sm" className="gap-1.5" onClick={handleAdd} disabled={isPending}>
            {isPending ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <PlusCircle className="h-3.5 w-3.5" />
            )}
            Adicionar rateio
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// ─── Card Transações Vinculadas (S3-008) ──────────────────────────────────────

function LinkedTransactionsCard({ sourceDocumentId }: { sourceDocumentId: string }) {
  const [links, setLinks] = useState<DocumentTransactionLinkWithTransaction[]>([]);
  const [isPending, startTransition] = useTransition();
  const [txInput, setTxInput] = useState("");
  const [addError, setAddError] = useState<string | null>(null);

  // S4-004: sugestão de reconciliação via IA
  const { suggest: suggestRecon, loading: loadingRecon } = useAISuggest<{
    result: {
      candidates?: { id: string; description: string; amount: number }[];
      message?: string;
    };
  }>();
  const [reconCandidates, setReconCandidates] = useState<
    { id: string; description: string; amount: number }[]
  >([]);

  async function handleSuggestRecon() {
    const res = await suggestRecon("suggest_reconciliation", { document_id: sourceDocumentId });
    const candidates = res?.result?.candidates ?? [];
    if (candidates.length > 0) {
      setReconCandidates(candidates);
    } else {
      setReconCandidates([]);
    }
  }

  const load = useCallback(() => {
    listLinkedTransactions(sourceDocumentId)
      .then(setLinks)
      .catch(() => {});
  }, [sourceDocumentId]);

  useEffect(() => {
    load();
  }, [load]);

  function handleLink() {
    const id = txInput.trim();
    if (!id) return;
    setAddError(null);
    startTransition(async () => {
      try {
        await linkTransactionToDocument(sourceDocumentId, id, "payment");
        setTxInput("");
        load();
      } catch (e: unknown) {
        setAddError(e instanceof Error ? e.message : String(e));
      }
    });
  }

  function handleUnlink(linkId: string) {
    startTransition(async () => {
      await unlinkTransactionFromDocument(linkId);
      load();
    });
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-semibold">Transações vinculadas</CardTitle>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleSuggestRecon}
            disabled={loadingRecon}
            title="Sugerir transações para vincular via IA"
          >
            {loadingRecon ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Sparkles className="h-3 w-3" />
            )}
            Sugerir via IA
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* Candidatos sugeridos pela IA */}
        {reconCandidates.length > 0 && (
          <div className="space-y-1.5 rounded-md border border-dashed border-primary/40 bg-primary/5 p-2">
            <p className="text-xs font-medium text-primary">Sugestões da IA</p>
            {reconCandidates.map((c) => (
              <div
                key={c.id}
                className="flex items-center justify-between gap-2 rounded-sm bg-background px-2 py-1 text-xs"
              >
                <span className="flex-1 truncate">{c.description}</span>
                <span className="tabular-nums">{formatCurrency(c.amount)}</span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => {
                    setTxInput(c.id);
                    setReconCandidates([]);
                  }}
                >
                  Usar
                </Button>
              </div>
            ))}
          </div>
        )}
        {links.length === 0 ? (
          <p className="text-xs text-muted-foreground">Nenhuma transação vinculada.</p>
        ) : (
          <ul className="space-y-1.5">
            {links.map((l) => {
              const tx = l.transaction as { description?: string; amount?: number };
              return (
                <li
                  key={l.id}
                  className="flex items-center justify-between gap-2 rounded-md border px-3 py-1.5 text-sm"
                >
                  <span className="flex-1 truncate text-muted-foreground">
                    {tx.description ?? l.transaction_id}
                  </span>
                  <span className="tabular-nums">{formatCurrency(tx.amount ?? null)}</span>
                  <Badge variant="outline" className="text-xs">
                    {l.link_type}
                  </Badge>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 shrink-0 text-destructive hover:text-destructive"
                    onClick={() => handleUnlink(l.id)}
                    disabled={isPending}
                  >
                    <Unlink className="h-3.5 w-3.5" />
                  </Button>
                </li>
              );
            })}
          </ul>
        )}

        <Separator />
        <div className="flex gap-2">
          <Input
            placeholder="ID da transação"
            className="h-8 font-mono text-xs"
            value={txInput}
            onChange={(e) => setTxInput(e.target.value)}
          />
          <Button
            size="sm"
            variant="outline"
            className="gap-1"
            onClick={handleLink}
            disabled={isPending || !txInput.trim()}
          >
            {isPending ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Link2 className="h-3.5 w-3.5" />
            )}
            Vincular
          </Button>
        </div>
        {addError && <p className="text-xs text-destructive">{addError}</p>}
      </CardContent>
    </Card>
  );
}

// ─── Card Resumo Fatura (S3-009) ──────────────────────────────────────────────

interface ReconciliationProgress {
  total_count: number;
  reconciled_count: number;
  progress_pct: number;
}

function StatementSummaryCard({ doc, batchId }: { doc: SourceDocument; batchId: string | null }) {
  const [progress, setProgress] = useState<ReconciliationProgress | null>(null);

  useEffect(() => {
    if (!batchId) return;
    fetch(`/api/reconciliation/${batchId}/progress`)
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (d) setProgress(d);
      })
      .catch(() => {});
  }, [batchId]);

  const meta = (doc.metadata ?? {}) as Record<string, unknown>;

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-semibold">Resumo da fatura</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <dl className="grid grid-cols-2 gap-y-2 text-sm">
          <div>
            <dt className="text-muted-foreground">Total</dt>
            <dd className="font-semibold">{formatCurrency((meta["total"] as number) ?? null)}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Vencimento</dt>
            <dd>{formatDate((meta["due_date"] as string) ?? null)}</dd>
          </div>
          {!!meta["cycle_start"] && !!meta["cycle_end"] && (
            <div className="col-span-2">
              <dt className="text-muted-foreground">Ciclo</dt>
              <dd>
                {formatDate(meta["cycle_start"] as string)} –{" "}
                {formatDate(meta["cycle_end"] as string)}
              </dd>
            </div>
          )}
        </dl>

        {progress && (
          <div className="space-y-1">
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>
                Conciliados: {progress.reconciled_count}/{progress.total_count}
              </span>
              <span>{Math.round(progress.progress_pct)}%</span>
            </div>
            <Progress value={progress.progress_pct} className="h-2" />
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ─── Tabela de Lançamentos (S3-009) ───────────────────────────────────────────

type DraftFilter = "all" | "pending" | "approved" | "rejected";

const DRAFT_CHIP: Record<
  string,
  { label: string; variant: "default" | "secondary" | "destructive" | "outline" }
> = {
  pending: { label: "Pendente", variant: "secondary" },
  pending_review: { label: "Pendente", variant: "secondary" },
  approved: { label: "Conciliado", variant: "default" },
  rejected: { label: "Rejeitado", variant: "destructive" },
};

function DraftTable({ batchId }: { batchId: string }) {
  const [records, setRecords] = useState<DraftRecord[]>([]);
  const [filter, setFilter] = useState<DraftFilter>("all");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [isPending, startTransition] = useTransition();

  // S4-006/007
  const { setDrawerOpen, setPendingMessage } = useChatContext();

  const load = useCallback(() => {
    getDraftRecords({ batchId })
      .then(setRecords)
      .catch(() => {});
  }, [batchId]);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = records.filter((r) => {
    if (filter === "all") return true;
    if (filter === "pending") return r.status === "pending_review" || r.status === "pending";
    return r.status === filter;
  });

  const pendingIds = filtered
    .filter((r) => r.status !== "approved" && r.status !== "rejected")
    .map((r) => r.id);
  const allSelected = pendingIds.length > 0 && pendingIds.every((id) => selected.has(id));

  function toggleAll() {
    if (allSelected) {
      setSelected(new Set());
    } else {
      setSelected(new Set(pendingIds));
    }
  }

  function toggleOne(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function handleApproveSelected() {
    startTransition(async () => {
      await Promise.all([...selected].map((id) => approveDraftRecord(id)));
      setSelected(new Set());
      load();
    });
  }

  function handleApproveBatch() {
    startTransition(async () => {
      await approveDraftBatch(batchId);
      setSelected(new Set());
      load();
    });
  }

  const FILTER_OPTIONS: { key: DraftFilter; label: string }[] = [
    { key: "all", label: "Todos" },
    { key: "pending", label: "Pendentes" },
    { key: "approved", label: "Conciliados" },
    { key: "rejected", label: "Rejeitados" },
  ];

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-sm font-semibold">Lançamentos</CardTitle>
          <div className="flex gap-1">
            {FILTER_OPTIONS.map((o) => (
              <Button
                key={o.key}
                variant={filter === o.key ? "default" : "ghost"}
                size="sm"
                className="h-7 text-xs"
                onClick={() => setFilter(o.key)}
              >
                {o.label}
              </Button>
            ))}
          </div>
        </div>

        {selected.size > 0 && (
          <div className="flex gap-2 pt-1">
            <Button
              size="sm"
              className="h-7 gap-1 text-xs"
              onClick={handleApproveSelected}
              disabled={isPending}
            >
              {isPending ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Check className="h-3 w-3" />
              )}
              Aprovar selecionados ({selected.size})
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              onClick={handleApproveBatch}
              disabled={isPending}
            >
              Aprovar todos
            </Button>
          </div>
        )}
      </CardHeader>

      <CardContent>
        {filtered.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nenhum lançamento nesta visão.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-xs text-muted-foreground">
                  <th className="w-8 pb-2 text-left">
                    <Checkbox checked={allSelected} onCheckedChange={toggleAll} />
                  </th>
                  <th className="pb-2 text-left">Descrição</th>
                  <th className="pb-2 text-left">Categoria</th>
                  <th className="pb-2 text-right">Valor</th>
                  <th className="pb-2 text-center">Status</th>
                  <th className="pb-2 text-center">IA</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {filtered.map((r) => {
                  const rData = r.draft_data as Record<string, unknown> | null;
                  const chip = DRAFT_CHIP[r.status] ?? {
                    label: r.status,
                    variant: "outline" as const,
                  };
                  const isApproved = r.status === "approved";
                  const category = (rData?.["category"] as string) ?? null;
                  return (
                    <tr key={r.id} className="hover:bg-muted/30">
                      <td className="py-2">
                        <Checkbox
                          checked={selected.has(r.id)}
                          onCheckedChange={() => toggleOne(r.id)}
                          disabled={isApproved}
                        />
                      </td>
                      <td className="py-2">
                        <span className="line-clamp-1">
                          {(rData?.["description"] as string) ?? r.id}
                        </span>
                      </td>
                      {/* S4-006: Chip de categoria inline */}
                      <td className="py-2">
                        {category ? (
                          <Badge variant="secondary" className="text-xs font-normal">
                            {category}
                          </Badge>
                        ) : (
                          <span className="text-xs text-muted-foreground">—</span>
                        )}
                      </td>
                      <td className="py-2 text-right tabular-nums">
                        {formatCurrency((rData?.["amount"] as number) ?? null)}
                      </td>
                      <td className="py-2 text-center">
                        <Badge variant={chip.variant} className="text-xs">
                          {chip.label}
                        </Badge>
                      </td>
                      {/* S4-007: Botão Conciliar com IA */}
                      <td className="py-2 text-center">
                        {!isApproved && (
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6"
                            title="Conciliar com IA"
                            onClick={() => {
                              setPendingMessage(
                                `Concilie o lançamento "${(rData?.["description"] as string) ?? r.id}" ` +
                                  `(valor: ${formatCurrency((rData?.["amount"] as number) ?? null)}) ` +
                                  `do batch ${batchId}. ` +
                                  "Busque transações compatíveis e sugira a melhor correspondência.",
                              );
                              setDrawerOpen(true);
                            }}
                          >
                            <Sparkles className="h-3.5 w-3.5 text-primary" />
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ─── Componente principal ─────────────────────────────────────────────────────

export default function DocumentDetailNew({
  doc,
  signedUrl,
  variant,
  draftBatch,
  ingestionMetadata,
}: DocumentDetailNewProps) {
  const gridCols =
    variant === "statement"
      ? "lg:grid-cols-[minmax(0,480px)_minmax(0,1fr)]"
      : "lg:grid-cols-[minmax(0,1fr)_340px]";

  const meta = (doc.metadata ?? {}) as Record<string, unknown>;
  const docAmount = meta["amount"] != null ? Number(meta["amount"]) : null;

  return (
    <div className={`grid gap-6 ${gridCols}`}>
      {/* Coluna esquerda — Visualização do PDF */}
      <div className="space-y-4">
        {signedUrl ? (
          <PDFPreview url={signedUrl} />
        ) : (
          <Card>
            <CardContent className="flex min-h-[320px] items-center justify-center text-sm text-muted-foreground">
              Arquivo não disponível para visualização.
            </CardContent>
          </Card>
        )}
      </div>

      {/* Coluna direita — Cards contextuais */}
      <div className="space-y-4">
        {variant === "generic" ? (
          <>
            {/* Tela 13 */}
            <IngestionInsightsCard metadata={ingestionMetadata} />
            <MetadataCard doc={doc} />
            {draftBatch ? (
              <DraftTable batchId={draftBatch.id} />
            ) : (
              <Card>
                <CardContent className="py-6 text-center text-sm text-muted-foreground">
                  Nenhum lote de rascunhos vinculado a este documento.
                </CardContent>
              </Card>
            )}
            <SplitsCard sourceDocumentId={doc.id} docAmount={docAmount} />
            <LinkedTransactionsCard sourceDocumentId={doc.id} />
          </>
        ) : (
          <>
            {/* Tela 14 */}
            <IngestionInsightsCard metadata={ingestionMetadata} />
            <StatementSummaryCard doc={doc} batchId={draftBatch?.id ?? null} />
            {draftBatch ? (
              <DraftTable batchId={draftBatch.id} />
            ) : (
              <Card>
                <CardContent className="py-8 text-center text-sm text-muted-foreground">
                  Nenhum lote de rascunhos vinculado a esta fatura.
                </CardContent>
              </Card>
            )}
          </>
        )}
      </div>
    </div>
  );
}
