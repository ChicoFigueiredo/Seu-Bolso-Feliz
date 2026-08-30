"use client";

/**
 * A tela Hoje. Existe para responder, em cinco a dez segundos: quanto eu tenho,
 * quanto disso é realmente meu, o que vence agora, e como está o mês.
 *
 * A ordem das seções é a ordem das perguntas, não a ordem das tabelas. E dois
 * cuidados que o brief pede explicitamente e que são fáceis de perder:
 *
 *   - "Disponível agora", "Livre após compromissos" e "Resultado do mês" são
 *     três coisas diferentes e aparecem separadas, nunca fundidas num "saldo".
 *   - Nada estimado é apresentado como fato. Quando existe compromisso sem
 *     valor confirmado, o número livre vem marcado como estimativa, com a
 *     razão à vista.
 */
import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  AccountBalanceCard,
  Button,
  Card,
  CreditCardSummary,
  EmptyState,
  FinancialCenterCard,
  FinancialTimeline,
  Icon,
  MoneyMetricCard,
  PendingBox,
  PeriodSummary,
  SectionTitle,
  UpcomingCommitment,
  formatBRL,
} from "@sbf/design-system";
import type { PainelHoje } from "@/app/actions/painel-hoje";

const DIAS = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"];

/** Rótulo humano para a data de um compromisso: "Hoje", "Amanhã", "Ter". */
function rotuloDoDia(iso: string, hojeIso: string): string {
  if (iso === hojeIso) return "Hoje";
  const d = new Date(`${iso}T12:00:00`);
  const hoje = new Date(`${hojeIso}T12:00:00`);
  const dias = Math.round((d.getTime() - hoje.getTime()) / 86400000);
  if (dias === 1) return "Amanhã";
  if (dias === -1) return "Ontem";
  if (dias < 0) return "Atrasado";
  return DIAS[d.getDay()] ?? "";
}

function dataCurta(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  return `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
}

export function PainelHojeView({ dados }: { dados: PainelHoje }) {
  const router = useRouter();
  const [pendenciasAbertas, setPendenciasAbertas] = useState(false);

  const hojeIso = new Date().toISOString().slice(0, 10);
  const temAlgo = dados.contas.length > 0 || dados.cartoes.length > 0 || dados.proximos.length > 0;

  if (!temAlgo) {
    return (
      <EmptyState
        icon="account_balance"
        title="Ainda não há nada por aqui"
        description="Conecte uma conta pelo Open Finance ou envie um comprovante para o Seu Bolso Feliz começar a montar sua posição."
      >
        <Button variant="primary" icon="link" onClick={() => router.push("/dashboard/conexoes")}>
          Conectar uma conta
        </Button>
      </EmptyState>
    );
  }

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      {/* ── Meu dinheiro agora ───────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Meu dinheiro agora</SectionTitle>

        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))",
          }}
        >
          <MoneyMetricCard
            variant="hero"
            label="Disponível agora"
            value={formatBRL(dados.disponivelAgora)}
            caption={`Em ${dados.contas.length} ${dados.contas.length === 1 ? "conta" : "contas"}`}
          />
          <MoneyMetricCard
            label="Livre após compromissos"
            value={formatBRL(dados.livreAposCompromissos)}
            estimated={dados.estimativaIncompleta}
            caption={
              dados.estimativaIncompleta
                ? "Estimativa: há compromisso sem valor confirmado"
                : `Já descontando ${dados.proximos.length} compromisso${dados.proximos.length === 1 ? "" : "s"}`
            }
          />
          {dados.dividaTotal > 0 ? (
            <MoneyMetricCard
              label="Dívida total"
              value={formatBRL(dados.dividaTotal)}
              caption={`${formatBRL(dados.compromissoMensalDividas)} por mês`}
              actionLabel="Ver dívidas"
              onAction={() => router.push("/dashboard/dividas")}
            />
          ) : null}
        </div>

        {dados.contas.length > 0 ? (
          <div
            style={{
              display: "grid",
              gap: 10,
              gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
            }}
          >
            {dados.contas.map((c) => (
              <AccountBalanceCard
                key={c.id}
                name={c.nome}
                balance={formatBRL(c.saldo)}
                caption={c.instituicao ?? undefined}
                icon="account_balance"
                onClick={() => router.push(`/dashboard/contas/${c.id}`)}
              />
            ))}
          </div>
        ) : null}

        {dados.cartoes.length > 0 ? (
          <div
            style={{
              display: "grid",
              gap: 10,
              gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
            }}
          >
            {dados.cartoes.map((c) => (
              <CreditCardSummary
                key={c.id}
                name={c.nome}
                invoice={formatBRL(c.faturaAtual)}
                available={c.limiteLivre == null ? "—" : formatBRL(c.limiteLivre)}
                limit={c.limiteTotal == null ? "—" : formatBRL(c.limiteTotal)}
                used={c.limiteTotal ? Math.min(100, (c.faturaAtual / c.limiteTotal) * 100) : 0}
                dueLabel={c.vencimento ? `Vence ${dataCurta(c.vencimento)}` : undefined}
                dueTone={
                  c.vencimento && c.vencimento < hojeIso
                    ? "danger"
                    : c.vencimento && c.vencimento <= proximosSeteDias(hojeIso)
                      ? "warn"
                      : "neutral"
                }
              />
            ))}
          </div>
        ) : null}
      </section>

      {/* ── Próximos dias ────────────────────────────────────────────── */}
      <section>
        <FinancialTimeline
          title="Próximos dias"
          actionLabel="Ver tudo"
          onAction={() => router.push("/dashboard/movimentacoes")}
          footnote={
            dados.faltaAteFimDoPeriodo > 0
              ? `Até o fim do período, você ainda precisa de ${formatBRL(dados.faltaAteFimDoPeriodo)}.`
              : "O que há nas contas cobre os compromissos deste período."
          }
          footnoteIcon={dados.faltaAteFimDoPeriodo > 0 ? "info" : "check_circle"}
        >
          {dados.proximos.length === 0 ? (
            <div style={{ padding: "18px 4px", fontSize: 13, color: "var(--text-3)" }}>
              Nenhum compromisso à vista neste período.
            </div>
          ) : (
            dados.proximos.map((c) => (
              <UpcomingCommitment
                key={`${c.origem}-${c.id}`}
                dayLabel={rotuloDoDia(c.data, hojeIso)}
                dateLabel={dataCurta(c.data)}
                name={c.descricao}
                meta={[c.produto, c.valorEstimado ? "valor previsto" : null]
                  .filter(Boolean)
                  .join(" · ")}
                amount={formatBRL(c.valor)}
                status={c.vencido ? "overdue" : "pending"}
              />
            ))
          )}
        </FinancialTimeline>
      </section>

      {/* ── Situação do mês ──────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <PeriodSummary
          title={`Situação de ${dados.periodo.rotulo}`}
          income={formatBRL(dados.mes.entrou)}
          spent={formatBRL(dados.mes.saiu)}
          committed={formatBRL(dados.mes.aindaVaiSair)}
          projected={formatBRL(dados.mes.livreProjetado)}
          bar={{
            income: dados.mes.entrou,
            spent: dados.mes.saiu,
            committed: dados.mes.aindaVaiSair,
          }}
        />

        {dados.pendencias.length > 0 ? (
          <PendingBox
            title="Precisam da sua atenção"
            open={pendenciasAbertas}
            onToggle={() => setPendenciasAbertas((v) => !v)}
            items={dados.pendencias.map((p) => ({
              label: p.rotulo,
              count: p.quantidade,
              tone: p.rotulo === "Vencidas" ? ("danger" as const) : ("neutral" as const),
            }))}
            onSelect={(item) => {
              const alvo = dados.pendencias.find((p) => p.rotulo === item.label);
              if (alvo) router.push(alvo.rota);
            }}
          />
        ) : null}
      </section>

      {/* ── Centros ──────────────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Projetos e centros</SectionTitle>
        {dados.centros.length === 0 ? (
          <Card variant="quiet">
            <div
              style={{
                display: "flex",
                gap: 10,
                alignItems: "center",
                fontSize: 13,
                color: "var(--text-3)",
              }}
            >
              <Icon name="workspaces" size={18} />
              <span>
                Nenhum centro ainda. Eles separam quanto cada projeto consome sem duplicar o
                lançamento.
              </span>
            </div>
          </Card>
        ) : (
          <div
            style={{
              display: "grid",
              gap: 10,
              gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
            }}
          >
            {dados.centros.map((c) => (
              <FinancialCenterCard
                key={c.id}
                name={c.nome}
                balance={formatBRL(c.saldo)}
                negative={c.saldo < 0}
                meta={`${formatBRL(c.gastoNoMes)} neste período`}
                onClick={() => router.push(`/dashboard/centros/${c.id}`)}
              />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function proximosSeteDias(hojeIso: string): string {
  const d = new Date(`${hojeIso}T12:00:00`);
  d.setDate(d.getDate() + 7);
  return d.toISOString().slice(0, 10);
}
