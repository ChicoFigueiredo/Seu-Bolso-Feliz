import { describe, expect, it } from "vitest";
import { createStorageClientFromEnv } from "./index";
import { loadStorageConfigFromEnv } from "./env";

describe("loadStorageConfigFromEnv", () => {
  it("lê o token do Vercel Blob", () => {
    expect(loadStorageConfigFromEnv({ BLOB_READ_WRITE_TOKEN: "vercel_blob_rw_abc" })).toEqual({
      token: "vercel_blob_rw_abc",
    });
  });

  it("lança nomeando a variável quando o token está ausente", () => {
    expect(() => loadStorageConfigFromEnv({})).toThrow(/BLOB_READ_WRITE_TOKEN/);
  });

  it("trata token vazio como ausente", () => {
    expect(() => loadStorageConfigFromEnv({ BLOB_READ_WRITE_TOKEN: "" })).toThrow(
      /BLOB_READ_WRITE_TOKEN/,
    );
  });
});

describe("createStorageClientFromEnv", () => {
  /**
   * Este é o ponto do contrato que mais importa na prática: sem o token, o
   * erro tem que aparecer aqui, com o nome da variável, e não no meio de um
   * upload de boleto — que é onde ele apareceria se o cliente deixasse o
   * `@vercel/blob` procurar a credencial sozinho no `process.env`.
   */
  it("falha na construção quando o token não está no ambiente", () => {
    expect(() => createStorageClientFromEnv({})).toThrow(/BLOB_READ_WRITE_TOKEN/);
  });

  it("constrói normalmente quando o token está presente", () => {
    expect(() =>
      createStorageClientFromEnv({ BLOB_READ_WRITE_TOKEN: "vercel_blob_rw_abc" }),
    ).not.toThrow();
  });
});
