"use client";

import { useCallback, useEffect, useState } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import { createClient } from "@/lib/supabase/client";
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  getExternalAccountMappings,
  updateExternalAccountMapping,
} from "@/app/actions/pluggy-connections";

const IGNORE_VALUE = "__ignore__";

interface FinancialProductOption {
  id: string;
  name: string;
  institutionName: string;
}

interface MappingRow {
  id: string;
  externalAccountName: string | null;
  externalAccountType: string | null;
  financialProductId: string | null;
  status: string;
}

const statusLabels: Record<string, string> = {
  pending_mapping: "Aguardando vínculo",
  mapped: "Vinculada",
  ignored: "Ignorada",
};

function MappingRowEditor({
  mapping,
  products,
  onSaved,
}: {
  mapping: MappingRow;
  products: FinancialProductOption[];
  onSaved: (row: MappingRow) => void;
}) {
  const [value, setValue] = useState(
    mapping.status === "ignored" ? IGNORE_VALUE : (mapping.financialProductId ?? ""),
  );
  const [saving, setSaving] = useState(false);

  async function handleSave() {
    setSaving(true);
    try {
      const updated =
        value === IGNORE_VALUE
          ? await updateExternalAccountMapping(mapping.id, {
              status: "ignored",
              financial_product_id: null,
            })
          : await updateExternalAccountMapping(mapping.id, {
              status: "mapped",
              financial_product_id: value,
            });
      toast.success("Vínculo salvo");
      onSaved({
        ...mapping,
        financialProductId: updated.financial_product_id,
        status: updated.status,
      });
    } catch (error) {
      toast.error("Erro ao salvar vínculo", {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <TableRow>
      <TableCell className="font-medium">{mapping.externalAccountName ?? "Conta"}</TableCell>
      <TableCell className="text-muted-foreground">{mapping.externalAccountType ?? "—"}</TableCell>
      <TableCell>
        <Badge variant={mapping.status === "mapped" ? "default" : "secondary"}>
          {statusLabels[mapping.status] ?? mapping.status}
        </Badge>
      </TableCell>
      <TableCell>
        <Select value={value} onValueChange={setValue}>
          <SelectTrigger className="w-64">
            <SelectValue placeholder="Selecione um produto financeiro" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={IGNORE_VALUE}>Ignorar esta conta</SelectItem>
            {products.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name} — {p.institutionName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </TableCell>
      <TableCell className="text-right">
        <Button size="sm" onClick={handleSave} disabled={saving || !value}>
          {saving ? "Salvando..." : "Salvar"}
        </Button>
      </TableCell>
    </TableRow>
  );
}

export default function ConnectionMappingPage() {
  const params = useParams();
  const id = params.id as string;
  const [fetching, setFetching] = useState(true);
  const [institutionName, setInstitutionName] = useState<string>("");
  const [mappings, setMappings] = useState<MappingRow[]>([]);
  const [products, setProducts] = useState<FinancialProductOption[]>([]);

  const load = useCallback(async () => {
    const supabase = createClient();
    const [{ data: connection }, mappingRows, { data: productRows }] = await Promise.all([
      supabase.from("provider_connections").select("institution_name").eq("id", id).single(),
      getExternalAccountMappings(id),
      supabase.from("financial_products").select("id, name, institutions(name)"),
    ]);

    setInstitutionName(connection?.institution_name ?? "Instituição desconhecida");
    setMappings(
      mappingRows.map((m) => ({
        id: m.id,
        externalAccountName: m.external_account_name,
        externalAccountType: m.external_account_type,
        financialProductId: m.financial_product_id,
        status: m.status,
      })),
    );
    setProducts(
      (productRows ?? []).map((p) => ({
        id: p.id,
        name: p.name,
        institutionName: (p.institutions as { name: string } | null)?.name ?? "",
      })),
    );
    setFetching(false);
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);

  function handleSaved(updated: MappingRow) {
    setMappings((prev) => prev.map((m) => (m.id === updated.id ? updated : m)));
  }

  if (fetching) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/connections">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{institutionName}</h1>
          <p className="text-muted-foreground">
            Vincule cada conta encontrada a um produto financeiro cadastrado
          </p>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Contas</CardTitle>
          <CardDescription>
            Transações só entram na revisão automaticamente para contas vinculadas ou ignoradas —
            contas aguardando vínculo continuam sincronizando, mas o rascunho pede pra você escolher
            a conta na revisão.
          </CardDescription>
        </CardHeader>
        <CardContent className="p-0">
          {mappings.length === 0 ? (
            <p className="p-6 text-muted-foreground">
              Nenhuma conta encontrada nesta conexão ainda.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Conta</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Produto financeiro</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {mappings.map((m) => (
                  <MappingRowEditor
                    key={m.id}
                    mapping={m}
                    products={products}
                    onSaved={handleSaved}
                  />
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
