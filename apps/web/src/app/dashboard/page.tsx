import { createClient } from "@/lib/supabase/server";
import { redirect } from "next/navigation";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  formatCurrency,
  formatDate,
  formatRelativeDate,
  isOverdue,
  isDueToday,
} from "@/lib/format";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Calendar,
  Clock,
  CreditCard,
  TrendingDown,
  Wallet,
} from "lucide-react";
import { prioritizeItems } from "@sbf/domain/priority";

export default async function DashboardPage() {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) redirect("/login");

  const now = new Date();
  const startOfMonth = new Date(now.getFullYear(), now.getMonth(), 1).toISOString().split("T")[0]!;
  const endOfMonth = new Date(now.getFullYear(), now.getMonth() + 1, 0)
    .toISOString()
    .split("T")[0]!;
  const next30days = new Date(now.getTime() + 30 * 86400000).toISOString().split("T")[0]!;
  const today = now.toISOString().split("T")[0]!;

  // Parallel queries for dashboard data
  const [
    { data: upcomingDue },
    { data: recentTransactions },
    ,
    { data: openStatements },
    { data: activeLiabilities },
    { data: recurringInstances },
    { data: monthExpenses },
    { data: monthIncome },
  ] = await Promise.all([
    // Upcoming due: transactions that are unpaid and due soon
    supabase
      .from("transactions")
      .select("id, description, amount, event_date, priority, type, category_id, supplier_id")
      .gte("event_date", today)
      .lte("event_date", next30days)
      .in("type", ["expense", "fee", "interest_charge", "statement_payment"])
      .eq("is_confirmed", false)
      .order("event_date", { ascending: true })
      .limit(20),
    // Recent transactions
    supabase
      .from("transactions")
      .select("id, description, amount, event_date, type, is_confirmed")
      .order("event_date", { ascending: false })
      .limit(5),
    // Institutions summary
    supabase.from("institutions").select("id, name, type").order("name"),
    // Open statement cycles
    supabase
      .from("statement_cycles")
      .select("id, due_date, total_amount, status, card_id, cards(last_four_digits)")
      .in("status", ["open", "closed"])
      .order("due_date", { ascending: true })
      .limit(10),
    // Active liabilities
    supabase
      .from("liabilities")
      .select("id, name, original_amount, outstanding_balance, type, interest_rate")
      .eq("status", "active")
      .order("outstanding_balance", { ascending: false })
      .limit(10),
    // Pending recurring instances
    supabase
      .from("recurring_instances")
      .select(
        "id, expected_date, expected_amount, status, recurring_template_id, recurring_templates(name)",
      )
      .eq("status", "pending")
      .gte("expected_date", today)
      .lte("expected_date", next30days)
      .order("expected_date", { ascending: true })
      .limit(15),
    // This month expenses
    supabase
      .from("transactions")
      .select("amount")
      .gte("event_date", startOfMonth)
      .lte("event_date", endOfMonth)
      .in("type", ["expense", "fee", "interest_charge"]),
    // This month income
    supabase
      .from("transactions")
      .select("amount")
      .gte("event_date", startOfMonth)
      .lte("event_date", endOfMonth)
      .eq("type", "income"),
  ]);

  const totalExpenses = (monthExpenses ?? []).reduce((sum, t) => sum + Math.abs(t.amount), 0);
  const totalIncome = (monthIncome ?? []).reduce((sum, t) => sum + t.amount, 0);
  const balance = totalIncome - totalExpenses;
  const totalDebt = (activeLiabilities ?? []).reduce(
    (sum, l) => sum + (l.outstanding_balance ?? 0),
    0,
  );

  // Prioritize upcoming items
  const priorityItems = prioritizeItems(
    (upcomingDue ?? []).map((t) => ({
      id: t.id,
      priority: (t.priority as "essential" | "high" | "medium" | "low" | "optional") ?? "medium",
      dueDate: t.event_date,
      tags: [],
      amount: Math.abs(t.amount),
      description: t.description ?? "",
    })),
    now,
  );

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Visão Geral</h1>
        <p className="text-muted-foreground">
          O que precisa da sua atenção agora — {formatDate(now)}
        </p>
      </div>

      {/* Summary Cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Receita do Mês</CardTitle>
            <ArrowUp className="size-4 text-green-600" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">{formatCurrency(totalIncome)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Despesas do Mês</CardTitle>
            <ArrowDown className="size-4 text-red-600" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-red-600">{formatCurrency(totalExpenses)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Saldo do Mês</CardTitle>
            <Wallet className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div
              className={`text-2xl font-bold ${balance >= 0 ? "text-green-600" : "text-red-600"}`}
            >
              {formatCurrency(balance)}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Dívida Total</CardTitle>
            <TrendingDown className="size-4 text-orange-600" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-600">{formatCurrency(totalDebt)}</div>
            <p className="text-xs text-muted-foreground">
              {(activeLiabilities ?? []).length} passivo(s) ativo(s)
            </p>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* Priority Queue - What to pay first */}
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <AlertTriangle className="size-5 text-orange-500" />
              Fila de Prioridade
            </CardTitle>
            <CardDescription>O que pagar primeiro nos próximos 30 dias</CardDescription>
          </CardHeader>
          <CardContent>
            {priorityItems.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                Nenhum pagamento pendente nos próximos 30 dias.
              </p>
            ) : (
              <div className="space-y-3">
                {priorityItems.slice(0, 8).map((item) => (
                  <div
                    key={item.id}
                    className={`flex items-center gap-3 rounded-lg border p-3 ${
                      isOverdue(item.dueDate ?? "")
                        ? "border-red-300 bg-red-50 dark:border-red-900 dark:bg-red-950"
                        : isDueToday(item.dueDate ?? "")
                          ? "border-orange-300 bg-orange-50 dark:border-orange-900 dark:bg-orange-950"
                          : ""
                    }`}
                  >
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-sm font-medium">{item.description}</p>
                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Calendar className="size-3" />
                        {item.dueDate ? formatRelativeDate(item.dueDate) : "—"}
                        {isOverdue(item.dueDate ?? "") && (
                          <Badge variant="destructive" className="text-[10px] px-1 py-0">
                            ATRASADO
                          </Badge>
                        )}
                        {isDueToday(item.dueDate ?? "") && (
                          <Badge className="bg-orange-500 text-[10px] px-1 py-0">HOJE</Badge>
                        )}
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="text-sm font-semibold">{formatCurrency(item.amount ?? 0)}</p>
                      <PriorityBadge priority={item.priority} />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Upcoming Recurring */}
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Clock className="size-5 text-blue-500" />
              Recorrências Próximas
            </CardTitle>
            <CardDescription>Contas recorrentes nos próximos 30 dias</CardDescription>
          </CardHeader>
          <CardContent>
            {(recurringInstances ?? []).length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                Nenhuma recorrência pendente.
              </p>
            ) : (
              <div className="space-y-3">
                {(recurringInstances ?? []).slice(0, 8).map((inst) => (
                  <div key={inst.id} className="flex items-center gap-3 rounded-lg border p-3">
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-sm font-medium">
                        {(inst.recurring_templates as { name: string } | null)?.name ??
                          "Recorrência"}
                      </p>
                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Calendar className="size-3" />
                        {formatDate(inst.expected_date)}
                      </div>
                    </div>
                    <p className="text-sm font-semibold">
                      {inst.expected_amount ? formatCurrency(inst.expected_amount) : "—"}
                    </p>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Open Statements / Invoices */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <CreditCard className="size-5 text-purple-500" />
              Faturas em Aberto
            </CardTitle>
            <CardDescription>Faturas de cartão pendentes</CardDescription>
          </CardHeader>
          <CardContent>
            {(openStatements ?? []).length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                Nenhuma fatura em aberto.
              </p>
            ) : (
              <div className="space-y-3">
                {(openStatements ?? []).map((st) => (
                  <div
                    key={st.id}
                    className={`flex items-center gap-3 rounded-lg border p-3 ${
                      st.due_date && isOverdue(st.due_date)
                        ? "border-red-300 bg-red-50 dark:border-red-900 dark:bg-red-950"
                        : ""
                    }`}
                  >
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-sm font-medium">
                        {(st.cards as { last_four_digits: string } | null)?.last_four_digits
                          ? `Cartão •${(st.cards as { last_four_digits: string }).last_four_digits}`
                          : "Cartão"}
                      </p>
                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Calendar className="size-3" />
                        Vence {st.due_date ? formatDate(st.due_date) : "—"}
                        {st.due_date && isOverdue(st.due_date) && (
                          <Badge variant="destructive" className="text-[10px] px-1 py-0">
                            ATRASADO
                          </Badge>
                        )}
                      </div>
                    </div>
                    <p className="text-sm font-semibold">
                      {st.total_amount ? formatCurrency(st.total_amount) : "—"}
                    </p>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Recent Transactions */}
        <Card>
          <CardHeader>
            <CardTitle>Últimas Transações</CardTitle>
            <CardDescription>Movimentações recentes</CardDescription>
          </CardHeader>
          <CardContent>
            {(recentTransactions ?? []).length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                Nenhuma transação registrada ainda.
              </p>
            ) : (
              <div className="space-y-3">
                {(recentTransactions ?? []).map((t) => (
                  <div key={t.id} className="flex items-center gap-3 rounded-lg border p-3">
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-sm font-medium">{t.description ?? "—"}</p>
                      <p className="text-xs text-muted-foreground">{formatDate(t.event_date)}</p>
                    </div>
                    <div className="text-right">
                      <p
                        className={`text-sm font-semibold ${
                          t.type === "income" ? "text-green-600" : "text-red-600"
                        }`}
                      >
                        {t.type === "income" ? "+" : "-"}
                        {formatCurrency(Math.abs(t.amount))}
                      </p>
                      <Badge
                        variant={t.is_confirmed ? "default" : "secondary"}
                        className="text-[10px]"
                      >
                        {t.is_confirmed ? "Confirmado" : "Previsto"}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function PriorityBadge({ priority }: { priority: string | null | undefined }) {
  const config: Record<string, { label: string; className: string }> = {
    essential: {
      label: "Essencial",
      className: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200",
    },
    high: {
      label: "Alta",
      className: "bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200",
    },
    medium: {
      label: "Média",
      className: "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200",
    },
    low: {
      label: "Baixa",
      className: "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200",
    },
    optional: {
      label: "Opcional",
      className: "bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-200",
    },
  };
  const p = config[priority ?? "medium"] ?? config["medium"]!;
  return (
    <Badge variant="outline" className={`text-[10px] px-1 py-0 ${p!.className}`}>
      {p!.label}
    </Badge>
  );
}
