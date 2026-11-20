//! Motor de IA do SBF (spec 09). Esta é a **costura**: tipos e traits fixados antes da
//! implementação paralela. O agente do motor implementa os provedores; o agente da triagem
//! usa só o que está aqui (com [`Provedor`] falso nos testes).
//!
//! Regras que valem para toda implementação:
//! - o texto enviado nunca vai em argv, nunca vai a log e nunca é gravado em disco;
//! - o registro do que saiu ([`Saida`]) guarda tamanho, tokens e custo, nunca conteúdo;
//! - nenhum teste chama provedor real.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod anthropic;
mod chat_completions;
mod claude_cli;
mod conversa;
mod esquema;
mod falso;
mod fila;
mod openrouter;
mod ponte_http;
mod tempo;

pub use anthropic::{Anthropic, ConfiguracaoAnthropic, ENDERECO_DA_ANTHROPIC};
pub use chat_completions::{
    ChatCompletions, ConfiguracaoChatCompletions, ENDERECO_DO_OLLAMA, ENDERECO_DO_OPENROUTER,
    MODELO_LOCAL_PADRAO,
};
pub use claude_cli::{ClaudeCli, SaidaDoClaudeComFerramentas, ServidorMcp};
pub use conversa::{
    ChamadaDeFerramenta, DefinicaoDeFerramenta, Mensagem, NaturezaDoCusto, PedidoDeConversa,
    ProvedorDeConversa, RespostaDeConversa,
};
pub use esquema::{extrair_json, validar_esquema};
pub use falso::{Falso, FalsoDeConversa, RegistroFalso};
pub use fila::{
    Atribuicao, ConfiguracaoDaFila, Controle, Evento, Fila, Formato, Interrupcao, LoteResolvido,
    Relatorio,
};
pub use openrouter::{ConfiguracaoOpenRouter, FonteDaChave, OpenRouter};
pub use ponte_http::validar_endereco;
pub use tempo::{agora_rfc3339, rfc3339_de};

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
    /// O usuário mandou parar (botão Parar de Rita).
    #[error("interrompido")]
    Interrompido,
}

pub type ResultadoDeIa<T> = Result<T, ErroDeIa>;

/// Um provedor de modelo. Síncrono: a fila roda numa thread própria.
pub trait Provedor: Send + Sync {
    /// Nome estável para `saida_ia.provedor` (`claude-cli`, `openrouter`, `falso`).
    fn nome(&self) -> &'static str;
    /// Uma chamada. Não faz cache nem retentativa: isso é da fila.
    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta>;
    /// Começa uma rodada: zera o que é "por rodada" (o teto do OpenRouter). A fila chama
    /// no início de cada rodada.
    fn nova_rodada(&self) {}
    /// `false` quando o provedor já não aceita pedidos por gasto (teto de rodada ou do mês).
    /// A fila só troca para a reserva se ela responde `true`.
    fn dentro_do_teto(&self) -> bool {
        true
    }
}

/// Onde a fila grava o que saiu e guarda o cache. O inventário implementa sobre
/// `saida_ia` e `cache_ia` (spec 09, `sql/0003-triagem.sql`); os testes, em memória.
pub trait Registro: Send + Sync {
    fn registrar_saida(&self, saida: &Saida) -> ResultadoDeIa<()>;
    /// Registra o que **vai** sair, antes do envio (ADR-015: falha fechada). Quem chama só
    /// envia se isto der `Ok`. Devolve o id da linha, para [`Self::fechar_saida`] completar
    /// com os tokens e o custo quando a resposta chegar. O padrão registra a linha de uma vez
    /// (sem tokens) e não devolve id.
    ///
    /// # Errors
    /// O registro não abriu ou não gravou: nada deve ser enviado.
    fn abrir_saida(&self, saida: &Saida) -> ResultadoDeIa<Option<i64>> {
        self.registrar_saida(saida).map(|()| None)
    }
    /// Completa a linha aberta por [`Self::abrir_saida`] com o consumo da resposta. A saída já
    /// está registrada: uma falha aqui só perde os tokens, nunca autoriza nem bloqueia nada.
    ///
    /// # Errors
    /// Falha do registro.
    fn fechar_saida(&self, _id: i64, _consumo: Consumo) -> ResultadoDeIa<()> {
        Ok(())
    }
    fn ler_cache(&self, chave: &str) -> ResultadoDeIa<Option<serde_json::Value>>;
    fn gravar_cache(&self, chave: &str, json: &serde_json::Value) -> ResultadoDeIa<()>;
}

/// A chave do cache: `sha256(prompt_versao + degrau + provedor + entrada)`, em hex minúsculo.
/// Os campos são separados por um byte zero, para `("ab","c")` e `("a","bc")` não colidirem.
/// A instrução não entra: a `prompt_versao` a representa.
pub fn chave_do_cache(pedido: &Pedido, provedor: &str) -> String {
    let degrau = match pedido.degrau {
        Degrau::Rapido => "rapido",
        Degrau::Forte => "forte",
    };
    let mut resumo = Sha256::new();
    for campo in [
        pedido.prompt_versao.as_str(),
        degrau,
        provedor,
        pedido.entrada.as_str(),
    ] {
        resumo.update(campo.as_bytes());
        resumo.update([0_u8]);
    }
    resumo
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            use std::fmt::Write;
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pedido(entrada: &str, degrau: Degrau) -> Pedido {
        Pedido {
            prompt_versao: "v1".into(),
            instrucao: "i".into(),
            entrada: entrada.into(),
            esquema: serde_json::json!({}),
            degrau,
            max_tokens_saida: 100,
        }
    }

    #[test]
    fn chave_e_sha256_em_hex_e_estavel() {
        let a = chave_do_cache(&pedido("x", Degrau::Rapido), "falso");
        assert_eq!(a.len(), 64);
        assert!(
            a.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_eq!(a, chave_do_cache(&pedido("x", Degrau::Rapido), "falso"));
    }

    #[test]
    fn chave_muda_com_entrada_degrau_provedor_e_versao() {
        let base = chave_do_cache(&pedido("x", Degrau::Rapido), "falso");
        assert_ne!(base, chave_do_cache(&pedido("y", Degrau::Rapido), "falso"));
        assert_ne!(base, chave_do_cache(&pedido("x", Degrau::Forte), "falso"));
        assert_ne!(base, chave_do_cache(&pedido("x", Degrau::Rapido), "outro"));
        let mut p = pedido("x", Degrau::Rapido);
        p.prompt_versao = "v2".into();
        assert_ne!(base, chave_do_cache(&p, "falso"));
    }

    #[test]
    fn chave_nao_colide_por_fronteira_de_campos() {
        let mut a = pedido("c", Degrau::Rapido);
        a.prompt_versao = "ab".into();
        let mut b = pedido("bc", Degrau::Rapido);
        b.prompt_versao = "a".into();
        assert_ne!(chave_do_cache(&a, "p"), chave_do_cache(&b, "p"));
    }

    #[test]
    fn chave_conhecida_de_sha256() {
        // Conferido contra o sha256 dos mesmos bytes.
        let esperado = {
            let mut r = Sha256::new();
            r.update(b"v1\0rapido\0falso\0x\0");
            r.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            chave_do_cache(&pedido("x", Degrau::Rapido), "falso"),
            esperado
        );
    }
}
