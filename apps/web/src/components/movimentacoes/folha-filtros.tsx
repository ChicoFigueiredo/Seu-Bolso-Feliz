"use client";

/**
 * A folha de filtros detalhados de Movimentações.
 *
 * Período e tipo ficam à vista, em pílulas, porque são os dois recortes que
 * quase toda visita usa. Situação, categoria, tag, centro e conta têm
 * cardinalidade alta demais para caber numa fila de pílulas, então vivem
 * aqui — e cada um que fica ligado volta como pílula removível na tela, para
 * que nenhum filtro escondido explique sozinho uma lista vazia.
 *
 * Os campos usam `<select>` nativo de propósito: no celular ele abre o
 * seletor do próprio sistema, que é mais rápido e mais acessível do que
 * qualquer lista que eu desenhasse aqui.
 */

import type { CSSProperties, ReactNode } from "react";
import { Button, Sheet } from "@sbf/design-system";
import type { OpcoesMovimentacoes } from "@/app/actions/painel-movimentacoes";
import {
  OPCOES_SITUACAO,
  type ChaveSituacao,
  type FiltrosMovimentacoes,
  type OpcaoFiltro,
} from "./filtros";

export interface FolhaFiltrosProps {
  aberta: boolean;
  filtros: FiltrosMovimentacoes;
  opcoes: OpcoesMovimentacoes;
  /** Quantas movimentações o recorte atual encontra — feedback imediato. */
  total: number;
  onAlterar: (parcial: Partial<FiltrosMovimentacoes>) => void;
  onLimpar: () => void;
  onFechar: () => void;
}

const rotuloEstilo: CSSProperties = {
  fontSize: 12.5,
  fontWeight: 700,
  color: "var(--text-3)",
  marginBottom: 6,
  display: "block",
};

const campoEstilo: CSSProperties = {
  fontFamily: "var(--sbf-font-sans)",
  width: "100%",
  minHeight: 44,
  padding: "10px 12px",
  fontSize: 14,
  fontWeight: 600,
  color: "var(--text)",
  background: "var(--surface)",
  border: "1px solid var(--line)",
  borderRadius: "var(--sbf-radius-md)",
};

function Campo({
  rotulo,
  valor,
  vazio,
  opcoes,
  onChange,
}: {
  rotulo: string;
  valor: string;
  /** Texto da opção "sem filtro" — o valor vazio. */
  vazio: string;
  opcoes: readonly OpcaoFiltro[];
  onChange: (valor: string) => void;
}): ReactNode {
  return (
    <label style={{ display: "block" }}>
      <span style={rotuloEstilo}>{rotulo}</span>
      <select
        value={valor}
        onChange={(evento) => onChange(evento.target.value)}
        style={campoEstilo}
      >
        <option value="">{vazio}</option>
        {opcoes.map((opcao) => (
          <option key={opcao.value} value={opcao.value}>
            {opcao.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function FolhaFiltros({
  aberta,
  filtros,
  opcoes,
  total,
  onAlterar,
  onLimpar,
  onFechar,
}: FolhaFiltrosProps) {
  if (!aberta) return null;

  const paraLista = (itens: { id: string; nome: string }[]): OpcaoFiltro[] =>
    itens.map((item) => ({ value: item.id, label: item.nome }));

  // A folha cobre a tela inteira, inclusive o BottomNav fixo (z-index 40 no
  // shell) — senão ela nasceria atrás da navegação do celular.
  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 50 }}>
      <Sheet
        title="Filtros"
        onClose={onFechar}
        style={{ maxHeight: "92%", overflowY: "auto", gap: 16 }}
      >
        <Campo
          rotulo="Situação"
          valor={filtros.situacao === "todas" ? "" : filtros.situacao}
          vazio="Todas as situações"
          // "Todas" já é a opção vazia; repeti-la na lista daria dois caminhos
          // para o mesmo estado.
          opcoes={OPCOES_SITUACAO.filter((opcao) => opcao.value !== "todas")}
          onChange={(valor) =>
            onAlterar({ situacao: (valor === "" ? "todas" : valor) as ChaveSituacao })
          }
        />
        <Campo
          rotulo="Conta ou cartão"
          valor={filtros.produtoId ?? ""}
          vazio="Todas as contas e cartões"
          opcoes={paraLista(opcoes.contas)}
          onChange={(valor) => onAlterar({ produtoId: valor || null })}
        />
        <Campo
          rotulo="Categoria"
          valor={filtros.categoriaId ?? ""}
          vazio="Todas as categorias"
          opcoes={paraLista(opcoes.categorias)}
          onChange={(valor) => onAlterar({ categoriaId: valor || null })}
        />
        <Campo
          rotulo="Centro"
          valor={filtros.centroId ?? ""}
          vazio="Todos os centros"
          opcoes={paraLista(opcoes.centros)}
          onChange={(valor) => onAlterar({ centroId: valor || null })}
        />
        <Campo
          rotulo="Tag"
          valor={filtros.tagId ?? ""}
          vazio="Todas as tags"
          opcoes={paraLista(opcoes.tags)}
          onChange={(valor) => onAlterar({ tagId: valor || null })}
        />

        <div style={{ display: "flex", gap: 10, marginTop: 4 }}>
          <Button variant="outline" size="lg" icon="restart_alt" onClick={onLimpar}>
            Limpar
          </Button>
          <Button variant="primary" size="lg" onClick={onFechar} style={{ flex: "1 1 auto" }}>
            {total === 1 ? "Ver 1 movimentação" : `Ver ${total} movimentações`}
          </Button>
        </div>
      </Sheet>
    </div>
  );
}
