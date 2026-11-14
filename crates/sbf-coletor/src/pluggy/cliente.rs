//! O cliente REST da Pluggy e a normalização das contas e transações (`pluggy-client.ts` e
//! `pluggy-provider.ts`). Autenticação por `clientId`/`clientSecret` → `apiKey` (cache de ~2 h).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::{ConfigPluggy, Segredo};
use crate::erro::ErroDeColeta;
use crate::gmail::cliente::sem_url;

const SERVICO: &str = "pluggy";
/// A Pluggy documenta 2 h; renova-se um pouco antes.
const VALIDADE_DA_CHAVE: Duration = Duration::from_secs(110 * 60);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContaPluggy {
    pub id: String,
    #[serde(default, rename = "type")]
    pub tipo: String,
    pub subtype: Option<String>,
    #[serde(default)]
    pub name: String,
    pub number: Option<String>,
    pub balance: Option<f64>,
    #[serde(default)]
    pub currency_code: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ComerciantePluggy {
    pub name: Option<String>,
    pub business_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransacaoPluggy {
    pub id: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub description: String,
    pub description_raw: Option<String>,
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub currency_code: String,
    pub category: Option<String>,
    #[serde(default, rename = "type")]
    pub tipo: String,
    pub merchant: Option<ComerciantePluggy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direcao {
    Entrada,
    Saida,
}

impl Direcao {
    /// O valor do contrato de draft (`inflow`/`outflow`).
    pub fn como_texto(self) -> &'static str {
        match self {
            Self::Entrada => "inflow",
            Self::Saida => "outflow",
        }
    }
}

/// `NormalizedTransaction`: sempre em magnitude positiva, com a direção à parte.
#[derive(Debug, Clone, PartialEq)]
pub struct TransacaoNormalizada {
    pub id_externo: String,
    pub direcao: Direcao,
    pub centavos: i64,
    pub moeda: String,
    /// `AAAA-MM-DD`.
    pub data: String,
    pub descricao: String,
    pub descricao_bruta: Option<String>,
    pub categoria: Option<String>,
    pub nome_do_comerciante: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContaNormalizada {
    pub id_externo: String,
    pub tipo: String,
    pub saldo_em_centavos: Option<i64>,
}

/// `Math.round` do JavaScript: meio arredonda para cima, também nos negativos.
pub(crate) fn arredondar_como_js(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn para_centavos(valor: f64) -> i64 {
    // Os valores cabem em i64 com folga; o corte é só para o compilador.
    arredondar_como_js(valor * 100.0) as i64
}

/// `type` (DEBIT/CREDIT) é a fonte de verdade da direção, não o sinal de `amount`, que a
/// Pluggy documenta como invertido para cartão de crédito.
pub fn normalizar_transacao(t: &TransacaoPluggy) -> TransacaoNormalizada {
    let comerciante = t.merchant.as_ref();
    TransacaoNormalizada {
        id_externo: t.id.clone(),
        direcao: if t.tipo == "CREDIT" {
            Direcao::Entrada
        } else {
            Direcao::Saida
        },
        centavos: para_centavos(t.amount.abs()),
        moeda: t.currency_code.clone(),
        data: t.date.chars().take(10).collect(),
        descricao: t.description.clone(),
        descricao_bruta: t.description_raw.clone(),
        categoria: t.category.clone(),
        // `businessName ?? name`: só a ausência cai para o `name`; texto vazio fica.
        nome_do_comerciante: comerciante.and_then(|c| c.business_name.clone().or(c.name.clone())),
    }
}

pub fn normalizar_conta(c: &ContaPluggy) -> ContaNormalizada {
    ContaNormalizada {
        id_externo: c.id.clone(),
        tipo: c.tipo.clone(),
        saldo_em_centavos: c.balance.map(para_centavos),
    }
}

/// Uma página de transações já normalizadas; `proximo_cursor` é só o token `after`.
#[derive(Debug, Clone, Default)]
pub struct PaginaDeTransacoes {
    pub transacoes: Vec<TransacaoNormalizada>,
    pub proximo_cursor: Option<String>,
}

#[derive(Deserialize)]
struct RespostaDeTransacoes {
    #[serde(default)]
    results: Vec<TransacaoPluggy>,
    next: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RespostaDaAutenticacao {
    api_key: String,
}

fn decodificar_percentual(texto: &str) -> String {
    let bytes = texto.as_bytes();
    let mut saida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let par = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                match par {
                    Some(b) => {
                        saida.push(b);
                        i += 2;
                    }
                    None => saida.push(b'%'),
                }
            }
            b => saida.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

/// Reduz o `next` da v2 ao token de paginação (`extrairCursor`).
///
/// A v2 devolve a próxima página como uma query string pronta
/// (`?accountId=…&dateFrom=…&after=<token>`) e só o `after` é cursor. Guardar só ele evita que
/// um cursor gravado para a conta A, reaplicado na conta B, importe as transações de A; e vale
/// também na entrada: um cursor antigo salvo no formato errado volta a funcionar sozinho. `?` e
/// `&` não existem no alfabeto do base64, então distinguem a query string de um token limpo.
pub fn extrair_cursor(proximo: Option<&str>) -> Option<String> {
    let p = proximo.filter(|p| !p.is_empty())?;
    let e_query = p.starts_with('?') || p.starts_with("after=") || p.contains("&after=");
    if !e_query {
        return Some(p.to_owned());
    }
    let query = p.strip_prefix('?').unwrap_or(p);
    query.split('&').find_map(|par| {
        let (k, v) = par.split_once('=')?;
        (k == "after").then(|| decodificar_percentual(v))
    })
}

pub struct ClientePluggy {
    http: reqwest::Client,
    config: ConfigPluggy,
    chave: Mutex<Option<(Segredo, Instant)>>,
}

impl std::fmt::Debug for ClientePluggy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientePluggy")
            .field("base_api", &self.config.base_api)
            .finish_non_exhaustive()
    }
}

impl ClientePluggy {
    pub fn novo(config: ConfigPluggy) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            http,
            config,
            chave: Mutex::new(None),
        }
    }

    async fn chave_da_api(&self) -> Result<String, ErroDeColeta> {
        {
            let guarda = self.chave.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((c, expira)) = guarda.as_ref()
                && Instant::now() < *expira
            {
                return Ok(c.expor().to_owned());
            }
        }
        let resposta = self
            .http
            .post(format!("{}/auth", self.config.base_api))
            .json(&serde_json::json!({
                "clientId": self.config.client_id,
                "clientSecret": self.config.client_secret.expor(),
            }))
            .send()
            .await
            .map_err(|e| ErroDeColeta::Rede {
                servico: SERVICO,
                detalhe: sem_url(e),
            })?;
        let status = resposta.status();
        if !status.is_success() {
            return Err(ErroDeColeta::Api {
                servico: SERVICO,
                status: status.as_u16(),
                caminho: "/auth".to_owned(),
                corpo: String::new(),
            });
        }
        let dados: RespostaDaAutenticacao =
            resposta.json().await.map_err(|_| ErroDeColeta::Resposta {
                servico: SERVICO,
                detalhe: "a resposta de /auth não tem apiKey".to_owned(),
            })?;
        *self.chave.lock().unwrap_or_else(|e| e.into_inner()) = Some((
            Segredo::novo(dados.api_key.clone()),
            Instant::now() + VALIDADE_DA_CHAVE,
        ));
        Ok(dados.api_key)
    }

    async fn pedir<T: DeserializeOwned>(
        &self,
        caminho: &str,
        parametros: &[(&str, &str)],
    ) -> Result<T, ErroDeColeta> {
        let chave = self.chave_da_api().await?;
        let resposta = self
            .http
            .get(format!("{}{caminho}", self.config.base_api))
            .query(parametros)
            .header("X-API-KEY", chave)
            .send()
            .await
            .map_err(|e| ErroDeColeta::Rede {
                servico: SERVICO,
                detalhe: sem_url(e),
            })?;
        let status = resposta.status();
        if !status.is_success() {
            // O corpo de erro da Pluggy pode ecoar identificadores: fica de fora da mensagem.
            return Err(ErroDeColeta::Api {
                servico: SERVICO,
                status: status.as_u16(),
                caminho: caminho.to_owned(),
                corpo: String::new(),
            });
        }
        resposta.json().await.map_err(|e| ErroDeColeta::Resposta {
            servico: SERVICO,
            detalhe: format!("{caminho}: {}", sem_url(e)),
        })
    }

    /// As contas de um item (`/accounts?itemId=`).
    pub async fn listar_contas(&self, item: &str) -> Result<Vec<ContaNormalizada>, ErroDeColeta> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            results: Vec<ContaPluggy>,
        }
        let r: R = self.pedir("/accounts", &[("itemId", item)]).await?;
        Ok(r.results.iter().map(normalizar_conta).collect())
    }

    /// Transações de uma conta pela `/v2/transactions`: filtros `dateFrom`/`dateTo`, sem
    /// `page`/`pageSize` (a v2 recusa com 400), paginação por `after`.
    pub async fn listar_transacoes(
        &self,
        conta: &str,
        de: Option<&str>,
        ate: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<PaginaDeTransacoes, ErroDeColeta> {
        let after = extrair_cursor(cursor);
        let mut p: Vec<(&str, &str)> = vec![("accountId", conta)];
        if let Some(d) = de {
            p.push(("dateFrom", d));
        }
        if let Some(a) = ate {
            p.push(("dateTo", a));
        }
        if let Some(a) = after.as_deref() {
            p.push(("after", a));
        }
        let r: RespostaDeTransacoes = self.pedir("/v2/transactions", &p).await?;
        Ok(PaginaDeTransacoes {
            transacoes: r.results.iter().map(normalizar_transacao).collect(),
            proximo_cursor: extrair_cursor(r.next.as_deref()),
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cursor_de_query_string_vira_so_o_after() {
        assert_eq!(
            extrair_cursor(Some("?accountId=c1&dateFrom=2026-01-01&after=AbC123%3D%3D")).as_deref(),
            Some("AbC123==")
        );
        assert_eq!(extrair_cursor(Some("after=tok")).as_deref(), Some("tok"));
        assert_eq!(
            extrair_cursor(Some("x=1&after=tok")).as_deref(),
            Some("tok")
        );
        assert_eq!(
            extrair_cursor(Some("tokenLimpo123")).as_deref(),
            Some("tokenLimpo123")
        );
        assert_eq!(extrair_cursor(Some("?accountId=c1")), None);
        assert_eq!(extrair_cursor(Some("")), None);
        assert_eq!(extrair_cursor(None), None);
    }

    #[test]
    fn arredondamento_do_javascript() {
        assert_eq!(arredondar_como_js(2.5), 3.0);
        assert_eq!(arredondar_como_js(-2.5), -2.0);
        assert_eq!(arredondar_como_js(1234.4999999999998), 1234.0);
        assert_eq!(para_centavos(19.99), 1999);
        assert_eq!(para_centavos(0.1 + 0.2), 30);
    }

    fn transacao(json: serde_json::Value) -> TransacaoPluggy {
        serde_json::from_value(json).expect("transação")
    }

    #[test]
    fn credito_de_cartao_com_sinal_invertido_e_saida() {
        let t = transacao(serde_json::json!({
            "id": "t1", "accountId": "c1", "date": "2026-03-10T03:00:00.000Z",
            "description": "Loja Exemplo", "amount": 123.45, "currencyCode": "BRL",
            "category": "Shopping", "type": "DEBIT", "status": "POSTED",
            "merchant": {"name": "Loja", "businessName": "Loja Exemplo LTDA"}
        }));
        let n = normalizar_transacao(&t);
        assert_eq!(n.direcao, Direcao::Saida);
        assert_eq!(n.centavos, 12345);
        assert_eq!(n.data, "2026-03-10");
        assert_eq!(n.nome_do_comerciante.as_deref(), Some("Loja Exemplo LTDA"));

        let pagamento = transacao(serde_json::json!({
            "id": "t2", "accountId": "c1", "date": "2026-03-11", "description": "Pagamento",
            "amount": -500.0, "currencyCode": "BRL", "category": null, "type": "CREDIT",
            "status": "PENDING", "merchant": {"name": "Só o nome"}
        }));
        let n = normalizar_transacao(&pagamento);
        assert_eq!(n.direcao, Direcao::Entrada);
        assert_eq!(n.centavos, 50000);
        assert_eq!(n.categoria, None);
        assert_eq!(n.nome_do_comerciante.as_deref(), Some("Só o nome"));
    }
}
