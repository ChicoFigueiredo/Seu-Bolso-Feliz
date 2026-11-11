"use client";

/**
 * A tela Documentos: a caixa de entrada de boletos, faturas e comprovantes,
 * com a captura rápida em cima.
 *
 * A ordem das seções é a ordem das perguntas de quem chega:
 *
 *   1. **Tenho um comprovante na mão agora.** A captura vem primeiro porque é
 *      a razão mais frequente de abrir esta tela no celular.
 *   2. **Sobrou algo para mim?** Se sim, aparece no topo da lista, sozinho na
 *      frente. Se não, a tela diz isso — "nada esperando por você" é notícia
 *      boa e merece ser dita, não deduzida de uma lista sem destaque.
 *   3. **O que chegou, e em que pé está?** O resto da caixa, em linguagem
 *      humana: `pending_review`, `ai_lite_enriching` e o resto do vocabulário
 *      do pipeline não chegam aqui.
 *
 * A lista se reabastece sozinha enquanto houver documento em processamento —
 * um estado que muda no servidor sem ninguém avisar o navegador seria uma tela
 * mentindo em silêncio. O relógio para quando não há mais nada em curso e
 * quando a aba sai de foco.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Button,
  Card,
  ChipGroup,
  DocumentInboxItem,
  EmptyState,
  Icon,
  SectionTitle,
  Toast,
  formatBRL,
} from "@sbf/design-system";
import {
  type CaixaDeDocumentos,
  type DocumentoDaCaixa,
  carregarCaixaDeDocumentos,
} from "@/app/actions/painel-documentos";
import { CapturaRapida } from "./captura-rapida";
import { FolhaDoDocumento } from "./folha-do-documento";
import { type BaldeDeDocumento, ORDEM_DOS_BALDES, ROTULO_DO_BALDE } from "./estado-do-documento";

/** De quanto em quanto tempo a lista se atualiza sozinha, quando há fila. */
const INTERVALO_DE_ATUALIZACAO = 12_000;

/** Quantas rodadas automáticas antes de devolver o comando para a pessoa. */
const MAX_ATUALIZACOES_AUTOMATICAS = 20;

/** Quanto a lista cresce a cada "Ver mais". */
const PASSO_DA_PAGINA = 120;

type Filtro = "tudo" | BaldeDeDocumento;

export function CaixaDeDocumentosView({ inicial }: { inicial: CaixaDeDocumentos }) {
  const [caixa, setCaixa] = useState(inicial);
  const [limite, setLimite] = useState(inicial.documentos.length || PASSO_DA_PAGINA);
  const [filtro, setFiltro] = useState<Filtro>("tudo");
  const [abertoId, setAbertoId] = useState<string | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  const [recarregando, setRecarregando] = useState(false);

  const limiteAtual = useRef(limite);
  limiteAtual.current = limite;

  const recarregar = useCallback(async () => {
    setRecarregando(true);
    try {
      setCaixa(await carregarCaixaDeDocumentos({ limite: limiteAtual.current }));
    } finally {
      setRecarregando(false);
    }
  }, []);

  const verMais = useCallback(async () => {
    const novo = limiteAtual.current + PASSO_DA_PAGINA;
    setLimite(novo);
    limiteAtual.current = novo;
    await recarregar();
  }, [recarregar]);

  // Enquanto houver documento sendo lido, o estado da tela envelhece sozinho.
  // Fora isso, nada de relógio: uma caixa parada não precisa de rede.
  //
  // O teto de rodadas existe porque "processando" nem sempre termina — worker
  // parado em desenvolvimento, job travado — e uma aba esquecida aberta não
  // deve bater no servidor para sempre. Depois do teto, sobra o "Atualizar".
  const emProcessamento = caixa.contagens.processando > 0;
  useEffect(() => {
    if (!emProcessamento) return;
    let rodadas = 0;
    const relogio = setInterval(() => {
      if (document.visibilityState !== "visible") return;
      if (++rodadas > MAX_ATUALIZACOES_AUTOMATICAS) {
        clearInterval(relogio);
        return;
      }
      void recarregar();
    }, INTERVALO_DE_ATUALIZACAO);
    return () => clearInterval(relogio);
  }, [emProcessamento, recarregar]);

  useEffect(() => {
    if (!aviso) return;
    const relogio = setTimeout(() => setAviso(null), 3600);
    return () => clearTimeout(relogio);
  }, [aviso]);

  const idsNaLista = useMemo(() => new Set(caixa.documentos.map((d) => d.id)), [caixa.documentos]);

  const precisamDeRevisao = caixa.documentos.filter((d) => d.balde === "revisar");
  const visiveis =
    filtro === "tudo" ? caixa.documentos : caixa.documentos.filter((d) => d.balde === filtro);

  const opcoesDeFiltro = [
    { value: "tudo", label: "Tudo", ...(caixa.completo ? { count: caixa.total } : {}) },
    ...ORDEM_DOS_BALDES.map((balde) => ({
      value: balde,
      label: ROTULO_DO_BALDE[balde],
      ...(caixa.completo ? { count: caixa.contagens[balde] } : {}),
    })),
  ];

  return (
    <div style={{ display: "grid", gap: 20, paddingBottom: 24 }}>
      <CapturaRapida
        aoReceber={() => void recarregar()}
        idsNaLista={idsNaLista}
        aoAbrirDocumento={setAbertoId}
      />

      {caixa.total === 0 ? (
        <EmptyState
          icon="folder_open"
          title="Sua caixa de entrada está vazia"
          description="Fotografe um comprovante, anexe um boleto, ou conecte o Gmail para os documentos chegarem sozinhos."
        />
      ) : (
        <>
          {/* A boa notícia só é dita quando a leitura cobriu a caixa inteira.
              Com a lista paginada, "nada esperando por você" poderia estar
              ignorando um boleto de três meses atrás fora da janela — e nesta
              tela uma tranquilidade falsa custa caro. */}
          {precisamDeRevisao.length === 0 && caixa.completo ? (
            <Card variant="quiet" padding={14}>
              <div
                style={{
                  display: "flex",
                  gap: 10,
                  alignItems: "center",
                  fontFamily: "var(--sbf-font-sans)",
                }}
              >
                <Icon name="check_circle" size={20} style={{ color: "var(--primary)" }} />
                <span style={{ fontSize: 13, color: "var(--text-2)" }}>
                  Nada esperando por você. Tudo que chegou já foi lido ou já está em andamento.
                </span>
              </div>
            </Card>
          ) : null}

          <section style={{ display: "grid", gap: 12 }}>
            <SectionTitle
              actionLabel={recarregando ? "Atualizando…" : "Atualizar"}
              onAction={() => void recarregar()}
            >
              O que chegou
            </SectionTitle>

            <ChipGroup
              options={opcoesDeFiltro}
              value={filtro}
              onChange={(valor) => setFiltro(valor as Filtro)}
              scroll
            />

            {visiveis.length === 0 ? (
              <EmptyState
                icon="filter_alt_off"
                title={`Nada em "${filtro === "tudo" ? "Tudo" : ROTULO_DO_BALDE[filtro]}"`}
                description="Troque o filtro para ver o resto da caixa de entrada."
              />
            ) : (
              <Card variant="flush">
                {visiveis.map((documento) => (
                  <DocumentInboxItem
                    key={documento.id}
                    name={documento.nome}
                    meta={metaDoDocumento(documento)}
                    value={
                      documento.valor == null
                        ? undefined
                        : formatBRL(documento.valor, { cents: true })
                    }
                    state={documento.estado}
                    onClick={() => setAbertoId(documento.id)}
                  />
                ))}
              </Card>
            )}

            {!caixa.completo ? (
              <div style={{ display: "flex", justifyContent: "center" }}>
                <Button
                  variant="outline"
                  size="lg"
                  icon="expand_more"
                  onClick={() => void verMais()}
                >
                  Ver mais ({caixa.documentos.length} de {caixa.total})
                </Button>
              </div>
            ) : null}
          </section>
        </>
      )}

      {abertoId ? (
        <FolhaDoDocumento
          documentoId={abertoId}
          aoFechar={() => setAbertoId(null)}
          aoMudar={() => void recarregar()}
          avisar={setAviso}
        />
      ) : null}

      {aviso ? (
        <div
          style={{
            position: "fixed",
            left: 0,
            right: 0,
            bottom: "calc(96px + env(safe-area-inset-bottom))",
            display: "flex",
            justifyContent: "center",
            padding: "0 16px",
            zIndex: 60,
            pointerEvents: "none",
          }}
        >
          <Toast>{aviso}</Toast>
        </div>
      ) : null}
    </div>
  );
}

/** Quanto do "por quê" cabe numa linha da lista antes de virar parágrafo. */
const MAX_DO_PORQUE = 90;

/**
 * A linha de contexto de cada documento.
 *
 * Origem e chegada respondem "de onde veio isto". O "por quê" só entra nos
 * baldes em que ele acrescenta algo — em "processando" e "concluídos" a pílula
 * de estado já disse tudo, e repetir com outras palavras só faz a linha crescer
 * numa tela de 390px. O texto inteiro continua na folha de detalhe.
 */
function metaDoDocumento(documento: DocumentoDaCaixa): string {
  const contexto = [documento.origem, documento.chegada].filter(Boolean).join(" · ");
  const explica = documento.balde === "revisar" || documento.balde === "erro";
  const porQue = explica && documento.porQue ? encurtar(documento.porQue) : null;
  return [contexto, porQue].filter(Boolean).join(" — ");
}

function encurtar(texto: string): string {
  return texto.length <= MAX_DO_PORQUE ? texto : `${texto.slice(0, MAX_DO_PORQUE - 1).trimEnd()}…`;
}
