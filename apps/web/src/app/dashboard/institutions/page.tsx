import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, sql } from "drizzle-orm";
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

// Contagem de produtos por instituição via `count(...)` — o mesmo padrão
// defensivo de `painel-hoje.ts`: o valor pode voltar como string dependendo
// do driver (bigint do Postgres), então nunca é usado sem passar por `num`.
const num = (v: unknown) => (v == null ? 0 : Number(v));

export default async function InstitutionsPage() {
  const user = await requireUser();
  const db = getDb();
  const institutions = await db
    .select({
      id: schema.institutions.id,
      name: schema.institutions.name,
      type: schema.institutions.type,
      productCount: sql<number>`count(${schema.financial_products.id})`,
    })
    .from(schema.institutions)
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.financial_products.institution_id, schema.institutions.id),
        eq(schema.financial_products.user_id, user.ownerId),
      ),
    )
    .where(eq(schema.institutions.user_id, user.ownerId))
    .groupBy(schema.institutions.id)
    .orderBy(asc(schema.institutions.name));

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

      {institutions.length === 0 ? (
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
                {institutions.map((inst) => (
                  <TableRow key={inst.id}>
                    <TableCell className="font-medium">{inst.name}</TableCell>
                    <TableCell>
                      <Badge variant="secondary">{typeLabels[inst.type] ?? inst.type}</Badge>
                    </TableCell>
                    <TableCell className="text-center">{num(inst.productCount)}</TableCell>
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
