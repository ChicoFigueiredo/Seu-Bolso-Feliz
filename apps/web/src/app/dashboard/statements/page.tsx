import { createClient } from "@/lib/supabase/server";
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
  const supabase = await createClient();
  const { data: statements } = await supabase
    .from("statement_cycles")
    .select(
      "id, reference_month, cycle_start_date, cycle_end_date, due_date, total_amount, paid_amount, status, cards(last_four_digits, card_brand, financial_products(name, institutions(name)))",
    )
    .order("due_date", { ascending: false });

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Faturas</h1>
        <p className="text-muted-foreground">Ciclos de fatura dos seus cartões de crédito</p>
      </div>

      {(statements ?? []).length === 0 ? (
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
                {(statements ?? []).map((s) => {
                  const card = s.cards as {
                    last_four_digits: string;
                    card_brand: string | null;
                    financial_products: {
                      name: string;
                      institutions: { name: string } | null;
                    } | null;
                  } | null;
                  const inst = card?.financial_products?.institutions?.name ?? "";
                  const prod = card?.financial_products?.name ?? "";
                  const cardLabel = `${inst} ${prod} •${card?.last_four_digits ?? ""}`.trim();
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
