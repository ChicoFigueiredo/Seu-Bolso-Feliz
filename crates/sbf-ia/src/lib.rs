//! Motor de IA do SBF (spec 09). Esta é a **costura**: tipos e traits fixados antes da
//! implementação paralela. O agente do motor implementa os provedores; o agente da triagem
//! usa só o que está aqui (com [`Provedor`] falso nos testes).
//!
//! Regras que valem para toda implementação:
//! - o texto enviado nunca vai em argv, nunca vai a log e nunca é gravado em disco;
//! - o registro do que saiu ([`Saida`]) guarda tamanho, tokens e custo, nunca conteúdo;
//! - nenhum teste chama provedor real.

use serde::{Deserialize, Serialize};

/// Quanto do conteúdo pode sair para um destinatário (ADR-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NivelDeConfianca {
    /// CPF, conta, cartão e linha digitável mascarados. Padrão de destinatário novo.
    Mascarado,
    /// Mascarado, salvo os tipos liberados.
    Parcial,
    /// O texto inteiro (decisão do CEO para a triagem, spec 09 D2).
    Completo,
}

/// O degrau do modelo (spec 09, "Dois degraus"). O provedor traduz para o nome dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Degrau {
    /// Rápido e barato (Haiku na assinatura; equivalente no OpenRouter).
    Rapido,
    /// Forte (Sonnet). Só para o que voltou com confiança baixa.
    Forte,
}

/// Um pedido ao modelo: instrução fixa, entrada do lote e o esquema JSON da resposta.
#[derive(Debug, Clone, PartialEq)]
pub struct Pedido {
    /// Versão do prompt; entra na chave do cache.
    pub prompt_versao: String,
    /// A instrução (sistema). Não leva dado do usuário.
    pub instrucao: String,
    /// A entrada com os itens do lote, já no nível de confiança do destinatário.
    pub entrada: String,
    /// JSON Schema que a resposta tem de seguir.
    pub esquema: serde_json::Value,
    pub degrau: Degrau,
    /// Limite de tokens da resposta.
    pub max_tokens_saida: u32,
}

/// A resposta, já como JSON validado contra [`Pedido::esquema`] pelo provedor.
#[derive(Debug, Clone, PartialEq)]
pub struct Resposta {
    pub json: serde_json::Value,
    pub consumo: Consumo,
    /// O nome real do modelo que respondeu.
    pub modelo: String,
}

/// Tokens e custo de uma chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Consumo {
    pub tokens_entrada: u64,
    pub tokens_saida: u64,
    /// Na assinatura é nominal (o que custaria na API); no OpenRouter é real.
    /// Centésimos de centavo de dólar, inteiro, sem ponto flutuante.
    pub custo_dezmilesimos_usd: u64,
    /// `true` quando sai da cota da assinatura, sem cobrança por token.
    pub nominal: bool,
}

/// O que saiu da máquina, para `saida_ia` (ADR-015). Nunca o conteúdo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saida {
    pub destinatario: String,
    pub provedor: String,
    pub modelo: String,
    pub nivel: NivelDeConfianca,
    pub bytes_enviados: u64,
    pub consumo: Consumo,
    /// RFC 3339 UTC.
    pub quando: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDeIa {
    /// Limite de ritmo ou de cota: quem chama pausa e tenta depois.
    #[error("limite do provedor; tentar em {espera_segundos}s")]
    Limite { espera_segundos: u64 },
    /// Teto de gasto atingido (só OpenRouter).
    #[error("teto de gasto atingido")]
    Teto,
    /// A resposta não seguiu o esquema.
    #[error("resposta fora do esquema: {0}")]
    RespostaInvalida(String),
    /// Provedor ausente ou mal configurado (por exemplo, `claude` não instalado).
    #[error("provedor indisponível: {0}")]
    Indisponivel(String),
    #[error("falha do provedor: {0}")]
    Outro(String),
}

pub type ResultadoDeIa<T> = Result<T, ErroDeIa>;

/// Um provedor de modelo. Síncrono: a fila roda numa thread própria.
pub trait Provedor: Send + Sync {
    /// Nome estável para `saida_ia.provedor` (`claude-cli`, `openrouter`, `falso`).
    fn nome(&self) -> &'static str;
    /// Uma chamada. Não faz cache nem retentativa: isso é da fila.
    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta>;
}

/// Onde a fila grava o que saiu e guarda o cache. O inventário implementa sobre
/// `saida_ia` e `cache_ia` (spec 09, `sql/0003-triagem.sql`); os testes, em memória.
pub trait Registro: Send + Sync {
    fn registrar_saida(&self, saida: &Saida) -> ResultadoDeIa<()>;
    fn ler_cache(&self, chave: &str) -> ResultadoDeIa<Option<serde_json::Value>>;
    fn gravar_cache(&self, chave: &str, json: &serde_json::Value) -> ResultadoDeIa<()>;
}

/// A chave do cache: `sha256(prompt_versao + modelo do degrau + entrada)`, em hex.
/// O agente do motor implementa (a assinatura é a costura).
pub fn chave_do_cache(pedido: &Pedido, provedor: &str) -> String {
    let _ = (pedido, provedor);
    unimplemented!("agente do motor: sha256 de prompt_versao, degrau, provedor e entrada")
}
