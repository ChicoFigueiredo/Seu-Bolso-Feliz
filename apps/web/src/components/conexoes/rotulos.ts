/**
 * A tradução entre o que o banco guarda e o que a tela Conexões diz.
 *
 * Fica separado das telas por um motivo só: o vocabulário de status desta
 * área é o ponto onde é mais fácil mentir sem querer. `revoked` não é
 * "cancelado no banco", `expired` não é "erro", e "sincronizando" não existe
 * como status — é um backfill em andamento. Cada rótulo aqui foi escrito
 * olhando o CHECK da coluna correspondente em packages/db/schema.sql.
 */
import type { SbfTone } from "@sbf/design-system";
import type { Conexao, StatusContaExterna } from "@/app/actions/painel-conexoes";

export interface Aparencia {
  rotulo: string;
  tom: SbfTone;
  icone: string;
}

/**
 * O estado da conexão como o usuário o entende.
 *
 * "Importando histórico" vem antes do status da linha de propósito: uma
 * conexão ativa que está puxando 12 meses de extrato é, para quem olha, uma
 * conexão ocupada — e saber disso explica por que a movimentação ainda não
 * apareceu.
 */
export function aparenciaDaConexao(conexao: Conexao): Aparencia {
  if (conexao.status === "active" && conexao.importandoHistorico) {
    return { rotulo: "Importando histórico", tom: "secondary", icone: "hourglass_top" };
  }
  switch (conexao.status) {
    case "active":
      return { rotulo: "Conectada", tom: "primary", icone: "check_circle" };
    case "expired":
      return { rotulo: "Precisa reautenticar", tom: "warn", icone: "lock_clock" };
    case "error":
      return { rotulo: "Com erro", tom: "danger", icone: "error" };
    case "revoked":
      return { rotulo: "Desconectada", tom: "neutral", icone: "link_off" };
  }
}

/**
 * O que está acontecendo e o que fazer, quando não é só "está tudo bem".
 * `null` quando não há nada a explicar — uma caixa de aviso permanente vira
 * ruído e ensina o usuário a ignorar avisos.
 */
export function explicacaoDoStatus(conexao: Conexao): string | null {
  switch (conexao.status) {
    case "expired":
      return "O banco pediu uma nova autorização. Enquanto isso, nenhuma movimentação nova entra por aqui. Reautenticar leva menos de um minuto e não apaga nada do que já veio.";
    case "error":
      return "A última tentativa de sincronizar falhou. Costuma ser senha trocada ou autorização vencida no banco — reautenticar resolve na maioria das vezes.";
    case "revoked":
      return "Esta conexão está desligada: nada novo entra por ela. As movimentações que já vieram continuam no app. Reconectar traz o mesmo banco de volta com o vínculo das contas preservado.";
    case "active":
      return conexao.importandoHistorico
        ? "O histórico está sendo importado em segundo plano. Pode fechar a tela — as movimentações vão aparecer em Documentos, para revisão, conforme chegam."
        : null;
  }
}

/** `external_account_mappings.status` — os três valores do CHECK, e só eles. */
export function aparenciaDaConta(status: StatusContaExterna): Aparencia {
  switch (status) {
    case "mapped":
      return { rotulo: "Vinculada", tom: "primary", icone: "link" };
    case "ignored":
      return { rotulo: "Não importada", tom: "neutral", icone: "block" };
    case "pending_mapping":
      return { rotulo: "Aguardando você", tom: "warn", icone: "help" };
  }
}

/** Tipos de `financial_products.type` (CHECK em packages/db/schema.sql). */
const ROTULO_TIPO_PRODUTO: Record<string, string> = {
  checking_account: "Conta corrente",
  savings_account: "Poupança",
  credit_card: "Cartão de crédito",
  overdraft: "Cheque especial",
  personal_loan: "Empréstimo",
  mortgage: "Financiamento",
  investment: "Investimento",
  other: "Outro",
};

export function rotuloTipoProduto(tipo: string): string {
  return ROTULO_TIPO_PRODUTO[tipo] ?? tipo;
}

/**
 * `external_account_type` guarda o `type` cru da Pluggy (BANK, CREDIT).
 * Valor desconhecido volta como veio: inventar tradução para um código que
 * não conhecemos é pior do que mostrar o código.
 */
const ROTULO_TIPO_EXTERNO: Record<string, string> = {
  BANK: "Conta bancária",
  CREDIT: "Cartão de crédito",
  CHECKING_ACCOUNT: "Conta corrente",
  SAVINGS_ACCOUNT: "Poupança",
  CREDIT_CARD: "Cartão de crédito",
};

export function rotuloTipoExterno(tipo: string | null): string | null {
  if (!tipo) return null;
  return ROTULO_TIPO_EXTERNO[tipo.toUpperCase()] ?? tipo;
}

/**
 * Fuso fixo, e não o do ambiente, de propósito.
 *
 * "use client" não quer dizer "só no cliente": o componente também é renderizado
 * no servidor para o HTML da primeira resposta. Se esta função lesse o fuso do
 * ambiente, o servidor (UTC, na Vercel) escreveria "hoje, 11:12" e o navegador
 * "hoje, 08:12" — divergência de hidratação, e o usuário podendo ficar com o
 * horário do datacenter na tela. Com o fuso fixo os dois lados escrevem a mesma
 * coisa, e a coisa certa: o produto é de um usuário só, no Brasil (ADR-007).
 */
const FUSO_DO_USUARIO = "America/Sao_Paulo";

/** Data civil (aaaa-mm-dd) e hora (hh:mm) do instante, no fuso do usuário. */
function civil(data: Date): { dia: string; hora: string } {
  const partes = new Intl.DateTimeFormat("pt-BR", {
    timeZone: FUSO_DO_USUARIO,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(data);
  const parte = (tipo: string) => partes.find((p) => p.type === tipo)?.value ?? "";
  return {
    dia: `${parte("year")}-${parte("month")}-${parte("day")}`,
    hora: `${parte("hour")}:${parte("minute")}`,
  };
}

/** "hoje, 08:12" / "ontem, 22:40" / "24/08/2026, 09:03". */
export function momentoRelativo(iso: string | null): string | null {
  if (!iso) return null;
  const data = new Date(iso);
  if (Number.isNaN(data.getTime())) return null;

  const alvo = civil(data);
  const hoje = civil(new Date());
  const emMs = (dia: string) => Date.parse(`${dia}T00:00:00Z`);
  const diasAtras = Math.round((emMs(hoje.dia) - emMs(alvo.dia)) / 86400000);

  if (diasAtras === 0) return `hoje, ${alvo.hora}`;
  if (diasAtras === 1) return `ontem, ${alvo.hora}`;
  if (diasAtras > 1 && diasAtras < 7) return `há ${diasAtras} dias, ${alvo.hora}`;

  const [ano, mes, dia] = alvo.dia.split("-");
  return `${dia}/${mes}/${ano}, ${alvo.hora}`;
}

export function plural(quantidade: number, singular: string, pluralizado: string): string {
  return `${quantidade} ${quantidade === 1 ? singular : pluralizado}`;
}
