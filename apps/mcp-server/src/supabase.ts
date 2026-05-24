/**
 * @sbf/mcp-server — Supabase client factory
 * Cria um client Supabase usando service_role key (MCP server roda local, server-side).
 */
import { createClient, type SupabaseClient } from "@supabase/supabase-js";

let _client: SupabaseClient | null = null;

export function getSupabaseClient(): SupabaseClient {
  if (_client) return _client;

  const url = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!key) {
    throw new Error("SUPABASE_SERVICE_ROLE_KEY is required for MCP server");
  }

  _client = createClient(url, key, {
    auth: { autoRefreshToken: false, persistSession: false },
  });

  return _client;
}

export function getUserId(): string {
  const uid = process.env.LOCAL_USER_ID;
  if (!uid) throw new Error("LOCAL_USER_ID is required for MCP server");
  return uid;
}
