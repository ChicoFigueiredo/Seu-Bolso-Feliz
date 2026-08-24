/**
 * Formas cruas da API REST da Pluggy (Open Finance).
 * Fonte: https://docs.pluggy.ai (auth, connect-token, item, account,
 * transactions) — pesquisado em 2026-08-24, sem sandbox disponível para
 * verificação ao vivo (PLUGGY_CLIENT_ID/SECRET dependem do CEO, ver ADR-008).
 * Verificar contra uma chamada real ou o OpenAPI spec antes do Gate da Fase 2.
 */

export interface PluggyAuthResponse {
  apiKey: string;
}

export interface PluggyConnectTokenResponse {
  accessToken: string;
}

export interface PluggyConnector {
  id: number;
  name: string;
}

export interface PluggyItem {
  id: string;
  connector: PluggyConnector;
  status: string;
  executionStatus: string;
  lastUpdatedAt: string | null;
  nextAutoSyncAt?: string | null;
  clientUserId?: string | null;
  webhookUrl?: string | null;
}

export interface PluggyAccount {
  id: string;
  itemId: string;
  type: string;
  subtype?: string;
  name: string;
  number?: string;
  balance: number;
  currencyCode: string;
}

export interface PluggyMerchant {
  name?: string;
  businessName?: string;
  cnpj?: string;
  category?: string;
}

/**
 * `type`: DEBIT (saída) | CREDIT (entrada). `status`: PENDING | POSTED.
 * `amount` tem sinal, mas a convenção documentada varia por tipo de conta
 * (cartão de crédito inverte o sentido usual) — por isso o mapeamento usa
 * `type`, nunca o sinal de `amount`, como fonte de direção. Ver
 * ../map-to-draft.ts.
 */
export interface PluggyTransaction {
  id: string;
  accountId: string;
  date: string;
  description: string;
  descriptionRaw?: string;
  amount: number;
  amountInAccountCurrency?: number;
  balance?: number;
  currencyCode: string;
  category: string | null;
  categoryId?: string | null;
  providerCode?: string;
  providerId?: string | null;
  type: "DEBIT" | "CREDIT";
  status: "PENDING" | "POSTED";
  merchant?: PluggyMerchant | null;
}

export interface PluggyTransactionsPage {
  results: PluggyTransaction[];
  total?: number;
  totalPages?: number;
  page?: number;
}
