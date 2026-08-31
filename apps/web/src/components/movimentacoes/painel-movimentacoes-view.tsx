"use client";

/**
 * A tela Movimentações: tudo que entrou e saiu, com busca, filtros e o
 * caminho para corrigir.
 *
 * Três decisões que valem o comentário:
 *
 *   - **O resumo descreve o recorte inteiro, não a página carregada.** Os
 *     totais vêm de agregação no banco; a lista vem paginada. Um resumo que
 *     só somasse as 30 linhas visíveis mentiria a cada "Carregar mais".
 *
 *   - **Compra no cartão não se confunde com saída de conta.** A linha diz
 *     "No cartão <nome>" por escrito e ganha um filete lateral. O texto vem
 *     primeiro de propósito: cor sozinha não comunica estado (WCAG 2.2 AA).
 *
 *   - **Previsto nunca vira fato.** Os números grandes somam só movimentação
 *     confirmada; o que está previsto aparece marcado, com o valor à vista.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Card,
  ChipGroup,
  EmptyState,
  Icon,
  MoneyMetricCard,
  SectionTitle,
  TransactionRow,
  formatBRL,
} from "@sbf/design-system";
import {
  carregarMovimentacoes,
  type OpcoesMovimentacoes,
  type PainelMovimentacoes,
} from "@/app/actions/painel-movimentacoes";
import {
  FILTROS_PADRAO,
  OPCOES_PERIODO,
  OPCOES_SITUACAO,
  OPCOES_TIPO,
  TAMANHO_PAGINA,
  agruparPorDia,
  contarFiltrosDetalhados,
  filtrosParaBusca,
  metaDaMovimentacao,
  temFiltroAtivo,
  valorComSinal,
  type ChavePeriodo,
  type ChaveTipo,
  type FiltrosMovimentacoes,
} from "./filtros";
import { FolhaFiltros } from "./folha-filtros";

export interface PainelMovimentacoesViewProps {
  dadosIniciais: PainelMovimentacoes;
  filtrosIniciais: FiltrosMovimentacoes;
  opcoes: OpcoesMovimentacoes;
}

interface PilulaAtiva {
  chave: string;
  rotulo: string;
  remover: () => void;
}

const ATRASO_BUSCA_MS = 350;

function plural(quantidade: number, singular: string, plural_: string): string {
  return quantidade === 1 ? singular : plural_;
}

export function PainelMovimentacoesView({
  dadosIniciais,
  filtrosIniciais,
  opcoes,
}: PainelMovimentacoesViewProps) {
  const router = useRouter();

  const [filtros, setFiltros] = useState<FiltrosMovimentacoes>(filtrosIniciais);
  const [dados, setDados] = useState<PainelMovimentacoes>(dadosIniciais);
  const [texto, setTexto] = useState(filtrosIniciais.busca);
  const [carregando, setCarregando] = useState(false);
  const [erro, setErro] = useState(false);
  const [folhaAberta, setFolhaAberta] = useState(false);

  // A primeira renderização já recebe os dados do servidor; buscar de novo
  // aqui faria a tela pedir duas vezes a mesma coisa a cada visita.
  const primeiraRenderizacao = useRef(true);
  // "A última requisição vence": sem isso, uma resposta lenta de um filtro
  // antigo pode chegar depois e sobrescrever o resultado do filtro atual.
  const requisicaoAtual = useRef(0);

  useEffect(() => {
    if (primeiraRenderizacao.current) {
      primeiraRenderizacao.current = false;
      return;
    }

    const minha = ++requisicaoAtual.current;
    setCarregando(true);
    setErro(false);

    carregarMovimentacoes(filtros)
      .then((novos) => {
        if (minha === requisicaoAtual.current) setDados(novos);
      })
      .catch(() => {
        if (minha === requisicaoAtual.current) setErro(true);
      })
      .finally(() => {
        if (minha === requisicaoAtual.current) setCarregando(false);
      });

    // A URL acompanha o recorte sem disparar navegação: `replaceState` mantém
    // o link compartilhável e o botão voltar coerentes sem refazer o RSC.
    const busca = filtrosParaBusca(filtros);
    window.history.replaceState(null, "", busca ? `?${busca}` : window.location.pathname);
  }, [filtros]);

  /** Trocar qualquer filtro recomeça a paginação: o recorte é outro. */
  const aplicar = useCallback((parcial: Partial<FiltrosMovimentacoes>) => {
    setFiltros((atuais) => ({ ...atuais, ...parcial, limite: TAMANHO_PAGINA }));
  }, []);

  useEffect(() => {
    if (texto === filtros.busca) return;
    const relogio = setTimeout(() => aplicar({ busca: texto }), ATRASO_BUSCA_MS);
    return () => clearTimeout(relogio);
  }, [texto, filtros.busca, aplicar]);

  const limparTudo = useCallback(() => {
    setTexto("");
    setFiltros({ ...FILTROS_PADRAO });
  }, []);

  const carregarMais = useCallback(() => {
    setFiltros((atuais) => ({ ...atuais, limite: atuais.limite + TAMANHO_PAGINA }));
  }, []);

  const { resumo, itens } = dados;
  const grupos = useMemo(() => agruparPorDia(itens, dados.hoje), [itens, dados.hoje]);
  const filtrosDetalhados = contarFiltrosDetalhados(filtros);
  const comFiltro = temFiltroAtivo(filtros);
  const previstoTotal = resumo.previstoEntrada + resumo.previstoSaida;

  const nomeDe = (lista: { id: string; nome: string }[], id: string | null): string =>
    lista.find((item) => item.id === id)?.nome ?? "—";

  const pilulas: PilulaAtiva[] = [];
  if (filtros.situacao !== "todas") {
    pilulas.push({
      chave: "situacao",
      rotulo: OPCOES_SITUACAO.find((o) => o.value === filtros.situacao)?.label ?? "Situação",
      remover: () => aplicar({ situacao: "todas" }),
    });
  }
  if (filtros.produtoId) {
    pilulas.push({
      chave: "conta",
      rotulo: nomeDe(opcoes.contas, filtros.produtoId),
      remover: () => aplicar({ produtoId: null }),
    });
  }
  if (filtros.categoriaId) {
    pilulas.push({
      chave: "categoria",
      rotulo: `Categoria: ${nomeDe(opcoes.categorias, filtros.categoriaId)}`,
      remover: () => aplicar({ categoriaId: null }),
    });
  }
  if (filtros.centroId) {
    pilulas.push({
      chave: "centro",
      rotulo: `Centro: ${nomeDe(opcoes.centros, filtros.centroId)}`,
      remover: () => aplicar({ centroId: null }),
    });
  }
  if (filtros.tagId) {
    pilulas.push({
      chave: "tag",
      rotulo: `Tag: ${nomeDe(opcoes.tags, filtros.tagId)}`,
      remover: () => aplicar({ tagId: null }),
    });
  }

  return (
    <div style={{ display: "grid", gap: 20, paddingBottom: 24 }}>
      {/* ── Resumo do recorte ─────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Resumo · {dados.periodo.rotulo}</SectionTitle>

        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))",
          }}
        >
          <MoneyMetricCard
            variant="hero"
            label="Resultado"
            value={formatBRL(resumo.resultado)}
            caption="Entrou menos saiu, contando só o que já foi confirmado"
            note={
              previstoTotal > 0
                ? `Fora ${formatBRL(previstoTotal)} em ${resumo.quantidadePrevista} ${plural(resumo.quantidadePrevista, "movimentação prevista", "movimentações previstas")}, ainda sem confirmação.`
                : undefined
            }
          />
          <MoneyMetricCard
            label="Entrou"
            value={formatBRL(resumo.entrou)}
            caption="Entradas confirmadas nas contas"
            note={
              resumo.previstoEntrada > 0
                ? `Mais ${formatBRL(resumo.previstoEntrada)} previstos, ainda não confirmados.`
                : undefined
            }
          />
          <MoneyMetricCard
            label="Saiu"
            value={formatBRL(resumo.saiu)}
            caption="Saídas confirmadas das contas, incluindo pagamento de fatura"
            note={
              resumo.previstoSaida > 0
                ? `Mais ${formatBRL(resumo.previstoSaida)} previstos, ainda não confirmados.`
                : undefined
            }
          />
          {resumo.noCartao !== 0 ? (
            <MoneyMetricCard
              label="No cartão"
              value={formatBRL(resumo.noCartao)}
              caption="Comprado no cartão neste recorte"
              note="Ainda não saiu da conta: sai no pagamento da fatura, por isso não entra em “Saiu”."
            />
          ) : null}
        </div>
      </section>

      {/* ── Busca e filtros ───────────────────────────────────────────── */}
      <section>
        <Card variant="quiet" padding={12}>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <div style={{ position: "relative", flex: "1 1 auto", minWidth: 0 }}>
              <Icon
                name="search"
                size={19}
                style={{
                  position: "absolute",
                  left: 12,
                  top: "50%",
                  transform: "translateY(-50%)",
                  color: "var(--text-4)",
                  pointerEvents: "none",
                }}
              />
              <input
                type="search"
                value={texto}
                onChange={(evento) => setTexto(evento.target.value)}
                placeholder="Buscar por descrição ou fornecedor"
                aria-label="Buscar movimentações por descrição ou fornecedor"
                style={{
                  fontFamily: "var(--sbf-font-sans)",
                  width: "100%",
                  boxSizing: "border-box",
                  minHeight: 44,
                  padding: "10px 12px 10px 38px",
                  fontSize: 14,
                  color: "var(--text)",
                  background: "var(--surface)",
                  border: "1px solid var(--line)",
                  borderRadius: "var(--sbf-radius-pill)",
                  outlineColor: "var(--primary)",
                }}
              />
            </div>
            <Button
              variant={filtrosDetalhados > 0 ? "secondary" : "outline"}
              size="lg"
              icon="tune"
              onClick={() => setFolhaAberta(true)}
            >
              {filtrosDetalhados > 0 ? `Filtros (${filtrosDetalhados})` : "Filtros"}
            </Button>
          </div>

          <ChipGroup
            scroll
            options={[...OPCOES_PERIODO]}
            value={filtros.periodo}
            onChange={(valor) => aplicar({ periodo: valor as ChavePeriodo })}
          />
          <ChipGroup
            scroll
            options={[...OPCOES_TIPO]}
            value={filtros.tipo}
            onChange={(valor) => aplicar({ tipo: valor as ChaveTipo })}
          />

          {pilulas.length > 0 ? (
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              {pilulas.map((pilula) => (
                <button
                  key={pilula.chave}
                  type="button"
                  onClick={pilula.remover}
                  aria-label={`Remover filtro ${pilula.rotulo}`}
                  style={{
                    fontFamily: "var(--sbf-font-sans)",
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    minHeight: 38,
                    padding: "6px 10px 6px 14px",
                    fontSize: 12.5,
                    fontWeight: 700,
                    cursor: "pointer",
                    color: "var(--secondary-ink)",
                    background: "var(--secondary-soft)",
                    border: "1px solid var(--secondary-line)",
                    borderRadius: "var(--sbf-radius-pill)",
                  }}
                >
                  {pilula.rotulo}
                  <Icon name="close" size={16} />
                </button>
              ))}
            </div>
          ) : null}
        </Card>
      </section>

      {/* ── Lista ─────────────────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>
          {resumo.total === 1 ? "1 movimentação" : `${resumo.total} movimentações`}
        </SectionTitle>

        {erro ? (
          <Card variant="quiet">
            <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
              <Icon name="error" size={19} style={{ color: "var(--danger)" }} />
              <span style={{ flex: "1 1 auto", fontSize: 13, color: "var(--text-2)" }}>
                Não consegui carregar as movimentações agora.
              </span>
              <Button
                variant="outline"
                size="lg"
                icon="refresh"
                onClick={() => setFiltros((atuais) => ({ ...atuais }))}
              >
                Tentar de novo
              </Button>
            </div>
          </Card>
        ) : null}

        {carregando && itens.length === 0 ? (
          <Card variant="quiet">
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                fontSize: 13,
                color: "var(--text-3)",
              }}
            >
              <Icon name="progress_activity" size={18} />
              <span>Carregando movimentações…</span>
            </div>
          </Card>
        ) : null}

        {itens.length === 0 && !carregando ? (
          <Card>
            <EmptyState
              icon={comFiltro ? "filter_alt_off" : "receipt_long"}
              title={comFiltro ? "Nada com esses filtros" : "Nenhuma movimentação ainda"}
              description={
                comFiltro
                  ? "Nenhuma movimentação casa com o recorte atual. Amplie o período ou tire um filtro para ver mais."
                  : "Assim que uma conta for conectada ou um comprovante chegar, tudo que entrou e saiu aparece aqui."
              }
            >
              {comFiltro ? (
                <Button variant="outline" size="lg" icon="restart_alt" onClick={limparTudo}>
                  Limpar filtros
                </Button>
              ) : (
                <Button
                  variant="primary"
                  size="lg"
                  icon="add"
                  onClick={() => router.push("/dashboard/transactions/new")}
                >
                  Registrar movimentação
                </Button>
              )}
            </EmptyState>
          </Card>
        ) : (
          <div
            style={{
              display: "grid",
              gap: 14,
              opacity: carregando ? 0.55 : 1,
              transition: "opacity var(--sbf-motion-fast) var(--sbf-ease)",
            }}
            aria-busy={carregando}
          >
            {grupos.map((grupo) => (
              <div key={grupo.data} style={{ display: "grid", gap: 6 }}>
                <div
                  style={{
                    fontSize: 12,
                    fontWeight: 800,
                    letterSpacing: ".04em",
                    textTransform: "uppercase",
                    color: "var(--text-3)",
                    paddingLeft: 4,
                  }}
                >
                  {grupo.rotulo}
                </div>
                <Card variant="flush">
                  {grupo.itens.map((item) => (
                    <TransactionRow
                      key={item.id}
                      description={item.descricao}
                      meta={metaDaMovimentacao(item)}
                      amount={valorComSinal(item.direcao, formatBRL(item.valor, { cents: true }))}
                      direction={item.direcao === "entrada" ? "in" : "out"}
                      centers={item.centros}
                      hasDocument={item.temDocumento}
                      pending={item.prevista}
                      onClick={() => router.push(`/dashboard/transactions/${item.id}`)}
                      // O filete lateral reforça o "No cartão" que já está
                      // escrito no meta — nunca o substitui.
                      style={
                        item.noCartao
                          ? { borderLeft: "3px solid var(--secondary)", paddingLeft: 13 }
                          : undefined
                      }
                    />
                  ))}
                </Card>
              </div>
            ))}
          </div>
        )}

        {dados.temMais ? (
          <Button
            block
            variant="outline"
            size="lg"
            icon="expand_more"
            disabled={carregando}
            onClick={carregarMais}
          >
            {carregando ? "Carregando…" : "Carregar mais"}
          </Button>
        ) : null}

        {itens.length > 0 ? (
          <div style={{ fontSize: 12.5, color: "var(--text-3)", textAlign: "center" }}>
            {`Mostrando ${itens.length} de ${resumo.total} ${plural(resumo.total, "movimentação", "movimentações")}.`}
          </div>
        ) : null}
      </section>

      <FolhaFiltros
        aberta={folhaAberta}
        filtros={filtros}
        opcoes={opcoes}
        total={resumo.total}
        onAlterar={aplicar}
        onLimpar={limparTudo}
        onFechar={() => setFolhaAberta(false)}
      />
    </div>
  );
}
