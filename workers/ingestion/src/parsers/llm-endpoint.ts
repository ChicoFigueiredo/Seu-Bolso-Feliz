/**
 * Endpoint do LLM usado pelos enriquecedores — OpenAI por padrão, OpenRouter
 * (ou qualquer API compatível) por variável de ambiente.
 *
 * ANTES: três chamadas `fetch` com `https://api.openai.com/v1/chat/completions`
 * escrito literalmente, cada uma repetindo a leitura da chave e a montagem dos
 * cabeçalhos. Trocar de provedor exigia editar dois arquivos e três lugares —
 * e o `ai-full-enricher` roda `gpt-4o` com visão, que é a chamada mais cara do
 * sistema. Poder apontá-la para um modelo mais barato sem publicar código é o
 * ponto inteiro deste módulo.
 */

export const OPENAI_BASE_URL_PADRAO = "https://api.openai.com/v1";
export const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";

export interface CredenciaisLlm {
  apiKey: string;
  baseUrl: string;
  headers: Record<string, string>;
}

/**
 * Resolve provedor, chave e cabeçalhos.
 *
 * `OPENROUTER_API_KEY` tem precedência sobre `OPENAI_API_KEY` porque definir a
 * primeira é uma escolha explícita; a segunda costuma ficar no `.env` por
 * inércia, e o comportamento surpreendente seria a chave que a pessoa acabou
 * de configurar ser ignorada.
 */
export function resolverCredenciaisLlm(contexto: string): CredenciaisLlm {
  const openRouterKey = process.env.OPENROUTER_API_KEY;
  const openAiKey = process.env.OPENAI_API_KEY;
  const apiKey = openRouterKey ?? openAiKey;

  if (!apiKey) {
    throw new Error(
      `OPENAI_API_KEY ou OPENROUTER_API_KEY não configurada — ${contexto} não disponível`,
    );
  }

  const usandoOpenRouter = Boolean(openRouterKey);
  const baseUrl =
    process.env.OPENAI_BASE_URL ??
    (usandoOpenRouter ? OPENROUTER_BASE_URL : OPENAI_BASE_URL_PADRAO);

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    Authorization: `Bearer ${apiKey}`,
  };

  // O OpenRouter reduz o teto de requisições de chamadas não identificadas.
  if (usandoOpenRouter) {
    headers["HTTP-Referer"] = process.env.NEXT_PUBLIC_APP_URL ?? "https://seubolsofeliz.local";
    headers["X-Title"] = "Seu Bolso Feliz";
  }

  return { apiKey, baseUrl: baseUrl.replace(/\/$/, ""), headers };
}

/** POST em `/chat/completions`, com o provedor já resolvido. */
export async function chamarChatCompletions(
  corpo: Record<string, unknown>,
  contexto: string,
): Promise<Response> {
  const { baseUrl, headers } = resolverCredenciaisLlm(contexto);

  return fetch(`${baseUrl}/chat/completions`, {
    method: "POST",
    headers,
    body: JSON.stringify(corpo),
  });
}
