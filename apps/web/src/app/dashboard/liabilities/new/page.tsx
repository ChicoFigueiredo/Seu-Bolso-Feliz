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
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";

export default function NewLiabilityPage() {
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
    const { error } = await supabase.from("liabilities").insert({
      user_id: user.id,
      name: fd.get("name") as string,
      type: fd.get("type") as string,
      financial_product_id: fd.get("product") as string,
      original_amount: Number(fd.get("original_amount")),
      outstanding_balance: Number(fd.get("outstanding_balance")),
      interest_rate: fd.get("interest_rate") ? Number(fd.get("interest_rate")) / 100 : null,
      rate_type: (fd.get("rate_type") as string) || null,
      amortization_system: (fd.get("amortization") as string) || null,
      total_installments: fd.get("total_installments")
        ? Number(fd.get("total_installments"))
        : null,
      start_date: (fd.get("start_date") as string) || null,
      end_date: (fd.get("end_date") as string) || null,
    });
    setSaving(false);
    if (error) {
      toast.error("Erro ao salvar", { description: error.message });
    } else {
      toast.success("Dívida cadastrada com sucesso");
      router.push("/dashboard/liabilities");
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/dashboard/liabilities">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <h1 className="text-2xl font-bold tracking-tight">Nova Dívida</h1>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados da Dívida</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Nome *</Label>
              <Input
                id="name"
                name="name"
                required
                placeholder="Ex: Financiamento Caixa, Empréstimo Nubank"
              />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="type">Tipo *</Label>
                <Select name="type" required defaultValue="personal_loan">
                  <SelectTrigger id="type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="personal_loan">Empréstimo Pessoal</SelectItem>
                    <SelectItem value="mortgage">Financiamento Imobiliário</SelectItem>
                    <SelectItem value="overdraft">Cheque Especial</SelectItem>
                    <SelectItem value="installment_plan">Parcelamento</SelectItem>
                    <SelectItem value="other">Outro</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="product">Produto Financeiro *</Label>
                <Select name="product" required>
                  <SelectTrigger id="product">
                    <SelectValue placeholder="Selecione" />
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
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="original_amount">Valor Original (R$) *</Label>
                <Input
                  id="original_amount"
                  name="original_amount"
                  type="number"
                  step="0.01"
                  min="0"
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="outstanding_balance">Saldo Devedor Atual (R$) *</Label>
                <Input
                  id="outstanding_balance"
                  name="outstanding_balance"
                  type="number"
                  step="0.01"
                  min="0"
                  required
                />
              </div>
            </div>

            <div className="grid grid-cols-3 gap-4">
              <div className="space-y-2">
                <Label htmlFor="interest_rate">Taxa de Juros (%)</Label>
                <Input
                  id="interest_rate"
                  name="interest_rate"
                  type="number"
                  step="0.01"
                  min="0"
                  placeholder="Ex: 1.49"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="rate_type">Período da Taxa</Label>
                <Select name="rate_type">
                  <SelectTrigger id="rate_type">
                    <SelectValue placeholder="—" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="monthly">Mensal</SelectItem>
                    <SelectItem value="annual">Anual</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="amortization">Sistema de Amortização</Label>
                <Select name="amortization">
                  <SelectTrigger id="amortization">
                    <SelectValue placeholder="—" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="sac">SAC</SelectItem>
                    <SelectItem value="price">Price</SelectItem>
                    <SelectItem value="mixed">Misto</SelectItem>
                    <SelectItem value="other">Outro</SelectItem>
                    <SelectItem value="none">Nenhum</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid grid-cols-3 gap-4">
              <div className="space-y-2">
                <Label htmlFor="total_installments">Total de Parcelas</Label>
                <Input id="total_installments" name="total_installments" type="number" min="1" />
              </div>
              <div className="space-y-2">
                <Label htmlFor="start_date">Data de Início</Label>
                <Input id="start_date" name="start_date" type="date" />
              </div>
              <div className="space-y-2">
                <Label htmlFor="end_date">Data de Término</Label>
                <Input id="end_date" name="end_date" type="date" />
              </div>
            </div>

            <div className="flex gap-3 pt-2">
              <Button type="submit" disabled={saving}>
                {saving ? "Salvando…" : "Salvar"}
              </Button>
              <Button variant="outline" asChild>
                <Link href="/dashboard/liabilities">Cancelar</Link>
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
