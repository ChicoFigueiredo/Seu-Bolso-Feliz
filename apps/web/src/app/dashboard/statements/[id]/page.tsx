import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq } from "drizzle-orm";
import Link from "next/link";
import { ArrowLeft, CreditCard } from "lucide-react";
import { Button } from "@/components/ui/button";
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
import { formatCurrency, formatDate } from "@/lib/format";
import { redirect } from "next/navigation";

const statusLabels: Record<string, string> = {
  open: "Aberta",
  closed: "Fechada",
  paid: "Paga",
  partial: "Parcial",
  overdue: "Atrasada",
};

export default async function StatementDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const { ownerId } = await requireUser();
  const db = getDb();

  // Sem RLS, o filtro por dono precisa estar explícito nesta consulta e em
  // cada JOIN abaixo (defesa em profundidade — ver apps/web/src/app/actions/painel-hoje.ts).
  const [statement] = await db
    .select({
      id: schema.statement_cycles.id,
      reference_month: schema.statement_cycles.reference_month,
      due_date: schema.statement_cycles.due_date,
      total_amount: schema.statement_cycles.total_amount,
      paid_amount: schema.statement_cycles.paid_amount,
      status: schema.statement_cycles.status,
      last_four_digits: schema.cards.last_four_digits,
      card_brand: schema.cards.card_brand,
      product_name: schema.financial_products.name,
      institution_name: schema.institutions.name,
    })
    .from(schema.statement_cycles)
    .leftJoin(
      schema.cards,
      and(eq(schema.cards.id, schema.statement_cycles.card_id), eq(schema.cards.user_id, ownerId)),
    )
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.financial_products.id, schema.cards.financial_product_id),
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
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, ownerId)))
    .limit(1);

  if (!statement) redirect("/dashboard/statements");

  const items = await db
    .select({
      id: schema.statement_items.id,
      description: schema.statement_items.description,
      amount: schema.statement_items.amount,
      transaction_date: schema.statement_items.transaction_date,
      installment_number: schema.statement_items.installment_number,
      total_installments: schema.statement_items.total_installments,
    })
    .from(schema.statement_items)
    .where(
      and(
        eq(schema.statement_items.statement_cycle_id, id),
        eq(schema.statement_items.user_id, ownerId),
      ),
    )
    .orderBy(desc(schema.statement_items.transaction_date));

  const cardLabel =
    `${statement.institution_name ?? ""} ${statement.product_name ?? ""} •${statement.last_four_digits ?? ""}`.trim();
  const remaining = (statement.total_amount ?? 0) - (statement.paid_amount ?? 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/statements">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            Fatura {formatDate(statement.reference_month)}
          </h1>
          <p className="text-muted-foreground">{cardLabel}</p>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Total</CardDescription>
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold">
              {formatCurrency(statement.total_amount ?? 0)}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Pago</CardDescription>
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold text-green-600">
              {formatCurrency(statement.paid_amount ?? 0)}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Restante</CardDescription>
          </CardHeader>
          <CardContent>
            <span
              className={`text-2xl font-bold ${remaining > 0 ? "text-red-600" : "text-green-600"}`}
            >
              {formatCurrency(remaining)}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Status</CardDescription>
          </CardHeader>
          <CardContent>
            <Badge variant="outline" className="text-sm">
              {statusLabels[statement.status] ?? statement.status}
            </Badge>
            <p className="mt-1 text-xs text-muted-foreground">
              Vence em {formatDate(statement.due_date)}
            </p>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <CreditCard className="size-5" />
            Itens da Fatura
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {items.length === 0 ? (
            <p className="p-6 text-center text-muted-foreground">Nenhum item nesta fatura.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Data</TableHead>
                  <TableHead>Descrição</TableHead>
                  <TableHead>Parcela</TableHead>
                  <TableHead className="text-right">Valor</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item) => (
                  <TableRow key={item.id}>
                    <TableCell className="text-sm">
                      {item.transaction_date ? formatDate(item.transaction_date) : "—"}
                    </TableCell>
                    <TableCell className="font-medium">{item.description ?? "—"}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {item.installment_number && item.total_installments
                        ? `${item.installment_number}/${item.total_installments}`
                        : "—"}
                    </TableCell>
                    <TableCell className="text-right font-mono text-sm text-red-600">
                      {formatCurrency(item.amount)}
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
