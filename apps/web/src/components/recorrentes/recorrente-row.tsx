"use client";

import { Button, Card, Icon, Pill, formatBRL, type SbfTone } from "@sbf/design-system";
import type {
  PrioridadeRecorrencia,
  RecorrenteResumo,
  TipoRecorrente,
} from "@/app/actions/painel-recorrentes";

export interface RecorrenteRowProps {
  recorrente: RecorrenteResumo;
  /** Alguma ação deste card já está em andamento — desativa os botões pra
   *  não disparar duas vezes o mesmo clique. */
  ocupado?: boolean;
  onEditar: () => void;
  onExcluir: () => void;
  onAlternarAtivo: () => void;
  onGerarInstancias: () => void;
}

const ICONE_TIPO: Record<TipoRecorrente, string> = {
  income: "trending_up",
  expense: "receipt_long",
  liability_payment: "account_balance",
  statement_payment: "credit_card",
};

const PRIORIDADE_LABEL: Record<PrioridadeRecorrencia, string> = {
  essential: "Essencial",
  high: "Alta",
  medium: "Média",
  low: "Baixa",
  optional: "Opcional",
};

const PRIORIDADE_TONE: Record<PrioridadeRecorrencia, SbfTone> = {
  essential: "primary",
  high: "secondary",
  medium: "neutral",
  low: "neutral",
  optional: "neutral",
};

/** Data curta pt-BR — `22/08`, sem o ano (o ano quase nunca importa aqui). */
function dataCurta(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  return `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
}

/** Alvo de toque ≥44px mesmo nos tamanhos menores do Button — ver relatório
 *  final sobre o design-system não garantir isso por padrão. */
const TOQUE_MINIMO = { minHeight: 44 } as const;

/**
 * Um recorrente na lista principal da tela.
 *
 * Não reaproveita `RecurringItem` do design-system: aquele componente é pra
 * "média histórica vs. próximo valor" de um padrão detectado por IA, sem
 * espaço pra pausar/editar/excluir nem pra distinguir valor fixo de
 * variável — os dois motivos deste card existir. Reaproveita `Card`, `Pill`,
 * `Icon` e `Button`, as mesmas peças por baixo.
 */
export function RecorrenteRow({
  recorrente,
  ocupado = false,
  onEditar,
  onExcluir,
  onAlternarAtivo,
  onGerarInstancias,
}: RecorrenteRowProps) {
  const r = recorrente;
  const meta = [r.frequenciaLabel, r.categoria, r.fornecedor ?? r.produtoFinanceiro]
    .filter(Boolean)
    .join(" · ");

  return (
    <Card
      padding={16}
      style={{ borderRadius: "var(--sbf-radius-lg)", gap: 10, opacity: r.ativo ? 1 : 0.72 }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
        <Icon
          name={ICONE_TIPO[r.tipo]}
          size={20}
          style={{ color: "var(--text-3)", marginTop: 2, flex: "0 0 auto" }}
        />
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 7, flexWrap: "wrap" }}>
            <span style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>{r.nome}</span>
            {!r.ativo ? (
              <Pill tone="neutral" size="xs" icon="pause_circle">
                pausado
              </Pill>
            ) : null}
            {r.tipo === "income" ? (
              <Pill tone="secondary" size="xs">
                receita
              </Pill>
            ) : null}
            {r.prioridade ? (
              <Pill tone={PRIORIDADE_TONE[r.prioridade]} size="xs">
                {PRIORIDADE_LABEL[r.prioridade]}
              </Pill>
            ) : null}
          </div>
          {meta ? (
            <div style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>{meta}</div>
          ) : null}
        </div>
        <div style={{ textAlign: "right", flex: "0 0 auto" }}>
          {r.valor == null ? (
            <Pill tone="warn" size="xs">
              sem valor
            </Pill>
          ) : (
            <>
              <div
                style={{
                  fontSize: 17,
                  fontWeight: 800,
                  fontVariantNumeric: "tabular-nums",
                  color: "var(--text)",
                }}
              >
                {r.variavel ? "~ " : ""}
                {formatBRL(r.valor)}
              </div>
              {r.variavel ? (
                <Pill tone="warn" size="xs">
                  estimativa
                </Pill>
              ) : null}
            </>
          )}
        </div>
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          fontSize: 12.5,
          flexWrap: "wrap",
        }}
      >
        <Icon
          name="event"
          size={16}
          style={{ color: r.proximaInstancia?.vencida ? "var(--danger)" : "var(--text-3)" }}
        />
        {r.proximaInstancia ? (
          <span
            style={{
              fontWeight: 700,
              color: r.proximaInstancia.vencida ? "var(--danger)" : "var(--text-2)",
            }}
          >
            {r.proximaInstancia.vencida ? "Vencida em " : "Próxima em "}
            {dataCurta(r.proximaInstancia.data)}
          </span>
        ) : r.ativo ? (
          <>
            <span style={{ color: "var(--text-3)" }}>Sem instâncias geradas ainda</span>
            <button
              type="button"
              onClick={onGerarInstancias}
              disabled={ocupado}
              style={{
                border: 0,
                background: "transparent",
                padding: 0,
                fontFamily: "inherit",
                fontSize: 12.5,
                fontWeight: 700,
                color: "var(--primary)",
                cursor: ocupado ? "not-allowed" : "pointer",
              }}
            >
              Gerar agora
            </button>
          </>
        ) : (
          <span style={{ color: "var(--text-3)" }}>Pausado — não gera novas instâncias</span>
        )}
      </div>

      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button
          variant="outline"
          size="sm"
          icon={r.ativo ? "pause_circle" : "play_circle"}
          onClick={onAlternarAtivo}
          disabled={ocupado}
          style={TOQUE_MINIMO}
        >
          {r.ativo ? "Pausar" : "Reativar"}
        </Button>
        <Button
          variant="outline"
          size="sm"
          icon="edit"
          onClick={onEditar}
          disabled={ocupado}
          style={TOQUE_MINIMO}
        >
          Editar
        </Button>
        <Button
          variant="quiet"
          size="sm"
          icon="delete"
          onClick={onExcluir}
          disabled={ocupado}
          style={{ ...TOQUE_MINIMO, color: "var(--danger)", marginLeft: "auto" }}
        >
          Excluir
        </Button>
      </div>
    </Card>
  );
}
