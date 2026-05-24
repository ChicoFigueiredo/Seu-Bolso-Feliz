import { createClient } from "@/lib/supabase/server";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { formatCurrency, formatDate } from "@/lib/format";
import { ReportFilters } from "./filters";
import { BarChart3, TrendingUp, TrendingDown, ArrowRightLeft } from "lucide-react";

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

  // Default to current month if no params
  const now = new Date();
  let from: string =
    params.from ?? `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
  let to: string = params.to ?? "";

  if (params.mode === "financial_period") {
    // Load user financial preferences to calculate period
    const { data: prefs } = await supabase
      .from("user_financial_preferences")
      .select("financial_cycle_start_day")
      .maybeSingle();

    if (prefs?.financial_cycle_start_day) {
      const day = prefs.financial_cycle_start_day;
      const today = new Date();
      const periodStart = new Date(today.getFullYear(), today.getMonth(), day);
      if (periodStart > today) periodStart.setMonth(periodStart.getMonth() - 1);
      const periodEnd = new Date(periodStart);
      periodEnd.setMonth(periodEnd.getMonth() + 1);
      periodEnd.setDate(periodEnd.getDate() - 1);
      from = periodStart.toISOString().split("T")[0]!;
      to = periodEnd.toISOString().split("T")[0]!;
    }
  }

  if (!to) {
    const d = new Date(from);
    d.setMonth(d.getMonth() + 1);
    d.setDate(0); // last day of month
    to = d.toISOString().split("T")[0]!;
  }

  const supplierId = params.supplier || null;

  // Fetch suppliers for the filter dropdown
  const suppliersRes = await supabase
    .from("suppliers")
    .select("id, name")
    .eq("is_active", true)
    .order("name");

  // Queries — each applies date range + optional supplier filter
  const incomeQ = supabase
    .from("transactions")
    .select("amount")
    .eq("type", "income")
    .gte("event_date", from)
    .lte("event_date", to);
  const expenseQ = supabase
    .from("transactions")
    .select("amount")
    .in("type", ["expense", "fee", "interest_charge"])
    .gte("event_date", from)
    .lte("event_date", to);
  const byCategoryQ = supabase
    .from("transactions")
    .select("amount, categories(name)")
    .in("type", ["expense", "fee", "interest_charge"])
    .gte("event_date", from)
    .lte("event_date", to)
    .not("category_id", "is", null)
    .order("amount", { ascending: false });
  const byTypeQ = supabase
    .from("transactions")
    .select("type, amount")
    .gte("event_date", from)
    .lte("event_date", to);
  const topExpensesQ = supabase
    .from("transactions")
    .select("description, amount, event_date, type")
    .in("type", ["expense", "fee", "interest_charge"])
    .gte("event_date", from)
    .lte("event_date", to)
    .order("amount", { ascending: false })
    .limit(10);
  const bySupplierQ = supabase
    .from("transactions")
    .select("amount, suppliers(name)")
    .in("type", ["expense", "fee", "interest_charge"])
    .gte("event_date", from)
    .lte("event_date", to)
    .not("supplier_id", "is", null)
    .order("amount", { ascending: false });

  // Apply supplier filter when selected
  if (supplierId) {
    incomeQ.eq("supplier_id", supplierId);
    expenseQ.eq("supplier_id", supplierId);
    byCategoryQ.eq("supplier_id", supplierId);
    byTypeQ.eq("supplier_id", supplierId);
    topExpensesQ.eq("supplier_id", supplierId);
    bySupplierQ.eq("supplier_id", supplierId);
  }

  const [incomeRes, expenseRes, byCategoryRes, byTypeRes, topExpensesRes, bySupplierRes] =
    await Promise.all([incomeQ, expenseQ, byCategoryQ, byTypeQ, topExpensesQ, bySupplierQ]);

  const totalIncome = (incomeRes.data ?? []).reduce((s, r) => s + r.amount, 0);
  const totalExpense = (expenseRes.data ?? []).reduce((s, r) => s + r.amount, 0);
  const balance = totalIncome - totalExpense;

  // Group by category
  const catMap = new Map<string, number>();
  for (const row of byCategoryRes.data ?? []) {
    const catName = (row.categories as { name: string } | null)?.name ?? "Sem categoria";
    catMap.set(catName, (catMap.get(catName) ?? 0) + row.amount);
  }
  const byCategory = [...catMap.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);

  // Group by type
  const typeMap = new Map<string, number>();
  for (const row of byTypeRes.data ?? []) {
    typeMap.set(row.type, (typeMap.get(row.type) ?? 0) + row.amount);
  }

  // Group by supplier
  const supplierMap = new Map<string, number>();
  for (const row of bySupplierRes.data ?? []) {
    const supplierName = (row.suppliers as { name: string } | null)?.name ?? "Sem fornecedor";
    supplierMap.set(supplierName, (supplierMap.get(supplierName) ?? 0) + row.amount);
  }
  const bySupplier = [...supplierMap.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);

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
                    <p className="text-xs text-muted-foreground">{formatDate(t.event_date)}</p>
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
