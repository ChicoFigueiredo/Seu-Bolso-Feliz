"use client";

/**
 * A fila "esperando sua confirmação" — o que a IA extraiu e ainda não virou
 * lançamento porque nenhuma IA neste produto grava algo material sozinha.
 *
 * `AIReviewItem` já embute a regra de produto mais importante daqui: toda
 * sugestão carrega a própria confiança, e "Dispensar" (aqui, "Rejeitar") é tão
 * proeminente quanto aceitar. O título/corpo/rótulo de confiança já chegam
 * prontos de `carregarPainelInteligencia()` (painel-inteligencia.ts) — esta
 * view só formata o valor em reais (a única formatação que, por convenção do
 * repo, fica do lado do cliente; ver o comentário no topo daquele arquivo) e
 * cuida de estado de tela: filtro por tipo, confirmação em lote, e feedback de
 * carregando/erro.
 *
 * "Corrigir" não tem um terceiro botão dentro do card — `AIReviewItem` só tem
 * ação + dispensa — por isso vira um link discreto para a tela de revisão que
 * já resolve edição de campo a campo (draft-review-form.tsx). Duplicar aquele
 * formulário aqui seria o tipo de retrabalho que o CLAUDE.md deste projeto
 * pede para evitar.
 */
import { useState } from "react";
import Link from "next/link";
import { AIReviewItem, Button, ChipGroup, EmptyState, Sheet, formatBRL } from "@sbf/design-system";
import { approveDraftRecord, rejectDraftRecord } from "@/app/actions/ingestion";
import {
  aprovarItensDaFila,
  rejeitarItensDaFila,
  type ItemFila,
} from "@/app/actions/painel-inteligencia";
import { AvisoInline } from "./aviso-inline";

export interface FilaConfirmacaoProps {
  itensIniciais: ItemFila[];
  totalPendente: number;
}

const ROTULO_TIPO: Record<string, string> = {
  transaction: "Lançamentos",
  liability: "Dívidas",
  recurring_template: "Recorrências",
  consumption_metric: "Consumo",
};

/** Overlay fixo — mesma técnica de shell/more-sheet.tsx, só que sem CSS module próprio. */
const overlayStyle = { position: "fixed", inset: 0, zIndex: 50 } as const;

export function FilaConfirmacao({ itensIniciais, totalPendente }: FilaConfirmacaoProps) {
  const [itens, setItens] = useState(itensIniciais);
  const [filtro, setFiltro] = useState("todos");
  const [carregandoId, setCarregandoId] = useState<string | null>(null);
  const [carregandoLote, setCarregandoLote] = useState(false);
  const [sheetLote, setSheetLote] = useState<"confirmar" | "rejeitar" | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  if (itens.length === 0) {
    return (
      <EmptyState
        icon="check_circle"
        title="Nada esperando confirmação"
        description="A IA não tem nenhuma sugestão pendente agora. Assim que um novo documento pedir revisão, ele aparece aqui."
      />
    );
  }

  const tiposPresentes = Array.from(new Set(itens.map((i) => i.draftType)));
  const visiveis = filtro === "todos" ? itens : itens.filter((i) => i.draftType === filtro);

  function removerDaFila(ids: string[]) {
    setItens((prev) => prev.filter((i) => !ids.includes(i.id)));
  }

  async function confirmarUm(id: string) {
    setErro(null);
    setCarregandoId(id);
    try {
      await approveDraftRecord(id);
      removerDaFila([id]);
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não foi possível confirmar este item.");
    } finally {
      setCarregandoId(null);
    }
  }

  async function rejeitarUm(id: string) {
    setErro(null);
    setCarregandoId(id);
    try {
      await rejectDraftRecord(id);
      removerDaFila([id]);
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não foi possível rejeitar este item.");
    } finally {
      setCarregandoId(null);
    }
  }

  async function executarLote(acao: "confirmar" | "rejeitar") {
    setErro(null);
    setCarregandoLote(true);
    try {
      const ids = visiveis.map((i) => i.id);
      const resultado =
        acao === "confirmar" ? await aprovarItensDaFila(ids) : await rejeitarItensDaFila(ids);
      removerDaFila(resultado.ok);
      if (resultado.falhas.length > 0) {
        const verbo = acao === "confirmar" ? "confirmados" : "rejeitados";
        setErro(
          `${resultado.falhas.length} de ${ids.length} item(ns) não puderam ser ${verbo}. Tente novamente ou corrija individualmente.`,
        );
      }
    } finally {
      setCarregandoLote(false);
      setSheetLote(null);
    }
  }

  const rotuloFiltro = filtro === "todos" ? "todos" : "estes";

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          gap: 10,
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: 12.5, color: "var(--text-3)" }}>
          {totalPendente > itens.length
            ? `Mostrando os ${itens.length} mais antigos de ${totalPendente}`
            : `${itens.length} ${itens.length === 1 ? "item" : "itens"} aguardando confirmação`}
        </span>

        {visiveis.length > 1 ? (
          <div style={{ display: "flex", gap: 8 }}>
            <Button
              variant="soft"
              size="sm"
              icon="done_all"
              onClick={() => setSheetLote("confirmar")}
              disabled={carregandoLote}
            >
              Confirmar {rotuloFiltro}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setSheetLote("rejeitar")}
              disabled={carregandoLote}
            >
              Rejeitar {rotuloFiltro}
            </Button>
          </div>
        ) : null}
      </div>

      {tiposPresentes.length > 1 ? (
        <ChipGroup
          scroll
          value={filtro}
          onChange={setFiltro}
          options={[
            { value: "todos", label: "Todos", count: itens.length },
            ...tiposPresentes.map((t) => ({
              value: t,
              label: ROTULO_TIPO[t] ?? t,
              count: itens.filter((i) => i.draftType === t).length,
            })),
          ]}
        />
      ) : null}

      {erro ? <AvisoInline>{erro}</AvisoInline> : null}

      <div style={{ display: "grid", gap: 10 }}>
        {visiveis.map((item) => (
          <div key={item.id} style={{ display: "grid", gap: 6 }}>
            <AIReviewItem
              icon={item.icone}
              title={item.valor != null ? `${item.nome} · ${formatBRL(item.valor)}` : item.nome}
              body={item.detalhe}
              actionLabel="Confirmar"
              onAction={() => confirmarUm(item.id)}
              dismissLabel="Rejeitar"
              onDismiss={() => rejeitarUm(item.id)}
              confidence={item.confidenceLabel}
              style={carregandoId === item.id ? { opacity: 0.6, pointerEvents: "none" } : undefined}
            />
            {item.linkCorrigir ? (
              <Link
                href={item.linkCorrigir}
                style={{
                  fontSize: 12,
                  fontWeight: 700,
                  color: "var(--primary)",
                  textDecoration: "none",
                  paddingLeft: 6,
                }}
              >
                Ver detalhes e corrigir
              </Link>
            ) : null}
          </div>
        ))}
      </div>

      {sheetLote ? (
        <div style={overlayStyle}>
          <Sheet
            title={`${sheetLote === "confirmar" ? "Confirmar" : "Rejeitar"} ${visiveis.length} ${
              visiveis.length === 1 ? "item" : "itens"
            }?`}
            onClose={() => setSheetLote(null)}
          >
            <p style={{ fontSize: 13, color: "var(--text-3)", lineHeight: 1.5, margin: 0 }}>
              {sheetLote === "confirmar"
                ? `Isso marca ${visiveis.length === 1 ? "este item" : "estes itens"} como confirmados. Nenhum lançamento é criado agora — a revisão final continua na tela de documentos.`
                : `A IA aprende que errou ${visiveis.length === 1 ? "esta sugestão" : "estas sugestões"}. Nenhum lançamento é criado.`}
            </p>
            <div style={{ display: "flex", gap: 8 }}>
              {/* `flex: 1` nos dois — não `block` (que força width:100% e
                  espremeria o botão vizinho na mesma linha). */}
              <Button
                variant="primary"
                onClick={() => executarLote(sheetLote)}
                disabled={carregandoLote}
                style={{ flex: 1 }}
              >
                {carregandoLote
                  ? "Um momento…"
                  : `Sim, ${sheetLote === "confirmar" ? "confirmar" : "rejeitar"}`}
              </Button>
              <Button
                variant="outline"
                onClick={() => setSheetLote(null)}
                disabled={carregandoLote}
                style={{ flex: 1 }}
              >
                Cancelar
              </Button>
            </div>
          </Sheet>
        </div>
      ) : null}
    </div>
  );
}
