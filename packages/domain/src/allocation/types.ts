// @sbf/domain — Tipos do rateio (divisão do valor de uma movimentação entre
// centros). Vocabulário em CONTEXT.md: Centro tem saldo e aceita rateio; Tag
// não. Rateio guarda valor, nunca percentual.

/** Um rateio: quanto do valor de uma movimentação foi para um centro. */
export interface AllocationInput {
  /** Identificador do centro que recebe a parte. */
  financialCenterId: string;
  /** Valor em reais, sempre positivo. O sinal vem da movimentação. */
  amount: number;
  /** Observação livre do usuário sobre esta parte. */
  note?: string | null;
}

export type AllocationIssueCode =
  /** O valor da movimentação não é um número finito. */
  | "movimentacao_invalida"
  /** O valor do rateio não é um número finito. */
  | "valor_invalido"
  /** O valor do rateio é zero ou negativo (ou arredonda para zero centavo). */
  | "valor_nao_positivo"
  /** O rateio não aponta para nenhum centro. */
  | "centro_ausente"
  /** O mesmo centro aparece mais de uma vez no conjunto. */
  | "centro_repetido"
  /** A soma dos rateios passou do valor da movimentação. */
  | "soma_excede_movimentacao";

export interface AllocationIssue {
  code: AllocationIssueCode;
  /** Mensagem pronta para a interface, em português. */
  message: string;
  /** Centro envolvido, quando o problema é de um rateio específico. */
  financialCenterId?: string;
}

export interface AllocationValidation {
  /** Verdadeiro quando o conjunto pode ser gravado. */
  ok: boolean;
  /** Soma dos rateios, em reais. */
  allocated: number;
  /**
   * Parte ainda não rateada: |valor da movimentação| − soma dos rateios.
   * Estado legítimo e visível, não erro. Fica negativa quando a soma excede
   * a movimentação — aí `ok` é falso e o número serve só para a UI dizer
   * quanto sobrou.
   */
  unallocated: number;
  issues: AllocationIssue[];
}
