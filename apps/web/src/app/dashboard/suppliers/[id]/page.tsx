"use client";

import { useEffect, useState } from "react";
import { useRouter, useParams } from "next/navigation";
import { createClient } from "@/lib/supabase/client";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { Plus, Trash2, FileText } from "lucide-react";
import { formatDate, formatCurrency } from "@/lib/format";

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

const aliasTypeOptions = [
  { value: "former_name", label: "Nome anterior" },
  { value: "abbreviation", label: "Abreviação" },
  { value: "trade_name", label: "Nome fantasia" },
  { value: "billing_name", label: "Nome na fatura" },
  { value: "other", label: "Outro" },
];

const contractTypeOptions = [
  { value: "subscription", label: "Assinatura" },
  { value: "installment", label: "Parcelamento" },
  { value: "on_demand", label: "Sob demanda" },
  { value: "prepaid", label: "Pré-pago" },
  { value: "other", label: "Outro" },
];

interface Alias {
  id: string;
  alias_name: string;
  alias_type: string;
  valid_from: string | null;
  valid_until: string | null;
}

interface Contract {
  id: string;
  identifier: string | null;
  contract_type: string;
  label: string | null;
  start_date: string | null;
  end_date: string | null;
  is_active: boolean;
}

interface SourceDocRef {
  id: string;
  filename: string;
  origin_type: string;
  status: string;
  created_at: string | null;
  metadata: Record<string, unknown> | null;
}

export default function EditSupplierPage() {
  const router = useRouter();
  const params = useParams();
  const id = params.id as string;

  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [fetching, setFetching] = useState(true);

  // Supplier fields
  const [name, setName] = useState("");
  const [type, setType] = useState("company");
  const [tradeName, setTradeName] = useState("");
  const [documentNumber, setDocumentNumber] = useState("");
  const [notes, setNotes] = useState("");

  // Aliases
  const [aliases, setAliases] = useState<Alias[]>([]);
  const [newAliasName, setNewAliasName] = useState("");
  const [newAliasType, setNewAliasType] = useState("billing_name");
  const [addingAlias, setAddingAlias] = useState(false);

  // Contracts
  const [contracts, setContracts] = useState<Contract[]>([]);
  const [newContractId, setNewContractId] = useState("");
  const [newContractType, setNewContractType] = useState("subscription");
  const [newContractLabel, setNewContractLabel] = useState("");
  const [addingContract, setAddingContract] = useState(false);

  // Documentos vinculados
  const [documents, setDocuments] = useState<SourceDocRef[]>([]);

  useEffect(() => {
    async function load() {
      const supabase = createClient();

      const [supplierRes, aliasesRes, contractsRes, docsRes] = await Promise.all([
        supabase.from("suppliers").select("*").eq("id", id).single(),
        supabase
          .from("supplier_aliases")
          .select("*")
          .eq("supplier_id", id)
          .order("created_at", { ascending: false }),
        supabase
          .from("supplier_contracts")
          .select("*")
          .eq("supplier_id", id)
          .order("start_date", { ascending: false }),
        supabase
          .from("source_documents")
          .select("id, filename, origin_type, status, created_at, metadata")
          .eq("supplier_id", id)
          .order("created_at", { ascending: false })
          .limit(50),
      ]);

      if (supplierRes.data) {
        setName(supplierRes.data.name);
        setType(supplierRes.data.type);
        setTradeName(supplierRes.data.trade_name ?? "");
        setDocumentNumber(supplierRes.data.document_number ?? "");
        setNotes(supplierRes.data.notes ?? "");
      }
      if (aliasesRes.data) setAliases(aliasesRes.data);
      if (contractsRes.data) setContracts(contractsRes.data);
      if (docsRes.data) setDocuments(docsRes.data as SourceDocRef[]);

      setFetching(false);
    }
    load();
  }, [id]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    const supabase = createClient();
    const { error } = await supabase
      .from("suppliers")
      .update({
        name,
        type,
        trade_name: tradeName || null,
        document_number: documentNumber || null,
        notes: notes || null,
      })
      .eq("id", id);

    if (error) {
      toast.error("Erro ao atualizar", { description: error.message });
      setLoading(false);
      return;
    }
    toast.success("Fornecedor atualizado");
    router.push("/dashboard/suppliers");
    router.refresh();
  }

  async function handleDelete() {
    setDeleting(true);
    const supabase = createClient();
    const { error } = await supabase.from("suppliers").update({ is_active: false }).eq("id", id);

    if (error) {
      toast.error("Erro ao desativar", { description: error.message });
      setDeleting(false);
      return;
    }
    toast.success("Fornecedor desativado");
    router.push("/dashboard/suppliers");
    router.refresh();
  }

  async function handleAddAlias() {
    if (!newAliasName.trim()) return;
    setAddingAlias(true);
    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setAddingAlias(false);
      return;
    }

    const { data, error } = await supabase
      .from("supplier_aliases")
      .insert({
        supplier_id: id,
        alias_name: newAliasName.trim(),
        alias_type: newAliasType,
        user_id: user.id,
      })
      .select()
      .single();

    if (error) {
      toast.error("Erro ao adicionar alias", { description: error.message });
    } else if (data) {
      setAliases((prev) => [data, ...prev]);
      setNewAliasName("");
      toast.success("Alias adicionado");
    }
    setAddingAlias(false);
  }

  async function handleDeleteAlias(aliasId: string) {
    const supabase = createClient();
    const { error } = await supabase.from("supplier_aliases").delete().eq("id", aliasId);
    if (error) {
      toast.error("Erro ao remover alias", { description: error.message });
      return;
    }
    setAliases((prev) => prev.filter((a) => a.id !== aliasId));
    toast.success("Alias removido");
  }

  async function handleAddContract() {
    if (!newContractId.trim()) return;
    setAddingContract(true);
    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) {
      setAddingContract(false);
      return;
    }

    const { data, error } = await supabase
      .from("supplier_contracts")
      .insert({
        supplier_id: id,
        identifier: newContractId.trim(),
        contract_type: newContractType,
        label: newContractLabel || null,
        user_id: user.id,
      })
      .select()
      .single();

    if (error) {
      toast.error("Erro ao adicionar contrato", { description: error.message });
    } else if (data) {
      setContracts((prev) => [data, ...prev]);
      setNewContractId("");
      setNewContractLabel("");
      toast.success("Contrato adicionado");
    }
    setAddingContract(false);
  }

  async function handleDeleteContract(contractId: string) {
    const supabase = createClient();
    const { error } = await supabase.from("supplier_contracts").delete().eq("id", contractId);
    if (error) {
      toast.error("Erro ao remover contrato", { description: error.message });
      return;
    }
    setContracts((prev) => prev.filter((c) => c.id !== contractId));
    toast.success("Contrato removido");
  }

  if (fetching) {
    return (
      <div className="mx-auto max-w-2xl space-y-6">
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
    <div className="mx-auto max-w-2xl space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Editar Fornecedor</h1>
          <p className="text-muted-foreground">Atualize dados, aliases e contratos</p>
        </div>
        <Dialog>
          <DialogTrigger asChild>
            <Button variant="destructive" size="sm">
              <Trash2 className="mr-2 size-4" />
              Desativar
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Desativar fornecedor?</DialogTitle>
              <DialogDescription>
                O fornecedor será marcado como inativo. O histórico de transações será preservado.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="destructive" onClick={handleDelete} disabled={deleting}>
                {deleting ? "Desativando..." : "Confirmar Desativação"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      {/* Dados do Fornecedor */}
      <Card>
        <CardHeader>
          <CardTitle>Dados do Fornecedor</CardTitle>
          <CardDescription>Atualize nome, tipo e informações gerais</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
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
                value={tradeName}
                onChange={(e) => setTradeName(e.target.value)}
                placeholder="Ex: Neoenergia Pernambuco"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="document_number">CNPJ / CPF</Label>
              <Input
                id="document_number"
                value={documentNumber}
                onChange={(e) => setDocumentNumber(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea
                id="notes"
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                rows={3}
              />
            </div>
            <div className="flex gap-3 pt-4">
              <Button type="button" variant="outline" onClick={() => router.back()}>
                Cancelar
              </Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Salvando..." : "Salvar Alterações"}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      {/* Aliases */}
      <Card>
        <CardHeader>
          <CardTitle>Aliases / Nomes Alternativos</CardTitle>
          <CardDescription>
            Nomes anteriores, abreviações ou nomes como aparecem em faturas
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-end gap-2">
            <div className="flex-1 space-y-1">
              <Label htmlFor="alias-name">Nome</Label>
              <Input
                id="alias-name"
                value={newAliasName}
                onChange={(e) => setNewAliasName(e.target.value)}
                placeholder="Ex: Celpe, ENEL SP"
              />
            </div>
            <div className="w-40 space-y-1">
              <Label htmlFor="alias-type">Tipo</Label>
              <Select value={newAliasType} onValueChange={setNewAliasType}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {aliasTypeOptions.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button onClick={handleAddAlias} disabled={addingAlias || !newAliasName.trim()}>
              <Plus className="mr-1 size-4" />
              Adicionar
            </Button>
          </div>

          {aliases.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead>Desde</TableHead>
                  <TableHead className="text-right">Ação</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {aliases.map((alias) => (
                  <TableRow key={alias.id}>
                    <TableCell className="font-medium">{alias.alias_name}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {aliasTypeOptions.find((o) => o.value === alias.alias_type)?.label ??
                          alias.alias_type}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {alias.valid_from ? formatDate(alias.valid_from) : "—"}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" onClick={() => handleDeleteAlias(alias.id)}>
                        <Trash2 className="size-4 text-destructive" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Contratos */}
      <Card>
        <CardHeader>
          <CardTitle>Contratos / Identificadores</CardTitle>
          <CardDescription>
            Números de contrato, assinaturas ou identificadores de serviço
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-end gap-2">
            <div className="flex-1 space-y-1">
              <Label htmlFor="contract-id">Identificador</Label>
              <Input
                id="contract-id"
                value={newContractId}
                onChange={(e) => setNewContractId(e.target.value)}
                placeholder="Ex: CTR-2024-001, UC 123456"
              />
            </div>
            <div className="w-40 space-y-1">
              <Label htmlFor="contract-type">Tipo</Label>
              <Select value={newContractType} onValueChange={setNewContractType}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {contractTypeOptions.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button onClick={handleAddContract} disabled={addingContract || !newContractId.trim()}>
              <Plus className="mr-1 size-4" />
              Adicionar
            </Button>
          </div>

          <div className="space-y-1">
            <Label htmlFor="contract-label">Descrição (opcional)</Label>
            <Input
              id="contract-label"
              value={newContractLabel}
              onChange={(e) => setNewContractLabel(e.target.value)}
              placeholder="Ex: Conta de luz residencial"
            />
          </div>

          {contracts.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Identificador</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead>Descrição</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Ação</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {contracts.map((contract) => (
                  <TableRow key={contract.id}>
                    <TableCell className="font-medium">{contract.identifier ?? "—"}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {contractTypeOptions.find((o) => o.value === contract.contract_type)
                          ?.label ?? contract.contract_type}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{contract.label ?? "—"}</TableCell>
                    <TableCell>
                      <Badge variant={contract.is_active ? "default" : "secondary"}>
                        {contract.is_active ? "Ativo" : "Encerrado"}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleDeleteContract(contract.id)}
                      >
                        <Trash2 className="size-4 text-destructive" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Documentos vinculados — S3-012 */}
      <Card>
        <CardHeader>
          <CardTitle>Documentos vinculados</CardTitle>
          <CardDescription>
            Notas fiscais, faturas e comprovantes associados a este fornecedor
          </CardDescription>
        </CardHeader>
        <CardContent>
          {documents.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Nenhum documento associado a este fornecedor.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Arquivo</TableHead>
                  <TableHead>Origem</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Valor</TableHead>
                  <TableHead>Data</TableHead>
                  <TableHead className="text-right">Ação</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {documents.map((doc) => {
                  const meta = (doc.metadata ?? {}) as Record<string, unknown>;
                  return (
                    <TableRow key={doc.id}>
                      <TableCell className="max-w-[200px] truncate font-medium">
                        <span className="flex items-center gap-1.5">
                          <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                          {doc.filename}
                        </span>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {doc.origin_type === "gmail"
                          ? "Gmail"
                          : doc.origin_type === "local_file"
                            ? "Local"
                            : "Upload"}
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={
                            doc.status === "approved"
                              ? "default"
                              : doc.status === "failed"
                                ? "destructive"
                                : "secondary"
                          }
                          className="text-xs"
                        >
                          {doc.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="tabular-nums">
                        {meta["amount"] != null ? formatCurrency(Number(meta["amount"])) : "—"}
                      </TableCell>
                      <TableCell className="text-sm">
                        {doc.created_at ? formatDate(doc.created_at) : "—"}
                      </TableCell>
                      <TableCell className="text-right">
                        <Button variant="ghost" size="sm" asChild>
                          <Link href={`/dashboard/documents/${doc.id}`}>Ver</Link>
                        </Button>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
