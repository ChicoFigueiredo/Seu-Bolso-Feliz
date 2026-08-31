"use client";

/**
 * A lista de Projetos & Centros.
 *
 * A tela existe para responder duas perguntas, nesta ordem: **quanto cada
 * projeto consumiu neste período** e **quanto do período ainda não é de
 * projeto nenhum**. A segunda vem primeiro na página de propósito — é a única
 * que tem trabalho pendente atrás dela.
 *
 * Um Centro é *para quem* ou *para qual projeto* o dinheiro foi. Não é
 * Categoria (a natureza do gasto) nem Tag (rótulo livre, sem valor e sem
 * saldo): é a única das três que tem saldo e aceita rateio.
 *
 * Convenção de casas decimais desta seção: número de painel sai em real
 * inteiro (a varredura fica limpa), valor de uma movimentação e campo de
 * rateio saem com centavos — ali o centavo é a unidade de trabalho.
 */

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Card,
  EmptyState,
  FinancialCenterCard,
  Icon,
  SectionTitle,
  SegmentedBar,
  TransactionRow,
  formatBRL,
  formatDate,
} from "@sbf/design-system";
import { carregarRateioDaMovimentacao } from "@/app/actions/painel-centros";
import type {
  CentroNoPainel,
  PainelCentros,
  RateioDaMovimentacao,
} from "@/app/actions/painel-centros";
import { CentroFolha } from "./centro-folha";
import { RateioFolha, Recado } from "./rateio-folha";
import { corDoCentro, rotuloDoTipo } from "./paleta";
import styles from "./centros.module.css";

export function PainelCentrosView({ dados }: { dados: PainelCentros }) {
  const router = useRouter();
  const [rateio, setRateio] = useState<RateioDaMovimentacao | null>(null);
  // `undefined` = folha fechada; `null` = criando; um centro = editando.
  const [folhaCentro, setFolhaCentro] = useState<CentroNoPainel | null | undefined>(undefined);
  const [abrindo, setAbrindo] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [arquivadosAbertos, setArquivadosAbertos] = useState(false);
  const [, iniciarCarregamento] = useTransition();

  const recarregar = () => router.refresh();

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

  const semNenhumCentro = dados.centrosAtivos.length === 0 && dados.centrosArquivados.length === 0;

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      <SectionTitle actionLabel="Novo centro" onAction={() => setFolhaCentro(null)}>
        Projetos e centros
      </SectionTitle>

      {erro ? (
        <Recado tom="alerta" icone="error">
          {erro}
        </Recado>
      ) : null}

      {semNenhumCentro ? (
        <EmptyState
          icon="workspaces"
          title="Nenhum centro ainda"
          description="Um centro separa quanto um projeto ou uma pessoa consome — Edulingo, Casa, Daiane — sem duplicar a movimentação. A mesma despesa vira N rateios analíticos, e a soma nunca passa do valor original."
        >
          <Button variant="primary" icon="add" onClick={() => setFolhaCentro(null)}>
            Criar o primeiro centro
          </Button>
        </EmptyState>
      ) : (
        <>
          <NaoAlocado dados={dados} />

          <section style={{ display: "grid", gap: 12 }}>
            <SectionTitle>Centros ativos</SectionTitle>
            {dados.centrosAtivos.length === 0 ? (
              <Card variant="quiet">
                <span style={{ fontSize: 13, color: "var(--text-3)" }}>
                  Todos os seus centros estão arquivados. Reative um, ou crie outro, para voltar a
                  ratear.
                </span>
              </Card>
            ) : (
              <div className={styles.grade}>
                {dados.centrosAtivos.map((centro) => (
                  <CartaoDeCentro
                    key={centro.id}
                    centro={centro}
                    onAbrir={() => router.push(`/dashboard/centros/${centro.id}`)}
                    onEditar={() => setFolhaCentro(centro)}
                  />
                ))}
              </div>
            )}
          </section>

          <Pendentes
            dados={dados}
            abrindo={abrindo}
            onRatear={abrirRateio}
            onVerTudo={() => router.push("/dashboard/movimentacoes")}
          />

          {dados.centrosArquivados.length > 0 ? (
            <section style={{ display: "grid", gap: 12 }}>
              <SectionTitle
                actionLabel={arquivadosAbertos ? "Esconder" : "Mostrar"}
                onAction={() => setArquivadosAbertos((aberto) => !aberto)}
              >
                {`Arquivados (${String(dados.centrosArquivados.length)})`}
              </SectionTitle>
              <p style={{ fontSize: 12.5, color: "var(--text-3)", margin: 0, lineHeight: 1.5 }}>
                Centro arquivado não recebe rateio novo, mas o histórico dele continua inteiro nos
                relatórios. Arquivar não é apagar.
              </p>
              {arquivadosAbertos ? (
                <div className={styles.grade}>
                  {dados.centrosArquivados.map((centro) => (
                    <CartaoDeCentro
                      key={centro.id}
                      centro={centro}
                      onAbrir={() => router.push(`/dashboard/centros/${centro.id}`)}
                      onEditar={() => setFolhaCentro(centro)}
                    />
                  ))}
                </div>
              ) : null}
            </section>
          ) : null}
        </>
      )}

      <RateioFolha dados={rateio} onFechar={() => setRateio(null)} onGravado={recarregar} />
      <CentroFolha
        centro={folhaCentro}
        onFechar={() => setFolhaCentro(undefined)}
        onGravado={recarregar}
      />
    </div>
  );
}

/**
 * O total que ainda não é de projeto nenhum.
 *
 * Fica no topo porque é o único número desta tela com trabalho pendente
 * atrás. E é declarado com precisão — "saídas do período" — porque
 * pagamento de fatura está de fora de propósito: ele nunca vai poder ser
 * rateado, e mostrá-lo como pendência criaria uma tarefa impossível.
 */
function NaoAlocado({ dados }: { dados: PainelCentros }) {
  const { total, saidasDoPeriodo, movimentacoes } = dados.naoAlocado;
  const atribuido = Math.max(0, saidasDoPeriodo - total);

  return (
    <Card style={{ gap: 14 }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 12,
        }}
      >
        <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text-3)" }}>
          {`Sem centro em ${dados.periodo.rotuloCurto}`}
        </span>
        <span style={{ fontSize: 12.5, color: "var(--text-3)" }}>
          {`de ${formatBRL(saidasDoPeriodo)} em saídas`}
        </span>
      </div>

      <div
        style={{
          fontFamily: "var(--sbf-font-serif)",
          fontSize: 38,
          lineHeight: 1,
          fontVariantNumeric: "tabular-nums",
          color: total > 0 ? "var(--text)" : "var(--success)",
        }}
      >
        {formatBRL(total)}
      </div>

      <SegmentedBar
        total={saidasDoPeriodo}
        remainderLabel="sem centro"
        segments={[{ label: "já atribuído", value: atribuido, color: "var(--primary)" }]}
      />

      <div style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.5 }}>
        {total > 0
          ? `${String(movimentacoes)} ${
              movimentacoes === 1 ? "movimentação ainda tem" : "movimentações ainda têm"
            } parte sem centro. Não é erro — é só dinheiro que ainda não foi atribuído a projeto nenhum.`
          : "Todas as saídas rateáveis do período já estão atribuídas a algum centro."}
        {
          " Pagamento de fatura fica fora desta conta: as compras dentro dela já foram rateadas uma a uma."
        }
      </div>
    </Card>
  );
}

function Pendentes({
  dados,
  abrindo,
  onRatear,
  onVerTudo,
}: {
  dados: PainelCentros;
  abrindo: string | null;
  onRatear: (movimentacaoId: string) => void;
  onVerTudo: () => void;
}) {
  if (dados.pendentes.length === 0) return null;

  const faltamMais = dados.naoAlocado.movimentacoes - dados.pendentes.length;

  return (
    <section style={{ display: "grid", gap: 12 }}>
      <SectionTitle actionLabel="Ver movimentações" onAction={onVerTudo}>
        Ainda sem centro
      </SectionTitle>
      <Card variant="flush">
        {dados.pendentes.map((movimentacao) => (
          <TransactionRow
            key={movimentacao.id}
            description={movimentacao.descricao}
            meta={[
              formatDate(`${movimentacao.data}T12:00:00`),
              movimentacao.produto,
              movimentacao.alocado > 0
                ? `${formatBRL(movimentacao.alocado, { cents: true })} já rateado`
                : null,
              abrindo === movimentacao.id ? "abrindo…" : null,
            ]
              .filter(Boolean)
              .join(" · ")}
            amount={formatBRL(movimentacao.naoAlocado, { cents: true })}
            direction="out"
            pending={false}
            onClick={() => onRatear(movimentacao.id)}
          />
        ))}
      </Card>
      <p style={{ fontSize: 12, color: "var(--text-3)", margin: 0 }}>
        {faltamMais > 0
          ? `O valor à direita é o que falta atribuir, não o valor cheio. Há mais ${String(faltamMais)} movimentação${faltamMais === 1 ? "" : "ões"} na mesma situação.`
          : "O valor à direita é o que falta atribuir, não o valor cheio da movimentação."}
      </p>
    </section>
  );
}

/**
 * Cartão de um centro.
 *
 * O corpo do cartão leva ao detalhe e o lápis abre a edição. Os dois são
 * irmãos no DOM, não aninhados: `FinancialCenterCard` já vira `<button>`
 * quando recebe `onClick`, e botão dentro de botão é HTML inválido — o
 * navegador desmonta a árvore e o clique de dentro simplesmente some.
 */
function CartaoDeCentro({
  centro,
  onAbrir,
  onEditar,
}: {
  centro: CentroNoPainel;
  onAbrir: () => void;
  onEditar: () => void;
}) {
  const arquivado = centro.status === "archived";
  const movimento =
    centro.saiuNoPeriodo > 0 || centro.entrouNoPeriodo > 0
      ? `saiu ${formatBRL(centro.saiuNoPeriodo)}${
          centro.entrouNoPeriodo > 0 ? ` · entrou ${formatBRL(centro.entrouNoPeriodo)}` : ""
        }`
      : "sem movimento neste período";

  return (
    <div style={{ position: "relative", minWidth: 0 }}>
      <FinancialCenterCard
        name={centro.nome}
        color={corDoCentro(centro.cor)}
        balance={formatBRL(centro.saldo)}
        negative={centro.saldo < 0}
        meta={[rotuloDoTipo(centro.tipo), arquivado ? "arquivado" : null, movimento]
          .filter(Boolean)
          .join(" · ")}
        onClick={onAbrir}
        style={{ paddingRight: 56, opacity: arquivado ? 0.72 : 1 }}
      />
      <button
        type="button"
        aria-label={`Editar o centro ${centro.nome}`}
        onClick={onEditar}
        className={styles.alvo}
        style={{
          position: "absolute",
          top: 8,
          right: 8,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          border: 0,
          background: "transparent",
          borderRadius: "var(--sbf-radius-pill)",
          color: "var(--text-4)",
          cursor: "pointer",
        }}
      >
        <Icon name="edit" size={18} />
      </button>
    </div>
  );
}
