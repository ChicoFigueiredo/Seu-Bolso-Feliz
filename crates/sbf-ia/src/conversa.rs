//! Conversa com ferramentas (spec 15, Rita): o que a triagem em lote não precisa. Um modelo
//! recebe mensagens e a lista de ferramentas, e responde com texto e/ou chamadas de
//! ferramenta; quem chama (o laço de Rita) executa as chamadas e devolve o resultado.
//!
//! O formato é o do Chat Completions do OpenAI ([`crate::ChatCompletions`]), com um adaptador
//! para a API Messages da Anthropic ([`crate::Anthropic`]). O `claude -p` não passa por aqui:
//! ele roda o próprio laço e fala com as ferramentas por MCP ([`crate::ClaudeCli`]).
//!
//! Regras (as mesmas do resto de `sbf-ia`): nenhum texto de documento vai a log nem a erro, e
//! nenhuma chave aparece em lugar nenhum.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::AtomicBool;

use crate::{Consumo, ResultadoDeIa};

/// Uma chamada de ferramenta pedida pelo modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChamadaDeFerramenta {
    /// O id da chamada (o resultado volta com ele).
    pub id: String,
    pub nome: String,
    /// Os argumentos, já como JSON.
    pub argumentos: Value,
}

/// Uma mensagem da conversa.
#[derive(Debug, Clone, PartialEq)]
pub enum Mensagem {
    Sistema(String),
    Usuario(String),
    Assistente {
        texto: Option<String>,
        chamadas: Vec<ChamadaDeFerramenta>,
    },
    /// O resultado de uma chamada. É dado, nunca instrução.
    Ferramenta {
        id: String,
        nome: String,
        conteudo: String,
    },
}

/// Uma ferramenta que o modelo pode chamar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinicaoDeFerramenta {
    pub nome: String,
    pub descricao: String,
    /// JSON Schema dos argumentos (um objeto).
    pub parametros: Value,
}

/// Um pedido de conversa.
#[derive(Debug, Clone, PartialEq)]
pub struct PedidoDeConversa {
    pub mensagens: Vec<Mensagem>,
    pub ferramentas: Vec<DefinicaoDeFerramenta>,
    /// Limite de tokens da resposta.
    pub max_tokens_saida: u32,
}

/// A resposta de um passo da conversa.
#[derive(Debug, Clone, PartialEq)]
pub struct RespostaDeConversa {
    pub texto: Option<String>,
    pub chamadas: Vec<ChamadaDeFerramenta>,
    pub consumo: Consumo,
    /// O nome do modelo que respondeu.
    pub modelo: String,
}

/// Como o custo de um provedor é contado (spec 15, D11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturezaDoCusto {
    /// Cobrado por token (OpenRouter, API da Anthropic).
    Real,
    /// O que custaria na API; a assinatura não cobra por token (`claude -p`).
    Nominal,
    /// Roda na máquina do usuário (Ollama).
    Zero,
}

/// Um provedor de conversa com ferramentas. Síncrono: o laço de Rita roda numa thread própria.
pub trait ProvedorDeConversa: Send + Sync {
    /// Nome estável para `saida_ia.provedor`.
    fn nome(&self) -> String;
    /// O modelo configurado (rótulo do autor do rascunho).
    fn modelo(&self) -> String;
    fn natureza_do_custo(&self) -> NaturezaDoCusto;
    /// Um passo. `cancelar` ligado interrompe a espera e devolve
    /// [`crate::ErroDeIa::Interrompido`].
    ///
    /// # Errors
    /// Falha do provedor, limite, resposta ilegível ou interrupção.
    fn conversar(
        &self,
        pedido: &PedidoDeConversa,
        cancelar: &AtomicBool,
    ) -> ResultadoDeIa<RespostaDeConversa>;
}

/// Dólares por milhão de tokens, em dezmilésimos de dólar por token, para somar sem ponto
/// flutuante: `custo = entrada * pe + saida * ps` com `pe` e `ps` em dezmilésimos de dólar por
/// milhão de tokens.
pub(crate) fn custo_por_tokens(
    tokens_entrada: u64,
    tokens_saida: u64,
    dezmilesimos_por_mtok_entrada: u64,
    dezmilesimos_por_mtok_saida: u64,
) -> u64 {
    let total = u128::from(tokens_entrada) * u128::from(dezmilesimos_por_mtok_entrada)
        + u128::from(tokens_saida) * u128::from(dezmilesimos_por_mtok_saida);
    // Arredonda para cima: o custo nunca aparece menor do que é.
    u64::try_from(total.div_ceil(1_000_000)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn custo_por_tokens_arredonda_para_cima() {
        // US$ 3 por Mtok = 30_000 dezmilésimos por Mtok.
        assert_eq!(custo_por_tokens(1_000_000, 0, 30_000, 150_000), 30_000);
        assert_eq!(custo_por_tokens(1, 0, 30_000, 150_000), 1);
        assert_eq!(custo_por_tokens(0, 0, 30_000, 150_000), 0);
        assert_eq!(
            custo_por_tokens(1_000_000, 1_000_000, 30_000, 150_000),
            180_000
        );
    }
}
