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
import { sql } from "drizzle-orm";
import { gravarSegredo, type Database } from "@sbf/db";

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
  db: Database,
  userId: string,
  limit = MAX_CANDIDATES,
): Promise<SecretMatch[]> {
  let rows: Array<Record<string, unknown>>;
  try {
    const result = await db.execute(
      sql`select * from public.fn_get_secrets(${userId}::uuid, 'pdf_password', ${limit}::int)`,
    );
    rows = (result.rows ?? []) as Array<Record<string, unknown>>;
  } catch (err) {
    // Nunca propagar detalhe de segredo para o log do pipeline.
    throw new Error(
      `Falha ao resolver senhas: ${err instanceof Error ? err.message : String(err)}`,
      { cause: err },
    );
  }

  return rows.map((row) => ({
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
  db: Database,
  userId: string,
  supplierId?: string,
  contractIdentifier?: string,
): Promise<SecretMatch | null> {
  const candidates = await listPdfPasswordCandidates(db, userId);
  if (candidates.length === 0) return null;

  const scoped =
    candidates.find(
      (c) => supplierId && c.entityType === "supplier" && c.entityId === supplierId,
    ) ?? candidates.find((c) => contractIdentifier && c.contractIdentifier === contractIdentifier);

  return scoped ?? candidates[0]!;
}

/** Registra que esta senha funcionou, para que suba na ordem da próxima vez. */
export async function markSecretUsed(
  db: Database,
  userId: string,
  secretId: string,
): Promise<void> {
  try {
    await db.execute(sql`select public.fn_mark_secret_used(${userId}::uuid, ${secretId}::uuid)`);
  } catch {
    // Falhar aqui é irrelevante para o resultado do parse; não vale derrubar o job.
  }
}

/**
 * Registra que esta senha abre os PDFs deste fornecedor, para o próximo
 * documento dele acertar de primeira em vez de varrer todas as senhas.
 *
 * ⚠️ **Esta função não tem nenhum chamador hoje.** Ela é a segunda metade de um
 * atalho cuja primeira metade (`listPdfPasswordCandidates`) está ligada e
 * funcionando: o pipeline acerta a senha por tentativa, mas nunca guarda de
 * quem ela é, então repete a varredura toda vez. Ligar isto exige chamar aqui
 * no ponto em que o parse resolve o fornecedor — depois que o documento já foi
 * aberto —, e esse ponto ainda não passa `supplierId` adiante.
 *
 * Até aqui, a chamada apontava para `public.fn_set_secret`, que **não existe no
 * Neon** (é uma das seis descartadas na migração). Ou seja: mesmo que alguém a
 * chamasse, ela falharia — e o `catch {}` silencioso garantia que ninguém
 * descobrisse. Agora usa `gravarSegredo`, o mesmo caminho da tela de senhas.
 */
export async function associateSecretToSupplier(
  _db: Database,
  plaintext: string,
  supplierId: string,
  userId: string,
  label?: string,
): Promise<void> {
  await gravarSegredo({
    userId,
    plaintext,
    secretType: "pdf_password",
    entityType: "supplier",
    entityId: supplierId,
    label: label ?? null,
    origem: "worker-ingestao",
  });
}
