import { createClient } from "@/lib/supabase/server";
import Link from "next/link";
import { Plus, Receipt, FileText } from "lucide-react";
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
import { formatCurrency, formatDate } from "@/lib/format";
import { TransactionFilters } from "./filters";

const typeLabels: Record<string, string> = {
  income: "Receita",
  expense: "Despesa",
  statement_payment: "Pagamento Fatura",
  liability_payment: "Pagamento Dívida",
  refund: "Estorno",
  adjustment: "Ajuste",
  fee: "Taxa",
  interest_charge: "Juros",
};

interface SearchParams {
  type?: string;
  confirmed?: string;
  from?: string;
  to?: string;
  page?: string;
}

export default async function TransactionsPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const params = await searchParams;
  const supabase = await createClient();

  let query = supabase
    .from("transactions")
    .select(
      "id, description, amount, event_date, type, is_confirmed, priority, supplier_id, source_document_id, suppliers(name)",
      { count: "exact" },
    )
    .order("event_date", { ascending: false });

  if (params.type) query = query.eq("type", params.type);
  if (params.confirmed === "true") query = query.eq("is_confirmed", true);
  if (params.confirmed === "false") query = query.eq("is_confirmed", false);
  if (params.from) query = query.gte("event_date", params.from);
  if (params.to) query = query.lte("event_date", params.to);

  const page = Number(params.page ?? "1");
  const perPage = 25;
  query = query.range((page - 1) * perPage, page * perPage - 1);

  const { data: transactions, count } = await query;
  const totalPages = Math.ceil((count ?? 0) / perPage);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Transações</h1>
          <p className="text-muted-foreground">{count ?? 0} transações registradas</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/transactions/new">
            <Plus className="mr-2 size-4" />
            Nova Transação
          </Link>
        </Button>
      </div>

      <TransactionFilters current={params} />

      {(transactions ?? []).length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Receipt className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma transação encontrada</CardTitle>
            <CardDescription className="mb-4">
              {params.type || params.confirmed || params.from
                ? "Tente ajustar os filtros."
                : "Comece registrando suas transações financeiras."}
            </CardDescription>
            {!params.type && !params.confirmed && !params.from && (
              <Button asChild>
                <Link href="/dashboard/transactions/new">
                  <Plus className="mr-2 size-4" />
                  Registrar Primeira Transação
                </Link>
              </Button>
            )}
          </CardContent>
        </Card>
      ) : (
        <>
          <Card>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Data</TableHead>
                    <TableHead>Descrição</TableHead>
                    <TableHead>Fornecedor</TableHead>
                    <TableHead>Tipo</TableHead>
                    <TableHead>Confirmado</TableHead>
                    <TableHead className="text-right">Valor</TableHead>
                    <TableHead>Documento</TableHead>
                    <TableHead className="text-right">Ações</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {(transactions ?? []).map((t) => (
                    <TableRow key={t.id}>
                      <TableCell className="text-sm">{formatDate(t.event_date)}</TableCell>
                      <TableCell className="max-w-[200px] truncate font-medium">
                        {t.description ?? "—"}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {(t.suppliers as { name: string } | null)?.name ?? "—"}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline" className="text-xs">
                          {typeLabels[t.type] ?? t.type}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={t.is_confirmed ? "default" : "secondary"}
                          className="text-xs"
                        >
                          {t.is_confirmed ? "Confirmado" : "Previsto"}
                        </Badge>
                      </TableCell>
                      <TableCell
                        className={`text-right font-mono text-sm ${
                          t.type === "income" || t.type === "refund"
                            ? "text-green-600"
                            : "text-red-600"
                        }`}
                      >
                        {t.type === "income" || t.type === "refund" ? "+" : "-"}
                        {formatCurrency(Math.abs(t.amount))}
                      </TableCell>
                      <TableCell>
                        {(t as { source_document_id?: string | null }).source_document_id ? (
                          <Button variant="ghost" size="sm" asChild className="gap-1 px-2">
                            <Link
                              href={`/dashboard/documents/${(t as { source_document_id: string }).source_document_id}`}
                            >
                              <FileText className="size-3.5" />
                              Ver
                            </Link>
                          </Button>
                        ) : (
                          <span className="text-xs text-muted-foreground">—</span>
                        )}
                      </TableCell>
                      <TableCell className="text-right">
                        <Button variant="ghost" size="sm" asChild>
                          <Link href={`/dashboard/transactions/${t.id}`}>Editar</Link>
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>

          {/* Pagination */}
          {totalPages > 1 && (
            <div className="flex items-center justify-center gap-2">
              {page > 1 && (
                <Button variant="outline" size="sm" asChild>
                  <Link
                    href={`/dashboard/transactions?${new URLSearchParams({ ...params, page: String(page - 1) })}`}
                  >
                    Anterior
                  </Link>
                </Button>
              )}
              <span className="text-sm text-muted-foreground">
                Página {page} de {totalPages}
              </span>
              {page < totalPages && (
                <Button variant="outline" size="sm" asChild>
                  <Link
                    href={`/dashboard/transactions?${new URLSearchParams({ ...params, page: String(page + 1) })}`}
                  >
                    Próxima
                  </Link>
                </Button>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
