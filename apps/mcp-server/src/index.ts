/**
 * @sbf/mcp-server — MCP server local para operação assistida via Copilot/VS Code
 *
 * Expõe tools operacionais do domínio financeiro para uso no VS Code/GitHub Copilot.
 * Roda via stdio transport — o VS Code lança o processo e comunica via stdin/stdout.
 */
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

import { getSupabaseClient, getUserId } from "./supabase.js";
import { scanLocalFolder } from "./tools/scan-local-folder.js";
import { listUnparsedDocuments } from "./tools/list-unparsed-documents.js";
import { reprocessDocument } from "./tools/reprocess-document.js";
import { resolveSupplierCandidates } from "./tools/resolve-supplier-candidates.js";
import { listDraftBatches } from "./tools/list-draft-batches.js";
import { approveDraftBatch } from "./tools/approve-draft-batch.js";
import { findDocumentsWithoutPassword } from "./tools/find-documents-without-password.js";
import { recomputeFinancialPeriods } from "./tools/recompute-financial-periods.js";
import { ingestDocument } from "./tools/ingest-document.js";

const server = new McpServer({
  name: "sbf-mcp-server",
  version: "0.1.0",
});

// ---------------------------------------------------------------------------
// Tool: scan_local_folder
// ---------------------------------------------------------------------------
server.tool(
  "scan_local_folder",
  "Escaneia um diretório local por documentos financeiros (PDF, imagens, CSV, XLS, OFX) e cria jobs de ingestão no Supabase. Retorna contagem de descobertos, ignorados e erros.",
  { dirPath: z.string().describe("Caminho absoluto do diretório a escanear") },
  async ({ dirPath }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const result = await scanLocalFolder(supabase, userId, dirPath);
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: list_unparsed_documents
// ---------------------------------------------------------------------------
server.tool(
  "list_unparsed_documents",
  "Lista documentos que ainda não foram processados (status: discovered, downloaded, hashed, queued). Opcionalmente inclui documentos com erro.",
  {
    limit: z.number().optional().describe("Máximo de resultados (padrão: 50)"),
    includeErrors: z.boolean().optional().describe("Incluir documentos com status FAILED"),
  },
  async ({ limit, includeErrors }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const docs = await listUnparsedDocuments(supabase, userId, {
      limit,
      includeErrors,
    });
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(docs, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: reprocess_document
// ---------------------------------------------------------------------------
server.tool(
  "reprocess_document",
  "Força reprocessamento de um documento específico, criando nova run e novo job com force_reprocess=true.",
  {
    documentId: z.string().uuid().describe("UUID do source_document a reprocessar"),
  },
  async ({ documentId }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const result = await reprocessDocument(supabase, userId, documentId);
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: resolve_supplier_candidates
// ---------------------------------------------------------------------------
server.tool(
  "resolve_supplier_candidates",
  "Busca fornecedores por CNPJ exato e/ou similaridade de nome. Útil para vincular documentos a fornecedores existentes.",
  {
    text: z.string().optional().describe("Texto parcial do nome do fornecedor"),
    cnpj: z.string().optional().describe("CNPJ exato (apenas dígitos)"),
  },
  async ({ text, cnpj }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const candidates = await resolveSupplierCandidates(supabase, userId, {
      text,
      cnpj,
    });
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(candidates, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: list_draft_batches
// ---------------------------------------------------------------------------
server.tool(
  "list_draft_batches",
  "Lista batches de drafts pendentes de revisão, com detalhes dos drafts individuais (tipo, status, confiança, descrição sugerida).",
  {
    status: z
      .string()
      .optional()
      .describe("Filtrar por status do batch (open, reviewing, approved, partial, rejected)"),
    limit: z.number().optional().describe("Máximo de resultados (padrão: 20)"),
  },
  async ({ status, limit }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const batches = await listDraftBatches(supabase, userId, { status, limit });
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(batches, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: approve_draft_batch
// ---------------------------------------------------------------------------
server.tool(
  "approve_draft_batch",
  "Aprova um batch de drafts inteiro. Opcionalmente rejeita drafts específicos por ID (os demais são aprovados). Atualiza status do batch.",
  {
    batchId: z.string().uuid().describe("UUID do draft_batch a aprovar"),
    rejectDraftIds: z
      .array(z.string().uuid())
      .optional()
      .describe("Lista de UUIDs de draft_records específicos a rejeitar neste batch"),
  },
  async ({ batchId, rejectDraftIds }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const result = await approveDraftBatch(supabase, userId, batchId, {
      rejectDraftIds,
    });
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: find_documents_without_password
// ---------------------------------------------------------------------------
server.tool(
  "find_documents_without_password",
  "Encontra documentos PDF que falharam no processamento por falta de senha. Útil para identificar quais PDFs protegidos precisam de perfil de senha.",
  {},
  async () => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const docs = await findDocumentsWithoutPassword(supabase, userId);
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(docs, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: recompute_financial_periods
// ---------------------------------------------------------------------------
server.tool(
  "recompute_financial_periods",
  "Gera ou atualiza os períodos financeiros do usuário com base no dia de início do ciclo configurado nas preferências. Usa a RPC generate_financial_periods do Supabase.",
  {
    startDay: z
      .number()
      .min(1)
      .max(31)
      .optional()
      .describe(
        "Dia de início do ciclo (1-31). Se omitido, usa o valor salvo nas preferências do usuário.",
      ),
    monthsAhead: z
      .number()
      .min(1)
      .max(60)
      .optional()
      .describe("Quantos meses à frente gerar (padrão: 12)"),
  },
  async ({ startDay, monthsAhead }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const result = await recomputeFinancialPeriods(supabase, userId, {
      startDay,
      monthsAhead,
    });
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Tool: ingest_document
// ---------------------------------------------------------------------------
server.tool(
  "ingest_document",
  "Dispara ingestão (ou reprocessamento com IA) de um documento financeiro. Use aiMode='full' para análise com visão (gpt-4o), 'lite' para enriquecimento rápido (gpt-4o-mini), 'skip' para apenas parsing determinístico, ou 'auto' (padrão) para decidir automaticamente pela confiança.",
  {
    documentId: z.string().uuid().describe("ID (UUID) do documento em source_documents"),
    aiMode: z
      .enum(["auto", "lite", "full", "skip"])
      .optional()
      .default("auto")
      .describe("Modo de IA: auto | lite | full | skip"),
  },
  async ({ documentId, aiMode }) => {
    const supabase = getSupabaseClient();
    const userId = getUserId();
    const result = await ingestDocument(supabase, userId, documentId, aiMode ?? "auto");
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  },
);

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("[sbf-mcp-server] Running on stdio");
}

main().catch((err) => {
  console.error("[sbf-mcp-server] Fatal error:", err);
  process.exit(1);
});
