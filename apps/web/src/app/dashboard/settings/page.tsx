"use client";

import { useState, useEffect } from "react";
import { createClient } from "@/lib/supabase/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { toast } from "sonner";
import {
  Plus,
  Trash2,
  Settings2,
  ScanLine,
  Files,
  ClipboardCheck,
  BookOpen,
  Activity,
} from "lucide-react";
import Link from "next/link";

interface Category {
  id: string;
  name: string;
  parent_id: string | null;
  icon: string | null;
  color: string | null;
  is_active: boolean;
}

interface Tag {
  id: string;
  name: string;
  color: string | null;
  influences_priority: boolean;
  suggested_priority: string | null;
}

export default function SettingsPage() {
  const supabase = createClient();
  const [prefs, setPrefs] = useState({
    financial_cycle_start_day: "",
    financial_cycle_anchor_date: "",
    default_currency: "BRL",
  });
  const [prefsExist, setPrefsExist] = useState(false);
  const [categories, setCategories] = useState<Category[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [newCat, setNewCat] = useState("");
  const [newTag, setNewTag] = useState("");
  const [newTagPriority, setNewTagPriority] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    Promise.all([
      supabase.from("user_financial_preferences").select("*").maybeSingle(),
      supabase.from("categories").select("*").order("name"),
      supabase.from("tags").select("*").order("name"),
    ]).then(([{ data: p }, { data: cats }, { data: ts }]) => {
      if (p) {
        setPrefsExist(true);
        setPrefs({
          financial_cycle_start_day: p.financial_cycle_start_day
            ? String(p.financial_cycle_start_day)
            : "",
          financial_cycle_anchor_date: p.financial_cycle_anchor_date ?? "",
          default_currency: p.default_currency ?? "BRL",
        });
      }
      setCategories(cats ?? []);
      setTags(ts ?? []);
    });
  }, [supabase]);

  async function savePrefs() {
    setSaving(true);
    const payload = {
      financial_cycle_start_day: prefs.financial_cycle_start_day
        ? Number(prefs.financial_cycle_start_day)
        : null,
      financial_cycle_anchor_date: prefs.financial_cycle_anchor_date || null,
      default_currency: prefs.default_currency,
    };
    if (prefsExist) {
      const { error } = await supabase
        .from("user_financial_preferences")
        .update(payload)
        .not("id", "is", null);
      if (error) toast.error("Erro", { description: error.message });
      else toast.success("Preferências salvas");
    } else {
      const {
        data: { user },
      } = await supabase.auth.getUser();
      if (!user) {
        setSaving(false);
        return;
      }
      const { error } = await supabase
        .from("user_financial_preferences")
        .insert({ ...payload, user_id: user.id });
      if (error) toast.error("Erro", { description: error.message });
      else {
        toast.success("Preferências salvas");
        setPrefsExist(true);
      }
    }
    setSaving(false);
  }

  async function addCategory() {
    if (!newCat.trim()) return;
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return;
    const { data, error } = await supabase
      .from("categories")
      .insert({ name: newCat.trim(), user_id: user.id })
      .select()
      .single();
    if (error) toast.error("Erro", { description: error.message });
    else {
      setCategories([...categories, data]);
      setNewCat("");
      toast.success("Categoria criada");
    }
  }

  async function deleteCategory(id: string) {
    const { error } = await supabase.from("categories").delete().eq("id", id);
    if (error) toast.error("Erro", { description: error.message });
    else setCategories(categories.filter((c) => c.id !== id));
  }

  async function addTag() {
    if (!newTag.trim()) return;
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return;
    const { data, error } = await supabase
      .from("tags")
      .insert({
        name: newTag.trim(),
        user_id: user.id,
        influences_priority: !!newTagPriority,
        suggested_priority: newTagPriority || null,
      })
      .select()
      .single();
    if (error) toast.error("Erro", { description: error.message });
    else {
      setTags([...tags, data]);
      setNewTag("");
      setNewTagPriority("");
      toast.success("Tag criada");
    }
  }

  async function deleteTag(id: string) {
    const { error } = await supabase.from("tags").delete().eq("id", id);
    if (error) toast.error("Erro", { description: error.message });
    else setTags(tags.filter((t) => t.id !== id));
  }

  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Configurações</h1>
        <p className="text-muted-foreground">Ciclo financeiro, categorias e tags</p>
      </div>

      {/* Financial Cycle Preferences */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Settings2 className="size-5" />
            Ciclo Financeiro
          </CardTitle>
          <CardDescription>
            Configure o dia de início do seu ciclo financeiro personalizado. Exemplo: dia 20 → seu
            mês financeiro vai de 20/03 a 19/04.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-3 gap-4">
            <div className="space-y-2">
              <Label>Dia de início do ciclo</Label>
              <Input
                type="number"
                min={1}
                max={31}
                placeholder="Ex: 20"
                value={prefs.financial_cycle_start_day}
                onChange={(e) => setPrefs({ ...prefs, financial_cycle_start_day: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label>Data âncora</Label>
              <Input
                type="date"
                value={prefs.financial_cycle_anchor_date}
                onChange={(e) =>
                  setPrefs({ ...prefs, financial_cycle_anchor_date: e.target.value })
                }
              />
            </div>
            <div className="space-y-2">
              <Label>Moeda padrão</Label>
              <Input
                value={prefs.default_currency}
                onChange={(e) => setPrefs({ ...prefs, default_currency: e.target.value })}
              />
            </div>
          </div>
          <Button onClick={savePrefs} disabled={saving}>
            {saving ? "Salvando…" : "Salvar Preferências"}
          </Button>
        </CardContent>
      </Card>

      <Separator />

      {/* Categories */}
      <Card>
        <CardHeader>
          <CardTitle>Categorias</CardTitle>
          <CardDescription>Categorias para classificação principal de transações</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-2">
            <Input
              placeholder="Nome da categoria"
              value={newCat}
              onChange={(e) => setNewCat(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addCategory()}
            />
            <Button onClick={addCategory} size="sm">
              <Plus className="mr-1 size-3" />
              Adicionar
            </Button>
          </div>

          {categories.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nenhuma categoria cadastrada.</p>
          ) : (
            <div className="flex flex-wrap gap-2">
              {categories.map((cat) => (
                <Badge key={cat.id} variant="secondary" className="gap-1 py-1 pl-3 pr-1">
                  {cat.name}
                  <button
                    onClick={() => deleteCategory(cat.id)}
                    className="ml-1 rounded-full p-0.5 hover:bg-destructive/20"
                  >
                    <Trash2 className="size-3" />
                  </button>
                </Badge>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Separator />

      {/* Tags */}
      <Card>
        <CardHeader>
          <CardTitle>Tags</CardTitle>
          <CardDescription>
            Tags para classificação complementar. Tags podem influenciar prioridade de pagamento.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-2">
            <Input
              placeholder="Nome da tag"
              value={newTag}
              onChange={(e) => setNewTag(e.target.value)}
              className="flex-1"
            />
            <Select value={newTagPriority} onValueChange={setNewTagPriority}>
              <SelectTrigger className="w-[160px]">
                <SelectValue placeholder="Prioridade?" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="essential">Essencial</SelectItem>
                <SelectItem value="high">Alta</SelectItem>
                <SelectItem value="medium">Média</SelectItem>
                <SelectItem value="low">Baixa</SelectItem>
                <SelectItem value="optional">Opcional</SelectItem>
              </SelectContent>
            </Select>
            <Button onClick={addTag} size="sm">
              <Plus className="mr-1 size-3" />
              Adicionar
            </Button>
          </div>

          {tags.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nenhuma tag cadastrada.</p>
          ) : (
            <div className="flex flex-wrap gap-2">
              {tags.map((tag) => (
                <Badge key={tag.id} variant="outline" className="gap-1 py-1 pl-3 pr-1">
                  {tag.name}
                  {tag.suggested_priority && (
                    <span className="text-xs text-muted-foreground">
                      ({tag.suggested_priority})
                    </span>
                  )}
                  <button
                    onClick={() => deleteTag(tag.id)}
                    className="ml-1 rounded-full p-0.5 hover:bg-destructive/20"
                  >
                    <Trash2 className="size-3" />
                  </button>
                </Badge>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Avançado / Pipeline */}
      <Card id="avancado">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Settings2 className="size-5" />
            Avançado / Pipeline
          </CardTitle>
          <CardDescription>
            Ferramentas técnicas de ingestão, revisão e automação de documentos.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-2 sm:grid-cols-2">
            <Link
              href="/dashboard/documents"
              className="flex items-center gap-3 rounded-lg border p-3 text-sm hover:bg-muted/50 transition-colors"
            >
              <ScanLine className="size-4 shrink-0 text-muted-foreground" />
              <div>
                <p className="font-medium">Documentos</p>
                <p className="text-xs text-muted-foreground">Visão geral do pipeline</p>
              </div>
            </Link>
            <Link
              href="/dashboard/ingestion/documents"
              className="flex items-center gap-3 rounded-lg border p-3 text-sm hover:bg-muted/50 transition-colors"
            >
              <Files className="size-4 shrink-0 text-muted-foreground" />
              <div>
                <p className="font-medium">Documentos Ingeridos</p>
                <p className="text-xs text-muted-foreground">Listagem técnica por job</p>
              </div>
            </Link>
            <Link
              href="/dashboard/ingestion/review"
              className="flex items-center gap-3 rounded-lg border p-3 text-sm hover:bg-muted/50 transition-colors"
            >
              <ClipboardCheck className="size-4 shrink-0 text-muted-foreground" />
              <div>
                <p className="font-medium">Revisão</p>
                <p className="text-xs text-muted-foreground">Aprovação e rejeição de drafts</p>
              </div>
            </Link>
            <Link
              href="/dashboard/ingestion/patterns"
              className="flex items-center gap-3 rounded-lg border p-3 text-sm hover:bg-muted/50 transition-colors"
            >
              <BookOpen className="size-4 shrink-0 text-muted-foreground" />
              <div>
                <p className="font-medium">Padrões</p>
                <p className="text-xs text-muted-foreground">Templates de extração por tipo</p>
              </div>
            </Link>
            <Link
              href="/dashboard/ingestion/logs"
              className="flex items-center gap-3 rounded-lg border p-3 text-sm hover:bg-muted/50 transition-colors"
            >
              <Activity className="size-4 shrink-0 text-muted-foreground" />
              <div>
                <p className="font-medium">Logs de Ingestão</p>
                <p className="text-xs text-muted-foreground">Histórico e diagnóstico</p>
              </div>
            </Link>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
