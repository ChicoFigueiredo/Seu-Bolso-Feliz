/**
 * Implementa `FinancialDataProvider` sobre a API da Pluggy.
 * Não sabe nada de `draft_records`/banco de dados — ver `map-to-draft.ts` para
 * o próximo passo (normalizado → `TransactionDraftV1`).
 */
import type {
  ConnectTokenResult,
  CreateConnectTokenOptions,
  FinancialDataProvider,
  ListTransactionsOptions,
  ListTransactionsResult,
  NormalizedAccount,
  NormalizedConnection,
  NormalizedTransaction,
  NormalizedTransactionDirection,
} from "../financial-data-provider";
import { PluggyClient } from "./pluggy-client";
import type { PluggyAccount, PluggyTransaction } from "./types";

export interface PluggyProviderConfig {
  clientId: string;
  clientSecret: string;
  baseUrl?: string;
}

const TRANSACTIONS_PAGE_SIZE = 500;

/**
 * O conector do Meu Pluggy (meu.pluggy.ai).
 *
 * Ele não é marcado como Open Finance na API — é um proxy — mas é por Open
 * Finance que o usuário liga os bancos dele lá dentro, e nenhuma senha de
 * banco passa por aqui. Precisa estar na lista à parte porque o filtro de
 * `isOpenFinance` o esconderia, e ele é justamente o caminho gratuito para uso
 * pessoal: a conexão direta pelo Dashboard exige plano pago.
 */
const CONECTOR_MEU_PLUGGY = 200;

export class PluggyProvider implements FinancialDataProvider {
  readonly providerName = "pluggy" as const;
  private readonly client: PluggyClient;

  constructor(config: PluggyProviderConfig) {
    this.client = new PluggyClient(config);
  }

  /** `PLUGGY_CLIENT_ID`/`PLUGGY_CLIENT_SECRET` — bloqueio do CEO, ver ADR-008. */
  static fromEnv(): PluggyProvider {
    const clientId = process.env.PLUGGY_CLIENT_ID;
    const clientSecret = process.env.PLUGGY_CLIENT_SECRET;
    if (!clientId || !clientSecret) {
      throw new Error(
        "PLUGGY_CLIENT_ID/PLUGGY_CLIENT_SECRET não configurados. Criar app em Meu Pluggy " +
          "(bloqueio do CEO, ver ADR-008 em docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md).",
      );
    }
    return new PluggyProvider({ clientId, clientSecret });
  }

  async createConnectToken(options?: CreateConnectTokenOptions): Promise<ConnectTokenResult> {
    const data = await this.client.createConnectToken({
      itemId: options?.itemId,
      clientUserId: options?.clientUserId,
      webhookUrl: options?.webhookUrl,
    });
    return { connectToken: data.accessToken };
  }

  /**
   * Ids dos conectores que falam Open Finance — os que levam o usuário a
   * autorizar no site do próprio banco, sem senha nenhuma passando por aqui.
   *
   * A Pluggy lista os dois mundos juntos, e para vários bancos o conector
   * antigo (usuário + senha do internet banking) aparece ao lado do de Open
   * Finance com um nome quase idêntico. Deixar a escolha para o usuário é
   * pedir que ele erre; e o erro custa a senha do banco dele.
   */
  async listOpenFinanceConnectorIds(country = "BR"): Promise<number[]> {
    const conectores = await this.client.listConnectors(country);
    return conectores
      .filter((c) => (c.isOpenFinance || c.id === CONECTOR_MEU_PLUGGY) && !c.isSandbox)
      .map((c) => c.id);
  }

  async getConnection(externalItemId: string): Promise<NormalizedConnection> {
    const item = await this.client.getItem(externalItemId);
    return {
      externalItemId: item.id,
      institutionName: item.connector?.name,
      status: item.status,
      raw: item as unknown as Record<string, unknown>,
    };
  }

  async listAccounts(externalItemId: string): Promise<NormalizedAccount[]> {
    const accounts = await this.client.listAccounts(externalItemId);
    return accounts.map(normalizeAccount);
  }

  async listTransactions(
    externalAccountId: string,
    options?: ListTransactionsOptions,
  ): Promise<ListTransactionsResult> {
    const page = options?.cursor ? Number(options.cursor) : 1;
    const data = await this.client.listTransactions(externalAccountId, {
      from: options?.from,
      to: options?.to,
      page,
      pageSize: TRANSACTIONS_PAGE_SIZE,
    });

    const hasMore = data.totalPages !== undefined && page < data.totalPages;
    return {
      transactions: data.results.map(normalizeTransaction),
      nextCursor: hasMore ? String(page + 1) : undefined,
    };
  }
}

export function normalizeAccount(account: PluggyAccount): NormalizedAccount {
  return {
    externalAccountId: account.id,
    externalItemId: account.itemId,
    name: account.name,
    type: account.type,
    subtype: account.subtype,
    currency: account.currencyCode,
    balanceCents: Math.round(account.balance * 100),
    raw: account as unknown as Record<string, unknown>,
  };
}

/**
 * `type` (DEBIT/CREDIT) é a fonte de verdade da direção — não o sinal de
 * `amount`, que a Pluggy documenta como invertido para cartão de crédito
 * (débito positivo, crédito/pagamento negativo). `amountCents` sai sempre
 * como magnitude positiva.
 */
export function normalizeTransaction(tx: PluggyTransaction): NormalizedTransaction {
  const direction: NormalizedTransactionDirection = tx.type === "CREDIT" ? "inflow" : "outflow";
  return {
    externalTransactionId: tx.id,
    externalAccountId: tx.accountId,
    direction,
    amountCents: Math.round(Math.abs(tx.amount) * 100),
    currency: tx.currencyCode,
    date: tx.date.slice(0, 10),
    description: tx.description,
    descriptionRaw: tx.descriptionRaw,
    status: tx.status === "POSTED" ? "posted" : "pending",
    category: tx.category ?? undefined,
    merchantName: tx.merchant?.businessName ?? tx.merchant?.name,
    merchantTaxId: tx.merchant?.cnpj,
    raw: tx as unknown as Record<string, unknown>,
  };
}
