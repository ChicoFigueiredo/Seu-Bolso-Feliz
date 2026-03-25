/**
 * @sbf/worker-gmail-scanner — Orquestrador principal
 *
 * Escaneia a label "Comprovantes" do Gmail, baixa anexos,
 * faz upload para Supabase Storage e cria source_documents + ingestion_jobs.
 *
 * Uso:
 *   bun run workers/gmail-scanner/src/index.ts --label Comprovantes
 *   bun run workers/gmail-scanner/src/index.ts --label Comprovantes --dry-run
 *   bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 50
 *   bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 10 --dry-run
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";

import { createGmailClient, type GmailClient } from "./gmail-client";
import {
  processMessage,
  decodeBase64Url,
  type AttachmentInfo,
  type MessageMetadata,
} from "./message-processor";
import { getSupabaseClient } from "./supabase";

// ─── CLI Options ──────────────────────────────────────────────

interface ScanOptions {
  label: string;
  limit: number;
  dryRun: boolean;
  batchSize: number;
}

function parseArgs(): ScanOptions {
  const args = process.argv.slice(2);
  const options: ScanOptions = {
    label: "Comprovantes",
    limit: Infinity,
    dryRun: false,
    batchSize: 50,
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--label":
        options.label = args[++i] ?? "Comprovantes";
        break;
      case "--limit":
        options.limit = Number(args[++i]) || Infinity;
        break;
      case "--dry-run":
        options.dryRun = true;
        break;
      case "--batch-size":
        options.batchSize = Number(args[++i]) || 50;
        break;
      case "--help":
        console.log(`
Gmail Scanner — Escaneia label do Gmail e cria jobs de ingestão

Opções:
  --label <nome>       Nome da label do Gmail (padrão: Comprovantes)
  --limit <n>          Processar no máximo N mensagens (padrão: todas)
  --batch-size <n>     Tamanho do batch para listagem (padrão: 50)
  --dry-run            Apenas listar, não criar jobs nem fazer upload
  --help               Exibir esta ajuda
`);
        process.exit(0);
    }
  }

  return options;
}

// ─── Stats ────────────────────────────────────────────────────

interface ScanStats {
  messagesListed: number;
  messagesWithAttachments: number;
  attachmentsFound: number;
  attachmentsUploaded: number;
  jobsCreated: number;
  skippedAlreadyProcessed: number;
  errors: number;
}

function createStats(): ScanStats {
  return {
    messagesListed: 0,
    messagesWithAttachments: 0,
    attachmentsFound: 0,
    attachmentsUploaded: 0,
    jobsCreated: 0,
    skippedAlreadyProcessed: 0,
    errors: 0,
  };
}

// ─── Core ─────────────────────────────────────────────────────

function getUserId(): string {
  const uid = process.env.LOCAL_USER_ID;
  if (!uid) throw new Error("LOCAL_USER_ID is required for gmail-scanner");
  return uid;
}

/**
 * Verifica se um attachment (by origin_key) já foi processado.
 */
async function isAlreadyProcessed(
  supabase: SupabaseClient,
  userId: string,
  originKey: string,
): Promise<boolean> {
  const { data } = await supabase
    .from("source_documents")
    .select("id")
    .eq("user_id", userId)
    .eq("origin_type", SourceDocumentOrigin.GMAIL)
    .eq("origin_key", originKey)
    .limit(1);

  return (data?.length ?? 0) > 0;
}

/**
 * Faz download do attachment, upload para Storage, cria source_document e ingestion_job.
 */
async function processAttachment(
  gmail: GmailClient,
  supabase: SupabaseClient,
  userId: string,
  runId: string,
  metadata: MessageMetadata,
  attachment: AttachmentInfo,
  stats: ScanStats,
): Promise<void> {
  const originKey = buildOriginKey({
    type: "gmail",
    messageId: metadata.messageId,
    attachmentId: attachment.attachmentId,
  });

  // Idempotência: pular se já processado
  if (await isAlreadyProcessed(supabase, userId, originKey)) {
    stats.skippedAlreadyProcessed++;
    return;
  }

  // 1. Download do attachment via Gmail API
  const attachmentData = await gmail.getAttachment(metadata.messageId, attachment.attachmentId);
  const fileBuffer = decodeBase64Url(attachmentData.data);

  // 2. Upload para Supabase Storage
  const storagePath = `${userId}/${crypto.randomUUID()}/${attachment.filename}`;
  const { error: uploadError } = await supabase.storage
    .from("ingestion-originals")
    .upload(storagePath, fileBuffer, { contentType: attachment.mimeType });

  if (uploadError) {
    console.error(`  ❌ Upload failed: ${attachment.filename} — ${uploadError.message}`);
    stats.errors++;
    return;
  }

  stats.attachmentsUploaded++;

  // 3. Criar source_document
  const { data: doc, error: docError } = await supabase
    .from("source_documents")
    .insert({
      user_id: userId,
      origin_type: SourceDocumentOrigin.GMAIL,
      origin_key: originKey,
      gmail_message_id: metadata.messageId,
      gmail_thread_id: metadata.threadId,
      gmail_attachment_id: attachment.attachmentId,
      gmail_label: metadata.labelIds.join(","),
      gmail_date: metadata.date,
      gmail_from: metadata.from,
      gmail_subject: metadata.subject,
      filename: attachment.filename,
      mime_type: attachment.mimeType,
      file_size_bytes: fileBuffer.byteLength,
      storage_path: storagePath,
      status: "active",
    })
    .select("id")
    .single();

  if (docError || !doc) {
    console.error(`  ❌ source_document failed: ${attachment.filename} — ${docError?.message}`);
    stats.errors++;
    return;
  }

  // 4. Criar ingestion_job
  const { error: jobError } = await supabase.from("ingestion_jobs").insert({
    run_id: runId,
    user_id: userId,
    source_document_id: doc.id,
    status: IngestionJobStatus.DISCOVERED,
    metadata: {
      filename: attachment.filename,
      origin: "gmail_scanner",
      gmail_subject: metadata.subject,
      gmail_from: metadata.from,
      gmail_date: metadata.date,
    },
  });

  if (jobError) {
    console.error(`  ❌ ingestion_job failed: ${attachment.filename} — ${jobError.message}`);
    stats.errors++;
    return;
  }

  stats.jobsCreated++;
  console.log(`  ✅ ${attachment.filename} (${formatBytes(fileBuffer.byteLength)})`);
}

/**
 * Formata bytes em formato legível.
 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Executa o scan completo da label do Gmail.
 */
async function scanGmailLabel(options: ScanOptions): Promise<ScanStats> {
  const stats = createStats();
  const gmail = createGmailClient();
  const supabase = getSupabaseClient();
  const userId = getUserId();

  console.log(`\n📧 Gmail Scanner — Label: "${options.label}"`);
  console.log(`   Limit: ${options.limit === Infinity ? "todas" : options.limit}`);
  console.log(`   Dry run: ${options.dryRun ? "SIM" : "NÃO"}`);
  console.log("");

  // 1. Buscar label ID
  const labelId = await gmail.findLabelId(options.label);
  if (!labelId) {
    console.error(`❌ Label "${options.label}" não encontrada no Gmail.`);
    console.error("   Labels disponíveis:");
    const labels = await gmail.listLabels();
    for (const l of labels) {
      console.error(`     - ${l.name}`);
    }
    process.exit(1);
  }

  console.log(`✅ Label encontrada: "${options.label}" (ID: ${labelId})\n`);

  // 2. Criar ingestion_run (se não dry-run)
  let runId = "dry-run";
  if (!options.dryRun) {
    const { data: run, error: runError } = await supabase
      .from("ingestion_runs")
      .insert({
        user_id: userId,
        source_type: SourceDocumentOrigin.GMAIL,
        status: IngestionRunStatus.RUNNING,
        metadata: {
          label: options.label,
          label_id: labelId,
          limit: options.limit === Infinity ? null : options.limit,
        },
      })
      .select("id")
      .single();

    if (runError || !run) {
      console.error("❌ Falha ao criar ingestion_run:", runError?.message);
      process.exit(1);
    }
    runId = run.id;
    console.log(`📋 Run criado: ${runId}\n`);
  }

  // 3. Listar mensagens com paginação
  let pageToken: string | undefined;
  let totalProcessed = 0;

  try {
    do {
      const remaining = options.limit - totalProcessed;
      const fetchSize = Math.min(options.batchSize, remaining);

      if (fetchSize <= 0) break;

      const listResult = await gmail.listMessages(labelId, fetchSize, pageToken);

      if (!listResult.messages || listResult.messages.length === 0) {
        if (totalProcessed === 0) {
          console.log("📭 Nenhuma mensagem encontrada na label.");
        }
        break;
      }

      for (const msgRef of listResult.messages) {
        if (totalProcessed >= options.limit) break;

        stats.messagesListed++;
        totalProcessed++;

        // Buscar mensagem completa
        const message = await gmail.getMessage(msgRef.id);
        const processed = processMessage(message);

        if (processed.attachments.length === 0) {
          continue;
        }

        stats.messagesWithAttachments++;
        stats.attachmentsFound += processed.attachments.length;

        const { metadata, attachments } = processed;
        const dateStr = metadata.date ? new Date(metadata.date).toLocaleDateString("pt-BR") : "?";

        console.log(`📨 [${totalProcessed}] ${dateStr} — ${metadata.subject.substring(0, 60)}`);
        console.log(`   De: ${metadata.from.substring(0, 50)} | Anexos: ${attachments.length}`);

        if (options.dryRun) {
          for (const att of attachments) {
            console.log(`  📎 ${att.filename} (${att.mimeType}, ${formatBytes(att.size)})`);
          }
          continue;
        }

        // Processar cada attachment
        for (const attachment of attachments) {
          try {
            await processAttachment(gmail, supabase, userId, runId, metadata, attachment, stats);
          } catch (err) {
            stats.errors++;
            console.error(
              `  ❌ Erro processando ${attachment.filename}:`,
              err instanceof Error ? err.message : String(err),
            );
          }
        }
      }

      pageToken = listResult.nextPageToken;
    } while (pageToken && totalProcessed < options.limit);
  } finally {
    // 4. Finalizar run
    if (!options.dryRun && runId !== "dry-run") {
      await supabase
        .from("ingestion_runs")
        .update({
          status:
            stats.errors > 0 && stats.jobsCreated === 0
              ? IngestionRunStatus.FAILED
              : IngestionRunStatus.COMPLETED,
          completed_at: new Date().toISOString(),
          stats: { ...stats },
        })
        .eq("id", runId);
    }
  }

  return stats;
}

// ─── Main ─────────────────────────────────────────────────────

async function main(): Promise<void> {
  const options = parseArgs();

  const stats = await scanGmailLabel(options);

  // Resumo final
  console.log("\n" + "━".repeat(60));
  console.log("📊 Resumo do Scan");
  console.log("━".repeat(60));
  console.log(`   Mensagens listadas:         ${stats.messagesListed}`);
  console.log(`   Mensagens com anexos:       ${stats.messagesWithAttachments}`);
  console.log(`   Anexos encontrados:         ${stats.attachmentsFound}`);
  if (!options.dryRun) {
    console.log(`   Anexos enviados ao Storage: ${stats.attachmentsUploaded}`);
    console.log(`   Jobs de ingestão criados:   ${stats.jobsCreated}`);
  }
  console.log(`   Já processados (skip):      ${stats.skippedAlreadyProcessed}`);
  console.log(`   Erros:                      ${stats.errors}`);
  console.log("━".repeat(60) + "\n");

  if (stats.errors > 0) {
    process.exit(1);
  }
}

if (import.meta.main) {
  main();
}

export { scanGmailLabel, type ScanOptions, type ScanStats };
