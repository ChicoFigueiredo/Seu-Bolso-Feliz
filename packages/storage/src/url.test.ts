import { describe, expect, it } from "vitest";
import type { ResolvedStorageConfig } from "./types";
import { buildListUrl, buildObjectUrl, encodeObjectKey } from "./url";

function makeConfig(overrides: Partial<ResolvedStorageConfig> = {}): ResolvedStorageConfig {
  return {
    endpoint: "http://127.0.0.1:9000",
    region: "us-east-1",
    forcePathStyle: true,
    ...overrides,
  };
}

describe("encodeObjectKey", () => {
  it("preserva as barras da hierarquia e escapa cada segmento", () => {
    expect(encodeObjectKey("user-1/manual/boleto.pdf")).toBe("user-1/manual/boleto.pdf");
    expect(encodeObjectKey("user-1/comprovante março.pdf")).toBe(
      "user-1/comprovante%20mar%C3%A7o.pdf",
    );
  });
});

describe("buildObjectUrl", () => {
  it("monta URL path-style: endpoint/bucket/chave", () => {
    const url = buildObjectUrl(makeConfig(), "ingestion-originals", "user-1/manual/boleto.pdf");
    expect(url.toString()).toBe(
      "http://127.0.0.1:9000/ingestion-originals/user-1/manual/boleto.pdf",
    );
  });

  it("normaliza barra dupla quando o endpoint termina com /", () => {
    const url = buildObjectUrl(
      makeConfig({ endpoint: "http://127.0.0.1:9000/" }),
      "ingestion-originals",
      "boleto.pdf",
    );
    expect(url.toString()).toBe("http://127.0.0.1:9000/ingestion-originals/boleto.pdf");
  });

  it("escapa espaços e acentos no nome do arquivo", () => {
    const url = buildObjectUrl(makeConfig(), "ingestion-originals", "user-1/comprovante março.pdf");
    expect(url.toString()).toBe(
      "http://127.0.0.1:9000/ingestion-originals/user-1/comprovante%20mar%C3%A7o.pdf",
    );
  });

  it("lança quando forcePathStyle é false — só path-style está implementado", () => {
    expect(() =>
      buildObjectUrl(makeConfig({ forcePathStyle: false }), "bucket", "chave.pdf"),
    ).toThrow(/path-style/);
  });
});

describe("buildListUrl", () => {
  it("sempre inclui list-type=2", () => {
    const url = buildListUrl(makeConfig(), "ingestion-originals");
    expect(url.pathname).toBe("/ingestion-originals/");
    expect(url.searchParams.get("list-type")).toBe("2");
    expect(url.searchParams.has("prefix")).toBe(false);
  });

  it("inclui prefix, max-keys e continuation-token quando informados", () => {
    const url = buildListUrl(makeConfig(), "ingestion-originals", {
      prefix: "user-1/manual/",
      maxKeys: 50,
      continuationToken: "tok-1",
    });
    expect(url.searchParams.get("prefix")).toBe("user-1/manual/");
    expect(url.searchParams.get("max-keys")).toBe("50");
    expect(url.searchParams.get("continuation-token")).toBe("tok-1");
  });

  it("lança quando forcePathStyle é false", () => {
    expect(() =>
      buildListUrl(makeConfig({ forcePathStyle: false }), "ingestion-originals"),
    ).toThrow(/path-style/);
  });
});
