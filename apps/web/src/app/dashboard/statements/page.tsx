import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq } from "drizzle-orm";
import Link from "next/link";
import { CreditCard } from "lucide-react";
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
import { formatCurrency, formatDate, isOverdue } from "@/lib/format";

const statusLabels: Record<string, string> = {
  open: "Aberta",
  closed: "Fechada",
  paid: "Paga",
  partial: "Parcial",
  overdue: "Atrasada",
};

const statusVariants: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  open: "outline",
  closed: "secondary",
  paid: "default",
  partial: "secondary",
  overdue: "destructive",
};

export default async function StatementsPage() {
  const { ownerId } = await requireUser();
  const db = getDb();

  // Sem RLS, o filtro por dono precisa estar explícito nesta consulta e em
  // cada JOIN abaixo (defesa em profundidade — ver apps/web/src/app/actions/painel-hoje.ts).
  const statements = await db
    .select({
      id: schema.statement_cycles.id,
      reference_month: schema.statement_cycles.reference_month,
      cycle_start_date: schema.statement_cycles.cycle_start_date,
      cycle_end_date: schema.statement_cycles.cycle_end_date,
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
    .where(eq(schema.statement_cycles.user_id, ownerId))
    .orderBy(desc(schema.statement_cycles.due_date));

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Faturas</h1>
        <p className="text-muted-foreground">Ciclos de fatura dos seus cartões de crédito</p>
      </div>

      {statements.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <CreditCard className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma fatura encontrada</CardTitle>
            <CardDescription>
              As faturas aparecerão aqui quando você cadastrar cartões e ciclos.
            </CardDescription>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Cartão</TableHead>
                  <TableHead>Referência</TableHead>
                  <TableHead>Período</TableHead>
                  <TableHead>Vencimento</TableHead>
                  <TableHead className="text-right">Total</TableHead>
                  <TableHead className="text-right">Pago</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {statements.map((s) => {
                  const cardLabel =
                    `${s.institution_name ?? ""} ${s.product_name ?? ""} •${s.last_four_digits ?? ""}`.trim();
                  const overdue = isOverdue(s.due_date) && s.status !== "paid";

                  return (
                    <TableRow key={s.id} className={overdue ? "bg-red-50 dark:bg-red-950/20" : ""}>
                      <TableCell className="font-medium">{cardLabel}</TableCell>
                      <TableCell className="text-sm">{formatDate(s.reference_month)}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {formatDate(s.cycle_start_date)} — {formatDate(s.cycle_end_date)}
                      </TableCell>
                      <TableCell
                        className={`text-sm ${overdue ? "font-semibold text-red-600" : ""}`}
                      >
                        {formatDate(s.due_date)}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {formatCurrency(s.total_amount ?? 0)}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {formatCurrency(s.paid_amount ?? 0)}
                      </TableCell>
                      <TableCell>
                        <Badge variant={statusVariants[s.status] ?? "outline"} className="text-xs">
                          {statusLabels[s.status] ?? s.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        <Button variant="ghost" size="sm" asChild>
                          <Link href={`/dashboard/statements/${s.id}`}>Detalhes</Link>
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
