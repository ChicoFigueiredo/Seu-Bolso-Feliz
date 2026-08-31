"use client";

import { useEffect, useState, type CSSProperties } from "react";
import { toast } from "sonner";
import { Button, Field, Sheet } from "@sbf/design-system";
import { deleteRecurringTemplate, getRecurringInstances } from "@/app/actions/recurring";
import type { RecorrenteResumo } from "@/app/actions/painel-recorrentes";

export interface ExcluirRecorrenteSheetProps {
  template: RecorrenteResumo;
  onFechar: () => void;
  onExcluido: () => void;
}

interface Contagem {
  total: number;
  pendentes: number;
  conciliadas: number;
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
 * Confirmação de exclusão — sem mentir sobre o efeito.
 *
 * `deleteRecurringTemplate` é soft delete: só marca `is_active = false`,
 * exatamente o que "Pausar" já faz (ver recurring.ts). Não existe hoje uma
 * exclusão de verdade nem uma coluna que distinga "pausado" de "excluído" —
 * então esta folha não promete nada que o banco não cumpre: não diz que o
 * histórico some, não diz que as instâncias são canceladas. Ela busca a
 * contagem real de instâncias deste template (não só as do período atual)
 * antes de habilitar o botão de confirmar, pra "o que acontece com elas"
 * ser um número visto, não uma suposição.
 */
export function ExcluirRecorrenteSheet({
  template,
  onFechar,
  onExcluido,
}: ExcluirRecorrenteSheetProps) {
  const [carregando, setCarregando] = useState(true);
  const [contagem, setContagem] = useState<Contagem | null>(null);
  const [excluindo, setExcluindo] = useState(false);

  useEffect(() => {
    let cancelado = false;
    getRecurringInstances(template.id)
      .then((instancias) => {
        if (cancelado) return;
        setContagem({
          total: instancias.length,
          pendentes: instancias.filter((i) => i.status === "pending" && i.transaction_id == null)
            .length,
          conciliadas: instancias.filter((i) => i.transaction_id != null).length,
        });
      })
      .catch(() => {
        if (!cancelado) setContagem(null);
      })
      .finally(() => {
        if (!cancelado) setCarregando(false);
      });
    return () => {
      cancelado = true;
    };
  }, [template.id]);

  async function handleConfirmar() {
    setExcluindo(true);
    try {
      await deleteRecurringTemplate(template.id);
      toast.success("Recorrente desativado");
      onExcluido();
    } catch (erro) {
      toast.error("Não deu pra desativar o recorrente", {
        description: erro instanceof Error ? erro.message : String(erro),
      });
    } finally {
      setExcluindo(false);
    }
  }

  return (
    <div style={OVERLAY_STYLE}>
      <Sheet title="Desativar este recorrente?" onClose={onFechar}>
        <p style={{ fontSize: 13.5, color: "var(--text-2)", lineHeight: 1.5, margin: 0 }}>
          <strong>{template.nome}</strong> para de gerar novas instâncias e sai da lista de ativos.
          Isso não apaga nada: o histórico já registrado continua onde está, e dá pra reativar
          depois pelo filtro "Pausados".
        </p>

        <div style={{ display: "grid" }}>
          <Field
            label="Instâncias no total"
            value={carregando ? "Verificando…" : String(contagem?.total ?? 0)}
          />
          <Field
            label="Pendentes (sem transação)"
            value={carregando ? "…" : String(contagem?.pendentes ?? 0)}
          />
          <Field
            label="Já conciliadas com uma movimentação"
            value={carregando ? "…" : String(contagem?.conciliadas ?? 0)}
            divider={false}
          />
        </div>

        {!carregando && contagem && contagem.pendentes > 0 ? (
          <p style={{ fontSize: 12.5, color: "var(--warn)", margin: 0, lineHeight: 1.4 }}>
            As {contagem.pendentes} instância(s) pendente(s) continuam existindo mesmo depois de
            desativar — desativar não as cancela nem as apaga.
          </p>
        ) : null}

        <div style={{ display: "flex", gap: 10, paddingTop: 4 }}>
          <Button
            type="button"
            variant="outline"
            size="lg"
            onClick={handleConfirmar}
            disabled={excluindo || carregando}
            style={{ flex: 1, minHeight: 44, color: "var(--danger)", borderColor: "var(--danger)" }}
          >
            {excluindo ? "Desativando…" : "Sim, desativar"}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="lg"
            onClick={onFechar}
            disabled={excluindo}
            style={{ flex: 1, minHeight: 44 }}
          >
            Cancelar
          </Button>
        </div>
      </Sheet>
    </div>
  );
}
