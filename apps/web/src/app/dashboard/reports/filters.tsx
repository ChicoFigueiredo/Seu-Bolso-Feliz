"use client";

import { useRouter, usePathname } from "next/navigation";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface FiltersProps {
  current: {
    mode?: string;
    from?: string;
    to?: string;
    supplier?: string;
  };
  suppliers: { id: string; name: string }[];
}

export function ReportFilters({ current, suppliers }: FiltersProps) {
  const router = useRouter();
  const pathname = usePathname();

  function update(key: string, value: string | undefined) {
    const p = new URLSearchParams();
    const next = { ...current, [key]: value };
    for (const [k, v] of Object.entries(next)) {
      if (v) p.set(k, v);
    }
    router.push(`${pathname}?${p.toString()}`);
  }

  function setMode(mode: string) {
    if (mode === "financial_period") {
      const p = new URLSearchParams({ mode });
      router.push(`${pathname}?${p.toString()}`);
    } else {
      update("mode", undefined);
    }
  }

  return (
    <div className="flex flex-wrap items-end gap-4">
      <Tabs value={current.mode ?? "civil"} onValueChange={setMode}>
        <TabsList>
          <TabsTrigger value="civil">Mês Civil</TabsTrigger>
          <TabsTrigger value="financial_period">Período Financeiro</TabsTrigger>
          <TabsTrigger value="custom">Intervalo Livre</TabsTrigger>
        </TabsList>
      </Tabs>

      {(current.mode === "custom" || current.mode === "civil" || !current.mode) && (
        <>
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
        </>
      )}

      <div className="space-y-1">
        <label className="text-xs font-medium text-muted-foreground">Fornecedor</label>
        <Select
          value={current.supplier ?? "all"}
          onValueChange={(v) => update("supplier", v === "all" ? undefined : v)}
        >
          <SelectTrigger className="w-[200px]">
            <SelectValue placeholder="Todos" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">Todos</SelectItem>
            {suppliers.map((s) => (
              <SelectItem key={s.id} value={s.id}>
                {s.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}
