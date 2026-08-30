import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, lte, sql, type SQL } from "drizzle-orm";
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
  const { ownerId } = await requireUser();
  const db = getDb();

  // Sem RLS, o filtro por dono precisa estar explícito nesta consulta e no
  // JOIN abaixo (defesa em profundidade — ver apps/web/src/app/actions/painel-hoje.ts).
  const conditions: SQL[] = [eq(schema.transactions.user_id, ownerId)];
  if (params.type) conditions.push(eq(schema.transactions.type, params.type));
  if (params.confirmed === "true") conditions.push(eq(schema.transactions.is_confirmed, true));
  if (params.confirmed === "false") conditions.push(eq(schema.transactions.is_confirmed, false));
  if (params.from) conditions.push(gte(schema.transactions.event_date, params.from));
  if (params.to) conditions.push(lte(schema.transactions.event_date, params.to));
  const where = and(...conditions);

  const page = Number(params.page ?? "1");
  const perPage = 25;

  const [transactions, [countRow]] = await Promise.all([
    db
      .select({
        id: schema.transactions.id,
        description: schema.transactions.description,
        amount: schema.transactions.amount,
        event_date: schema.transactions.event_date,
        type: schema.transactions.type,
        is_confirmed: schema.transactions.is_confirmed,
        priority: schema.transactions.priority,
        supplier_id: schema.transactions.supplier_id,
        source_document_id: schema.transactions.source_document_id,
        supplier_name: schema.suppliers.name,
      })
      .from(schema.transactions)
      .leftJoin(
        schema.suppliers,
        and(
          eq(schema.suppliers.id, schema.transactions.supplier_id),
          eq(schema.suppliers.user_id, ownerId),
        ),
      )
      .where(where)
      .orderBy(desc(schema.transactions.event_date))
      .limit(perPage)
      .offset((page - 1) * perPage),
    db
      .select({ count: sql<number>`count(*)` })
      .from(schema.transactions)
      .where(where),
  ]);

  const count = Number(countRow?.count ?? 0);
  const totalPages = Math.ceil(count / perPage);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Transações</h1>
          <p className="text-muted-foreground">{count} transações registradas</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/transactions/new">
            <Plus className="mr-2 size-4" />
            Nova Transação
          </Link>
        </Button>
      </div>

      <TransactionFilters current={params} />

      {transactions.length === 0 ? (
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
                  {transactions.map((t) => (
                    <TableRow key={t.id}>
                      <TableCell className="text-sm">{formatDate(t.event_date)}</TableCell>
                      <TableCell className="max-w-[200px] truncate font-medium">
                        {t.description ?? "—"}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {t.supplier_name ?? "—"}
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
                        {t.source_document_id ? (
                          <Button variant="ghost" size="sm" asChild className="gap-1 px-2">
                            <Link href={`/dashboard/documents/${t.source_document_id}`}>
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
