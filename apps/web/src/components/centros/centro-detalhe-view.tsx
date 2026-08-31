"use client";

/**
 * O detalhe de um Centro.
 *
 * Responde: quanto este projeto consumiu no período, como esse consumo se
 * distribuiu ao longo dele, e **quais movimentações** o formaram.
 *
 * O número de cada linha é o **valor alocado a este centro**, não o valor
 * cheio da movimentação. Trocar os dois é o erro que faz o Edulingo parecer
 * ter consumido a conta de luz inteira quando levou só um terço dela — por
 * isso o valor cheio aparece como contexto, em texto pequeno, e nunca como o
 * número principal da linha.
 */

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Card,
  EmptyState,
  Icon,
  MoneyMetricCard,
  Pill,
  SectionTitle,
  TransactionRow,
  formatBRL,
  formatDate,
} from "@sbf/design-system";
import { carregarRateioDaMovimentacao } from "@/app/actions/painel-centros";
import type {
  CentroNoPainel,
  DetalheCentro,
  RateioDaMovimentacao,
} from "@/app/actions/painel-centros";
import { CentroFolha } from "./centro-folha";
import { EvolucaoGasto } from "./evolucao-gasto";
import { SeloDoCentro } from "./identidade";
import { RateioFolha, Recado } from "./rateio-folha";
import { rotuloDoTipo } from "./paleta";
import { hojeIso as hojeIsoSP } from "@/lib/datas";

/** A folha de edição fala em `CentroNoPainel`; o detalhe tem os mesmos dados. */
function comoCentroNoPainel(dados: DetalheCentro): CentroNoPainel {
  return {
    id: dados.centro.id,
    nome: dados.centro.nome,
    tipo: dados.centro.tipo,
    status: dados.centro.status,
    descricao: dados.centro.descricao,
    icone: dados.centro.icone,
    cor: dados.centro.cor,
    saldo: dados.saldo,
    entrouNoPeriodo: dados.entrouNoPeriodo,
    saiuNoPeriodo: dados.saiuNoPeriodo,
    resultadoNoPeriodo: dados.resultadoNoPeriodo,
    rateios: dados.rateios,
  };
}

export function CentroDetalheView({ dados }: { dados: DetalheCentro }) {
  const router = useRouter();
  const [rateio, setRateio] = useState<RateioDaMovimentacao | null>(null);
  const [editando, setEditando] = useState(false);
  const [abrindo, setAbrindo] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [, iniciarCarregamento] = useTransition();

  const hojeIso = hojeIsoSP();
  const arquivado = dados.centro.status === "archived";

  const abrirRateio = (movimentacaoId: string) => {
    setErro(null);
    setAbrindo(movimentacaoId);
    iniciarCarregamento(async () => {
      try {
        const dadosDoRateio = await carregarRateioDaMovimentacao(movimentacaoId);
        if (!dadosDoRateio) {
          setErro("Não encontrei essa movimentação. Ela pode ter sido apagada em outra aba.");
          return;
        }
        setRateio(dadosDoRateio);
      } catch (falha) {
        setErro(
          falha instanceof Error ? falha.message : "Não consegui abrir o rateio. Tente de novo.",
        );
      } finally {
        setAbrindo(null);
      }
    });
  };

  const cortou = dados.totalMovimentacoes > dados.movimentacoes.length;
  const periodo = dados.periodo.rotuloCurto;

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      <Button
        variant="quiet"
        icon="arrow_back"
        size="lg"
        onClick={() => router.push("/dashboard/centros")}
        style={{ justifySelf: "start", paddingLeft: 0 }}
      >
        Projetos e centros
      </Button>

      <Card style={{ gap: 14 }}>
        <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
          <SeloDoCentro cor={dados.centro.cor} icone={dados.centro.icone} tamanho={44} />
          <div style={{ display: "grid", gap: 4, flex: "1 1 auto", minWidth: 0 }}>
            <span style={{ fontSize: 18, fontWeight: 800, letterSpacing: "-.015em" }}>
              {dados.centro.nome}
            </span>
            <span
              style={{
                display: "flex",
                gap: 6,
                alignItems: "center",
                flexWrap: "wrap",
                fontSize: 12.5,
                color: "var(--text-3)",
              }}
            >
              {rotuloDoTipo(dados.centro.tipo)}
              {arquivado ? (
                <Pill tone="neutral" size="xs" icon="archive">
                  arquivado
                </Pill>
              ) : null}
            </span>
          </div>
          <Button variant="outline" size="lg" icon="edit" onClick={() => setEditando(true)}>
            Editar
          </Button>
        </div>

        {dados.centro.descricao ? (
          <p style={{ margin: 0, fontSize: 13, color: "var(--text-2)", lineHeight: 1.5 }}>
            {dados.centro.descricao}
          </p>
        ) : null}

        {arquivado ? (
          <Recado tom="neutro" icone="archive">
            Este centro está arquivado: ele não recebe rateio novo e some das listas de escolha. O
            histórico abaixo continua inteiro — arquivar não é apagar.
          </Recado>
        ) : null}
      </Card>

      {erro ? (
        <Recado tom="alerta" icone="error">
          {erro}
        </Recado>
      ) : null}

      <section style={{ display: "grid", gap: 12 }}>
        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
          }}
        >
          <MoneyMetricCard
            variant="hero"
            label="Saldo do centro"
            value={formatBRL(dados.saldo)}
            caption={`Entrou ${formatBRL(dados.entrouSempre)} · saiu ${formatBRL(dados.saiuSempre)}, desde sempre`}
          />
          <MoneyMetricCard
            label={`Saiu em ${periodo}`}
            value={formatBRL(dados.saiuNoPeriodo)}
            caption={
              dados.entrouNoPeriodo > 0
                ? `Entrou ${formatBRL(dados.entrouNoPeriodo)} no mesmo período`
                : "Nenhuma entrada neste período"
            }
          />
          <MoneyMetricCard
            label="Resultado do período"
            value={formatBRL(dados.resultadoNoPeriodo)}
            caption="Entrou menos saiu, dentro do período"
          />
        </div>
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>{`Evolução em ${periodo}`}</SectionTitle>
        <Card>
          <EvolucaoGasto pontos={dados.evolucao} hojeIso={hojeIso} />
        </Card>
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Movimentações rateadas</SectionTitle>
        {dados.movimentacoes.length === 0 ? (
          <EmptyState
            icon="call_split"
            title="Nada rateado neste período"
            description="Quando uma movimentação for dividida para este centro, ela aparece aqui com o valor que coube a ele — não com o valor cheio."
          >
            <Button
              variant="outline"
              icon="arrow_back"
              onClick={() => router.push("/dashboard/centros")}
            >
              Ver o que ainda não tem centro
            </Button>
          </EmptyState>
        ) : (
          <>
            <Card variant="flush">
              {dados.movimentacoes.map((movimentacao) => (
                <TransactionRow
                  key={`${movimentacao.id}-${movimentacao.data}`}
                  description={movimentacao.descricao}
                  meta={[
                    formatDate(`${movimentacao.data}T12:00:00`),
                    movimentacao.produto,
                    movimentacao.valorAlocado < movimentacao.valorMovimentacao
                      ? `parte de ${formatBRL(movimentacao.valorMovimentacao, { cents: true })}`
                      : "movimentação inteira",
                    movimentacao.nota,
                    abrindo === movimentacao.id ? "abrindo…" : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                  amount={formatBRL(movimentacao.valorAlocado, { cents: true })}
                  direction={movimentacao.direcao === "entrada" ? "in" : "out"}
                  onClick={() => abrirRateio(movimentacao.id)}
                />
              ))}
            </Card>
            <p
              style={{
                display: "flex",
                gap: 6,
                alignItems: "flex-start",
                fontSize: 12,
                color: "var(--text-3)",
                margin: 0,
                lineHeight: 1.45,
              }}
            >
              <Icon name="info" size={15} style={{ flex: "0 0 auto", marginTop: 1 }} />
              <span>
                {cortou
                  ? `O valor de cada linha é o que coube a este centro. Mostrando ${String(dados.movimentacoes.length)} de ${String(dados.totalMovimentacoes)} movimentações do período.`
                  : "O valor de cada linha é o que coube a este centro, não o valor cheio da movimentação."}
              </span>
            </p>
          </>
        )}
      </section>

      <RateioFolha
        dados={rateio}
        onFechar={() => setRateio(null)}
        onGravado={() => router.refresh()}
      />
      <CentroFolha
        centro={editando ? comoCentroNoPainel(dados) : undefined}
        onFechar={() => setEditando(false)}
        onGravado={() => router.refresh()}
        // Excluído: recarregar esta página cairia num 404 logo depois de uma
        // ação que deu certo. O lugar para onde voltar é a lista.
        onExcluido={() => router.push("/dashboard/centros")}
      />
    </div>
  );
}
