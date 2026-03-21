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

export default function EditTransactionPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const router = useRouter();
  const supabase = createClient();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState({
    description: "",
    amount: "",
    type: "expense",
    is_confirmed: false,
    event_date: "",
    competence_date: "",
    financial_product_id: "",
    notes: "",
    priority: "",
  });
  const [products, setProducts] = useState<
    { id: string; name: string; institution_name: string }[]
  >([]);

  useEffect(() => {
    Promise.all([
      supabase.from("transactions").select("*").eq("id", id).single(),
      supabase.from("financial_products").select("id, name, institutions(name)"),
    ]).then(([{ data: t }, { data: prods }]) => {
      if (t) {
        setForm({
          description: t.description ?? "",
          amount: String(t.amount),
          type: t.type,
          is_confirmed: t.is_confirmed ?? false,
          event_date: t.event_date ?? "",
          competence_date: t.competence_date ?? "",
          financial_product_id: t.financial_product_id ?? "",
          notes: t.notes ?? "",
          priority: t.priority ?? "",
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
      .from("transactions")
      .update({
        description: form.description,
        amount: Number(form.amount),
        type: form.type,
        is_confirmed: form.is_confirmed,
        event_date: form.event_date,
        competence_date: form.competence_date || undefined,
        financial_product_id: form.financial_product_id || undefined,
        notes: form.notes || undefined,
        priority: form.priority || undefined,
      })
      .eq("id", id);
    setSaving(false);
    if (error) {
      toast.error("Erro ao atualizar", { description: error.message });
    } else {
      toast.success("Transação atualizada");
      router.push("/dashboard/transactions");
    }
  }

  async function handleDelete() {
    const { error } = await supabase.from("transactions").delete().eq("id", id);
    if (error) {
      toast.error("Erro ao excluir", { description: error.message });
    } else {
      toast.success("Transação excluída");
      router.push("/dashboard/transactions");
    }
  }

  if (loading) {
    return (
      <div className="mx-auto max-w-2xl space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-[500px] w-full" />
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="icon" asChild>
            <Link href="/dashboard/transactions">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <h1 className="text-2xl font-bold tracking-tight">Editar Transação</h1>
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
                Tem certeza que deseja excluir esta transação? Esta ação não pode ser desfeita.
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
          <CardTitle>Dados da Transação</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleUpdate} className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="type">Tipo</Label>
                <Select value={form.type} onValueChange={(v) => setForm({ ...form, type: v })}>
                  <SelectTrigger id="type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="income">Receita</SelectItem>
                    <SelectItem value="expense">Despesa</SelectItem>
                    <SelectItem value="statement_payment">Pagamento Fatura</SelectItem>
                    <SelectItem value="liability_payment">Pagamento Dívida</SelectItem>
                    <SelectItem value="refund">Estorno</SelectItem>
                    <SelectItem value="adjustment">Ajuste</SelectItem>
                    <SelectItem value="fee">Taxa</SelectItem>
                    <SelectItem value="interest_charge">Juros</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="is_confirmed">Confirmado</Label>
                <Select
                  value={form.is_confirmed ? "true" : "false"}
                  onValueChange={(v) => setForm({ ...form, is_confirmed: v === "true" })}
                >
                  <SelectTrigger id="is_confirmed">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="true">Sim</SelectItem>
                    <SelectItem value="false">Não (Previsto)</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="description">Descrição</Label>
              <Input
                id="description"
                value={form.description}
                onChange={(e) => setForm({ ...form, description: e.target.value })}
                required
              />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="amount">Valor (R$)</Label>
                <Input
                  id="amount"
                  type="number"
                  step="0.01"
                  min="0"
                  value={form.amount}
                  onChange={(e) => setForm({ ...form, amount: e.target.value })}
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="priority">Prioridade</Label>
                <Select
                  value={form.priority}
                  onValueChange={(v) => setForm({ ...form, priority: v })}
                >
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

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="event_date">Data do Evento</Label>
                <Input
                  id="event_date"
                  type="date"
                  value={form.event_date}
                  onChange={(e) => setForm({ ...form, event_date: e.target.value })}
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="competence_date">Data de Competência</Label>
                <Input
                  id="competence_date"
                  type="date"
                  value={form.competence_date}
                  onChange={(e) => setForm({ ...form, competence_date: e.target.value })}
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="product">Produto Financeiro</Label>
              <Select
                value={form.financial_product_id}
                onValueChange={(v) => setForm({ ...form, financial_product_id: v })}
              >
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

            <div className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea
                id="notes"
                rows={3}
                value={form.notes}
                onChange={(e) => setForm({ ...form, notes: e.target.value })}
              />
            </div>

            <div className="flex gap-3 pt-2">
              <Button type="submit" disabled={saving}>
                {saving ? "Salvando…" : "Salvar Alterações"}
              </Button>
              <Button variant="outline" asChild>
                <Link href="/dashboard/transactions">Cancelar</Link>
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
