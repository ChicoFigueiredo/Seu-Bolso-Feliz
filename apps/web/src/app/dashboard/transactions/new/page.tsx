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
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";

export default function NewTransactionPage() {
  const router = useRouter();
  const supabase = createClient();
  const [saving, setSaving] = useState(false);
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
    const { error } = await supabase.from("transactions").insert({
      user_id: user.id,
      description: fd.get("description") as string,
      amount: Number(fd.get("amount")),
      type: fd.get("type") as string,
      event_date: fd.get("event_date") as string,
      competence_date: (fd.get("competence_date") as string) || undefined,
      financial_product_id: fd.get("product") as string,
      notes: (fd.get("notes") as string) || undefined,
      priority: (fd.get("priority") as string) || undefined,
    });
    setSaving(false);
    if (error) {
      toast.error("Erro ao salvar transação", { description: error.message });
    } else {
      toast.success("Transação criada com sucesso");
      router.push("/dashboard/transactions");
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/transactions">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <h1 className="text-2xl font-bold tracking-tight">Nova Transação</h1>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados da Transação</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
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
                    <SelectItem value="statement_payment">Pagamento Fatura</SelectItem>
                    <SelectItem value="refund">Estorno</SelectItem>
                    <SelectItem value="adjustment">Ajuste</SelectItem>
                    <SelectItem value="fee">Taxa</SelectItem>
                    <SelectItem value="interest_charge">Juros</SelectItem>
                    <SelectItem value="liability_payment">Pagamento Dívida</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="description">Descrição *</Label>
              <Input id="description" name="description" required />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="amount">Valor (R$) *</Label>
                <Input id="amount" name="amount" type="number" step="0.01" min="0" required />
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

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="event_date">Data do Evento *</Label>
                <Input id="event_date" name="event_date" type="date" required />
              </div>

              <div className="space-y-2">
                <Label htmlFor="competence_date">Data de Competência</Label>
                <Input id="competence_date" name="competence_date" type="date" />
              </div>
            </div>

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

            <div className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea id="notes" name="notes" rows={3} />
            </div>

            <div className="flex gap-3 pt-2">
              <Button type="submit" disabled={saving}>
                {saving ? "Salvando…" : "Salvar Transação"}
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
