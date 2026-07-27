/**
 * @sbf/contracts — contratos canônicos e versionados entre o pipeline de
 * ingestão e a camada de materialização.
 *
 * Existe porque gerador e materializador falavam schemas incompatíveis e
 * nada no sistema os obrigava a concordar: o gerador emitia `type: "despesa"`,
 * `due_date` e `base_amount`; o materializador exigia `type: "expense"`,
 * `event_date`, `amount` e `financial_product_id`. Nenhum draft gerado
 * automaticamente jamais passou na validação.
 *
 * Agora os dois lados importam os mesmos schemas daqui.
 */
export * from "./draft";
export * from "./draft/migrate-v0";
export * from "./draft/parse";
export * from "./mappers/extraction-to-draft";
export * from "./obligation/intent-to-obligation-type";
