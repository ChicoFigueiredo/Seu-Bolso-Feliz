/**
 * "A flag chega ao destino" vira teste, e não esperança.
 *
 * O orquestrador anterior declarava --from-date, --to-date, --process,
 * --ai-mode e --user-id, e nenhum deles produzia efeito: os scanners filhos
 * tinham switch sem default e engoliam qualquer flag desconhecida em silêncio.
 * Estes testes travam essa classe inteira de bug.
 */
import { describe, it, expect } from "vitest";
import { parseArgs, CliError, DEFAULTS, HELP } from "./args";

describe("toda flag declarada é aceita e tem efeito", () => {
  it.each([
    [["--gmail"], "source", "gmail"],
    [["--local"], "source", "local"],
    [["--both"], "source", "both"],
    [["--label", "Faturas"], "label", "Faturas"],
    [["--query", "from:nubank"], "query", "from:nubank"],
    [["--from-date", "2026-01-01"], "fromDate", "2026-01-01"],
    [["--to-date", "2026-06-30"], "toDate", "2026-06-30"],
    [["--include-body"], "includeBody", true],
    [["--no-attachments"], "includeAttachments", false],
    [["--dir", "/tmp/x"], "dir", "/tmp/x"],
    [["--recursive"], "recursive", true],
    [["--move-processed-to", "/tmp/done"], "moveProcessedTo", "/tmp/done"],
    [["--limit", "10"], "limit", 10],
    [["--batch-size", "25"], "batchSize", 25],
    [["--max-jobs", "5"], "maxJobs", 5],
    [["--dry-run"], "dryRun", true],
    [["--verbose"], "verbose", true],
    [["--process"], "process", true],
    [["--ai-mode", "full"], "aiMode", "full"],
    [["--user-id", "abc-123"], "userId", "abc-123"],
    [["--help"], "help", true],
  ])("%p define %s = %p", (argv, key, expected) => {
    const opts = parseArgs(argv as string[]);
    expect(opts[key as keyof typeof opts]).toEqual(expected);
  });

  it("toda flag do texto de ajuda é reconhecida pelo parser", () => {
    // Trava a divergência entre o que a ajuda promete e o que o parser aceita —
    // exatamente o descompasso que existia antes.
    const flags = [...HELP.matchAll(/^\s{2}(--[a-z-]+)/gm)].map((m) => m[1]!);
    expect(flags.length).toBeGreaterThan(15);

    const needsValue = new Set([
      "--label",
      "--query",
      "--from-date",
      "--to-date",
      "--dir",
      "--move-processed-to",
      "--limit",
      "--batch-size",
      "--max-jobs",
      "--ai-mode",
      "--user-id",
    ]);

    for (const flag of flags) {
      const argv = needsValue.has(flag)
        ? [flag, flag === "--ai-mode" ? "auto" : flag.includes("date") ? "2026-01-01" : "1"]
        : [flag];
      expect(() => parseArgs(argv), `${flag} deveria ser aceita`).not.toThrow();
    }
  });
});

describe("falha alto em vez de engolir", () => {
  it("rejeita flag desconhecida", () => {
    expect(() => parseArgs(["--nao-existe"])).toThrow(CliError);
  });

  it("rejeita flag que precisa de valor e não recebeu", () => {
    expect(() => parseArgs(["--label"])).toThrow(CliError);
    expect(() => parseArgs(["--label", "--verbose"])).toThrow(CliError);
  });

  it("rejeita ai-mode inválido", () => {
    expect(() => parseArgs(["--ai-mode", "turbo"])).toThrow(/ai-mode/);
  });

  it.each([
    ["--limit", "0"],
    ["--limit", "abc"],
    ["--batch-size", "-1"],
    ["--max-jobs", "0"],
  ])("rejeita %s %s", (flag, value) => {
    expect(() => parseArgs([flag, value])).toThrow(CliError);
  });
});

describe("combinações", () => {
  it("--gmail e --local juntos viram both", () => {
    expect(parseArgs(["--gmail", "--local"]).source).toBe("both");
    expect(parseArgs(["--local", "--gmail"]).source).toBe("both");
  });

  it("compõe backfill completo", () => {
    const o = parseArgs([
      "--gmail",
      "--query",
      "from:nubank",
      "--from-date",
      "2026-01-01",
      "--to-date",
      "2026-06-30",
      "--process",
      "--ai-mode",
      "lite",
      "--verbose",
    ]);
    expect(o).toMatchObject({
      source: "gmail",
      query: "from:nubank",
      fromDate: "2026-01-01",
      toDate: "2026-06-30",
      process: true,
      aiMode: "lite",
      verbose: true,
    });
  });

  it("sem argumentos usa os padrões", () => {
    expect(parseArgs([])).toEqual(DEFAULTS);
  });
});
