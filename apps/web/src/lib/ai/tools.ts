/**
 * AI function calling tool definitions for the SBF chat assistant.
 * Each tool maps to a real operation in the ingestion/financial domain.
 */

import { tool } from "ai";
import { z } from "zod";
import { createClient } from "@/lib/supabase/server";
import type { Json } from "@/../../packages/shared-types/src/database.types";

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

    let query = supabase
      .from("source_documents")
      .select("id, filename, mime_type, origin_type, status, created_at")
      .eq("user_id", user.id)
      .order("created_at", { ascending: false })
      .limit(limit);

    if (status) query = query.eq("status", status);

    const { data, error } = await query;
    if (error) return { error: error.message };
    return { documents: data, count: data?.length ?? 0 };
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

    const [docResult, jobsResult, draftsResult] = await Promise.all([
      supabase
        .from("source_documents")
        .select("*")
        .eq("id", documentId)
        .eq("user_id", user.id)
        .single(),
      supabase
        .from("ingestion_jobs")
        .select("id, status, step, error_message, created_at, updated_at")
        .eq("source_document_id", documentId)
        .order("created_at", { ascending: false }),
      supabase
        .from("draft_records")
        .select("id, status, draft_type, draft_data, confidence_score, created_at")
        .eq("source_document_id", documentId)
        .order("created_at", { ascending: false }),
    ]);

    if (docResult.error) return { error: docResult.error.message };
    return {
      document: docResult.data,
      jobs: jobsResult.data ?? [],
      drafts: draftsResult.data ?? [],
    };
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

    const { data, error } = await supabase
      .from("draft_records")
      .select(
        "id, status, draft_type, draft_data, confidence_score, source_document_id, created_at",
      )
      .eq("user_id", user.id)
      .eq("status", status)
      .order("created_at", { ascending: false })
      .limit(limit);

    if (error) return { error: error.message };
    return { drafts: data, count: data?.length ?? 0 };
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

    const { data, error } = await supabase
      .from("draft_records")
      .select(
        "id, status, draft_type, draft_data, confidence_score, corrections, source_document_id, created_at, updated_at",
      )
      .eq("id", draftId)
      .eq("user_id", user.id)
      .single();

    if (error) return { error: error.message };

    // Fetch source document info separately
    let sourceDoc = null;
    if (data?.source_document_id) {
      const { data: doc } = await supabase
        .from("source_documents")
        .select("filename, mime_type, origin_type")
        .eq("id", data.source_document_id)
        .single();
      sourceDoc = doc;
    }

    return { draft: { ...data, source_document: sourceDoc } };
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

    let query = supabase
      .from("transactions")
      .select("id, description, amount, transaction_date, category, supplier_name, created_at")
      .eq("user_id", user.id)
      .order("transaction_date", { ascending: false })
      .limit(limit);

    if (category) query = query.eq("category", category);

    const { data, error } = await query;
    if (error) return { error: error.message };
    return { transactions: data, count: data?.length ?? 0 };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 2: Analysis Tools
// ══════════════════════════════════════════════════════════════

export const suggestSupplier = tool({
  description: "Sugere o fornecedor/empresa de um documento baseado no texto extraído.",
  parameters: z.object({
    extractedText: z.string().describe("Texto extraído do documento para análise"),
    documentType: z.string().optional().describe("Tipo do documento (boleto, fatura, recibo)"),
  }),
  execute: async ({ extractedText, documentType }) => {
    // This tool returns suggestions for the AI to use in its response
    // The AI model itself does the analysis based on the text
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

    // Apply corrections if any
    if (corrections && Object.keys(corrections).length > 0) {
      const { data: draft } = await supabase
        .from("draft_records")
        .select("draft_data")
        .eq("id", draftId)
        .eq("user_id", user.id)
        .single();

      if (draft) {
        const updatedData = {
          ...(draft.draft_data as Record<string, unknown>),
          ...corrections,
        };
        await supabase
          .from("draft_records")
          .update({
            draft_data: updatedData as Json,
            corrections: corrections,
            status: "corrected" as const,
          })
          .eq("id", draftId)
          .eq("user_id", user.id);
      }
    }

    const { error } = await supabase
      .from("draft_records")
      .update({ status: "approved" as const })
      .eq("id", draftId)
      .eq("user_id", user.id);

    if (error) return { error: error.message };
    return { success: true, message: `Draft ${draftId} aprovado.` };
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

    const { error } = await supabase
      .from("draft_records")
      .update({
        status: "rejected" as const,
        corrections: { rejection_reason: reason },
      })
      .eq("id", draftId)
      .eq("user_id", user.id);

    if (error) return { error: error.message };
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

    // Archive pending/rejected drafts
    await supabase
      .from("draft_records")
      .update({ status: "archived" as const })
      .eq("source_document_id", documentId)
      .eq("user_id", user.id)
      .in("status", ["pending_review", "rejected"]);

    // Reset latest job to queued
    const { data: jobs } = await supabase
      .from("ingestion_jobs")
      .select("id")
      .eq("source_document_id", documentId)
      .order("created_at", { ascending: false })
      .limit(1);

    if (jobs && jobs.length > 0 && jobs[0]) {
      await supabase
        .from("ingestion_jobs")
        .update({ status: "queued" as const, step: "hash" })
        .eq("id", jobs[0].id);
    }

    return {
      success: true,
      message: `Documento ${documentId} enviado para reprocessamento.`,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 4: Pattern Tools
// ══════════════════════════════════════════════════════════════

export const listPatterns = tool({
  description: "Lista padrões documentais registrados que ajudam na classificação automática.",
  parameters: z.object({
    active: z.boolean().optional().default(true).describe("Filtrar apenas padrões ativos"),
  }),
  execute: async () => {
    // document_patterns table will be created in Marco 5
    return {
      patterns: [],
      message: "Padrões documentais ainda não implementados. Será disponível no Marco 5.",
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

    const { data: draft, error: draftError } = await supabase
      .from("draft_records")
      .select("draft_data")
      .eq("id", draftId)
      .eq("user_id", user.id)
      .single();

    if (draftError || !draft) return { error: draftError?.message ?? "Draft não encontrado" };

    const draftData = draft.draft_data as Record<string, unknown> | null;
    const amount = draftData?.amount ?? draftData?.valor;
    const date = draftData?.date ?? draftData?.data;

    if (!amount)
      return {
        candidates: [],
        message: "Valor não encontrado no draft para reconciliação.",
      };

    // Search for transactions with similar amount (±5%)
    const numAmount = Number(amount);
    const margin = numAmount * 0.05;

    let query = supabase
      .from("transactions")
      .select("id, description, amount, transaction_date, supplier_name")
      .eq("user_id", user.id)
      .gte("amount", numAmount - margin)
      .lte("amount", numAmount + margin)
      .order("transaction_date", { ascending: false })
      .limit(5);

    if (date) {
      const dateStr = String(date);
      const dateMs = new Date(dateStr).getTime();
      if (!isNaN(dateMs)) {
        query = query.gte(
          "transaction_date",
          new Date(dateMs - 7 * 24 * 60 * 60 * 1000).toISOString(),
        );
      }
    }

    const { data: candidates, error } = await query;
    if (error) return { error: error.message };
    return { candidates: candidates ?? [], draftAmount: numAmount };
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
  approve_draft: approveDraft,
  reject_draft: rejectDraft,
  reprocess_document: reprocessDocumentTool,
  list_patterns: listPatterns,
  suggest_reconciliation: suggestReconciliation,
};
