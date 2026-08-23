"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
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
import { createCard, updateCard, type CardInput } from "@/app/actions/cards";
import type { Card as CardRow, FinancialProduct } from "@sbf/shared-types";

interface Props {
  produtos: FinancialProduct[];
  cartao?: CardRow;
}

/** Campo vazio vira `null`, não `0` nem `""` — "não informado" é um estado. */
function numeroOuNulo(valor: FormDataEntryValue | null): number | null {
  const texto = String(valor ?? "").trim();
  if (texto === "") return null;
  const n = Number(texto.replace(",", "."));
  return Number.isFinite(n) ? n : null;
}

function textoOuNulo(valor: FormDataEntryValue | null): string | null {
  const texto = String(valor ?? "").trim();
  return texto === "" ? null : texto;
}

export function CardForm({ produtos, cartao }: Props) {
  const router = useRouter();
  const [pendente, startTransition] = useTransition();
  const [produtoId, setProdutoId] = useState(cartao?.financial_product_id ?? "");

  const editando = Boolean(cartao);

  function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);

    if (!produtoId) {
      toast.error("Escolha o produto financeiro do cartão");
      return;
    }

    const input: CardInput = {
      financial_product_id: produtoId,
      last_four_digits: textoOuNulo(form.get("last_four_digits")),
      card_brand: textoOuNulo(form.get("card_brand")),
      holder_name: textoOuNulo(form.get("holder_name")),
      credit_limit: numeroOuNulo(form.get("credit_limit")),
      closing_day: numeroOuNulo(form.get("closing_day")),
      due_day: numeroOuNulo(form.get("due_day")),
      is_primary: form.get("is_primary") === "on",
      is_active: form.get("is_active") === "on",
    };

    startTransition(async () => {
      try {
        if (cartao) await updateCard(cartao.id, input);
        else await createCard(input);

        toast.success(editando ? "Cartão atualizado" : "Cartão cadastrado");
        router.push("/dashboard/cards");
        router.refresh();
      } catch (err) {
        toast.error(editando ? "Erro ao salvar" : "Erro ao cadastrar", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });
  }

  if (produtos.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Nenhum produto financeiro cadastrado</CardTitle>
          <CardDescription>
            Um cartão pertence a um produto financeiro, que por sua vez pertence a uma instituição.
            Cadastre primeiro a instituição e o produto.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild>
            <a href="/dashboard/products/new">Cadastrar produto financeiro</a>
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <form onSubmit={onSubmit}>
      <Card>
        <CardHeader>
          <CardTitle>{editando ? "Editar cartão" : "Novo cartão"}</CardTitle>
          <CardDescription>
            Fechamento e vencimento são o que permite saber em qual fatura uma compra cai.
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="financial_product_id">Produto financeiro *</Label>
            <Select value={produtoId} onValueChange={setProdutoId}>
              <SelectTrigger id="financial_product_id">
                <SelectValue placeholder="Selecione o produto" />
              </SelectTrigger>
              <SelectContent>
                {produtos.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="last_four_digits">Últimos 4 dígitos</Label>
              <Input
                id="last_four_digits"
                name="last_four_digits"
                inputMode="numeric"
                maxLength={4}
                placeholder="1234"
                defaultValue={cartao?.last_four_digits ?? ""}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="card_brand">Bandeira</Label>
              <Input
                id="card_brand"
                name="card_brand"
                placeholder="Visa, Mastercard…"
                defaultValue={cartao?.card_brand ?? ""}
              />
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="holder_name">Nome impresso no cartão</Label>
            <Input id="holder_name" name="holder_name" defaultValue={cartao?.holder_name ?? ""} />
          </div>

          <div className="grid gap-4 sm:grid-cols-3">
            <div className="space-y-2">
              <Label htmlFor="credit_limit">Limite (R$)</Label>
              <Input
                id="credit_limit"
                name="credit_limit"
                inputMode="decimal"
                placeholder="5000"
                defaultValue={cartao?.credit_limit ?? ""}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="closing_day">Dia do fechamento</Label>
              <Input
                id="closing_day"
                name="closing_day"
                inputMode="numeric"
                min={1}
                max={31}
                type="number"
                placeholder="20"
                defaultValue={cartao?.closing_day ?? ""}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="due_day">Dia do vencimento</Label>
              <Input
                id="due_day"
                name="due_day"
                inputMode="numeric"
                min={1}
                max={31}
                type="number"
                placeholder="28"
                defaultValue={cartao?.due_day ?? ""}
              />
            </div>
          </div>

          <div className="flex flex-wrap gap-6 pt-2">
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                name="is_primary"
                defaultChecked={cartao?.is_primary ?? true}
                className="size-4"
              />
              Cartão principal
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                name="is_active"
                defaultChecked={cartao?.is_active ?? true}
                className="size-4"
              />
              Ativo
            </label>
          </div>

          <div className="flex gap-2 pt-2">
            <Button type="submit" disabled={pendente}>
              {pendente ? "Salvando…" : editando ? "Salvar" : "Cadastrar"}
            </Button>
            <Button type="button" variant="outline" onClick={() => router.back()}>
              Cancelar
            </Button>
          </div>
        </CardContent>
      </Card>
    </form>
  );
}
