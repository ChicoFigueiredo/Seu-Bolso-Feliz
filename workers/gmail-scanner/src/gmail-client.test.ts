import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { GmailClient } from "./gmail-client";

// ══════════════════════════════════════════════════════════════
// Mocks
// ══════════════════════════════════════════════════════════════

const mockConfig = {
  clientId: "test-client-id",
  clientSecret: "test-client-secret",
  refreshToken: "test-refresh-token",
};

function mockFetchResponse(data: unknown, ok = true, status = 200) {
  return {
    ok,
    status,
    json: () => Promise.resolve(data),
    text: () => Promise.resolve(JSON.stringify(data)),
  } as Response;
}

// ══════════════════════════════════════════════════════════════
// GmailClient — Token refresh
// ══════════════════════════════════════════════════════════════

describe("GmailClient — token refresh", () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    fetchSpy = vi.spyOn(globalThis, "fetch");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("obtém access_token na primeira requisição", async () => {
    const client = new GmailClient(mockConfig);

    // Token refresh call
    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ access_token: "at-123", expires_in: 3600 }));
    // API call (listLabels)
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({ labels: [{ id: "L1", name: "INBOX", type: "system" }] }),
    );

    const labels = await client.listLabels();

    expect(labels).toHaveLength(1);
    expect(labels[0]!.name).toBe("INBOX");

    // Verify token request was made with correct params
    const tokenCall = fetchSpy.mock.calls[0]!;
    expect(tokenCall[0]).toBe("https://oauth2.googleapis.com/token");
    const body = tokenCall[1]!.body as URLSearchParams;
    expect(body.get("client_id")).toBe("test-client-id");
    expect(body.get("grant_type")).toBe("refresh_token");
  });

  it("reutiliza token em cache se não expirou", async () => {
    const client = new GmailClient(mockConfig);

    // Token refresh (uma vez)
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({ access_token: "at-cached", expires_in: 3600 }),
    );
    // Primeira API call
    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ labels: [] }));
    // Segunda API call (sem refresh de token)
    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ labels: [] }));

    await client.listLabels();
    await client.listLabels();

    // Token refresh deve ter sido chamado apenas 1 vez
    // Total: 1 token + 2 API = 3 calls
    expect(fetchSpy).toHaveBeenCalledTimes(3);
  });

  it("lança erro se refresh token falhar", async () => {
    const client = new GmailClient(mockConfig);

    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ error: "invalid_grant" }, false, 401));

    await expect(client.listLabels()).rejects.toThrow(/Failed to refresh Gmail access token/);
  });
});

// ══════════════════════════════════════════════════════════════
// GmailClient — API Methods
// ══════════════════════════════════════════════════════════════

describe("GmailClient — API methods", () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;
  let client: GmailClient;

  function setupToken() {
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({ access_token: "at-test", expires_in: 3600 }),
    );
  }

  beforeEach(() => {
    fetchSpy = vi.spyOn(globalThis, "fetch");
    client = new GmailClient(mockConfig);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("findLabelId retorna id correto", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({
        labels: [
          { id: "Label_1", name: "Comprovantes", type: "user" },
          { id: "INBOX", name: "INBOX", type: "system" },
        ],
      }),
    );

    const id = await client.findLabelId("Comprovantes");
    expect(id).toBe("Label_1");
  });

  it("findLabelId retorna null quando label não existe", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({ labels: [{ id: "INBOX", name: "INBOX", type: "system" }] }),
    );

    const id = await client.findLabelId("NaoExiste");
    expect(id).toBeNull();
  });

  it("findLabelId é case-insensitive", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({
        labels: [{ id: "Label_1", name: "Comprovantes", type: "user" }],
      }),
    );

    const id = await client.findLabelId("comprovantes");
    expect(id).toBe("Label_1");
  });

  it("listMessages envia labelIds e maxResults", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({
        messages: [{ id: "msg-1", threadId: "t-1" }],
        resultSizeEstimate: 1,
      }),
    );

    const result = await client.listMessages("Label_1", 10);

    expect(result.messages).toHaveLength(1);

    // Verify URL params
    const apiCall = fetchSpy.mock.calls[1]!;
    const url = new URL(apiCall[0] as string);
    expect(url.searchParams.get("labelIds")).toBe("Label_1");
    expect(url.searchParams.get("maxResults")).toBe("10");
  });

  it("listMessages envia pageToken quando fornecido", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ messages: [], resultSizeEstimate: 0 }));

    await client.listMessages("Label_1", 50, "page-token-abc");

    const apiCall = fetchSpy.mock.calls[1]!;
    const url = new URL(apiCall[0] as string);
    expect(url.searchParams.get("pageToken")).toBe("page-token-abc");
  });

  it("getMessage busca formato full", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({
        id: "msg-123",
        threadId: "t-123",
        labelIds: [],
        snippet: "",
        internalDate: "1711929600000",
        sizeEstimate: 1024,
        payload: {
          mimeType: "text/plain",
          headers: [],
          body: { size: 0 },
        },
      }),
    );

    const msg = await client.getMessage("msg-123");
    expect(msg.id).toBe("msg-123");

    const apiCall = fetchSpy.mock.calls[1]!;
    const url = new URL(apiCall[0] as string);
    expect(url.pathname).toContain("/messages/msg-123");
    expect(url.searchParams.get("format")).toBe("full");
  });

  it("getAttachment retorna dados base64url", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(mockFetchResponse({ size: 1234, data: "SGVsbG8" }));

    const att = await client.getAttachment("msg-1", "att-1");
    expect(att.data).toBe("SGVsbG8");
    expect(att.size).toBe(1234);

    const apiCall = fetchSpy.mock.calls[1]!;
    const url = new URL(apiCall[0] as string);
    expect(url.pathname).toContain("/messages/msg-1/attachments/att-1");
  });

  it("lança erro em resposta não-ok da API", async () => {
    setupToken();
    fetchSpy.mockResolvedValueOnce(
      mockFetchResponse({ error: { message: "Not Found" } }, false, 404),
    );

    await expect(client.getMessage("invalid")).rejects.toThrow(/Gmail API error \(404\)/);
  });
});
