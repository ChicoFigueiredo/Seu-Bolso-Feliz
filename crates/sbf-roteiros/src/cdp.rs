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
}

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
        })
    }

    /// Chama um método e espera a resposta (até 30 s). Eventos que chegam no meio ficam em
    /// [`Self::eventos`].
    ///
    /// # Errors
    /// Erro do Chrome (só o `message`), prazo ou falha de rede.
    pub fn chamar(&mut self, metodo: &str, params: &Value) -> Resultado<Value> {
        let id = self.proximo;
        self.proximo += 1;
        let msg = json!({"id": id, "method": metodo, "params": params}).to_string();
        self.ws.enviar(msg.as_bytes())?;
        self.esperar_resposta(id, metodo)
    }

    fn esperar_resposta(&mut self, id: u64, metodo: &str) -> Resultado<Value> {
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            let resta = limite.saturating_duration_since(Instant::now());
            if resta.is_zero() {
                return Err(erro(format!("o Chrome não respondeu a {metodo}")));
            }
            let Some(texto) = self.ws.receber(resta.min(Duration::from_secs(1)))? else {
                continue;
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
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            if v.get("method").is_some() {
                self.eventos.push_back(v);
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
                self.eventos.push_back(v);
            }
        }
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
