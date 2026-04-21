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
import { Sparkles, Loader2 } from "lucide-react";
import { useAISuggest } from "@/hooks/use-ai-suggest";

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

  // S4-008: campos controlados para pré-preenchimento via IA
  const [name, setName] = useState("");
  const [tradeName, setTradeName] = useState("");
  const [documentNumber, setDocumentNumber] = useState("");

  const { suggest: suggestName, loading: loadingAI } = useAISuggest<{
    result: {
      name?: string;
      trade_name?: string;
      aliases?: string[];
      found?: boolean;
      message?: string;
    };
  }>();

  async function handleCNPJBlur() {
    const raw = documentNumber.trim();
    if (raw.replace(/\D/g, "").length < 11) return; // mínimo CPF
    if (name) return; // já tem nome — não sobrescrever

    const res = await suggestName("suggest_supplier_name", { document_number: raw });
    const data = res?.result;
    if (!data) return;

    if (data.name && !name) setName(data.name);
    if (data.trade_name && !tradeName) setTradeName(data.trade_name);

    if (data.found) {
      toast.info("Fornecedor já cadastrado", { description: data.message ?? "" });
    } else if (data.suggested_names && data.suggested_names.length > 0) {
      toast.success("Nome sugerido pela IA", { description: data.message ?? "" });
    }
  }

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setLoading(true);

    const formData = new FormData(e.currentTarget);
    const type = formData.get("type") as string;
    const notes = (formData.get("notes") as string) || null;

    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setLoading(false);
      return;
    }

    const { error } = await supabase.from("suppliers").insert({
      name,
      type,
      document_number: documentNumber || null,
      trade_name: tradeName || null,
      notes,
      user_id: user.id,
    });

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
              <Input
                id="name"
                placeholder="Ex: Neoenergia, GitHub, Vivo"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
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
              <Input
                id="trade_name"
                placeholder="Ex: Neoenergia Pernambuco"
                value={tradeName}
                onChange={(e) => setTradeName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="document_number" className="flex items-center gap-1.5">
                CNPJ / CPF
                {loadingAI && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />}
                {!loadingAI && documentNumber.replace(/\D/g, "").length >= 11 && !name && (
                  <Sparkles
                    className="h-3 w-3 text-primary"
                    title="Saia do campo para buscar via IA"
                  />
                )}
              </Label>
              <Input
                id="document_number"
                placeholder="Ex: 12.345.678/0001-90"
                value={documentNumber}
                onChange={(e) => setDocumentNumber(e.target.value)}
                onBlur={handleCNPJBlur}
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
