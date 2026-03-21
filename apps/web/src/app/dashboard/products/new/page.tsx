"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { createClient } from "@/lib/supabase/client";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const productTypeOptions = [
  { value: "checking_account", label: "Conta Corrente" },
  { value: "savings_account", label: "Poupança" },
  { value: "credit_card", label: "Cartão de Crédito" },
  { value: "debit_card", label: "Cartão de Débito" },
  { value: "investment", label: "Investimento" },
  { value: "loan", label: "Empréstimo" },
  { value: "mortgage", label: "Financiamento" },
  { value: "insurance", label: "Seguro" },
  { value: "overdraft", label: "Cheque Especial" },
  { value: "other", label: "Outro" },
];

export default function NewProductPage() {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [institutions, setInstitutions] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    async function loadInstitutions() {
      const supabase = createClient();
      const { data } = await supabase.from("institutions").select("id, name").order("name");
      setInstitutions(data ?? []);
    }
    loadInstitutions();
  }, []);

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setLoading(true);

    const formData = new FormData(e.currentTarget);
    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setLoading(false);
      return;
    }
    const { error } = await supabase.from("financial_products").insert({
      user_id: user.id,
      name: formData.get("name") as string,
      type: formData.get("type") as string,
      institution_id: formData.get("institution_id") as string,
      current_balance: formData.get("balance") ? Number(formData.get("balance")) : null,
    });

    if (error) {
      toast.error("Erro ao criar produto", { description: error.message });
      setLoading(false);
      return;
    }

    toast.success("Produto criado com sucesso");
    router.push("/dashboard/products");
    router.refresh();
  }

  return (
    <div className="mx-auto max-w-lg space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Novo Produto Financeiro</h1>
        <p className="text-muted-foreground">Conta, cartão, investimento ou empréstimo</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados do Produto</CardTitle>
          <CardDescription>Preencha as informações do produto</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="institution_id">Instituição</Label>
              <Select name="institution_id" required>
                <SelectTrigger>
                  <SelectValue placeholder="Selecione a instituição" />
                </SelectTrigger>
                <SelectContent>
                  {institutions.map((inst) => (
                    <SelectItem key={inst.id} value={inst.id}>
                      {inst.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="name">Nome</Label>
              <Input id="name" name="name" placeholder="Ex: Conta Corrente Principal" required />
            </div>
            <div className="space-y-2">
              <Label htmlFor="type">Tipo</Label>
              <Select name="type" defaultValue="checking_account">
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {productTypeOptions.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="balance">Saldo Atual (opcional)</Label>
              <Input id="balance" name="balance" type="number" step="0.01" placeholder="0,00" />
            </div>
            <div className="flex gap-3 pt-4">
              <Button type="button" variant="outline" onClick={() => router.back()}>
                Cancelar
              </Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Salvando..." : "Criar Produto"}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
