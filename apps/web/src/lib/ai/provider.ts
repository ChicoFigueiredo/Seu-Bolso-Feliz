/**
 * Provedor de LLM da web — OpenAI por padrão, OpenRouter (ou qualquer API
 * compatível) por variável de ambiente.
 *
 * ANTES: `openai("gpt-4o")` e `openai("gpt-4o-mini")` literais dentro das
 * rotas. Trocar de modelo — ou de provedor — exigia editar código e publicar.
 * Isso importa mais do que parece num app pessoal: o custo do chat sai do
 * bolso de quem usa, e experimentar um modelo mais barato não deveria custar
 * um deploy.
 *
 * A API do OpenRouter é compatível com a da OpenAI, então o mesmo cliente
 * serve aos dois; o que muda é a URL base, a chave e o nome do modelo, que lá
 * leva prefixo de fornecedor (`openai/gpt-4o`, `anthropic/claude-3.5-sonnet`).
 */
import { createOpenAI } from "@ai-sdk/openai";

export const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";

const MODELO_CONVERSA_PADRAO = "gpt-4o";
const MODELO_SUGESTAO_PADRAO = "gpt-4o-mini";

function usandoOpenRouter(): boolean {
  return Boolean(process.env.OPENROUTER_API_KEY);
}

/**
 * O OpenRouter aplica limites mais duros a chamadas sem identificação de
 * origem, e usa esses dois cabeçalhos para atribuir uso e listar a aplicação.
 * Sem eles a integração funciona, mas com um teto de requisições menor — o
 * tipo de detalhe que só aparece em produção, sob carga.
 */
function cabecalhosDeAtribuicao(): Record<string, string> | undefined {
  if (!usandoOpenRouter()) return undefined;

  return {
    "HTTP-Referer": process.env.NEXT_PUBLIC_APP_URL ?? "https://seubolsofeliz.local",
    "X-Title": "Seu Bolso Feliz",
  };
}

const cliente = createOpenAI({
  apiKey: process.env.OPENROUTER_API_KEY ?? process.env.OPENAI_API_KEY,
  baseURL: usandoOpenRouter()
    ? (process.env.OPENAI_BASE_URL ?? OPENROUTER_BASE_URL)
    : process.env.OPENAI_BASE_URL,
  headers: cabecalhosDeAtribuicao(),
});

/** Modelo do chat: conversa longa com uso de ferramentas. */
export function modeloDeConversa() {
  return cliente(process.env.AI_CHAT_MODEL ?? MODELO_CONVERSA_PADRAO);
}

/** Modelo das sugestões inline: uma chamada, uma ferramenta, barato. */
export function modeloDeSugestao() {
  return cliente(process.env.AI_SUGGEST_MODEL ?? MODELO_SUGESTAO_PADRAO);
}

/** Há credencial para falar com algum provedor? */
export function iaConfigurada(): boolean {
  return Boolean(process.env.OPENROUTER_API_KEY ?? process.env.OPENAI_API_KEY);
}

/** Nome do provedor ativo, para mensagens de erro que ajudem de verdade. */
export function provedorAtivo(): "openrouter" | "openai" {
  return usandoOpenRouter() ? "openrouter" : "openai";
}
