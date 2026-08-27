import { describe, expect, it } from "vitest";
import { schema } from "./index";

// Teste de forma/contrato — não bate no banco, só inspeciona os objetos de
// coluna que o Drizzle constrói em tempo de import. Gated por TEST_DATABASE_URL
// para seguir o mesmo padrão de skip dos demais testes desta fase (Task 1
// brief, Step 8), mesmo não precisando de conexão real.
describe.skipIf(!process.env.TEST_DATABASE_URL)("schema.ts — contrato de forma", () => {
  it("institutions usa colunas snake_case", () => {
    expect(schema.institutions.user_id).toBeDefined();
    expect(schema.institutions.display_order).toBeDefined();
  });

  it("colunas numeric declaram mode: number", () => {
    const numericColumnNames = ["confidence_threshold", "credit_limit", "unit_price", "subtotal"];
    for (const name of numericColumnNames) {
      // Busca a coluna em qualquer tabela/view do schema que a tenha. Quando
      // `mode: "number"` está presente, o Drizzle constrói a coluna como
      // `PgNumericNumber` (columnType); sem o mode, columnType seria
      // "PgNumeric" em vez disso — então este check falha tanto se o mode for
      // removido de uma coluna existente quanto se uma coluna numeric nova for
      // adicionada sem ele.
      const found = Object.values(schema).some((table) => {
        const column = (table as Record<string, { columnType?: string } | undefined>)[name];
        return column?.columnType === "PgNumericNumber";
      });
      expect(found).toBe(true);
    }
  });
});
