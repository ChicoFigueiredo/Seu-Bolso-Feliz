/**
 * O vocabulário da caixa de entrada de documentos.
 *
 * A máquina de estados da ingestão tem catorze status (`discovered`,
 * `ai_lite_enriching`, `reconciled`, ...) e a tabela `source_documents` tem
 * mais oito. Nada disso é assunto de quem tirou foto de um comprovante. Este
 * módulo é a única tradução entre os dois mundos: entra o que o pipeline
 * gravou, sai o que a pessoa lê — e, quando algo deu errado, **por que** deu e
 * **o que fazer**.
 *
 * Duas regras que valem para toda frase daqui:
 *
 *   1. Nenhum estado é comunicado só por cor ou só por ícone. O texto sempre
 *      diz a mesma coisa (WCAG 2.2 AA, e a razão de `Pill` exigir rótulo).
 *   2. Falha nunca é só "erro". Quem chegou aqui precisa saber se o arquivo
 *      ainda existe, o que impediu a leitura, e qual é o próximo passo. O
 *      arquivo original **nunca** é descartado — dizer isso é parte da
 *      mensagem, não um detalhe técnico.
 *
 * É um módulo puro de propósito: a server action o usa para derivar os dados
 * da tela e o componente o usa para rotular filtros, sem duplicar a regra.
 */
import type { DocumentState } from "@sbf/design-system";

/** Os quatro baldes do filtro da caixa de entrada. */
export type BaldeDeDocumento = "revisar" | "processando" | "concluidos" | "erro";

/** Ordem de exibição: o que depende da pessoa aparece primeiro. */
export const ORDEM_DOS_BALDES: readonly BaldeDeDocumento[] = [
  "revisar",
  "erro",
  "processando",
  "concluidos",
];

export const ROTULO_DO_BALDE: Record<BaldeDeDocumento, string> = {
  revisar: "Precisam de revisão",
  processando: "Processando",
  concluidos: "Concluídos",
  erro: "Com erro",
};

/** O que o pipeline gravou sobre um documento, cru. */
export interface SinaisDoDocumento {
  /** `source_documents.status`. */
  statusDoDocumento: string;
  /** `ingestion_jobs.status` do job mais recente, quando existe. */
  statusDoJob: string | null;
  /** `ingestion_jobs.error_message` do job mais recente. */
  mensagemDeErro: string | null;
  /** `ingestion_jobs.needs_full_ai_review`. */
  precisaDeIaCompleta: boolean;
  /** Rascunhos deste documento que ainda esperam uma decisão. */
  rascunhosPendentes: number;
  /**
   * O diário do pipeline avisou que o arquivo é um PDF protegido que nenhuma
   * senha cadastrada abriu.
   *
   * Existe como sinal separado porque o worker **não** marca o job como falho
   * nesse caso: ele grava uma versão vazia e segue. Sem este aviso, um PDF
   * protegido chega à revisão parecendo um documento comum do qual "nada foi
   * extraído" — e a pessoa não tem como saber que o que falta é a senha.
   */
  avisoDeSenha: boolean;
}

export interface DiagnosticoDoDocumento {
  estado: DocumentState;
  balde: BaldeDeDocumento;
  /** O que aconteceu, numa frase. `null` quando não há nada a explicar. */
  porQue: string | null;
  /** O próximo passo de quem lê. `null` quando não há nada a fazer. */
  oQueFazer: string | null;
  /** Rota do próximo passo, quando ele mora em outra tela. */
  rotaDoQueFazer: string | null;
}

/** Status de `source_documents` que significam "acabou bem". */
const CONCLUIDOS = new Set(["processed", "approved"]);

/**
 * Marcas de senha nas mensagens do pipeline.
 *
 * Casar por texto não é elegante, mas é o único canal disponível: a coluna
 * `parsed_document_versions.is_password_protected` existe e **nunca é
 * escrita** por ninguém. Ver o relatório de defeitos.
 */
const MARCAS_DE_SENHA = ["senha", "password", "protegid", "encrypted"];

export function pareceProblemaDeSenha(mensagem: string | null | undefined): boolean {
  if (!mensagem) return false;
  const texto = mensagem.toLowerCase();
  return MARCAS_DE_SENHA.some((marca) => texto.includes(marca));
}

const SENHA: Omit<DiagnosticoDoDocumento, "porQue"> & { porQue: string } = {
  estado: "protegido-por-senha",
  balde: "erro",
  porQue: "O arquivo é um PDF protegido e nenhuma senha cadastrada conseguiu abri-lo.",
  oQueFazer:
    "Cadastre a senha do PDF e mande reprocessar. O arquivo original continua guardado — nada foi perdido.",
  rotaDoQueFazer: "/dashboard/settings/passwords",
};

/**
 * Traduz o que o pipeline gravou no que a pessoa lê.
 *
 * A ordem das perguntas importa e não é arbitrária: primeiro o que bloqueia
 * (senha, falha), depois o que depende da pessoa (revisão), e só então o que
 * está em andamento ou pronto. Um documento que falhou **e** tem rascunho
 * pendente é, antes de tudo, um documento que falhou.
 */
export function diagnosticarDocumento(sinais: SinaisDoDocumento): DiagnosticoDoDocumento {
  const {
    statusDoDocumento,
    statusDoJob,
    mensagemDeErro,
    precisaDeIaCompleta,
    rascunhosPendentes,
    avisoDeSenha,
  } = sinais;

  const falhou = statusDoDocumento === "failed" || statusDoJob === "failed";

  if (avisoDeSenha || (falhou && pareceProblemaDeSenha(mensagemDeErro))) {
    return { ...SENHA };
  }

  if (falhou) {
    return {
      estado: "nao-reconhecido",
      balde: "erro",
      porQue: mensagemDeErro
        ? `A leitura parou: ${primeiraLinha(mensagemDeErro)}`
        : "A leitura parou e o pipeline não registrou o motivo.",
      oQueFazer:
        "Tente reprocessar. Se falhar de novo, abra o arquivo original — ele continua guardado — e lance à mão.",
      rotaDoQueFazer: null,
    };
  }

  if (rascunhosPendentes > 0) {
    return {
      estado: "precisa-revisar",
      balde: "revisar",
      porQue:
        rascunhosPendentes === 1
          ? "1 lançamento esperando sua decisão."
          : `${rascunhosPendentes} lançamentos esperando sua decisão.`,
      oQueFazer: "Confira o que foi lido e aprove, corrija ou rejeite.",
      rotaDoQueFazer: null,
    };
  }

  if (statusDoDocumento === "pending_review") {
    // Chegou à revisão sem nada pendente para decidir: ou a extração veio
    // vazia, ou os rascunhos já foram resolvidos e o status ficou para trás.
    return {
      estado: "precisa-revisar",
      balde: "revisar",
      porQue: precisaDeIaCompleta
        ? "A leitura automática não teve confiança suficiente para propor um lançamento."
        : "O documento chegou à revisão sem nenhum lançamento proposto.",
      oQueFazer: "Peça a análise completa por IA ou abra o original e lance à mão.",
      rotaDoQueFazer: null,
    };
  }

  if (CONCLUIDOS.has(statusDoDocumento)) {
    return {
      estado: "pronto",
      balde: "concluidos",
      porQue: null,
      oQueFazer: null,
      rotaDoQueFazer: null,
    };
  }

  return {
    estado: "processando",
    balde: "processando",
    porQue:
      statusDoDocumento === "new" || statusDoJob === "discovered"
        ? "Recebido. Entrando na fila."
        : "O sistema está lendo este arquivo.",
    oQueFazer: null,
    rotaDoQueFazer: null,
  };
}

/** Mensagens de erro do pipeline às vezes trazem stack; só a 1ª linha serve. */
function primeiraLinha(texto: string): string {
  const linha = texto.split("\n")[0]?.trim() ?? texto;
  return linha.length > 180 ? `${linha.slice(0, 177)}…` : linha;
}

const ROTULO_DA_ORIGEM: Record<string, string> = {
  gmail: "Gmail",
  local_file: "Pasta local",
  manual_upload: "Enviado por você",
};

/**
 * De onde o documento veio, em uma frase.
 *
 * Câmera, upload, Gmail e pasta local caem na mesma lista de propósito — a
 * origem é contexto, não uma aba separada.
 */
export function rotuloDaOrigem(origem: string, remetente?: string | null): string {
  const base = ROTULO_DA_ORIGEM[origem] ?? origem;
  if (origem === "gmail" && remetente) return `${base} · ${enderecoCurto(remetente)}`;
  return base;
}

/** `"Cemig <fatura@cemig.com.br>"` → `"fatura@cemig.com.br"`. */
function enderecoCurto(remetente: string): string {
  const entreSinais = /<([^>]+)>/.exec(remetente);
  return (entreSinais?.[1] ?? remetente).trim();
}

export function formatarTamanho(bytes: number | null | undefined): string | null {
  if (bytes == null || bytes <= 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** `"2026-08-30T09:41:00Z"` → `"30/08 às 06:41"`. */
export function formatarChegada(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const data = new Date(iso);
  if (Number.isNaN(data.getTime())) return null;
  const dia = data.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit" });
  const hora = data.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
  return `${dia} às ${hora}`;
}

/**
 * Confiança numérica → o vocabulário do `Field`.
 *
 * Os cortes (0,8 e 0,5) são os mesmos que o pipeline usa para decidir se
 * aciona a IA completa; mantê-los iguais evita que a tela diga "alta" sobre um
 * campo que o próprio sistema achou fraco o bastante para reprocessar.
 */
export function faixaDeConfianca(
  valor: number | null | undefined,
): "alta" | "provavel" | "baixa" | null {
  if (valor == null || !Number.isFinite(valor)) return null;
  if (valor >= 0.8) return "alta";
  if (valor >= 0.5) return "provavel";
  return "baixa";
}
