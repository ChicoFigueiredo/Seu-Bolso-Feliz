import { describe, it, expect, vi } from "vitest";
import { StorageError } from "@sbf/storage";
import {
  BUCKET_ORIGINAIS,
  mensagemDeErro,
  uploadOriginal,
  type DestinoDeUpload,
} from "./storage-upload";

/** Dublê mínimo do StorageClient: registra a chamada ou lança o que mandarem. */
function storageFake(aoEnviar?: () => Promise<void>) {
  const upload = vi.fn(async () => {
    if (aoEnviar) await aoEnviar();
  });
  const from = vi.fn(() => ({ upload }));
  return { storage: { from } as unknown as DestinoDeUpload, from, upload };
}

describe("uploadOriginal", () => {
  it("envia para o bucket de originais com o content-type informado", async () => {
    const { storage, from, upload } = storageFake();
    const dados = new Uint8Array([1, 2, 3]);

    const resultado = await uploadOriginal(storage, "user/uuid/nota.pdf", dados, "application/pdf");

    expect(resultado).toEqual({ ok: true });
    expect(from).toHaveBeenCalledWith(BUCKET_ORIGINAIS);
    expect(upload).toHaveBeenCalledWith("user/uuid/nota.pdf", dados, {
      contentType: "application/pdf",
    });
  });

  it("converte StorageError em resultado, sem propagar a exceção", async () => {
    // É o ponto da migração: o cliente antigo devolvia `{ error }` e o scanner
    // seguia para o próximo anexo. Se a exceção escapasse, um 403 de storage
    // derrubaria o scan inteiro em vez de custar um arquivo.
    const { storage } = storageFake(async () => {
      throw new StorageError("Access Denied", 403, "AccessDenied", "user/uuid/nota.pdf");
    });

    const resultado = await uploadOriginal(storage, "user/uuid/nota.pdf", "x", "text/plain");

    expect(resultado).toEqual({ ok: false, erro: "Access Denied (AccessDenied 403)" });
  });

  it("converte erro genérico (rede caiu, por exemplo) em resultado", async () => {
    const { storage } = storageFake(async () => {
      throw new TypeError("fetch failed");
    });

    await expect(uploadOriginal(storage, "p", "x", "text/plain")).resolves.toEqual({
      ok: false,
      erro: "fetch failed",
    });
  });

  it("converte valor lançado que nem é Error", async () => {
    const { storage } = storageFake(async () => {
      throw "boom";
    });

    await expect(uploadOriginal(storage, "p", "x", "text/plain")).resolves.toEqual({
      ok: false,
      erro: "boom",
    });
  });
});

describe("mensagemDeErro", () => {
  it("junta código e status ao texto do StorageError", () => {
    expect(mensagemDeErro(new StorageError("No such key", 404, "NoSuchKey"))).toBe(
      "No such key (NoSuchKey 404)",
    );
  });

  it("usa só a mensagem quando o StorageError não traz código nem status", () => {
    expect(mensagemDeErro(new StorageError("falhou"))).toBe("falhou");
  });

  it("usa a mensagem de qualquer Error", () => {
    expect(mensagemDeErro(new Error("insert falhou"))).toBe("insert falhou");
  });

  it("serializa o que não é Error", () => {
    expect(mensagemDeErro({ codigo: 1 })).toBe("[object Object]");
    expect(mensagemDeErro(null)).toBe("null");
  });
});
