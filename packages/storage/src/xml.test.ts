import { describe, expect, it } from "vitest";
import { decodeXmlEntities, parseListObjectsXml, parseS3ErrorXml } from "./xml";

describe("decodeXmlEntities", () => {
  it("decodifica as entidades XML básicas", () => {
    expect(decodeXmlEntities("Fatura &amp; Boleto")).toBe("Fatura & Boleto");
    expect(decodeXmlEntities("&lt;tag&gt;")).toBe("<tag>");
    expect(decodeXmlEntities("&quot;citado&quot;")).toBe('"citado"');
    expect(decodeXmlEntities("&apos;ok&apos; e &#39;ok&#39;")).toBe("'ok' e 'ok'");
  });

  it("deixa texto sem entidades intacto", () => {
    expect(decodeXmlEntities("comprovante-março.pdf")).toBe("comprovante-março.pdf");
  });
});

describe("parseListObjectsXml", () => {
  it("extrai key/size/lastModified de cada <Contents>", () => {
    const xml = `<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
  <Name>ingestion-originals</Name>
  <Prefix>user-1/</Prefix>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>user-1/manual/boleto.pdf</Key>
    <Size>12345</Size>
    <LastModified>2026-08-20T14:33:00.000Z</LastModified>
  </Contents>
  <Contents>
    <Key>user-1/manual/comprovante &amp; recibo.pdf</Key>
    <Size>999</Size>
    <LastModified>2026-08-21T09:00:00.000Z</LastModified>
  </Contents>
</ListBucketResult>`;

    const result = parseListObjectsXml(xml);

    expect(result.isTruncated).toBe(false);
    expect(result.nextContinuationToken).toBeUndefined();
    expect(result.objects).toEqual([
      {
        key: "user-1/manual/boleto.pdf",
        size: 12345,
        lastModified: "2026-08-20T14:33:00.000Z",
      },
      {
        key: "user-1/manual/comprovante & recibo.pdf",
        size: 999,
        lastModified: "2026-08-21T09:00:00.000Z",
      },
    ]);
  });

  it("expõe paginação quando IsTruncated=true", () => {
    const xml = `<ListBucketResult>
  <IsTruncated>true</IsTruncated>
  <NextContinuationToken>abc123</NextContinuationToken>
</ListBucketResult>`;

    const result = parseListObjectsXml(xml);

    expect(result.isTruncated).toBe(true);
    expect(result.nextContinuationToken).toBe("abc123");
    expect(result.objects).toEqual([]);
  });

  it("devolve lista vazia para um bucket sem objetos", () => {
    const xml = `<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>`;
    expect(parseListObjectsXml(xml).objects).toEqual([]);
  });
});

describe("parseS3ErrorXml", () => {
  it("extrai code e message do corpo de erro padrão do S3", () => {
    const xml = `<?xml version="1.0" encoding="UTF-8"?>
<Error>
  <Code>NoSuchKey</Code>
  <Message>The specified key does not exist.</Message>
  <Key>user-1/manual/inexistente.pdf</Key>
  <RequestId>abc-123</RequestId>
</Error>`;

    expect(parseS3ErrorXml(xml)).toEqual({
      code: "NoSuchKey",
      message: "The specified key does not exist.",
    });
  });

  it("devolve campos undefined quando o corpo não é um erro S3 reconhecível", () => {
    expect(parseS3ErrorXml("")).toEqual({ code: undefined, message: undefined });
    expect(parseS3ErrorXml("<html>Bad Gateway</html>")).toEqual({
      code: undefined,
      message: undefined,
    });
  });
});
