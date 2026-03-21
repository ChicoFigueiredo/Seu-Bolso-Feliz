"use client";

import { useRouter, usePathname } from "next/navigation";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { X } from "lucide-react";

interface FiltersProps {
  current: {
    type?: string;
    confirmed?: string;
    from?: string;
    to?: string;
  };
}

export function TransactionFilters({ current }: FiltersProps) {
  const router = useRouter();
  const pathname = usePathname();

  function update(key: string, value: string | undefined) {
    const p = new URLSearchParams();
    const next = { ...current, [key]: value };
    for (const [k, v] of Object.entries(next)) {
      if (v && k !== "page") p.set(k, v);
    }
    router.push(`${pathname}?${p.toString()}`);
  }

  function clear() {
    router.push(pathname);
  }

  const hasFilters = current.type || current.confirmed || current.from || current.to;

  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="space-y-1">
        <label className="text-xs font-medium text-muted-foreground">Tipo</label>
        <Select value={current.type ?? ""} onValueChange={(v) => update("type", v || undefined)}>
          <SelectTrigger className="w-[160px]">
            <SelectValue placeholder="Todos" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="income">Receita</SelectItem>
            <SelectItem value="expense">Despesa</SelectItem>
            <SelectItem value="statement_payment">Pag. Fatura</SelectItem>
            <SelectItem value="liability_payment">Pag. Dívida</SelectItem>
            <SelectItem value="refund">Estorno</SelectItem>
            <SelectItem value="adjustment">Ajuste</SelectItem>
            <SelectItem value="fee">Taxa</SelectItem>
            <SelectItem value="interest_charge">Juros</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-1">
        <label className="text-xs font-medium text-muted-foreground">Confirmado</label>
        <Select
          value={current.confirmed ?? ""}
          onValueChange={(v) => update("confirmed", v || undefined)}
        >
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="Todos" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="true">Sim</SelectItem>
            <SelectItem value="false">Não</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-1">
        <label className="text-xs font-medium text-muted-foreground">De</label>
        <Input
          type="date"
          className="w-[150px]"
          value={current.from ?? ""}
          onChange={(e) => update("from", e.target.value || undefined)}
        />
      </div>

      <div className="space-y-1">
        <label className="text-xs font-medium text-muted-foreground">Até</label>
        <Input
          type="date"
          className="w-[150px]"
          value={current.to ?? ""}
          onChange={(e) => update("to", e.target.value || undefined)}
        />
      </div>

      {hasFilters && (
        <Button variant="ghost" size="sm" onClick={clear}>
          <X className="mr-1 size-3" />
          Limpar
        </Button>
      )}
    </div>
  );
}
