"use client";

import { useState } from "react";
import Link from "next/link";
import {
  approveDraftRecord,
  rejectDraftRecord,
  approveDraftBatch,
  updateDraftData,
} from "@/app/actions/ingestion";
import {
  postApprovedDraftRecord,
  postApprovedDraftBatch,
  type MaterializationResult,
} from "@/app/actions/materialization";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { StatusBadge } from "@/components/status-badge";
import { ConfidenceIndicator } from "@/components/confidence-indicator";
import { toast } from "sonner";
import {
  Check,
  X,
  CheckCheck,
  Loader2,
  ArrowRight,
  AlertTriangle,
  ExternalLink,
} from "lucide-react";
import type { DraftRecord, DraftBatch, FinancialProduct } from "@sbf/shared-types";

interface DraftReviewFormProps {
  batch: DraftBatch;
  drafts: DraftRecord[];
  financialProducts: FinancialProduct[];
}

/** Tipos de draft que exigem uma conta/cartão antes de poderem ser lançados. */
const NEEDS_PRODUCT = new Set(["transaction", "liability"]);

export function DraftReviewForm({
  batch,
  drafts: initialDrafts,
  financialProducts,
}: DraftReviewFormProps) {
  const [drafts, setDrafts] = useState(initialDrafts);
  const [loading, setLoading] = useState<string | null>(null);
  const [rejectReasons, setRejectReasons] = useState<Record<string, string>>({});
  const [showRejectFor, setShowRejectFor] = useState<string | null>(null);
  /** Conta padrão do lote; cada draft pode sobrescrever. */
  const [batchProductId, setBatchProductId] = useState<string>("");
  const [productOverrides, setProductOverrides] = useState<Record<string, string>>({});
  /** Erros de lançamento por draft, para explicar a falha na própria linha. */
  const [postErrors, setPostErrors] = useState<Record<string, MaterializationResult>>({});

  const pendingCount = drafts.filter((d) => d.status === "pending_review").length;
  const approvedCount = drafts.filter((d) =>
    ["approved", "corrected"].includes(d.status as string),
  ).length;

  const productFor = (id: string) => productOverrides[id] ?? batchProductId;

  const draftNeedsProduct = (draft: DraftRecord) =>
    NEEDS_PRODUCT.has(draft.draft_type as string) &&
    !(draft.draft_data as Record<string, unknown> | null)?.financial_product_id;

  /**
   * Grava a conta escolhida dentro do draft_data.
   *
   * `updateDraftData` marca o registro como `corrected`, que é a semântica
   * certa: o humano completou um dado que a extração não tinha como saber.
   */
  async function persistProduct(draft: DraftRecord): Promise<DraftRecord | null> {
    const productId = productFor(draft.id);
    if (!productId) return draft;

    const data = { ...((draft.draft_data as Record<string, unknown>) ?? {}) };
    if (data.financial_product_id === productId) return draft;
    data.financial_product_id = productId;

    try {
      return await updateDraftData(draft.id, data);
    } catch (err) {
      toast.error("Erro ao gravar a conta", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
      return null;
    }
  }

  function applyResult(id: string, result: MaterializationResult) {
    setPostErrors((prev) => {
      const next = { ...prev };
      if (result.success) delete next[id];
      else next[id] = result;
      return next;
    });

    if (result.success) {
      setDrafts((prev) =>
        prev.map((d) =>
          d.id === id
            ? {
                ...d,
                status: "posted",
                posted_record_id: result.postedRecordId,
                posted_record_type: result.postedRecordType,
              }
            : d,
        ),
      );
    }
  }

  /** Aprova e lança em duas fases distintas, com rótulos próprios. */
  async function handleApproveAndPost(draft: DraftRecord) {
    setLoading(draft.id);
    try {
      const withProduct = await persistProduct(draft);
      if (!withProduct) return;

      const approved = await approveDraftRecord(draft.id);
      setDrafts((prev) => prev.map((d) => (d.id === draft.id ? { ...d, ...approved } : d)));

      const result = await postApprovedDraftRecord(draft.id);
      applyResult(draft.id, result);

      if (result.success) toast.success("Aprovado e lançado");
      else
        toast.warning("Aprovado, mas não lançado", {
          description: result.validationErrors[0] ?? result.message,
        });
    } catch (err) {
      toast.error("Erro ao aprovar", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setLoading(null);
    }
  }

  /** Retenta apenas o lançamento de um draft já aprovado. */
  async function handleRetryPost(draft: DraftRecord) {
    setLoading(draft.id);
    try {
      const withProduct = await persistProduct(draft);
      if (!withProduct) return;
      applyResult(draft.id, await postApprovedDraftRecord(draft.id));
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

  async function handleApproveAllAndPost() {
    setLoading("batch");
    try {
      // Fase 1 — gravar a conta escolhida em cada draft que precisa dela.
      for (const draft of drafts.filter((d) => d.status === "pending_review")) {
        if (draftNeedsProduct(draft) && productFor(draft.id)) await persistProduct(draft);
      }

      // Fase 2 — aprovar.
      await approveDraftBatch(batch.id);
      setDrafts((prev) =>
        prev.map((d) =>
          d.status === "pending_review"
            ? { ...d, status: "approved", approved_at: new Date().toISOString() }
            : d,
        ),
      );

      // Fase 3 — lançar.
      const result = await postApprovedDraftBatch(batch.id);
      for (const r of result.results) applyResult(r.draftRecordId, r);

      if (result.failed === 0) toast.success(`${result.succeeded} lançamento(s) criado(s)`);
      else
        toast.warning(`${result.succeeded} lançados · ${result.failed} com problema`, {
          description: "Veja o detalhe em cada rascunho abaixo.",
        });
    } catch (err) {
      toast.error("Erro ao processar lote", {
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
        {Object.entries(obj)
          .filter(([key]) => key !== "provenance" && key !== "schema_version")
          .map(([key, value]) => (
            <div key={key} className="contents">
              <dt className="font-medium text-muted-foreground">{key}</dt>
              <dd className="truncate">
                {value === null || value === undefined
                  ? "—"
                  : typeof value === "object"
                    ? JSON.stringify(value)
                    : String(value)}
              </dd>
            </div>
          ))}
      </dl>
    );
  }

  const productSelect = (value: string, onChange: (v: string) => void, placeholder: string) => (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="w-full">
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {financialProducts.map((p) => (
          <SelectItem key={p.id} value={p.id}>
            {p.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );

  return (
    <div className="space-y-4">
      {/* Cabeçalho do lote */}
      <div className="space-y-3 rounded-lg border p-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold">
              {batch.name ?? `Lote ${batch.id.slice(0, 8)}`}
            </h2>
            <p className="text-sm text-muted-foreground">
              {drafts.length} rascunho(s) — {pendingCount} pendente(s), {approvedCount} aprovado(s)
              não lançado(s)
            </p>
          </div>
          {pendingCount > 0 && (
            <Button onClick={handleApproveAllAndPost} disabled={loading === "batch"}>
              {loading === "batch" ? (
                <Loader2 className="mr-2 size-4 animate-spin" />
              ) : (
                <CheckCheck className="mr-2 size-4" />
              )}
              Aprovar e lançar ({pendingCount})
            </Button>
          )}
        </div>

        {/*
          Seletor de conta no nível do lote.
          transactions.financial_product_id é NOT NULL e nada no pipeline
          consegue inferir a conta a partir do documento. Sem esta escolha,
          todo lançamento reprovaria na validação.
        */}
        {financialProducts.length > 0 ? (
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">
              Lançar em (padrão do lote — pode ser alterado por rascunho)
            </Label>
            {productSelect(batchProductId, setBatchProductId, "Selecione a conta ou cartão")}
          </div>
        ) : (
          <div className="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600" />
            <span>
              Nenhuma conta ou cartão cadastrado. Cadastre um em{" "}
              <Link href="/dashboard/products" className="underline">
                Produtos
              </Link>{" "}
              antes de lançar.
            </span>
          </div>
        )}
      </div>

      {/* Rascunhos */}
      {drafts.map((draft) => {
        const failure = postErrors[draft.id];
        const needsProduct = draftNeedsProduct(draft);
        const isApproved = ["approved", "corrected"].includes(draft.status as string);

        return (
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
              {renderDraftData(draft.draft_data)}

              {/* Registro criado: prova verificável de que o dinheiro entrou no ledger */}
              {draft.posted_record_id && (
                <div className="flex items-center gap-2 rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3 text-sm">
                  <Check className="size-4 shrink-0 text-emerald-600" />
                  <span>Lançado em {draft.posted_record_type}</span>
                  {draft.posted_record_type === "transactions" && (
                    <Link
                      href={`/dashboard/transactions?highlight=${draft.posted_record_id}`}
                      className="inline-flex items-center gap-1 underline"
                    >
                      ver <ExternalLink className="size-3" />
                    </Link>
                  )}
                </div>
              )}

              {/* Falha de lançamento, com os campos exatos que faltam */}
              {failure && (
                <div className="space-y-1 rounded-md border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
                  <div className="flex items-center gap-2 font-medium">
                    <AlertTriangle className="size-4 text-amber-600" />
                    {failure.message}
                  </div>
                  {failure.validationErrors.length > 0 && (
                    <ul className="ml-6 list-disc text-xs text-muted-foreground">
                      {failure.validationErrors.map((e) => (
                        <li key={e}>{e}</li>
                      ))}
                    </ul>
                  )}
                </div>
              )}

              {/* Conta por rascunho */}
              {needsProduct && !draft.posted_record_id && financialProducts.length > 0 && (
                <div className="space-y-1">
                  <Label className="text-xs text-muted-foreground">Conta deste rascunho</Label>
                  {productSelect(
                    productFor(draft.id),
                    (v) => setProductOverrides((prev) => ({ ...prev, [draft.id]: v })),
                    "Usar o padrão do lote",
                  )}
                </div>
              )}

              {/* Ações */}
              {draft.status === "pending_review" && (
                <div className="flex items-center gap-2 pt-2">
                  <Button
                    size="sm"
                    onClick={() => handleApproveAndPost(draft)}
                    disabled={loading === draft.id}
                  >
                    {loading === draft.id ? (
                      <Loader2 className="mr-1 size-3 animate-spin" />
                    ) : (
                      <ArrowRight className="mr-1 size-3" />
                    )}
                    Aprovar e lançar
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

              {/* Aprovado mas não lançado: o estado intermediário fica visível */}
              {isApproved && !draft.posted_record_id && (
                <div className="flex items-center gap-2 pt-2">
                  <Button
                    size="sm"
                    onClick={() => handleRetryPost(draft)}
                    disabled={loading === draft.id}
                  >
                    {loading === draft.id ? (
                      <Loader2 className="mr-1 size-3 animate-spin" />
                    ) : (
                      <ArrowRight className="mr-1 size-3" />
                    )}
                    {failure ? "Tentar novamente" : "Lançar"}
                  </Button>
                </div>
              )}

              {/* Motivo da rejeição */}
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
        );
      })}

      {drafts.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
          <p>Nenhum rascunho neste lote.</p>
        </div>
      )}
    </div>
  );
}
