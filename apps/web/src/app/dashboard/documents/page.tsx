"use client";

import { useState, useEffect } from "react";
import { createClient } from "@/lib/supabase/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import { FileUp, FileText, Trash2 } from "lucide-react";

interface Doc {
  id: string;
  name: string;
  file_type: string | null;
  file_size: number | null;
  document_type: string | null;
  created_at: string;
}

const docTypeLabels: Record<string, string> = {
  receipt: "Comprovante",
  invoice: "Nota Fiscal",
  statement: "Extrato",
  contract: "Contrato",
  proof: "Prova",
  other: "Outro",
};

function formatFileSize(bytes: number | null): string {
  if (!bytes) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function DocumentsPage() {
  const supabase = createClient();
  const [docs, setDocs] = useState<Doc[]>([]);
  const [uploading, setUploading] = useState(false);
  const [docType, setDocType] = useState("other");

  useEffect(() => {
    loadDocs();
  }, []);

  async function loadDocs() {
    const { data } = await supabase
      .from("documents")
      .select("id, name, file_type, file_size, document_type, created_at")
      .order("created_at", { ascending: false });
    setDocs(data ?? []);
  }

  async function handleUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    setUploading(true);
    const ext = file.name.split(".").pop() ?? "";
    const filePath = `documents/${crypto.randomUUID()}.${ext}`;

    const { error: uploadError } = await supabase.storage
      .from("user-files")
      .upload(filePath, file, { contentType: file.type });

    if (uploadError) {
      toast.error("Erro no upload", { description: uploadError.message });
      setUploading(false);
      return;
    }

    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setUploading(false);
      return;
    }

    const { error: dbError } = await supabase.from("documents").insert({
      user_id: user.id,
      name: file.name,
      file_path: filePath,
      file_type: file.type || ext,
      file_size: file.size,
      document_type: docType,
    });

    setUploading(false);
    if (dbError) {
      toast.error("Erro ao registrar documento", { description: dbError.message });
    } else {
      toast.success("Documento enviado com sucesso");
      loadDocs();
    }

    e.target.value = "";
  }

  async function handleDelete(id: string, filePath?: string) {
    if (filePath) {
      await supabase.storage.from("user-files").remove([filePath]);
    }
    const { error } = await supabase.from("documents").delete().eq("id", id);
    if (error) toast.error("Erro ao excluir", { description: error.message });
    else {
      setDocs(docs.filter((d) => d.id !== id));
      toast.success("Documento excluído");
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Documentos</h1>
        <p className="text-muted-foreground">
          Armazene comprovantes, extratos, contratos e outros documentos
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileUp className="size-5" />
            Enviar Documento
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap items-end gap-4">
            <div className="space-y-2">
              <Label>Tipo do documento</Label>
              <Select value={docType} onValueChange={setDocType}>
                <SelectTrigger className="w-[180px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="receipt">Comprovante</SelectItem>
                  <SelectItem value="invoice">Nota Fiscal</SelectItem>
                  <SelectItem value="statement">Extrato</SelectItem>
                  <SelectItem value="contract">Contrato</SelectItem>
                  <SelectItem value="proof">Prova</SelectItem>
                  <SelectItem value="other">Outro</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Arquivo</Label>
              <Input
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.xlsx,.csv,.doc,.docx"
                onChange={handleUpload}
                disabled={uploading}
              />
            </div>
            {uploading && <p className="text-sm text-muted-foreground">Enviando…</p>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Documentos Armazenados</CardTitle>
          <CardDescription>{docs.length} documento(s)</CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {docs.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <FileText className="mb-4 size-12 text-muted-foreground" />
              <p className="text-muted-foreground">Nenhum documento enviado ainda.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead>Formato</TableHead>
                  <TableHead>Tamanho</TableHead>
                  <TableHead>Data</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {docs.map((doc) => (
                  <TableRow key={doc.id}>
                    <TableCell className="max-w-[200px] truncate font-medium">{doc.name}</TableCell>
                    <TableCell>
                      <Badge variant="outline" className="text-xs">
                        {docTypeLabels[doc.document_type ?? ""] ?? doc.document_type}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {doc.file_type ?? "—"}
                    </TableCell>
                    <TableCell className="text-sm">{formatFileSize(doc.file_size)}</TableCell>
                    <TableCell className="text-sm">
                      {new Date(doc.created_at).toLocaleDateString("pt-BR")}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" onClick={() => handleDelete(doc.id)}>
                        <Trash2 className="size-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
