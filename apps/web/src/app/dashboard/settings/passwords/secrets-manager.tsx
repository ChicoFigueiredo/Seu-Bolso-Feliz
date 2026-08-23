"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { KeyRound, Trash2, Plus, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
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
import { deleteSecret, setSecret, type SecretSummary } from "@/app/actions/secrets";
import { formatDate } from "@/lib/format";

interface Props {
  secrets: SecretSummary[];
  fornecedores: { id: string; name: string }[];
}

const ESCOPOS = [
  { value: "global", label: "Qualquer documento" },
  { value: "supplier", label: "Um fornecedor específico" },
] as const;

/**
 * Gestão de senhas de PDF.
 *
 * A action `setSecret` já existia e ninguém a chamava: não havia tela. Quem
 * recebe fatura protegida por senha — a maioria dos bancos — via o documento
 * parar no pipeline com "PDF protegido" e não tinha onde informar a senha.
 *
 * O valor em claro só trafega no envio. A listagem nunca o recebe de volta: o
 * servidor devolve rótulo, escopo e uso, e a senha decriptada existe apenas
 * dentro de `fn_get_secrets`, concedida somente a `service_role`.
 */
export function SecretsManager({ secrets, fornecedores }: Props) {
  const router = useRouter();
  const [pendente, startTransition] = useTransition();
  const [aberto, setAberto] = useState(secrets.length === 0);
  const [escopo, setEscopo] = useState<string>("global");
  const [fornecedorId, setFornecedorId] = useState<string>("");

  function salvar(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    const plaintext = String(form.get("plaintext") ?? "");
    const label = String(form.get("label") ?? "").trim();

    if (!plaintext) {
      toast.error("Informe a senha");
      return;
    }
    if (escopo === "supplier" && !fornecedorId) {
      toast.error("Escolha o fornecedor");
      return;
    }

    const formEl = e.currentTarget;

    startTransition(async () => {
      try {
        await setSecret({
          plaintext,
          label: label || null,
          entityType: escopo === "supplier" ? "supplier" : null,
          entityId: escopo === "supplier" ? fornecedorId : null,
        });
        toast.success("Senha salva", {
          description: "O pipeline vai tentá-la no próximo documento protegido.",
        });
        formEl.reset();
        setFornecedorId("");
        setAberto(false);
        router.refresh();
      } catch (err) {
        toast.error("Não foi possível salvar", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });
  }

  function excluir(id: string) {
    if (!confirm("Excluir esta senha? Documentos protegidos por ela deixarão de abrir.")) return;

    startTransition(async () => {
      try {
        await deleteSecret(id);
        toast.success("Senha excluída");
        router.refresh();
      } catch (err) {
        toast.error("Não foi possível excluir", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });
  }

  const nomeDoFornecedor = new Map(fornecedores.map((f) => [f.id, f.name]));

  function descreverEscopo(s: SecretSummary): string {
    if (s.entityType === "supplier" && s.entityId) {
      return nomeDoFornecedor.get(s.entityId) ?? "Fornecedor removido";
    }
    return "Qualquer documento";
  }

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ShieldCheck className="size-5" />
            Como as senhas são guardadas
          </CardTitle>
          <CardDescription>
            A senha é criptografada dentro do banco e nunca volta para o navegador — nem para esta
            tela. Quando um PDF protegido chega, o worker tenta as senhas cadastradas em ordem de
            uso recente, e a que abrir sobe na fila. A senha nunca aparece em log algum.
          </CardDescription>
        </CardHeader>
      </Card>

      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Senhas cadastradas</h2>
          <p className="text-sm text-muted-foreground">
            {secrets.length === 0
              ? "Nenhuma senha cadastrada"
              : `${secrets.length} senha(s) — usadas automaticamente pela ingestão`}
          </p>
        </div>
        {!aberto && (
          <Button onClick={() => setAberto(true)}>
            <Plus className="mr-2 size-4" />
            Nova senha
          </Button>
        )}
      </div>

      {aberto && (
        <Card>
          <CardHeader>
            <CardTitle>Nova senha</CardTitle>
            <CardDescription>
              Cadastre no escopo mais amplo que fizer sentido: uma senha global é tentada em
              qualquer documento, e o pipeline aprende sozinho quais funcionam.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={salvar} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="plaintext">Senha *</Label>
                <Input
                  id="plaintext"
                  name="plaintext"
                  type="password"
                  autoComplete="off"
                  placeholder="A senha que abre o PDF"
                  required
                />
                <p className="text-xs text-muted-foreground">
                  Costuma ser o CPF, os primeiros dígitos do CPF ou a data de nascimento.
                </p>
              </div>

              <div className="space-y-2">
                <Label htmlFor="label">Rótulo</Label>
                <Input
                  id="label"
                  name="label"
                  placeholder="Ex.: CPF do titular"
                  autoComplete="off"
                />
                <p className="text-xs text-muted-foreground">
                  Como você reconhece esta senha. Não é a senha em si — este texto aparece na lista.
                </p>
              </div>

              <div className="space-y-2">
                <Label htmlFor="escopo">Aplica-se a</Label>
                <Select value={escopo} onValueChange={setEscopo}>
                  <SelectTrigger id="escopo">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {ESCOPOS.map((e) => (
                      <SelectItem key={e.value} value={e.value}>
                        {e.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {escopo === "supplier" && (
                <div className="space-y-2">
                  <Label htmlFor="fornecedor">Fornecedor *</Label>
                  <Select value={fornecedorId} onValueChange={setFornecedorId}>
                    <SelectTrigger id="fornecedor">
                      <SelectValue placeholder="Selecione o fornecedor" />
                    </SelectTrigger>
                    <SelectContent>
                      {fornecedores.map((f) => (
                        <SelectItem key={f.id} value={f.id}>
                          {f.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}

              <div className="flex gap-2">
                <Button type="submit" disabled={pendente}>
                  {pendente ? "Salvando…" : "Salvar senha"}
                </Button>
                <Button type="button" variant="outline" onClick={() => setAberto(false)}>
                  Cancelar
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      )}

      {secrets.length > 0 && (
        <div className="space-y-2">
          {secrets.map((s) => (
            <Card key={s.id}>
              <CardContent className="flex items-center justify-between gap-4 py-4">
                <div className="flex min-w-0 items-center gap-3">
                  <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0">
                    <p className="truncate font-medium">{s.label ?? "Sem rótulo"}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {descreverEscopo(s)}
                      {s.lastUsedAt
                        ? ` · último uso em ${formatDate(s.lastUsedAt)}`
                        : " · nunca usada"}
                    </p>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {s.successCount > 0 && (
                    <Badge variant="secondary">{s.successCount} acerto(s)</Badge>
                  )}
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => excluir(s.id)}
                    disabled={pendente}
                  >
                    <Trash2 className="size-3.5" />
                    <span className="sr-only">Excluir</span>
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
