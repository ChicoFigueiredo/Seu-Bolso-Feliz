"use client";

/**
 * A tela Dívidas. Existe para responder duas perguntas: quanto falta, e
 * quando acaba.
 *
 * A ordem segue a pergunta: visão geral (quanto/quando) no topo, depois as
 * dívidas que realmente pesam no bolso agora (ativas), depois as que
 * precisam de atenção (renegociadas/inadimplentes — ainda devem, mas ficam
 * fora dos totais do topo, como em `painel-hoje.ts`), depois as quitadas —
 * celebradas, não arquivadas escondidas.
 *
 * Nenhuma projeção aparece como fato: toda vez que `previsaoFim` ou o valor
 * de uma parcela vêm de estimativa, a tela mostra a pílula/nota "estimado" ao
 * lado, nunca escondida.
 */

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import {
  Button,
  Card,
  DebtProgressCard,
  EmptyState,
  GoalProgress,
  MoneyMetricCard,
  Pill,
  SectionTitle,
  formatBRL,
  formatDate,
} from "@sbf/design-system";
import type { DividaResumo, PainelDividas } from "@/app/actions/painel-dividas";
import { SheetDivida } from "./sheet-divida";
import { STATUS_DIVIDA_LABEL, TIPO_DIVIDA_LABEL } from "./labels";

export function DividasView({ dados }: { dados: PainelDividas }) {
  const router = useRouter();
  const [criando, setCriando] = useState(false);

  function aoSalvar() {
    setCriando(false);
    router.refresh();
  }

  if (!dados.temAlgumaDivida) {
    return (
      <>
        <EmptyState
          icon="celebration"
          title="Nenhuma dívida por aqui"
          description="Boa notícia: você não tem nenhum empréstimo, financiamento ou parcelamento em aberto. Quando pegar um, cadastre aqui pra acompanhar a quitação de perto."
        >
          <Button variant="primary" size="lg" icon="add" onClick={() => setCriando(true)}>
            Cadastrar dívida
          </Button>
        </EmptyState>
        {criando ? <SheetDivida onClose={() => setCriando(false)} onSalvo={aoSalvar} /> : null}
      </>
    );
  }

  const quitadoAteAgora = dados.ativas.reduce(
    (s, d) => s + Math.max(0, d.valorOriginal - d.saldoDevedor),
    0,
  );

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Visão geral</SectionTitle>
        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))",
          }}
        >
          <MoneyMetricCard
            variant="hero"
            label="Dívida total"
            value={formatBRL(dados.dividaTotal)}
            caption={
              `${dados.ativas.length} dívida${dados.ativas.length === 1 ? "" : "s"} ativa${dados.ativas.length === 1 ? "" : "s"}` +
              (quitadoAteAgora > 0 ? ` · ${formatBRL(quitadoAteAgora)} já quitados` : "")
            }
          />
          <MoneyMetricCard
            label="Compromisso mensal"
            value={formatBRL(dados.compromissoMensal)}
            estimated={dados.compromissoMensalEstimado || dados.compromissoIncompleto}
            caption={dados.compromissoIncompleto ? undefined : "Soma da parcela das dívidas ativas"}
            note={
              dados.compromissoIncompleto
                ? "Não inclui dívida(s) ativa(s) sem cronograma nem total de parcelas definido."
                : dados.compromissoMensalEstimado
                  ? "Inclui parcela estimada (saldo ÷ parcelas restantes) pra dívida sem próxima parcela lançada."
                  : undefined
            }
          />
          <MoneyMetricCard
            label="Previsão de quitação total"
            value={
              dados.fimDoUltimoCompromisso ? formatDate(dados.fimDoUltimoCompromisso.data) : "—"
            }
            estimated={dados.fimDoUltimoCompromisso?.estimativa ?? false}
            caption={
              dados.fimDoUltimoCompromisso && !dados.fimDoUltimoCompromisso.estimativa
                ? "Data do último vencimento cadastrado"
                : undefined
            }
            note={
              dados.fimDoUltimoCompromisso?.premissa ??
              (dados.fimIncompleto
                ? "Falta total de parcelas (ou data de término) em alguma dívida pra projetar."
                : undefined)
            }
          />
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
          <SectionTitle>Ativas</SectionTitle>
          <Button variant="primary" size="lg" icon="add" onClick={() => setCriando(true)}>
            Nova
          </Button>
        </div>
        {dados.ativas.length === 0 ? (
          <Card variant="quiet">
            <span style={{ fontSize: 13, color: "var(--text-3)" }}>
              Nenhuma dívida ativa no momento.
            </span>
          </Card>
        ) : (
          <div style={{ display: "grid", gap: 12 }}>
            {dados.ativas.map((d) => (
              <CartaoDivida key={d.id} divida={d} />
            ))}
          </div>
        )}
      </section>

      {dados.atencao.length > 0 ? (
        <section style={{ display: "grid", gap: 12 }}>
          <SectionTitle>Precisam de atenção</SectionTitle>
          <div style={{ display: "grid", gap: 12 }}>
            {dados.atencao.map((d) => (
              <CartaoDivida key={d.id} divida={d} />
            ))}
          </div>
        </section>
      ) : null}

      {dados.quitadas.length > 0 ? (
        <section style={{ display: "grid", gap: 12 }}>
          <SectionTitle>Quitadas</SectionTitle>
          <div style={{ display: "grid", gap: 12 }}>
            {dados.quitadas.map((d) => (
              <Link
                key={d.id}
                href={`/dashboard/dividas/${d.id}`}
                style={{ textDecoration: "none", display: "block" }}
              >
                <GoalProgress
                  name={d.nome}
                  progress={1}
                  achieved
                  achievement={`Você quitou ${formatBRL(d.valorOriginal)} nesta dívida.`}
                />
              </Link>
            ))}
          </div>
        </section>
      ) : null}

      {criando ? <SheetDivida onClose={() => setCriando(false)} onSalvo={aoSalvar} /> : null}
    </div>
  );
}

function CartaoDivida({ divida }: { divida: DividaResumo }) {
  const fatos = [
    divida.taxa
      ? `${(divida.taxa.valor * 100).toFixed(2)}% ${divida.taxa.tipo === "annual" ? "a.a." : "a.m."}`
      : null,
    divida.previsaoFim
      ? `${divida.previsaoFim.estimativa ? "previsão" : "termina"} ${formatDate(divida.previsaoFim.data)}`
      : null,
  ].filter((f): f is string => f != null);

  return (
    <Link
      href={`/dashboard/dividas/${divida.id}`}
      style={{ textDecoration: "none", display: "block", color: "inherit" }}
    >
      {divida.status !== "active" ? (
        <div style={{ marginBottom: 6 }}>
          <Pill tone={divida.status === "defaulted" ? "danger" : "secondary"}>
            {STATUS_DIVIDA_LABEL[divida.status] ?? divida.status}
          </Pill>
        </div>
      ) : null}
      <DebtProgressCard
        name={divida.nome}
        meta={`${TIPO_DIVIDA_LABEL[divida.tipo] ?? divida.tipo} · ${divida.credor}`}
        balance={formatBRL(divida.saldoDevedor)}
        paid={divida.progresso}
        installments={
          divida.parcelasTotais != null
            ? `${divida.parcelasPagas} de ${divida.parcelasTotais} pagas`
            : undefined
        }
        facts={fatos}
      />
      {divida.proximoVencimento ? (
        <div
          style={{ marginTop: 6, display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 }}
        >
          <Pill tone={divida.proximoVencido ? "danger" : "neutral"} size="xs">
            {divida.proximoVencido ? "Atrasada desde" : "Próxima em"}{" "}
            {formatDate(divida.proximoVencimento)}
          </Pill>
          {divida.valorParcela != null ? (
            <span style={{ fontSize: 12.5, color: "var(--text-3)" }}>
              {formatBRL(divida.valorParcela)}
              {divida.valorParcelaEstimado ? " · estimado" : ""}
            </span>
          ) : null}
        </div>
      ) : null}
    </Link>
  );
}
