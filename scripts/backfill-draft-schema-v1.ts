/**
 * Backfill de draft_records.draft_data: v0 → v1.
 *
 * Deliberadamente TypeScript, e não plpgsql: a conversão v0→v1 precisa ser
 * byte-idêntica à que `parseDraftPayload` faz em runtime. Uma segunda cópia da
 * regra em plpgsql divergiria com o tempo, e uma divergência aqui corromperia
 * drafts históricos em silêncio.
 *
 * Propriedades:
 *   - idempotente: reexecutar não altera linhas já em v1;
 *   - preserva o original em draft_data_legacy antes de sobrescrever;
 *   - NÃO toca linhas posted/rejected/archived — histórico é imutável;
 *   - processa em lotes, para não segurar uma transação longa.
 *
 * Uso:
 *   bun run scripts/backfill-draft-schema-v1.ts [--dry-run] [--batch-size 200]
 */
import { createClient } from "@supabase/supabase-js";
import { DRAFT_SCHEMA_VERSION, migrateV0ToV1, DRAFT_SCHEMAS, type DraftType } from "@sbf/contracts";

const MUTABLE_STATUSES = ["pending_review", "approved", "corrected"];

interface Options {
  dryRun: boolean;
  batchSize: number;
}

function parseArgs(argv: string[]): Options {
  const opts: Options = { dryRun: false, batchSize: 200 };
  for (let i = 0; i < argv.length; i++) {
    switch (argv[i]) {
      case "--dry-run":
        opts.dryRun = true;
        break;
      case "--batch-size":
        opts.batchSize = Number(argv[++i]) || 200;
        break;
      case "--help":
        console.log(
          "uso: bun run scripts/backfill-draft-schema-v1.ts [--dry-run] [--batch-size N]",
        );
        process.exit(0);
        break;
      default:
        // Um flag desconhecido silenciosamente ignorado é a classe de bug que
        // custou caro na orquestração. Falhar alto.
        throw new Error(`Flag desconhecida: ${argv[i]}`);
    }
  }
  return opts;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));

  const url = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_SECRET_KEY;
  if (!url || !key) throw new Error("SUPABASE_URL e SUPABASE_SECRET_KEY são obrigatórios");

  const supabase = createClient(url, key, {
    auth: { autoRefreshToken: false, persistSession: false },
  });

  let migrated = 0;
  let failed = 0;
  let scanned = 0;

  for (;;) {
    const { data: rows, error } = await supabase
      .from("draft_records")
      .select("id, draft_type, draft_data, draft_schema_version, status")
      .eq("draft_schema_version", 0)
      .in("status", MUTABLE_STATUSES)
      .order("created_at", { ascending: true })
      .limit(opts.batchSize);

    if (error) throw new Error(`Falha ao ler draft_records: ${error.message}`);
    if (!rows || rows.length === 0) break;

    for (const row of rows) {
      scanned++;
      const draftType = row.draft_type as DraftType;
      const schema = DRAFT_SCHEMAS[draftType];
      if (!schema) {
        console.warn(`  ! ${row.id}: draft_type desconhecido "${row.draft_type}", pulando`);
        failed++;
        continue;
      }

      const v1 = migrateV0ToV1(draftType, row.draft_data);
      const check = schema.safeParse(v1);
      if (!check.success) {
        console.warn(
          `  ! ${row.id}: migração não validou — ${check.error.errors
            .map((e) => `${e.path.join(".")}: ${e.message}`)
            .join("; ")}`,
        );
        failed++;
        continue;
      }

      if (opts.dryRun) {
        migrated++;
        continue;
      }

      const { error: upErr } = await supabase
        .from("draft_records")
        .update({
          draft_data_legacy: row.draft_data,
          draft_data: v1,
          draft_schema_version: DRAFT_SCHEMA_VERSION,
        })
        .eq("id", row.id)
        .eq("draft_schema_version", 0); // guarda contra corrida com outra execução

      if (upErr) {
        console.warn(`  ! ${row.id}: falha ao gravar — ${upErr.message}`);
        failed++;
        continue;
      }
      migrated++;
    }

    // Em dry-run nada muda, então a próxima página seria a mesma: parar.
    if (opts.dryRun) break;
  }

  console.log(
    `${opts.dryRun ? "[DRY RUN] " : ""}analisados ${scanned}, migrados ${migrated}, falhas ${failed}`,
  );
  if (failed > 0) process.exitCode = 1;
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  });
}
