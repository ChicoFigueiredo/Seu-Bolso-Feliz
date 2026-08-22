/**
 * Escolha de provedor do LLM.
 *
 * O que este arquivo protege: um app financeiro pessoal paga a conta da IA do
 * bolso de quem o usa, e o `ai-full-enricher` roda visão com `gpt-4o`, a
 * chamada mais cara do sistema. Apontá-la para um modelo mais barato não pode
 * exigir editar código — e trocar de provedor não pode, em silêncio, mandar a
 * chave do OpenRouter para a api.openai.com.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  OPENAI_BASE_URL_PADRAO,
  OPENROUTER_BASE_URL,
  resolverCredenciaisLlm,
} from "../../workers/ingestion/src/parsers/llm-endpoint";

const CHAVES = ["OPENAI_API_KEY", "OPENROUTER_API_KEY", "OPENAI_BASE_URL", "NEXT_PUBLIC_APP_URL"];
let original: Record<string, string | undefined>;

beforeEach(() => {
  original = Object.fromEntries(CHAVES.map((k) => [k, process.env[k]]));
  for (const k of CHAVES) delete process.env[k];
});

afterEach(() => {
  for (const [k, v] of Object.entries(original)) {
    if (v === undefined) delete process.env[k];
    else process.env[k] = v;
  }
});

describe("resolverCredenciaisLlm", () => {
  it("usa a OpenAI quando só há OPENAI_API_KEY", () => {
    process.env.OPENAI_API_KEY = "sk-teste";
    const { baseUrl, headers } = resolverCredenciaisLlm("teste");

    expect(baseUrl).toBe(OPENAI_BASE_URL_PADRAO);
    expect(headers.Authorization).toBe("Bearer sk-teste");
    expect(headers["HTTP-Referer"]).toBeUndefined();
  });

  it("usa o OpenRouter quando há OPENROUTER_API_KEY", () => {
    process.env.OPENROUTER_API_KEY = "sk-or-teste";
    const { baseUrl, headers } = resolverCredenciaisLlm("teste");

    expect(baseUrl).toBe(OPENROUTER_BASE_URL);
    expect(headers.Authorization).toBe("Bearer sk-or-teste");
  });

  it("o OpenRouter tem precedência sobre a OpenAI quando as duas existem", () => {
    // A OPENAI_API_KEY costuma ficar no .env por inércia. Ignorar a chave que a
    // pessoa acabou de configurar seria o comportamento surpreendente — e o
    // sintoma (cobrança na conta errada) demoraria a aparecer.
    process.env.OPENAI_API_KEY = "sk-antiga";
    process.env.OPENROUTER_API_KEY = "sk-or-nova";
    const { baseUrl, headers } = resolverCredenciaisLlm("teste");

    expect(baseUrl).toBe(OPENROUTER_BASE_URL);
    expect(headers.Authorization).toBe("Bearer sk-or-nova");
  });

  it("manda os cabeçalhos de atribuição só no OpenRouter", () => {
    process.env.OPENROUTER_API_KEY = "sk-or-teste";
    process.env.NEXT_PUBLIC_APP_URL = "https://meudominio.com.br";
    const { headers } = resolverCredenciaisLlm("teste");

    expect(headers["HTTP-Referer"]).toBe("https://meudominio.com.br");
    expect(headers["X-Title"]).toBe("Seu Bolso Feliz");
  });

  it("OPENAI_BASE_URL sobrepõe os dois — é a porta para qualquer API compatível", () => {
    process.env.OPENAI_API_KEY = "sk-local";
    process.env.OPENAI_BASE_URL = "http://127.0.0.1:11434/v1";
    expect(resolverCredenciaisLlm("teste").baseUrl).toBe("http://127.0.0.1:11434/v1");
  });

  it("remove a barra final para não montar URL com barra dupla", () => {
    process.env.OPENAI_API_KEY = "sk-teste";
    process.env.OPENAI_BASE_URL = "https://exemplo.com/v1/";
    expect(resolverCredenciaisLlm("teste").baseUrl).toBe("https://exemplo.com/v1");
  });

  it("falha com mensagem acionável quando não há chave nenhuma", () => {
    expect(() => resolverCredenciaisLlm("enriquecimento lite")).toThrow(
      /OPENAI_API_KEY ou OPENROUTER_API_KEY.*enriquecimento lite/,
    );
  });
});
