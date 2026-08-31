"use client";

/**
 * Detalhe de uma dívida: os termos, o cronograma de parcelas (pagas / a
 * vencer / atrasadas, visualmente distintas por cor + ícone + texto — nunca
 * só cor) e as ações de CRUD, todas via `Sheet`.
 */

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Card,
  ChipGroup,
  EmptyState,
  Field,
  Icon,
  Pill,
  ProgressBar,
  SectionTitle,
  formatBRL,
  formatDate,
} from "@sbf/design-system";
import {
  gerarCronogramaAutomatico,
  type DetalheDivida,
  type ParcelaDetalhe,
} from "@/app/actions/painel-dividas";
import { SheetDivida } from "./sheet-divida";
import { SheetParcela } from "./sheet-parcela";
import {
  SISTEMA_AMORTIZACAO_LABEL,
  SITUACAO_PARCELA_ICONE,
  SITUACAO_PARCELA_LABEL,
  SITUACAO_PARCELA_TOM,
  STATUS_DIVIDA_LABEL,
  TIPO_DIVIDA_LABEL,
} from "./labels";
import styles from "./dividas.module.css";

type Filtro = "todas" | "a_vencer" | "atrasada" | "paga";

export function DividaDetalheView({ divida }: { divida: DetalheDivida }) {
  const router = useRouter();
  const [editando, setEditando] = useState(false);
  const [parcelaEmEdicao, setParcelaEmEdicao] = useState<ParcelaDetalhe | "nova" | null>(null);
  const [gerando, setGerando] = useState(false);
  const [erroCronograma, setErroCronograma] = useState<string | null>(null);
  const [filtro, setFiltro] = useState<Filtro>("todas");

  function aoSalvar() {
    setEditando(false);
    setParcelaEmEdicao(null);
    router.refresh();
  }

  async function gerarCronograma() {
    setGerando(true);
    setErroCronograma(null);
    try {
      await gerarCronogramaAutomatico(divida.id);
      router.refresh();
    } catch (e) {
      setErroCronograma(e instanceof Error ? e.message : "Não deu pra gerar o cronograma.");
      setGerando(false);
    }
  }

  const parcelasFiltradas = useMemo(() => {
    if (filtro === "todas") return divida.parcelas;
    if (filtro === "paga") return divida.parcelas.filter((p) => p.situacao === "paga");
    if (filtro === "atrasada") return divida.parcelas.filter((p) => p.situacao === "atrasada");
    return divida.parcelas.filter((p) => p.situacao === "a_vencer" || p.situacao === "parcial");
  }, [divida.parcelas, filtro]);

  const contagem = useMemo(() => {
    const c = { atrasada: 0, aVencer: 0, paga: 0 };
    for (const p of divida.parcelas) {
      if (p.situacao === "atrasada") c.atrasada++;
      else if (p.situacao === "paga") c.paga++;
      else c.aVencer++;
    }
    return c;
  }, [divida.parcelas]);

  return (
    <div style={{ display: "grid", gap: 20, paddingBottom: 24 }}>
      <section style={{ display: "grid", gap: 12 }}>
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: 12,
          }}
        >
          <div style={{ minWidth: 0 }}>
            <div style={{ fontSize: 19, fontWeight: 800, color: "var(--text)" }}>{divida.nome}</div>
            <div style={{ fontSize: 13, color: "var(--text-3)" }}>{divida.credor}</div>
          </div>
          {divida.status !== "active" ? (
            <Pill
              tone={
                divida.status === "defaulted"
                  ? "danger"
                  : divida.status === "paid_off"
                    ? "success"
                    : "secondary"
              }
            >
              {STATUS_DIVIDA_LABEL[divida.status] ?? divida.status}
            </Pill>
          ) : null}
        </div>

        <Card>
          <ProgressBar
            value={divida.progresso}
            size="md"
            label={`${Math.round(divida.progresso * 100)}% quitado`}
            hint={
              divida.parcelasTotais != null
                ? `${divida.parcelasPagas} de ${divida.parcelasTotais} pagas`
                : undefined
            }
          />
          <div style={{ display: "flex", justifyContent: "space-between", gap: 12, marginTop: 12 }}>
            <div>
              <div style={{ fontSize: 11.5, color: "var(--text-3)", fontWeight: 600 }}>
                Saldo devedor
              </div>
              <div style={{ fontSize: 20, fontWeight: 800, color: "var(--text)" }}>
                {formatBRL(divida.saldoDevedor)}
              </div>
            </div>
            <div style={{ textAlign: "right" }}>
              <div style={{ fontSize: 11.5, color: "var(--text-3)", fontWeight: 600 }}>
                Valor original
              </div>
              <div style={{ fontSize: 20, fontWeight: 800, color: "var(--text)" }}>
                {formatBRL(divida.valorOriginal)}
              </div>
            </div>
          </div>
        </Card>

        <Card style={{ gap: 0 }}>
          <Field label="Tipo" value={TIPO_DIVIDA_LABEL[divida.tipo] ?? divida.tipo} />
          <Field
            label="Produto financeiro"
            value={[divida.instituicaoNome, divida.produtoNome].filter(Boolean).join(" — ") || "—"}
          />
          {divida.taxa ? (
            <Field
              label="Taxa de juros"
              value={`${(divida.taxa.valor * 100).toFixed(2)}% ${divida.taxa.tipo === "annual" ? "a.a." : "a.m."}`}
            />
          ) : null}
          {divida.sistemaAmortizacao ? (
            <Field
              label="Sistema de amortização"
              value={
                SISTEMA_AMORTIZACAO_LABEL[divida.sistemaAmortizacao] ?? divida.sistemaAmortizacao
              }
            />
          ) : null}
          {divida.inicio ? <Field label="Início" value={formatDate(divida.inicio)} /> : null}
          <Field
            label="Previsão de quitação"
            value={divida.previsaoFim ? formatDate(divida.previsaoFim.data) : "—"}
            confidence={divida.previsaoFim?.estimativa ? "baixa" : undefined}
            divider={false}
          />
        </Card>
        {divida.previsaoFim?.premissa ? (
          <div style={{ display: "flex", gap: 7, fontSize: 12.5, color: "var(--text-3)" }}>
            <Icon
              name="info"
              size={16}
              style={{ color: "var(--warn)", flexShrink: 0, marginTop: 1 }}
            />
            <span>{divida.previsaoFim.premissa}</span>
          </div>
        ) : null}

        <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
          <Button variant="outline" size="lg" icon="edit" onClick={() => setEditando(true)}>
            Editar dívida
          </Button>
        </div>
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
          }}
        >
          <SectionTitle>Cronograma de parcelas</SectionTitle>
          <Button variant="primary" size="lg" icon="add" onClick={() => setParcelaEmEdicao("nova")}>
            Nova
          </Button>
        </div>

        {divida.parcelas.length === 0 ? (
          <EmptyState
            icon="calendar_month"
            title="Nenhuma parcela lançada"
            description={
              divida.previaCronograma
                ? `Dá pra gerar as ${divida.previaCronograma.parcelas} parcelas de uma vez — ${divida.previaCronograma.premissa}, primeira de ${formatBRL(divida.previaCronograma.primeiraParcela)}.`
                : "Lance as parcelas uma a uma, ou preencha total de parcelas e data de início na dívida pra gerar o cronograma inteiro de uma vez."
            }
          >
            {divida.previaCronograma ? (
              <Button
                variant="primary"
                size="lg"
                icon="auto_awesome"
                onClick={gerarCronograma}
                disabled={gerando}
              >
                {gerando ? "Gerando…" : "Gerar cronograma automático"}
              </Button>
            ) : (
              <Button variant="outline" size="lg" icon="edit" onClick={() => setEditando(true)}>
                Preencher total de parcelas e início
              </Button>
            )}
          </EmptyState>
        ) : (
          <>
            <ChipGroup
              scroll
              value={filtro}
              onChange={(v) => setFiltro(v as Filtro)}
              options={[
                { value: "todas", label: "Todas", count: divida.parcelas.length },
                { value: "atrasada", label: "Atrasadas", count: contagem.atrasada },
                { value: "a_vencer", label: "A vencer", count: contagem.aVencer },
                { value: "paga", label: "Pagas", count: contagem.paga },
              ]}
            />
            <Card variant="flush">
              {parcelasFiltradas.length === 0 ? (
                <div
                  style={{
                    padding: "18px 4px",
                    fontSize: 13,
                    color: "var(--text-3)",
                    textAlign: "center",
                  }}
                >
                  Nenhuma parcela nesse filtro.
                </div>
              ) : (
                parcelasFiltradas.map((p) => (
                  <LinhaParcela key={p.id} parcela={p} onClick={() => setParcelaEmEdicao(p)} />
                ))
              )}
            </Card>
          </>
        )}
        {erroCronograma ? <div className={styles.formError}>{erroCronograma}</div> : null}
      </section>

      {editando ? (
        <SheetDivida
          dividaExistente={divida}
          onClose={() => setEditando(false)}
          onSalvo={aoSalvar}
        />
      ) : null}
      {parcelaEmEdicao ? (
        <SheetParcela
          liabilityId={divida.id}
          parcelaExistente={parcelaEmEdicao === "nova" ? undefined : parcelaEmEdicao}
          proximoNumeroSugerido={divida.parcelas.length + 1}
          onClose={() => setParcelaEmEdicao(null)}
          onSalvo={aoSalvar}
        />
      ) : null}
    </div>
  );
}

function LinhaParcela({ parcela, onClick }: { parcela: ParcelaDetalhe; onClick: () => void }) {
  const tom = SITUACAO_PARCELA_TOM[parcela.situacao] ?? "neutral";
  return (
    <button type="button" onClick={onClick} className={styles.scheduleRow}>
      <div
        style={{
          flex: "0 0 auto",
          width: 34,
          fontSize: 12.5,
          fontWeight: 700,
          color: "var(--text-3)",
        }}
      >
        #{parcela.numero}
      </div>
      <div style={{ flex: "1 1 auto", minWidth: 0, display: "grid", gap: 4 }}>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>
          {formatDate(parcela.vencimento)}
        </div>
        <div>
          <Pill tone={tom} size="xs" icon={SITUACAO_PARCELA_ICONE[parcela.situacao]}>
            {SITUACAO_PARCELA_LABEL[parcela.situacao] ?? parcela.situacao}
          </Pill>
        </div>
      </div>
      <div
        style={{
          flex: "0 0 auto",
          fontSize: 14,
          fontWeight: 800,
          color: "var(--text)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {formatBRL(parcela.valor)}
      </div>
      <Icon name="chevron_right" size={18} style={{ color: "var(--text-4)", flexShrink: 0 }} />
    </button>
  );
}
