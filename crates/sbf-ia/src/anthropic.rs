//! Adaptador da API Messages da Anthropic para a conversa com ferramentas (spec 15, D12).
//!
//! O portão da bancada só põe `Authorization: Bearer` por conta própria; a API da Anthropic
//! pede a chave em `x-api-key`. A chave vem da [`FonteDaChave`], é lida a cada pedido e vai só
//! nesse cabeçalho, que o `Debug` do pedido do portão mostraria: por isso o pedido nunca é
//! impresso nem entra em erro aqui. (Quando a bancada ganhar cabeçalho de autorização
//! configurável, esta é a linha a trocar.)

use crate::conversa::{
    ChamadaDeFerramenta, Mensagem, NaturezaDoCusto, PedidoDeConversa, ProvedorDeConversa,
    RespostaDeConversa, custo_por_tokens,
};
use crate::ponte_http::{abrir_rede, executar, ler_endereco, mapear};
use crate::{Consumo, ErroDeIa, FonteDaChave, ResultadoDeIa};
use bancada_rede::{Pedido as PedidoHttp, Rede};
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// O endereço da API.
pub const ENDERECO_DA_ANTHROPIC: &str = "https://api.anthropic.com/v1";
const VERSAO_DA_API: &str = "2023-06-01";

/// Configuração do adaptador.
#[derive(Debug, Clone)]
pub struct ConfiguracaoAnthropic {
    pub endereco: String,
    pub modelo: String,
    pub prazo: Duration,
    /// Dezmilésimos de dólar por milhão de tokens (entrada, saída), para o custo estimado.
    /// `None`: tabela pelo nome do modelo.
    pub precos: Option<(u64, u64)>,
}

impl ConfiguracaoAnthropic {
    pub fn nova(modelo: impl Into<String>) -> Self {
        Self {
            endereco: ENDERECO_DA_ANTHROPIC.to_owned(),
            modelo: modelo.into(),
            prazo: Duration::from_secs(300),
            precos: None,
        }
    }
}

/// US$ por milhão de tokens, em dezmilésimos, pela família do modelo: o custo mostrado é uma
/// estimativa pela tabela de preços públicos, e a configuração pode trocá-la.
fn precos_do_modelo(modelo: &str) -> (u64, u64) {
    let m = modelo.to_ascii_lowercase();
    if m.contains("haiku") {
        (10_000, 50_000)
    } else if m.contains("opus") {
        (50_000, 250_000)
    } else {
        (30_000, 150_000)
    }
}

/// O provedor.
pub struct Anthropic {
    rede: Rede,
    config: ConfiguracaoAnthropic,
    base: String,
    fonte: Arc<dyn FonteDaChave>,
}

impl fmt::Debug for Anthropic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Anthropic")
            .field("modelo", &self.config.modelo)
            .finish_non_exhaustive()
    }
}

impl Anthropic {
    /// # Errors
    /// [`ErroDeIa::Indisponivel`] se o endereço não serve ou o portão não abre.
    pub fn novo(
        config: ConfiguracaoAnthropic,
        fonte: Arc<dyn FonteDaChave>,
    ) -> ResultadoDeIa<Self> {
        let endereco = ler_endereco(&config.endereco)?;
        // Sem `com_autorizacao`: a chave vai em `x-api-key`, não em `Authorization`.
        let rede = abrir_rede("anthropic", &endereco, None, config.prazo)?;
        Ok(Self {
            rede,
            base: endereco.base,
            config,
            fonte,
        })
    }
}

pub(crate) fn corpo_do_pedido(modelo: &str, p: &PedidoDeConversa) -> Value {
    let mut sistema = Vec::new();
    let mut mensagens: Vec<Value> = Vec::new();
    for m in &p.mensagens {
        match m {
            Mensagem::Sistema(t) => sistema.push(t.clone()),
            Mensagem::Usuario(t) => mensagens.push(json!({"role": "user", "content": t})),
            Mensagem::Assistente { texto, chamadas } => {
                let mut blocos = Vec::new();
                if let Some(t) = texto.as_ref().filter(|t| !t.is_empty()) {
                    blocos.push(json!({"type": "text", "text": t}));
                }
                for c in chamadas {
                    blocos.push(json!({
                        "type": "tool_use", "id": c.id, "name": c.nome, "input": c.argumentos,
                    }));
                }
                if blocos.is_empty() {
                    blocos.push(json!({"type": "text", "text": "."}));
                }
                mensagens.push(json!({"role": "assistant", "content": blocos}));
            }
            Mensagem::Ferramenta { id, conteudo, .. } => {
                let bloco = json!({"type": "tool_result", "tool_use_id": id, "content": conteudo});
                // Resultados seguidos vão numa mensagem só (a API exige alternância).
                let anterior = mensagens.last_mut().filter(|a| {
                    a["role"] == "user"
                        && a["content"]
                            .as_array()
                            .is_some_and(|b| b.first().is_some_and(|x| x["type"] == "tool_result"))
                });
                match anterior {
                    Some(a) => {
                        if let Some(lista) = a["content"].as_array_mut() {
                            lista.push(bloco);
                        }
                    }
                    None => mensagens.push(json!({"role": "user", "content": [bloco]})),
                }
            }
        }
    }
    let mut corpo = json!({
        "model": modelo,
        "max_tokens": p.max_tokens_saida,
        "temperature": 0,
        "messages": mensagens,
    });
    if !sistema.is_empty() {
        corpo["system"] = json!(sistema.join("\n\n"));
    }
    if !p.ferramentas.is_empty() {
        corpo["tools"] = Value::Array(
            p.ferramentas
                .iter()
                .map(|f| {
                    json!({"name": f.nome, "description": f.descricao, "input_schema": f.parametros})
                })
                .collect(),
        );
    }
    corpo
}

pub(crate) fn interpretar(
    v: &Value,
    modelo_pedido: &str,
    precos: (u64, u64),
) -> ResultadoDeIa<RespostaDeConversa> {
    let blocos = v
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ErroDeIa::RespostaInvalida("sem conteúdo na resposta".to_owned()))?;
    let mut texto = String::new();
    let mut chamadas = Vec::new();
    for b in blocos {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = b.get("text").and_then(Value::as_str) {
                    texto.push_str(t);
                }
            }
            Some("tool_use") => {
                if let Some(nome) = b.get("name").and_then(Value::as_str) {
                    chamadas.push(ChamadaDeFerramenta {
                        id: b.get("id").and_then(Value::as_str).map_or_else(
                            || format!("chamada_{}", chamadas.len() + 1),
                            str::to_owned,
                        ),
                        nome: nome.to_owned(),
                        argumentos: b.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
            }
            _ => {}
        }
    }
    let uso = v.get("usage");
    let numero = |k: &str| {
        uso.and_then(|u| u.get(k))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let entrada = numero("input_tokens")
        + numero("cache_creation_input_tokens")
        + numero("cache_read_input_tokens");
    let saida = numero("output_tokens");
    let texto = texto.trim().to_owned();
    Ok(RespostaDeConversa {
        texto: (!texto.is_empty()).then_some(texto),
        chamadas,
        consumo: Consumo {
            tokens_entrada: entrada,
            tokens_saida: saida,
            custo_dezmilesimos_usd: custo_por_tokens(entrada, saida, precos.0, precos.1),
            nominal: false,
        },
        modelo: v
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(modelo_pedido)
            .to_owned(),
    })
}

impl ProvedorDeConversa for Anthropic {
    fn nome(&self) -> String {
        "anthropic".to_owned()
    }

    fn modelo(&self) -> String {
        self.config.modelo.clone()
    }

    fn natureza_do_custo(&self) -> NaturezaDoCusto {
        NaturezaDoCusto::Real
    }

    fn endereco(&self) -> String {
        self.base.clone()
    }

    fn conversar(
        &self,
        pedido: &PedidoDeConversa,
        cancelar: &AtomicBool,
    ) -> ResultadoDeIa<RespostaDeConversa> {
        let chave = self
            .fonte
            .chave()
            .ok_or_else(|| ErroDeIa::Indisponivel("chave da Anthropic ausente".to_owned()))?;
        let bytes = serde_json::to_vec(&corpo_do_pedido(&self.config.modelo, pedido))
            .map_err(|_| ErroDeIa::Outro("montar o pedido".to_owned()))?;
        let http = PedidoHttp::post(format!("{}/messages", self.base))
            .aceitar("application/json")
            // RISCO (fora do escopo desta frente: a bancada é outro repositório): o `Debug` do
            // `PedidoHttp` da bancada imprime os cabeçalhos, inclusive este `x-api-key`. Por isso
            // o pedido nunca é impresso, logado nem anexado a erro neste arquivo. A correção de
            // verdade é a bancada mascarar `x-api-key` (ou ganhar autorização configurável).
            .cabecalho("x-api-key", chave.expor())
            .cabecalho("anthropic-version", VERSAO_DA_API)
            .json(bytes)
            .prazo(self.config.prazo);
        drop(chave);
        let resposta = executar(&self.rede, http, cancelar)
            .map_err(|e| mapear(&self.rede, "Anthropic", &e))?;
        let v: Value = serde_json::from_slice(&resposta.corpo)
            .map_err(|_| ErroDeIa::Outro("resposta da Anthropic ilegível".to_owned()))?;
        let precos = self
            .config
            .precos
            .unwrap_or_else(|| precos_do_modelo(&self.config.modelo));
        interpretar(&v, &self.config.modelo, precos)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::conversa::DefinicaoDeFerramenta;

    #[test]
    fn o_corpo_separa_o_sistema_e_junta_os_resultados() {
        let p = PedidoDeConversa {
            mensagens: vec![
                Mensagem::Sistema("regras".into()),
                Mensagem::Usuario("pedido".into()),
                Mensagem::Assistente {
                    texto: Some("vou ver".into()),
                    chamadas: vec![
                        ChamadaDeFerramenta {
                            id: "a".into(),
                            nome: "ver_item".into(),
                            argumentos: json!({}),
                        },
                        ChamadaDeFerramenta {
                            id: "b".into(),
                            nome: "rastro_do_item".into(),
                            argumentos: json!({}),
                        },
                    ],
                },
                Mensagem::Ferramenta {
                    id: "a".into(),
                    nome: "ver_item".into(),
                    conteudo: "x".into(),
                },
                Mensagem::Ferramenta {
                    id: "b".into(),
                    nome: "rastro_do_item".into(),
                    conteudo: "y".into(),
                },
            ],
            ferramentas: vec![DefinicaoDeFerramenta {
                nome: "ver_item".into(),
                descricao: "d".into(),
                parametros: json!({"type": "object"}),
            }],
            max_tokens_saida: 400,
        };
        let c = corpo_do_pedido("m", &p);
        assert_eq!(c["system"], "regras");
        assert_eq!(c["messages"].as_array().map(Vec::len), Some(3));
        assert_eq!(c["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(
            c["messages"][2]["content"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(c["messages"][2]["content"][1]["tool_use_id"], "b");
        assert_eq!(c["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn interpreta_texto_chamadas_e_custo() {
        let v = json!({
            "model": "claude-x",
            "content": [
                {"type": "text", "text": "ok"},
                {"type": "tool_use", "id": "t1", "name": "ver_item", "input": {"pagina": 1}}
            ],
            "usage": {"input_tokens": 1_000_000, "output_tokens": 0}
        });
        let r = interpretar(&v, "m", (30_000, 150_000)).expect("resposta");
        assert_eq!(r.texto.as_deref(), Some("ok"));
        assert_eq!(r.chamadas[0].nome, "ver_item");
        assert_eq!(r.chamadas[0].argumentos, json!({"pagina": 1}));
        assert_eq!(r.consumo.custo_dezmilesimos_usd, 30_000);
    }
}
