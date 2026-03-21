import { createClient } from "@/lib/supabase/server";
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
  const supabase = await createClient();

  const { data: statement } = await supabase
    .from("statement_cycles")
    .select("*, cards(last_four_digits, card_brand, financial_products(name, institutions(name)))")
    .eq("id", id)
    .single();

  if (!statement) redirect("/dashboard/statements");

  const { data: items } = await supabase
    .from("statement_items")
    .select("id, description, amount, transaction_date, installment_number, total_installments")
    .eq("statement_cycle_id", id)
    .order("transaction_date", { ascending: false });

  const card = statement.cards as {
    last_four_digits: string;
    card_brand: string | null;
    financial_products: { name: string; institutions: { name: string } | null } | null;
  } | null;
  const cardLabel =
    `${card?.financial_products?.institutions?.name ?? ""} ${card?.financial_products?.name ?? ""} •${card?.last_four_digits ?? ""}`.trim();
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
          {(items ?? []).length === 0 ? (
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
                {(items ?? []).map((item) => (
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
