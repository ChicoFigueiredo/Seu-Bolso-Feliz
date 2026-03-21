import { createClient } from "@/lib/supabase/server";
import Link from "next/link";
import { Building2, Plus } from "lucide-react";
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
  bank: "Banco",
  credit_union: "Cooperativa",
  brokerage: "Corretora",
  fintech: "Fintech",
  other: "Outro",
};

export default async function InstitutionsPage() {
  const supabase = await createClient();
  const { data: institutions } = await supabase
    .from("institutions")
    .select("*, financial_products(count)")
    .order("name");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Instituições</h1>
          <p className="text-muted-foreground">Bancos, fintechs e cooperativas cadastradas</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/institutions/new">
            <Plus className="mr-2 size-4" />
            Nova Instituição
          </Link>
        </Button>
      </div>

      {(institutions ?? []).length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Building2 className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma instituição cadastrada</CardTitle>
            <CardDescription className="mb-4">
              Comece cadastrando seus bancos e instituições financeiras.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/institutions/new">
                <Plus className="mr-2 size-4" />
                Cadastrar Primeira Instituição
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
                  <TableHead className="text-center">Produtos</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(institutions ?? []).map((inst) => (
                  <TableRow key={inst.id}>
                    <TableCell className="font-medium">{inst.name}</TableCell>
                    <TableCell>
                      <Badge variant="secondary">{typeLabels[inst.type] ?? inst.type}</Badge>
                    </TableCell>
                    <TableCell className="text-center">
                      {(inst.financial_products as { count: number }[])?.[0]?.count ?? 0}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" asChild>
                        <Link href={`/dashboard/institutions/${inst.id}`}>Editar</Link>
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
