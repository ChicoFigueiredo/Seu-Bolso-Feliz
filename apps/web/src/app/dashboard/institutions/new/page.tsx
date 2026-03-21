"use client";

import { useState } from "react";
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

const typeOptions = [
  { value: "bank", label: "Banco" },
  { value: "credit_union", label: "Cooperativa de Crédito" },
  { value: "brokerage", label: "Corretora" },
  { value: "fintech", label: "Fintech" },
  { value: "other", label: "Outro" },
];

export default function NewInstitutionPage() {
  const router = useRouter();
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setLoading(true);

    const formData = new FormData(e.currentTarget);
    const name = formData.get("name") as string;
    const type = formData.get("type") as string;

    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setLoading(false);
      return;
    }
    const { error } = await supabase.from("institutions").insert({ name, type, user_id: user.id });

    if (error) {
      toast.error("Erro ao criar instituição", { description: error.message });
      setLoading(false);
      return;
    }

    toast.success("Instituição criada com sucesso");
    router.push("/dashboard/institutions");
    router.refresh();
  }

  return (
    <div className="mx-auto max-w-lg space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Nova Instituição</h1>
        <p className="text-muted-foreground">Cadastre um banco, fintech ou cooperativa</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados da Instituição</CardTitle>
          <CardDescription>Preencha os dados básicos</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Nome</Label>
              <Input id="name" name="name" placeholder="Ex: Nubank" required />
            </div>
            <div className="space-y-2">
              <Label htmlFor="type">Tipo</Label>
              <Select name="type" defaultValue="bank">
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
            <div className="flex gap-3 pt-4">
              <Button type="button" variant="outline" onClick={() => router.back()}>
                Cancelar
              </Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Salvando..." : "Criar Instituição"}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
