/**
 * Resolução de senhas de documentos protegidos.
 *
 * A versão anterior devolvia `encrypted_value` cru, que seguia direto para o
 * pdf-parse como se fosse a senha — `decrypt_secret` nunca teve um chamador.
 * Como nenhum escritor jamais chamou `encrypt_secret` tampouco, o valor era, na
 * prática, texto puro; a criptografia existia só no comentário da coluna.
 *
 * Agora a decriptação acontece dentro de `fn_get_secrets` (SECURITY DEFINER,
 * concedida apenas a service_role), e a chave nunca sai do banco.
 *
 * Sobre a estratégia de tentativa: no ponto do pipeline em que a senha é
 * necessária — dentro do catch de PdfPasswordRequiredError, ANTES de conseguir
 * ler o documento — o fornecedor genuinamente ainda não é conhecido. Passar
 * `supplierId` não é possível. Então tentamos os segredos do usuário em ordem
 * de uso recente, com teto; ao acertar, registramos o uso, e quando o parse
 * resolver o fornecedor a associação é gravada para o próximo documento
 * acertar de primeira. Operação local e limitada, sem oráculo, auto-corretiva.
 */
import type { SupabaseClient } from "@supabase/supabase-js";

export interface SecretMatch {
  secretId: string;
  /** Já decriptado. */
  value: string;
  entityType: string | null;
  entityId: string | null;
  contractIdentifier: string | null;
  label: string | null;
}

/** Teto de tentativas. Evita varrer dezenas de senhas por documento. */
const MAX_CANDIDATES = 20;

/**
 * Lista as senhas candidatas do usuário, já decriptadas, mais promissoras
 * primeiro. A ordenação por uso recente vem da própria RPC.
 */
export async function listPdfPasswordCandidates(
  supabase: SupabaseClient,
  userId: string,
  limit = MAX_CANDIDATES,
): Promise<SecretMatch[]> {
  const { data, error } = await supabase.rpc("fn_get_secrets", {
    p_user_id: userId,
    p_secret_type: "pdf_password",
    p_limit: limit,
  });

  if (error) {
    // Nunca propagar detalhe de segredo para o log do pipeline.
    throw new Error(`Falha ao resolver senhas: ${error.message}`);
  }

  return ((data ?? []) as Array<Record<string, unknown>>).map((row) => ({
    secretId: row.secret_id as string,
    value: row.value as string,
    entityType: (row.entity_type as string | null) ?? null,
    entityId: (row.entity_id as string | null) ?? null,
    contractIdentifier: (row.contract_identifier as string | null) ?? null,
    label: (row.label as string | null) ?? null,
  }));
}

/**
 * Compatibilidade: devolve a primeira candidata.
 *
 * Prefira `listPdfPasswordCandidates`, que permite tentar mais de uma. Quando
 * `supplierId` ou `contractIdentifier` são conhecidos, a candidata de escopo
 * correspondente é priorizada.
 */
export async function findPdfPassword(
  supabase: SupabaseClient,
  userId: string,
  supplierId?: string,
  contractIdentifier?: string,
): Promise<SecretMatch | null> {
  const candidates = await listPdfPasswordCandidates(supabase, userId);
  if (candidates.length === 0) return null;

  const scoped =
    candidates.find(
      (c) => supplierId && c.entityType === "supplier" && c.entityId === supplierId,
    ) ?? candidates.find((c) => contractIdentifier && c.contractIdentifier === contractIdentifier);

  return scoped ?? candidates[0]!;
}

/** Registra que esta senha funcionou, para que suba na ordem da próxima vez. */
export async function markSecretUsed(
  supabase: SupabaseClient,
  userId: string,
  secretId: string,
): Promise<void> {
  const { error } = await supabase.rpc("fn_mark_secret_used", {
    p_user_id: userId,
    p_secret_id: secretId,
  });
  // Falhar aqui é irrelevante para o resultado do parse; não vale derrubar o job.
  if (error) return;
}

/**
 * Associa uma senha que funcionou ao fornecedor recém-descoberto, para que o
 * próximo documento do mesmo emissor resolva na primeira tentativa.
 *
 * Usa a RPC de escrita, que criptografa. Nunca grava texto puro.
 */
export async function associateSecretToSupplier(
  supabase: SupabaseClient,
  plaintext: string,
  supplierId: string,
  label?: string,
): Promise<void> {
  const { error } = await supabase.rpc("fn_set_secret", {
    p_secret_type: "pdf_password",
    p_plaintext: plaintext,
    p_entity_type: "supplier",
    p_entity_id: supplierId,
    p_contract_identifier: null,
    p_label: label ?? null,
  });
  if (error) return;
}
