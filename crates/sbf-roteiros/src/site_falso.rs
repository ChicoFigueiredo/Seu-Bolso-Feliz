//! O site de mentira: um servidor HTTP minúsculo em `127.0.0.1`, numa thread, para os testes
//! e a demonstração dos roteiros. **Nada de site real.** Páginas: login (CPF e senha), MFA
//! simulado, painel (com o botão Pagar, um link disfarçado e um formulário de transferência),
//! extratos (tabela) e o download de um PDF fictício.
//!
//! O servidor conta quantas vezes cada rota perigosa foi alcançada: um teste de bloqueio
//! afirma que o contador é zero.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// O CPF que o site aceita (só dígitos; o formulário tira a pontuação).
pub const CPF_DO_SITE: &str = "12345678909";
/// A senha fictícia que o site aceita.
pub const SENHA_DO_SITE: &str = "SenhaFicticia#4821";
/// O código de MFA do site.
pub const CODIGO_DE_MFA: &str = "123456";
/// O conteúdo do PDF fictício (outubro).
pub const PDF_FICTICIO: &[u8] =
    b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n% extrato ficticio de outubro\n%%EOF\n";

/// Como o site se comporta.
#[derive(Debug, Clone, Copy, Default)]
pub struct Configuracao {
    /// Pede o código depois do login.
    pub com_mfa: bool,
    /// Mostra um CAPTCHA antes do formulário de login.
    pub com_captcha: bool,
}

/// O que o site viu.
#[derive(Debug, Default)]
pub struct Contadores {
    /// Rotas de pagamento, transferência etc. alcançadas. Tem de ficar em zero.
    pub perigosas: AtomicUsize,
    /// Downloads servidos.
    pub downloads: AtomicUsize,
    /// Logins aceitos.
    pub logins: AtomicUsize,
    /// Pedidos de login recusados.
    pub logins_recusados: AtomicUsize,
}

#[derive(Debug)]
struct Estado {
    /// A porta do servidor (as páginas hostis a citam).
    porta: u16,
    configuracao: Configuracao,
    contadores: Contadores,
    /// O layout mudou: o botão de baixar ganha outro id (o "site alterado").
    layout_novo: AtomicBool,
    /// O CAPTCHA foi resolvido (o usuário "marcou a caixa").
    captcha_ok: AtomicBool,
    parar: AtomicBool,
}

/// O servidor.
#[derive(Debug)]
pub struct SiteFalso {
    porta: u16,
    estado: Arc<Estado>,
    fio: Option<JoinHandle<()>>,
}

impl SiteFalso {
    /// Sobe o servidor numa porta livre de `127.0.0.1`.
    ///
    /// # Errors
    /// Se não conseguir abrir a porta.
    pub fn iniciar(configuracao: Configuracao) -> std::io::Result<Self> {
        let escuta = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let porta = escuta.local_addr()?.port();
        let estado = Arc::new(Estado {
            porta,
            configuracao,
            contadores: Contadores::default(),
            layout_novo: AtomicBool::new(false),
            captcha_ok: AtomicBool::new(false),
            parar: AtomicBool::new(false),
        });
        let e = Arc::clone(&estado);
        let fio = std::thread::spawn(move || {
            for conexao in escuta.incoming() {
                if e.parar.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(c) = conexao else { continue };
                let e2 = Arc::clone(&e);
                std::thread::spawn(move || {
                    let _ = atender(c, &e2);
                });
            }
        });
        Ok(Self {
            porta,
            estado,
            fio: Some(fio),
        })
    }

    /// A URL base (`http://127.0.0.1:porta`).
    #[must_use]
    pub fn url(&self, caminho: &str) -> String {
        format!("http://127.0.0.1:{}{caminho}", self.porta)
    }

    /// A porta.
    #[must_use]
    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// Quantas rotas perigosas foram alcançadas.
    #[must_use]
    pub fn perigosas(&self) -> usize {
        self.estado.contadores.perigosas.load(Ordering::SeqCst)
    }

    /// Quantos downloads foram servidos.
    #[must_use]
    pub fn downloads(&self) -> usize {
        self.estado.contadores.downloads.load(Ordering::SeqCst)
    }

    /// Quantos logins foram aceitos.
    #[must_use]
    pub fn logins(&self) -> usize {
        self.estado.contadores.logins.load(Ordering::SeqCst)
    }

    /// Quantos logins foram recusados.
    #[must_use]
    pub fn logins_recusados(&self) -> usize {
        self.estado
            .contadores
            .logins_recusados
            .load(Ordering::SeqCst)
    }

    /// Muda o layout: o botão de baixar passa a ter outro id (o site foi alterado).
    pub fn mudar_layout(&self) {
        self.estado.layout_novo.store(true, Ordering::SeqCst);
    }
}

impl Drop for SiteFalso {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        // acorda o accept
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.porta)));
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

struct Pedido {
    metodo: String,
    caminho: String,
    cookies: String,
    corpo: String,
}

fn ler_pedido(c: &mut TcpStream) -> std::io::Result<Option<Pedido>> {
    c.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = Vec::new();
    let mut parte = [0_u8; 4096];
    let fim_do_cabecalho = loop {
        let n = c.read(&mut parte)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&parte[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
    };
    let cabecalho = String::from_utf8_lossy(&buf[..fim_do_cabecalho]).into_owned();
    let mut linhas = cabecalho.lines();
    let primeira = linhas.next().unwrap_or("");
    let mut p = primeira.split(' ');
    let metodo = p.next().unwrap_or("").to_owned();
    let caminho = p.next().unwrap_or("/").to_owned();
    let mut tamanho = 0_usize;
    let mut cookies = String::new();
    for l in linhas {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => tamanho = v.trim().parse().unwrap_or(0),
                "cookie" => cookies.push_str(v.trim()),
                _ => {}
            }
        }
    }
    while buf.len() < fim_do_cabecalho + tamanho {
        let n = c.read(&mut parte)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&parte[..n]);
    }
    let corpo = String::from_utf8_lossy(&buf[fim_do_cabecalho..]).into_owned();
    Ok(Some(Pedido {
        metodo,
        caminho,
        cookies,
        corpo,
    }))
}

fn campo_do_formulario(corpo: &str, nome: &str) -> String {
    corpo
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == nome)
        .map(|(_, v)| decodificar(v))
        .unwrap_or_default()
}

fn decodificar(v: &str) -> String {
    let b = v.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(x) = h {
                    saida.push(x);
                    i += 2;
                } else {
                    saida.push(b'%');
                }
            }
            x => saida.push(x),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

fn tem(cookies: &str, nome: &str) -> bool {
    cookies.split(';').any(|c| c.trim() == format!("{nome}=ok"))
}

fn html(titulo: &str, corpo: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{titulo}</title>\
         <style>body{{font-family:sans-serif;max-width:640px;margin:2em auto}}\
         input,button,a.botao{{font-size:16px;padding:6px 10px;margin:4px 0}}table{{border-collapse:collapse}}\
         td,th{{border:1px solid #999;padding:4px 10px}}</style></head><body>\
         <h1>Banco Ficticio</h1>{corpo}</body></html>"
    )
    .into_bytes()
}

fn responder(
    c: &mut TcpStream,
    status: &str,
    extra: &str,
    tipo: &str,
    corpo: &[u8],
) -> std::io::Result<()> {
    write!(
        c,
        "HTTP/1.1 {status}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        corpo.len()
    )?;
    c.write_all(corpo)?;
    c.flush()?;
    let _ = c.shutdown(Shutdown::Write);
    Ok(())
}

fn redirecionar(c: &mut TcpStream, para: &str, cookie: Option<&str>) -> std::io::Result<()> {
    let extra = format!(
        "Location: {para}\r\n{}",
        cookie
            .map(|k| format!("Set-Cookie: {k}; Path=/\r\n"))
            .unwrap_or_default()
    );
    responder(c, "303 See Other", &extra, "text/plain", b"")
}

fn atender(mut c: TcpStream, e: &Estado) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut c)? else {
        return Ok(());
    };
    let rota = p.caminho.split('?').next().unwrap_or("/").to_owned();
    let html_ok = |c: &mut TcpStream, t: &str, corpo: &str| {
        responder(c, "200 OK", "", "text/html; charset=utf-8", &html(t, corpo))
    };
    let logado = tem(&p.cookies, "sessao") && (!e.configuracao.com_mfa || tem(&p.cookies, "mfa"));
    match (p.metodo.as_str(), rota.as_str()) {
        ("GET", "/") => redirecionar(&mut c, "/login", None),
        ("GET", "/login") => {
            let erro = if p.caminho.contains("erro=1") {
                "<p id=\"erro\">Dados inválidos.</p>"
            } else {
                ""
            };
            let captcha = if e.configuracao.com_captcha && !e.captcha_ok.load(Ordering::SeqCst) {
                "<div class=\"g-recaptcha\" data-sitekey=\"falsa\"><label><input type=\"checkbox\" id=\"humano\" \
                 onclick=\"fetch('/captcha-ok',{method:'POST'})\"> Não sou um robô</label></div>"
            } else {
                ""
            };
            html_ok(
                &mut c,
                "Entrar",
                &format!(
                    "{erro}<p>Acesse sua conta</p>{captcha}<form id=\"f\" method=\"post\" action=\"/login\">\
                 <label>CPF <input id=\"cpf\" name=\"cpf\" type=\"text\" autocomplete=\"username\"></label><br>\
                 <label>Senha <input id=\"senha\" name=\"senha\" type=\"password\" autocomplete=\"current-password\"></label><br>\
                 <button id=\"entrar\" type=\"submit\">Entrar</button></form>"
                ),
            )
        }
        ("POST", "/captcha-ok") => {
            e.captcha_ok.store(true, Ordering::SeqCst);
            responder(&mut c, "204 No Content", "", "text/plain", b"")
        }
        ("POST", "/login") => {
            let cpf: String = campo_do_formulario(&p.corpo, "cpf")
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            let senha = campo_do_formulario(&p.corpo, "senha");
            if cpf == CPF_DO_SITE && senha == SENHA_DO_SITE {
                e.contadores.logins.fetch_add(1, Ordering::SeqCst);
                if e.configuracao.com_mfa {
                    redirecionar(&mut c, "/mfa", Some("sessao=ok"))
                } else {
                    redirecionar(&mut c, "/painel", Some("sessao=ok"))
                }
            } else {
                e.contadores.logins_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, "/login?erro=1", None)
            }
        }
        ("GET", "/mfa") => html_ok(
            &mut c,
            "Verificação",
            "<p>Digite o código de verificação enviado por SMS.</p>\
             <form id=\"fm\" method=\"post\" action=\"/mfa\"><input id=\"codigo\" name=\"codigo\" \
             autocomplete=\"one-time-code\"><button id=\"validar\" type=\"submit\">Validar</button></form>",
        ),
        ("POST", "/mfa") => {
            if campo_do_formulario(&p.corpo, "codigo") == CODIGO_DE_MFA {
                redirecionar(&mut c, "/painel", Some("mfa=ok"))
            } else {
                redirecionar(&mut c, "/mfa", None)
            }
        }
        ("GET", "/painel") if logado => html_ok(
            &mut c,
            "Painel",
            "<p>Bem-vindo ao painel.</p><nav><a id=\"extratos-link\" href=\"/extratos\">Extratos</a></nav>\
             <form method=\"post\" action=\"/pagar\"><button id=\"pagar\" type=\"submit\">Pagar</button></form>\
             <a id=\"seguir\" class=\"botao\" href=\"/pagamento/confirmar\">Continuar</a>\
             <form method=\"post\" action=\"/transferir\"><button id=\"enviar\" type=\"submit\">Enviar</button></form>\
             <button id=\"pix\" onclick=\"location.href='/pix'\">PIX</button>",
        ),
        ("GET", "/extratos") if logado => {
            let id = if e.layout_novo.load(Ordering::SeqCst) {
                "baixar-extrato"
            } else {
                "baixar"
            };
            html_ok(
                &mut c,
                "Extratos",
                &format!(
                    "<table id=\"extratos\"><tr><th>Mês</th><th>Valor</th></tr>\
                 <tr><td>Outubro</td><td>R$ 1.234,56</td></tr><tr><td>Setembro</td><td>R$ 987,65</td></tr></table>\
                 <p><a id=\"{id}\" href=\"/extrato-2026-10.pdf\" download>Baixar extrato de outubro</a></p>"
                ),
            )
        }
        ("GET", "/extrato-2026-10.pdf") if logado => {
            e.contadores.downloads.fetch_add(1, Ordering::SeqCst);
            responder(
                &mut c,
                "200 OK",
                "Content-Disposition: attachment; filename=\"extrato-2026-10.pdf\"\r\n",
                "application/pdf",
                PDF_FICTICIO,
            )
        }
        // Páginas hostis dos testes de segurança (RO-01, RO-02): nada aqui é um site real.
        ("GET", "/hostil") => html_ok(
            &mut c,
            "Entrar",
            // Mente para o SBF: troca a ponte da página por uma que jura estar no site do login.
            &format!(
                "<p>Acesse sua conta</p><form method=\"post\" action=\"/login\">\
                 <input id=\"senha\" name=\"senha\" type=\"password\"></form>\
                 <script>window.__sbfPonte={{pagina:()=>({{url:'http://127.0.0.1:{}/login',titulo:'Entrar',\
                 texto:'',elementos:[],formularios:[],tabelas:[],quadros:[]}}),\
                 descrever:()=>({{campos:[],eh_senha:true,eh_campo:true,tipo:'password'}}),\
                 visivel:()=>true,bloqueios:()=>[]}};</script>",
                e.porta
            ),
        ),
        ("GET", "/continuar") => html_ok(
            &mut c,
            "Continuar",
            // Um botão de texto inocente que, por script, paga e transfere.
            "<button id=\"continuar\" onclick=\"fetch('/pagar',{method:'POST'});location='/transferir'\">\
             Continuar</button><button id=\"baixar\" onclick=\"location='/extrato-2026-10.pdf'\">Baixar</button>",
        ),
        ("GET", "/coberto") => html_ok(
            &mut c,
            "Coberto",
            // O botão de verdade fica sob uma capa transparente que leva a outro lugar.
            "<button id=\"alvo\" style=\"position:absolute;left:40px;top:40px;width:200px;height:60px\" \
             onclick=\"document.title='alvo'\">Baixar extrato</button>\
             <div id=\"capa\" style=\"position:fixed;left:0;top:0;width:100%;height:100%\" \
             onclick=\"location='/navegou'\"></div>",
        ),
        ("GET", "/coberto-por-quadro") => html_ok(
            &mut c,
            "Coberto",
            "<button id=\"alvo\" style=\"position:absolute;left:40px;top:40px;width:200px;height:60px\" \
             onclick=\"document.title='alvo'\">Baixar extrato</button>\
             <iframe id=\"quadro\" srcdoc=\"<body onclick=&quot;top.location='/navegou'&quot; \
             style='margin:0;height:100vh'></body>\" style=\"position:fixed;left:0;top:0;width:100%;\
             height:100%;border:0\"></iframe>",
        ),
        ("GET", "/abre-janela") => html_ok(
            &mut c,
            "Janela",
            // Abre uma janela nova (que o Chrome trata como outro alvo) para o que o clique
            // inocente não pode fazer na aba vigiada.
            "<button id=\"abre\" onclick=\"window.open('/transferir')\">Continuar</button>",
        ),
        ("GET", "/comprovante") => html_ok(
            &mut c,
            "Comprovante",
            "<button id=\"ok\" onclick=\"document.title='clicou'\">Comprovante de pagamento</button>\
             <button id=\"mau\" onclick=\"document.title='mau'\">Comprovante de pagamento: pagar agora</button>",
        ),
        ("GET", "/navegou") => html_ok(&mut c, "Navegou", "<p>Fui levado para outro lugar.</p>"),
        ("GET", "/sombra") => html_ok(
            &mut c,
            "Sombra",
            // O botão "Pagar" mora num shadow DOM: o evento chega à janela com o alvo trocado
            // pelo hospedeiro, que não tem texto nenhum.
            "<div id=\"hospedeiro\" style=\"display:block;width:200px;height:60px\"></div>\
             <script>const r=document.getElementById('hospedeiro').attachShadow({mode:'open'});\
             r.innerHTML=\"<button style='width:200px;height:60px'>Pagar</button>\";\
             r.firstChild.onclick=()=>{location='/navegou'};</script>",
        ),
        ("GET", "/quadro-de-fora") => html_ok(
            &mut c,
            "Quadro",
            // Um iframe de OUTRO site (localhost x 127.0.0.1): o Chrome o roda em outro processo.
            &format!(
                "<p>Painel</p><iframe id=\"q\" src=\"http://localhost:{}/envia\"></iframe>",
                e.porta
            ),
        ),
        ("GET", "/envia") => html_ok(
            &mut c,
            "Envia",
            "<script>fetch('/pagar',{method:'POST'});</script>",
        ),
        (_, "/pagar" | "/pagamento/confirmar" | "/transferir" | "/pix") => {
            e.contadores.perigosas.fetch_add(1, Ordering::SeqCst);
            html_ok(&mut c, "Feito", "<p>Operação realizada.</p>")
        }
        (_, "/painel" | "/extratos" | "/extrato-2026-10.pdf") => {
            redirecionar(&mut c, "/login", None)
        }
        _ => responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado"),
    }
}
