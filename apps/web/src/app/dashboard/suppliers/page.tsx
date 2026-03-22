import { createClient } from "@/lib/supabase/server";
import Link from "next/link";
import { Store, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";

const typeLabels: Record<string, string> = {
  company: "Empresa",
  individual: "Pessoa Física",
  government: "Governo",
  utility: "Utilidade",
  telecom: "Telecom",
  saas: "SaaS",
  platform: "Plataforma",
  other: "Outro",
};

export default async function SuppliersPage() {
  const supabase = await createClient();
  const { data: suppliers } = await supabase
    .from("suppliers")
    .select("*, supplier_aliases(count), supplier_contracts(count)")
    .eq("is_active", true)
    .order("name");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Fornecedores</h1>
          <p className="text-muted-foreground">Empresas, serviços e prestadores cadastrados</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/suppliers/new">
            <Plus className="mr-2 size-4" />
            Novo Fornecedor
          </Link>
        </Button>
      </div>

      {(suppliers ?? []).length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Store className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhum fornecedor cadastrado</CardTitle>
            <CardDescription className="mb-4">
              Comece cadastrando seus fornecedores e prestadores de serviço.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/suppliers/new">
                <Plus className="mr-2 size-4" />
                Cadastrar Primeiro Fornecedor
              </Link>
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead>Nome Fantasia</TableHead>
                  <TableHead className="text-center">Aliases</TableHead>
                  <TableHead className="text-center">Contratos</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(suppliers ?? []).map((supplier) => (
                  <TableRow key={supplier.id}>
                    <TableCell className="font-medium">{supplier.name}</TableCell>
                    <TableCell>
                      <Badge variant="secondary">
                        {typeLabels[supplier.type] ?? supplier.type}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {supplier.trade_name ?? "—"}
                    </TableCell>
                    <TableCell className="text-center">
                      {(supplier.supplier_aliases as { count: number }[])?.[0]?.count ?? 0}
                    </TableCell>
                    <TableCell className="text-center">
                      {(supplier.supplier_contracts as { count: number }[])?.[0]?.count ?? 0}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" asChild>
                        <Link href={`/dashboard/suppliers/${supplier.id}`}>Ver / Editar</Link>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
