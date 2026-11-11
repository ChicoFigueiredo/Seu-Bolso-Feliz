import { z } from "zod";

// ══════════════════════════════════════════════════════════════
// Centros
//
// Centro é *para quem* ou *para qual projeto* o dinheiro foi — Edulingo,
// Imob Max, Daiane. Tem saldo, entradas e saídas, e se comporta como uma
// conta corrente virtual (CONTEXT.md). Não confundir com Tag (rótulo livre,
// sem valor e sem saldo) nem com Categoria (a natureza do gasto).
//
// Os campos seguem o formato da linha em `financial_centers` (snake_case),
// igual aos demais schemas deste pacote.
// ══════════════════════════════════════════════════════════════

/** Natureza do centro: um projeto, uma pessoa ou um negócio. */
export const financialCenterTypeSchema = z.enum(["projeto", "pessoa", "negocio"]);

/** Centro arquivado continua no histórico, mas não recebe rateio novo. */
export const financialCenterStatusSchema = z.enum(["active", "archived"]);

export const createFinancialCenterSchema = z.object({
  // `unique (user_id, name)` no banco — o trim evita que "Edulingo " passe
  // como um segundo centro.
  name: z.string().trim().min(1).max(200),
  type: financialCenterTypeSchema.default("projeto"),
  description: z.string().max(1000).nullish(),
  status: financialCenterStatusSchema.default("active"),
  icon: z.string().max(50).nullish(),
  color: z.string().max(20).nullish(),
});

export const updateFinancialCenterSchema = createFinancialCenterSchema.partial();

export type FinancialCenterType = z.infer<typeof financialCenterTypeSchema>;
export type FinancialCenterStatus = z.infer<typeof financialCenterStatusSchema>;

// `z.input` e não `z.infer`: `type` e `status` têm `.default()`, então quem
// chama pode omitir os dois — é o schema que preenche. `z.infer` descreve o
// que sai do `.parse()`, onde os dois já são obrigatórios.
export type CreateFinancialCenterInput = z.input<typeof createFinancialCenterSchema>;
export type UpdateFinancialCenterInput = z.input<typeof updateFinancialCenterSchema>;
export type FinancialCenterFields = z.infer<typeof createFinancialCenterSchema>;
