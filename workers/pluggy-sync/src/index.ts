/**
 * @sbf/worker-pluggy-sync — Entry point
 *
 * Sincroniza transações Pluggy de todas as conexões ativas do usuário.
 * Rodagem única (pensado pra cron na VPS, não poll loop contínuo — ver
 * Fase 5 do plano de integração Pluggy). Não faz backfill de 365 dias
 * (Fase 3, ainda não implementada): a janela padrão é curta, pra sync
 * incremental.
 *
 * Uso:
 *   bun run workers/pluggy-sync/src/index.ts
 *   bun run workers/pluggy-sync/src/index.ts --from 2026-01-01 --to 2026-08-24
 */
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import { getSupabaseClient } from "./supabase";
import { syncProviderConnection, type AccountMapping } from "./sync-runner";

export { syncProviderConnection } from "./sync-runner";

const DEFAULT_LOOKBACK_DAYS = 7;

interface CliOptions {
  from?: string;
  to?: string;
}

function parseArgs(): CliOptions {
  const args = process.argv.slice(2);
  const opts: CliOptions = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--from") opts.from = args[++i];
    if (args[i] === "--to") opts.to = args[++i];
  }
  return opts;
}

function isoDaysAgo(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() - days);
  return d.toISOString().split("T")[0]!;
}

async function main(): Promise<void> {
  const userId = process.env.LOCAL_USER_ID;
  if (!userId) {
    throw new Error("LOCAL_USER_ID é obrigatório (app de usuário único, ver ADR-007)");
  }

  const opts = parseArgs();
  const provider = PluggyProvider.fromEnv();
  const supabase = getSupabaseClient();

  const { data: connections, error } = await supabase
    .from("provider_connections")
    .select("id, external_item_id, last_synced_at")
    .eq("user_id", userId)
    .eq("provider", "pluggy")
    .eq("status", "active");

  if (error) throw new Error(`Falha ao listar provider_connections: ${error.message}`);
  if (!connections || connections.length === 0) {
    console.log("Nenhuma conexão Pluggy ativa.");
    return;
  }

  for (const connection of connections) {
    const { data: mappingRows, error: mappingError } = await supabase
      .from("external_account_mappings")
      .select("external_account_id, financial_product_id, status")
      .eq("provider_connection_id", connection.id as string);

    if (mappingError) {
      console.error(`Falha ao listar contas de ${connection.id}: ${mappingError.message}`);
      continue;
    }

    const mappings: AccountMapping[] = (mappingRows ?? []).map((m) => ({
      externalAccountId: m.external_account_id as string,
      financialProductId: m.financial_product_id as string | null,
      status: m.status as AccountMapping["status"],
    }));

    const from =
      opts.from ??
      (connection.last_synced_at as string | null)?.split("T")[0] ??
      isoDaysAgo(DEFAULT_LOOKBACK_DAYS);
    const to = opts.to ?? new Date().toISOString().split("T")[0]!;

    const result = await syncProviderConnection({
      supabase,
      userId,
      provider,
      externalItemId: connection.external_item_id as string,
      mappings,
      from,
      to,
    });

    console.log(
      `Conexão ${connection.id}: ${result.created} draft(s) criado(s), ` +
        `${result.skippedDuplicate} já sincronizado(s), ${result.skippedIgnored} ignorado(s).`,
    );

    await supabase
      .from("provider_connections")
      .update({ last_synced_at: new Date().toISOString() })
      .eq("id", connection.id as string);
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
