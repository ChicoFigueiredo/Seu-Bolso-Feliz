"use client";

/**
 * Captura rápida de comprovante.
 *
 * É a ação mais importante desta tela: a pessoa está de pé, na loja, com o
 * papel na mão e uma mão livre. Tudo aqui responde a isso — alvos grandes, o
 * botão de câmera flutuando acima do BottomNav, e nenhuma etapa a mais entre
 * apontar a câmera e o arquivo estar guardado.
 *
 * Três decisões que parecem detalhe e não são:
 *
 *   1. **Dois inputs, não um.** Um `<input capture="environment">` abre a
 *      câmera direto e **impede** escolher um PDF que já está no telefone. Um
 *      input sem `capture` nunca abre a câmera de imediato. Como as duas
 *      coisas são pedidas, existem os dois — "Fotografar" e "Anexar".
 *
 *   2. **O `accept` da câmera lista JPEG/PNG/WebP, não `image/*`.** O iPhone
 *      fotografa em HEIC, que `uploadAceito` recusa; com um `accept` que só
 *      cita formatos suportados, o próprio iOS entrega JPEG convertido. Com
 *      `image/*` ele entregaria HEIC e o envio morreria no servidor.
 *
 *   3. **Nada é descartado em silêncio.** O `File` escolhido fica em memória
 *      até a pessoa mandar embora. Se o envio falhar, o item continua na tela
 *      com o motivo e um "Tentar de novo" que reenvia o mesmo arquivo.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ACCEPT_UPLOAD,
  FORMATOS,
  MIME_TYPES_CANONICOS,
  descricaoDosFormatos,
  uploadAceito,
} from "@sbf/contracts";
import { Button, Card, Icon, Pill, ProgressBar } from "@sbf/design-system";
import { uploadDocument } from "@/app/actions/ingestion";
import styles from "./documentos.module.css";
import { formatarTamanho } from "./estado-do-documento";

/** O que o seletor de arquivos oferece: extensões e os MIMEs canônicos. */
const ACEITA_ARQUIVO = [ACCEPT_UPLOAD, ...MIME_TYPES_CANONICOS].join(",");

/** O que a câmera pode entregar. Derivado da lista, nunca escrito à mão. */
const ACEITA_CAMERA = [
  ...new Set(FORMATOS.filter((f) => f.suporte === "ocr").map((f) => f.mimeType)),
].join(",");

const FORMATOS_EM_TEXTO = descricaoDosFormatos();

/** Quanto tempo o item local fica visível depois de aparecer na lista real. */
const ESPERA_ANTES_DE_SUMIR = 4000;

type EtapaDoEnvio = "aguardando" | "enviando" | "recebido" | "processando" | "falhou" | "recusado";

interface ItemDeEnvio {
  chave: string;
  arquivo: File;
  etapa: EtapaDoEnvio;
  mensagem: string | null;
  documentoId: string | null;
  /** Documento já existente, quando o arquivo é o mesmo de um envio anterior. */
  duplicadoDe: string | null;
}

const ROTULO_DA_ETAPA: Record<EtapaDoEnvio, { texto: string; icone: string }> = {
  aguardando: { texto: "Na fila", icone: "schedule" },
  enviando: { texto: "Enviando", icone: "cloud_upload" },
  recebido: { texto: "Recebido", icone: "check_circle" },
  processando: { texto: "Em processamento", icone: "hourglass_top" },
  falhou: { texto: "Não enviou", icone: "error" },
  recusado: { texto: "Formato não aceito", icone: "block" },
};

/** Etapas que fazem parte da leva em curso — a base do "X de Y". */
const NA_LEVA: ReadonlySet<EtapaDoEnvio> = new Set<EtapaDoEnvio>([
  "aguardando",
  "enviando",
  "recebido",
  "processando",
]);

export interface CapturaRapidaProps {
  /** Um arquivo virou documento no servidor: hora de reler a lista. */
  aoReceber: () => void;
  /** Ids já visíveis na caixa de entrada — o item local some quando chega lá. */
  idsNaLista: ReadonlySet<string>;
  /** Abre o detalhe de um documento (usado quando o arquivo é duplicado). */
  aoAbrirDocumento: (id: string) => void;
}

export function CapturaRapida({ aoReceber, idsNaLista, aoAbrirDocumento }: CapturaRapidaProps) {
  const [itens, setItens] = useState<ItemDeEnvio[]>([]);
  const [arrastando, setArrastando] = useState(false);
  const entradaDeArquivo = useRef<HTMLInputElement>(null);
  const entradaDeCamera = useRef<HTMLInputElement>(null);

  // A fila roda fora do ciclo de render e precisa enxergar o estado no instante
  // em que roda, não o do último render — daí o espelho em ref. Todo mundo
  // escreve por `aplicar`, então os dois nunca divergem.
  const espelho = useRef<ItemDeEnvio[]>([]);
  const escoando = useRef(false);
  const relogios = useRef<ReturnType<typeof setTimeout>[]>([]);

  useEffect(() => {
    const pendentes = relogios.current;
    return () => pendentes.forEach(clearTimeout);
  }, []);

  const aplicar = useCallback((mudar: (atuais: ItemDeEnvio[]) => ItemDeEnvio[]) => {
    espelho.current = mudar(espelho.current);
    setItens(espelho.current);
  }, []);

  const trocar = useCallback(
    (chave: string, mudanca: Partial<ItemDeEnvio>) => {
      aplicar((atuais) => atuais.map((i) => (i.chave === chave ? { ...i, ...mudanca } : i)));
    },
    [aplicar],
  );

  // Um arquivo por vez, de propósito: numa rede de celular, quatro uploads
  // simultâneos terminam todos mais devagar e nenhum consegue dizer em que pé
  // está. Em série, sempre há um "enviando" e uma posição na fila.
  const escoarFila = useCallback(async () => {
    if (escoando.current) return;
    escoando.current = true;
    try {
      for (;;) {
        const item = espelho.current.find((i) => i.etapa === "aguardando");
        if (!item) break;

        trocar(item.chave, { etapa: "enviando", mensagem: null });
        try {
          const formulario = new FormData();
          formulario.append("file", item.arquivo);
          const documento = await uploadDocument(formulario);
          trocar(item.chave, {
            etapa: "recebido",
            documentoId: documento.id,
            mensagem: "Guardamos o arquivo original.",
          });
          aoReceber();
        } catch (erro) {
          trocar(item.chave, { etapa: "falhou", ...interpretarFalha(erro) });
        }
      }
    } finally {
      escoando.current = false;
    }
  }, [aoReceber, trocar]);

  const acrescentar = useCallback(
    (arquivos: FileList | File[] | null) => {
      const lista = Array.from(arquivos ?? []);
      if (lista.length === 0) return;

      const novos: ItemDeEnvio[] = lista.map((arquivo) => {
        // A recusa acontece aqui, antes de qualquer byte subir. Recusar depois
        // deixaria a pessoa esperando um envio que o servidor já ia negar.
        const aceito = uploadAceito(arquivo.name, arquivo.type);
        return {
          chave: `${arquivo.name}-${arquivo.size}-${arquivo.lastModified}-${chaveAleatoria()}`,
          arquivo,
          etapa: aceito ? "aguardando" : "recusado",
          mensagem: aceito ? null : `Aceitamos ${FORMATOS_EM_TEXTO}.`,
          documentoId: null,
          duplicadoDe: null,
        };
      });

      aplicar((atuais) => [...novos, ...atuais]);
      if (novos.some((i) => i.etapa === "aguardando")) void escoarFila();
    },
    [aplicar, escoarFila],
  );

  // Assim que o documento aparece na caixa de entrada, o item local cumpriu o
  // papel: a linha de verdade, com o estado de verdade, já está na lista.
  useEffect(() => {
    const chegaram = itens.filter(
      (i) => i.etapa === "recebido" && i.documentoId && idsNaLista.has(i.documentoId),
    );
    if (chegaram.length === 0) return;

    for (const item of chegaram) {
      trocar(item.chave, {
        etapa: "processando",
        mensagem: "Está na fila de leitura — acompanhe na lista abaixo.",
      });
      relogios.current.push(
        setTimeout(() => {
          aplicar((atuais) => atuais.filter((i) => i.chave !== item.chave));
        }, ESPERA_ANTES_DE_SUMIR),
      );
    }
  }, [itens, idsNaLista, trocar, aplicar]);

  const tentarDeNovo = useCallback(
    (chave: string) => {
      trocar(chave, { etapa: "aguardando", mensagem: null, duplicadoDe: null });
      void escoarFila();
    },
    [escoarFila, trocar],
  );

  const dispensar = useCallback(
    (chave: string) => {
      aplicar((atuais) => atuais.filter((i) => i.chave !== chave));
    },
    [aplicar],
  );

  const naLeva = itens.filter((i) => NA_LEVA.has(i.etapa));
  const restantes = naLeva.filter((i) => i.etapa === "aguardando" || i.etapa === "enviando").length;

  const abrirCamera = () => entradaDeCamera.current?.click();
  const abrirSeletor = () => entradaDeArquivo.current?.click();

  return (
    <section style={{ display: "grid", gap: 12 }}>
      <input
        ref={entradaDeArquivo}
        type="file"
        multiple
        accept={ACEITA_ARQUIVO}
        className={styles.entradaEscondida}
        onChange={(e) => {
          acrescentar(e.target.files);
          e.target.value = "";
        }}
      />
      {/* `capture="environment"` abre a câmera traseira direto — sem galeria,
          sem seletor. É o caminho de "fotografar o comprovante agora". */}
      <input
        ref={entradaDeCamera}
        type="file"
        accept={ACEITA_CAMERA}
        capture="environment"
        className={styles.entradaEscondida}
        onChange={(e) => {
          acrescentar(e.target.files);
          e.target.value = "";
        }}
      />

      {/* Arrastar é caminho de desktop; no celular os dois botões dão conta.
          A zona de solta é a div interna para o Card não virar botão — quem
          clica tem alvos explícitos, com rótulo. */}
      <Card
        padding={16}
        style={{
          gap: 12,
          borderStyle: arrastando ? "dashed" : "solid",
          borderColor: arrastando ? "var(--primary)" : "var(--line)",
        }}
      >
        <div
          onDragOver={(e) => {
            e.preventDefault();
            setArrastando(true);
          }}
          onDragLeave={() => setArrastando(false)}
          onDrop={(e) => {
            e.preventDefault();
            setArrastando(false);
            acrescentar(e.dataTransfer.files);
          }}
          style={{ display: "grid", gap: 12 }}
        >
          <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
            <span
              style={{
                width: 36,
                height: 36,
                flex: "0 0 36px",
                borderRadius: "var(--sbf-radius-md)",
                background: "var(--primary-soft)",
                color: "var(--primary-ink)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon name="photo_camera" size={20} />
            </span>
            <div style={{ minWidth: 0 }}>
              <div style={{ fontSize: 14.5, fontWeight: 700, letterSpacing: "-.01em" }}>
                Registrar um comprovante
              </div>
              <div
                style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 2, lineHeight: 1.45 }}
              >
                Fotografe ou anexe. O arquivo original fica guardado do jeito que chegou, mesmo se a
                leitura automática falhar.
              </div>
            </div>
          </div>

          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <Button
              variant="primary"
              size="lg"
              icon="photo_camera"
              onClick={abrirCamera}
              style={{ flex: "1 1 150px" }}
            >
              Fotografar
            </Button>
            <Button
              variant="outline"
              size="lg"
              icon="attach_file"
              onClick={abrirSeletor}
              style={{ flex: "1 1 150px" }}
            >
              Anexar arquivo
            </Button>
          </div>

          <div style={{ fontSize: 11.5, color: "var(--text-4)" }}>
            Aceitamos {FORMATOS_EM_TEXTO}. Pode mandar vários de uma vez.
          </div>
        </div>
      </Card>

      {restantes > 0 && naLeva.length > 1 ? (
        <ProgressBar
          value={(naLeva.length - restantes) / naLeva.length}
          label={`Enviando ${naLeva.length - restantes + 1} de ${naLeva.length}`}
          hint="Um arquivo por vez"
        />
      ) : null}

      {itens.length > 0 ? (
        <Card variant="flush">
          {itens.map((item) => (
            <LinhaDeEnvio
              key={item.chave}
              item={item}
              aoTentarDeNovo={() => tentarDeNovo(item.chave)}
              aoDispensar={() => dispensar(item.chave)}
              aoAbrirDocumento={aoAbrirDocumento}
            />
          ))}
        </Card>
      ) : null}

      {/* O atalho que continua ao alcance do polegar depois de rolar a lista. */}
      <div className={styles.acaoFlutuante}>
        <button
          type="button"
          aria-label="Fotografar um comprovante"
          onClick={abrirCamera}
          style={{
            width: 56,
            height: 56,
            borderRadius: "var(--sbf-radius-pill)",
            border: "1px solid var(--line)",
            background: "var(--surface)",
            color: "var(--primary)",
            boxShadow: "0 10px 26px -12px rgba(24,35,31,.75)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
          }}
        >
          <Icon name="photo_camera" size={26} />
        </button>
      </div>
    </section>
  );
}

function LinhaDeEnvio({
  item,
  aoTentarDeNovo,
  aoDispensar,
  aoAbrirDocumento,
}: {
  item: ItemDeEnvio;
  aoTentarDeNovo: () => void;
  aoDispensar: () => void;
  aoAbrirDocumento: (id: string) => void;
}) {
  const rotulo = ROTULO_DA_ETAPA[item.etapa];
  const problema = item.etapa === "falhou" || item.etapa === "recusado";
  const tamanho = formatarTamanho(item.arquivo.size);
  const jaExiste = item.duplicadoDe;

  return (
    <div
      style={{
        display: "flex",
        gap: 12,
        alignItems: "flex-start",
        padding: "13px 16px",
        borderTop: "1px solid var(--line-soft)",
        background: "var(--surface)",
        fontFamily: "var(--sbf-font-sans)",
      }}
    >
      <Icon
        name={rotulo.icone}
        size={20}
        style={{ color: problema ? "var(--danger)" : "var(--text-3)", marginTop: 2 }}
      />

      <div style={{ flex: "1 1 auto", minWidth: 0 }}>
        <div
          style={{
            fontSize: 13.5,
            fontWeight: 700,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {item.arquivo.name}
        </div>
        <div style={{ fontSize: 12, color: "var(--text-3)", marginTop: 2, lineHeight: 1.45 }}>
          {[tamanho, item.mensagem].filter(Boolean).join(" · ")}
        </div>

        {problema ? (
          <div style={{ display: "flex", gap: 8, marginTop: 10, flexWrap: "wrap" }}>
            {jaExiste ? (
              <Button
                variant="soft"
                size="md"
                icon="open_in_new"
                onClick={() => aoAbrirDocumento(jaExiste)}
              >
                Abrir o que já existe
              </Button>
            ) : null}
            {item.etapa === "falhou" ? (
              <Button variant="soft" size="md" icon="refresh" onClick={aoTentarDeNovo}>
                Tentar de novo
              </Button>
            ) : null}
            <Button variant="outline" size="md" onClick={aoDispensar}>
              Descartar
            </Button>
          </div>
        ) : null}
      </div>

      <Pill
        tone={problema ? "danger" : item.etapa === "recebido" ? "primary" : "neutral"}
        size="xs"
      >
        {rotulo.texto}
      </Pill>
    </div>
  );
}

function chaveAleatoria(): string {
  return Math.random().toString(36).slice(2, 10);
}

/**
 * Erro do servidor → frase útil.
 *
 * O caso do arquivo duplicado é o único que o servidor descreve com um id
 * dentro da mensagem; extraí-lo permite oferecer "abrir o que já existe" em vez
 * de deixar a pessoa procurar. Se a mensagem mudar de forma, a regex falha e o
 * texto padrão continua correto — nada quebra.
 */
function interpretarFalha(erro: unknown): { mensagem: string; duplicadoDe: string | null } {
  const bruto = erro instanceof Error ? erro.message : String(erro);

  if (/duplicad/i.test(bruto)) {
    const id = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i.exec(bruto)?.[0];
    return {
      mensagem: "Este arquivo já tinha sido enviado antes — o original está guardado.",
      duplicadoDe: id ?? null,
    };
  }

  return {
    mensagem: `${bruto} O arquivo continua aqui: dá para tentar de novo.`,
    duplicadoDe: null,
  };
}
