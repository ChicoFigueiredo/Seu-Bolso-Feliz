import { notFound } from "next/navigation";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { ArrowLeft, FileText, ExternalLink } from "lucide-react";
import { getSourceDocument, getDocumentStorageUrl } from "@/app/actions/ingestion";

// ─── Helpers ─────────────────────────────────────────────────────────────────

const STATUS_LABEL: Record<string, string> = {
  new: "Novo",
  queued: "Na fila",
  processing: "Processando",
  processed: "Processado",
  pending_review: "Aguardando revisão",
  approved: "Aprovado",
  failed: "Erro",
};

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  new: "outline",
  queued: "secondary",
  processing: "secondary",
  processed: "default",
  pending_review: "secondary",
  approved: "default",
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
      </div>

      <Separator />

      {/* Metadados */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Metadados</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
            <div>
              <dt className="text-muted-foreground">ID</dt>
              <dd className="font-mono text-xs">{doc.id}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Tipo MIME</dt>
              <dd>{doc.mime_type ?? "—"}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Tamanho</dt>
              <dd>{formatFileSize(doc.file_size_bytes ?? null)}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Origem</dt>
              <dd>{ORIGIN_LABEL[doc.origin_type] ?? doc.origin_type}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Chave de origem</dt>
              <dd className="truncate font-mono text-xs">{doc.origin_key}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Criado em</dt>
              <dd>
                {new Date(doc.created_at).toLocaleString("pt-BR", {
                  dateStyle: "short",
                  timeStyle: "short",
                })}
              </dd>
            </div>
            {doc.gmail_subject && (
              <div className="col-span-2">
                <dt className="text-muted-foreground">Assunto (Gmail)</dt>
                <dd>{doc.gmail_subject}</dd>
              </div>
            )}
            {doc.gmail_from && (
              <div>
                <dt className="text-muted-foreground">Remetente (Gmail)</dt>
                <dd>{doc.gmail_from}</dd>
              </div>
            )}
          </dl>
        </CardContent>
      </Card>

      {/* Acesso ao arquivo */}
      {signedUrl && (
        <div className="flex">
          <Button variant="outline" asChild>
            <a href={signedUrl} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="mr-2 size-4" />
              Abrir arquivo
            </a>
          </Button>
        </div>
      )}

      {/* Placeholder para Sprint 3 */}
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-10 text-center">
          <p className="text-sm text-muted-foreground">
            Visualização do documento, extração de dados e detalhes do pipeline serão adicionados na
            Sprint 3.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
