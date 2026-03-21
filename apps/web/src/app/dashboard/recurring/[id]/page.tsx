"use client";

import { useState, useEffect, use } from "react";
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
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "sonner";
import { ArrowLeft, Trash2 } from "lucide-react";
import Link from "next/link";

export default function EditRecurringPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const router = useRouter();
  const supabase = createClient();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState({
    name: "",
    type: "expense",
    frequency: "monthly",
    day_of_month: "",
    amount: "",
    is_variable_amount: false,
    financial_product_id: "",
    priority: "",
    starts_at: "",
    ends_at: "",
    notes: "",
    is_active: true,
  });
  const [products, setProducts] = useState<
    { id: string; name: string; institution_name: string }[]
  >([]);

  useEffect(() => {
    Promise.all([
      supabase.from("recurring_templates").select("*").eq("id", id).single(),
      supabase.from("financial_products").select("id, name, institutions(name)"),
    ]).then(([{ data: t }, { data: prods }]) => {
      if (t) {
        setForm({
          name: t.name,
          type: t.type,
          frequency: t.frequency,
          day_of_month: t.day_of_month ? String(t.day_of_month) : "",
          amount: t.amount ? String(t.amount) : "",
          is_variable_amount: t.is_variable_amount,
          financial_product_id: t.financial_product_id ?? "",
          priority: t.priority ?? "",
          starts_at: t.starts_at ?? "",
          ends_at: t.ends_at ?? "",
          notes: t.notes ?? "",
          is_active: t.is_active,
        });
      }
      setProducts(
        (prods ?? []).map((p) => ({
          id: p.id,
          name: p.name,
          institution_name: (p.institutions as { name: string } | null)?.name ?? "",
        })),
      );
      setLoading(false);
    });
  }, [id, supabase]);

  async function handleUpdate(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setSaving(true);
    const { error } = await supabase
      .from("recurring_templates")
      .update({
        name: form.name,
        type: form.type,
        frequency: form.frequency,
        day_of_month: form.day_of_month ? Number(form.day_of_month) : null,
        amount: form.is_variable_amount ? null : Number(form.amount),
        is_variable_amount: form.is_variable_amount,
        financial_product_id: form.financial_product_id || null,
        priority: form.priority || null,
        starts_at: form.starts_at || null,
        ends_at: form.ends_at || null,
        notes: form.notes || null,
        is_active: form.is_active,
      })
      .eq("id", id);
    setSaving(false);
    if (error) {
      toast.error("Erro ao atualizar", { description: error.message });
    } else {
      toast.success("Template atualizado");
      router.push("/dashboard/recurring");
    }
  }

  async function handleDelete() {
    const { error } = await supabase.from("recurring_templates").delete().eq("id", id);
    if (error) toast.error("Erro ao excluir", { description: error.message });
    else {
      toast.success("Template excluído");
      router.push("/dashboard/recurring");
    }
  }

  if (loading)
    return (
      <div className="mx-auto max-w-2xl space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-[500px] w-full" />
      </div>
    );

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="icon" asChild>
            <Link href="/dashboard/recurring">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <h1 className="text-2xl font-bold tracking-tight">Editar Template</h1>
        </div>
        <Dialog>
          <DialogTrigger asChild>
            <Button variant="destructive" size="sm">
              <Trash2 className="mr-2 size-4" />
              Excluir
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Confirmar exclusão</DialogTitle>
              <DialogDescription>
                As instâncias vinculadas também serão removidas.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="destructive" onClick={handleDelete}>
                Sim, excluir
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados da Recorrência</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleUpdate} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Nome</Label>
              <Input
                id="name"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
              />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label>Tipo</Label>
                <Select value={form.type} onValueChange={(v) => setForm({ ...form, type: v })}>
                  <SelectTrigger>
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
                <Label>Frequência</Label>
                <Select
                  value={form.frequency}
                  onValueChange={(v) => setForm({ ...form, frequency: v })}
                >
                  <SelectTrigger>
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
                <Label>Dia do mês</Label>
                <Input
                  type="number"
                  min={1}
                  max={31}
                  value={form.day_of_month}
                  onChange={(e) => setForm({ ...form, day_of_month: e.target.value })}
                />
              </div>
              <div className="space-y-2">
                <Label>Prioridade</Label>
                <Select
                  value={form.priority}
                  onValueChange={(v) => setForm({ ...form, priority: v })}
                >
                  <SelectTrigger>
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

            <div className="flex items-center gap-4 py-2">
              <div className="flex items-center gap-2">
                <Switch
                  checked={form.is_variable_amount}
                  onCheckedChange={(v) => setForm({ ...form, is_variable_amount: v })}
                />
                <Label>Valor variável</Label>
              </div>
              <div className="flex items-center gap-2">
                <Switch
                  checked={form.is_active}
                  onCheckedChange={(v) => setForm({ ...form, is_active: v })}
                />
                <Label>Ativo</Label>
              </div>
            </div>

            {!form.is_variable_amount && (
              <div className="space-y-2">
                <Label>Valor (R$)</Label>
                <Input
                  type="number"
                  step="0.01"
                  min="0"
                  value={form.amount}
                  onChange={(e) => setForm({ ...form, amount: e.target.value })}
                  required
                />
              </div>
            )}

            <div className="space-y-2">
              <Label>Produto Financeiro</Label>
              <Select
                value={form.financial_product_id}
                onValueChange={(v) => setForm({ ...form, financial_product_id: v })}
              >
                <SelectTrigger>
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
                <Label>Início</Label>
                <Input
                  type="date"
                  value={form.starts_at}
                  onChange={(e) => setForm({ ...form, starts_at: e.target.value })}
                />
              </div>
              <div className="space-y-2">
                <Label>Término</Label>
                <Input
                  type="date"
                  value={form.ends_at}
                  onChange={(e) => setForm({ ...form, ends_at: e.target.value })}
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label>Observações</Label>
              <Textarea
                rows={3}
                value={form.notes}
                onChange={(e) => setForm({ ...form, notes: e.target.value })}
              />
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
