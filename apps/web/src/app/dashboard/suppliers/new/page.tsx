"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { createClient } from "@/lib/supabase/client";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const typeOptions = [
  { value: "company", label: "Empresa" },
  { value: "individual", label: "Pessoa Física" },
  { value: "government", label: "Governo" },
  { value: "utility", label: "Utilidade (Água, Luz, Gás)" },
  { value: "telecom", label: "Telecom" },
  { value: "saas", label: "SaaS / Software" },
  { value: "platform", label: "Plataforma Digital" },
  { value: "other", label: "Outro" },
];

export default function NewSupplierPage() {
  const router = useRouter();
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setLoading(true);

    const formData = new FormData(e.currentTarget);
    const name = formData.get("name") as string;
    const type = formData.get("type") as string;
    const document_number = (formData.get("document_number") as string) || null;
    const trade_name = (formData.get("trade_name") as string) || null;
    const notes = (formData.get("notes") as string) || null;

    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setLoading(false);
      return;
    }

    const { error } = await supabase
      .from("suppliers")
      .insert({ name, type, document_number, trade_name, notes, user_id: user.id });

    if (error) {
      toast.error("Erro ao criar fornecedor", { description: error.message });
      setLoading(false);
      return;
    }

    toast.success("Fornecedor criado com sucesso");
    router.push("/dashboard/suppliers");
    router.refresh();
  }

  return (
    <div className="mx-auto max-w-lg space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Novo Fornecedor</h1>
        <p className="text-muted-foreground">Cadastre uma empresa, serviço ou prestador</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados do Fornecedor</CardTitle>
          <CardDescription>Preencha os dados básicos</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Nome</Label>
              <Input id="name" name="name" placeholder="Ex: Neoenergia, GitHub, Vivo" required />
            </div>
            <div className="space-y-2">
              <Label htmlFor="type">Tipo</Label>
              <Select name="type" defaultValue="company">
                <SelectTrigger>
                  <SelectValue placeholder="Selecione o tipo" />
                </SelectTrigger>
                <SelectContent>
                  {typeOptions.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="trade_name">Nome Fantasia</Label>
              <Input id="trade_name" name="trade_name" placeholder="Ex: Neoenergia Pernambuco" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="document_number">CNPJ / CPF</Label>
              <Input
                id="document_number"
                name="document_number"
                placeholder="Ex: 12.345.678/0001-90"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea
                id="notes"
                name="notes"
                placeholder="Informações adicionais sobre o fornecedor"
                rows={3}
              />
            </div>
            <div className="flex gap-3 pt-4">
              <Button type="button" variant="outline" onClick={() => router.back()}>
                Cancelar
              </Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Salvando..." : "Criar Fornecedor"}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
