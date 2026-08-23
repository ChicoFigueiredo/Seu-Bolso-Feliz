"use client";

import { useCallback, useRef, useState, useTransition } from "react";
import { Loader2, Upload, FileUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { uploadDocument } from "@/app/actions/ingestion";
import {
  ACCEPT_UPLOAD,
  MIME_TYPES_ACEITOS,
  descricaoDosFormatos,
  extensaoAceita,
} from "@sbf/contracts";

interface Props {
  onSuccess?: () => void;
}

export function DocumentUploadDnD({ onSuccess }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isPending, startTransition] = useTransition();
  const inputRef = useRef<HTMLInputElement>(null);

  const submit = useCallback(
    (file: File) => {
      setSelectedFile(file);
      startTransition(async () => {
        try {
          const formData = new FormData();
          formData.append("file", file);
          await uploadDocument(formData);
          toast.success("Documento enviado", {
            description: "O pipeline de ingestão foi iniciado.",
          });
          setSelectedFile(null);
          onSuccess?.();
        } catch (err) {
          toast.error("Erro no upload", {
            description: err instanceof Error ? err.message : "Erro desconhecido",
          });
          setSelectedFile(null);
        }
      });
    },
    [onSuccess],
  );

  // ── Drag handlers ───────────────────────────────────────────────────────────

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  }, []);

  const onDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  }, []);

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(false);

      if (isPending) return;

      const file = e.dataTransfer.files[0];
      if (!file) return;

      // A extensão manda, e o MIME só ajuda: o navegador rotula OFX e CSV
      // como `application/octet-stream` com frequência, e recusar por MIME
      // barraria arquivo válido antes de qualquer tentativa de leitura.
      if (!extensaoAceita(file.name) && !MIME_TYPES_ACEITOS.has(file.type)) {
        toast.error("Tipo de arquivo não suportado", {
          description: `Aceitos: ${descricaoDosFormatos()}`,
        });
        return;
      }

      submit(file);
    },
    [isPending, submit],
  );

  const onInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      submit(file);
      e.target.value = "";
    },
    [submit],
  );

  return (
    <div
      role="button"
      aria-label="Área de upload — clique ou arraste um arquivo"
      tabIndex={0}
      onDragOver={onDragOver}
      onDragEnter={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      onClick={() => !isPending && inputRef.current?.click()}
      onKeyDown={(e) => e.key === "Enter" && !isPending && inputRef.current?.click()}
      className={cn(
        "relative flex min-h-[120px] cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed px-6 py-8 text-center transition-colors",
        isDragging
          ? "border-primary bg-primary/5 text-primary"
          : "border-muted-foreground/25 text-muted-foreground hover:border-primary/50 hover:bg-muted/30",
        isPending && "pointer-events-none opacity-70",
      )}
    >
      <input
        ref={inputRef}
        type="file"
        accept={ACCEPT_UPLOAD}
        className="sr-only"
        onChange={onInputChange}
        disabled={isPending}
        tabIndex={-1}
      />

      {isPending ? (
        <>
          <Loader2 className="size-8 animate-spin text-primary" />
          <p className="text-sm font-medium text-primary">
            Enviando <span className="font-semibold">{selectedFile?.name}</span>…
          </p>
        </>
      ) : isDragging ? (
        <>
          <Upload className="size-8 text-primary" />
          <p className="text-sm font-semibold">Solte aqui para enviar</p>
        </>
      ) : (
        <>
          <FileUp className="size-8" />
          <div>
            <p className="text-sm font-medium">
              Arraste um arquivo ou{" "}
              <span className="text-primary underline underline-offset-2">
                clique para selecionar
              </span>
            </p>
            <p className="mt-1 text-xs">{descricaoDosFormatos()}</p>
          </div>
        </>
      )}
    </div>
  );
}
