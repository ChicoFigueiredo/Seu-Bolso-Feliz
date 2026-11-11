/**
 * @sbf/worker-pluggy-sync — Entry point
 *
 * Sincroniza transações Pluggy de todas as conexões ativas do usuário.
 * Rodagem única (pensado pra cron na VPS, não poll loop contínuo — ver
 * Fase 5 do plano de integração Pluggy).
 *
 * Dois modos, mesma função (syncProviderConnection) — só a janela e o uso
 * de checkpoint mudam:
 *   - incremental (padrão): janela curta (7 dias, ou desde last_synced_at),
 *     sem checkpoint — poucas páginas, last_synced_at já é retomável o
 *     suficiente.
 *   - --backfill: janela de 365 dias por padrão, com checkpoint em
 *     ingestion_checkpoints por conta — retomável entre execuções (Fase 3).
 *
 * Uso:
 *   bun run workers/pluggy-sync/src/index.ts
 *   bun run workers/pluggy-sync/src/index.ts --from 2026-01-01 --to 2026-08-24
 *   bun run workers/pluggy-sync/src/index.ts --backfill
 *   bun run workers/pluggy-sync/src/index.ts --backfill --from 2025-08-24
 */
import { and, eq } from "drizzle-orm";
import { getDb, resolveOwnerId, schema } from "@sbf/db";
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import { syncProviderConnection, type AccountMapping } from "./sync-runner";
import { planejarAtualizacaoDeSaldos } from "./saldos";

export { syncProviderConnection } from "./sync-runner";

const DEFAULT_LOOKBACK_DAYS = 7;
const BACKFILL_LOOKBACK_DAYS = 365;

interface CliOptions {
  from?: string;
  to?: string;
  backfill: boolean;
}

function parseArgs(): CliOptions {
  const args = process.argv.slice(2);
  const opts: CliOptions = { backfill: false };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--from") opts.from = args[++i];
    if (args[i] === "--to") opts.to = args[++i];
    if (args[i] === "--backfill") opts.backfill = true;
  }
  return opts;
}

function isoDaysAgo(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() - days);
  return d.toISOString().split("T")[0]!;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

async function main(): Promise<void> {
  // O dono sai do banco, não de um UUID copiado à mão para o `.env`: um valor
  // velho ali faria o backfill gravar centenas de linhas em nome de um usuário
  // inexistente, sem erro nenhum, e a tela ficaria vazia. Ver @sbf/db/owner.
  const userId = await resolveOwnerId();

  const opts = parseArgs();
  const provider = PluggyProvider.fromEnv();
  const db = getDb();

  let connections: {
    id: string;
    external_item_id: string;
    last_synced_at: string | null;
  }[];
  try {
    connections = await db
      .select({
        id: schema.provider_connections.id,
        external_item_id: schema.provider_connections.external_item_id,
        last_synced_at: schema.provider_connections.last_synced_at,
      })
      .from(schema.provider_connections)
      .where(
        and(
          eq(schema.provider_connections.user_id, userId),
          eq(schema.provider_connections.provider, "pluggy"),
          eq(schema.provider_connections.status, "active"),
        ),
      );
  } catch (err) {
    throw new Error(`Falha ao listar provider_connections: ${errorMessage(err)}`, { cause: err });
  }

  if (connections.length === 0) {
    console.log("Nenhuma conexão Pluggy ativa.");
    return;
  }

  for (const connection of connections) {
    let mappingRows: {
      external_account_id: string;
      financial_product_id: string | null;
      status: string;
    }[];
    try {
      mappingRows = await db
        .select({
          external_account_id: schema.external_account_mappings.external_account_id,
          financial_product_id: schema.external_account_mappings.financial_product_id,
          status: schema.external_account_mappings.status,
        })
        .from(schema.external_account_mappings)
        .where(eq(schema.external_account_mappings.provider_connection_id, connection.id));
    } catch (err) {
      console.error(`Falha ao listar contas de ${connection.id}: ${errorMessage(err)}`);
      continue;
    }

    const mappings: AccountMapping[] = mappingRows.map((m) => ({
      externalAccountId: m.external_account_id,
      financialProductId: m.financial_product_id,
      status: m.status as AccountMapping["status"],
    }));

    const from =
      opts.from ??
      (opts.backfill
        ? isoDaysAgo(BACKFILL_LOOKBACK_DAYS)
        : (connection.last_synced_at?.split("T")[0] ?? isoDaysAgo(DEFAULT_LOOKBACK_DAYS)));
    const to = opts.to ?? new Date().toISOString().split("T")[0]!;

    const result = await syncProviderConnection({
      db,
      userId,
      provider,
      externalItemId: connection.external_item_id,
      mappings,
      from,
      to,
      mode: opts.backfill ? "backfill" : "incremental",
    });

    console.log(
      `Conexão ${connection.id}: ${result.created} draft(s) criado(s), ` +
        `${result.skippedDuplicate} já sincronizado(s), ${result.skippedIgnored} ignorado(s), ` +
        `${result.itemErrors} erro(s).`,
    );

    // Saldo é posição, não fluxo: as transações importadas acima nunca o
    // atualizam, então toda sincronização termina espelhando o saldo que o
    // banco declarou. Falha aqui não pode derrubar a sincronização — as
    // transações já estão gravadas e valem sozinhas.
    try {
      const contas = await provider.listAccounts(connection.external_item_id);
      const plano = planejarAtualizacaoDeSaldos(contas, mappings);
      for (const atualizacao of plano) {
        await db
          .update(schema.financial_products)
          .set({ current_balance: atualizacao.saldo, updated_at: new Date().toISOString() })
          .where(
            and(
              eq(schema.financial_products.id, atualizacao.financialProductId),
              eq(schema.financial_products.user_id, userId),
            ),
          );
      }
      if (plano.length > 0) {
        console.log(`Conexão ${connection.id}: saldo atualizado em ${plano.length} conta(s).`);
      }
    } catch (err) {
      console.error(`Falha ao atualizar saldos de ${connection.id}: ${errorMessage(err)}`);
    }

    await db
      .update(schema.provider_connections)
      .set({ last_synced_at: new Date().toISOString() })
      .where(eq(schema.provider_connections.id, connection.id));
  }
}

// Só roda quando este arquivo É o programa. Sem a guarda, qualquer `import`
// deste módulo — um teste, uma ferramenta, um autocompletar de editor que
// resolva o módulo — dispara uma sincronização de verdade contra a Pluggy e
// grava no banco. Já era assim antes; é o tipo de coisa que só se descobre
// depois de acontecer.
if (import.meta.main) {
  main().catch((err) => {
    console.error(errorMessage(err));
    process.exit(1);
  });
}
