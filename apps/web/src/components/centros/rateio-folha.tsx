"use client";

/**
 * A folha de rateio — o coração de Projetos & Centros.
 *
 * Rateio é a divisão do valor de **uma** movimentação entre centros. Uma
 * movimentação real vira N rateios analíticos **sem duplicar o lançamento**:
 * é por isso que dá para separar o gasto do Edulingo do gasto pessoal sem
 * inventar uma segunda despesa (CONTEXT.md).
 *
 * As três regras que a interface precisa tornar impossíveis de violar:
 *
 *   1. **Valor, nunca percentual.** Os campos são em reais e a conversão para
 *      centavos passa por `rateio-aritmetica.ts`, num ponto só. O percentual
 *      ao lado de cada linha é derivado apenas para leitura.
 *
 *   2. **A soma nunca passa da movimentação.** Total, alocado e não alocado
 *      são recalculados a cada tecla, e salvar fica bloqueado enquanto houver
 *      excesso. O banco também recusaria (trigger `fn_valida_rateio`), mas
 *      descobrir isso depois de preencher tudo é péssimo.
 *
 *   3. **Pagamento de fatura não rateia.** As compras dentro da fatura já
 *      foram rateadas uma a uma. A explicação aparece **no lugar do
 *      formulário**, não como erro depois da tentativa.
 *
 * O que sobra sem centro é "não alocado": estado legítimo e visível, nunca
 * apresentado como falha.
 */

import { useMemo, useRef, useState, useTransition, type ReactNode } from "react";
import { Button, Card, Icon, Pill, SegmentedBar, formatBRL, formatDate } from "@sbf/design-system";
import { allocateTransaction, allocateTransactionEvenly } from "@/app/actions/financial-centers";
import type { RateioDaMovimentacao } from "@/app/actions/painel-centros";
import { CampoSelecao, CampoValor } from "./campos";
import { Folha } from "./folha";
import {
  formatarCentavos,
  paraAlocacoes,
  reaisParaCentavos,
  resumirRateio,
  type LinhaRateio,
  type ProblemaLinha,
  type ResumoRateio,
} from "./rateio-aritmetica";
import styles from "./centros.module.css";

export interface RateioFolhaProps {
  /** `null` mantém a folha fechada. */
  dados: RateioDaMovimentacao | null;
  onFechar: () => void;
  /** Chamado depois de gravar, para a página recarregar os números. */
  onGravado: () => void;
}

/** Linha do formulário com identidade própria, para o React não trocar o foco
 * de campo quando uma linha do meio é removida. */
interface LinhaComId extends LinhaRateio {
  id: string;
}

const MENSAGEM_DO_PROBLEMA: Record<ProblemaLinha, string> = {
  sem_centro: "Escolha o centro desta linha.",
  centro_repetido: "Este centro já está em outra linha — junte as duas num valor só.",
  valor_invalido: "Não consegui ler este valor.",
  valor_nao_positivo: "O valor precisa ser maior que zero.",
};

export function RateioFolha({ dados, onFechar, onGravado }: RateioFolhaProps) {
  if (!dados) return null;
  // `key`: trocar de movimentação recomeça o formulário do zero em vez de
  // reaproveitar as linhas da anterior.
  return (
    <Formulario key={dados.transacaoId} dados={dados} onFechar={onFechar} onGravado={onGravado} />
  );
}

function Formulario({
  dados,
  onFechar,
  onGravado,
}: {
  dados: RateioDaMovimentacao;
  onFechar: () => void;
  onGravado: () => void;
}) {
  const proximoId = useRef(0);
  const novaLinha = (linha: LinhaRateio): LinhaComId => {
    proximoId.current += 1;
    return { ...linha, id: `linha-${String(proximoId.current)}` };
  };

  const [linhas, setLinhas] = useState<LinhaComId[]>(() =>
    dados.rateios.length > 0
      ? dados.rateios.map((rateio) =>
          novaLinha({
            centroId: rateio.centroId,
            valorTexto: formatarCentavos(reaisParaCentavos(rateio.valor)),
          }),
        )
      : // Sem rateio ainda: uma linha em branco, que é o próximo passo óbvio.
        [novaLinha({ centroId: "", valorTexto: "" })],
  );
  const [erroDoServidor, setErroDoServidor] = useState<string | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  const [gravando, iniciarGravacao] = useTransition();

  const resumo = useMemo(() => resumirRateio(dados.valor, linhas), [dados.valor, linhas]);

  const centrosUsados = new Set(linhas.map((linha) => linha.centroId).filter(Boolean));
  const aindaCabemCentros = dados.centros.some((centro) => !centrosUsados.has(centro.id));
  const centrosEscolhidos = [...new Set(linhas.map((linha) => linha.centroId).filter(Boolean))];
  const podeDividirIgualmente = centrosEscolhidos.length >= 2 && !gravando;
  const removeTudo = resumo.linhasConsideradas === 0 && dados.rateios.length > 0;

  const limparRecados = () => {
    setErroDoServidor(null);
    setAviso(null);
  };

  const alterarLinha = (id: string, mudanca: Partial<LinhaRateio>) => {
    limparRecados();
    setLinhas((atuais) =>
      atuais.map((linha) => (linha.id === id ? { ...linha, ...mudanca } : linha)),
    );
  };

  /** Quanto sobraria se esta linha estivesse vazia — em centavos. */
  const restanteSemALinha = (id: string) =>
    resumirRateio(
      dados.valor,
      linhas.map((linha) => (linha.id === id ? { ...linha, valorTexto: "" } : linha)),
    ).naoAlocadoCentavos;

  const executar = (acao: () => Promise<void>) => {
    limparRecados();
    iniciarGravacao(async () => {
      try {
        await acao();
        onGravado();
      } catch (erro) {
        setErroDoServidor(
          erro instanceof Error ? erro.message : "Não consegui gravar o rateio. Tente de novo.",
        );
      }
    });
  };

  const salvar = () =>
    executar(async () => {
      await allocateTransaction(dados.transacaoId, paraAlocacoes(linhas));
      onFechar();
    });

  const dividirIgualmente = () =>
    executar(async () => {
      const resultado = await allocateTransactionEvenly(dados.transacaoId, centrosEscolhidos);
      setLinhas(
        resultado.allocations.map((rateio) =>
          novaLinha({
            centroId: rateio.financial_center_id,
            valorTexto: formatarCentavos(reaisParaCentavos(rateio.amount)),
          }),
        ),
      );
      setAviso("Dividido igualmente e já salvo. O resto do centavo ficou no primeiro centro.");
    });

  return (
    <Folha titulo="Ratear entre centros" aberta onFechar={onFechar}>
      <Cabecalho dados={dados} />

      {!dados.podeRatear ? (
        <>
          <Recado tom="alerta" icone="block">
            {dados.motivoNaoPodeRatear}
          </Recado>
          <Button variant="outline" size="lg" block onClick={onFechar}>
            Entendi
          </Button>
        </>
      ) : dados.centros.length === 0 ? (
        <>
          <Recado tom="neutro" icone="workspaces">
            Você ainda não tem nenhum centro ativo. Crie um em Projetos &amp; Centros — é o centro
            que separa o gasto de um projeto sem duplicar a movimentação.
          </Recado>
          <Button variant="outline" size="lg" block onClick={onFechar}>
            Fechar
          </Button>
        </>
      ) : (
        <>
          <Resumo resumo={resumo} />

          <div style={{ display: "grid", gap: 14 }}>
            {linhas.map((linha, indice) => {
              const problema = resumo.problemas[indice] ?? null;
              const restanteFora = restanteSemALinha(linha.id);

              return (
                <div
                  key={linha.id}
                  className={styles.linhaRateio}
                  style={{
                    paddingBottom: indice === linhas.length - 1 ? 0 : 14,
                    borderBottom:
                      indice === linhas.length - 1 ? "none" : "1px solid var(--line-soft)",
                  }}
                >
                  <CampoSelecao
                    label={`Centro ${String(indice + 1)}`}
                    valor={linha.centroId}
                    placeholder="Escolha um centro"
                    erro={
                      problema === "sem_centro" || problema === "centro_repetido"
                        ? MENSAGEM_DO_PROBLEMA[problema]
                        : null
                    }
                    onChange={(valor) => alterarLinha(linha.id, { centroId: valor })}
                    opcoes={dados.centros
                      .filter(
                        (centro) => centro.id === linha.centroId || !centrosUsados.has(centro.id),
                      )
                      .map((centro) => ({ valor: centro.id, rotulo: centro.nome }))}
                  />

                  <CampoValor
                    label="Valor"
                    valor={linha.valorTexto}
                    aria-label={`Valor do rateio da linha ${String(indice + 1)}`}
                    erro={
                      problema === "valor_invalido" || problema === "valor_nao_positivo"
                        ? MENSAGEM_DO_PROBLEMA[problema]
                        : null
                    }
                    onChange={(valor) => alterarLinha(linha.id, { valorTexto: valor })}
                  />

                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    {restanteFora > 0 ? (
                      <Button
                        variant="quiet"
                        size="sm"
                        onClick={() =>
                          alterarLinha(linha.id, { valorTexto: formatarCentavos(restanteFora) })
                        }
                        style={{ minHeight: 44 }}
                      >
                        {`usar ${formatBRL(restanteFora / 100, { cents: true })}`}
                      </Button>
                    ) : null}
                    <Button
                      variant="quiet"
                      size="sm"
                      icon="close"
                      aria-label={`Remover a linha ${String(indice + 1)}`}
                      onClick={() => {
                        limparRecados();
                        setLinhas((atuais) => atuais.filter((outra) => outra.id !== linha.id));
                      }}
                      style={{ minHeight: 44, minWidth: 44 }}
                    />
                  </div>
                </div>
              );
            })}
          </div>

          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <Button
              variant="outline"
              icon="add"
              size="lg"
              disabled={!aindaCabemCentros}
              onClick={() => {
                limparRecados();
                setLinhas((atuais) => [...atuais, novaLinha({ centroId: "", valorTexto: "" })]);
              }}
            >
              Adicionar centro
            </Button>
            <Button
              variant="outline"
              icon="balance"
              size="lg"
              disabled={!podeDividirIgualmente}
              onClick={dividirIgualmente}
            >
              Dividir igualmente
            </Button>
          </div>

          <p style={{ fontSize: 12, color: "var(--text-3)", margin: 0, lineHeight: 1.45 }}>
            {aindaCabemCentros
              ? "“Dividir igualmente” grava na hora, entre os centros já escolhidos — o resto do centavo fica no primeiro."
              : "Todos os centros ativos já estão neste rateio. Crie outro na lista para separar mais."}
          </p>

          {aviso ? (
            <Recado tom="bom" icone="check_circle">
              {aviso}
            </Recado>
          ) : null}
          {erroDoServidor ? (
            <Recado tom="alerta" icone="error">
              {erroDoServidor}
            </Recado>
          ) : null}

          <div style={{ display: "flex", gap: 10 }}>
            <Button variant="outline" size="lg" onClick={onFechar} style={{ flex: "0 0 auto" }}>
              Cancelar
            </Button>
            <Button
              variant="primary"
              size="lg"
              block
              disabled={!resumo.podeSalvar || gravando}
              onClick={salvar}
            >
              {gravando ? "Salvando…" : removeTudo ? "Remover todos os rateios" : "Salvar rateio"}
            </Button>
          </div>

          {resumo.excedeu ? (
            <p style={{ fontSize: 12.5, color: "var(--danger)", margin: 0, fontWeight: 700 }}>
              A soma passou do valor da movimentação em{" "}
              {formatBRL(Math.abs(resumo.naoAlocadoCentavos) / 100, { cents: true })}. Ajuste antes
              de salvar.
            </p>
          ) : null}
        </>
      )}
    </Folha>
  );
}

function Cabecalho({ dados }: { dados: RateioDaMovimentacao }) {
  return (
    <Card variant="quiet" style={{ gap: 6 }}>
      <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>{dados.descricao}</div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <span style={{ fontSize: 12.5, color: "var(--text-3)" }}>
          {formatDate(`${dados.data}T12:00:00`)}
          {dados.produto ? ` · ${dados.produto}` : ""}
        </span>
        <Pill tone="neutral" size="xs">
          {formatBRL(dados.valor, { cents: true })}
        </Pill>
      </div>
    </Card>
  );
}

function Resumo({ resumo }: { resumo: ResumoRateio }) {
  const alocado = resumo.alocadoCentavos / 100;
  const naoAlocado = resumo.naoAlocadoCentavos / 100;
  const total = resumo.totalCentavos / 100;

  return (
    <div style={{ display: "grid", gap: 10 }}>
      <SegmentedBar
        total={total}
        remainderLabel="não alocado"
        segments={[
          {
            label: "atribuído a centros",
            value: Math.min(alocado, total),
            color: resumo.excedeu ? "var(--danger)" : "var(--primary)",
          },
        ]}
      />
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          gap: 12,
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 13,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        <span style={{ color: "var(--text-3)" }}>
          Alocado{" "}
          <strong style={{ color: "var(--text)" }}>{formatBRL(alocado, { cents: true })}</strong>
        </span>
        <span style={{ color: "var(--text-3)" }}>
          Não alocado{" "}
          <strong style={{ color: resumo.excedeu ? "var(--danger)" : "var(--text)" }}>
            {formatBRL(naoAlocado, { cents: true })}
          </strong>
        </span>
      </div>
    </div>
  );
}

/** Caixa de recado: aviso do servidor, confirmação ou impedimento explicado. */
export function Recado({
  tom,
  icone,
  children,
}: {
  tom: "bom" | "alerta" | "neutro";
  icone: string;
  children: ReactNode;
}) {
  const cor =
    tom === "bom" ? "var(--success)" : tom === "alerta" ? "var(--danger)" : "var(--text-3)";
  return (
    <Card
      variant="quiet"
      style={{ gap: 6, background: tom === "alerta" ? "var(--warn-soft)" : "var(--muted)" }}
    >
      <div
        style={{
          display: "flex",
          gap: 9,
          alignItems: "flex-start",
          fontSize: 13,
          lineHeight: 1.5,
          color: "var(--text-2)",
        }}
      >
        <Icon name={icone} size={18} style={{ color: cor, flex: "0 0 auto" }} />
        <span>{children}</span>
      </div>
    </Card>
  );
}
