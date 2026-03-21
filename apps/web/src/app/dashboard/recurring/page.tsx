import { createClient } from "@/lib/supabase/server";
import Link from "next/link";
import { Plus, Repeat } from "lucide-react";
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
  income: "Receita",
  expense: "Despesa",
  liability_payment: "Pag. Dívida",
  statement_payment: "Pag. Fatura",
};

const freqLabels: Record<string, string> = {
  monthly: "Mensal",
  weekly: "Semanal",
  biweekly: "Quinzenal",
  quarterly: "Trimestral",
  annual: "Anual",
  custom: "Personalizado",
};

export default async function RecurringPage() {
  const supabase = await createClient();
  const { data: templates } = await supabase
    .from("recurring_templates")
    .select(
      "id, name, type, amount, is_variable_amount, frequency, day_of_month, priority, is_active",
    )
    .order("name");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Recorrências</h1>
          <p className="text-muted-foreground">Templates de receitas e despesas recorrentes</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/recurring/new">
            <Plus className="mr-2 size-4" />
            Novo Template
          </Link>
        </Button>
      </div>

      {(templates ?? []).length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Repeat className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma recorrência cadastrada</CardTitle>
            <CardDescription className="mb-4">
              Cadastre templates para despesas e receitas que se repetem.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/recurring/new">
                <Plus className="mr-2 size-4" />
                Primeiro Template
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
                  <TableHead>Tipo</TableHead>
                  <TableHead>Frequência</TableHead>
                  <TableHead>Dia</TableHead>
                  <TableHead className="text-right">Valor</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(templates ?? []).map((t) => (
                  <TableRow key={t.id}>
                    <TableCell className="font-medium">{t.name}</TableCell>
                    <TableCell>
                      <Badge variant="outline" className="text-xs">
                        {typeLabels[t.type] ?? t.type}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm">
                      {freqLabels[t.frequency] ?? t.frequency}
                    </TableCell>
                    <TableCell className="text-sm">{t.day_of_month ?? "—"}</TableCell>
                    <TableCell className="text-right font-mono text-sm">
                      {t.is_variable_amount ? "Variável" : formatCurrency(t.amount ?? 0)}
                    </TableCell>
                    <TableCell>
                      <Badge variant={t.is_active ? "default" : "secondary"} className="text-xs">
                        {t.is_active ? "Ativo" : "Inativo"}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" asChild>
                        <Link href={`/dashboard/recurring/${t.id}`}>Editar</Link>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
