/**
 * Nome de exibição de uma conta externa.
 *
 * O caso que motivou isto: a Caixa devolve 31 subcontas TODAS chamadas
 * "CAIXA". Sem o número, o usuário vê 31 cartões idênticos na tela Hoje e
 * nenhuma forma de saber qual é qual. Pior: existe corrente e poupança com o
 * MESMO número (poupança vinculada — ex.: 593889552-5), então nem o número
 * sozinho basta — o subtipo entra no nome.
 *
 * O nome resultante vira `financial_products.name` na criação automática do
 * produto, e dali em diante é do usuário: sincronizações NUNCA renomeiam.
 */
import type { NormalizedAccount } from "./financial-data-provider";

export function nomeDeExibicaoDaConta(
  conta: Pick<NormalizedAccount, "name" | "subtype" | "number">,
): string {
  // A Pluggy devolve nomes com padding ("BANDEIRADO<40 espaços>").
  const base = conta.name?.trim() || "Conta";
  const ehPoupanca = conta.subtype?.toUpperCase() === "SAVINGS_ACCOUNT";
  const comSubtipo = ehPoupanca ? `${base} Poupança` : base;
  const numero = conta.number?.trim();
  return numero ? `${comSubtipo} · ${numero}` : comSubtipo;
}
