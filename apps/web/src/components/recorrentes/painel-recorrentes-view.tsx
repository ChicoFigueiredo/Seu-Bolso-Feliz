"use client";

/**
 * A tela Recorrentes. Existe para responder uma pergunta antes de qualquer
 * outra: quanto do mês já está prometido antes de eu gastar qualquer coisa?
 * Esse número mora no topo, sozinho, como o card `hero` da tela Hoje.
 *
 * Todo dado já vem pronto de `carregarPainelRecorrentes()` — este componente
 * só decide o que mostrar (filtro ativo/pausado/todos) e dispara as mutações
 * (as mesmas actions de recurring.ts + painel-recorrentes.ts), sempre
 * seguidas de `router.refresh()` pra buscar o estado novo do servidor.
 */
import { useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import {
  Button,
  ChipGroup,
  EmptyState,
  FinancialTimeline,
  MoneyMetricCard,
  SectionTitle,
  formatBRL,
} from "@sbf/design-system";
import { updateRecurringInstance, updateRecurringTemplate } from "@/app/actions/recurring";
import {
  gerarInstanciasDoPeriodo,
  gerarInstanciasDoTemplate,
  type InstanciaPeriodoResumo,
  type PainelRecorrentes,
  type RecorrenteResumo,
} from "@/app/actions/painel-recorrentes";
import { RecorrenteRow } from "./recorrente-row";
import { InstanciaRow } from "./instancia-row";
import { RecorrenteFormSheet } from "./recorrente-form-sheet";
import { ExcluirRecorrenteSheet } from "./excluir-recorrente-sheet";

type Filtro = "ativos" | "pausados" | "todos";

type EstadoFolha =
  | { tipo: "criar" }
  | { tipo: "editar"; template: RecorrenteResumo }
  | { tipo: "excluir"; template: RecorrenteResumo }
  | null;

function mensagemErro(erro: unknown): string {
  return erro instanceof Error ? erro.message : String(erro);
}

export function PainelRecorrentesView({ dados }: { dados: PainelRecorrentes }) {
  const router = useRouter();
  const [filtro, setFiltro] = useState<Filtro>("ativos");
  const [folha, setFolha] = useState<EstadoFolha>(null);
  const [idsOcupados, setIdsOcupados] = useState<Set<string>>(new Set());
  const [gerandoGlobal, setGerandoGlobal] = useState(false);

  async function comOcupado(id: string, acao: () => Promise<void>) {
    setIdsOcupados((atual) => new Set(atual).add(id));
    try {
      await acao();
    } finally {
      setIdsOcupados((atual) => {
        const novo = new Set(atual);
        novo.delete(id);
        return novo;
      });
    }
  }

  function fecharEAtualizar() {
    setFolha(null);
    router.refresh();
  }

  async function alternarAtivo(r: RecorrenteResumo) {
    await comOcupado(r.id, async () => {
      try {
        await updateRecurringTemplate(r.id, { is_active: !r.ativo });
        toast.success(r.ativo ? "Recorrente pausado" : "Recorrente reativado");
        router.refresh();
      } catch (erro) {
        toast.error("Não deu pra atualizar o recorrente", { description: mensagemErro(erro) });
      }
    });
  }

  async function gerarUm(r: RecorrenteResumo) {
    await comOcupado(r.id, async () => {
      try {
        const criadas = await gerarInstanciasDoTemplate(r.id);
        toast.success(
          criadas > 0 ? `${criadas} instância(s) criada(s)` : "Nada novo pra gerar agora",
        );
        router.refresh();
      } catch (erro) {
        toast.error("Não deu pra gerar instâncias", { description: mensagemErro(erro) });
      }
    });
  }

  async function pularInstancia(i: InstanciaPeriodoResumo) {
    await comOcupado(i.id, async () => {
      try {
        await updateRecurringInstance(i.id, { status: "skipped" });
        toast.success("Instância pulada");
        router.refresh();
      } catch (erro) {
        toast.error("Não deu pra pular esta instância", { description: mensagemErro(erro) });
      }
    });
  }

  async function gerarTodos() {
    setGerandoGlobal(true);
    try {
      const resultado = await gerarInstanciasDoPeriodo();
      if (resultado.falhas.length > 0) {
        toast.error(
          `${resultado.criadas} instância(s) criada(s), mas ${resultado.falhas.length} recorrente(s) falharam`,
          {
            description: resultado.falhas.map((f) => `${f.nome}: ${f.erro}`).join(" · "),
          },
        );
      } else {
        toast.success(
          resultado.criadas > 0
            ? `${resultado.criadas} instância(s) criada(s)`
            : "Tudo já estava gerado",
        );
      }
      router.refresh();
    } catch (erro) {
      toast.error("Não deu pra gerar as instâncias", { description: mensagemErro(erro) });
    } finally {
      setGerandoGlobal(false);
    }
  }

  if (dados.recorrentes.length === 0) {
    return (
      <>
        <EmptyState
          icon="autorenew"
          title="Nenhum recorrente ainda"
          description="Cadastre assinaturas, contas de consumo e parcelamentos que se repetem todo mês para ver, de cara, quanto do seu mês já está comprometido."
        >
          <Button
            variant="primary"
            icon="add"
            size="lg"
            onClick={() => setFolha({ tipo: "criar" })}
            style={{ minHeight: 44 }}
          >
            Criar meu primeiro recorrente
          </Button>
        </EmptyState>
        {folha?.tipo === "criar" ? (
          <RecorrenteFormSheet
            modo="criar"
            onFechar={() => setFolha(null)}
            onSalvo={fecharEAtualizar}
          />
        ) : null}
      </>
    );
  }

  const listaFiltrada = dados.recorrentes.filter((r) => {
    if (filtro === "ativos") return r.ativo;
    if (filtro === "pausados") return !r.ativo;
    return true;
  });

  const { previstas, pagas, conciliadas } = dados.resumoInstancias;

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      <section>
        <MoneyMetricCard
          variant="hero"
          label="Total mensal comprometido"
          value={formatBRL(dados.totalMensalComprometido)}
          estimated={dados.totalMensalEstimado}
          caption={
            `Somando ${dados.contagem.ativos} recorrente(s) ativo(s)` +
            (dados.totalMensalEstimado
              ? " · inclui valor variável ou ainda sem valor definido"
              : "")
          }
        />
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 10,
            flexWrap: "wrap",
          }}
        >
          <SectionTitle>Seus recorrentes</SectionTitle>
          <div style={{ display: "flex", gap: 8 }}>
            <Button
              variant="outline"
              size="md"
              icon="autorenew"
              onClick={gerarTodos}
              disabled={gerandoGlobal}
              style={{ minHeight: 44 }}
            >
              {gerandoGlobal ? "Gerando…" : "Gerar instâncias"}
            </Button>
            <Button
              variant="primary"
              size="md"
              icon="add"
              onClick={() => setFolha({ tipo: "criar" })}
              style={{ minHeight: 44 }}
            >
              Novo
            </Button>
          </div>
        </div>

        <ChipGroup
          options={[
            { value: "ativos", label: "Ativos", count: dados.contagem.ativos },
            { value: "pausados", label: "Pausados", count: dados.contagem.pausados },
            {
              value: "todos",
              label: "Todos",
              count: dados.contagem.ativos + dados.contagem.pausados,
            },
          ]}
          value={filtro}
          onChange={(v) => setFiltro(v as Filtro)}
        />

        {listaFiltrada.length === 0 ? (
          <div style={{ padding: "18px 4px", fontSize: 13, color: "var(--text-3)" }}>
            {filtro === "pausados"
              ? "Nenhum recorrente pausado no momento."
              : "Nenhum recorrente ativo no momento."}
          </div>
        ) : (
          <div style={{ display: "grid", gap: 10 }}>
            {listaFiltrada.map((r) => (
              <RecorrenteRow
                key={r.id}
                recorrente={r}
                ocupado={idsOcupados.has(r.id)}
                onEditar={() => setFolha({ tipo: "editar", template: r })}
                onExcluir={() => setFolha({ tipo: "excluir", template: r })}
                onAlternarAtivo={() => alternarAtivo(r)}
                onGerarInstancias={() => gerarUm(r)}
              />
            ))}
          </div>
        )}
      </section>

      <section>
        <FinancialTimeline
          title="Instâncias do período"
          actionLabel="Ver movimentações"
          onAction={() => router.push("/dashboard/movimentacoes")}
          footnote={`${previstas} prevista(s) · ${pagas} paga(s) · ${conciliadas} já conciliada(s) com uma movimentação`}
          footnoteIcon="fact_check"
        >
          {dados.instancias.length === 0 ? (
            <div style={{ padding: "18px 4px", fontSize: 13, color: "var(--text-3)" }}>
              Nenhuma instância neste período ainda. Gere as instâncias dos seus recorrentes pra ver
              o que vence.
            </div>
          ) : (
            dados.instancias.map((i) => (
              <InstanciaRow
                key={i.id}
                instancia={i}
                ocupado={idsOcupados.has(i.id)}
                onPular={
                  i.status === "pending" && !i.conciliada ? () => pularInstancia(i) : undefined
                }
              />
            ))
          )}
        </FinancialTimeline>
      </section>

      {folha?.tipo === "criar" ? (
        <RecorrenteFormSheet
          modo="criar"
          onFechar={() => setFolha(null)}
          onSalvo={fecharEAtualizar}
        />
      ) : null}
      {folha?.tipo === "editar" ? (
        <RecorrenteFormSheet
          modo="editar"
          template={folha.template}
          onFechar={() => setFolha(null)}
          onSalvo={fecharEAtualizar}
        />
      ) : null}
      {folha?.tipo === "excluir" ? (
        <ExcluirRecorrenteSheet
          template={folha.template}
          onFechar={() => setFolha(null)}
          onExcluido={fecharEAtualizar}
        />
      ) : null}
    </div>
  );
}
