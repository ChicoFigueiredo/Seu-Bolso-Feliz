/**
 * AI function calling tool definitions for the SBF chat assistant.
 * Each tool maps to a real operation in the ingestion/financial domain.
 */

import { tool } from "ai";
import { z } from "zod";
import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, ilike, inArray, isNotNull, lte, or } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

/**
 * Valida posse das FKs opcionais (supplierId/institutionId) recebidas do
 * cliente antes de gravar em `document_patterns` — sem isso, o usuário A
 * poderia apontar o próprio padrão para um supplier ou institution que
 * pertence a outro usuário (IDOR). Mesmo padrão de
 * `app/actions/patterns.ts` (`assertOwnsOptionalFks`), adaptado para o
 * contrato de retorno de erro (não-throw) que as tools de IA usam.
 * Achado registrado no ledger da Task 15 (patterns.ts) e reservado para
 * correção nesta task, quando este arquivo foi finalmente convertido.
 */
async function findUnownedFkError(
  userId: string,
  fks: { supplierId?: string | null; institutionId?: string | null },
): Promise<string | null> {
  const db = getDb();

  if (fks.supplierId) {
    const [supplier] = await db
      .select({ id: schema.suppliers.id })
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, fks.supplierId), eq(schema.suppliers.user_id, userId)));
    if (!supplier) return "Fornecedor não encontrado";
  }

  if (fks.institutionId) {
    const [institution] = await db
      .select({ id: schema.institutions.id })
      .from(schema.institutions)
      .where(
        and(eq(schema.institutions.id, fks.institutionId), eq(schema.institutions.user_id, userId)),
      );
    if (!institution) return "Instituição não encontrada";
  }

  return null;
}

// ══════════════════════════════════════════════════════════════
// Group 1: Read Tools
// ══════════════════════════════════════════════════════════════

export const listPendingDocuments = tool({
  description:
    "Lista documentos ingeridos com drafts pendentes de revisão. Retorna nome, tipo, data e quantidade de drafts.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
    status: z
      .string()
      .optional()
      .describe("Filtrar por status do documento (ex: processed, failed)"),
  }),
  execute: async ({ limit, status }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.source_documents.user_id, user.id)];
    if (status) conditions.push(eq(schema.source_documents.status, status));

    const documents = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        mime_type: schema.source_documents.mime_type,
        origin_type: schema.source_documents.origin_type,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
      })
      .from(schema.source_documents)
      .where(and(...conditions))
      .orderBy(desc(schema.source_documents.created_at))
      .limit(limit);

    return { documents, count: documents.length };
  },
});

export const getDocumentDetails = tool({
  description:
    "Obtém detalhes completos de um documento incluindo metadados, jobs de processamento e drafts gerados.",
  parameters: z.object({
    documentId: z.string().uuid().describe("ID do documento"),
  }),
  execute: async ({ documentId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    const [docRows, jobs, drafts] = await Promise.all([
      db
        .select()
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, documentId),
            eq(schema.source_documents.user_id, user.id),
          ),
        ),
      db
        .select({
          id: schema.ingestion_jobs.id,
          status: schema.ingestion_jobs.status,
          error_message: schema.ingestion_jobs.error_message,
          created_at: schema.ingestion_jobs.created_at,
          updated_at: schema.ingestion_jobs.updated_at,
        })
        .from(schema.ingestion_jobs)
        .where(
          and(
            eq(schema.ingestion_jobs.source_document_id, documentId),
            eq(schema.ingestion_jobs.user_id, user.id),
          ),
        )
        .orderBy(desc(schema.ingestion_jobs.created_at)),
      db
        .select({
          id: schema.draft_records.id,
          status: schema.draft_records.status,
          draft_type: schema.draft_records.draft_type,
          draft_data: schema.draft_records.draft_data,
          confidence_score: schema.draft_records.confidence_score,
          created_at: schema.draft_records.created_at,
        })
        .from(schema.draft_records)
        .where(
          and(
            eq(schema.draft_records.source_document_id, documentId),
            eq(schema.draft_records.user_id, user.id),
          ),
        )
        .orderBy(desc(schema.draft_records.created_at)),
    ]);

    const document = docRows[0];
    if (!document) return { error: "Documento não encontrado" };

    return { document, jobs, drafts };
  },
});

export const listDrafts = tool({
  description: "Lista drafts (rascunhos) extraídos de documentos, com status e dados extraídos.",
  parameters: z.object({
    status: z
      .string()
      .optional()
      .default("pending_review")
      .describe("Status do draft: pending_review, approved, rejected"),
    limit: z.number().optional().default(10).describe("Máximo de drafts"),
  }),
  execute: async ({ status, limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const drafts = await db
      .select({
        id: schema.draft_records.id,
        status: schema.draft_records.status,
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        source_document_id: schema.draft_records.source_document_id,
        created_at: schema.draft_records.created_at,
      })
      .from(schema.draft_records)
      .where(
        and(eq(schema.draft_records.user_id, user.id), eq(schema.draft_records.status, status)),
      )
      .orderBy(desc(schema.draft_records.created_at))
      .limit(limit);

    return { drafts, count: drafts.length };
  },
});

export const getDraftDetails = tool({
  description:
    "Obtém detalhes completos de um draft específico, incluindo dados extraídos e documento de origem.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({
        id: schema.draft_records.id,
        status: schema.draft_records.status,
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        corrections: schema.draft_records.corrections,
        source_document_id: schema.draft_records.source_document_id,
        created_at: schema.draft_records.created_at,
        updated_at: schema.draft_records.updated_at,
      })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    let sourceDoc = null;
    if (draft.source_document_id) {
      const [doc] = await db
        .select({
          filename: schema.source_documents.filename,
          mime_type: schema.source_documents.mime_type,
          origin_type: schema.source_documents.origin_type,
        })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, draft.source_document_id),
            eq(schema.source_documents.user_id, user.id),
          ),
        );
      sourceDoc = doc ?? null;
    }

    return { draft: { ...draft, source_document: sourceDoc } };
  },
});

export const listRecentTransactions = tool({
  description: "Lista transações recentes do usuário para contexto e reconciliação.",
  parameters: z.object({
    limit: z.number().optional().default(20).describe("Máximo de transações"),
    category: z.string().optional().describe("Filtrar por categoria"),
  }),
  execute: async ({ limit, category }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.transactions.user_id, user.id)];
    if (category) conditions.push(eq(schema.categories.name, category));

    const transactions = await db
      .select({
        id: schema.transactions.id,
        description: schema.transactions.description,
        amount: schema.transactions.amount,
        event_date: schema.transactions.event_date,
        created_at: schema.transactions.created_at,
        category_name: schema.categories.name,
        supplier_name: schema.suppliers.name,
      })
      .from(schema.transactions)
      .leftJoin(
        schema.categories,
        and(
          eq(schema.transactions.category_id, schema.categories.id),
          // Defesa em profundidade: mesmo que `category_id` aponte pra uma
          // categoria de outro usuário (dado legado/bug de escrita anterior),
          // nunca devolver o nome dela — mesmo padrão de cards.ts.
          eq(schema.categories.user_id, user.id),
        ),
      )
      .leftJoin(
        schema.suppliers,
        and(
          eq(schema.transactions.supplier_id, schema.suppliers.id),
          eq(schema.suppliers.user_id, user.id),
        ),
      )
      .where(and(...conditions))
      .orderBy(desc(schema.transactions.event_date))
      .limit(limit);

    return { transactions, count: transactions.length };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 2: Analysis Tools (sem acesso a dados — inalteradas)
// ══════════════════════════════════════════════════════════════

export const suggestSupplier = tool({
  description: "Sugere o fornecedor/empresa de um documento baseado no texto extraído.",
  parameters: z.object({
    extractedText: z.string().describe("Texto extraído do documento para análise"),
    documentType: z.string().optional().describe("Tipo do documento (boleto, fatura, recibo)"),
  }),
  execute: async ({ extractedText, documentType }) => {
    return {
      instruction:
        "Analise o texto e sugira o fornecedor mais provável. Considere: razão social, CNPJ, nome fantasia, padrões conhecidos de faturas.",
      extractedText: extractedText.slice(0, 2000),
      documentType,
    };
  },
});

export const suggestCategoryTags = tool({
  description:
    "Sugere categoria principal e tags para uma transação baseado na descrição e fornecedor.",
  parameters: z.object({
    description: z.string().describe("Descrição da transação"),
    supplierName: z.string().optional().describe("Nome do fornecedor"),
    amount: z.number().optional().describe("Valor da transação"),
  }),
  execute: async ({ description, supplierName, amount }) => {
    return {
      instruction:
        "Sugira categoria principal e até 5 tags relevantes. Categorias possíveis: moradia, alimentação, transporte, saúde, educação, lazer, trabalho, financeiro, outros. Tags devem ser específicas.",
      description,
      supplierName,
      amount,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 3: Action Tools (require user confirmation)
// ══════════════════════════════════════════════════════════════

export const approveDraft = tool({
  description:
    "Aprova um draft, marcando como aceito. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft a aprovar"),
    corrections: z
      .record(z.string())
      .optional()
      .describe("Correções a aplicar antes de aprovar (campo: valor)"),
  }),
  execute: async ({ draftId, corrections }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    if (corrections && Object.keys(corrections).length > 0) {
      const [draft] = await db
        .select({ draft_data: schema.draft_records.draft_data })
        .from(schema.draft_records)
        .where(
          and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)),
        );

      if (draft) {
        const updatedData = {
          ...(draft.draft_data as Record<string, unknown>),
          ...corrections,
        };
        await db
          .update(schema.draft_records)
          .set({ draft_data: updatedData as Json, corrections, status: "corrected" })
          .where(
            and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)),
          );
      }
    }

    // Nota: o código atual não usa `.select().single()` neste update — só
    // checa `error`, nunca linhas afetadas. 0 linhas (draft inexistente ou
    // de outro usuário) hoje AINDA retorna `{success: true, message: "...
    // aprovado"}`. Comportamento pré-existente preservado por paridade
    // (exceção documentada à regra geral desta fase) — isolamento de DADOS
    // continua garantido, só a mensagem de retorno é otimista.
    await db
      .update(schema.draft_records)
      .set({ status: "approved" })
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    return {
      success: true,
      message: `Draft ${draftId} aprovado. Ainda NÃO foi lançado — use postDraft para criar o registro financeiro.`,
    };
  },
});

/**
 * Sem alteração — delega para materialization.ts (já reescrito na Task 13
 * do plano), sem chamada direta a supabase/Drizzle aqui.
 */
export const postDraft = tool({
  description:
    "Lança um draft JÁ APROVADO, criando o registro financeiro definitivo (transação, dívida, recorrência ou métrica). IMPORTANTE: só funciona em drafts aprovados; sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft aprovado a lançar"),
  }),
  execute: async ({ draftId }) => {
    const { postApprovedDraftRecord } = await import("@/app/actions/materialization");
    const result = await postApprovedDraftRecord(draftId);

    if (!result.success) {
      return {
        error: result.message,
        validationErrors: result.validationErrors,
      };
    }
    return {
      success: true,
      message: result.message,
      postedRecordId: result.postedRecordId,
      postedRecordType: result.postedRecordType,
    };
  },
});

export const rejectDraft = tool({
  description:
    "Rejeita um draft com motivo. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft a rejeitar"),
    reason: z.string().describe("Motivo da rejeição"),
  }),
  execute: async ({ draftId, reason }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    // Mesma exceção documentada de approveDraft: sem check de linhas
    // afetadas, paridade com o código atual.
    await db
      .update(schema.draft_records)
      .set({ status: "rejected", corrections: { rejection_reason: reason } })
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    return { success: true, message: `Draft ${draftId} rejeitado: ${reason}` };
  },
});

export const reprocessDocumentTool = tool({
  description:
    "Reprocessa um documento, criando novo job de ingestão. Útil quando o processamento original falhou ou dados mudaram.",
  parameters: z.object({
    documentId: z.string().uuid().describe("ID do documento a reprocessar"),
  }),
  execute: async ({ documentId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    // Bulk: 0 linhas é normal (nenhum draft pendente/rejeitado) — sem throw,
    // mesmo comportamento de hoje.
    await db
      .update(schema.draft_records)
      .set({ status: "archived" })
      .where(
        and(
          eq(schema.draft_records.source_document_id, documentId),
          eq(schema.draft_records.user_id, user.id),
          inArray(schema.draft_records.status, ["pending_review", "rejected"]),
        ),
      );

    const [latestJob] = await db
      .select({ id: schema.ingestion_jobs.id })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, documentId),
          eq(schema.ingestion_jobs.user_id, user.id),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at))
      .limit(1);

    if (latestJob) {
      await db
        .update(schema.ingestion_jobs)
        .set({ status: "queued" })
        .where(
          and(
            eq(schema.ingestion_jobs.id, latestJob.id),
            eq(schema.ingestion_jobs.user_id, user.id),
          ),
        );
    }

    return {
      success: true,
      message: `Documento ${documentId} enviado para reprocessamento.`,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 4: Pattern Tools (M5-006 — ADR-006)
// ══════════════════════════════════════════════════════════════

export const listDocumentPatterns = tool({
  description:
    "Lista padrões documentais cadastrados pelo usuário que orientam a extração e classificação automática. Mostra taxa de acerto e status.",
  parameters: z.object({
    documentType: z
      .string()
      .optional()
      .describe("Filtrar por tipo de documento (ex: conta_energia, boleto_generico)"),
    activeOnly: z.boolean().optional().default(true).describe("Retornar apenas padrões ativos"),
    limit: z.number().optional().default(20).describe("Máximo de padrões"),
  }),
  execute: async ({ documentType, activeOnly, limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.document_patterns.user_id, user.id)];
    if (activeOnly) conditions.push(eq(schema.document_patterns.is_active, true));
    if (documentType) conditions.push(eq(schema.document_patterns.document_type, documentType));

    const rows = await db
      .select({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
        is_active: schema.document_patterns.is_active,
        feedback_count: schema.document_patterns.feedback_count,
        success_count: schema.document_patterns.success_count,
        confidence_threshold: schema.document_patterns.confidence_threshold,
        updated_at: schema.document_patterns.updated_at,
      })
      .from(schema.document_patterns)
      .where(and(...conditions))
      .orderBy(desc(schema.document_patterns.version))
      .limit(limit);

    const patterns = rows.map((p) => ({
      ...p,
      successRate:
        p.feedback_count > 0 ? Math.round((p.success_count / p.feedback_count) * 100) : null,
    }));

    return { patterns, count: patterns.length };
  },
});

export const registerDocumentPattern = tool({
  description:
    "Cria um novo padrão documental que ensina o sistema a extrair e classificar um tipo específico de documento automaticamente.",
  parameters: z.object({
    name: z
      .string()
      .describe("Nome descritivo do padrão (ex: 'Conta CEMIG BH', 'Boleto Celular Vivo')"),
    documentType: z
      .string()
      .describe(
        "Tipo do documento (ex: conta_energia, boleto_generico, fatura_cartao, conta_agua, conta_telefone, outros)",
      ),
    extractionRules: z
      .record(z.union([z.string(), z.number(), z.boolean()]))
      .optional()
      .describe("Regras de extração (campo -> regex ou valor esperado)"),
    fieldMappings: z
      .record(
        z.union([
          z.string(),
          z.object({
            target: z.string(),
            transform: z.enum(["uppercase", "lowercase", "trim", "parse_number"]).optional(),
          }),
        ]),
      )
      .optional()
      .describe("Mapeamentos de campo (fonte -> destino ou { target, transform })"),
    confidenceThreshold: z
      .number()
      .min(0)
      .max(1)
      .optional()
      .default(0.7)
      .describe("Confiança mínima para aplicação automática (0.0-1.0)"),
    supplierId: z.string().uuid().optional().describe("ID do fornecedor associado (opcional)"),
    institutionId: z.string().uuid().optional().describe("ID da instituição associada (opcional)"),
  }),
  execute: async ({
    name,
    documentType,
    extractionRules,
    fieldMappings,
    confidenceThreshold,
    supplierId,
    institutionId,
  }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    // IDOR fechado nesta task: supplierId/institutionId vêm do cliente (a IA
    // repassa o que o usuário pediu) e, sem esta checagem, o padrão poderia
    // ser vinculado a um fornecedor/instituição de outro usuário. Mesmo
    // achado já corrigido em `app/actions/patterns.ts`
    // (`assertOwnsOptionalFks`), agora aplicado aqui também.
    const fkError = await findUnownedFkError(user.id, { supplierId, institutionId });
    if (fkError) return { error: fkError };

    const db = getDb();
    const [pattern] = await db
      .insert(schema.document_patterns)
      .values({
        user_id: user.id,
        name,
        document_type: documentType,
        extraction_rules: extractionRules ?? {},
        field_mappings: fieldMappings ?? {},
        confidence_threshold: confidenceThreshold,
        supplier_id: supplierId ?? null,
        institution_id: institutionId ?? null,
        version: 1,
        is_active: true,
        feedback_count: 0,
        success_count: 0,
      })
      .returning({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
      });

    return {
      success: true,
      pattern,
      message: `Padrão "${name}" criado com sucesso. ID: ${pattern?.id}`,
    };
  },
});

export const updateDocumentPattern = tool({
  description:
    "Atualiza regras de extração ou mapeamentos de um padrão documental existente. Cria nova versão automaticamente para preservar histórico.",
  parameters: z.object({
    patternId: z.string().uuid().describe("ID do padrão a atualizar"),
    extractionRules: z
      .record(z.union([z.string(), z.number(), z.boolean()]))
      .optional()
      .describe("Novas regras de extração"),
    fieldMappings: z
      .record(
        z.union([
          z.string(),
          z.object({
            target: z.string(),
            transform: z.enum(["uppercase", "lowercase", "trim", "parse_number"]).optional(),
          }),
        ]),
      )
      .optional()
      .describe("Novos mapeamentos de campo"),
    confidenceThreshold: z
      .number()
      .min(0)
      .max(1)
      .optional()
      .describe("Novo threshold de confiança"),
    isActive: z.boolean().optional().describe("Ativar ou desativar o padrão"),
  }),
  execute: async ({ patternId, extractionRules, fieldMappings, confidenceThreshold, isActive }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [existing] = await db
      .select({ id: schema.document_patterns.id, name: schema.document_patterns.name })
      .from(schema.document_patterns)
      .where(
        and(
          eq(schema.document_patterns.id, patternId),
          eq(schema.document_patterns.user_id, user.id),
        ),
      );

    if (!existing) return { error: "Padrão não encontrado" };

    const updates: Partial<typeof schema.document_patterns.$inferInsert> = {};
    if (extractionRules !== undefined) updates.extraction_rules = extractionRules;
    if (fieldMappings !== undefined) updates.field_mappings = fieldMappings;
    if (confidenceThreshold !== undefined) updates.confidence_threshold = confidenceThreshold;
    if (isActive !== undefined) updates.is_active = isActive;

    if (Object.keys(updates).length === 0) {
      return { error: "Nenhum campo para atualizar fornecido" };
    }

    // Este update DO CÓDIGO ATUAL usa `.select().single()` (diferente de
    // approveDraft/rejectDraft) — 0 linhas afetadas já produz erro hoje.
    // Preservado com `.returning()` + checagem de nulo.
    const [updated] = await db
      .update(schema.document_patterns)
      .set(updates)
      .where(
        and(
          eq(schema.document_patterns.id, patternId),
          eq(schema.document_patterns.user_id, user.id),
        ),
      )
      .returning({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
        is_active: schema.document_patterns.is_active,
      });

    if (!updated) return { error: "Padrão não encontrado" };
    return {
      success: true,
      pattern: updated,
      message: `Padrão "${existing.name}" atualizado.`,
    };
  },
});

export const suggestReconciliation = tool({
  description:
    "Sugere reconciliação entre um draft e transações existentes baseado em valor, data e fornecedor.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft para buscar candidatos de reconciliação"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({ draft_data: schema.draft_records.draft_data })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    const draftData = draft.draft_data as Record<string, unknown> | null;
    const amount = draftData?.amount ?? draftData?.valor;
    const date = draftData?.date ?? draftData?.data;

    if (!amount) {
      return { candidates: [], message: "Valor não encontrado no draft para reconciliação." };
    }

    // BUG CORRIGIDO nesta reescrita: o código atual pedia as colunas
    // `transaction_date` e `supplier_name`, que NÃO EXISTEM em `transactions`
    // (as reais são `event_date` e `supplier_id`, com nome vindo por join em
    // `suppliers`) — mesma classe de bug já corrigida em `listRecentTransactions`
    // dentro deste mesmo arquivo.
    const numAmount = Number(amount);
    const margin = numAmount * 0.05;

    const conditions = [
      eq(schema.transactions.user_id, user.id),
      gte(schema.transactions.amount, numAmount - margin),
      lte(schema.transactions.amount, numAmount + margin),
    ];

    if (date) {
      const dateMs = new Date(String(date)).getTime();
      if (!isNaN(dateMs)) {
        const sevenDaysBefore = new Date(dateMs - 7 * 24 * 60 * 60 * 1000)
          .toISOString()
          .split("T")[0]!;
        conditions.push(gte(schema.transactions.event_date, sevenDaysBefore));
      }
    }

    const candidates = await db
      .select({
        id: schema.transactions.id,
        description: schema.transactions.description,
        amount: schema.transactions.amount,
        event_date: schema.transactions.event_date,
        supplier_name: schema.suppliers.name,
      })
      .from(schema.transactions)
      .leftJoin(
        schema.suppliers,
        and(
          eq(schema.transactions.supplier_id, schema.suppliers.id),
          // Defesa em profundidade: mesmo padrão de cards.ts/listRecentTransactions.
          eq(schema.suppliers.user_id, user.id),
        ),
      )
      .where(and(...conditions))
      .orderBy(desc(schema.transactions.event_date))
      .limit(5);

    return { candidates, draftAmount: numAmount };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 5: Classification & Analysis Tools
// ══════════════════════════════════════════════════════════════

export const suggestDocumentType = tool({
  description:
    "Sugere o tipo de documento (boleto, fatura, recibo, comprovante, extrato, contrato) baseado no texto extraído.",
  parameters: z.object({
    extractedText: z.string().describe("Texto extraído do documento"),
    filename: z.string().optional().describe("Nome do arquivo original"),
  }),
  execute: async ({ extractedText, filename }) => {
    return {
      instruction:
        "Analise o texto e o nome do arquivo para classificar o tipo de documento. Tipos possíveis: boleto, fatura_cartao, fatura_servico, recibo, comprovante_pagamento, extrato_bancario, contrato, nota_fiscal, outro. Explique sua classificação.",
      extractedText: extractedText.slice(0, 2000),
      filename,
    };
  },
});

export const explainClassification = tool({
  description:
    "Explica por que um documento ou transação foi classificado de determinada forma. Útil para transparência e aprendizado.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft para explicar a classificação"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        corrections: schema.draft_records.corrections,
        source_document_id: schema.draft_records.source_document_id,
      })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    let sourceInfo = null;
    if (draft.source_document_id) {
      const [doc] = await db
        .select({
          filename: schema.source_documents.filename,
          mime_type: schema.source_documents.mime_type,
          origin_type: schema.source_documents.origin_type,
          metadata: schema.source_documents.metadata,
        })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, draft.source_document_id),
            eq(schema.source_documents.user_id, user.id),
          ),
        );
      sourceInfo = doc ?? null;
    }

    return {
      instruction:
        "Explique em linguagem acessível como este draft foi classificado, quais dados foram extraídos e qual a confiança. Se houver correções, mencione o que foi ajustado.",
      draft: {
        type: draft.draft_type,
        data: draft.draft_data,
        confidence: draft.confidence_score,
        corrections: draft.corrections,
      },
      source: sourceInfo,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 6: Specialized Query Tools
// ══════════════════════════════════════════════════════════════

export const listErrorDocuments = tool({
  description: "Lista documentos com erro de processamento, para diagnóstico e reprocessamento.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
  }),
  execute: async ({ limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const documents = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.user_id, user.id),
          eq(schema.source_documents.status, "error"),
        ),
      )
      .orderBy(desc(schema.source_documents.created_at))
      .limit(limit);

    return { documents, count: documents.length };
  },
});

export const listMissingPasswordDocuments = tool({
  description:
    "Lista documentos que falharam por falta de senha (PDF protegido), para que o usuário possa fornecer.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
  }),
  execute: async ({ limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const rows = await db
      .select({
        id: schema.ingestion_jobs.id,
        source_document_id: schema.ingestion_jobs.source_document_id,
        status: schema.ingestion_jobs.status,
        error_message: schema.ingestion_jobs.error_message,
        filename: schema.source_documents.filename,
      })
      .from(schema.ingestion_jobs)
      .innerJoin(
        schema.source_documents,
        eq(schema.ingestion_jobs.source_document_id, schema.source_documents.id),
      )
      .where(
        and(
          eq(schema.ingestion_jobs.user_id, user.id),
          eq(schema.source_documents.user_id, user.id),
          eq(schema.ingestion_jobs.status, "failed"),
          or(
            ilike(schema.ingestion_jobs.error_message, "%password%"),
            ilike(schema.ingestion_jobs.error_message, "%senha%"),
          ),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at))
      .limit(limit);

    return { documents: rows, count: rows.length };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 7: Batch Action Tools
// ══════════════════════════════════════════════════════════════

export const batchApproveDrafts = tool({
  description:
    "Aprova múltiplos drafts de uma vez. IMPORTANTE: SEMPRE liste os drafts primeiro e peça confirmação explícita do usuário antes de executar.",
  parameters: z.object({
    draftIds: z.array(z.string().uuid()).describe("Lista de IDs dos drafts a aprovar"),
  }),
  execute: async ({ draftIds }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    if (draftIds.length > 50) {
      return { error: "Máximo de 50 drafts por batch." };
    }

    const db = getDb();
    const results: { id: string; success: boolean; error?: string }[] = [];

    // Loop sequencial de updates independentes, sem transação — paridade
    // com o código atual (candidato a Apêndice B, não decidido aqui).
    for (const draftId of draftIds) {
      try {
        // Mesma exceção documentada de approveDraft/rejectDraft: sem check
        // de linhas afetadas.
        await db
          .update(schema.draft_records)
          .set({ status: "approved" })
          .where(
            and(
              eq(schema.draft_records.id, draftId),
              eq(schema.draft_records.user_id, user.id),
              eq(schema.draft_records.status, "pending_review"),
            ),
          );
        results.push({ id: draftId, success: true });
      } catch (err) {
        results.push({ id: draftId, success: false, error: (err as Error).message });
      }
    }

    const approved = results.filter((r) => r.success).length;
    const failed = results.filter((r) => !r.success).length;

    return {
      success: true,
      message: `Batch concluído: ${approved} aprovados, ${failed} falharam. Nenhum foi lançado — use postDraftBatch para criar os registros financeiros.`,
      details: results,
    };
  },
});

/**
 * Sem alteração — delega para materialization.ts (Task 13), sem chamada
 * direta a supabase/Drizzle aqui.
 */
export const postDraftBatch = tool({
  description:
    "Lança todos os drafts JÁ APROVADOS de um batch, criando os registros financeiros definitivos. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    batchId: z.string().uuid().describe("ID do batch a lançar"),
  }),
  execute: async ({ batchId }) => {
    const { postApprovedDraftBatch } = await import("@/app/actions/materialization");
    const result = await postApprovedDraftBatch(batchId);

    return {
      success: result.failed === 0,
      message: `Lançamento concluído: ${result.succeeded} lançados, ${result.failed} com problema.`,
      details: result.results.map((r) => ({
        id: r.draftRecordId,
        success: r.success,
        message: r.message,
        validationErrors: r.validationErrors,
      })),
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Sprint 4 Tools: suggest_splits, suggest_supplier_name, explain_extraction
// ══════════════════════════════════════════════════════════════

export const suggestSplits = tool({
  description:
    "Sugere como dividir (ratear) um documento de custo em múltiplas linhas de despesa. " +
    "Analisa o tipo do documento, valor total e histórico de padrões para propor splits.",
  parameters: z.object({
    document_id: z.string().describe("UUID do documento fonte"),
    total_amount: z
      .number()
      .optional()
      .describe("Valor total do documento (usado quando não disponível no banco)"),
    hint: z
      .string()
      .optional()
      .describe("Dica adicional do usuário sobre como dividir (ex: 50% pessoal, 50% empresa)"),
  }),
  execute: async ({ document_id, total_amount, hint }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [doc] = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        document_type: schema.source_documents.document_type,
        supplier_name_raw: schema.source_documents.supplier_name_raw,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.id, document_id),
          eq(schema.source_documents.user_id, user.id),
        ),
      );

    if (!doc) return { error: "Documento não encontrado" };

    const meta = doc.metadata as Record<string, unknown> | null;
    const amount = total_amount ?? (meta?.total_amount as number | null) ?? null;

    return {
      document_id,
      filename: doc.filename,
      document_type: doc.document_type,
      supplier_name_raw: doc.supplier_name_raw,
      total_amount: amount,
      hint: hint ?? null,
      suggestion:
        "Com base no tipo de documento e fornecedor, analise os splits mais prováveis. " +
        "Se for nota fiscal de serviço, considere rateio por centro de custo. " +
        "Se for fatura de utilidade, considere pessoal vs. profissional. " +
        "Retorne uma lista de splits sugeridos com percentual e descrição.",
      message: "Revise e ajuste os percentuais conforme necessário antes de confirmar o rateio.",
    };
  },
});

export const suggestSupplierName = tool({
  description:
    "Dado um CNPJ, sugere a razão social e possíveis aliases/nomes alternativos do fornecedor " +
    "com base no histórico de documentos ingeridos e padrões conhecidos.",
  parameters: z.object({
    document_number: z.string().describe("CNPJ ou CPF do fornecedor (apenas dígitos ou formatado)"),
  }),
  execute: async ({ document_number }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const digits = document_number.replace(/\D/g, "");

    const existing = await db
      .select({
        id: schema.suppliers.id,
        name: schema.suppliers.name,
        trade_name: schema.suppliers.trade_name,
        document_number: schema.suppliers.document_number,
      })
      .from(schema.suppliers)
      .where(
        and(
          eq(schema.suppliers.user_id, user.id),
          eq(schema.suppliers.document_number, document_number),
        ),
      )
      .limit(1);

    if (existing.length > 0) {
      const s = existing[0]!;
      return {
        found: true,
        supplier_id: s.id,
        name: s.name,
        trade_name: s.trade_name,
        document_number: s.document_number,
        aliases: [s.name, s.trade_name].filter(Boolean),
        message: "Fornecedor já cadastrado — dados recuperados do histórico.",
      };
    }

    const docs = await db
      .select({
        supplier_name_raw: schema.source_documents.supplier_name_raw,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.user_id, user.id),
          isNotNull(schema.source_documents.supplier_name_raw),
        ),
      )
      .limit(20);

    const found = docs.filter((d) => {
      const meta = d.metadata as Record<string, unknown> | null;
      const cnpj = (meta?.cnpj as string | null)?.replace(/\D/g, "");
      return cnpj === digits;
    });

    const namesFound = [
      ...new Set(found.map((d) => d.supplier_name_raw).filter(Boolean)),
    ] as string[];

    return {
      found: false,
      document_number,
      digits,
      suggested_names: namesFound,
      aliases: namesFound,
      message:
        namesFound.length > 0
          ? `Encontrados ${namesFound.length} nomes nos documentos ingeridos.`
          : "Nenhum histórico encontrado. Consulte a Receita Federal ou insira manualmente.",
    };
  },
});

export const explainExtraction = tool({
  description:
    "Explica por que um campo específico foi extraído com determinada confiança por OCR ou IA. " +
    "Analisa o documento e fornece contexto sobre a qualidade da extração.",
  parameters: z.object({
    document_id: z.string().describe("UUID do documento fonte"),
    field_name: z.string().describe("Nome do campo (ex: total_amount, supplier_name, due_date)"),
    extracted_value: z.string().optional().describe("Valor extraído (para verificação)"),
    confidence: z.number().optional().describe("Confiança reportada (0-1)"),
    source: z.string().optional().describe("Método de extração (ex: ocr, regex, gpt-4o)"),
  }),
  execute: async ({ document_id, field_name, extracted_value, confidence, source }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [doc] = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        document_type: schema.source_documents.document_type,
        metadata: schema.source_documents.metadata,
        status: schema.source_documents.status,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.id, document_id),
          eq(schema.source_documents.user_id, user.id),
        ),
      );

    if (!doc) return { error: "Documento não encontrado" };

    const meta = doc.metadata as Record<string, unknown> | null;
    const docConfidence = (meta?.confidence as number | null) ?? null;

    return {
      document_id,
      filename: doc.filename,
      document_type: doc.document_type,
      field_name,
      extracted_value: extracted_value ?? null,
      confidence: confidence ?? docConfidence,
      source: source ?? "desconhecido",
      context: {
        document_status: doc.status,
        overall_confidence: docConfidence,
      },
      explanation:
        `O campo "${field_name}" foi extraído via ${source ?? "método não informado"} ` +
        `com confiança ${confidence !== undefined ? Math.round(confidence * 100) + "%" : "não informada"}. ` +
        "Fatores que podem reduzir a confiança: qualidade do scan, layout não padronizado, " +
        "caracteres ambíguos (O/0, l/1), carimbo sobreposto, ou modelo de documento desconhecido. " +
        "Recomenda-se verificar visualmente o PDF e corrigir o valor se necessário.",
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Tool registry for the chat route
// ══════════════════════════════════════════════════════════════

export const sbfTools = {
  list_pending_documents: listPendingDocuments,
  get_document_details: getDocumentDetails,
  list_drafts: listDrafts,
  get_draft_details: getDraftDetails,
  list_recent_transactions: listRecentTransactions,
  suggest_supplier: suggestSupplier,
  suggest_category_tags: suggestCategoryTags,
  suggest_document_type: suggestDocumentType,
  explain_classification: explainClassification,
  approve_draft: approveDraft,
  post_draft: postDraft,
  reject_draft: rejectDraft,
  reprocess_document: reprocessDocumentTool,
  list_document_patterns: listDocumentPatterns,
  register_document_pattern: registerDocumentPattern,
  update_document_pattern: updateDocumentPattern,
  suggest_reconciliation: suggestReconciliation,
  list_error_documents: listErrorDocuments,
  list_missing_password_documents: listMissingPasswordDocuments,
  batch_approve_drafts: batchApproveDrafts,
  post_draft_batch: postDraftBatch,
  // Sprint 4
  suggest_splits: suggestSplits,
  suggest_supplier_name: suggestSupplierName,
  explain_extraction: explainExtraction,
};
