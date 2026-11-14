//! Provedor `OpenRouter`: a reserva paga (spec 09, D1). HTTPS pelo portão da bancada
//! ([`bancada_rede`]): só o endereço do catálogo, e a chave vem de fora, de uma
//! [`FonteDaChave`] (o chaveiro fica com quem chama), nunca de argv, log ou disco.
//!
//! O gasto é real e tem dois tetos, em dezmilésimos de dólar: **por rodada** (zera em
//! [`Provedor::nova_rodada`]) e **por mês** (zera na virada do mês UTC). Atingido qualquer um,
//! nenhum pedido sai e o erro é [`ErroDeIa::Teto`].

use crate::claude_cli::dezmilesimos;
use crate::esquema::{extrair_json, validar_esquema};
use crate::tempo::{agora_unix, mes_de};
use crate::{Consumo, Degrau, ErroDeIa, Pedido, Provedor, Resposta, ResultadoDeIa};
use bancada_rede::{
    Autorizacao, Catalogo, ConfiguracaoDaRede, Endereco, ErroDeRede, FonteDeAutorizacao,
    Pedido as PedidoHttp, Rede, Segredo,
};
use serde_json::{Value, json};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

const BASE: &str = "https://openrouter.ai/api/v1";
const AUTORIDADE: &str = "openrouter.ai";

/// De onde vem a chave do OpenRouter. Chamada a cada pedido, para trocar ou apagar a chave
/// valer no pedido seguinte. Uma closure `Fn() -> Option<Segredo>` serve.
pub trait FonteDaChave: Send + Sync {
    fn chave(&self) -> Option<Segredo>;
}

impl<F> FonteDaChave for F
where
    F: Fn() -> Option<Segredo> + Send + Sync,
{
    fn chave(&self) -> Option<Segredo> {
        self()
    }
}

struct Autorizador(Arc<dyn FonteDaChave>);

impl fmt::Debug for Autorizador {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Autorizador(***)")
    }
}

impl FonteDeAutorizacao for Autorizador {
    fn autorizacao(&self, _endereco: &Endereco) -> Option<Autorizacao> {
        self.0.chave().and_then(|c| Autorizacao::portador(&c))
    }
}

/// Configuração do OpenRouter.
#[derive(Debug, Clone)]
pub struct ConfiguracaoOpenRouter {
    /// Modelo do degrau rápido (formato `fornecedor/modelo` do OpenRouter).
    pub modelo_rapido: String,
    pub modelo_forte: String,
    /// Teto por rodada, em dezmilésimos de dólar. `0` bloqueia tudo.
    pub teto_rodada_dezmilesimos: u64,
    /// Teto por mês, idem.
    pub teto_mes_dezmilesimos: u64,
    pub prazo: Duration,
    /// Só para testes: base `http://127.0.0.1:porta` de um servidor falso em loopback, no
    /// lugar do host oficial.
    pub base_de_teste: Option<String>,
}

impl Default for ConfiguracaoOpenRouter {
    fn default() -> Self {
        Self {
            modelo_rapido: "anthropic/claude-haiku-4.5".to_owned(),
            modelo_forte: "anthropic/claude-sonnet-4.5".to_owned(),
            teto_rodada_dezmilesimos: 50_000, // US$ 5,00
            teto_mes_dezmilesimos: 200_000,   // US$ 20,00
            prazo: Duration::from_secs(180),
            base_de_teste: None,
        }
    }
}

#[derive(Debug, Default)]
struct Gasto {
    rodada: u64,
    mes: u64,
    mes_chave: String,
}

/// O OpenRouter como provedor.
pub struct OpenRouter {
    rede: Rede,
    config: ConfiguracaoOpenRouter,
    gasto: Mutex<Gasto>,
    mes_atual: Box<dyn Fn() -> String + Send + Sync>,
}

impl fmt::Debug for OpenRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenRouter")
            .field("gasto", &self.gasto())
            .finish_non_exhaustive()
    }
}

impl OpenRouter {
    /// # Errors
    /// [`ErroDeIa::Indisponivel`] se o portão de rede não abre.
    pub fn novo(
        fonte: Arc<dyn FonteDaChave>,
        config: ConfiguracaoOpenRouter,
    ) -> ResultadoDeIa<Self> {
        let endereco = Endereco::novo("openrouter", BASE)
            .map_err(|_| ErroDeIa::Indisponivel("endereço do OpenRouter".to_owned()))?
            .com_autorizacao();
        let mut rede = ConfiguracaoDaRede::nova(Catalogo::novo().com(endereco))
            .autorizacao(Arc::new(Autorizador(fonte)))
            .prazo(config.prazo)
            .agente_de_usuario("sbf-ia");
        if let Some(base) = &config.base_de_teste {
            rede = rede.rota(AUTORIDADE, base);
        }
        let rede = Rede::abrir(rede)
            .map_err(|e| ErroDeIa::Indisponivel(format!("portão de rede: {e}")))?;
        Ok(Self {
            rede,
            config,
            gasto: Mutex::new(Gasto::default()),
            mes_atual: Box::new(|| mes_de(agora_unix())),
        })
    }

    /// Troca a fonte do mês corrente (`AAAA-MM`); para testar a virada do mês.
    #[must_use]
    pub fn com_mes_atual(mut self, mes: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.mes_atual = Box::new(mes);
        self
    }

    /// Restaura o gasto do mês corrente (quem chama soma a `saida_ia` do mês ao abrir o app).
    pub fn definir_gasto_do_mes(&self, dezmilesimos: u64) {
        let mut g = self.travar();
        g.mes_chave = (self.mes_atual)();
        g.mes = dezmilesimos;
    }

    /// `(rodada, mês)` gastos, em dezmilésimos de dólar.
    pub fn gasto(&self) -> (u64, u64) {
        let mut g = self.travar();
        self.virar_mes(&mut g);
        (g.rodada, g.mes)
    }

    fn travar(&self) -> std::sync::MutexGuard<'_, Gasto> {
        self.gasto.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn virar_mes(&self, g: &mut Gasto) {
        let agora = (self.mes_atual)();
        if g.mes_chave != agora {
            g.mes_chave = agora;
            g.mes = 0;
        }
    }

    fn sob_o_teto(&self) -> bool {
        let (rodada, mes) = self.gasto();
        rodada < self.config.teto_rodada_dezmilesimos && mes < self.config.teto_mes_dezmilesimos
    }

    fn somar(&self, dezmilesimos: u64) {
        let mut g = self.travar();
        self.virar_mes(&mut g);
        g.rodada = g.rodada.saturating_add(dezmilesimos);
        g.mes = g.mes.saturating_add(dezmilesimos);
    }

    fn modelo_do(&self, degrau: Degrau) -> &str {
        match degrau {
            Degrau::Rapido => &self.config.modelo_rapido,
            Degrau::Forte => &self.config.modelo_forte,
        }
    }
}

impl Provedor for OpenRouter {
    fn nome(&self) -> &'static str {
        "openrouter"
    }

    fn nova_rodada(&self) {
        self.travar().rodada = 0;
    }

    fn dentro_do_teto(&self) -> bool {
        self.sob_o_teto()
    }

    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        if !self.sob_o_teto() {
            return Err(ErroDeIa::Teto);
        }
        let sistema = format!(
            "{}\n\nResponda somente com um JSON que siga este esquema JSON:\n{}",
            pedido.instrucao, pedido.esquema
        );
        let corpo = json!({
            "model": self.modelo_do(pedido.degrau),
            "messages": [
                {"role": "system", "content": sistema},
                {"role": "user", "content": pedido.entrada},
            ],
            "max_tokens": pedido.max_tokens_saida,
            "temperature": 0,
            "response_format": {"type": "json_object"},
            "usage": {"include": true},
        });
        let bytes = serde_json::to_vec(&corpo)
            .map_err(|_| ErroDeIa::Outro("montar o pedido".to_owned()))?;
        let http = PedidoHttp::post(format!("{BASE}/chat/completions"))
            .aceitar("application/json")
            .json(bytes)
            .autorizado();
        let resposta = self.rede.executar(&http).map_err(|e| self.mapear(&e))?;
        let v: Value = serde_json::from_slice(&resposta.corpo)
            .map_err(|_| ErroDeIa::Outro("resposta do OpenRouter ilegível".to_owned()))?;
        let uso = v.get("usage");
        let numero = |k: &str| {
            uso.and_then(|u| u.get(k))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        let custo = uso
            .and_then(|u| u.get("cost"))
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c >= 0.0)
            .map_or(0, dezmilesimos);
        // O gasto vale mesmo que a resposta venha fora do esquema.
        self.somar(custo);
        let texto = v
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| ErroDeIa::RespostaInvalida("sem conteúdo na resposta".to_owned()))?;
        let json = extrair_json(texto).map_err(ErroDeIa::RespostaInvalida)?;
        validar_esquema(&pedido.esquema, &json).map_err(ErroDeIa::RespostaInvalida)?;
        let modelo = v
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_else(|| self.modelo_do(pedido.degrau))
            .to_owned();
        Ok(Resposta {
            json,
            consumo: Consumo {
                tokens_entrada: numero("prompt_tokens"),
                tokens_saida: numero("completion_tokens"),
                custo_dezmilesimos_usd: custo,
                nominal: false,
            },
            modelo,
        })
    }
}

impl OpenRouter {
    /// Erro de rede para erro de IA, sem corpo nem cabeçalho (podem ecoar o pedido).
    fn mapear(&self, erro: &ErroDeRede) -> ErroDeIa {
        match erro {
            ErroDeRede::Limitado { ate } => {
                let agora = self.rede.relogio().agora();
                ErroDeIa::Limite {
                    espera_segundos: ate.desde(agora).as_secs().max(1),
                }
            }
            ErroDeRede::Http { status: 402, .. } => ErroDeIa::Teto,
            ErroDeRede::AutorizacaoRecusada { .. } | ErroDeRede::SemCredencial { .. } => {
                ErroDeIa::Indisponivel("chave do OpenRouter ausente ou recusada".to_owned())
            }
            ErroDeRede::Http { status, .. } => {
                ErroDeIa::Outro(format!("OpenRouter: HTTP {status}"))
            }
            outro if outro.e_falta_de_rede() => {
                ErroDeIa::Outro("OpenRouter inacessível".to_owned())
            }
            _ => ErroDeIa::Outro("falha ao falar com o OpenRouter".to_owned()),
        }
    }
}
