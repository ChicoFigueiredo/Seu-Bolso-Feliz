/**
 * Quantas linhas pedir ao banco, e o que a tela pode oferecer depois.
 *
 * Função pura num módulo próprio porque a versão anterior desta decisão vivia
 * embutida na server action e estava errada sem que nada percebesse: o
 * servidor limitava a 600 linhas, mas o botão "Carregar mais" aparecia
 * enquanto `total > carregadas`. Ao chegar em 600 o botão continuava lá e
 * não carregava mais nada — clique morto, e o usuário concluindo que o app
 * tinha perdido o histórico dele.
 */

/**
 * Teto de linhas por requisição.
 *
 * Existe para o navegador não montar dezenas de milhares de nós de uma vez.
 * É alto de propósito: um ano de extrato bancário passa folgadamente de mil
 * linhas (a conta real que motivou o número tem 2858 em 12 meses), e um teto
 * baixo transforma "ver tudo" em algo inalcançável.
 */
export const LIMITE_MAXIMO = 5000;

export interface DecisaoPaginacao {
  /** Quantas linhas a consulta deve trazer. */
  limite: number;
  /** Há mais linhas E dá para carregá-las: mostra "Carregar mais". */
  temMais: boolean;
  /** Há mais linhas mas o teto foi atingido: a tela pede um filtro. */
  atingiuTeto: boolean;
}

export function limiteDaConsulta(limitePedido: number, tamanhoPagina: number): number {
  return Math.max(tamanhoPagina, Math.min(limitePedido, LIMITE_MAXIMO));
}

export function decidirPaginacao(
  total: number,
  carregadas: number,
): Omit<DecisaoPaginacao, "limite"> {
  const faltam = total > carregadas;
  return {
    temMais: faltam && carregadas < LIMITE_MAXIMO,
    atingiuTeto: faltam && carregadas >= LIMITE_MAXIMO,
  };
}
