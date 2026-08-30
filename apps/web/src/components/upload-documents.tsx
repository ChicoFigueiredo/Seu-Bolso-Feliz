"use client";

import { useState, useRef } from "react";
import { uploadDocument } from "@/app/actions/ingestion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { toast } from "sonner";
import { FileUp, Loader2 } from "lucide-react";
import { ACCEPT_UPLOAD } from "@sbf/contracts";

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

    // O upload passa pelo servidor em vez de ir do navegador direto ao storage.
    // Não é rodeio: é a Server Action que faz a deduplicação por hash de
    // conteúdo, registra o documento e enfileira o job — o caminho antigo subia
    // o arquivo primeiro e só depois descobria que era duplicado, deixando lixo
    // no bucket. E nenhuma credencial de storage precisa existir no navegador.
    let enviados = 0;
    const falhas: string[] = [];

    try {
      for (const file of fileArray) {
        const formData = new FormData();
        formData.append("file", file);
        try {
          await uploadDocument(formData);
          enviados += 1;
        } catch (e) {
          const motivo = e instanceof Error ? e.message : "erro desconhecido";
          falhas.push(`${file.name}: ${motivo}`);
        }
      }

      if (enviados === 0) {
        toast.error("Nenhum arquivo foi enviado", {
          description: falhas.join(" · ") || undefined,
        });
        return;
      }

      toast.success(`${enviados} arquivo(s) enviado(s)`, {
        description:
          falhas.length > 0
            ? `${falhas.length} não entrou: ${falhas.join(" · ")}`
            : "Já estão na fila de processamento.",
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
            accept={ACCEPT_UPLOAD}
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
