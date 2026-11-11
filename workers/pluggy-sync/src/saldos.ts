/**
 * Saldo das contas: da Pluggy para `financial_products.current_balance`.
 *
 * Sem isto a tela Hoje mostra R$ 0 em toda conta para sempre — a coluna nasce
 * com default 0 e a importação de transações não a toca (transação é fluxo;
 * saldo é posição, e a posição verdadeira quem sabe é o banco).
 *
 * Regras deliberadas:
 *   - Só contas `BANK` (corrente/poupança). O `balance` de cartão na Pluggy é
 *     o aberto da fatura, não "dinheiro disponível" — escrever isso em
 *     `current_balance` somaria fatura como se fosse saldo em conta. Fatura
 *     vem de `statement_cycles`, outro caminho.
 *   - Saldo negativo entra como veio (cheque especial é posição real).
 *   - Só mapeamentos com produto vinculado e não ignorados.
 */
import type { NormalizedAccount } from "@sbf/financial-connectors";

export interface MapeamentoParaSaldo {
  externalAccountId: string;
  financialProductId: string | null;
  status: string;
}

export interface AtualizacaoDeSaldo {
  financialProductId: string;
  /** Em reais, como a coluna `numeric` guarda (mode: "number"). */
  saldo: number;
}

export function planejarAtualizacaoDeSaldos(
  contas: NormalizedAccount[],
  mapeamentos: MapeamentoParaSaldo[],
): AtualizacaoDeSaldo[] {
  const contaPorId = new Map(contas.map((c) => [c.externalAccountId, c]));
  const atualizacoes: AtualizacaoDeSaldo[] = [];

  for (const m of mapeamentos) {
    if (!m.financialProductId || m.status === "ignored") continue;
    const conta = contaPorId.get(m.externalAccountId);
    if (!conta) continue; // conta saiu da Pluggy — não inventamos saldo
    if (conta.type?.toUpperCase() !== "BANK") continue;
    if (conta.balanceCents == null) continue;

    atualizacoes.push({
      financialProductId: m.financialProductId,
      saldo: conta.balanceCents / 100,
    });
  }

  return atualizacoes;
}
