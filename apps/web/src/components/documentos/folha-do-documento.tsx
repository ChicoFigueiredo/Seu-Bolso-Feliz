"use client";

/**
 * A folha de detalhe de um documento.
 *
 * Responde, nesta ordem, às perguntas de quem abriu: em que pé está isto? o
 * que o sistema leu, e com quanta confiança? cadê o arquivo que eu mandei? e o
 * que eu faço agora?
 *
 * O link para o **arquivo original** aparece antes dos campos extraídos, e não
 * depois. Quando a leitura automática falha — PDF protegido, formato que
 * nenhum parser conhece — é a única coisa da tela que ainda serve, e procurá-la
 * no fim de uma lista de campos vazios seria cruel. Quando o arquivo não está
 * mais no storage, a folha diz isso em voz alta em vez de esconder o botão.
 */
import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Card, Field, Icon, Pill, Sheet, formatBRL, toneStyles } from "@sbf/design-system";
import {
  approveDraftRecord,
  rejectDraftRecord,
  reprocessDocument,
  triggerAiFullAnalysis,
} from "@/app/actions/ingestion";
import {
  type DetalheDocumento,
  type EdicaoDeRascunho,
  type RascunhoDoDocumento,
  type ValorDeCampo,
  carregarDetalheDocumento,
  corrigirRascunhoDeTransacao,
} from "@/app/actions/painel-documentos";
import styles from "./documentos.module.css";

export interface FolhaDoDocumentoProps {
  documentoId: string;
  aoFechar: () => void;
  /** Algo mudou no servidor: a caixa de entrada precisa ser relida. */
  aoMudar: () => void;
  /** Aviso passageiro de sucesso. Erro nunca passa por aqui — ver `erro`. */
  avisar: (texto: string) => void;
}

export function FolhaDoDocumento({
  documentoId,
  aoFechar,
  aoMudar,
  avisar,
}: FolhaDoDocumentoProps) {
  const router = useRouter();
  const [detalhe, setDetalhe] = useState<DetalheDocumento | null>(null);
  const [carregando, setCarregando] = useState(true);
  const [ocupado, setOcupado] = useState(false);
  const [erro, setErro] = useState<string | null>(null);

  const recarregar = useCallback(async () => {
    setCarregando(true);
    try {
      setDetalhe(await carregarDetalheDocumento(documentoId));
    } catch (e) {
      setErro(mensagemDe(e));
    } finally {
      setCarregando(false);
    }
  }, [documentoId]);

  useEffect(() => {
    void recarregar();
  }, [recarregar]);

  /**
   * Executa uma ação do servidor e mantém as duas telas coerentes.
   *
   * O erro fica **na folha**, não num aviso que some: quem viu "não deu para
   * aprovar" precisa decidir o que fazer, e uma mensagem passageira tira a
   * informação da tela antes da decisão.
   */
  const executar = useCallback(
    async (acao: () => Promise<unknown>, sucesso: string) => {
      setOcupado(true);
      setErro(null);
      try {
        await acao();
        avisar(sucesso);
        aoMudar();
        await recarregar();
      } catch (e) {
        setErro(mensagemDe(e));
      } finally {
        setOcupado(false);
      }
    },
    [aoMudar, avisar, recarregar],
  );

  // Esc e toque no véu fecham a folha. O `Sheet` só oferece o "×", e uma folha
  // modal da qual só se sai mirando um alvo de 34px é hostil no celular.
  useEffect(() => {
    const aoTeclar = (evento: KeyboardEvent) => {
      if (evento.key === "Escape") aoFechar();
    };
    window.addEventListener("keydown", aoTeclar);
    return () => window.removeEventListener("keydown", aoTeclar);
  }, [aoFechar]);

  return (
    <div
      className={styles.folhaSobreposta}
      onClick={(evento) => {
        // O véu é irmão do cartão dentro do Sheet, não filho deste div — daí
        // perguntar pelo `[role=dialog]` em vez de comparar com currentTarget.
        const alvo = evento.target;
        if (alvo instanceof Element && alvo.closest('[role="dialog"]')) return;
        aoFechar();
      }}
    >
      <Sheet
        title={detalhe?.nome ?? "Documento"}
        onClose={aoFechar}
        style={{ maxHeight: "88dvh", overflowY: "auto" }}
      >
        {carregando && !detalhe ? (
          <div style={{ padding: "22px 0", fontSize: 13, color: "var(--text-3)" }}>Abrindo…</div>
        ) : !detalhe ? (
          <div style={{ padding: "22px 0", fontSize: 13, color: "var(--text-3)" }}>
            Este documento não está mais disponível.
          </div>
        ) : (
          <div style={{ display: "grid", gap: 14 }}>
            <Cabecalho detalhe={detalhe} />

            {detalhe.porQue || detalhe.oQueFazer ? (
              <Explicacao
                detalhe={detalhe}
                aoIrPara={(rota) => {
                  aoFechar();
                  router.push(rota);
                }}
              />
            ) : null}

            <ArquivoOriginal detalhe={detalhe} />

            {detalhe.campos.length > 0 ? (
              <Card padding={16} style={{ gap: 11 }}>
                <div style={{ fontSize: 13, fontWeight: 700 }}>O que o sistema leu</div>
                {detalhe.campos.map((campo, indice) => (
                  <Field
                    key={campo.rotulo}
                    label={campo.rotulo}
                    value={textoDoValor(campo.valor)}
                    confidence={campo.confianca ?? undefined}
                    divider={indice < detalhe.campos.length - 1}
                  />
                ))}
              </Card>
            ) : null}

            {detalhe.pendencias.length > 0 ? (
              <Card variant="quiet" padding={14} style={{ gap: 7 }}>
                <div style={{ fontSize: 12.5, fontWeight: 700, color: "var(--text-2)" }}>
                  O que ficou faltando
                </div>
                {detalhe.pendencias.map((pendencia) => (
                  <div
                    key={pendencia}
                    style={{
                      display: "flex",
                      gap: 8,
                      fontSize: 12.5,
                      color: "var(--text-3)",
                      lineHeight: 1.45,
                    }}
                  >
                    <Icon name="remove" size={16} style={{ color: "var(--text-4)" }} />
                    <span>{pendencia}</span>
                  </div>
                ))}
              </Card>
            ) : null}

            {detalhe.rascunhos.map((rascunho) => (
              <LinhaDeRascunho
                key={rascunho.id}
                rascunho={rascunho}
                ocupado={ocupado}
                aoAprovar={() =>
                  executar(() => approveDraftRecord(rascunho.id), "Lançamento aprovado.")
                }
                aoRejeitar={() =>
                  executar(
                    () => rejectDraftRecord(rascunho.id, "Rejeitado na revisão de documentos"),
                    "Lançamento rejeitado.",
                  )
                }
                aoCorrigir={(edicao) =>
                  executar(
                    () =>
                      corrigirRascunhoDeTransacao({
                        documentoId: detalhe.id,
                        rascunhoId: rascunho.id,
                        ...edicao,
                      }),
                    "Correção salva.",
                  )
                }
              />
            ))}

            {erro ? (
              <Card
                padding={14}
                style={{
                  gap: 6,
                  borderColor: toneStyles.danger.line,
                  background: toneStyles.danger.bg,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    gap: 8,
                    alignItems: "center",
                    fontSize: 13,
                    fontWeight: 700,
                    color: toneStyles.danger.fg,
                  }}
                >
                  <Icon name="error" size={18} />
                  Não deu para concluir
                </div>
                <div style={{ fontSize: 12.5, color: toneStyles.danger.fg, lineHeight: 1.45 }}>
                  {erro}
                </div>
              </Card>
            ) : null}

            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <Button
                variant="outline"
                size="lg"
                icon="refresh"
                disabled={ocupado}
                onClick={() =>
                  executar(
                    () => reprocessDocument(detalhe.id),
                    "Mandamos ler de novo. Isso leva alguns instantes.",
                  )
                }
              >
                Ler de novo
              </Button>
              <Button
                variant="secondary"
                size="lg"
                icon="auto_awesome"
                disabled={ocupado}
                onClick={() =>
                  executar(
                    () => triggerAiFullAnalysis(detalhe.id),
                    "Pedimos a leitura completa por IA.",
                  )
                }
              >
                Leitura completa por IA
              </Button>
            </div>

            {detalhe.mensagens.length > 0 ? <Diario detalhe={detalhe} /> : null}
          </div>
        )}
      </Sheet>
    </div>
  );
}

// ── Blocos ──────────────────────────────────────────────────────────────────

function Cabecalho({ detalhe }: { detalhe: DetalheDocumento }) {
  const meta = [detalhe.origem, detalhe.chegada, detalhe.formato, detalhe.tamanho]
    .filter(Boolean)
    .join(" · ");

  return (
    <div style={{ display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" }}>
      <Pill tone={TOM_DO_ESTADO[detalhe.estado]} icon={ICONE_DO_ESTADO[detalhe.estado]}>
        {ROTULO_DO_ESTADO[detalhe.estado]}
      </Pill>
      <span style={{ fontSize: 12, color: "var(--text-3)" }}>{meta}</span>
    </div>
  );
}

function Explicacao({
  detalhe,
  aoIrPara,
}: {
  detalhe: DetalheDocumento;
  aoIrPara: (rota: string) => void;
}) {
  const preocupante = detalhe.balde === "erro";
  const rota = detalhe.rotaDoQueFazer;
  return (
    <Card
      variant={preocupante ? "surface" : "quiet"}
      padding={14}
      style={
        preocupante
          ? { gap: 7, borderColor: toneStyles.warn.line, background: toneStyles.warn.bg }
          : { gap: 7 }
      }
    >
      {detalhe.porQue ? (
        <div style={{ fontSize: 13, fontWeight: 700, color: "var(--text)", lineHeight: 1.45 }}>
          {detalhe.porQue}
        </div>
      ) : null}
      {detalhe.oQueFazer ? (
        <div style={{ fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.45 }}>
          {detalhe.oQueFazer}
        </div>
      ) : null}
      {rota ? (
        <div style={{ marginTop: 4 }}>
          <Button variant="soft" size="lg" icon="key" onClick={() => aoIrPara(rota)}>
            Cadastrar a senha
          </Button>
        </div>
      ) : null}
    </Card>
  );
}

function ArquivoOriginal({ detalhe }: { detalhe: DetalheDocumento }) {
  if (!detalhe.temOriginal) {
    return (
      <Card variant="quiet" padding={14}>
        <div style={{ display: "flex", gap: 9, fontSize: 12.5, color: "var(--text-3)" }}>
          <Icon name="folder_off" size={18} />
          <span>O arquivo original deste documento não está mais guardado.</span>
        </div>
      </Card>
    );
  }

  if (!detalhe.urlDoOriginal) {
    return (
      <Card variant="quiet" padding={14}>
        <div style={{ display: "flex", gap: 9, fontSize: 12.5, color: "var(--text-3)" }}>
          <Icon name="cloud_off" size={18} />
          <span>
            O arquivo está guardado, mas não conseguimos gerar o link agora. Feche e abra de novo em
            alguns instantes.
          </span>
        </div>
      </Card>
    );
  }

  return (
    <a
      href={detalhe.urlDoOriginal}
      target="_blank"
      rel="noreferrer"
      style={{ textDecoration: "none" }}
    >
      <Card padding={14}>
        <div style={{ display: "flex", gap: 11, alignItems: "center" }}>
          <span
            style={{
              width: 36,
              height: 36,
              flex: "0 0 36px",
              borderRadius: "var(--sbf-radius-md)",
              background: "var(--muted)",
              color: "var(--text-2)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <Icon name="description" size={20} />
          </span>
          <span style={{ flex: "1 1 auto", minWidth: 0 }}>
            <span
              style={{
                display: "block",
                fontSize: 13.5,
                fontWeight: 700,
                color: "var(--text)",
              }}
            >
              Abrir o arquivo original
            </span>
            <span style={{ display: "block", fontSize: 12, color: "var(--text-3)", marginTop: 2 }}>
              Exatamente como chegou. O link vale por uma hora.
            </span>
          </span>
          <Icon name="open_in_new" size={19} style={{ color: "var(--text-4)" }} />
        </div>
      </Card>
    </a>
  );
}

function LinhaDeRascunho({
  rascunho,
  ocupado,
  aoAprovar,
  aoRejeitar,
  aoCorrigir,
}: {
  rascunho: RascunhoDoDocumento;
  ocupado: boolean;
  aoAprovar: () => void;
  aoRejeitar: () => void;
  aoCorrigir: (edicao: EdicaoDeRascunho) => void;
}) {
  const [corrigindo, setCorrigindo] = useState(false);
  const [rascunhoEditado, setRascunhoEditado] = useState<EdicaoDeRascunho | null>(null);

  const abrirCorrecao = () => {
    setRascunhoEditado(rascunho.edicao);
    setCorrigindo(true);
  };

  return (
    <Card padding={15} style={{ gap: 10 }}>
      <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ fontSize: 11.5, fontWeight: 700, color: "var(--text-3)" }}>
            {rascunho.rotuloDoTipo}
          </div>
          <div style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em", marginTop: 2 }}>
            {rascunho.titulo}
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 3 }}>
            {[
              rascunho.valor == null ? "sem valor" : formatBRL(rascunho.valor, { cents: true }),
              rascunho.data ? `vence ${dataCurta(rascunho.data)}` : "sem vencimento",
            ].join(" · ")}
          </div>
        </div>
        <Pill tone={rascunho.pendente ? "warn" : "primary"} size="xs">
          {rascunho.rotuloDoStatus}
        </Pill>
      </div>

      {corrigindo && rascunhoEditado ? (
        <FormularioDeCorrecao
          valores={rascunhoEditado}
          ocupado={ocupado}
          aoMudar={setRascunhoEditado}
          aoCancelar={() => setCorrigindo(false)}
          aoSalvar={() => {
            setCorrigindo(false);
            aoCorrigir(rascunhoEditado);
          }}
        />
      ) : rascunho.pendente ? (
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <Button variant="soft" size="lg" icon="check" disabled={ocupado} onClick={aoAprovar}>
            Aprovar
          </Button>
          {rascunho.edicao ? (
            <Button
              variant="outline"
              size="lg"
              icon="edit"
              disabled={ocupado}
              onClick={abrirCorrecao}
            >
              Corrigir
            </Button>
          ) : null}
          <Button variant="outline" size="lg" icon="close" disabled={ocupado} onClick={aoRejeitar}>
            Rejeitar
          </Button>
        </div>
      ) : null}

      {!corrigindo && rascunho.motivoSemEdicao && rascunho.pendente ? (
        <div style={{ fontSize: 11.5, color: "var(--text-4)", lineHeight: 1.45 }}>
          {rascunho.motivoSemEdicao}
        </div>
      ) : null}
    </Card>
  );
}

function FormularioDeCorrecao({
  valores,
  ocupado,
  aoMudar,
  aoCancelar,
  aoSalvar,
}: {
  valores: EdicaoDeRascunho;
  ocupado: boolean;
  aoMudar: (novos: EdicaoDeRascunho) => void;
  aoCancelar: () => void;
  aoSalvar: () => void;
}) {
  return (
    <div style={{ display: "grid", gap: 10 }}>
      <Entrada
        rotulo="Descrição"
        valor={valores.descricao}
        aoMudar={(texto) => aoMudar({ ...valores, descricao: texto })}
      />
      <Entrada
        rotulo="Fornecedor"
        valor={valores.fornecedor}
        aoMudar={(texto) => aoMudar({ ...valores, fornecedor: texto })}
      />
      <Entrada
        rotulo="Valor (R$)"
        tipo="number"
        valor={valores.valor == null ? "" : String(valores.valor)}
        aoMudar={(texto) =>
          aoMudar({ ...valores, valor: texto.trim() === "" ? null : Number(texto) })
        }
      />
      <Entrada
        rotulo="Vencimento"
        tipo="date"
        valor={valores.vencimento ?? ""}
        aoMudar={(texto) => aoMudar({ ...valores, vencimento: texto === "" ? null : texto })}
      />
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button
          variant="primary"
          size="lg"
          icon="save"
          disabled={ocupado || (valores.valor != null && !Number.isFinite(valores.valor))}
          onClick={aoSalvar}
        >
          Salvar correção
        </Button>
        <Button variant="outline" size="lg" disabled={ocupado} onClick={aoCancelar}>
          Cancelar
        </Button>
      </div>
    </div>
  );
}

function Entrada({
  rotulo,
  valor,
  tipo = "text",
  aoMudar,
}: {
  rotulo: string;
  valor: string;
  tipo?: "text" | "number" | "date";
  aoMudar: (texto: string) => void;
}) {
  return (
    <label style={{ display: "grid", gap: 5, fontFamily: "var(--sbf-font-sans)" }}>
      <span style={{ fontSize: 12, fontWeight: 700, color: "var(--text-3)" }}>{rotulo}</span>
      <input
        type={tipo}
        inputMode={tipo === "number" ? "decimal" : undefined}
        step={tipo === "number" ? "0.01" : undefined}
        value={valor}
        onChange={(e) => aoMudar(e.target.value)}
        style={{
          fontFamily: "inherit",
          fontSize: 14,
          minHeight: 44,
          padding: "10px 12px",
          borderRadius: "var(--sbf-radius-md)",
          border: "1px solid var(--line)",
          background: "var(--surface)",
          color: "var(--text)",
          width: "100%",
          boxSizing: "border-box",
        }}
      />
    </label>
  );
}

function Diario({ detalhe }: { detalhe: DetalheDocumento }) {
  const [aberto, setAberto] = useState(false);
  return (
    <div style={{ display: "grid", gap: 8 }}>
      <button
        type="button"
        onClick={() => setAberto((v) => !v)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 7,
          border: 0,
          background: "transparent",
          padding: "8px 0",
          minHeight: 44,
          cursor: "pointer",
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 12.5,
          fontWeight: 700,
          color: "var(--text-3)",
        }}
      >
        <Icon name={aberto ? "expand_less" : "expand_more"} size={18} />
        {aberto ? "Esconder" : "Ver"} o que o sistema registrou ({detalhe.mensagens.length})
      </button>
      {aberto ? (
        <Card variant="quiet" padding={13} style={{ gap: 9 }}>
          {detalhe.mensagens.map((mensagem, indice) => (
            <div key={`${mensagem.quando}-${indice}`} style={{ display: "grid", gap: 2 }}>
              <div style={{ fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.45 }}>
                {mensagem.texto}
              </div>
              <div style={{ fontSize: 11, color: "var(--text-4)" }}>
                {[mensagem.nivel === "error" ? "erro" : "aviso", mensagem.quando]
                  .filter(Boolean)
                  .join(" · ")}
              </div>
            </div>
          ))}
        </Card>
      ) : null}
    </div>
  );
}

// ── Vocabulário visual ──────────────────────────────────────────────────────

const ROTULO_DO_ESTADO: Record<DetalheDocumento["estado"], string> = {
  pronto: "Pronto",
  "precisa-revisar": "Precisa revisar",
  "possivel-duplicidade": "Possível duplicidade",
  "protegido-por-senha": "Protegido por senha",
  "nao-reconhecido": "Não reconhecido",
  processando: "Processando",
};

const TOM_DO_ESTADO: Record<
  DetalheDocumento["estado"],
  "primary" | "warn" | "secondary" | "danger" | "neutral"
> = {
  pronto: "primary",
  "precisa-revisar": "warn",
  "possivel-duplicidade": "warn",
  "protegido-por-senha": "secondary",
  "nao-reconhecido": "danger",
  processando: "neutral",
};

const ICONE_DO_ESTADO: Record<DetalheDocumento["estado"], string> = {
  pronto: "check_circle",
  "precisa-revisar": "rate_review",
  "possivel-duplicidade": "content_copy",
  "protegido-por-senha": "lock",
  "nao-reconhecido": "help",
  processando: "hourglass_top",
};

function textoDoValor(valor: ValorDeCampo): string {
  if (valor.tipo === "dinheiro") return formatBRL(valor.valor, { cents: true });
  if (valor.tipo === "data") return dataCurta(valor.iso);
  return valor.texto;
}

function dataCurta(iso: string): string {
  const data = new Date(`${iso}T12:00:00`);
  if (Number.isNaN(data.getTime())) return iso;
  return data.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit", year: "numeric" });
}

function mensagemDe(erro: unknown): string {
  return erro instanceof Error ? erro.message : String(erro);
}
