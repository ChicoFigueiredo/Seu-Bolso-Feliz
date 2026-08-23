import { createClient } from "@/lib/supabase/server";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { formatCurrency, formatDate } from "@/lib/format";
import { ReportFilters } from "./filters";
import { BarChart3, TrendingUp, TrendingDown, ArrowRightLeft } from "lucide-react";
import { resolverIntervalo } from "./report-period";

interface SearchParams {
  mode?: string;
  from?: string;
  to?: string;
  supplier?: string;
}

export default async function ReportsPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const params = await searchParams;
  const supabase = await createClient();

  // O cálculo do ciclo mora em @sbf/domain — ver report-period.ts para o
  // porquê de a versão inline anterior ter sido removida.
  const { data: prefs } =
    params.mode === "financial_period"
      ? await supabase
          .from("user_financial_preferences")
          .select("financial_cycle_start_day")
          .maybeSingle()
      : { data: null };

  const { from, to } = resolverIntervalo(params, prefs?.financial_cycle_start_day);

  const supplierId = params.supplier || null;

  // Fetch suppliers for the filter dropdown
  const suppliersRes = await supabase
    .from("suppliers")
    .select("id, name")
    .eq("is_active", true)
    .order("name");

  // Despesas vêm de `v_expenses_deduplicated`, não de `transactions` cru.
  //
  // A view aplica o ADR-001: um item de fatura já lançado como transação
  // aparece UMA vez, não duas, e `statement_payment` (o pagamento da fatura em
  // si) não entra como despesa. Somar `transactions` direto contava a compra e
  // o pagamento da fatura que a incluía — o relatório inflava exatamente na
  // proporção do que fosse pago com cartão, que é justamente a maior parte.
  //
  // A view já filtra por tipo de despesa; não há coluna `type` para filtrar
  // aqui, e não deve haver.
  const expensesQ = supabase
    .from("v_expenses_deduplicated")
    .select("canonical_id, amount, description, event_date, supplier_id, category_id")
    .gte("event_date", from)
    .lte("event_date", to);

  // Receita continua em `transactions`: a view é de despesas.
  const incomeQ = supabase
    .from("transactions")
    .select("amount")
    .eq("type", "income")
    .gte("event_date", from)
    .lte("event_date", to);

  const byTypeQ = supabase
    .from("transactions")
    .select("type, amount")
    .gte("event_date", from)
    .lte("event_date", to);

  const categoriesQ = supabase.from("categories").select("id, name");

  // Apply supplier filter when selected
  if (supplierId) {
    expensesQ.eq("supplier_id", supplierId);
    incomeQ.eq("supplier_id", supplierId);
    byTypeQ.eq("supplier_id", supplierId);
  }

  const [incomeRes, expensesRes, byTypeRes, categoriesRes] = await Promise.all([
    incomeQ,
    expensesQ,
    byTypeQ,
    categoriesQ,
  ]);

  const despesas = expensesRes.data ?? [];

  // A view não embute `categories(name)` nem `suppliers(name)`: o PostgREST só
  // faz embed onde há chave estrangeira declarada, e uma view não tem. Os nomes
  // são resolvidos por mapa, com uma consulta cada.
  const nomeDaCategoria = new Map(
    (categoriesRes.data ?? []).map((c) => [c.id as string, c.name as string]),
  );
  const nomeDoFornecedor = new Map(
    (suppliersRes.data ?? []).map((s) => [s.id as string, s.name as string]),
  );

  const totalIncome = (incomeRes.data ?? []).reduce((s, r) => s + r.amount, 0);
  const totalExpense = despesas.reduce((s, r) => s + Number(r.amount), 0);
  const balance = totalIncome - totalExpense;

  const agrupar = (chave: (r: (typeof despesas)[number]) => string) => {
    const mapa = new Map<string, number>();
    for (const row of despesas) {
      const k = chave(row);
      mapa.set(k, (mapa.get(k) ?? 0) + Number(row.amount));
    }
    return [...mapa.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
  };

  const byCategory = agrupar(
    (r) => nomeDaCategoria.get(r.category_id as string) ?? "Sem categoria",
  );
  const bySupplier = agrupar(
    (r) => nomeDoFornecedor.get(r.supplier_id as string) ?? "Sem fornecedor",
  );

  const topExpensesRes = {
    data: [...despesas]
      .sort((a, b) => Number(b.amount) - Number(a.amount))
      .slice(0, 10)
      .map((r) => ({
        description: r.description,
        amount: Number(r.amount),
        event_date: r.event_date,
        // A view já garante que só há tipos de despesa; o rótulo por linha
        // deixou de existir junto com a soma de `transactions` cru.
        type: "expense",
      })),
  };

  // Group by type
  const typeMap = new Map<string, number>();
  for (const row of byTypeRes.data ?? []) {
    typeMap.set(row.type, (typeMap.get(row.type) ?? 0) + row.amount);
  }

  const typeLabels: Record<string, string> = {
    income: "Receita",
    expense: "Despesa",
    transfer: "Transferência",
    statement_payment: "Pag. Fatura",
    refund: "Estorno",
    adjustment: "Ajuste",
    fee: "Taxa",
    interest_charge: "Juros",
    liability_payment: "Pag. Dívida",
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Relatórios</h1>
        <p className="text-muted-foreground">
          Período: {formatDate(from)} — {formatDate(to)}
        </p>
      </div>

      <ReportFilters
        current={{ mode: params.mode, from, to, supplier: supplierId ?? undefined }}
        suppliers={suppliersRes.data ?? []}
      />

      {/* Summary cards */}
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardDescription>Receita Total</CardDescription>
            <TrendingUp className="size-4 text-green-600" />
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold text-green-600">
              +{formatCurrency(totalIncome)}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardDescription>Despesa Total</CardDescription>
            <TrendingDown className="size-4 text-red-600" />
          </CardHeader>
          <CardContent>
            <span className="text-2xl font-bold text-red-600">-{formatCurrency(totalExpense)}</span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardDescription>Saldo</CardDescription>
            <ArrowRightLeft className="size-4" />
          </CardHeader>
          <CardContent>
            <span
              className={`text-2xl font-bold ${balance >= 0 ? "text-green-600" : "text-red-600"}`}
            >
              {balance >= 0 ? "+" : ""}
              {formatCurrency(balance)}
            </span>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* By Category */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BarChart3 className="size-5" />
              Despesas por Categoria
            </CardTitle>
          </CardHeader>
          <CardContent>
            {byCategory.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                Nenhuma despesa categorizada neste período.
              </p>
            ) : (
              <div className="space-y-3">
                {byCategory.map(([name, amount]) => {
                  const pct = totalExpense > 0 ? (amount / totalExpense) * 100 : 0;
                  return (
                    <div key={name} className="space-y-1">
                      <div className="flex justify-between text-sm">
                        <span className="font-medium">{name}</span>
                        <span className="text-muted-foreground">
                          {formatCurrency(amount)} ({pct.toFixed(1)}%)
                        </span>
                      </div>
                      <div className="h-2 w-full rounded-full bg-muted">
                        <div
                          className="h-full rounded-full bg-primary"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        {/* By Supplier */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BarChart3 className="size-5" />
              Despesas por Fornecedor
            </CardTitle>
          </CardHeader>
          <CardContent>
            {bySupplier.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                Nenhuma despesa com fornecedor neste período.
              </p>
            ) : (
              <div className="space-y-3">
                {bySupplier.map(([name, amount]) => {
                  const pct = totalExpense > 0 ? (amount / totalExpense) * 100 : 0;
                  return (
                    <div key={name} className="space-y-1">
                      <div className="flex justify-between text-sm">
                        <span className="font-medium">{name}</span>
                        <span className="text-muted-foreground">
                          {formatCurrency(amount)} ({pct.toFixed(1)}%)
                        </span>
                      </div>
                      <div className="h-2 w-full rounded-full bg-muted">
                        <div
                          className="h-full rounded-full bg-violet-500"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        {/* By Type */}
        <Card>
          <CardHeader>
            <CardTitle>Movimentação por Tipo</CardTitle>
          </CardHeader>
          <CardContent>
            {typeMap.size === 0 ? (
              <p className="text-sm text-muted-foreground">Nenhuma movimentação neste período.</p>
            ) : (
              <div className="space-y-3">
                {[...typeMap.entries()]
                  .sort((a, b) => b[1] - a[1])
                  .map(([type, amount]) => (
                    <div key={type} className="flex items-center justify-between">
                      <Badge variant="outline">{typeLabels[type] ?? type}</Badge>
                      <span className="font-mono text-sm font-semibold">
                        {formatCurrency(amount)}
                      </span>
                    </div>
                  ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Top expenses */}
      <Card>
        <CardHeader>
          <CardTitle>Maiores Despesas</CardTitle>
          <CardDescription>Top 10 despesas do período</CardDescription>
        </CardHeader>
        <CardContent>
          {(topExpensesRes.data ?? []).length === 0 ? (
            <p className="text-sm text-muted-foreground">Nenhuma despesa neste período.</p>
          ) : (
            <div className="space-y-2">
              {(topExpensesRes.data ?? []).map((t, i) => (
                <div
                  key={i}
                  className="flex items-center justify-between border-b py-2 last:border-0"
                >
                  <div>
                    <p className="font-medium">{t.description ?? "—"}</p>
                    <p className="text-xs text-muted-foreground">
                      {/* `event_date` é nullable na view: um item de fatura sem
                          data lançada continua sendo uma despesa real. */}
                      {t.event_date ? formatDate(t.event_date) : "sem data"}
                    </p>
                  </div>
                  <span className="font-mono text-sm font-semibold text-red-600">
                    -{formatCurrency(t.amount)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
