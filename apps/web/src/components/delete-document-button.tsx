"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Trash2, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { toast } from "sonner";
import { deleteSourceDocument } from "@/app/actions/ingestion";

interface Props {
  documentId: string;
  filename: string;
  triggerVariant?: "outline" | "icon";
  onDeleted?: () => void;
  redirectTo?: string;
}

type DeleteMode = "document_only" | "document_and_ingestion";

export function DeleteDocumentButton({
  documentId,
  filename,
  triggerVariant = "outline",
  onDeleted,
  redirectTo,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteMode, setDeleteMode] = useState<DeleteMode | null>(null);

  async function handleDelete(mode: DeleteMode) {
    setIsDeleting(true);
    setDeleteMode(mode);
    try {
      await deleteSourceDocument(documentId, mode);
      toast.success(
        mode === "document_only"
          ? "Documento removido da lista e arquivo original excluído"
          : "Documento e ingestão vinculada excluídos",
      );
      setOpen(false);
      onDeleted?.();
      if (redirectTo) {
        router.push(redirectTo);
      } else {
        router.refresh();
      }
    } catch (err) {
      toast.error("Erro ao excluir", {
        description: err instanceof Error ? err.message : "Erro desconhecido",
      });
    } finally {
      setIsDeleting(false);
      setDeleteMode(null);
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {triggerVariant === "icon" ? (
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground hover:text-destructive"
            disabled={isDeleting}
            title="Excluir documento"
          >
            {isDeleting ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
          </Button>
        ) : (
          <Button
            variant="outline"
            size="sm"
            disabled={isDeleting}
            className="border-destructive/30 text-destructive hover:bg-destructive/10 hover:text-destructive"
          >
            {isDeleting ? (
              <Loader2 className="mr-1.5 size-4 animate-spin" />
            ) : (
              <Trash2 className="mr-1.5 size-4" />
            )}
            Excluir
          </Button>
        )}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Escolha como excluir este documento</DialogTitle>
          <DialogDescription>
            O documento <span className="font-semibold">{filename}</span> pode ser removido de duas
            formas. Escolha a opção conforme o efeito desejado no pipeline e na auditoria.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3">
          <button
            type="button"
            disabled={isDeleting}
            onClick={() => handleDelete("document_only")}
            className="rounded-lg border border-border p-4 text-left transition-colors hover:border-primary/40 hover:bg-muted/40 disabled:pointer-events-none disabled:opacity-60"
          >
            <div className="flex items-center justify-between gap-3">
              <span className="font-medium text-foreground">Só documento</span>
              {isDeleting && deleteMode === "document_only" ? (
                <Loader2 className="size-4 animate-spin text-primary" />
              ) : null}
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Remove o arquivo original do Storage e tira o item da lista, mas preserva jobs, drafts
              e auditoria de ingestão.
            </p>
          </button>

          <button
            type="button"
            disabled={isDeleting}
            onClick={() => handleDelete("document_and_ingestion")}
            className="rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-left transition-colors hover:border-destructive hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-60"
          >
            <div className="flex items-center justify-between gap-3">
              <span className="font-medium text-foreground">Documento + ingestão</span>
              {isDeleting && deleteMode === "document_and_ingestion" ? (
                <Loader2 className="size-4 animate-spin text-destructive" />
              ) : null}
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Remove o documento e limpa jobs, logs, versões parseadas, drafts, fingerprints e
              demais artefatos de ingestão. Transações já aprovadas permanecem, mas perdem o vínculo
              com o documento.
            </p>
          </button>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isDeleting}>
            Cancelar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
