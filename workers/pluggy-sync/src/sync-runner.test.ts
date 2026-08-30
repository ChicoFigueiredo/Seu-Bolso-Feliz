import { describe, it, expect } from "vitest";
import { isUniqueViolation } from "./sync-runner";

/**
 * Antes da migração pra Drizzle, o PostgREST devolvia o SQLSTATE num objeto
 * de retorno (`{ error: { code } }`). Com o driver `pg`/Neon o erro é LANÇADO,
 * e o Drizzle ainda embrulha o erro do driver num `DrizzleQueryError` — o
 * original fica em `err.cause`. Estes testes travam os dois formatos e, tão
 * importante quanto, travam o que NÃO pode ser tratado como duplicata.
 */
describe("isUniqueViolation", () => {
  it("reconhece o SQLSTATE 23505 no nível de cima (erro cru do driver pg)", () => {
    const err = Object.assign(new Error("duplicate key value violates unique constraint"), {
      code: "23505",
    });
    expect(isUniqueViolation(err)).toBe(true);
  });

  it("reconhece o SQLSTATE 23505 aninhado em err.cause (DrizzleQueryError)", () => {
    const driverError = Object.assign(new Error("duplicate key value"), { code: "23505" });
    const wrapped = new Error("Failed query: insert into draft_records", { cause: driverError });
    expect(isUniqueViolation(wrapped)).toBe(true);
  });

  it("reconhece 23505 em cause mesmo quando o wrapper tem um code próprio diferente", () => {
    const driverError = Object.assign(new Error("duplicate key value"), { code: "23505" });
    const wrapped = Object.assign(new Error("Failed query"), {
      code: "DRIZZLE_QUERY_ERROR",
      cause: driverError,
    });
    expect(isUniqueViolation(wrapped)).toBe(true);
  });

  it("não trata erro genérico sem code como duplicata", () => {
    expect(isUniqueViolation(new Error("connection terminated unexpectedly"))).toBe(false);
  });

  it("não trata outro SQLSTATE como duplicata (23503 = violação de FK)", () => {
    const err = Object.assign(new Error("insert or update violates foreign key constraint"), {
      code: "23503",
    });
    expect(isUniqueViolation(err)).toBe(false);
  });

  it("não casa por texto da mensagem — só o SQLSTATE conta", () => {
    // Mensagem citando 23505 sem o campo `code`: a checagem por string seria
    // verdadeira aqui e enterraria um erro real como "já sincronizado".
    const err = new Error("erro inesperado ao redor do código 23505 do postgres");
    expect(isUniqueViolation(err)).toBe(false);
  });

  it("não estoura com null, undefined, string ou objeto sem code", () => {
    expect(isUniqueViolation(null)).toBe(false);
    expect(isUniqueViolation(undefined)).toBe(false);
    expect(isUniqueViolation("23505")).toBe(false);
    expect(isUniqueViolation({})).toBe(false);
    expect(isUniqueViolation(new Error("x", { cause: { semCode: true } }))).toBe(false);
  });
});
