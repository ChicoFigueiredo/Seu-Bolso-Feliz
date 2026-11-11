import { describe, expect, it } from "vitest";
import { createFinancialCenterSchema, updateFinancialCenterSchema } from "./financial-center";

describe("createFinancialCenterSchema", () => {
  it("aceita um centro só com o nome e aplica os padrões", () => {
    const parsed = createFinancialCenterSchema.parse({ name: "Edulingo" });
    expect(parsed).toMatchObject({ name: "Edulingo", type: "projeto", status: "active" });
  });

  it("apara o nome — o banco tem unicidade por (user_id, name)", () => {
    expect(createFinancialCenterSchema.parse({ name: "  Imob Max  " }).name).toBe("Imob Max");
  });

  it("recusa nome vazio ou só com espaços", () => {
    expect(createFinancialCenterSchema.safeParse({ name: "" }).success).toBe(false);
    expect(createFinancialCenterSchema.safeParse({ name: "   " }).success).toBe(false);
  });

  it("aceita os três tipos de centro e recusa o resto", () => {
    for (const type of ["projeto", "pessoa", "negocio"]) {
      expect(createFinancialCenterSchema.safeParse({ name: "X", type }).success).toBe(true);
    }
    expect(createFinancialCenterSchema.safeParse({ name: "X", type: "tag" }).success).toBe(false);
  });

  it("aceita apenas os dois status", () => {
    expect(createFinancialCenterSchema.safeParse({ name: "X", status: "archived" }).success).toBe(
      true,
    );
    expect(createFinancialCenterSchema.safeParse({ name: "X", status: "deleted" }).success).toBe(
      false,
    );
  });

  it("recusa nome longo demais para a coluna", () => {
    expect(createFinancialCenterSchema.safeParse({ name: "a".repeat(201) }).success).toBe(false);
  });
});

describe("updateFinancialCenterSchema", () => {
  it("aceita atualização parcial", () => {
    expect(updateFinancialCenterSchema.parse({ status: "archived" })).toEqual({
      status: "archived",
    });
    expect(updateFinancialCenterSchema.parse({})).toEqual({});
  });

  it("mantém as mesmas recusas do schema de criação", () => {
    expect(updateFinancialCenterSchema.safeParse({ name: "  " }).success).toBe(false);
    expect(updateFinancialCenterSchema.safeParse({ type: "centro_de_custo" }).success).toBe(false);
  });
});
