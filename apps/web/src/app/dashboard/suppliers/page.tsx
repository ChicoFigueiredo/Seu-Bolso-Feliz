import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, sql } from "drizzle-orm";
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

// Mesmo padrão defensivo de `painel-hoje.ts`: `count(...)` pode voltar como
// string dependendo do driver (bigint do Postgres).
const num = (v: unknown) => (v == null ? 0 : Number(v));

export default async function SuppliersPage() {
  const user = await requireUser();
  const db = getDb();
  // Dois leftJoin (aliases e contratos) na mesma consulta produzem produto
  // cartesiano por fornecedor — sem `distinct` aqui dentro do count, um
  // fornecedor com 2 aliases e 3 contratos contaria 6 em cada coluna, não
  // 2 e 3. `count(distinct ...)` é o que faz essa consulta equivaler ao
  // `select("*, supplier_aliases(count), supplier_contracts(count)")` do
  // PostgREST (que soma cada relação embutida separadamente).
  const suppliers = await db
    .select({
      id: schema.suppliers.id,
      name: schema.suppliers.name,
      type: schema.suppliers.type,
      trade_name: schema.suppliers.trade_name,
      aliasCount: sql<number>`count(distinct ${schema.supplier_aliases.id})`,
      contractCount: sql<number>`count(distinct ${schema.supplier_contracts.id})`,
    })
    .from(schema.suppliers)
    .leftJoin(
      schema.supplier_aliases,
      and(
        eq(schema.supplier_aliases.supplier_id, schema.suppliers.id),
        eq(schema.supplier_aliases.user_id, user.ownerId),
      ),
    )
    .leftJoin(
      schema.supplier_contracts,
      and(
        eq(schema.supplier_contracts.supplier_id, schema.suppliers.id),
        eq(schema.supplier_contracts.user_id, user.ownerId),
      ),
    )
    .where(and(eq(schema.suppliers.user_id, user.ownerId), eq(schema.suppliers.is_active, true)))
    .groupBy(schema.suppliers.id)
    .orderBy(asc(schema.suppliers.name));

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

      {suppliers.length === 0 ? (
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
                {suppliers.map((supplier) => (
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
                    <TableCell className="text-center">{num(supplier.aliasCount)}</TableCell>
                    <TableCell className="text-center">{num(supplier.contractCount)}</TableCell>
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
