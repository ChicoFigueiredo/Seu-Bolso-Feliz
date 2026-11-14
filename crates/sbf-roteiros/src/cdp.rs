//! Um cliente mínimo do Chrome DevTools Protocol sobre WebSocket, só com a biblioteca padrão
//! (mais `base64` e `getrandom`). Fala só com `127.0.0.1`.
//!
//! **RO-01.** A única função que carrega um segredo é [`Cdp::inserir_texto_secreto`]: monta a
//! mensagem `Input.insertText` num `Zeroizing<String>`, grava o quadro mascarado num
//! `Zeroizing<Vec<u8>>` e não guarda a mensagem em lugar nenhum. Nenhum erro deste módulo repete
//! parâmetros enviados: só o `message` que o Chrome devolve e o nome do método.

use crate::erro::{ErroDeRoteiro, Resultado};
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const MAXIMO_DA_MENSAGEM: usize = 64 * 1024 * 1024;

fn erro(m: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::Navegador(m.into())
}

/// Um GET HTTP simples em `127.0.0.1:porta` (para `/json/version` e `/json/list`).
///
/// # Errors
/// Falha de rede ou resposta que não é JSON.
pub fn http_json(porta: u16, caminho: &str) -> Resultado<Value> {
    let endereco = SocketAddr::from(([127, 0, 0, 1], porta));
    let mut s = TcpStream::connect_timeout(&endereco, Duration::from_secs(3))
        .map_err(|e| erro(format!("não conectou ao Chrome: {e}")))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        s,
        "GET {caminho} HTTP/1.1\r\nHost: 127.0.0.1:{porta}\r\nConnection: close\r\n\r\n"
    )?;
    let mut bruto = Vec::new();
    s.read_to_end(&mut bruto).ok();
    let texto = String::from_utf8_lossy(&bruto);
    let corpo = texto.split_once("\r\n\r\n").map_or("", |(_, c)| c);
    serde_json::from_str(corpo.trim()).map_err(|_| erro("o Chrome respondeu algo que não é JSON"))
}

/// A conexão WebSocket (cliente).
#[derive(Debug)]
struct Ws {
    fluxo: TcpStream,
    entrada: Vec<u8>,
}

fn aleatorio<const N: usize>() -> [u8; N] {
    let mut b = [0_u8; N];
    // Sem fonte de aleatoriedade, a máscara vira zero: o Chrome local aceita do mesmo jeito.
    let _ = getrandom::fill(&mut b);
    b
}

impl Ws {
    fn conectar(porta: u16, caminho: &str) -> Resultado<Self> {
        let endereco = SocketAddr::from(([127, 0, 0, 1], porta));
        let mut fluxo = TcpStream::connect_timeout(&endereco, Duration::from_secs(3))
            .map_err(|e| erro(format!("não conectou ao Chrome: {e}")))?;
        fluxo.set_nodelay(true).ok();
        let chave = base64::engine::general_purpose::STANDARD.encode(aleatorio::<16>());
        write!(
            fluxo,
            "GET {caminho} HTTP/1.1\r\nHost: 127.0.0.1:{porta}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {chave}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )?;
        fluxo.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut cabecalho = Vec::new();
        let mut byte = [0_u8; 1];
        while !cabecalho.ends_with(b"\r\n\r\n") {
            if fluxo.read(&mut byte)? == 0 || cabecalho.len() > 16 * 1024 {
                return Err(erro("o Chrome fechou o handshake do WebSocket"));
            }
            cabecalho.push(byte[0]);
        }
        if !String::from_utf8_lossy(&cabecalho).contains(" 101 ") {
            return Err(erro("o Chrome recusou o WebSocket"));
        }
        Ok(Self {
            fluxo,
            entrada: Vec::new(),
        })
    }

    /// Envia um quadro de texto. O buffer com a máscara aplicada é zerado ao sair.
    fn enviar(&mut self, carga: &[u8]) -> Resultado<()> {
        let mascara = aleatorio::<4>();
        let mut quadro: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(carga.len() + 14));
        quadro.push(0x81);
        let n = carga.len();
        if n < 126 {
            quadro.push(0x80 | n as u8);
        } else if n <= 0xFFFF {
            quadro.push(0x80 | 126);
            quadro.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            quadro.push(0x80 | 127);
            quadro.extend_from_slice(&(n as u64).to_be_bytes());
        }
        quadro.extend_from_slice(&mascara);
        quadro.extend(carga.iter().enumerate().map(|(i, b)| b ^ mascara[i % 4]));
        self.fluxo.write_all(&quadro)?;
        self.fluxo.flush()?;
        Ok(())
    }

    /// Um quadro completo já no buffer? Devolve (opcode, fim, carga) e o consome.
    fn tirar_quadro(&mut self) -> Option<(u8, bool, Vec<u8>)> {
        let b = &self.entrada;
        if b.len() < 2 {
            return None;
        }
        let fim = b[0] & 0x80 != 0;
        let opcode = b[0] & 0x0F;
        let mascarado = b[1] & 0x80 != 0;
        let mut n = u64::from(b[1] & 0x7F);
        let mut pos = 2;
        if n == 126 {
            if b.len() < 4 {
                return None;
            }
            n = u64::from(u16::from_be_bytes([b[2], b[3]]));
            pos = 4;
        } else if n == 127 {
            if b.len() < 10 {
                return None;
            }
            n = u64::from_be_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
            pos = 10;
        }
        let mascara = if mascarado {
            if b.len() < pos + 4 {
                return None;
            }
            let m = [b[pos], b[pos + 1], b[pos + 2], b[pos + 3]];
            pos += 4;
            Some(m)
        } else {
            None
        };
        let n = usize::try_from(n).ok()?;
        if b.len() < pos + n {
            return None;
        }
        let mut carga = b[pos..pos + n].to_vec();
        if let Some(m) = mascara {
            for (i, x) in carga.iter_mut().enumerate() {
                *x ^= m[i % 4];
            }
        }
        self.entrada.drain(..pos + n);
        Some((opcode, fim, carga))
    }

    /// Uma mensagem de texto completa, esperando até `prazo`. `None` se o prazo acabou.
    fn receber(&mut self, prazo: Duration) -> Resultado<Option<String>> {
        let limite = Instant::now() + prazo;
        let mut parcial: Vec<u8> = Vec::new();
        loop {
            while let Some((op, fim, carga)) = self.tirar_quadro() {
                match op {
                    0x8 => return Err(erro("o Chrome fechou a conexão")),
                    0x9 => {
                        // ping: pong com a mesma carga
                        let mut q = vec![0x8A, 0x80 | carga.len().min(125) as u8];
                        let m = aleatorio::<4>();
                        q.extend_from_slice(&m);
                        q.extend(
                            carga
                                .iter()
                                .take(125)
                                .enumerate()
                                .map(|(i, b)| b ^ m[i % 4]),
                        );
                        self.fluxo.write_all(&q)?;
                    }
                    0xA => {}
                    _ => {
                        parcial.extend_from_slice(&carga);
                        if parcial.len() > MAXIMO_DA_MENSAGEM {
                            return Err(erro("mensagem grande demais do Chrome"));
                        }
                        if fim {
                            return String::from_utf8(parcial)
                                .map(Some)
                                .map_err(|_| erro("mensagem do Chrome não é UTF-8"));
                        }
                    }
                }
            }
            let agora = Instant::now();
            if agora >= limite {
                // devolve o que já veio de uma mensagem em partes ao buffer? Não: raro; descarta.
                return Ok(None);
            }
            self.fluxo
                .set_read_timeout(Some((limite - agora).max(Duration::from_millis(1))))?;
            let mut buf = [0_u8; 65536];
            match self.fluxo.read(&mut buf) {
                Ok(0) => return Err(erro("o Chrome encerrou a conexão")),
                Ok(n) => self.entrada.extend_from_slice(&buf[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// Uma sessão CDP numa conexão (uma página, ou o navegador).
#[derive(Debug)]
pub struct Cdp {
    ws: Ws,
    proximo: u64,
    /// Eventos recebidos enquanto esperava uma resposta.
    pub eventos: VecDeque<Value>,
    /// O nome do mundo isolado em que o SBF lê a página (RO-01/RO-02): a página não alcança
    /// nada que o SBF define lá, nem troca os protótipos que ele usa.
    mundo: Option<String>,
    /// O quadro principal da aba (de `Page.getFrameTree`).
    quadro: Option<String>,
    /// O contexto de execução do mundo isolado no quadro principal, quando existe.
    contexto: Option<i64>,
    /// A política de rede do `Fetch` (desligada até [`Cdp::ativar_rede`]).
    rede: Option<PoliticaDeRede>,
    /// O que a política de rede barrou desde a última vez que o dono esvaziou a lista.
    pub barrados_na_rede: Vec<(String, String)>,
    /// Só na conexão do **navegador**: o alvo da aba vigiada. Qualquer outra página que
    /// aparecer (janela nova aberta por script) é fechada antes de carregar.
    alvo_proprio: Option<String>,
}

/// O que se barra na rede, decidido na hora, dentro do navegador (`Fetch.requestPaused`), antes
/// de a requisição sair.
#[derive(Debug, Clone, Default)]
pub struct PoliticaDeRede {
    /// O site em que o usuário aprovou usar a senha enquanto o passo de login roda: nesse
    /// intervalo, requisição que não seja `GET`/`HEAD` para outro host é barrada.
    pub host_de_login: Option<String>,
}

/// A decisão da política para uma requisição: `Some((campo, termo))` se barra.
///
/// - qualquer URL que case com a lista de bloqueio (RO-02), seja qual for o método e o tipo;
/// - documento para um esquema que não é de rede (`file:`, `chrome:`, `ftp:`);
/// - durante o login, método que muda estado para um host que não é o do login.
#[must_use]
pub fn decidir_requisicao(
    url: &str,
    metodo: &str,
    tipo: &str,
    host_de_login: Option<&str>,
) -> Option<(String, String)> {
    let esquema = url
        .split_once(':')
        .map_or("", |(e, _)| e)
        .to_ascii_lowercase();
    let de_rede = matches!(esquema.as_str(), "http" | "https" | "ws" | "wss");
    if !de_rede {
        if tipo == "Document" && !matches!(esquema.as_str(), "about" | "data" | "blob") {
            return Some(("rede".to_owned(), format!("esquema {esquema}")));
        }
        return None;
    }
    if let Some(b) = crate::bloqueio::avaliar_url(url) {
        return Some(("rede".to_owned(), b.termo));
    }
    if let Some(login) = host_de_login {
        let mudanca = !matches!(
            metodo.to_ascii_uppercase().as_str(),
            "GET" | "HEAD" | "OPTIONS"
        );
        let host = crate::passo::host_de(url);
        if mudanca && host.as_deref() != Some(login) {
            return Some((
                "rede".to_owned(),
                "envio para outro site durante o login".to_owned(),
            ));
        }
    }
    None
}

/// Os tipos de requisição que passam pelo `Fetch`: navegação, XHR, fetch e beacons. Imagens,
/// scripts e estilos seguem sem pausa (cada pausa custa uma ida e volta).
const TIPOS_INTERCEPTADOS: &[&str] = &["Document", "XHR", "Fetch", "Ping"];

impl Cdp {
    /// Conecta a um alvo: `ws_url` é o `webSocketDebuggerUrl` (`ws://127.0.0.1:porta/devtools/...`).
    ///
    /// # Errors
    /// URL fora de `127.0.0.1` ou falha de rede.
    pub fn conectar(ws_url: &str) -> Resultado<Self> {
        let resto = ws_url
            .strip_prefix("ws://127.0.0.1:")
            .or_else(|| ws_url.strip_prefix("ws://localhost:"))
            .ok_or_else(|| erro("o alvo do CDP não é local"))?;
        let (porta, caminho) = resto
            .split_once('/')
            .ok_or_else(|| erro("URL do CDP inválida"))?;
        let porta: u16 = porta.parse().map_err(|_| erro("porta do CDP inválida"))?;
        Ok(Self {
            ws: Ws::conectar(porta, &format!("/{caminho}"))?,
            proximo: 1,
            eventos: VecDeque::new(),
            mundo: None,
            quadro: None,
            contexto: None,
            rede: None,
            barrados_na_rede: Vec::new(),
            alvo_proprio: None,
        })
    }

    /// Chama um método e espera a resposta (até 30 s). Eventos que chegam no meio ficam em
    /// [`Self::eventos`].
    ///
    /// # Errors
    /// Erro do Chrome (só o `message`), prazo ou falha de rede.
    pub fn chamar(&mut self, metodo: &str, params: &Value) -> Resultado<Value> {
        let id = self.enviar_chamada(metodo, params)?;
        self.esperar_resposta(id, metodo)
    }

    /// Manda um método e devolve o `id`, sem esperar a resposta (veja [`Self::ler_ate`]).
    ///
    /// # Errors
    /// Falha de rede.
    pub fn enviar_chamada(&mut self, metodo: &str, params: &Value) -> Resultado<u64> {
        let id = self.proximo;
        self.proximo += 1;
        let msg = json!({"id": id, "method": metodo, "params": params}).to_string();
        self.ws.enviar(msg.as_bytes())?;
        Ok(id)
    }

    fn esperar_resposta(&mut self, id: u64, metodo: &str) -> Resultado<Value> {
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(r) = self.ler_ate(id, metodo, Duration::from_secs(1))? {
                return Ok(r);
            }
            if Instant::now() >= limite {
                return Err(erro(format!("o Chrome não respondeu a {metodo}")));
            }
        }
    }

    /// Lê por até `prazo` e devolve a resposta do `id`, se ela chegou. Os eventos que passam
    /// são tratados e guardados.
    ///
    /// # Errors
    /// Erro do Chrome (só o `message`) ou falha de rede.
    pub fn ler_ate(&mut self, id: u64, metodo: &str, prazo: Duration) -> Resultado<Option<Value>> {
        let limite = Instant::now() + prazo;
        loop {
            let resta = limite.saturating_duration_since(Instant::now());
            if resta.is_zero() {
                return Ok(None);
            }
            let Some(texto) = self.ws.receber(resta)? else {
                return Ok(None);
            };
            let v: Value =
                serde_json::from_str(&texto).map_err(|_| erro("mensagem do CDP ilegível"))?;
            if v.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(e) = v.get("error") {
                    let m = e
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("erro do Chrome");
                    return Err(erro(format!("{metodo}: {m}")));
                }
                return Ok(Some(v.get("result").cloned().unwrap_or(Value::Null)));
            }
            if v.get("method").is_some() {
                self.registrar(v);
            }
        }
    }

    /// Lê eventos por `prazo` inteiro e os põe em [`Self::eventos`].
    ///
    /// # Errors
    /// Falha de rede.
    pub fn bombear(&mut self, prazo: Duration) -> Resultado<()> {
        let limite = Instant::now() + prazo;
        loop {
            let resta = limite.saturating_duration_since(Instant::now());
            if resta.is_zero() {
                return Ok(());
            }
            if let Some(texto) = self.ws.receber(resta)?
                && let Ok(v) = serde_json::from_str::<Value>(&texto)
                && v.get("method").is_some()
            {
                self.registrar(v);
            }
        }
    }

    /// Trata um evento recebido: acompanha o mundo isolado, responde ao `Fetch` na hora e
    /// guarda o resto em [`Self::eventos`].
    fn registrar(&mut self, v: Value) {
        let metodo = v["method"].as_str().unwrap_or("").to_owned();
        match metodo.as_str() {
            "Runtime.executionContextCreated" => {
                let c = &v["params"]["context"];
                let do_mundo = self.mundo.as_deref().is_some_and(|m| c["name"] == m);
                let isolado = c["auxData"]["isDefault"] == false;
                let do_quadro = match (&self.quadro, c["auxData"]["frameId"].as_str()) {
                    (Some(q), Some(f)) => q == f,
                    _ => true,
                };
                if do_mundo && isolado && do_quadro {
                    self.contexto = c["id"].as_i64();
                }
            }
            "Runtime.executionContextDestroyed" => {
                if v["params"]["executionContextId"].as_i64() == self.contexto {
                    self.contexto = None;
                }
            }
            "Runtime.executionContextsCleared" => self.contexto = None,
            "Fetch.requestPaused" => {
                self.tratar_pedido(&v["params"], v["sessionId"].as_str());
                return;
            }
            "Target.attachedToTarget" => self.anexar_filho(&v["params"]),
            _ => {}
        }
        self.eventos.push_back(v);
    }

    /// Manda um comando sem esperar a resposta (a do `Fetch` tem de sair na hora, dentro da
    /// leitura de eventos). A resposta chega depois e é descartada por não ter dono.
    fn disparar(&mut self, sessao: Option<&str>, metodo: &str, params: &Value) {
        let id = self.proximo;
        self.proximo += 1;
        let mut m = json!({"id": id, "method": metodo, "params": params});
        if let Some(s) = sessao {
            m["sessionId"] = json!(s);
        }
        let _ = self.ws.enviar(m.to_string().as_bytes());
    }

    fn tratar_pedido(&mut self, p: &Value, sessao: Option<&str>) {
        let id = p["requestId"].as_str().unwrap_or("").to_owned();
        let url = p["request"]["url"].as_str().unwrap_or("");
        let metodo = p["request"]["method"].as_str().unwrap_or("GET");
        let tipo = p["resourceType"].as_str().unwrap_or("");
        let login = self.rede.as_ref().and_then(|r| r.host_de_login.clone());
        match decidir_requisicao(url, metodo, tipo, login.as_deref()) {
            Some(b) => {
                self.barrados_na_rede.push(b);
                self.disparar(
                    sessao,
                    "Fetch.failRequest",
                    &json!({"requestId": id, "errorReason": "BlockedByClient"}),
                );
            }
            None => self.disparar(sessao, "Fetch.continueRequest", &json!({"requestId": id})),
        }
    }

    /// Um alvo novo entrou. Na conexão do navegador, página que não é a nossa (janela aberta
    /// por script) é fechada antes de carregar; na da aba, um alvo filho (um iframe de outro
    /// site, que o Chrome roda em outro processo) recebe a mesma política de rede, e só segue
    /// depois de ligada.
    fn anexar_filho(&mut self, p: &Value) {
        let Some(sessao) = p["sessionId"].as_str().map(str::to_owned) else {
            return;
        };
        if let Some(nosso) = self.alvo_proprio.clone() {
            let info = &p["targetInfo"];
            if info["type"] == "page"
                && let Some(id) = info["targetId"].as_str()
                && id != nosso
            {
                self.barrados_na_rede
                    .push(("rede".to_owned(), "janela nova".to_owned()));
                self.disparar(None, "Target.closeTarget", &json!({"targetId": id}));
            }
            self.disparar(Some(&sessao), "Runtime.runIfWaitingForDebugger", &json!({}));
            return;
        }
        if self.rede.is_some() {
            self.disparar(
                Some(&sessao),
                "Fetch.enable",
                &json!({"patterns": padroes_do_fetch()}),
            );
            self.disparar(
                Some(&sessao),
                "Target.setAutoAttach",
                &json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
            );
        }
        self.disparar(Some(&sessao), "Runtime.runIfWaitingForDebugger", &json!({}));
    }

    /// Liga a política de rede (`Fetch` e anexação automática de iframes de outro site).
    ///
    /// # Errors
    /// Erro do Chrome.
    pub fn ativar_rede(&mut self) -> Resultado<()> {
        self.rede = Some(PoliticaDeRede::default());
        self.chamar("Fetch.enable", &json!({"patterns": padroes_do_fetch()}))?;
        self.chamar(
            "Target.setAutoAttach",
            &json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
        )?;
        Ok(())
    }

    /// Na conexão do **navegador**: fecha, antes de carregar, qualquer página que não seja
    /// `alvo` (a aba vigiada). A janela nova que um script abre não passa pela política de
    /// rede da aba.
    ///
    /// # Errors
    /// Erro do Chrome.
    pub fn vigiar_janelas_novas(&mut self, alvo: &str) -> Resultado<()> {
        self.alvo_proprio = Some(alvo.to_owned());
        self.chamar(
            "Target.setAutoAttach",
            &json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
        )?;
        Ok(())
    }

    /// O host do login aprovado enquanto o passo de login roda (`None` fora dele).
    pub fn definir_host_de_login(&mut self, host: Option<String>) {
        if let Some(r) = &mut self.rede {
            r.host_de_login = host;
        }
    }

    /// Escolhe o mundo isolado em que o SBF lê a página, no quadro principal `quadro`.
    pub fn definir_mundo(&mut self, nome: &str, quadro: &str) {
        self.mundo = Some(nome.to_owned());
        self.quadro = Some(quadro.to_owned());
        self.contexto = None;
    }

    /// O quadro principal da aba e a URL **que o navegador carregou** nele (do próprio
    /// Chrome, nunca de JavaScript da página).
    ///
    /// # Errors
    /// Erro do Chrome.
    pub fn quadro_principal(&mut self) -> Resultado<(String, String)> {
        let r = self.chamar("Page.getFrameTree", &json!({}))?;
        let f = &r["frameTree"]["frame"];
        let id = f["id"]
            .as_str()
            .ok_or_else(|| erro("o Chrome não informou o quadro principal"))?
            .to_owned();
        let url = f["url"].as_str().unwrap_or("").to_owned();
        let fragmento = f["urlFragment"].as_str().unwrap_or("");
        Ok((id, format!("{url}{fragmento}")))
    }

    fn contexto_isolado(&mut self) -> Resultado<i64> {
        if self.contexto.is_none() {
            // eventos que já chegaram e ainda não foram lidos
            self.bombear(Duration::from_millis(1))?;
        }
        if let Some(c) = self.contexto {
            return Ok(c);
        }
        let (Some(quadro), Some(nome)) = (self.quadro.clone(), self.mundo.clone()) else {
            return Err(erro("o mundo isolado não foi preparado"));
        };
        let r = self.chamar(
            "Page.createIsolatedWorld",
            &json!({"frameId": quadro, "worldName": nome, "grantUniveralAccess": false}),
        )?;
        let id = r["executionContextId"]
            .as_i64()
            .ok_or_else(|| erro("o Chrome não criou o mundo isolado"))?;
        self.contexto = Some(id);
        Ok(id)
    }

    /// `Runtime.evaluate` **no mundo isolado** do SBF: a página não vê o que roda lá e não
    /// troca os protótipos que o script usa. É daqui que sai tudo o que decide algo (o
    /// elemento, o campo, o que a IA lê).
    ///
    /// # Errors
    /// Erro do Chrome ou exceção no script.
    pub fn avaliar_isolado(&mut self, expressao: &str) -> Resultado<Value> {
        let mut ultima = erro("o mundo isolado não respondeu");
        for _ in 0..3 {
            let contexto = match self.contexto_isolado() {
                Ok(c) => c,
                Err(e) => {
                    ultima = e;
                    continue;
                }
            };
            match self.chamar(
                "Runtime.evaluate",
                &json!({"expression": expressao, "returnByValue": true,
                        "awaitPromise": false, "contextId": contexto}),
            ) {
                Ok(r) => {
                    if r.get("exceptionDetails").is_some() {
                        return Err(erro("o script do SBF lançou uma exceção"));
                    }
                    return Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null));
                }
                Err(e) => {
                    // o contexto morreu numa navegação: recria e tenta de novo
                    self.contexto = None;
                    ultima = e;
                }
            }
        }
        Err(ultima)
    }

    /// `Runtime.evaluate` de uma expressão que devolve JSON (`returnByValue`).
    ///
    /// # Errors
    /// Erro do Chrome ou exceção na página.
    pub fn avaliar(&mut self, expressao: &str) -> Resultado<Value> {
        let r = self.chamar(
            "Runtime.evaluate",
            &json!({"expression": expressao, "returnByValue": true, "awaitPromise": false}),
        )?;
        if r.get("exceptionDetails").is_some() {
            return Err(erro("a página lançou uma exceção"));
        }
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    /// Digita `segredo` no elemento já focado, por `Input.insertText`.
    ///
    /// A mensagem é montada num `Zeroizing<String>` (o texto escapado direto no buffer) e
    /// enviada sem ser guardada; nada daqui vai a log nem a erro.
    ///
    /// # Errors
    /// Erro do Chrome (sem o texto) ou de rede.
    pub fn inserir_texto_secreto(&mut self, segredo: &str) -> Resultado<()> {
        let id = self.proximo;
        self.proximo += 1;
        let mut msg: Zeroizing<String> =
            Zeroizing::new(String::with_capacity(segredo.len() * 6 + 96));
        msg.push_str("{\"id\":");
        msg.push_str(&id.to_string());
        msg.push_str(",\"method\":\"Input.insertText\",\"params\":{\"text\":\"");
        escapar_json(segredo, &mut msg);
        msg.push_str("\"}}");
        let r = self.ws.enviar(msg.as_bytes());
        drop(msg);
        r?;
        self.esperar_resposta(id, "Input.insertText").map(|_| ())
    }
}

fn padroes_do_fetch() -> Value {
    Value::Array(
        TIPOS_INTERCEPTADOS
            .iter()
            .map(|t| json!({"urlPattern": "*", "resourceType": t, "requestStage": "Request"}))
            .collect(),
    )
}

/// Escapa `texto` como conteúdo de string JSON, acrescentando a `saida`.
fn escapar_json(texto: &str, saida: &mut String) {
    for c in texto.chars() {
        match c {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            '\n' => saida.push_str("\\n"),
            '\r' => saida.push_str("\\r"),
            '\t' => saida.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(saida, "\\u{:04x}", c as u32);
            }
            c => saida.push(c),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_politica_de_rede_barra_a_lista_o_envio_de_login_e_esquemas_de_arquivo() {
        // a lista de bloqueio vale para qualquer método e tipo
        assert!(decidir_requisicao("https://b.example/api/pagar", "POST", "Fetch", None).is_some());
        assert!(
            decidir_requisicao("https://b.example/transferir", "GET", "Document", None).is_some()
        );
        assert!(decidir_requisicao("https://b.example/extrato", "GET", "Document", None).is_none());
        // no login, envio que muda estado só para o host aprovado
        let login = Some("b.example");
        assert!(decidir_requisicao("https://b.example/login", "POST", "Document", login).is_none());
        assert!(decidir_requisicao("https://evil.example/c", "POST", "Fetch", login).is_some());
        assert!(decidir_requisicao("https://evil.example/c", "GET", "Document", login).is_none());
        // fora do login, POST comum passa
        assert!(decidir_requisicao("https://evil.example/c", "POST", "Fetch", None).is_none());
        // esquemas que não são de rede
        assert!(decidir_requisicao("file:///etc/passwd", "GET", "Document", None).is_some());
        assert!(decidir_requisicao("data:text/plain,oi", "GET", "Document", None).is_none());
    }

    #[test]
    fn escapar_json_confere_com_serde() {
        for t in [
            "simples",
            "aspas\"e\\barra",
            "linha\nnova\ttab",
            "acentuação ç ñ 日本",
            "\u{1}ctl",
        ] {
            let mut s = String::new();
            escapar_json(t, &mut s);
            let via_serde = serde_json::to_string(t).unwrap();
            assert_eq!(format!("\"{s}\""), via_serde, "{t}");
        }
    }
}
