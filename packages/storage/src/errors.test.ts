import {
  BlobAccessError,
  BlobError,
  BlobNotFoundError,
  BlobServiceNotAvailable,
  BlobServiceRateLimited,
  BlobStoreNotFoundError,
  BlobStoreSuspendedError,
} from "@vercel/blob";
import { describe, expect, it } from "vitest";
import { StorageError, toStorageError } from "./errors";

describe("StorageError", () => {
  it("expõe message/status/code/key e é um Error de verdade", () => {
    const error = new StorageError("Objeto já existe", 409, "already_exists", "user-1/foo.pdf");

    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe("StorageError");
    expect(error.message).toBe("Objeto já existe");
    expect(error.status).toBe(409);
    expect(error.code).toBe("already_exists");
    expect(error.key).toBe("user-1/foo.pdf");
  });

  it("permite omitir status/code/key", () => {
    const error = new StorageError("falha genérica");
    expect(error.status).toBeUndefined();
    expect(error.code).toBeUndefined();
    expect(error.key).toBeUndefined();
  });
});

describe("toStorageError", () => {
  it.each([
    [new BlobNotFoundError(), 404, "not_found"],
    [new BlobStoreNotFoundError(), 404, "store_not_found"],
    [new BlobAccessError(), 403, "access_denied"],
    [new BlobStoreSuspendedError(), 403, "store_suspended"],
    [new BlobServiceRateLimited(10), 429, "rate_limited"],
    [new BlobServiceNotAvailable(), 503, "service_unavailable"],
  ])("traduz %s em status %i / code %s", (erro, status, code) => {
    const traduzido = toStorageError(erro, "user-1/foo.pdf");

    expect(traduzido).toBeInstanceOf(StorageError);
    expect(traduzido.status).toBe(status);
    expect(traduzido.code).toBe(code);
    expect(traduzido.key).toBe("user-1/foo.pdf");
  });

  it("reconhece a recusa de sobrescrita pela mensagem do servidor", () => {
    const traduzido = toStorageError(
      new BlobError("This blob already exists, use allowOverwrite: true to overwrite it"),
      "k.pdf",
    );

    expect(traduzido.status).toBe(409);
    expect(traduzido.code).toBe("already_exists");
  });

  it("mantém os demais erros do SDK como blob_error, sem inventar status", () => {
    const traduzido = toStorageError(new BlobError("Bad request"));

    expect(traduzido.code).toBe("blob_error");
    expect(traduzido.status).toBeUndefined();
  });

  it("devolve o StorageError intacto quando já recebe um", () => {
    const original = new StorageError("já traduzido", 404, "not_found", "k.pdf");
    expect(toStorageError(original)).toBe(original);
  });

  it("aceita qualquer coisa lançada, não só Error", () => {
    expect(toStorageError(new Error("erro comum"), "k.pdf").message).toBe("erro comum");
    expect(toStorageError("string solta").message).toBe("string solta");
  });
});
