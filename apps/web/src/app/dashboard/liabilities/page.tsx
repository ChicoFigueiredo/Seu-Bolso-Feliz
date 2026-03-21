import { createClient } from "@/lib/supabase/server";
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
  const supabase = await createClient();
  const { data: liabilities } = await supabase
    .from("liabilities")
    .select(
      "id, name, type, original_amount, outstanding_balance, interest_rate, rate_type, total_installments, paid_installments, status, start_date, financial_products(name, institutions(name))",
    )
    .order("outstanding_balance", { ascending: false });

  const totalDebt = (liabilities ?? [])
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

      {(liabilities ?? []).length === 0 ? (
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
                {(liabilities ?? []).map((l) => {
                  const fp = l.financial_products as {
                    name: string;
                    institutions: { name: string } | null;
                  } | null;
                  const prodLabel = `${fp?.institutions?.name ?? ""} ${fp?.name ?? ""}`.trim();
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
