/**
 * Contrato genérico para qualquer fonte de dados financeiros externa
 * (Open Finance ou não). `PluggyProvider` é o primeiro implementador —
 * ver `./pluggy/pluggy-provider.ts`.
 *
 * `amountCents` é sempre uma magnitude não-negativa; a direção do dinheiro
 * vive em `direction`. Espelha a convenção de `@sbf/contracts`
 * (`amount_cents` + `direction` como campos separados, nunca um valor com
 * sinal) — ver `packages/contracts/src/draft/transaction.ts`.
 */

export type NormalizedTransactionStatus = "pending" | "posted";
export type NormalizedTransactionDirection = "inflow" | "outflow";

export interface NormalizedTransaction {
  externalTransactionId: string;
  externalAccountId: string;
  direction: NormalizedTransactionDirection;
  amountCents: number;
  currency: string;
  /** Data em que o banco lançou a transação, YYYY-MM-DD. */
  date: string;
  description: string;
  descriptionRaw?: string;
  status: NormalizedTransactionStatus;
  category?: string;
  merchantName?: string;
  merchantTaxId?: string;
  /** Payload original do provedor, para auditoria/debug — nunca usado no draft. */
  raw: Record<string, unknown>;
}

export interface NormalizedAccount {
  externalAccountId: string;
  externalItemId: string;
  name: string;
  type: string;
  subtype?: string;
  currency: string;
  balanceCents?: number;
  raw: Record<string, unknown>;
}

export interface NormalizedConnection {
  externalItemId: string;
  institutionName?: string;
  status: string;
  raw: Record<string, unknown>;
}

export interface ListTransactionsOptions {
  from?: string;
  to?: string;
  cursor?: string;
}

export interface ListTransactionsResult {
  transactions: NormalizedTransaction[];
  nextCursor?: string;
}

export interface CreateConnectTokenOptions {
  webhookUrl?: string;
  clientUserId?: string;
  /** Reautenticar um item existente em vez de criar um novo. */
  itemId?: string;
}

export interface ConnectTokenResult {
  connectToken: string;
}

/**
 * Qualquer fonte de dados bancários que o app possa consumir. Uma
 * implementação nunca escreve em `draft_records`/`transactions` diretamente —
 * isso é responsabilidade do código que consome `FinancialDataProvider`
 * (ver `./pluggy/map-to-draft.ts`), mantendo o provider livre de saber o
 * que é `draft_records`.
 */
export interface FinancialDataProvider {
  readonly providerName: string;

  /** Token de vida curta para o widget de conexão do frontend. */
  createConnectToken(options?: CreateConnectTokenOptions): Promise<ConnectTokenResult>;

  getConnection(externalItemId: string): Promise<NormalizedConnection>;

  listAccounts(externalItemId: string): Promise<NormalizedAccount[]>;

  listTransactions(
    externalAccountId: string,
    options?: ListTransactionsOptions,
  ): Promise<ListTransactionsResult>;
}
