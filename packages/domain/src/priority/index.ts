// @sbf/domain — Priority Logic (Prioridade de Pagamento)
//
// O sistema suporta priorização de pagamentos com 5 níveis:
// essential > high > medium > low > optional
//
// A prioridade pode ser:
// 1. Definida manualmente pelo usuário
// 2. Derivada de tags (ex: tag "essencial" → priority "essential")
// 3. Derivada do tipo de obrigação (ex: moradia, financiamento)
// 4. Combinação de regras

export type PriorityLevel = "essential" | "high" | "medium" | "low" | "optional";

const PRIORITY_ORDER: Record<PriorityLevel, number> = {
  essential: 0,
  high: 1,
  medium: 2,
  low: 3,
  optional: 4,
};

export interface PrioritizableItem {
  id: string;
  priority?: PriorityLevel | null;
  dueDate?: string | null;
  amount: number;
  description?: string | null;
  isOverdue?: boolean;
  tags?: Array<{
    name: string;
    suggestedPriority?: PriorityLevel | null;
    influencesPriority?: boolean;
  }>;
}

export interface PrioritizedItem extends PrioritizableItem {
  effectivePriority: PriorityLevel;
  sortScore: number;
}

/**
 * Deriva a prioridade efetiva de um item considerando:
 * 1. Prioridade manual (se definida, tem precedência)
 * 2. Tags que influenciam prioridade (usa a mais alta)
 * 3. Default: "medium"
 */
export function deriveEffectivePriority(item: PrioritizableItem): PriorityLevel {
  // Prioridade manual tem precedência
  if (item.priority) return item.priority;

  // Derivar de tags que influenciam prioridade
  if (item.tags && item.tags.length > 0) {
    const tagPriorities = item.tags
      .filter((t) => t.influencesPriority && t.suggestedPriority)
      .map((t) => t.suggestedPriority!);

    if (tagPriorities.length > 0) {
      // Retorna a prioridade mais alta (menor índice = mais prioritário)
      return tagPriorities.reduce((highest, current) =>
        PRIORITY_ORDER[current] < PRIORITY_ORDER[highest] ? current : highest,
      );
    }
  }

  return "medium";
}

/**
 * Calcula o score de ordenação para um item.
 * Quanto menor o score, mais urgente o item.
 *
 * Critérios de ordenação:
 * 1. Prioridade (essential primeiro)
 * 2. Items atrasados antes de items em dia
 * 3. Data de vencimento (mais próxima primeiro)
 * 4. Valor (maior primeiro, para itens empatados)
 */
export function calculateSortScore(item: PrioritizableItem, referenceDate?: Date): number {
  const ref = referenceDate ?? new Date();
  const priority = deriveEffectivePriority(item);
  let score = PRIORITY_ORDER[priority] * 10000;

  // Itens atrasados ganham urgência
  if (item.isOverdue) {
    score -= 50000;
  } else if (item.dueDate) {
    const due = new Date(item.dueDate);
    const daysUntilDue = Math.ceil((due.getTime() - ref.getTime()) / 86400000);
    if (daysUntilDue < 0) {
      score -= 50000; // atrasado
    } else if (daysUntilDue <= 3) {
      score -= 30000; // vence em até 3 dias
    } else if (daysUntilDue <= 7) {
      score -= 20000; // vence em até 7 dias
    }
    score += daysUntilDue; // desempate por proximidade
  }

  // Desempate por valor (maior valor tem prioridade)
  score -= item.amount * 0.001;

  return score;
}

/**
 * Ordena itens por prioridade para a tela principal.
 * Retorna items com prioridade efetiva e score calculados.
 */
export function prioritizeItems(
  items: PrioritizableItem[],
  referenceDate?: Date,
): PrioritizedItem[] {
  return items
    .map((item) => ({
      ...item,
      effectivePriority: deriveEffectivePriority(item),
      sortScore: calculateSortScore(item, referenceDate),
    }))
    .sort((a, b) => a.sortScore - b.sortScore);
}

/**
 * Filtra items por nível de prioridade.
 */
export function filterByPriority(
  items: PrioritizableItem[],
  minPriority: PriorityLevel,
): PrioritizableItem[] {
  const threshold = PRIORITY_ORDER[minPriority];
  return items.filter((item) => {
    const effective = deriveEffectivePriority(item);
    return PRIORITY_ORDER[effective] <= threshold;
  });
}

/**
 * Agrupa items por nível de prioridade.
 */
export function groupByPriority(
  items: PrioritizableItem[],
): Record<PriorityLevel, PrioritizableItem[]> {
  const groups: Record<PriorityLevel, PrioritizableItem[]> = {
    essential: [],
    high: [],
    medium: [],
    low: [],
    optional: [],
  };

  for (const item of items) {
    const priority = deriveEffectivePriority(item);
    groups[priority].push(item);
  }

  return groups;
}

/**
 * Retorna o rank numérico de uma prioridade (0 = mais urgente).
 */
export function getPriorityRank(priority: PriorityLevel): number {
  return PRIORITY_ORDER[priority];
}

/**
 * Compara duas prioridades. Retorna negativo se a é mais urgente que b.
 */
export function comparePriorities(a: PriorityLevel, b: PriorityLevel): number {
  return PRIORITY_ORDER[a] - PRIORITY_ORDER[b];
}
