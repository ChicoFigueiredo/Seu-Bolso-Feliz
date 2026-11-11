"use client";

import { Icon, Pill, formatBRL, type SbfTone } from "@sbf/design-system";
import type {
  InstanciaPeriodoResumo,
  StatusInstanciaRecorrente,
} from "@/app/actions/painel-recorrentes";

export interface InstanciaRowProps {
  instancia: InstanciaPeriodoResumo;
  ocupado?: boolean;
  /** Só aparece pra instância pendente e ainda não conciliada — pular não
   *  mexe em dinheiro, só tira do radar quando ela não vai acontecer. */
  onPular?: () => void;
}

const STATUS_LABEL: Record<StatusInstanciaRecorrente, string> = {
  pending: "Prevista",
  paid: "Paga",
  partial: "Parcial",
  skipped: "Pulada",
  overdue: "Vencida",
  cancelled: "Cancelada",
};

const STATUS_TONE: Record<StatusInstanciaRecorrente, SbfTone> = {
  pending: "neutral",
  paid: "success",
  partial: "warn",
  skipped: "neutral",
  overdue: "danger",
  cancelled: "neutral",
};

const DIAS = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"];

/**
 * Data de hoje no fuso do navegador. Não usa `toISOString()`: aquilo
 * converte pra UTC antes de fatiar, e num fuso negativo (Brasil, UTC-3) isso
 * empurra "hoje" pra amanhã já a partir das 21h locais — três horas por dia
 * em que "vencida" dispararia cedo demais. Achado ao construir esta tela;
 * `painel-hoje-view.tsx` tem o mesmo padrão (`new Date().toISOString()`) e a
 * mesma falha, fora do escopo desta tarefa — ver relatório final.
 */
function hojeIsoLocal(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Rótulo humano do dia — mesmo critério de painel-hoje-view.tsx. */
function rotuloDoDia(iso: string, hojeIso: string): string {
  if (iso === hojeIso) return "Hoje";
  const d = new Date(`${iso}T12:00:00`);
  const hoje = new Date(`${hojeIso}T12:00:00`);
  const dias = Math.round((d.getTime() - hoje.getTime()) / 86400000);
  if (dias === 1) return "Amanhã";
  if (dias === -1) return "Ontem";
  if (dias < 0) return "Atrasada";
  return DIAS[d.getDay()] ?? "";
}

function dataCurta(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  return `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
}

/**
 * Uma instância dentro do período — o que já foi gerado, previsto, pago ou
 * já virou movimentação de verdade.
 *
 * Não reaproveita `UpcomingCommitment`: aquele componente só conhece três
 * estados (pending/paid/overdue) e sempre desenha um botão "Paguei" pra
 * quem não está pago — aqui há seis status possíveis e a ação de marcar
 * pagamento não existe nesta tela (fica pra reconciliação de verdade). Uma
 * instância "conciliada" (com transaction_id) precisa disso visível — é a
 * defesa contra dupla contagem — por isso o pill de status dá lugar a
 * "Conciliada" antes de qualquer outra coisa.
 */
export function InstanciaRow({ instancia: i, ocupado = false, onPular }: InstanciaRowProps) {
  const hojeIso = hojeIsoLocal();
  const vencidaPorData = i.status === "pending" && i.dataEsperada < hojeIso;

  const statusLabel = i.conciliada
    ? "Conciliada"
    : vencidaPorData
      ? "Vencida"
      : STATUS_LABEL[i.status];
  const statusTone: SbfTone = i.conciliada
    ? "primary"
    : vencidaPorData
      ? "danger"
      : STATUS_TONE[i.status];
  const statusIcon = i.conciliada ? "link" : undefined;

  const podePular = i.status === "pending" && !i.conciliada && !!onPular;

  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        gap: 13,
        alignItems: "flex-start",
        padding: 13,
        borderRadius: "var(--sbf-radius-lg)",
        background: vencidaPorData ? "var(--warn-bg)" : "var(--surface)",
        border: `1px solid ${vencidaPorData ? "var(--warn-line)" : "var(--line-soft)"}`,
        color: "var(--text)",
      }}
    >
      <div style={{ flex: "0 0 54px", textAlign: "center", paddingTop: 2 }}>
        <div
          style={{
            fontSize: 11,
            fontWeight: 700,
            textTransform: "uppercase",
            letterSpacing: ".06em",
            color: vencidaPorData ? "var(--danger)" : "var(--primary-ink)",
          }}
        >
          {rotuloDoDia(i.dataEsperada, hojeIso)}
        </div>
        <div style={{ fontSize: 12, color: "var(--text-3)" }}>{dataCurta(i.dataEsperada)}</div>
      </div>

      <div style={{ flex: "1 1 auto", minWidth: 0 }}>
        <div style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em" }}>
          {i.templateNome}
        </div>
        <div style={{ marginTop: 4 }}>
          <Pill tone={statusTone} size="xs" icon={statusIcon}>
            {statusLabel}
          </Pill>
        </div>
      </div>

      <div
        style={{
          flex: "0 0 auto",
          textAlign: "right",
          display: "flex",
          flexDirection: "column",
          gap: 7,
          alignItems: "flex-end",
        }}
      >
        <div style={{ fontSize: 15, fontWeight: 800, fontVariantNumeric: "tabular-nums" }}>
          {i.valorReal != null
            ? formatBRL(i.valorReal)
            : i.valorEsperado != null
              ? formatBRL(i.valorEsperado)
              : "—"}
        </div>
        {podePular ? (
          <button
            type="button"
            onClick={onPular}
            disabled={ocupado}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              border: "1px solid var(--line)",
              background: "var(--surface)",
              color: "var(--text-3)",
              borderRadius: "var(--sbf-radius-pill)",
              padding: "6px 12px",
              minHeight: 44,
              fontSize: 12,
              fontWeight: 700,
              fontFamily: "inherit",
              cursor: ocupado ? "not-allowed" : "pointer",
            }}
          >
            <Icon name="block" size={14} />
            Pular
          </button>
        ) : null}
      </div>
    </div>
  );
}
