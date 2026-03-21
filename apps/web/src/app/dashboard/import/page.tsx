"use client";

import { useState } from "react";
import { createClient } from "@/lib/supabase/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { toast } from "sonner";
import { Upload, FileSpreadsheet, AlertTriangle, CheckCircle2 } from "lucide-react";

interface ParsedRow {
  description: string;
  amount: number;
  type: string;
  event_date: string;
  status?: string;
}

interface ImportResult {
  total: number;
  imported: number;
  skipped: number;
  errors: string[];
}

export default function ImportPage() {
  const supabase = createClient();
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<ParsedRow[]>([]);
  const [importing, setImporting] = useState(false);
  const [result, setResult] = useState<ImportResult | null>(null);

  function parseCSV(text: string): ParsedRow[] {
    const lines = text.trim().split("\n");
    if (lines.length < 2) return [];

    const header = lines[0]!.split(",").map((h) => h.trim().toLowerCase().replace(/"/g, ""));

    const descIdx = header.findIndex(
      (h) => h.includes("descri") || h === "description" || h === "nome",
    );
    const amountIdx = header.findIndex(
      (h) => h.includes("valor") || h === "amount" || h === "value",
    );
    const typeIdx = header.findIndex((h) => h.includes("tipo") || h === "type");
    const dateIdx = header.findIndex(
      (h) => h.includes("data") || h === "date" || h.includes("event"),
    );

    if (amountIdx === -1 || dateIdx === -1) return [];

    const rows: ParsedRow[] = [];
    for (let i = 1; i < lines.length; i++) {
      const cols = lines[i]!.split(",").map((c) => c.trim().replace(/"/g, ""));
      if (cols.length < 2) continue;

      const amount = parseFloat(cols[amountIdx]?.replace(/[^\d.,-]/g, "").replace(",", ".") ?? "");
      if (isNaN(amount)) continue;

      rows.push({
        description: descIdx >= 0 ? (cols[descIdx] ?? `Linha ${i}`) : `Linha ${i}`,
        amount: Math.abs(amount),
        type:
          typeIdx >= 0
            ? cols[typeIdx]?.toLowerCase().includes("receita") ||
              cols[typeIdx]?.toLowerCase() === "income"
              ? "income"
              : "expense"
            : amount >= 0
              ? "income"
              : "expense",
        event_date: cols[dateIdx] ?? "",
      });
    }
    return rows;
  }

  function handleFileSelect(e: React.ChangeEvent<HTMLInputElement>) {
    const selectedFile = e.target.files?.[0];
    if (!selectedFile) return;
    setFile(selectedFile);
    setResult(null);

    const reader = new FileReader();
    reader.onload = (ev) => {
      const text = ev.target?.result as string;
      const parsed = parseCSV(text);
      setPreview(parsed.slice(0, 10));
    };
    reader.readAsText(selectedFile);
  }

  async function handleImport() {
    if (!file) return;
    setImporting(true);

    // Upload CSV to storage
    const filePath = `imports/${crypto.randomUUID()}.csv`;
    await supabase.storage.from("user-files").upload(filePath, file, { contentType: "text/csv" });

    // Create import job
    const reader = new FileReader();
    reader.onload = async (ev) => {
      const text = ev.target?.result as string;
      const parsed = parseCSV(text);

      const {
        data: { user },
      } = await supabase.auth.getUser();
      if (!user) {
        setImporting(false);
        return;
      }

      const { data: job } = await supabase
        .from("import_jobs")
        .insert({
          user_id: user.id,
          source_type: "csv",
          file_path: filePath,
          status: "processing",
          total_rows: parsed.length,
        })
        .select()
        .single();

      let imported = 0;
      let skipped = 0;
      const errors: string[] = [];

      // Get first financial product as default
      const { data: products } = await supabase.from("financial_products").select("id").limit(1);
      const defaultProductId = products?.[0]?.id;

      if (!defaultProductId) {
        toast.error("Cadastre ao menos um produto financeiro antes de importar.");
        setImporting(false);
        return;
      }

      for (let i = 0; i < parsed.length; i++) {
        const row = parsed[i]!;
        // Normalize date: try dd/MM/yyyy or yyyy-MM-dd
        let eventDate = row.event_date;
        if (eventDate.includes("/")) {
          const parts = eventDate.split("/");
          if (parts.length === 3) {
            eventDate = `${parts[2]!}-${parts[1]!.padStart(2, "0")}-${parts[0]!.padStart(2, "0")}`;
          }
        }

        const { error } = await supabase.from("transactions").insert({
          user_id: user.id,
          description: row.description,
          amount: row.amount,
          type: row.type,
          event_date: eventDate,
          financial_product_id: defaultProductId,
          origin_type: "import",
          is_confirmed: true,
        });

        if (error) {
          errors.push(`Linha ${i + 1}: ${error.message}`);
          skipped++;
        } else {
          imported++;
        }
      }

      // Update import job
      if (job) {
        await supabase
          .from("import_jobs")
          .update({
            status: errors.length === 0 ? "completed" : "partial",
            imported_rows: imported,
            skipped_rows: skipped,
            error_rows: errors.length,
            error_details: errors.length > 0 ? { errors: errors.slice(0, 50) } : null,
            completed_at: new Date().toISOString(),
          })
          .eq("id", job.id);
      }

      setResult({ total: parsed.length, imported, skipped, errors });
      setImporting(false);
      toast.success(`Importação concluída: ${imported} de ${parsed.length} registros`);
    };
    reader.readAsText(file);
  }

  const progress = result ? Math.round((result.imported / result.total) * 100) : 0;

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Importar Dados</h1>
        <p className="text-muted-foreground">Importe transações a partir de planilhas CSV</p>
      </div>

      <Alert>
        <FileSpreadsheet className="size-4" />
        <AlertTitle>Formato esperado do CSV</AlertTitle>
        <AlertDescription>
          O arquivo deve conter colunas com: <strong>data</strong> (dd/MM/yyyy ou yyyy-MM-dd),{" "}
          <strong>valor</strong> (numérico), e opcionalmente <strong>descrição</strong> e{" "}
          <strong>tipo</strong> (receita/despesa). Valores negativos são tratados como despesa.
        </AlertDescription>
      </Alert>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Upload className="size-5" />
            Selecionar Arquivo
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label>Arquivo CSV</Label>
            <Input type="file" accept=".csv" onChange={handleFileSelect} />
          </div>

          {file && (
            <p className="text-sm text-muted-foreground">
              Arquivo: <strong>{file.name}</strong> ({(file.size / 1024).toFixed(1)} KB)
            </p>
          )}
        </CardContent>
      </Card>

      {preview.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Pré-visualização</CardTitle>
            <CardDescription>Primeiras {preview.length} linhas do arquivo</CardDescription>
          </CardHeader>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Data</TableHead>
                  <TableHead>Descrição</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead className="text-right">Valor</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {preview.map((row, i) => (
                  <TableRow key={i}>
                    <TableCell className="text-sm">{row.event_date}</TableCell>
                    <TableCell className="font-medium">{row.description}</TableCell>
                    <TableCell>
                      <Badge
                        variant={row.type === "income" ? "default" : "secondary"}
                        className="text-xs"
                      >
                        {row.type === "income" ? "Receita" : "Despesa"}
                      </Badge>
                    </TableCell>
                    <TableCell
                      className={`text-right font-mono text-sm ${row.type === "income" ? "text-green-600" : "text-red-600"}`}
                    >
                      R$ {row.amount.toFixed(2)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {preview.length > 0 && !result && (
        <Button onClick={handleImport} disabled={importing} className="w-full" size="lg">
          {importing ? "Importando…" : `Importar ${file?.name}`}
        </Button>
      )}

      {result && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              {result.errors.length === 0 ? (
                <CheckCircle2 className="size-5 text-green-600" />
              ) : (
                <AlertTriangle className="size-5 text-orange-600" />
              )}
              Resultado da Importação
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Progress value={progress} />
            <div className="grid grid-cols-3 gap-4 text-center">
              <div>
                <p className="text-2xl font-bold text-green-600">{result.imported}</p>
                <p className="text-sm text-muted-foreground">Importados</p>
              </div>
              <div>
                <p className="text-2xl font-bold text-orange-600">{result.skipped}</p>
                <p className="text-sm text-muted-foreground">Ignorados</p>
              </div>
              <div>
                <p className="text-2xl font-bold">{result.total}</p>
                <p className="text-sm text-muted-foreground">Total</p>
              </div>
            </div>
            {result.errors.length > 0 && (
              <div className="max-h-40 overflow-auto rounded border p-3">
                {result.errors.slice(0, 10).map((err, i) => (
                  <p key={i} className="text-xs text-red-600">
                    {err}
                  </p>
                ))}
                {result.errors.length > 10 && (
                  <p className="text-xs text-muted-foreground">
                    … e mais {result.errors.length - 10} erros
                  </p>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
