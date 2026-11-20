//! Provedor `ClaudeCli`: o Claude Code em modo headless (`claude -p`), na assinatura do CEO
//! (spec 09, D1). O custo que ele informa é **nominal** (o que custaria na API).
//!
//! Regras (IA-05):
//! - o prompt (instrução, esquema e entrada) vai **só pela entrada padrão**; o argv tem apenas
//!   opções fixas e o nome do modelo;
//! - sem ferramentas (`--tools ""`), sem sessão gravada, sem servidores MCP, sem comandos e
//!   sem configurações de projeto: um turno de texto;
//! - o diretório de trabalho é novo, vazio e 0700, sob `$XDG_RUNTIME_DIR`, apagado no fim;
//! - ambiente mínimo ([`sbf_processo::comando`]) mais o que o `claude` precisa para achar a
//!   sessão da assinatura (`HOME` e afins). Chave de API do shell **não** passa: ela faria o
//!   `claude` cobrar por token em vez de usar a assinatura;
//! - nada do texto enviado entra em mensagem de erro.

use crate::esquema::{extrair_json, validar_esquema};
use crate::tempo::agora_unix;
use crate::{Consumo, Degrau, ErroDeIa, Pedido, Provedor, Resposta, ResultadoDeIa};
use sbf_processo::{
    FalhaDeProcesso, comando, escrever_e_fechar, esperar, ler_com_limite, ler_o_fim,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// O que o `claude` herda do app, além do ambiente mínimo: onde ele guarda a sessão.
const HERDADAS: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "CLAUDE_CONFIG_DIR",
];

const SISTEMA: &str = "Você é um classificador de documentos. Responda somente com o JSON pedido, sem texto em volta e sem usar ferramentas.";

const SAIDA_MAXIMA: usize = 4 * 1024 * 1024;
const ERRO_MAXIMO: usize = 16 * 1024;
const ESPERA_PADRAO_DO_LIMITE: u64 = 300;

static CONTADOR: AtomicU64 = AtomicU64::new(0);

/// O `claude` como provedor.
#[derive(Debug, Clone)]
pub struct ClaudeCli {
    caminho: PathBuf,
    modelo_rapido: String,
    modelo_forte: String,
    prazo: Duration,
    espera_do_limite: u64,
    pasta_base: Option<PathBuf>,
}

/// O `claude` por um caminho absoluto de confiança: primeiro `~/.local/bin` (a instalação
/// padrão), depois cada pasta absoluta do `PATH` do usuário, depois as pastas do sistema.
/// É a única busca do binário `claude`: a triagem, Rita e a CLI a usam.
pub fn resolver_claude() -> Option<PathBuf> {
    let casa = std::env::var_os("HOME").map(PathBuf::from);
    let path = std::env::var_os("PATH");
    resolver_claude_em(casa.as_deref(), path.as_deref())
}

/// O miolo testável de [`resolver_claude`].
pub fn resolver_claude_em(casa: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let mut candidatos: Vec<PathBuf> = Vec::new();
    if let Some(casa) = casa.filter(|c| c.is_absolute()) {
        candidatos.push(casa.join(".local/bin/claude"));
    }
    if let Some(path) = path {
        candidatos.extend(
            std::env::split_paths(path)
                .filter(|p| p.is_absolute())
                .map(|p| p.join("claude")),
        );
    }
    if let Some(sistema) = sbf_processo::localizar("claude") {
        candidatos.push(sistema);
    }
    candidatos.into_iter().find(|c| sbf_processo::confiavel(c))
}

impl ClaudeCli {
    /// `caminho` é o binário `claude` (absoluto, em pasta que ninguém mais grava). Modelos
    /// pelos apelidos do Claude Code: `haiku` (rápido) e `sonnet` (forte).
    pub fn novo(caminho: impl Into<PathBuf>) -> Self {
        Self {
            caminho: caminho.into(),
            modelo_rapido: "haiku".to_owned(),
            modelo_forte: "sonnet".to_owned(),
            prazo: Duration::from_secs(300),
            espera_do_limite: ESPERA_PADRAO_DO_LIMITE,
            pasta_base: None,
        }
    }

    /// O `claude` pelo caminho de confiança que [`resolver_claude`] acha (a instalação do
    /// usuário em `~/.local/bin`, o `PATH` e as pastas do sistema). É a busca única do app.
    pub fn do_ambiente() -> Option<Self> {
        resolver_claude().map(Self::novo)
    }

    /// Procura `claude` nas pastas do sistema (nunca pelo `PATH` do usuário). Quem instalou
    /// em `~/.local/bin` informa o caminho com [`ClaudeCli::novo`].
    pub fn do_sistema() -> Option<Self> {
        sbf_processo::localizar("claude").map(Self::novo)
    }

    /// Troca os nomes dos modelos dos degraus.
    #[must_use]
    pub fn com_modelos(mut self, rapido: impl Into<String>, forte: impl Into<String>) -> Self {
        self.modelo_rapido = rapido.into();
        self.modelo_forte = forte.into();
        self
    }

    /// Prazo de uma chamada (padrão 5 minutos).
    #[must_use]
    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    /// Espera sugerida quando o limite não diz até quando (padrão 300 s).
    #[must_use]
    pub fn com_espera_do_limite(mut self, segundos: u64) -> Self {
        self.espera_do_limite = segundos;
        self
    }

    /// Onde criar o diretório de trabalho (padrão `$XDG_RUNTIME_DIR`, depois a pasta
    /// temporária do sistema).
    #[must_use]
    pub fn com_pasta_base(mut self, pasta: impl Into<PathBuf>) -> Self {
        self.pasta_base = Some(pasta.into());
        self
    }

    fn modelo_do(&self, degrau: Degrau) -> &str {
        match degrau {
            Degrau::Rapido => &self.modelo_rapido,
            Degrau::Forte => &self.modelo_forte,
        }
    }

    fn base(&self) -> PathBuf {
        self.pasta_base
            .clone()
            .or_else(|| {
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute() && p.is_dir())
            })
            .unwrap_or_else(std::env::temp_dir)
    }
}

/// Diretório de trabalho novo, vazio e 0700; apagado ao descartar.
pub(crate) struct PastaDeTrabalho(PathBuf);

impl PastaDeTrabalho {
    fn criar(base: &Path) -> std::io::Result<Self> {
        let nome = format!(
            "sbf-ia-{}-{}-{}",
            std::process::id(),
            agora_unix(),
            CONTADOR.fetch_add(1, Ordering::SeqCst)
        );
        let caminho = base.join(nome);
        let mut construtor = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            construtor.mode(0o700);
        }
        construtor.create(&caminho)?; // falha se já existe: nunca reaproveita
        Ok(Self(caminho))
    }
}

impl Drop for PastaDeTrabalho {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// O texto que vai pela entrada padrão: instrução, esquema e entrada.
fn montar_prompt(pedido: &Pedido) -> Zeroizing<Vec<u8>> {
    let texto = format!(
        "{}\n\nResponda com um único JSON que siga este esquema JSON, sem comentários:\n{}\n\nENTRADA:\n{}\n",
        pedido.instrucao, pedido.esquema, pedido.entrada
    );
    Zeroizing::new(texto.into_bytes())
}

impl Provedor for ClaudeCli {
    fn nome(&self) -> &'static str {
        "claude-cli"
    }

    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        let modelo = self.modelo_do(pedido.degrau);
        let pasta = PastaDeTrabalho::criar(&self.base())
            .map_err(|e| ErroDeIa::Indisponivel(format!("diretório de trabalho: {}", e.kind())))?;
        let argumentos = [
            "-p",
            "--output-format",
            "json",
            "--model",
            modelo,
            "--tools",
            "",
            "--system-prompt",
            SISTEMA,
            "--no-session-persistence",
            "--disable-slash-commands",
            "--strict-mcp-config",
            "--setting-sources",
            "project",
        ];
        let mut c = comando(&self.caminho, argumentos, HERDADAS).map_err(|_| {
            ErroDeIa::Indisponivel("o `claude` não foi encontrado ou não é confiável".to_owned())
        })?;
        c.current_dir(&pasta.0).env(
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            pedido.max_tokens_saida.to_string(),
        );
        let prazo = Instant::now() + self.prazo;
        let mut filho = c.spawn().map_err(|e| match FalhaDeProcesso::from(e) {
            FalhaDeProcesso::NaoEncontrado => {
                ErroDeIa::Indisponivel("o `claude` não pôde ser executado".to_owned())
            }
            outra => ErroDeIa::Outro(outra.to_string()),
        })?;
        let escrita = filho
            .stdin
            .take()
            .map(|stdin| escrever_e_fechar(stdin, montar_prompt(pedido)));
        let stdout = ler_com_limite(filho.stdout.take(), SAIDA_MAXIMA, 0);
        let stderr = ler_o_fim(filho.stderr.take(), ERRO_MAXIMO);
        let status = esperar(&mut filho, Some(prazo)).map_err(|e| match e {
            FalhaDeProcesso::TempoEsgotado => {
                ErroDeIa::Outro("o `claude` passou do prazo".to_owned())
            }
            outra => ErroDeIa::Outro(outra.to_string()),
        })?;
        if let Some(Ok(Err(e))) = escrita.map(std::thread::JoinHandle::join) {
            return Err(ErroDeIa::Outro(format!(
                "entrada do `claude`: {}",
                e.kind()
            )));
        }
        let lido = stdout.join().unwrap_or_default();
        let erro = stderr.join().unwrap_or_default();
        drop(pasta);
        interpretar(
            &lido.bytes,
            &erro,
            status.success(),
            pedido,
            modelo,
            self.espera_do_limite,
        )
    }
}

// ---------------------------------------------------------------------------------------
// Conversa com ferramentas (spec 15, Rita)
// ---------------------------------------------------------------------------------------

/// O servidor MCP que o `claude` sobe como filho: o `sbf mcp --rita ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServidorMcp {
    /// O programa (o próprio `sbf`).
    pub programa: PathBuf,
    pub argumentos: Vec<String>,
    /// O nome do servidor no `claude` (as ferramentas viram `mcp__<nome>__<ferramenta>`).
    pub nome: String,
}

/// O que o `claude -p` devolveu depois de rodar o próprio laço de ferramentas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaidaDoClaudeComFerramentas {
    /// A resposta final do modelo (pode ser vazia).
    pub texto: String,
    pub consumo: Consumo,
    pub modelo: String,
    /// Quantos turnos o `claude` reportou.
    pub turnos: u64,
}

const SISTEMA_COM_FERRAMENTAS: &str = "Você é Rita, a assistente de padrões de reconhecimento do Seu Bolso Feliz. Use somente as ferramentas do servidor MCP `rita`; as instruções vêm no pedido.";

/// Os argumentos do `claude` para uma conversa com as ferramentas de `servidor`, só as de
/// `permitidas` e no máximo `turnos` turnos. Puro, para testar sem processo.
pub(crate) fn argumentos_com_ferramentas(
    modelo: &str,
    configuracao_mcp: &Path,
    servidor: &str,
    permitidas: &[String],
    turnos: u32,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--model",
        modelo,
        "--tools",
        "",
        "--system-prompt",
        SISTEMA_COM_FERRAMENTAS,
        "--no-session-persistence",
        "--disable-slash-commands",
        "--strict-mcp-config",
        "--setting-sources",
        "project",
        "--max-turns",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    a.push(turnos.to_string());
    a.push("--mcp-config".to_owned());
    a.push(configuracao_mcp.display().to_string());
    a.push("--allowedTools".to_owned());
    a.extend(permitidas.iter().map(|f| format!("mcp__{servidor}__{f}")));
    a
}

/// A configuração MCP (JSON) de um servidor. Sem segredo nenhum.
pub(crate) fn configuracao_mcp(servidor: &ServidorMcp) -> Value {
    serde_json::json!({
        "mcpServers": {
            servidor.nome.clone(): {
                "type": "stdio",
                "command": servidor.programa.display().to_string(),
                "args": servidor.argumentos,
            }
        }
    })
}

impl ClaudeCli {
    /// Uma conversa em que o `claude` roda o laço e chama as ferramentas de Rita por MCP
    /// (`servidor`). O `prompt` (instruções e dados, com o texto do documento em bloco
    /// delimitado) vai só pela entrada padrão. As ferramentas `permitidas` são as únicas
    /// habilitadas; as embutidas do `claude` ficam desligadas. `turnos` limita o laço do
    /// próprio `claude`; o teto de 8 chamadas é do servidor MCP.
    ///
    /// # Errors
    /// O `claude` ausente, não autenticado, limite de uso, prazo, interrupção.
    pub fn conversar_com_mcp(
        &self,
        prompt: &str,
        servidor: &ServidorMcp,
        permitidas: &[String],
        turnos: u32,
        cancelar: &std::sync::atomic::AtomicBool,
    ) -> ResultadoDeIa<SaidaDoClaudeComFerramentas> {
        let pasta = PastaDeTrabalho::criar(&self.base())
            .map_err(|e| ErroDeIa::Indisponivel(format!("diretório de trabalho: {}", e.kind())))?;
        let arquivo_mcp = pasta.0.join("mcp.json");
        std::fs::write(
            &arquivo_mcp,
            serde_json::to_vec(&configuracao_mcp(servidor))
                .map_err(|_| ErroDeIa::Outro("montar a configuração MCP".to_owned()))?,
        )
        .map_err(|e| ErroDeIa::Indisponivel(format!("configuração MCP: {}", e.kind())))?;
        let argumentos = argumentos_com_ferramentas(
            &self.modelo_forte,
            &arquivo_mcp,
            &servidor.nome,
            permitidas,
            turnos,
        );
        let mut c = comando(&self.caminho, &argumentos, HERDADAS).map_err(|_| {
            ErroDeIa::Indisponivel("o `claude` não foi encontrado ou não é confiável".to_owned())
        })?;
        c.current_dir(&pasta.0);
        let prazo = Instant::now() + self.prazo;
        let mut filho = c.spawn().map_err(|e| match FalhaDeProcesso::from(e) {
            FalhaDeProcesso::NaoEncontrado => {
                ErroDeIa::Indisponivel("o `claude` não pôde ser executado".to_owned())
            }
            outra => ErroDeIa::Outro(outra.to_string()),
        })?;
        let escrita = filho
            .stdin
            .take()
            .map(|stdin| escrever_e_fechar(stdin, Zeroizing::new(prompt.as_bytes().to_vec())));
        let stdout = ler_com_limite(filho.stdout.take(), SAIDA_MAXIMA, 0);
        let stderr = ler_o_fim(filho.stderr.take(), ERRO_MAXIMO);
        let status = loop {
            if cancelar.load(Ordering::Acquire) {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(ErroDeIa::Interrompido);
            }
            match filho.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => return Err(ErroDeIa::Outro(format!("espera do `claude`: {}", e.kind()))),
            }
            if Instant::now() >= prazo {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(ErroDeIa::Outro("o `claude` passou do prazo".to_owned()));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if let Some(Ok(Err(e))) = escrita.map(std::thread::JoinHandle::join) {
            return Err(ErroDeIa::Outro(format!(
                "entrada do `claude`: {}",
                e.kind()
            )));
        }
        let lido = stdout.join().unwrap_or_default();
        let erro = stderr.join().unwrap_or_default();
        drop(pasta);
        interpretar_com_ferramentas(
            &lido.bytes,
            &erro,
            status.success(),
            &self.modelo_forte,
            self.espera_do_limite,
        )
    }
}

/// Lê a saída do `claude` de uma conversa com ferramentas. Pura.
pub(crate) fn interpretar_com_ferramentas(
    stdout: &[u8],
    stderr: &[u8],
    saiu_bem: bool,
    modelo_pedido: &str,
    espera_do_limite: u64,
) -> ResultadoDeIa<SaidaDoClaudeComFerramentas> {
    let erro_texto = String::from_utf8_lossy(stderr);
    let Some(objeto) = objeto_de_resultado(stdout) else {
        return Err(classificar_erro(
            &erro_texto,
            espera_do_limite,
            "saída ilegível do `claude`",
        ));
    };
    let texto = objeto.get("result").and_then(Value::as_str).unwrap_or("");
    let subtipo = objeto.get("subtype").and_then(Value::as_str).unwrap_or("");
    // Estourar os turnos não é falha: Rita mostra o que o servidor MCP guardou.
    let estourou = subtipo == "error_max_turns";
    let com_erro = !estourou
        && (objeto.get("is_error").and_then(Value::as_bool) == Some(true)
            || subtipo.starts_with("error")
            || !saiu_bem);
    if com_erro {
        let todo = format!("{texto}\n{erro_texto}");
        return Err(classificar_erro(
            &todo,
            espera_do_limite,
            "falha do `claude`",
        ));
    }
    let uso = objeto.get("usage");
    let soma = |chaves: &[&str]| -> u64 {
        chaves
            .iter()
            .filter_map(|k| uso.and_then(|u| u.get(*k)).and_then(Value::as_u64))
            .sum()
    };
    let custo = objeto
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite() && *c >= 0.0)
        .map_or(0, dezmilesimos);
    let modelo = objeto
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|m| m.keys().next().cloned())
        .unwrap_or_else(|| modelo_pedido.to_owned());
    Ok(SaidaDoClaudeComFerramentas {
        texto: texto.trim().to_owned(),
        consumo: Consumo {
            tokens_entrada: soma(&[
                "input_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]),
            tokens_saida: soma(&["output_tokens"]),
            custo_dezmilesimos_usd: custo,
            nominal: true,
        },
        modelo,
        turnos: objeto.get("num_turns").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// Lê a saída `--output-format json` do `claude`. Pura, para testar sem processo.
pub(crate) fn interpretar(
    stdout: &[u8],
    stderr: &[u8],
    saiu_bem: bool,
    pedido: &Pedido,
    modelo_pedido: &str,
    espera_do_limite: u64,
) -> ResultadoDeIa<Resposta> {
    let erro_texto = String::from_utf8_lossy(stderr);
    let Some(objeto) = objeto_de_resultado(stdout) else {
        return Err(classificar_erro(
            &erro_texto,
            espera_do_limite,
            "saída ilegível do `claude`",
        ));
    };
    let texto = objeto.get("result").and_then(Value::as_str).unwrap_or("");
    let subtipo_de_erro = objeto
        .get("subtype")
        .and_then(Value::as_str)
        .is_some_and(|s| s.starts_with("error"));
    let com_erro = objeto.get("is_error").and_then(Value::as_bool) == Some(true)
        || subtipo_de_erro
        || !saiu_bem;
    if com_erro {
        let todo = format!("{texto}\n{erro_texto}");
        return Err(classificar_erro(
            &todo,
            espera_do_limite,
            "falha do `claude`",
        ));
    }
    let json = extrair_json(texto).map_err(ErroDeIa::RespostaInvalida)?;
    validar_esquema(&pedido.esquema, &json).map_err(ErroDeIa::RespostaInvalida)?;
    let uso = objeto.get("usage");
    let soma = |chaves: &[&str]| -> u64 {
        chaves
            .iter()
            .filter_map(|k| uso.and_then(|u| u.get(*k)).and_then(Value::as_u64))
            .sum()
    };
    let custo = objeto
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite() && *c >= 0.0)
        .map_or(0, dezmilesimos);
    let modelo = objeto
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|m| m.keys().next().cloned())
        .unwrap_or_else(|| modelo_pedido.to_owned());
    Ok(Resposta {
        json,
        consumo: Consumo {
            tokens_entrada: soma(&[
                "input_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]),
            tokens_saida: soma(&["output_tokens"]),
            custo_dezmilesimos_usd: custo,
            nominal: true,
        },
        modelo,
    })
}

/// Dólares (do JSON do provedor) em dezmilésimos de dólar, arredondando para cima.
pub(crate) fn dezmilesimos(usd: f64) -> u64 {
    let v = (usd * 10_000.0).ceil();
    if v.is_finite() && v >= 0.0 {
        // Valor pequeno e não negativo, vindo de um custo: o corte é intencional.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            v as u64
        }
    } else {
        0
    }
}

fn objeto_de_resultado(stdout: &[u8]) -> Option<Value> {
    match serde_json::from_slice::<Value>(stdout).ok()? {
        v @ Value::Object(_) => Some(v),
        Value::Array(itens) => itens
            .into_iter()
            .find(|i| i.get("type").and_then(Value::as_str) == Some("result")),
        _ => None,
    }
}

fn classificar_erro(texto: &str, espera_do_limite: u64, padrao: &str) -> ErroDeIa {
    let t = texto.to_lowercase();
    const LIMITE: &[&str] = &[
        "usage limit",
        "limit reached",
        "rate limit",
        "rate_limit",
        "too many requests",
        "overloaded",
        "quota",
        "429",
    ];
    const AUTENTICACAO: &[&str] = &[
        "not logged in",
        "/login",
        "invalid api key",
        "authentication",
        "oauth token",
    ];
    if LIMITE.iter().any(|m| t.contains(m)) {
        return ErroDeIa::Limite {
            espera_segundos: espera_ate_o_fim_do_limite(texto).unwrap_or(espera_do_limite),
        };
    }
    if AUTENTICACAO.iter().any(|m| t.contains(m)) {
        return ErroDeIa::Indisponivel(
            "o `claude` não está autenticado (entre com `claude` e `/login`)".to_owned(),
        );
    }
    // Só o motivo curto: o texto do erro pode repetir parte do que foi enviado.
    ErroDeIa::Outro(padrao.to_owned())
}

/// "Claude AI usage limit reached|1760000000": segundos até o instante (epoch) depois da barra.
fn espera_ate_o_fim_do_limite(texto: &str) -> Option<u64> {
    let (_, cauda) = texto.rsplit_once('|')?;
    let digitos: String = cauda
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let epoch: u64 = digitos.parse().ok()?;
    let espera = epoch.saturating_sub(agora_unix());
    (espera > 0).then_some(espera.min(7 * 86_400))
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn pedido() -> Pedido {
        Pedido {
            prompt_versao: "v1".into(),
            instrucao: "classifique".into(),
            entrada: "DADO-SECRETO-ENTRADA".into(),
            esquema: json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
            degrau: Degrau::Rapido,
            max_tokens_saida: 1000,
        }
    }

    fn saida(result: &str) -> String {
        json!({
            "type":"result","subtype":"success","is_error":false,"result":result,
            "total_cost_usd":0.01234,
            "usage":{"input_tokens":100,"cache_creation_input_tokens":10,
                     "cache_read_input_tokens":5,"output_tokens":20},
            "modelUsage":{"claude-haiku-fake":{}}
        })
        .to_string()
    }

    #[test]
    fn le_resultado_uso_custo_e_modelo() {
        let r = interpretar(
            saida("{\"ok\":true}").as_bytes(),
            b"",
            true,
            &pedido(),
            "haiku",
            300,
        )
        .unwrap();
        assert_eq!(r.json, json!({"ok":true}));
        assert_eq!(r.consumo.tokens_entrada, 115);
        assert_eq!(r.consumo.tokens_saida, 20);
        assert_eq!(r.consumo.custo_dezmilesimos_usd, 124);
        assert!(r.consumo.nominal);
        assert_eq!(r.modelo, "claude-haiku-fake");
    }

    #[test]
    fn aceita_json_cercado_de_cerca_de_codigo() {
        let r = interpretar(
            saida("Aqui:\n```json\n{\"ok\":false}\n```").as_bytes(),
            b"",
            true,
            &pedido(),
            "haiku",
            300,
        )
        .unwrap();
        assert_eq!(r.json, json!({"ok":false}));
    }

    #[test]
    fn fora_do_esquema_e_resposta_invalida_sem_o_conteudo() {
        let e = interpretar(
            saida("{\"ok\":\"talvez\"}").as_bytes(),
            b"",
            true,
            &pedido(),
            "m",
            1,
        )
        .unwrap_err();
        assert!(
            matches!(e, ErroDeIa::RespostaInvalida(ref m) if !m.contains("talvez")),
            "{e}"
        );
        let e =
            interpretar(saida("sem json").as_bytes(), b"", true, &pedido(), "m", 1).unwrap_err();
        assert!(matches!(e, ErroDeIa::RespostaInvalida(_)));
    }

    #[test]
    fn limite_vira_limite_com_espera() {
        let futuro = agora_unix() + 600;
        let corpo = json!({"type":"result","is_error":true,
            "result": format!("Claude AI usage limit reached|{futuro}")})
        .to_string();
        let e = interpretar(corpo.as_bytes(), b"", false, &pedido(), "m", 300).unwrap_err();
        match e {
            ErroDeIa::Limite { espera_segundos } => {
                assert!((590..=600).contains(&espera_segundos), "{espera_segundos}");
            }
            outro => panic!("{outro}"),
        }
        let corpo = json!({"type":"result","is_error":true,"result":"API Error: 429 rate limit"})
            .to_string();
        let e = interpretar(corpo.as_bytes(), b"", false, &pedido(), "m", 77).unwrap_err();
        assert!(matches!(
            e,
            ErroDeIa::Limite {
                espera_segundos: 77
            }
        ));
    }

    #[test]
    fn nao_autenticado_e_indisponivel_e_o_resto_e_outro() {
        let corpo =
            json!({"is_error":true,"result":"Invalid API key · Please run /login"}).to_string();
        assert!(matches!(
            interpretar(corpo.as_bytes(), b"", false, &pedido(), "m", 1),
            Err(ErroDeIa::Indisponivel(_))
        ));
        let corpo = json!({"is_error":true,"result":"DADO-SECRETO-ENTRADA explodiu"}).to_string();
        let e = interpretar(corpo.as_bytes(), b"", false, &pedido(), "m", 1).unwrap_err();
        assert!(
            matches!(e, ErroDeIa::Outro(ref m) if !m.contains("SECRETO")),
            "{e}"
        );
        let e = interpretar(b"nao e json", b"limit reached", false, &pedido(), "m", 9).unwrap_err();
        assert!(matches!(e, ErroDeIa::Limite { espera_segundos: 9 }));
    }

    #[test]
    fn dezmilesimos_arredonda_para_cima() {
        assert_eq!(dezmilesimos(0.0), 0);
        assert_eq!(dezmilesimos(0.0001), 1);
        assert_eq!(dezmilesimos(0.00011), 2);
        assert_eq!(dezmilesimos(1.0), 10_000);
    }

    #[test]
    fn argumentos_da_conversa_so_liberam_as_ferramentas_de_rita() {
        let a = argumentos_com_ferramentas(
            "sonnet",
            Path::new("/x/mcp.json"),
            "rita",
            &["ver_item".to_owned(), "pre_testar".to_owned()],
            9,
        );
        let junto = a.join(" ");
        assert!(
            junto.contains("--tools  --system-prompt")
                || a.windows(2).any(|w| w[0] == "--tools" && w[1].is_empty())
        );
        assert!(junto.contains("--strict-mcp-config"));
        assert!(junto.contains("--max-turns 9"));
        assert!(junto.contains("--mcp-config /x/mcp.json"));
        assert!(junto.ends_with("mcp__rita__ver_item mcp__rita__pre_testar"));
        assert!(!junto.contains("Bash"));
    }

    #[test]
    fn a_configuracao_mcp_nao_leva_segredo_e_aponta_para_o_servidor() {
        let v = configuracao_mcp(&ServidorMcp {
            programa: PathBuf::from("/usr/bin/sbf"),
            argumentos: vec!["mcp".into(), "--rita".into()],
            nome: "rita".into(),
        });
        assert_eq!(v["mcpServers"]["rita"]["command"], "/usr/bin/sbf");
        assert_eq!(v["mcpServers"]["rita"]["args"][1], "--rita");
    }

    #[test]
    fn conversa_com_ferramentas_le_uso_e_aceita_estouro_de_turnos() {
        let ok = json!({
            "type":"result","subtype":"success","is_error":false,"result":"feito",
            "total_cost_usd":0.5,"num_turns":4,
            "usage":{"input_tokens":10,"output_tokens":2},
            "modelUsage":{"claude-sonnet-fake":{}}
        })
        .to_string();
        let r = interpretar_com_ferramentas(ok.as_bytes(), b"", true, "sonnet", 60).expect("saida");
        assert_eq!(r.texto, "feito");
        assert_eq!(r.turnos, 4);
        assert!(r.consumo.nominal);
        assert_eq!(r.modelo, "claude-sonnet-fake");
        let estouro =
            json!({"type":"result","subtype":"error_max_turns","is_error":false,"result":""})
                .to_string();
        assert!(interpretar_com_ferramentas(estouro.as_bytes(), b"", true, "s", 60).is_ok());
        let falha = json!({"type":"result","subtype":"error_during_execution","is_error":true,"result":"x"})
            .to_string();
        assert!(interpretar_com_ferramentas(falha.as_bytes(), b"", false, "s", 60).is_err());
    }
}
