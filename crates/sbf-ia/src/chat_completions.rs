//! Provedor de conversa pelo formato Chat Completions do OpenAI, com `tools` (spec 15, D12).
//! Um formato só atende o **local** (Ollama em `http://localhost:11434/v1`), o **OpenRouter** e
//! **qualquer API compatível** (endereço, modelo e, se houver, chave).
//!
//! HTTPS pelo portão da bancada ([`bancada_rede`]); `http://` só em loopback (ver
//! [`crate::ponte_http`]). A chave vem de uma [`FonteDaChave`] (o chaveiro fica com quem
//! chama) e nunca de argv, log ou erro. Nenhum erro carrega corpo de resposta.

use crate::claude_cli::dezmilesimos;
use crate::conversa::{
    ChamadaDeFerramenta, Mensagem, NaturezaDoCusto, PedidoDeConversa, ProvedorDeConversa,
    RespostaDeConversa,
};
use crate::ponte_http::{abrir_rede, executar, ler_endereco, mapear};
use crate::{Consumo, ErroDeIa, FonteDaChave, ResultadoDeIa};
use bancada_rede::{Pedido as PedidoHttp, Rede};
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// O endereço padrão do Ollama.
pub const ENDERECO_DO_OLLAMA: &str = "http://localhost:11434/v1";
/// O modelo local padrão (para calibrar as ferramentas).
pub const MODELO_LOCAL_PADRAO: &str = "qwen3-coder:30b";
/// O endereço do OpenRouter.
pub const ENDERECO_DO_OPENROUTER: &str = "https://openrouter.ai/api/v1";

/// Configuração de um provedor Chat Completions.
#[derive(Debug, Clone)]
pub struct ConfiguracaoChatCompletions {
    /// Nome estável (`local`, `openrouter`, ou o do provedor compatível).
    pub nome: String,
    /// `https://…/v1`, ou `http://localhost:porta/v1`.
    pub endereco: String,
    pub modelo: String,
    pub natureza: NaturezaDoCusto,
    /// Pede `usage: {include: true}` (o OpenRouter devolve o custo real).
    pub pedir_custo: bool,
    pub prazo: Duration,
}

impl ConfiguracaoChatCompletions {
    /// O Ollama local, sem custo.
    pub fn local(endereco: impl Into<String>, modelo: impl Into<String>) -> Self {
        Self {
            nome: "local".to_owned(),
            endereco: endereco.into(),
            modelo: modelo.into(),
            natureza: NaturezaDoCusto::Zero,
            pedir_custo: false,
            // Um modelo grande carregando na primeira chamada demora.
            prazo: Duration::from_secs(900),
        }
    }

    /// O OpenRouter (custo real).
    pub fn openrouter(modelo: impl Into<String>) -> Self {
        Self {
            nome: "openrouter".to_owned(),
            endereco: ENDERECO_DO_OPENROUTER.to_owned(),
            modelo: modelo.into(),
            natureza: NaturezaDoCusto::Real,
            pedir_custo: true,
            prazo: Duration::from_secs(300),
        }
    }

    /// Qualquer API compatível com OpenAI.
    pub fn compativel(
        nome: impl Into<String>,
        endereco: impl Into<String>,
        modelo: impl Into<String>,
    ) -> Self {
        Self {
            nome: nome.into(),
            endereco: endereco.into(),
            modelo: modelo.into(),
            natureza: NaturezaDoCusto::Real,
            pedir_custo: false,
            prazo: Duration::from_secs(300),
        }
    }
}

/// O provedor.
pub struct ChatCompletions {
    rede: Rede,
    config: ConfiguracaoChatCompletions,
    /// `https://…/v1` (catálogo) onde `/chat/completions` se soma.
    base: String,
}

impl fmt::Debug for ChatCompletions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatCompletions")
            .field("nome", &self.config.nome)
            .field("modelo", &self.config.modelo)
            .finish_non_exhaustive()
    }
}

impl ChatCompletions {
    /// `fonte` é a chave (`None` para o Ollama e outros sem chave).
    ///
    /// # Errors
    /// [`ErroDeIa::Indisponivel`] se o endereço não serve ou o portão não abre.
    pub fn novo(
        config: ConfiguracaoChatCompletions,
        fonte: Option<Arc<dyn FonteDaChave>>,
    ) -> ResultadoDeIa<Self> {
        let endereco = ler_endereco(&config.endereco)?;
        let rede = abrir_rede(&config.nome, &endereco, fonte, config.prazo)?;
        Ok(Self {
            rede,
            base: endereco.base,
            config,
        })
    }
}

fn mensagem_json(m: &Mensagem) -> Value {
    match m {
        Mensagem::Sistema(t) => json!({"role": "system", "content": t}),
        Mensagem::Usuario(t) => json!({"role": "user", "content": t}),
        Mensagem::Assistente { texto, chamadas } => {
            let mut v = json!({"role": "assistant", "content": texto});
            if !chamadas.is_empty() {
                v["tool_calls"] = Value::Array(
                    chamadas
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": {
                                    "name": c.nome,
                                    "arguments": c.argumentos.to_string(),
                                },
                            })
                        })
                        .collect(),
                );
            }
            v
        }
        Mensagem::Ferramenta { id, nome, conteudo } => {
            json!({"role": "tool", "tool_call_id": id, "name": nome, "content": conteudo})
        }
    }
}

pub(crate) fn corpo_do_pedido(config: &ConfiguracaoChatCompletions, p: &PedidoDeConversa) -> Value {
    let mut corpo = json!({
        "model": config.modelo,
        "messages": p.mensagens.iter().map(mensagem_json).collect::<Vec<_>>(),
        "max_tokens": p.max_tokens_saida,
        "temperature": 0,
        "stream": false,
    });
    if !p.ferramentas.is_empty() {
        corpo["tools"] = Value::Array(
            p.ferramentas
                .iter()
                .map(|f| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": f.nome,
                            "description": f.descricao,
                            "parameters": f.parametros,
                        },
                    })
                })
                .collect(),
        );
        corpo["tool_choice"] = json!("auto");
    }
    if config.pedir_custo {
        corpo["usage"] = json!({"include": true});
    }
    corpo
}

/// Os argumentos de uma chamada: o OpenAI manda um texto JSON; alguns servidores locais já
/// mandam o objeto.
fn argumentos_de(v: Option<&Value>) -> Value {
    match v {
        Some(Value::String(s)) if s.trim().is_empty() => json!({}),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
        Some(outro) if !outro.is_null() => outro.clone(),
        _ => json!({}),
    }
}

/// Interpreta a resposta (pura, para testar sem rede).
pub(crate) fn interpretar(
    v: &Value,
    modelo_pedido: &str,
    natureza: NaturezaDoCusto,
) -> ResultadoDeIa<RespostaDeConversa> {
    let msg = v
        .pointer("/choices/0/message")
        .ok_or_else(|| ErroDeIa::RespostaInvalida("sem mensagem na resposta".to_owned()))?;
    let texto = msg
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let mut chamadas = Vec::new();
    if let Some(lista) = msg.get("tool_calls").and_then(Value::as_array) {
        for (i, c) in lista.iter().enumerate() {
            let Some(nome) = c.pointer("/function/name").and_then(Value::as_str) else {
                continue;
            };
            chamadas.push(ChamadaDeFerramenta {
                id: c
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| format!("chamada_{}", i + 1), str::to_owned),
                nome: nome.to_owned(),
                argumentos: argumentos_de(c.pointer("/function/arguments")),
            });
        }
    }
    let uso = v.get("usage");
    let numero = |k: &str| {
        uso.and_then(|u| u.get(k))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let informado = uso
        .and_then(|u| u.get("cost"))
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite() && *c >= 0.0);
    let custo = match natureza {
        NaturezaDoCusto::Zero => 0,
        _ => informado.map_or(0, dezmilesimos),
    };
    // Cobra por token e não disse quanto: não é "US$ 0".
    let custo_ausente = matches!(
        natureza,
        NaturezaDoCusto::Real | NaturezaDoCusto::NaoInformado
    ) && informado.is_none();
    Ok(RespostaDeConversa {
        texto,
        chamadas,
        consumo: Consumo {
            tokens_entrada: numero("prompt_tokens"),
            tokens_saida: numero("completion_tokens"),
            custo_dezmilesimos_usd: custo,
            nominal: natureza == NaturezaDoCusto::Nominal,
        },
        custo_ausente,
        modelo: v
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(modelo_pedido)
            .to_owned(),
    })
}

impl ProvedorDeConversa for ChatCompletions {
    fn nome(&self) -> String {
        self.config.nome.clone()
    }

    fn modelo(&self) -> String {
        self.config.modelo.clone()
    }

    fn natureza_do_custo(&self) -> NaturezaDoCusto {
        self.config.natureza
    }

    fn endereco(&self) -> String {
        self.base.clone()
    }

    fn conversar(
        &self,
        pedido: &PedidoDeConversa,
        cancelar: &AtomicBool,
    ) -> ResultadoDeIa<RespostaDeConversa> {
        let bytes = serde_json::to_vec(&corpo_do_pedido(&self.config, pedido))
            .map_err(|_| ErroDeIa::Outro("montar o pedido".to_owned()))?;
        let mut http = PedidoHttp::post(format!("{}/chat/completions", self.base))
            .aceitar("application/json")
            .json(bytes)
            .prazo(self.config.prazo);
        if self.rede_com_chave() {
            http = http.autorizado();
        }
        let resposta = executar(&self.rede, http, cancelar)
            .map_err(|e| mapear(&self.rede, &self.config.nome, &e))?;
        let v: Value = serde_json::from_slice(&resposta.corpo)
            .map_err(|_| ErroDeIa::Outro("resposta do provedor ilegível".to_owned()))?;
        interpretar(&v, &self.config.modelo, self.config.natureza)
    }
}

impl ChatCompletions {
    /// O catálogo marca o endereço com autorização quando há fonte de chave.
    fn rede_com_chave(&self) -> bool {
        self.rede
            .catalogo()
            .enderecos()
            .iter()
            .any(bancada_rede::Endereco::recebe_autorizacao)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::conversa::DefinicaoDeFerramenta;

    fn pedido() -> PedidoDeConversa {
        PedidoDeConversa {
            mensagens: vec![
                Mensagem::Sistema("s".into()),
                Mensagem::Usuario("u".into()),
                Mensagem::Assistente {
                    texto: None,
                    chamadas: vec![ChamadaDeFerramenta {
                        id: "c1".into(),
                        nome: "ver_item".into(),
                        argumentos: json!({"pagina": 2}),
                    }],
                },
                Mensagem::Ferramenta {
                    id: "c1".into(),
                    nome: "ver_item".into(),
                    conteudo: "texto".into(),
                },
            ],
            ferramentas: vec![DefinicaoDeFerramenta {
                nome: "ver_item".into(),
                descricao: "d".into(),
                parametros: json!({"type": "object"}),
            }],
            max_tokens_saida: 500,
        }
    }

    #[test]
    fn o_corpo_leva_mensagens_ferramentas_e_resultados() {
        let c = ConfiguracaoChatCompletions::local("http://localhost:11434/v1", "m");
        let corpo = corpo_do_pedido(&c, &pedido());
        assert_eq!(corpo["model"], "m");
        assert_eq!(
            corpo["messages"][2]["tool_calls"][0]["function"]["name"],
            "ver_item"
        );
        assert_eq!(
            corpo["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"pagina\":2}"
        );
        assert_eq!(corpo["messages"][3]["role"], "tool");
        assert_eq!(corpo["messages"][3]["tool_call_id"], "c1");
        assert_eq!(corpo["tools"][0]["function"]["name"], "ver_item");
        assert!(corpo.get("usage").is_none());
        let o = ConfiguracaoChatCompletions::openrouter("a/b");
        assert_eq!(corpo_do_pedido(&o, &pedido())["usage"]["include"], true);
    }

    #[test]
    fn interpreta_chamadas_com_argumentos_em_texto_ou_objeto() {
        let v = json!({
            "model": "qwen",
            "choices": [{"message": {"content": null, "tool_calls": [
                {"id": "a", "function": {"name": "ver_item", "arguments": "{\"pagina\":1}"}},
                {"function": {"name": "listar_padroes", "arguments": {"estado": "ativo"}}},
                {"function": {"name": "rastro_do_item", "arguments": ""}},
            ]}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3}
        });
        let r = interpretar(&v, "m", NaturezaDoCusto::Zero).expect("resposta");
        assert_eq!(r.chamadas.len(), 3);
        assert_eq!(r.chamadas[0].argumentos, json!({"pagina": 1}));
        assert_eq!(r.chamadas[1].id, "chamada_2");
        assert_eq!(r.chamadas[1].argumentos, json!({"estado": "ativo"}));
        assert_eq!(r.chamadas[2].argumentos, json!({}));
        assert_eq!(r.consumo.tokens_entrada, 10);
        assert_eq!(r.consumo.custo_dezmilesimos_usd, 0);
        assert!(!r.custo_ausente, "o local é grátis: nada a informar");
        assert!(r.texto.is_none());
        assert_eq!(r.modelo, "qwen");
    }

    #[test]
    fn compativel_sem_usage_cost_diz_custo_ausente_e_com_cost_mostra_o_valor() {
        let sem = json!({
            "choices": [{"message": {"content": "oi"}}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 7}
        });
        let r = interpretar(&sem, "m", NaturezaDoCusto::Real).expect("resposta");
        assert!(r.custo_ausente, "sem usage.cost não é US$ 0");
        assert_eq!(r.consumo.tokens_entrada, 100);
        let com = json!({
            "choices": [{"message": {"content": "oi"}}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 7, "cost": 0.0123}
        });
        let r = interpretar(&com, "m", NaturezaDoCusto::Real).expect("resposta");
        assert!(!r.custo_ausente);
        assert_eq!(r.consumo.custo_dezmilesimos_usd, 123);
        // Um custo informado de zero é zero de verdade.
        let zero = json!({
            "choices": [{"message": {"content": "oi"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "cost": 0}
        });
        assert!(
            !interpretar(&zero, "m", NaturezaDoCusto::Real)
                .expect("resposta")
                .custo_ausente
        );
    }
}
