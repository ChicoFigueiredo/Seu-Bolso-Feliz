import { getDraftBatch, getDraftRecords } from "@/app/actions/ingestion";
import { DraftReviewForm } from "@/components/draft-review-form";
import { notFound } from "next/navigation";
import Link from "next/link";
import { ChevronLeft } from "lucide-react";

interface Props {
  params: Promise<{ batchId: string }>;
}

export default async function BatchReviewPage({ params }: Props) {
  const { batchId } = await params;
  const batch = await getDraftBatch(batchId);

  if (!batch) {
    notFound();
  }

  const drafts = await getDraftRecords({ batchId: batch.id });

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Link
          href="/dashboard/ingestion/review"
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="size-4" />
          Voltar
        </Link>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            Revisão: {batch.name ?? `Lote ${batch.id.slice(0, 8)}`}
          </h1>
          <p className="text-muted-foreground">{drafts.length} rascunho(s) para revisão</p>
        </div>
      </div>

      <DraftReviewForm batch={batch} drafts={drafts} />
    </div>
  );
}
