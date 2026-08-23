/**
 * Leitura das chaves de API novas (`sb_publishable_...` / `sb_secret_...`)
 * dentro das Edge Functions.
 *
 * ── Por que este arquivo existe ────────────────────────────────────────────
 *
 * O runtime injeta `SUPABASE_PUBLISHABLE_KEYS` e `SUPABASE_SECRET_KEYS` — no
 * PLURAL, e o valor é um JSON indexado por nome de chave, não uma string:
 *
 *     SUPABASE_SECRET_KEYS = {"default":"sb_secret_..."}
 *
 * As variáveis no singular que este código lia antes
 * (`SUPABASE_PUBLISHABLE_KEY`, `SUPABASE_SECRET_KEY`) **nunca existiram no
 * runtime hospedado**: a plataforma não as injeta, e não dá para criá-las como
 * secret — o CLI recusa qualquer nome com o prefixo reservado, com a mensagem
 * "Env name cannot start with SUPABASE_, skipping". Ou seja, as quatro funções
 * subiriam com a chave `undefined` e falhariam na primeira chamada.
 *
 * ── Cabeçalho ──────────────────────────────────────────────────────────────
 *
 * As chaves novas NÃO são JWT. Se forem enviadas em `Authorization: Bearer`, a
 * plataforma tenta interpretá-las como token e responde `Invalid JWT`. Elas vão
 * no cabeçalho `apikey` — que é o que o `supabase-js` já faz sozinho quando a
 * chave é passada em `createClient`. O `Authorization` fica reservado ao JWT do
 * usuário, que continua sendo um JWT de verdade.
 *
 * Como o `verify_jwt` embutido só entende chave legacy, cada função declara
 * `verify_jwt = false` em `supabase/config.toml` e autoriza no próprio código.
 */

function readKeySet(varName: string, keyName: string): string | undefined {
  const raw = Deno.env.get(varName);
  if (!raw) return undefined;

  try {
    const parsed = JSON.parse(raw) as Record<string, string>;
    return parsed[keyName];
  } catch {
    throw new Error(`${varName} não é um JSON válido — esperado {"default":"sb_..."}`);
  }
}

function resolve(newVar: string, legacyVar: string, keyName: string, label: string): string {
  const fromNewKeys = readKeySet(newVar, keyName);
  if (fromNewKeys) return fromNewKeys;

  // Só acontece em runtime antigo ou local desatualizado. É degradação
  // consciente e barulhenta: silenciar aqui seria reintroduzir a dependência
  // das chaves legacy sem ninguém perceber.
  const legacy = Deno.env.get(legacyVar);
  if (legacy) {
    console.warn(
      `[keys] ${newVar} ausente; usando ${legacyVar} (legacy). ` +
        `As chaves legacy deixam de funcionar quando forem desativadas no dashboard.`,
    );
    return legacy;
  }

  throw new Error(`Nenhuma ${label} disponível: nem ${newVar}["${keyName}"] nem ${legacyVar}.`);
}

/** Chave secreta (substitui a `service_role`). Ignora RLS. */
export function getSecretKey(keyName = "default"): string {
  return resolve("SUPABASE_SECRET_KEYS", "SUPABASE_SERVICE_ROLE_KEY", keyName, "chave secreta");
}

/** Chave publicável (substitui a `anon`). Respeita RLS. */
export function getPublishableKey(keyName = "default"): string {
  return resolve("SUPABASE_PUBLISHABLE_KEYS", "SUPABASE_ANON_KEY", keyName, "chave publicável");
}
