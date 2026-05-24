/**
 * @sbf/worker-gmail-scanner — Gmail API OAuth2 client
 *
 * Usa refresh_token para obter access_token automaticamente.
 * Expõe métodos para listar mensagens por label e obter mensagem completa.
 */

const GMAIL_API_BASE = "https://gmail.googleapis.com/gmail/v1/users/me";
const GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token";

export interface GmailClientConfig {
  clientId: string;
  clientSecret: string;
  refreshToken: string;
}

interface TokenCache {
  accessToken: string;
  expiresAt: number;
}

export interface GmailMessage {
  id: string;
  threadId: string;
  labelIds: string[];
  snippet: string;
  internalDate: string;
  payload: GmailPayload;
  sizeEstimate: number;
}

export interface GmailPayload {
  mimeType: string;
  headers: GmailHeader[];
  body: GmailBody;
  parts?: GmailPart[];
}

export interface GmailPart {
  partId: string;
  mimeType: string;
  filename: string;
  headers: GmailHeader[];
  body: GmailBody;
  parts?: GmailPart[];
}

export interface GmailHeader {
  name: string;
  value: string;
}

export interface GmailBody {
  attachmentId?: string;
  size: number;
  data?: string;
}

export interface GmailAttachmentData {
  size: number;
  data: string; // base64url encoded
}

export interface GmailLabel {
  id: string;
  name: string;
  type: string;
}

interface GmailListResponse {
  messages?: Array<{ id: string; threadId: string }>;
  nextPageToken?: string;
  resultSizeEstimate: number;
}

export class GmailClient {
  private config: GmailClientConfig;
  private tokenCache: TokenCache | null = null;

  constructor(config: GmailClientConfig) {
    this.config = config;
  }

  /**
   * Obtém access_token válido, renovando automaticamente se expirado.
   */
  private async getAccessToken(): Promise<string> {
    if (this.tokenCache && Date.now() < this.tokenCache.expiresAt - 60_000) {
      return this.tokenCache.accessToken;
    }

    const response = await fetch(GOOGLE_TOKEN_URL, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        client_id: this.config.clientId,
        client_secret: this.config.clientSecret,
        refresh_token: this.config.refreshToken,
        grant_type: "refresh_token",
      }),
    });

    if (!response.ok) {
      const errorBody = await response.text();
      throw new Error(
        `Failed to refresh Gmail access token (HTTP ${response.status}): ${errorBody}`,
      );
    }

    const data = (await response.json()) as {
      access_token: string;
      expires_in: number;
    };

    this.tokenCache = {
      accessToken: data.access_token,
      expiresAt: Date.now() + data.expires_in * 1000,
    };

    return this.tokenCache.accessToken;
  }

  /**
   * Faz request autenticado à Gmail API.
   */
  private async apiRequest<T>(path: string, params?: Record<string, string>): Promise<T> {
    const token = await this.getAccessToken();
    const url = new URL(`${GMAIL_API_BASE}${path}`);
    if (params) {
      for (const [key, value] of Object.entries(params)) {
        url.searchParams.set(key, value);
      }
    }

    const response = await fetch(url.toString(), {
      headers: { Authorization: `Bearer ${token}` },
    });

    if (!response.ok) {
      const errorBody = await response.text();
      throw new Error(`Gmail API error (${response.status}) on ${path}: ${errorBody}`);
    }

    return response.json() as Promise<T>;
  }

  /**
   * Lista labels do Gmail.
   */
  async listLabels(): Promise<GmailLabel[]> {
    const data = await this.apiRequest<{ labels: GmailLabel[] }>("/labels");
    return data.labels ?? [];
  }

  /**
   * Busca o ID de um label pelo nome.
   */
  async findLabelId(labelName: string): Promise<string | null> {
    const labels = await this.listLabels();
    const label = labels.find((l) => l.name.toLowerCase() === labelName.toLowerCase());
    return label?.id ?? null;
  }

  /**
   * Lista mensagens por label com paginação.
   */
  async listMessages(
    labelId: string,
    maxResults: number = 100,
    pageToken?: string,
  ): Promise<GmailListResponse> {
    const params: Record<string, string> = {
      labelIds: labelId,
      maxResults: String(maxResults),
    };
    if (pageToken) {
      params.pageToken = pageToken;
    }
    return this.apiRequest<GmailListResponse>("/messages", params);
  }

  /**
   * Obtém mensagem completa com payload (sem dados de attachment inline).
   */
  async getMessage(messageId: string): Promise<GmailMessage> {
    return this.apiRequest<GmailMessage>(`/messages/${messageId}`, {
      format: "full",
    });
  }

  /**
   * Obtém dados binários de um attachment.
   */
  async getAttachment(messageId: string, attachmentId: string): Promise<GmailAttachmentData> {
    return this.apiRequest<GmailAttachmentData>(
      `/messages/${messageId}/attachments/${attachmentId}`,
    );
  }
}

/**
 * Cria GmailClient a partir de variáveis de ambiente.
 */
export function createGmailClient(): GmailClient {
  const clientId = process.env.GOOGLE_MAIL_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_MAIL_CLIENT_SECRET;
  const refreshToken = process.env.GMAIL_REFRESH_TOKEN;

  if (!clientId) throw new Error("GOOGLE_MAIL_CLIENT_ID is required");
  if (!clientSecret) throw new Error("GOOGLE_MAIL_CLIENT_SECRET is required");
  if (!refreshToken) throw new Error("GMAIL_REFRESH_TOKEN is required");

  return new GmailClient({ clientId, clientSecret, refreshToken });
}
