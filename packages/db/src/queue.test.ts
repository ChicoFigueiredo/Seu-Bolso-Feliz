import { describe, it, expect, vi } from "vitest";
import {
  computeBackoffSeconds,
  formatPgTextArray,
  parsePgTextArray,
  resolveCapabilities,
  JobNotifier,
  sleep,
} from "./queue";

describe("formatPgTextArray / parsePgTextArray", () => {
  it("serializa lista vazia como {}", () => {
    expect(formatPgTextArray([])).toBe("{}");
  });

  it("faz round-trip de valores simples", () => {
    const values = ["ocr", "pasta-local", "senha-pdf"];
    expect(parsePgTextArray(formatPgTextArray(values))).toEqual(values);
  });

  it("escapa aspas e barras invertidas", () => {
    const values = ['com "aspas"', "com\\barra", "com,virgula"];
    expect(parsePgTextArray(formatPgTextArray(values))).toEqual(values);
  });

  it("aceita array já parseado pelo driver", () => {
    expect(parsePgTextArray(["ocr"])).toEqual(["ocr"]);
  });

  it("devolve lista vazia para entrada inesperada", () => {
    expect(parsePgTextArray(null)).toEqual([]);
    expect(parsePgTextArray(42)).toEqual([]);
    expect(parsePgTextArray("{}")).toEqual([]);
  });
});

describe("computeBackoffSeconds", () => {
  it("a primeira falha espera exatamente a base", () => {
    expect(computeBackoffSeconds(0)).toBe(5);
  });

  it("dobra a cada tentativa", () => {
    expect(computeBackoffSeconds(1)).toBe(10);
    expect(computeBackoffSeconds(2)).toBe(20);
    expect(computeBackoffSeconds(3)).toBe(40);
  });

  it("respeita o teto", () => {
    expect(computeBackoffSeconds(50)).toBe(3600);
    expect(computeBackoffSeconds(50, { maxSeconds: 60 })).toBe(60);
  });

  it("aceita base e fator customizados", () => {
    expect(computeBackoffSeconds(2, { baseSeconds: 1, factor: 3 })).toBe(9);
  });

  it("trata retryCount negativo como zero", () => {
    expect(computeBackoffSeconds(-5)).toBe(5);
  });
});

describe("resolveCapabilities", () => {
  it("usa o padrão do worker quando WORKER_CAPABILITIES não existe", () => {
    expect(resolveCapabilities(["ocr"], {})).toEqual(["ocr"]);
  });

  it("lê a lista separada por vírgula, ignorando espaços", () => {
    expect(resolveCapabilities([], { WORKER_CAPABILITIES: "ocr, pasta-local ,senha-pdf" })).toEqual(
      ["ocr", "pasta-local", "senha-pdf"],
    );
  });

  it("string vazia significa 'nenhuma capacidade', não 'use o padrão'", () => {
    expect(resolveCapabilities(["ocr"], { WORKER_CAPABILITIES: "" })).toEqual([]);
  });
});

describe("JobNotifier (sem banco)", () => {
  it("arm() resolve quando wake() dispara", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    const armed = notifier.arm();
    notifier.wake();
    await expect(armed.promise).resolves.toBe("notified");
  });

  it("cancel() libera a espera sem vazar o waiter", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    const armed = notifier.arm();
    armed.cancel();
    await expect(armed.promise).resolves.toBe("aborted");
    // wake() depois do cancel não deve encontrar ninguém esperando.
    expect(() => notifier.wake()).not.toThrow();
  });

  it("cai para timeout quando nenhuma notificação chega", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    await expect(notifier.wait({ timeoutMs: 20 })).resolves.toBe("timeout");
    await notifier.close();
  });

  it("respeita AbortSignal já abortado", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    const controller = new AbortController();
    controller.abort();
    await expect(notifier.wait({ timeoutMs: 5000, signal: controller.signal })).resolves.toBe(
      "aborted",
    );
    await notifier.close();
  });

  it("aborta no meio da espera", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    const controller = new AbortController();
    const promise = notifier.wait({ timeoutMs: 5000, signal: controller.signal });
    setTimeout(() => controller.abort(), 10);
    await expect(promise).resolves.toBe("aborted");
    await notifier.close();
  });

  it("close() acorda quem estava esperando", async () => {
    const notifier = new JobNotifier({ connectionString: undefined });
    const armed = notifier.arm();
    await notifier.close();
    await expect(armed.promise).resolves.toBe("notified");
  });
});

describe("sleep", () => {
  it("resolve depois do prazo", async () => {
    const start = Date.now();
    await sleep(30);
    expect(Date.now() - start).toBeGreaterThanOrEqual(20);
  });

  it("resolve imediatamente se o sinal já abortou", async () => {
    const controller = new AbortController();
    controller.abort();
    const spy = vi.fn();
    await sleep(10_000, controller.signal).then(spy);
    expect(spy).toHaveBeenCalled();
  });

  it("acorda no abort em vez de esperar o prazo inteiro", async () => {
    const controller = new AbortController();
    const start = Date.now();
    const promise = sleep(10_000, controller.signal);
    setTimeout(() => controller.abort(), 10);
    await promise;
    expect(Date.now() - start).toBeLessThan(5000);
  });
});
