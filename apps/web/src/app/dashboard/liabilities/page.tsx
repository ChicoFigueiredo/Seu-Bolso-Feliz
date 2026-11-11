import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq } from "drizzle-orm";
import Link from "next/link";
import { Plus, Landmark } from "lucide-react";
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
import { formatCurrency } from "@/lib/format";

const typeLabels: Record<string, string> = {
  personal_loan: "Empréstimo Pessoal",
  mortgage: "Financiamento Imobiliário",
  overdraft: "Cheque Especial",
  installment_plan: "Parcelamento",
  other: "Outro",
};

const statusLabels: Record<string, string> = {
  active: "Ativo",
  paid_off: "Quitado",
  renegotiated: "Renegociado",
  defaulted: "Inadimplente",
};

const statusVariants: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  active: "outline",
  paid_off: "default",
  renegotiated: "secondary",
  defaulted: "destructive",
};

export default async function LiabilitiesPage() {
  const { ownerId } = await requireUser();
  const db = getDb();
  const liabilities = await db
    .select({
      id: schema.liabilities.id,
      name: schema.liabilities.name,
      type: schema.liabilities.type,
      original_amount: schema.liabilities.original_amount,
      outstanding_balance: schema.liabilities.outstanding_balance,
      interest_rate: schema.liabilities.interest_rate,
      rate_type: schema.liabilities.rate_type,
      total_installments: schema.liabilities.total_installments,
      paid_installments: schema.liabilities.paid_installments,
      status: schema.liabilities.status,
      start_date: schema.liabilities.start_date,
      product_name: schema.financial_products.name,
      institution_name: schema.institutions.name,
    })
    .from(schema.liabilities)
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.financial_products.id, schema.liabilities.financial_product_id),
        eq(schema.financial_products.user_id, ownerId),
      ),
    )
    .leftJoin(
      schema.institutions,
      and(
        eq(schema.institutions.id, schema.financial_products.institution_id),
        eq(schema.institutions.user_id, ownerId),
      ),
    )
    .where(eq(schema.liabilities.user_id, ownerId))
    .orderBy(desc(schema.liabilities.outstanding_balance));

  const totalDebt = liabilities
    .filter((l) => l.status === "active")
    .reduce((sum, l) => sum + (l.outstanding_balance ?? 0), 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Dívidas</h1>
          <p className="text-muted-foreground">
            Dívida total ativa:{" "}
            <span className="font-semibold text-orange-600">{formatCurrency(totalDebt)}</span>
          </p>
        </div>
        <Button asChild>
          <Link href="/dashboard/liabilities/new">
            <Plus className="mr-2 size-4" />
            Nova Dívida
          </Link>
        </Button>
      </div>

      {liabilities.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Landmark className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma dívida cadastrada</CardTitle>
            <CardDescription className="mb-4">
              Cadastre seus empréstimos e financiamentos para acompanhar a quitação.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/liabilities/new">
                <Plus className="mr-2 size-4" />
                Cadastrar Dívida
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
                  <TableHead>Produto</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead className="text-right">Valor Original</TableHead>
                  <TableHead className="text-right">Saldo Devedor</TableHead>
                  <TableHead>Taxa</TableHead>
                  <TableHead>Parcelas</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {liabilities.map((l) => {
                  const prodLabel = `${l.institution_name ?? ""} ${l.product_name ?? ""}`.trim();
                  const progress = l.total_installments
                    ? Math.round((l.paid_installments / l.total_installments) * 100)
                    : null;

                  return (
                    <TableRow key={l.id}>
                      <TableCell className="font-medium">{l.name}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {prodLabel || "—"}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline" className="text-xs">
                          {typeLabels[l.type] ?? l.type}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {formatCurrency(l.original_amount)}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm font-semibold text-orange-600">
                        {formatCurrency(l.outstanding_balance)}
                      </TableCell>
                      <TableCell className="text-sm">
                        {l.interest_rate
                          ? `${(Number(l.interest_rate) * 100).toFixed(2)}% ${l.rate_type === "annual" ? "a.a." : "a.m."}`
                          : "—"}
                      </TableCell>
                      <TableCell className="text-sm">
                        {l.total_installments
                          ? `${l.paid_installments}/${l.total_installments} (${progress}%)`
                          : "—"}
                      </TableCell>
                      <TableCell>
                        <Badge variant={statusVariants[l.status] ?? "outline"} className="text-xs">
                          {statusLabels[l.status] ?? l.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        <Button variant="ghost" size="sm" asChild>
                          <Link href={`/dashboard/liabilities/${l.id}`}>Detalhes</Link>
                        </Button>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
