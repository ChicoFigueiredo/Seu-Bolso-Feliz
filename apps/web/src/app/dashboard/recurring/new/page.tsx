"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { createClient } from "@/lib/supabase/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";

export default function NewRecurringPage() {
  const router = useRouter();
  const supabase = createClient();
  const [saving, setSaving] = useState(false);
  const [isVariable, setIsVariable] = useState(false);
  const [products, setProducts] = useState<
    { id: string; name: string; institution_name: string }[]
  >([]);

  useEffect(() => {
    supabase
      .from("financial_products")
      .select("id, name, institutions(name)")
      .then(({ data }) => {
        setProducts(
          (data ?? []).map((p) => ({
            id: p.id,
            name: p.name,
            institution_name: (p.institutions as { name: string } | null)?.name ?? "",
          })),
        );
      });
  }, [supabase]);

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setSaving(true);
    const fd = new FormData(e.currentTarget);
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setSaving(false);
      return;
    }
    const { error } = await supabase.from("recurring_templates").insert({
      user_id: user.id,
      name: fd.get("name") as string,
      type: fd.get("type") as string,
      frequency: fd.get("frequency") as string,
      day_of_month: fd.get("day_of_month") ? Number(fd.get("day_of_month")) : null,
      amount: isVariable ? null : Number(fd.get("amount")),
      is_variable_amount: isVariable,
      financial_product_id: (fd.get("product") as string) || null,
      priority: (fd.get("priority") as string) || null,
      starts_at: (fd.get("starts_at") as string) || null,
      ends_at: (fd.get("ends_at") as string) || null,
      notes: (fd.get("notes") as string) || null,
    });
    setSaving(false);
    if (error) {
      toast.error("Erro ao salvar", { description: error.message });
    } else {
      toast.success("Template criado com sucesso");
      router.push("/dashboard/recurring");
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/recurring">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <h1 className="text-2xl font-bold tracking-tight">Novo Template Recorrente</h1>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados da Recorrência</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Nome *</Label>
              <Input id="name" name="name" required placeholder="Ex: Aluguel, Internet, Salário" />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="type">Tipo *</Label>
                <Select name="type" required defaultValue="expense">
                  <SelectTrigger id="type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="income">Receita</SelectItem>
                    <SelectItem value="expense">Despesa</SelectItem>
                    <SelectItem value="liability_payment">Pag. Dívida</SelectItem>
                    <SelectItem value="statement_payment">Pag. Fatura</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="frequency">Frequência *</Label>
                <Select name="frequency" required defaultValue="monthly">
                  <SelectTrigger id="frequency">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="monthly">Mensal</SelectItem>
                    <SelectItem value="weekly">Semanal</SelectItem>
                    <SelectItem value="biweekly">Quinzenal</SelectItem>
                    <SelectItem value="quarterly">Trimestral</SelectItem>
                    <SelectItem value="annual">Anual</SelectItem>
                    <SelectItem value="custom">Personalizado</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="day_of_month">Dia do mês</Label>
                <Input
                  id="day_of_month"
                  name="day_of_month"
                  type="number"
                  min={1}
                  max={31}
                  placeholder="1-31"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="priority">Prioridade</Label>
                <Select name="priority">
                  <SelectTrigger id="priority">
                    <SelectValue placeholder="Nenhuma" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="essential">Essencial</SelectItem>
                    <SelectItem value="high">Alta</SelectItem>
                    <SelectItem value="medium">Média</SelectItem>
                    <SelectItem value="low">Baixa</SelectItem>
                    <SelectItem value="optional">Opcional</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="flex items-center gap-2 py-2">
              <Switch id="variable" checked={isVariable} onCheckedChange={setIsVariable} />
              <Label htmlFor="variable">Valor variável</Label>
            </div>

            {!isVariable && (
              <div className="space-y-2">
                <Label htmlFor="amount">Valor (R$) *</Label>
                <Input id="amount" name="amount" type="number" step="0.01" min="0" required />
              </div>
            )}

            <div className="space-y-2">
              <Label htmlFor="product">Produto Financeiro</Label>
              <Select name="product">
                <SelectTrigger id="product">
                  <SelectValue placeholder="Selecione (opcional)" />
                </SelectTrigger>
                <SelectContent>
                  {products.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.institution_name} — {p.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="starts_at">Início</Label>
                <Input id="starts_at" name="starts_at" type="date" />
              </div>
              <div className="space-y-2">
                <Label htmlFor="ends_at">Término</Label>
                <Input id="ends_at" name="ends_at" type="date" />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea id="notes" name="notes" rows={3} />
            </div>

            <div className="flex gap-3 pt-2">
              <Button type="submit" disabled={saving}>
                {saving ? "Salvando…" : "Salvar"}
              </Button>
              <Button variant="outline" asChild>
                <Link href="/dashboard/recurring">Cancelar</Link>
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
