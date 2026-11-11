"use client";

import { useState, type CSSProperties, type FormEvent, type ReactNode } from "react";
import { toast } from "sonner";
import { Button, ChipGroup, Sheet } from "@sbf/design-system";
import { createRecurringTemplate, updateRecurringTemplate } from "@/app/actions/recurring";
import type {
  FrequenciaRecorrencia,
  PrioridadeRecorrencia,
  RecorrenteResumo,
  TipoRecorrente,
} from "@/app/actions/painel-recorrentes";

export interface RecorrenteFormSheetProps {
  modo: "criar" | "editar";
  /** Só em modo "editar" — os valores atuais do recorrente. */
  template?: RecorrenteResumo;
  onFechar: () => void;
  /** Criou ou salvou com sucesso — quem chama fecha a folha e atualiza a tela. */
  onSalvo: () => void;
}

interface FormState {
  name: string;
  type: TipoRecorrente;
  frequency: FrequenciaRecorrencia;
  dayOfMonth: string;
  customIntervalDays: string;
  priority: "" | PrioridadeRecorrencia;
  isVariableAmount: boolean;
  amount: string;
  startsAt: string;
  endsAt: string;
  notes: string;
}

function estadoInicial(template?: RecorrenteResumo): FormState {
  if (!template) {
    return {
      name: "",
      type: "expense",
      frequency: "monthly",
      dayOfMonth: "",
      customIntervalDays: "",
      priority: "",
      isVariableAmount: false,
      amount: "",
      startsAt: "",
      endsAt: "",
      notes: "",
    };
  }
  return {
    name: template.nome,
    type: template.tipo,
    frequency: template.frequencia,
    dayOfMonth: template.diaDoMes ? String(template.diaDoMes) : "",
    customIntervalDays: template.intervaloDiasCustom ? String(template.intervaloDiasCustom) : "",
    priority: template.prioridade ?? "",
    isVariableAmount: template.variavel,
    amount: template.valor != null ? String(template.valor) : "",
    startsAt: template.iniciaEm ?? "",
    endsAt: template.terminaEm ?? "",
    notes: template.notas ?? "",
  };
}

const TIPO_OPCOES: { value: TipoRecorrente; label: string }[] = [
  { value: "expense", label: "Despesa" },
  { value: "income", label: "Receita" },
  { value: "liability_payment", label: "Pagamento de dívida" },
  { value: "statement_payment", label: "Pagamento de fatura" },
];

const FREQUENCIA_OPCOES: { value: FrequenciaRecorrencia; label: string }[] = [
  { value: "monthly", label: "Mensal" },
  { value: "weekly", label: "Semanal" },
  { value: "biweekly", label: "Quinzenal" },
  { value: "quarterly", label: "Trimestral" },
  { value: "annual", label: "Anual" },
  { value: "custom", label: "Personalizada" },
];

const PRIORIDADE_OPCOES: { value: "" | PrioridadeRecorrencia; label: string }[] = [
  { value: "", label: "Nenhuma" },
  { value: "essential", label: "Essencial" },
  { value: "high", label: "Alta" },
  { value: "medium", label: "Média" },
  { value: "low", label: "Baixa" },
  { value: "optional", label: "Opcional" },
];

const estiloRotulo: CSSProperties = {
  fontSize: 12.5,
  fontWeight: 600,
  color: "var(--text-3)",
  display: "block",
  marginBottom: 6,
};

const estiloInput: CSSProperties = {
  width: "100%",
  boxSizing: "border-box",
  fontFamily: "var(--sbf-font-sans)",
  fontSize: 14,
  color: "var(--text)",
  background: "var(--surface)",
  border: "1px solid var(--line)",
  borderRadius: "var(--sbf-radius-lg)",
  padding: "10px 12px",
  minHeight: 44,
};

function Campo({
  rotulo,
  htmlFor,
  children,
  ajuda,
}: {
  rotulo: string;
  htmlFor: string;
  children: ReactNode;
  ajuda?: string;
}) {
  return (
    <div>
      <label htmlFor={htmlFor} style={estiloRotulo}>
        {rotulo}
      </label>
      {children}
      {ajuda ? (
        <div style={{ fontSize: 11.5, color: "var(--text-4)", marginTop: 4, lineHeight: 1.4 }}>
          {ajuda}
        </div>
      ) : null}
    </div>
  );
}

const OVERLAY_STYLE: CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 50,
  display: "flex",
  alignItems: "flex-end",
  justifyContent: "center",
};

/**
 * Folha de criar/editar um recorrente.
 *
 * Não pede produto financeiro, categoria nem fornecedor — a página antiga
 * (`dashboard/recurring/[id]`) também não pede categoria/fornecedor, e pedir
 * produto financeiro puxaria junto `getInstitutions()` só pra rotular o
 * seletor. Ficou fora do escopo desta primeira versão; um recorrente já
 * com esses vínculos (criado por outro caminho) continua com eles intactos
 * — este formulário nunca inclui essas chaves no payload, e `updateRecurringTemplate`
 * só toca nas colunas que vierem no objeto.
 */
export function RecorrenteFormSheet({
  modo,
  template,
  onFechar,
  onSalvo,
}: RecorrenteFormSheetProps) {
  const [form, setForm] = useState<FormState>(() => estadoInicial(template));
  const [salvando, setSalvando] = useState(false);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setSalvando(true);
    try {
      const payload = {
        name: form.name.trim(),
        type: form.type,
        frequency: form.frequency,
        day_of_month:
          form.frequency === "monthly" && form.dayOfMonth ? Number(form.dayOfMonth) : null,
        custom_interval_days:
          form.frequency === "custom" && form.customIntervalDays
            ? Number(form.customIntervalDays)
            : null,
        amount: form.isVariableAmount
          ? form.amount
            ? Number(form.amount)
            : null
          : Number(form.amount),
        is_variable_amount: form.isVariableAmount,
        priority: form.priority || null,
        starts_at: form.startsAt || null,
        ends_at: form.endsAt || null,
        notes: form.notes.trim() || null,
      };

      if (modo === "criar") {
        await createRecurringTemplate(payload);
        toast.success("Recorrente criado");
      } else {
        await updateRecurringTemplate(template!.id, payload);
        toast.success("Recorrente atualizado");
      }
      onSalvo();
    } catch (erro) {
      toast.error(
        modo === "criar" ? "Não deu pra criar o recorrente" : "Não deu pra atualizar o recorrente",
        {
          description: erro instanceof Error ? erro.message : String(erro),
        },
      );
    } finally {
      setSalvando(false);
    }
  }

  return (
    <div style={OVERLAY_STYLE}>
      <Sheet
        title={modo === "criar" ? "Novo recorrente" : "Editar recorrente"}
        onClose={onFechar}
        style={{ maxHeight: "90dvh", overflowY: "auto" }}
      >
        <form onSubmit={handleSubmit} style={{ display: "grid", gap: 16 }}>
          <Campo rotulo="Nome" htmlFor="rec-nome">
            <input
              id="rec-nome"
              style={estiloInput}
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="Ex.: Aluguel, Internet, Academia"
              required
            />
          </Campo>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <Campo rotulo="Tipo" htmlFor="rec-tipo">
              <select
                id="rec-tipo"
                style={estiloInput}
                value={form.type}
                onChange={(e) => setForm((f) => ({ ...f, type: e.target.value as TipoRecorrente }))}
              >
                {TIPO_OPCOES.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Campo>
            <Campo rotulo="Frequência" htmlFor="rec-frequencia">
              <select
                id="rec-frequencia"
                style={estiloInput}
                value={form.frequency}
                onChange={(e) =>
                  setForm((f) => ({ ...f, frequency: e.target.value as FrequenciaRecorrencia }))
                }
              >
                {FREQUENCIA_OPCOES.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Campo>
          </div>

          {form.frequency === "monthly" ? (
            <Campo
              rotulo="Dia do mês (opcional)"
              htmlFor="rec-dia"
              ajuda="Sem isso, a próxima data segue o mesmo dia da última instância."
            >
              <input
                id="rec-dia"
                type="number"
                min={1}
                max={31}
                style={estiloInput}
                value={form.dayOfMonth}
                onChange={(e) => setForm((f) => ({ ...f, dayOfMonth: e.target.value }))}
              />
            </Campo>
          ) : null}

          {form.frequency === "custom" ? (
            <Campo
              rotulo="Intervalo em dias"
              htmlFor="rec-intervalo"
              ajuda="Necessário pra calcular a próxima data e entrar no total mensal comprometido."
            >
              <input
                id="rec-intervalo"
                type="number"
                min={1}
                style={estiloInput}
                value={form.customIntervalDays}
                onChange={(e) => setForm((f) => ({ ...f, customIntervalDays: e.target.value }))}
                required
              />
            </Campo>
          ) : null}

          <Campo rotulo="Prioridade" htmlFor="rec-prioridade">
            <select
              id="rec-prioridade"
              style={estiloInput}
              value={form.priority}
              onChange={(e) =>
                setForm((f) => ({ ...f, priority: e.target.value as "" | PrioridadeRecorrencia }))
              }
            >
              {PRIORIDADE_OPCOES.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </Campo>

          <div>
            <span style={estiloRotulo}>Valor</span>
            <ChipGroup
              options={[
                { value: "fixo", label: "Valor fixo" },
                { value: "variavel", label: "Valor variável" },
              ]}
              value={form.isVariableAmount ? "variavel" : "fixo"}
              onChange={(v) => setForm((f) => ({ ...f, isVariableAmount: v === "variavel" }))}
              style={{ marginBottom: 10 }}
            />
            <Campo
              rotulo={form.isVariableAmount ? "Valor de referência (estimativa)" : "Valor (R$)"}
              htmlFor="rec-valor"
              ajuda={
                form.isVariableAmount
                  ? "Uma conta de consumo real: o valor muda todo ciclo. O que entrar aqui é usado como estimativa, nunca como fato."
                  : undefined
              }
            >
              <input
                id="rec-valor"
                type="number"
                step="0.01"
                min="0"
                style={estiloInput}
                value={form.amount}
                onChange={(e) => setForm((f) => ({ ...f, amount: e.target.value }))}
                required={!form.isVariableAmount}
              />
            </Campo>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <Campo rotulo="Início (opcional)" htmlFor="rec-inicio">
              <input
                id="rec-inicio"
                type="date"
                style={estiloInput}
                value={form.startsAt}
                onChange={(e) => setForm((f) => ({ ...f, startsAt: e.target.value }))}
              />
            </Campo>
            <Campo rotulo="Término (opcional)" htmlFor="rec-fim">
              <input
                id="rec-fim"
                type="date"
                style={estiloInput}
                value={form.endsAt}
                onChange={(e) => setForm((f) => ({ ...f, endsAt: e.target.value }))}
              />
            </Campo>
          </div>

          <Campo rotulo="Observações (opcional)" htmlFor="rec-notas">
            <textarea
              id="rec-notas"
              rows={3}
              style={{ ...estiloInput, resize: "vertical" }}
              value={form.notes}
              onChange={(e) => setForm((f) => ({ ...f, notes: e.target.value }))}
            />
          </Campo>

          <div style={{ display: "flex", gap: 10, paddingTop: 4 }}>
            <Button
              type="submit"
              variant="primary"
              size="lg"
              disabled={salvando}
              style={{ flex: 1, minHeight: 44 }}
            >
              {salvando ? "Salvando…" : "Salvar"}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="lg"
              onClick={onFechar}
              disabled={salvando}
              style={{ flex: 1, minHeight: 44 }}
            >
              Cancelar
            </Button>
          </div>
        </form>
      </Sheet>
    </div>
  );
}
