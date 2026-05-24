import { notFound } from "next/navigation";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { ArrowLeft, FileText, ExternalLink } from "lucide-react";
import {
  getSourceDocument,
  getDocumentStorageUrl,
  getDraftBatchByDocumentId,
  getLatestIngestionJobByDocumentId,
} from "@/app/actions/ingestion";
import DocumentDetailNew, { type DocumentDetailVariant } from "@/components/document-detail-new";
import { DeleteDocumentButton } from "@/components/delete-document-button";

// ─── Resolver variant com base em document_type ───────────────────────────────

const STATEMENT_TYPES = new Set(["credit_card_statement", "bank_statement"]);

function resolveVariant(documentType: string | null | undefined): DocumentDetailVariant {
  if (documentType && STATEMENT_TYPES.has(documentType)) return "statement";
  return "generic";
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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

// ─── Componente (Server Component) ───────────────────────────────────────────

interface Props {
  params: Promise<{ id: string }>;
}

export default async function DocumentDetailPage({ params }: Props) {
  const { id } = await params;
  const doc = await getSourceDocument(id);

  if (!doc) notFound();

  const signedUrl = doc.storage_path ? await getDocumentStorageUrl(doc.storage_path) : null;

  // Resolver variant com base em document_type
  const variant = resolveVariant(doc.document_type);

  // Carregar dados de revisão para qualquer variante.
  const [draftBatch, latestJob] = await Promise.all([
    getDraftBatchByDocumentId(doc.id).catch(() => null),
    getLatestIngestionJobByDocumentId(doc.id).catch(() => null),
  ]);

  const latestJobMeta = (latestJob?.metadata ?? {}) as Record<string, unknown>;
  const confidenceValue = latestJobMeta["confidence"];
  const confidence =
    typeof confidenceValue === "number"
      ? confidenceValue
      : typeof confidenceValue === "string"
        ? Number(confidenceValue)
        : null;

  const draftTypesRaw = latestJobMeta["draft_types"];
  const draftTypes = Array.isArray(draftTypesRaw)
    ? draftTypesRaw.filter((value): value is string => typeof value === "string")
    : [];

  const draftCountValue = latestJobMeta["draft_count"];
  const draftCount =
    typeof draftCountValue === "number"
      ? draftCountValue
      : typeof draftCountValue === "string"
        ? Number(draftCountValue)
        : null;

  const ingestionMetadata = {
    parserType:
      typeof latestJobMeta["parser_type"] === "string" ? latestJobMeta["parser_type"] : null,
    confidence: Number.isFinite(confidence) ? confidence : null,
    classification: draftTypes,
    draftCount: Number.isFinite(draftCount) ? draftCount : null,
    jobStatus: latestJob?.status ?? null,
  };

  return (
    <div className="space-y-6">
      {/* Breadcrumb / voltar */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" asChild>
          <Link href="/dashboard/documents">
            <ArrowLeft className="mr-1 size-4" />
            Documentos
          </Link>
        </Button>
        <span className="text-muted-foreground">/</span>
        <span className="max-w-[300px] truncate text-sm font-medium">{doc.filename}</span>
      </div>

      {/* Cabeçalho */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <FileText className="size-8 shrink-0 text-muted-foreground" />
          <div>
            <h1 className="text-xl font-bold leading-tight">{doc.filename}</h1>
            <p className="text-sm text-muted-foreground">
              {ORIGIN_LABEL[doc.origin_type] ?? doc.origin_type}
            </p>
          </div>
        </div>
        <Badge variant={STATUS_VARIANT[doc.status] ?? "outline"}>
          {STATUS_LABEL[doc.status] ?? doc.status}
        </Badge>
        <DeleteDocumentButton
          documentId={doc.id}
          filename={doc.filename}
          redirectTo="/dashboard/documents"
        />
      </div>

      {/* Metadados rápidos */}
      <div className="flex flex-wrap gap-4 text-sm text-muted-foreground">
        <span>
          <span className="mr-1 font-medium text-foreground">ID:</span>
          <span className="font-mono text-xs">{doc.id}</span>
        </span>
        {doc.mime_type && (
          <span>
            <span className="mr-1 font-medium text-foreground">MIME:</span>
            {doc.mime_type}
          </span>
        )}
        {doc.file_size_bytes && (
          <span>
            <span className="mr-1 font-medium text-foreground">Tamanho:</span>
            {formatFileSize(doc.file_size_bytes)}
          </span>
        )}
        {doc.created_at && (
          <span>
            <span className="mr-1 font-medium text-foreground">Criado:</span>
            {new Date(doc.created_at).toLocaleString("pt-BR", {
              dateStyle: "short",
              timeStyle: "short",
            })}
          </span>
        )}
        {signedUrl && (
          <a
            href={signedUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-primary hover:underline"
          >
            <ExternalLink className="size-3" />
            Abrir arquivo
          </a>
        )}
      </div>

      <Separator />

      {/* Tela 13 (generic) ou Tela 14 (statement) */}
      <DocumentDetailNew
        doc={doc}
        signedUrl={signedUrl}
        variant={variant}
        draftBatch={draftBatch}
        ingestionMetadata={ingestionMetadata}
      />
    </div>
  );
}
