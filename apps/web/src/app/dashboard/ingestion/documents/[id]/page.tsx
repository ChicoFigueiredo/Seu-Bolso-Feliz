import { getDocumentWithRelations, getDocumentStorageUrl } from "@/app/actions/ingestion";
import { notFound } from "next/navigation";
import Link from "next/link";
import { ChevronLeft } from "lucide-react";
import { DocumentDetailView } from "@/components/document-detail-view";

interface Props {
  params: Promise<{ id: string }>;
}

export default async function DocumentDetailPage({ params }: Props) {
  const { id } = await params;
  const result = await getDocumentWithRelations(id);

  if (!result) {
    notFound();
  }

  const { document, jobs, drafts } = result;

  // Get signed URL for viewing the file
  let fileUrl: string | null = null;
  if (document.storage_path) {
    fileUrl = await getDocumentStorageUrl(document.storage_path);
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <Link
          href="/dashboard/ingestion/documents"
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="size-4" />
          Documentos
        </Link>
        <div>
          <h1 className="text-xl font-bold tracking-tight truncate max-w-[600px]">
            {document.filename}
          </h1>
          <p className="text-sm text-muted-foreground">
            {document.origin_type?.replace("_", " ")} · {document.status}
          </p>
        </div>
      </div>

      <DocumentDetailView document={document} jobs={jobs} drafts={drafts} fileUrl={fileUrl} />
    </div>
  );
}
