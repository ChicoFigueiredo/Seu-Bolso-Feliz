/**
 * Cliente REST de baixo nível para a API da Pluggy. Sem lógica de domínio —
 * só autenticação (cache de apiKey por ~2h) e chamadas tipadas. A normalização
 * pra `NormalizedTransaction`/`FinancialDataProvider` vive em `pluggy-provider.ts`.
 */
import type {
  PluggyConnectorResumo,
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

  /**
   * Conectores disponíveis no país, com a marcação de Open Finance.
   *
   * Existe para o widget poder esconder os conectores que pedem a senha do
   * internet banking. Não é preferência estética: com Open Finance o usuário
   * autoriza no site do próprio banco e nenhuma senha passa por aqui, que é a
   * razão de o Open Finance existir. Na Pluggy os dois convivem na mesma
   * lista, às vezes com nomes idênticos a menos de um acento — "Caixa
   * Economica Federal" pede senha, "Caixa Econômica Federal" não.
   */
  async listConnectors(country = "BR"): Promise<PluggyConnectorResumo[]> {
    const dados = await this.request<{ results?: PluggyConnectorResumo[] }>(
      `/connectors?countries=${encodeURIComponent(country)}`,
    );
    return dados.results ?? [];
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
   * Transações de uma conta, pela `/v2/transactions`.
   *
   * A `/transactions` (v1) foi descontinuada e responde 410
   * `ENDPOINT_DEPRECATED` mandando usar esta. A v2 muda três coisas ao mesmo
   * tempo, e as três quebram em silêncio se passarem batidas:
   *
   *   - os filtros de data chamam `dateFrom`/`dateTo`, não `from`/`to`;
   *   - `page` e `pageSize` deixaram de existir — a v2 recusa a requisição
   *     inteira com 400 se algum deles aparecer;
   *   - a paginação é por `after`, e o campo `next` da resposta **não** é o
   *     valor de `after`: é a query string inteira da próxima chamada.
   */
  async listTransactions(
    accountId: string,
    options?: { from?: string; to?: string; cursor?: string },
  ): Promise<PluggyTransactionsPage> {
    const params = new URLSearchParams({ accountId });
    if (options?.from) params.set("dateFrom", options.from);
    if (options?.to) params.set("dateTo", options.to);
    const after = extrairCursor(options?.cursor);
    if (after) params.set("after", after);

    const pagina = await this.request<PluggyTransactionsPage>(
      `/v2/transactions?${params.toString()}`,
    );
    return { results: pagina.results, next: extrairCursor(pagina.next) ?? null };
  }
}

/**
 * Reduz o `next` da v2 ao token de paginação.
 *
 * A v2 devolve a próxima página como uma query string pronta —
 * `?accountId=…&dateFrom=…&dateTo=…&after=<token>` — e só o `after` é cursor.
 * Guardamos apenas ele, por dois motivos:
 *
 *   - o `accountId` vai embutido no `next`. Como o checkpoint guarda o cursor
 *     por conta, seguir a query inteira faria um cursor gravado para a conta A
 *     e reaplicado na conta B importar as transações de A. O filtro tem que
 *     vir sempre de quem chama, nunca do cursor;
 *   - mandar a query string no lugar do token devolve 400, e o valor errado
 *     fica **gravado** no checkpoint — toda execução seguinte repete o mesmo
 *     erro. Por isso a normalização vale também na entrada: um cursor antigo,
 *     salvo no formato errado, volta a funcionar sozinho.
 *
 * `?` e `&` não existem no alfabeto do base64, então servem para distinguir a
 * query string de um token que já veio limpo.
 */
export function extrairCursor(next: string | null | undefined): string | undefined {
  if (!next) return undefined;

  const ehQueryString =
    next.startsWith("?") || next.startsWith("after=") || next.includes("&after=");
  if (!ehQueryString) return next;

  const query = next.startsWith("?") ? next.slice(1) : next;
  return new URLSearchParams(query).get("after") ?? undefined;
}

async function safeJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return undefined;
  }
}
