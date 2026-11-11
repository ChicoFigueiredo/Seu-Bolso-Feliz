import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";
import { getLiabilityInstallments } from "@/app/actions/liabilities";
import Link from "next/link";
import { ArrowLeft, Landmark } from "lucide-react";
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
import { Progress } from "@/components/ui/progress";
import { formatCurrency, formatDate, isOverdue } from "@/lib/format";
import { redirect } from "next/navigation";

const installmentStatusLabels: Record<string, string> = {
  pending: "Pendente",
  paid: "Paga",
  partial: "Parcial",
  overdue: "Atrasada",
  waived: "Dispensada",
};

export default async function LiabilityDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const { ownerId } = await requireUser();
  const db = getDb();

  const [liability] = await db
    .select({
      id: schema.liabilities.id,
      name: schema.liabilities.name,
      type: schema.liabilities.type,
      original_amount: schema.liabilities.original_amount,
      outstanding_balance: schema.liabilities.outstanding_balance,
      interest_rate: schema.liabilities.interest_rate,
      rate_type: schema.liabilities.rate_type,
      amortization_system: schema.liabilities.amortization_system,
      total_installments: schema.liabilities.total_installments,
      paid_installments: schema.liabilities.paid_installments,
      start_date: schema.liabilities.start_date,
      end_date: schema.liabilities.end_date,
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
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, ownerId)));

  if (!liability) redirect("/dashboard/liabilities");

  const installments = await getLiabilityInstallments(id);

  const prodLabel = `${liability.institution_name ?? ""} ${liability.product_name ?? ""}`.trim();
  const progress = liability.total_installments
    ? Math.round((liability.paid_installments / liability.total_installments) * 100)
    : 0;
  const paidTotal = liability.original_amount - liability.outstanding_balance;

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/liabilities">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{liability.name}</h1>
          <p className="text-muted-foreground">{prodLabel}</p>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Valor Original</CardDescription>
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold">{formatCurrency(liability.original_amount)}</span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Saldo Devedor</CardDescription>
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold text-orange-600">
              {formatCurrency(liability.outstanding_balance)}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Amortizado</CardDescription>
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold text-green-600">{formatCurrency(paidTotal)}</span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Progresso</CardDescription>
          </CardHeader>
          <CardContent>
            {liability.total_installments ? (
              <>
                <span className="text-2xl font-bold">{progress}%</span>
                <Progress value={progress} className="mt-2" />
                <p className="mt-1 text-xs text-muted-foreground">
                  {liability.paid_installments}/{liability.total_installments} parcelas
                </p>
              </>
            ) : (
              <span className="text-sm text-muted-foreground">Sem parcelas definidas</span>
            )}
          </CardContent>
        </Card>
      </div>

      {liability.interest_rate && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Landmark className="size-5" />
              Detalhes Financeiros
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid gap-4 sm:grid-cols-3">
              <div>
                <p className="text-sm text-muted-foreground">Taxa de Juros</p>
                <p className="font-semibold">
                  {(Number(liability.interest_rate) * 100).toFixed(2)}%{" "}
                  {liability.rate_type === "annual" ? "a.a." : "a.m."}
                </p>
              </div>
              {liability.amortization_system && (
                <div>
                  <p className="text-sm text-muted-foreground">Sistema de Amortização</p>
                  <p className="font-semibold uppercase">{liability.amortization_system}</p>
                </div>
              )}
              {liability.start_date && (
                <div>
                  <p className="text-sm text-muted-foreground">Período</p>
                  <p className="font-semibold">
                    {formatDate(liability.start_date)} —{" "}
                    {liability.end_date ? formatDate(liability.end_date) : "Em aberto"}
                  </p>
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Cronograma de Parcelas</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {installments.length === 0 ? (
            <p className="p-6 text-center text-muted-foreground">
              Nenhuma parcela registrada ainda.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nº</TableHead>
                  <TableHead>Vencimento</TableHead>
                  <TableHead className="text-right">Total</TableHead>
                  <TableHead className="text-right">Principal</TableHead>
                  <TableHead className="text-right">Juros</TableHead>
                  <TableHead className="text-right">Seguro</TableHead>
                  <TableHead className="text-right">Pago</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {installments.map((inst) => {
                  const overdue = isOverdue(inst.due_date) && inst.status !== "paid";
                  return (
                    <TableRow
                      key={inst.id}
                      className={overdue ? "bg-red-50 dark:bg-red-950/20" : ""}
                    >
                      <TableCell className="font-mono text-sm">{inst.installment_number}</TableCell>
                      <TableCell
                        className={`text-sm ${overdue ? "font-semibold text-red-600" : ""}`}
                      >
                        {formatDate(inst.due_date)}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {formatCurrency(inst.total_amount)}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {inst.principal_amount ? formatCurrency(inst.principal_amount) : "—"}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {inst.interest_amount ? formatCurrency(inst.interest_amount) : "—"}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {inst.insurance_amount ? formatCurrency(inst.insurance_amount) : "—"}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm text-green-600">
                        {formatCurrency(inst.paid_amount)}
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={
                            inst.status === "paid"
                              ? "default"
                              : overdue
                                ? "destructive"
                                : "secondary"
                          }
                          className="text-xs"
                        >
                          {installmentStatusLabels[inst.status] ?? inst.status}
                        </Badge>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
