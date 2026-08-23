#!/usr/bin/env bun
/**
 * @sbf/cli — ponto de entrada único do pipeline de ingestão.
 *
 * Substitui workers/financial-evidence-worker, que era um wrapper de
 * spawnSync cujos cinco flags declarados não chegavam a lugar nenhum e cujo
 * --process não processava nada.
 */
import { parseArgs, CliError, HELP } from "./args";
import { runPipeline } from "./pipeline";

async function main(): Promise<void> {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    if (err instanceof CliError) {
      console.error(`❌ ${err.message}`);
      console.error("   Use --help para ver as opções disponíveis.");
      process.exit(2);
    }
    throw err;
  }

  if (opts.help) {
    console.log(HELP);
    return;
  }

  const stats = await runPipeline(opts);

  console.log("");
  console.log(`  Gmail:      ${stats.gmailDiscovered} documento(s)`);
  console.log(`  Pasta:      ${stats.localDiscovered} documento(s)`);
  console.log(`  Processados:${stats.jobsProcessed} job(s)`);
  for (const e of stats.errors) console.error(`  ⚠️  ${e}`);

  if (stats.errors.length > 0) process.exitCode = 1;
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  });
}

export { parseArgs, HELP } from "./args";
export { runPipeline, drainJobs } from "./pipeline";
