"use client";

import { useState, useRef } from "react";
import { createClient } from "@/lib/supabase/client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { toast } from "sonner";
import { FileUp, Loader2 } from "lucide-react";

const ACCEPTED_TYPES = ".pdf,.png,.jpg,.jpeg,.xlsx,.csv,.doc,.docx,.ofx,.qif";
const MAX_FILE_SIZE = 10 * 1024 * 1024; // 10MB

export function UploadDocuments({ onUploadComplete }: { onUploadComplete?: () => void }) {
  const [uploading, setUploading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  async function uploadFiles(files: FileList | File[]) {
    const fileArray = Array.from(files);
    if (fileArray.length === 0) return;

    // Validar tamanho
    const oversized = fileArray.filter((f) => f.size > MAX_FILE_SIZE);
    if (oversized.length > 0) {
      toast.error("Arquivo(s) muito grande(s)", {
        description: `Limite de 10MB. Arquivos rejeitados: ${oversized.map((f) => f.name).join(", ")}`,
      });
      return;
    }

    setUploading(true);
    const supabase = createClient();
    const uploadedPaths: string[] = [];

    try {
      // Upload cada arquivo para o storage
      for (const file of fileArray) {
        const docId = crypto.randomUUID();
        const storagePath = `uploads/${docId}/${file.name}`;

        const { error: uploadError } = await supabase.storage
          .from("ingestion-originals")
          .upload(storagePath, file, { contentType: file.type });

        if (uploadError) {
          toast.error(`Erro no upload de ${file.name}`, {
            description: uploadError.message,
          });
          continue;
        }

        uploadedPaths.push(storagePath);
      }

      if (uploadedPaths.length === 0) {
        toast.error("Nenhum arquivo enviado com sucesso");
        return;
      }

      // Chamar edge function trigger-ingestion
      const { data, error } = await supabase.functions.invoke("trigger-ingestion", {
        body: {
          source_type: "manual",
          file_paths: uploadedPaths,
        },
      });

      if (error) {
        toast.error("Erro ao iniciar ingestão", { description: error.message });
        return;
      }

      toast.success(`${uploadedPaths.length} arquivo(s) enviado(s)`, {
        description: `${data?.jobs_created ?? 0} job(s) de ingestão criado(s)`,
      });

      onUploadComplete?.();
    } finally {
      setUploading(false);
      if (inputRef.current) inputRef.current.value = "";
    }
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(false);
    if (e.dataTransfer.files.length > 0) {
      uploadFiles(e.dataTransfer.files);
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(true);
  }

  function handleDragLeave(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(false);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FileUp className="size-5" />
          Upload Manual
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div
          className={`relative flex flex-col items-center justify-center rounded-lg border-2 border-dashed p-8 transition-colors ${
            dragOver
              ? "border-primary bg-primary/5"
              : "border-muted-foreground/25 hover:border-muted-foreground/50"
          }`}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          {uploading ? (
            <div className="flex items-center gap-2">
              <Loader2 className="size-5 animate-spin" />
              <span className="text-sm text-muted-foreground">Enviando...</span>
            </div>
          ) : (
            <>
              <FileUp className="mb-3 size-10 text-muted-foreground" />
              <p className="mb-1 text-sm font-medium">
                Arraste arquivos aqui ou clique para selecionar
              </p>
              <p className="text-xs text-muted-foreground">
                PDF, imagens, planilhas, OFX, QIF — até 10MB cada
              </p>
              <Button
                variant="outline"
                size="sm"
                className="mt-3"
                onClick={() => inputRef.current?.click()}
              >
                Selecionar Arquivos
              </Button>
            </>
          )}
          <input
            ref={inputRef}
            type="file"
            className="hidden"
            accept={ACCEPTED_TYPES}
            multiple
            onChange={(e) => {
              if (e.target.files) uploadFiles(e.target.files);
            }}
            disabled={uploading}
          />
        </div>
      </CardContent>
    </Card>
  );
}
