/**
 * Aritmética do formulário de rateio.
 *
 * Tudo aqui acontece em **centavos inteiros**. Somar reais em ponto flutuante
 * perde centavo de forma silenciosa (`0.1 + 0.2 === 0.30000000000000004`,
 * `0.29 * 100 === 28.999999999999996`), e o rateio é exatamente o lugar onde
 * um centavo perdido vira "o Edulingo gastou R$ 0,01 a mais do que gastou".
 *
 * ⚠️ Duplicação consciente de `toCents` (packages/domain/src/allocation/cents.ts).
 * O `@sbf/domain` só expõe a raiz do pacote no `exports` do package.json, e
 * essa raiz arrasta `financial-intent/identity-key.ts`, que importa
 * `node:crypto` — o que quebra o bundle do cliente. Abrir um subpath
 * `@sbf/domain/allocation` exigiria editar um `package.json`, fora do escopo
 * desta tarefa (ver relatório). O algoritmo de arredondamento aqui é
 * **idêntico** ao do domínio de propósito: se os dois divergirem, o formulário
 * acusa excesso num rateio que o servidor considera perfeito.
 *
 * O que este módulo faz e o domínio não faz: ler o texto que o usuário digita
 * (`"R$ 1.234,56"`), que é problema de interface, não de domínio.
 */

/** Maior valor absoluto, em centavos, que cabe em `numeric(14,2)`. */
const MAX_CENTAVOS = 99_999_999_999_999;

export interface LinhaRateio {
  /** Centro que recebe a fatia. Vazio = linha ainda sem centro escolhido. */
  centroId: string;
  /** O que o usuário digitou, em reais. Texto cru, não número. */
  valorTexto: string;
}

/**
 * Por que uma linha não pode ser gravada.
 *
 * - `sem_centro` — tem valor, mas ninguém para receber.
 * - `centro_repetido` — o mesmo centro duas vezes; o banco tem unicidade
 *   `(transaction_id, financial_center_id)` e recusaria.
 * - `valor_invalido` — o texto não é um número.
 * - `valor_nao_positivo` — zero ou negativo. O sinal vem da movimentação,
 *   nunca do rateio (`check (amount > 0)` na tabela).
 */
export type ProblemaLinha =
  "sem_centro" | "centro_repetido" | "valor_invalido" | "valor_nao_positivo";

export interface ResumoRateio {
  /** Valor da movimentação em centavos, sempre positivo. */
  totalCentavos: number;
  /** Soma das linhas aproveitáveis, em centavos. */
  alocadoCentavos: number;
  /** Total menos alocado. Negativo quando a soma passou do valor. */
  naoAlocadoCentavos: number;
  /** A soma passou do valor da movimentação — o banco recusaria. */
  excedeu: boolean;
  /** Um problema por linha, na mesma ordem das linhas. `null` = linha ok. */
  problemas: (ProblemaLinha | null)[];
  /** Quantas linhas realmente viram rateio (linhas em branco não contam). */
  linhasConsideradas: number;
  /** Nenhum problema e nenhum excesso: dá para gravar. */
  podeSalvar: boolean;
}

/** Rateio pronto para `allocateTransaction`, em reais. */
export interface AlocacaoParaGravar {
  financialCenterId: string;
  amount: number;
}

/**
 * Converte reais em centavos inteiros, arredondando meio centavo para longe
 * do zero.
 *
 * O arredondamento acontece sobre a representação decimal do produto
 * (`toPrecision(15)`), não sobre o produto cru: `8.165 * 100` vale
 * `816.4999999999999` em IEEE-754, e um `Math.round` ingênuo devolveria 816
 * em vez de 817.
 */
function numeroParaCentavos(valor: number): number | null {
  if (!Number.isFinite(valor)) return null;
  const escalado = Number((valor * 100).toPrecision(15));
  const arredondado = Math.sign(escalado) * Math.round(Math.abs(escalado));
  if (Math.abs(arredondado) > MAX_CENTAVOS) return null;
  // Normaliza -0 para 0: `Object.is(-0, 0)` é falso e vazaria para a interface.
  return arredondado === 0 ? 0 : arredondado;
}

/**
 * Lê o texto de um campo de dinheiro em pt-BR e devolve centavos inteiros.
 * Devolve `null` quando o texto não descreve um número.
 *
 * Regras de separador, na ordem:
 * 1. Se há vírgula, ela é o decimal e o ponto é milhar — `1.234,56`.
 * 2. Sem vírgula, o ponto só é milhar quando **todos** os grupos depois dele
 *    têm três dígitos (`1.234`, `1.234.567`). Caso contrário é decimal —
 *    `12.5` e `12.34` vêm do teclado numérico do celular, que não oferece
 *    vírgula, e ler "12.5" como doze mil e quinhentos seria desastroso.
 */
export function paraCentavos(texto: string): number | null {
  const limpo = texto.replace(/\s|R\$/g, "");
  if (limpo === "") return null;
  if (!/^-?[\d.,]+$/.test(limpo)) return null;

  let normalizado: string;
  if (limpo.includes(",")) {
    normalizado = limpo.replace(/\./g, "").replace(",", ".");
  } else if (/^-?\d{1,3}(\.\d{3})+$/.test(limpo)) {
    normalizado = limpo.replace(/\./g, "");
  } else {
    normalizado = limpo;
  }

  // Sobrou mais de um separador decimal ("1,2,3", "1.2.3"): não é número.
  if ((normalizado.match(/\./g)?.length ?? 0) > 1) return null;
  if (!/^-?(\d+(\.\d*)?|\.\d+)$/.test(normalizado)) return null;

  return numeroParaCentavos(Number(normalizado));
}

/**
 * Reais (número, como vem do banco) para centavos inteiros.
 *
 * É o caminho de entrada dos valores que **já estão gravados**: o texto que o
 * usuário digita passa por `paraCentavos`. Os dois arredondam igual.
 */
export function reaisParaCentavos(valor: number): number {
  return numeroParaCentavos(valor) ?? 0;
}

/** Centavos de volta para o texto do campo — `123456` vira `"1234,56"`. */
export function formatarCentavos(centavos: number): string {
  const negativo = centavos < 0;
  const absoluto = Math.abs(Math.trunc(centavos));
  const reais = Math.floor(absoluto / 100);
  const resto = absoluto % 100;
  return `${negativo ? "-" : ""}${String(reais)},${String(resto).padStart(2, "0")}`;
}

/** Centavos de volta para reais, sem passar por texto. */
export function centavosParaReais(centavos: number): number {
  const reais = centavos / 100;
  return reais === 0 ? 0 : reais;
}

/** Linha que o usuário abriu e não preencheu: some do cálculo, sem acusar erro. */
function estaEmBranco(linha: LinhaRateio): boolean {
  return linha.centroId.trim() === "" && linha.valorTexto.trim() === "";
}

/**
 * Estado do formulário de rateio: quanto já foi distribuído, quanto sobra e se
 * dá para gravar.
 *
 * Espelha `validateAllocations` (@sbf/domain), que é quem decide de verdade no
 * servidor — aqui a resposta precisa ser a mesma, só que instantânea e por
 * linha, para o usuário ver o erro enquanto digita em vez de depois de salvar.
 *
 * A soma **pode** ficar abaixo do valor da movimentação: o resto é "não
 * alocado", estado legítimo e visível (CONTEXT.md). Nunca pode ficar acima.
 */
export function resumirRateio(
  valorMovimentacao: number,
  linhas: readonly LinhaRateio[],
): ResumoRateio {
  // Módulo: o sinal é da movimentação, não do rateio. Receita rateia igual a
  // despesa.
  const totalCentavos = Math.abs(numeroParaCentavos(valorMovimentacao) ?? 0);

  const problemas: (ProblemaLinha | null)[] = [];
  const centrosVistos = new Set<string>();
  let alocadoCentavos = 0;
  let linhasConsideradas = 0;

  for (const linha of linhas) {
    if (estaEmBranco(linha)) {
      problemas.push(null);
      continue;
    }

    const centroId = linha.centroId.trim();
    if (centroId === "") {
      problemas.push("sem_centro");
      continue;
    }
    if (centrosVistos.has(centroId)) {
      problemas.push("centro_repetido");
      continue;
    }

    const centavos = paraCentavos(linha.valorTexto);
    if (linha.valorTexto.trim() === "" || centavos === 0) {
      problemas.push("valor_nao_positivo");
      centrosVistos.add(centroId);
      continue;
    }
    if (centavos === null) {
      problemas.push("valor_invalido");
      centrosVistos.add(centroId);
      continue;
    }
    if (centavos < 0) {
      problemas.push("valor_nao_positivo");
      centrosVistos.add(centroId);
      continue;
    }

    centrosVistos.add(centroId);
    alocadoCentavos += centavos;
    linhasConsideradas += 1;
    problemas.push(null);
  }

  const naoAlocadoCentavos = totalCentavos - alocadoCentavos;
  const excedeu = alocadoCentavos > totalCentavos;

  return {
    totalCentavos,
    alocadoCentavos,
    naoAlocadoCentavos,
    excedeu,
    problemas,
    linhasConsideradas,
    podeSalvar: !excedeu && problemas.every((problema) => problema === null),
  };
}

/**
 * Converte as linhas do formulário no payload de `allocateTransaction`.
 *
 * Passa por centavos antes de voltar para reais, então o número enviado é
 * exatamente o que o resumo somou — nunca o `Number()` cru do que foi
 * digitado. Linhas em branco, sem centro ou sem valor válido ficam de fora;
 * quem impede o envio nesse estado é `podeSalvar`, não esta função.
 */
export function paraAlocacoes(linhas: readonly LinhaRateio[]): AlocacaoParaGravar[] {
  const alocacoes: AlocacaoParaGravar[] = [];
  for (const linha of linhas) {
    const centroId = linha.centroId.trim();
    if (centroId === "") continue;
    const centavos = paraCentavos(linha.valorTexto);
    if (centavos === null || centavos <= 0) continue;
    alocacoes.push({ financialCenterId: centroId, amount: centavosParaReais(centavos) });
  }
  return alocacoes;
}
