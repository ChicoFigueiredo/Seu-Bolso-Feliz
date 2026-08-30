import { describe, expect, it } from "vitest";
import { StorageError } from "./errors";

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
