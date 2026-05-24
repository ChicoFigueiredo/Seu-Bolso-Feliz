/**
 * Type helpers for tables not yet in the generated Database types.
 * These will be removed once `generate-types.sh` is run after applying
 * the ai_chat_tables migration.
 */

import type { SupabaseClient } from "@supabase/supabase-js";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnySupabase = SupabaseClient<any, any, any>;

/**
 * Access a table that doesn't exist yet in the generated types.
 * Use this for new tables before types are regenerated.
 */

export function untypedFrom(supabase: AnySupabase, table: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (supabase as any).from(table);
}

/**
 * Call an RPC function that doesn't exist yet in the generated types.
 */

export function untypedRpc(supabase: AnySupabase, fn: string, params: Record<string, unknown>) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (supabase as any).rpc(fn, params);
}
