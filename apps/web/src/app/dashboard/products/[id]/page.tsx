"use client";

import { useEffect, useState } from "react";
import { useRouter, useParams } from "next/navigation";
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Trash2 } from "lucide-react";

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

export default function EditProductPage() {
  const router = useRouter();
  const params = useParams();
  const id = params.id as string;
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [fetching, setFetching] = useState(true);
  const [name, setName] = useState("");
  const [type, setType] = useState("checking_account");
  const [institutionId, setInstitutionId] = useState("");
  const [balance, setBalance] = useState("");
  const [institutions, setInstitutions] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    async function load() {
      const supabase = createClient();
      const [productRes, instRes] = await Promise.all([
        supabase.from("financial_products").select("*").eq("id", id).single(),
        supabase.from("institutions").select("id, name").order("name"),
      ]);
      if (productRes.data) {
        setName(productRes.data.name);
        setType(productRes.data.type);
        setInstitutionId(productRes.data.institution_id);
        setBalance(productRes.data.current_balance?.toString() ?? "");
      }
      setInstitutions(instRes.data ?? []);
      setFetching(false);
    }
    load();
  }, [id]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    const supabase = createClient();
    const { error } = await supabase
      .from("financial_products")
      .update({
        name,
        type,
        institution_id: institutionId,
        current_balance: balance ? Number(balance) : null,
      })
      .eq("id", id);
    if (error) {
      toast.error("Erro ao atualizar", { description: error.message });
      setLoading(false);
      return;
    }
    toast.success("Produto atualizado");
    router.push("/dashboard/products");
    router.refresh();
  }

  async function handleDelete() {
    setDeleting(true);
    const supabase = createClient();
    const { error } = await supabase.from("financial_products").delete().eq("id", id);
    if (error) {
      toast.error("Erro ao excluir", { description: error.message });
      setDeleting(false);
      return;
    }
    toast.success("Produto excluído");
    router.push("/dashboard/products");
    router.refresh();
  }

  if (fetching) {
    return (
      <div className="mx-auto max-w-lg space-y-6">
        <Skeleton className="h-8 w-48" />
        <Card>
          <CardContent className="space-y-4 pt-6">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-lg space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Editar Produto</h1>
          <p className="text-muted-foreground">Atualize os dados do produto financeiro</p>
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
              <DialogTitle>Excluir produto?</DialogTitle>
              <DialogDescription>
                Esta ação não pode ser desfeita. Transações e dívidas vinculadas precisam ser
                removidas primeiro.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="destructive" onClick={handleDelete} disabled={deleting}>
                {deleting ? "Excluindo..." : "Confirmar Exclusão"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Dados do Produto</CardTitle>
          <CardDescription>Atualize as informações do produto</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="institution_id">Instituição</Label>
              <Select value={institutionId} onValueChange={setInstitutionId}>
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
              <Input id="name" value={name} onChange={(e) => setName(e.target.value)} required />
            </div>
            <div className="space-y-2">
              <Label htmlFor="type">Tipo</Label>
              <Select value={type} onValueChange={setType}>
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
              <Input
                id="balance"
                type="number"
                step="0.01"
                value={balance}
                onChange={(e) => setBalance(e.target.value)}
                placeholder="0,00"
              />
            </div>
            <div className="flex gap-3">
              <Button type="submit" disabled={loading}>
                {loading ? "Salvando..." : "Salvar Alterações"}
              </Button>
              <Button type="button" variant="outline" onClick={() => router.back()}>
                Cancelar
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
