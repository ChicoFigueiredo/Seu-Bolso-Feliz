/**
 * Testes do StorageClient contra a loja de blobs em memória de
 * `./test-support/fake-blob-api.ts` — sem rede, sem loja do Vercel Blob de
 * verdade. Cobrem o que o pacote é responsável por: o mapeamento
 * bucket→prefixo de caminho (ida e volta), a privacidade dos blobs
 * (`access: "private"` em toda operação, URL assinada com validade curta), o
 * token indo explícito em cada chamada, e a tradução dos erros do SDK para
 * `StorageError`.
 */
import { BlobError, BlobNotFoundError, BlobServiceRateLimited } from "@vercel/blob";
import { Buffer } from "node:buffer";
import { beforeEach, describe, expect, it } from "vitest";
import { StorageClient } from "./client";
import type { LojaDeBlobsFalsa } from "./test-support/fake-blob-api";
import { criarLojaDeBlobsFalsa } from "./test-support/fake-blob-api";

const TOKEN = "vercel_blob_rw_TOKEN_DE_TESTE";

describe("StorageClient", () => {
  let loja: LojaDeBlobsFalsa;

  beforeEach(() => {
    loja = criarLojaDeBlobsFalsa();
  });

  function client(): StorageClient {
    return new StorageClient({ token: TOKEN }, loja);
  }

  describe("construção", () => {
    it("exige o token na largada, antes de qualquer operação", () => {
      expect(() => new StorageClient({ token: "" }, loja)).toThrow(/token/);
      expect(loja.chamadas).toHaveLength(0);
    });
  });

  describe("upload", () => {
    it("grava sob o prefixo do bucket, privado, com o nome exato pedido", async () => {
      await client()
        .from("ingestion-originals")
        .upload("user-1/boleto.pdf", "conteudo-pdf", { contentType: "application/pdf" });

      expect(loja.chaves()).toEqual(["ingestion-originals/user-1/boleto.pdf"]);
      expect(Buffer.from(loja.bytes("ingestion-originals/user-1/boleto.pdf")!).toString()).toBe(
        "conteudo-pdf",
      );

      const chamada = loja.chamadas[0]!;
      expect(chamada.operacao).toBe("put");
      expect(chamada.options).toMatchObject({
        access: "private",
        token: TOKEN,
        contentType: "application/pdf",
        // Um sufixo aleatório mudaria o caminho gravado e o `storage_path`
        // do Postgres deixaria de encontrar o arquivo.
        addRandomSuffix: false,
      });
    });

    it("aceita Uint8Array sem arrastar os bytes vizinhos do buffer", async () => {
      const buffer = new Uint8Array([0, 0, 1, 2, 3, 0, 0]);
      const janela = buffer.subarray(2, 5);

      await client().from("b").upload("k.bin", janela);

      expect([...loja.bytes("b/k.bin")!]).toEqual([1, 2, 3]);
    });

    it("upsert padrão sobrescreve", async () => {
      const bucket = client().from("b");
      await bucket.upload("k.pdf", "primeiro");
      await bucket.upload("k.pdf", "segundo");

      expect(Buffer.from(loja.bytes("b/k.pdf")!).toString()).toBe("segundo");
    });

    it("upsert:false recusa sobrescrever e devolve already_exists", async () => {
      const bucket = client().from("b");
      await bucket.upload("k.pdf", "primeiro");

      await expect(bucket.upload("k.pdf", "segundo", { upsert: false })).rejects.toMatchObject({
        name: "StorageError",
        status: 409,
        code: "already_exists",
        key: "k.pdf",
      });
      // A recusa é do servidor, na mesma operação da escrita — não há HEAD
      // antes do PUT, e por isso não há corrida entre os dois.
      expect(loja.chamadas.map((c) => c.operacao)).toEqual(["put", "put"]);
      expect(Buffer.from(loja.bytes("b/k.pdf")!).toString()).toBe("primeiro");
    });

    it("traduz a recusa do serviço em StorageError com status e code", async () => {
      loja.falharCom(new BlobServiceRateLimited(30));

      await expect(client().from("b").upload("k.pdf", "x")).rejects.toMatchObject({
        name: "StorageError",
        status: 429,
        code: "rate_limited",
        key: "k.pdf",
      });
    });
  });

  describe("download", () => {
    it("devolve os bytes exatos do caminho, com o prefixo do bucket", async () => {
      loja.semear("ingestion-originals/user-1/boleto.pdf", new Uint8Array([1, 2, 3, 250]));

      const bytes = await client().from("ingestion-originals").download("user-1/boleto.pdf");

      expect([...bytes]).toEqual([1, 2, 3, 250]);
      expect(loja.chamadas[0]!.options).toMatchObject({ access: "private", token: TOKEN });
    });

    it("lança StorageError 404 quando o objeto não existe", async () => {
      await expect(client().from("b").download("sumido.pdf")).rejects.toMatchObject({
        name: "StorageError",
        status: 404,
        code: "not_found",
        key: "sumido.pdf",
      });
    });

    it("traduz BlobNotFoundError do SDK no mesmo 404", async () => {
      loja.falharCom(new BlobNotFoundError());

      await expect(client().from("b").download("k.pdf")).rejects.toMatchObject({
        status: 404,
        code: "not_found",
      });
    });
  });

  describe("getSignedUrl", () => {
    it("pede delegação restrita ao caminho e só para leitura", async () => {
      const antes = Date.now();
      const url = await client().from("ingestion-originals").getSignedUrl("user-1/boleto.pdf", 120);

      const delegacao = loja.chamadas.find((c) => c.operacao === "issueSignedToken")!;
      expect(delegacao.options).toMatchObject({
        token: TOKEN,
        pathname: "ingestion-originals/user-1/boleto.pdf",
        operations: ["get"],
      });

      const validUntil = delegacao.options!.validUntil as number;
      expect(validUntil).toBeGreaterThanOrEqual(antes + 120_000);
      expect(validUntil).toBeLessThanOrEqual(Date.now() + 120_000);

      const presign = loja.chamadas.find((c) => c.operacao === "presignUrl")!;
      expect(presign.options).toMatchObject({
        operation: "get",
        access: "private",
        pathname: "ingestion-originals/user-1/boleto.pdf",
      });
      expect(url).toContain("ingestion-originals/user-1/boleto.pdf");
    });

    it("usa 3600s de default quando expiresInSeconds é omitido", async () => {
      const antes = Date.now();
      await client().from("b").getSignedUrl("k.pdf");

      const delegacao = loja.chamadas.find((c) => c.operacao === "issueSignedToken")!;
      expect(delegacao.options!.validUntil as number).toBeGreaterThanOrEqual(antes + 3_600_000);
    });

    it("traduz a falha da delegação em StorageError", async () => {
      loja.falharCom(new BlobError("Access denied"));

      await expect(client().from("b").getSignedUrl("k.pdf")).rejects.toMatchObject({
        name: "StorageError",
        key: "k.pdf",
      });
    });
  });

  describe("remove", () => {
    it("aceita um único caminho", async () => {
      loja.semear("b/k.pdf", new Uint8Array([1]));

      await client().from("b").remove("k.pdf");

      expect(loja.chaves()).toEqual([]);
      expect(loja.chamadas[0]!.alvo).toEqual(["b/k.pdf"]);
    });

    it("aceita um array e remove tudo numa chamada só", async () => {
      loja.semear("b/a.pdf", new Uint8Array([1]));
      loja.semear("b/sub/b.pdf", new Uint8Array([2]));

      await client().from("b").remove(["a.pdf", "sub/b.pdf"]);

      expect(loja.chaves()).toEqual([]);
      expect(loja.chamadas).toHaveLength(1);
      expect(loja.chamadas[0]!.alvo).toEqual(["b/a.pdf", "b/sub/b.pdf"]);
    });

    it("remover o que já não existe é sucesso", async () => {
      loja.falharCom(new BlobNotFoundError());
      await expect(client().from("b").remove("ja-removido.pdf")).resolves.toBeUndefined();
    });

    it("outros erros continuam lançando StorageError", async () => {
      loja.falharCom(new BlobServiceRateLimited(5));

      await expect(client().from("b").remove("k.pdf")).rejects.toMatchObject({
        status: 429,
        code: "rate_limited",
      });
    });
  });

  describe("list", () => {
    it("filtra pelo bucket e devolve a chave sem o prefixo do bucket", async () => {
      loja.semear("ingestion-originals/user-1/a.pdf", new Uint8Array([1, 2]));
      loja.semear("ingestion-originals/user-2/b.pdf", new Uint8Array([1, 2, 3]));
      loja.semear("user-files/user-1/c.csv", new Uint8Array([9]));

      const resultado = await client().from("ingestion-originals").list("user-1/");

      expect(loja.chamadas[0]!.options).toMatchObject({
        token: TOKEN,
        prefix: "ingestion-originals/user-1/",
      });
      expect(resultado.objects).toEqual([
        { key: "user-1/a.pdf", size: 2, lastModified: "2026-08-20T00:00:00.000Z" },
      ]);
      expect(resultado.isTruncated).toBe(false);
      expect(resultado.nextContinuationToken).toBeUndefined();
    });

    it("lista o bucket inteiro quando o prefixo é omitido", async () => {
      loja.semear("ingestion-originals/a.pdf", new Uint8Array([1]));
      loja.semear("outro-bucket/b.pdf", new Uint8Array([1]));

      const resultado = await client().from("ingestion-originals").list();

      expect(resultado.objects.map((o) => o.key)).toEqual(["a.pdf"]);
    });

    it("expõe a paginação quando há mais objetos que o limite pedido", async () => {
      loja.semear("b/a.pdf", new Uint8Array([1]));
      loja.semear("b/b.pdf", new Uint8Array([1]));

      const resultado = await client().from("b").list(undefined, { maxKeys: 1 });

      expect(loja.chamadas[0]!.options).toMatchObject({ limit: 1 });
      expect(resultado.isTruncated).toBe(true);
      expect(resultado.nextContinuationToken).toBe("cursor-1");
    });

    it("repassa o cursor de continuação", async () => {
      await client().from("b").list(undefined, { continuationToken: "cursor-1" });
      expect(loja.chamadas[0]!.options).toMatchObject({ cursor: "cursor-1" });
    });

    it("traduz a falha da listagem em StorageError", async () => {
      loja.falharCom(new BlobServiceRateLimited(5));
      await expect(client().from("b").list()).rejects.toMatchObject({ status: 429 });
    });
  });

  it("manda o token em toda chamada — o SDK nunca cai no process.env sozinho", async () => {
    const bucket = client().from("b");
    await bucket.upload("k.pdf", "x");
    await bucket.download("k.pdf");
    await bucket.list();
    await bucket.remove("k.pdf");

    expect(loja.chamadas).not.toHaveLength(0);
    for (const chamada of loja.chamadas) {
      expect(chamada.options).toMatchObject({ token: TOKEN });
    }
  });
});
