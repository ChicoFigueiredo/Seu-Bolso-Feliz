/**
 * Busca de senhas de documentos protegidos no Supabase user_secrets.
 * Opera server-side (service_role) para acessar segredos encriptados.
 */
import type { SupabaseClient } from "@supabase/supabase-js";

export interface SecretMatch {
  secretId: string;
  value: string;
}

/**
 * Busca senha de PDF por fornecedor (supplier_id) na tabela user_secrets.
 * Retorna null se não encontrar.
 */
export async function findPdfPassword(
  supabase: SupabaseClient,
  userId: string,
  supplierId?: string,
  contractIdentifier?: string,
): Promise<SecretMatch | null> {
  // Busca por entidade específica (fornecedor + contrato)
  if (supplierId) {
    const { data } = await supabase
      .from("user_secrets")
      .select("id, encrypted_value")
      .eq("user_id", userId)
      .eq("secret_type", "pdf_password")
      .eq("entity_type", "supplier")
      .eq("entity_id", supplierId)
      .limit(1)
      .single();

    if (data) {
      return { secretId: data.id, value: data.encrypted_value };
    }
  }

  // Busca genérica por contrato (sem supplier vinculado)
  if (contractIdentifier) {
    const { data: secrets } = await supabase
      .from("user_secrets")
      .select("id, encrypted_value")
      .eq("user_id", userId)
      .eq("secret_type", "pdf_password")
      .is("entity_id", null);

    if (secrets) {
      // Procurar no metadata se há match por contrato
      // (futuro: campo contract_identifier na tabela)
      for (const secret of secrets) {
        return { secretId: secret.id, value: secret.encrypted_value };
      }
    }
  }

  // Fallback: buscar qualquer pdf_password genérica do usuário
  const { data: fallback } = await supabase
    .from("user_secrets")
    .select("id, encrypted_value")
    .eq("user_id", userId)
    .eq("secret_type", "pdf_password")
    .is("entity_type", null)
    .limit(1)
    .single();

  if (fallback) {
    return { secretId: fallback.id, value: fallback.encrypted_value };
  }

  return null;
}
