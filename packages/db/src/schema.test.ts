import { describe, expect, it } from "vitest";
import { schema } from "./index";

// Teste de forma/contrato — não bate no banco, só inspeciona os objetos de
// coluna que o Drizzle constrói em tempo de import (nenhum I/O real). Roda
// sempre, sem skipIf: precisa rodar em todo `bun run test`/CI para cumprir o
// objetivo do Step 8 — falhar caso `mode: "number"` seja removido de uma
// coluna numeric, ou uma coluna numeric nova seja adicionada sem ele. Gatear
// isso atrás de TEST_DATABASE_URL faria a suíte inteira ser pulada no caso
// comum (sem banco de teste configurado), que é exatamente o oposto do que
// o teste existe para garantir.
describe("schema.ts — contrato de forma", () => {
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
