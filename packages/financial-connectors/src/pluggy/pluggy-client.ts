/**
 * Cliente REST de baixo nível para a API da Pluggy. Sem lógica de domínio —
 * só autenticação (cache de apiKey por ~2h) e chamadas tipadas. A normalização
 * pra `NormalizedTransaction`/`FinancialDataProvider` vive em `pluggy-provider.ts`.
 */
import type {
  PluggyAccount,
  PluggyAuthResponse,
  PluggyConnectTokenResponse,
  PluggyItem,
  PluggyTransactionsPage,
} from "./types";

const DEFAULT_BASE_URL = "https://api.pluggy.ai";
/** Pluggy documenta 2h; renovamos um pouco antes para não expirar em voo. */
const API_KEY_TTL_MS = 110 * 60 * 1000;

export interface PluggyClientConfig {
  clientId: string;
  clientSecret: string;
  baseUrl?: string;
}

export class PluggyApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly body: unknown,
  ) {
    super(message);
    this.name = "PluggyApiError";
  }
}

export class PluggyClient {
  private readonly baseUrl: string;
  private cachedApiKey: { value: string; expiresAt: number } | null = null;

  constructor(private readonly config: PluggyClientConfig) {
    this.baseUrl = config.baseUrl ?? DEFAULT_BASE_URL;
  }

  private async getApiKey(): Promise<string> {
    if (this.cachedApiKey && this.cachedApiKey.expiresAt > Date.now()) {
      return this.cachedApiKey.value;
    }

    const response = await fetch(`${this.baseUrl}/auth`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        clientId: this.config.clientId,
        clientSecret: this.config.clientSecret,
      }),
    });

    if (!response.ok) {
      throw new PluggyApiError(
        `Falha ao autenticar com a Pluggy (${response.status})`,
        response.status,
        await safeJson(response),
      );
    }

    const data = (await response.json()) as PluggyAuthResponse;
    this.cachedApiKey = { value: data.apiKey, expiresAt: Date.now() + API_KEY_TTL_MS };
    return data.apiKey;
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const apiKey = await this.getApiKey();
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        "X-API-KEY": apiKey,
        ...init?.headers,
      },
    });

    if (!response.ok) {
      throw new PluggyApiError(
        `Chamada à Pluggy falhou: ${init?.method ?? "GET"} ${path} (${response.status})`,
        response.status,
        await safeJson(response),
      );
    }

    return (await response.json()) as T;
  }

  async createConnectToken(options?: {
    itemId?: string;
    clientUserId?: string;
    webhookUrl?: string;
    oauthRedirectUri?: string;
  }): Promise<PluggyConnectTokenResponse> {
    return this.request<PluggyConnectTokenResponse>("/connect_token", {
      method: "POST",
      body: JSON.stringify({
        itemId: options?.itemId,
        options: {
          clientUserId: options?.clientUserId,
          webhookUrl: options?.webhookUrl,
          oauthRedirectUri: options?.oauthRedirectUri,
        },
      }),
    });
  }

  async getItem(itemId: string): Promise<PluggyItem> {
    return this.request<PluggyItem>(`/items/${encodeURIComponent(itemId)}`);
  }

  async listAccounts(itemId: string): Promise<PluggyAccount[]> {
    const data = await this.request<{ results: PluggyAccount[] }>(
      `/accounts?itemId=${encodeURIComponent(itemId)}`,
    );
    return data.results;
  }

  /**
   * `page` é 1-based, espelhando a paginação clássica da Pluggy
   * (`{results, total, totalPages, page}`). `FinancialDataProvider` expõe
   * isso como um cursor opaco (string do número da página) — ver
   * `pluggy-provider.ts`.
   */
  async listTransactions(
    accountId: string,
    options?: { from?: string; to?: string; page?: number; pageSize?: number },
  ): Promise<PluggyTransactionsPage> {
    const params = new URLSearchParams({ accountId });
    if (options?.from) params.set("from", options.from);
    if (options?.to) params.set("to", options.to);
    if (options?.page) params.set("page", String(options.page));
    if (options?.pageSize) params.set("pageSize", String(options.pageSize));

    return this.request<PluggyTransactionsPage>(`/transactions?${params.toString()}`);
  }
}

async function safeJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return undefined;
  }
}
