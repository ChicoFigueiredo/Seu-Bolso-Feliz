/**
 * Testes de integração do StorageClient contra um servidor HTTP fake local
 * (ver `./test-support/fake-s3-server.ts`) — sem rede externa, sem SeaweedFS
 * real. Cobrem exatamente os três pontos pedidos: montagem de URL (path +
 * query chegando como esperado no servidor), assinatura SigV4 (headers
 * `Authorization`/`X-Amz-*` presentes e variando por requisição, presign
 * offline) e tratamento de erro (status não-2xx do S3 virando StorageError
 * tipado, com a exceção documentada do DELETE idempotente).
 */
import { Buffer } from "node:buffer";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { StorageClient } from "./client";
import type { FakeResponse, FakeS3Server } from "./test-support/fake-s3-server";
import { startFakeS3Server } from "./test-support/fake-s3-server";

const CREDENTIALS = { accessKeyId: "test-access-key", secretAccessKey: "test-secret-key" };

describe("StorageClient", () => {
  let server: FakeS3Server;
  let defaultResponse: FakeResponse;
  let responseByMethod: Partial<Record<string, FakeResponse>>;

  beforeEach(async () => {
    defaultResponse = { status: 200 };
    responseByMethod = {};
    server = await startFakeS3Server((req) => responseByMethod[req.method] ?? defaultResponse);
  });

  afterEach(async () => {
    await server.close();
  });

  function client(): StorageClient {
    return new StorageClient({ ...CREDENTIALS, region: "us-east-1", endpoint: server.url });
  }

  describe("upload", () => {
    it("faz PUT assinado com Content-Type e corpo corretos", async () => {
      await client()
        .from("ingestion-originals")
        .upload("user-1/boleto.pdf", "conteudo-pdf", { contentType: "application/pdf" });

      expect(server.requests).toHaveLength(1);
      const req = server.requests[0]!;
      expect(req.method).toBe("PUT");
      expect(req.url).toBe("/ingestion-originals/user-1/boleto.pdf");
      expect(req.headers["content-type"]).toBe("application/pdf");
      expect(req.body.toString()).toBe("conteudo-pdf");
      expect(String(req.headers.authorization)).toMatch(/^AWS4-HMAC-SHA256 /);
      expect(req.headers["x-amz-content-sha256"]).toBe("UNSIGNED-PAYLOAD");
    });

    it("upsert:false verifica existência via HEAD antes do PUT", async () => {
      responseByMethod.HEAD = { status: 404 };

      await client().from("b").upload("k.pdf", "x", { upsert: false });

      expect(server.requests.map((r) => r.method)).toEqual(["HEAD", "PUT"]);
    });

    it("upsert:false lança StorageError sem fazer PUT quando o objeto já existe", async () => {
      responseByMethod.HEAD = { status: 200 };

      await expect(
        client().from("b").upload("k.pdf", "x", { upsert: false }),
      ).rejects.toMatchObject({
        name: "StorageError",
        status: 409,
        code: "already_exists",
        key: "k.pdf",
      });
      expect(server.requests.map((r) => r.method)).toEqual(["HEAD"]);
    });

    it("lança StorageError com code/message do XML quando o servidor recusa", async () => {
      defaultResponse = {
        status: 403,
        body: "<Error><Code>AccessDenied</Code><Message>Credencial inválida</Message></Error>",
      };

      await expect(client().from("b").upload("k.pdf", "x")).rejects.toMatchObject({
        name: "StorageError",
        status: 403,
        code: "AccessDenied",
        message: "Credencial inválida",
      });
    });
  });

  describe("download", () => {
    it("faz GET assinado e devolve os bytes exatos", async () => {
      defaultResponse = { status: 200, body: "conteudo-binario" };

      const bytes = await client().from("ingestion-originals").download("user-1/boleto.pdf");

      expect(Buffer.from(bytes).toString()).toBe("conteudo-binario");
      expect(server.requests[0]!.method).toBe("GET");
      expect(server.requests[0]!.url).toBe("/ingestion-originals/user-1/boleto.pdf");
    });

    it("lança StorageError 404 com code NoSuchKey", async () => {
      defaultResponse = {
        status: 404,
        body: "<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message></Error>",
      };

      await expect(client().from("b").download("inexistente.pdf")).rejects.toMatchObject({
        status: 404,
        code: "NoSuchKey",
      });
    });
  });

  describe("getSignedUrl", () => {
    it("assina localmente (sem bater na rede) com os parâmetros de query esperados", async () => {
      const url = await client().from("ingestion-originals").getSignedUrl("user-1/boleto.pdf", 120);

      expect(server.requests).toHaveLength(0);

      const parsed = new URL(url);
      expect(parsed.pathname).toBe("/ingestion-originals/user-1/boleto.pdf");
      expect(parsed.searchParams.get("X-Amz-Expires")).toBe("120");
      expect(parsed.searchParams.get("X-Amz-Algorithm")).toBe("AWS4-HMAC-SHA256");
      expect(parsed.searchParams.get("X-Amz-Credential")).toContain(CREDENTIALS.accessKeyId);
      expect(parsed.searchParams.get("X-Amz-Signature")).toBeTruthy();
    });

    it("usa 3600s de default quando expiresInSeconds é omitido", async () => {
      const url = await client().from("b").getSignedUrl("k.pdf");
      expect(new URL(url).searchParams.get("X-Amz-Expires")).toBe("3600");
    });
  });

  describe("remove", () => {
    it("aceita um único caminho", async () => {
      defaultResponse = { status: 204 };

      await client().from("b").remove("k.pdf");

      expect(server.requests).toHaveLength(1);
      expect(server.requests[0]!.method).toBe("DELETE");
      expect(server.requests[0]!.url).toBe("/b/k.pdf");
    });

    it("aceita um array e remove cada chave em sequência", async () => {
      defaultResponse = { status: 204 };

      await client().from("b").remove(["a.pdf", "sub/b.pdf"]);

      expect(server.requests.map((r) => r.url)).toEqual(["/b/a.pdf", "/b/sub/b.pdf"]);
    });

    it("404 não é erro — DELETE é idempotente no S3", async () => {
      defaultResponse = { status: 404, body: "<Error><Code>NoSuchKey</Code></Error>" };
      await expect(client().from("b").remove("ja-removido.pdf")).resolves.toBeUndefined();
    });

    it("outros erros continuam lançando StorageError (após esgotar as retries de 5xx)", async () => {
      defaultResponse = { status: 500, body: "<Error><Code>InternalError</Code></Error>" };

      await expect(client().from("b").remove("k.pdf")).rejects.toMatchObject({
        status: 500,
        code: "InternalError",
      });

      // DEFAULT_RETRIES=2 → 1 tentativa inicial + 2 retries do aws4fetch em
      // 5xx. Confirma que o default conservador (client.ts) está de fato
      // sendo aplicado — o default da lib (10) faria isto estourar os 5s de
      // timeout do teste.
      expect(server.requests).toHaveLength(3);
    });
  });

  describe("list", () => {
    it("monta a query list-type=2 + prefix e parseia o XML de resposta", async () => {
      defaultResponse = {
        status: 200,
        body: `<ListBucketResult>
          <IsTruncated>false</IsTruncated>
          <Contents><Key>user-1/a.pdf</Key><Size>10</Size><LastModified>2026-08-20T00:00:00.000Z</LastModified></Contents>
          <Contents><Key>user-1/b.pdf</Key><Size>20</Size><LastModified>2026-08-21T00:00:00.000Z</LastModified></Contents>
        </ListBucketResult>`,
      };

      const result = await client().from("ingestion-originals").list("user-1/");

      const req = server.requests[0]!;
      expect(req.method).toBe("GET");
      const url = new URL(`http://fake${req.url}`);
      expect(url.pathname).toBe("/ingestion-originals/");
      expect(url.searchParams.get("list-type")).toBe("2");
      expect(url.searchParams.get("prefix")).toBe("user-1/");

      expect(result.isTruncated).toBe(false);
      expect(result.objects).toEqual([
        { key: "user-1/a.pdf", size: 10, lastModified: "2026-08-20T00:00:00.000Z" },
        { key: "user-1/b.pdf", size: 20, lastModified: "2026-08-21T00:00:00.000Z" },
      ]);
    });

    it("lança StorageError quando a listagem falha", async () => {
      defaultResponse = { status: 403, body: "<Error><Code>AccessDenied</Code></Error>" };
      await expect(client().from("b").list()).rejects.toMatchObject({ status: 403 });
    });
  });

  it("assina cada requisição de forma diferente (Authorization muda com o path)", async () => {
    await client().from("b").download("a.pdf");
    await client().from("b").download("b.pdf");

    const [first, second] = server.requests;
    expect(first!.headers.authorization).toBeTruthy();
    expect(first!.headers.authorization).not.toBe(second!.headers.authorization);
  });
});
