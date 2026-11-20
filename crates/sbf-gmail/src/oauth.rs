//! O login do Gmail pelo navegador: OAuth 2.0 com código de autorização, PKCE (S256) e um
//! servidor local em `127.0.0.1:8976/oauth2callback` (o mesmo redirecionamento do antigo
//! `get-token.ts`, que já está cadastrado no Google Console).
//!
//! Nada de segredo vai para log, `Debug` ou mensagem de erro: o token entra no chaveiro e a
//! saída só leva o e-mail da conta. A rede e o navegador ficam atrás de traits
//! ([`Provedor`], [`Receptor`]) para os testes.

use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::erro::{Erro, Resultado};

/// A porta do retorno.
pub const PORTA: u16 = 8976;
/// O endereço de retorno (cadastrado no Google Console).
pub const REDIRECT_URI: &str = "http://127.0.0.1:8976/oauth2callback";
/// O escopo: só leitura.
pub const ESCOPO: &str = "https://www.googleapis.com/auth/gmail.readonly";
const URL_DE_AUTORIZACAO: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const URL_DO_TOKEN: &str = "https://oauth2.googleapis.com/token";
const URL_DO_PERFIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/profile";
/// Quanto esperar a pessoa autorizar no navegador.
pub const TEMPO_LIMITE: Duration = Duration::from_secs(300);

/// Os tokens da troca.
pub struct Tokens {
    /// O access token (só para pedir o perfil).
    pub acesso: Zeroizing<String>,
    /// O refresh token; falta se o Google não o devolveu (app já autorizado sem `consent`).
    pub renovacao: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens").finish_non_exhaustive()
    }
}

/// O servidor de autorização (Google, ou um dublê).
pub trait Provedor {
    /// Troca o código pelos tokens.
    ///
    /// # Errors
    /// [`Erro::Login`].
    fn trocar_codigo(
        &self,
        client_id: &str,
        client_secret: &str,
        codigo: &str,
        verificador: &str,
    ) -> Resultado<Tokens>;

    /// O e-mail da conta autorizada.
    ///
    /// # Errors
    /// [`Erro::Login`].
    fn email(&self, acesso: &str) -> Resultado<String>;
}

/// Quem espera o navegador voltar (o servidor local, ou um dublê).
pub trait Receptor {
    /// Abre a porta antes de o navegador ser aberto (para uma porta ocupada falhar logo).
    ///
    /// # Errors
    /// [`Erro::Login`].
    fn preparar(&self) -> Resultado<()> {
        Ok(())
    }

    /// Espera o retorno do navegador e devolve o código de autorização, depois de conferir o
    /// `state`.
    ///
    /// # Errors
    /// [`Erro::Login`].
    fn esperar(&self, state: &str) -> Resultado<String>;
}

/// 128 bits aleatórios em hexadecimal.
fn aleatorio() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// O desafio PKCE (S256) de um verificador.
#[must_use]
pub fn desafio(verificador: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verificador.as_bytes()))
}

/// A URL de autorização.
#[must_use]
pub fn url_de_autorizacao(client_id: &str, state: &str, verificador: &str) -> String {
    let mut u = Url::parse(URL_DE_AUTORIZACAO).expect("URL fixa");
    u.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", ESCOPO)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", state)
        .append_pair("code_challenge", &desafio(verificador))
        .append_pair("code_challenge_method", "S256");
    u.into()
}

/// Só letras, dígitos, `_`, `-` e `.` (até 40): o `error=` do retorno vem do navegador.
fn codigo_de_erro_seguro(e: &str) -> String {
    e.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(40)
        .collect()
}

/// Lê a primeira linha da requisição de retorno (`GET /oauth2callback?code=...&state=... HTTP/1.1`)
/// e devolve o código, recusando se o `state` não é o esperado.
///
/// # Errors
/// [`Erro::Login`]: o Google recusou (`error=`), o `state` não confere, o código falta ou não
/// é o caminho do retorno.
pub fn ler_retorno(linha: &str, state_esperado: &str) -> Resultado<String> {
    let mut partes = linha.split_whitespace();
    let (Some("GET"), Some(alvo)) = (partes.next(), partes.next()) else {
        return Err(Erro::Login("retorno inválido".to_owned()));
    };
    let url = Url::parse(&format!("http://127.0.0.1{alvo}"))
        .map_err(|_| Erro::Login("retorno inválido".to_owned()))?;
    if url.path() != "/oauth2callback" {
        return Err(Erro::Login("retorno inválido".to_owned()));
    }
    let mut codigo = None;
    let mut state = None;
    let mut erro = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => codigo = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => erro = Some(v.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(state_esperado) {
        return Err(Erro::Login(
            "o retorno não é deste login (state diferente); nada foi guardado".to_owned(),
        ));
    }
    if let Some(e) = erro {
        return Err(Erro::Login(format!(
            "o Google recusou a autorização ({})",
            codigo_de_erro_seguro(&e)
        )));
    }
    codigo
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Erro::Login("o retorno veio sem código".to_owned()))
}

/// O servidor local de verdade, em `127.0.0.1:8976`.
#[derive(Debug)]
pub struct ReceptorLocal {
    /// Quanto esperar.
    pub tempo_limite: Duration,
    ouvinte: std::cell::RefCell<Option<TcpListener>>,
}

impl Default for ReceptorLocal {
    fn default() -> Self {
        Self::com_tempo(TEMPO_LIMITE)
    }
}

const PAGINA_OK: &str = "<html><head><meta charset=\"utf-8\"><title>Seu Bolso Feliz</title></head><body style=\"font-family:system-ui;text-align:center;padding:40px\"><h1>Autorização concluída</h1><p>Pode fechar esta aba e voltar ao Seu Bolso Feliz.</p></body></html>";
const PAGINA_ERRO: &str = "<html><head><meta charset=\"utf-8\"><title>Seu Bolso Feliz</title></head><body style=\"font-family:system-ui;text-align:center;padding:40px\"><h1>Não deu certo</h1><p>Volte ao Seu Bolso Feliz e tente entrar de novo.</p></body></html>";

fn responder(fluxo: &mut std::net::TcpStream, status: &str, corpo: &str) {
    let _ = write!(
        fluxo,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
}

impl ReceptorLocal {
    /// Um receptor que espera `tempo_limite`.
    #[must_use]
    pub fn com_tempo(tempo_limite: Duration) -> Self {
        Self {
            tempo_limite,
            ouvinte: std::cell::RefCell::new(None),
        }
    }

    /// Abre a porta já (para a URL só ser aberta no navegador depois que ela atende).
    ///
    /// # Errors
    /// [`Erro::Login`] se a porta está ocupada.
    pub fn escutar() -> Resultado<TcpListener> {
        let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, PORTA))).map_err(
            |e| {
                Erro::Login(format!(
                    "não consegui abrir a porta {PORTA} deste computador ({}); feche outro login em andamento",
                    e.kind()
                ))
            },
        )?;
        l.set_nonblocking(true)
            .map_err(|e| Erro::Login(e.kind().to_string()))?;
        Ok(l)
    }

    /// Espera o retorno em `ouvinte`.
    ///
    /// # Errors
    /// [`Erro::Login`].
    pub fn esperar_em(&self, ouvinte: &TcpListener, state: &str) -> Resultado<String> {
        let fim = Instant::now() + self.tempo_limite;
        loop {
            match ouvinte.accept() {
                Ok((mut fluxo, _)) => {
                    let _ = fluxo.set_nonblocking(false);
                    let _ = fluxo.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut buf = [0_u8; 4096];
                    let n = fluxo.read(&mut buf).unwrap_or(0);
                    let texto = String::from_utf8_lossy(&buf[..n]);
                    let linha = texto.lines().next().unwrap_or("");
                    // `/favicon.ico` e afins: ignora e continua esperando.
                    if !linha.contains("/oauth2callback") {
                        responder(&mut fluxo, "404 Not Found", "");
                        continue;
                    }
                    let r = ler_retorno(linha, state);
                    match &r {
                        Ok(_) => responder(&mut fluxo, "200 OK", PAGINA_OK),
                        Err(_) => responder(&mut fluxo, "400 Bad Request", PAGINA_ERRO),
                    }
                    return r;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= fim {
                        return Err(Erro::Login(
                            "o tempo para autorizar no navegador acabou".to_owned(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(Erro::Login(e.kind().to_string())),
            }
        }
    }
}

impl Receptor for ReceptorLocal {
    fn preparar(&self) -> Resultado<()> {
        *self.ouvinte.borrow_mut() = Some(Self::escutar()?);
        Ok(())
    }

    fn esperar(&self, state: &str) -> Resultado<String> {
        let guardado = self.ouvinte.borrow_mut().take();
        let ouvinte = match guardado {
            Some(o) => o,
            None => Self::escutar()?,
        };
        self.esperar_em(&ouvinte, state)
    }
}

/// O Google de verdade.
pub struct Google {
    http: reqwest::Client,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for Google {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Google").finish_non_exhaustive()
    }
}

impl Google {
    /// Monta o cliente HTTP e o runtime.
    ///
    /// # Errors
    /// [`Erro::Login`].
    pub fn novo() -> Resultado<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Erro::Login(e.kind().to_string()))?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| Erro::Login("o cliente HTTP não subiu".to_owned()))?;
        Ok(Self { http, runtime })
    }
}

impl Provedor for Google {
    fn trocar_codigo(
        &self,
        client_id: &str,
        client_secret: &str,
        codigo: &str,
        verificador: &str,
    ) -> Resultado<Tokens> {
        #[derive(serde::Deserialize)]
        struct R {
            access_token: Option<String>,
            refresh_token: Option<String>,
            error: Option<String>,
        }
        let r: R = self.runtime.block_on(async {
            let resposta = self
                .http
                .post(URL_DO_TOKEN)
                .form(&[
                    ("code", codigo),
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                    ("redirect_uri", REDIRECT_URI),
                    ("grant_type", "authorization_code"),
                    ("code_verifier", verificador),
                ])
                .send()
                .await
                .map_err(|e| {
                    Erro::Login(format!("sem resposta do Google ({})", e.without_url()))
                })?;
            resposta
                .json::<R>()
                .await
                .map_err(|_| Erro::Login("resposta inesperada do Google".to_owned()))
        })?;
        if let Some(e) = r.error {
            return Err(Erro::Login(format!(
                "o Google recusou a troca do código ({})",
                codigo_de_erro_seguro(&e)
            )));
        }
        let acesso = r
            .access_token
            .ok_or_else(|| Erro::Login("o Google não devolveu o access token".to_owned()))?;
        Ok(Tokens {
            acesso: Zeroizing::new(acesso),
            renovacao: r.refresh_token.map(Zeroizing::new),
        })
    }

    fn email(&self, acesso: &str) -> Resultado<String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct P {
            email_address: String,
        }
        let p: P = self.runtime.block_on(async {
            self.http
                .get(URL_DO_PERFIL)
                .bearer_auth(acesso)
                .send()
                .await
                .map_err(|e| Erro::Login(format!("sem resposta do Gmail ({})", e.without_url())))?
                .json::<P>()
                .await
                .map_err(|_| Erro::Login("não consegui ler o perfil do Gmail".to_owned()))
        })?;
        Ok(p.email_address)
    }
}

/// O resultado de um login.
pub struct Login {
    /// O refresh token novo.
    pub renovacao: Zeroizing<String>,
    /// O e-mail da conta.
    pub email: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Login")
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

/// Roda o fluxo inteiro: monta o pedido, chama `abrir` com a URL, espera o retorno no
/// `receptor`, troca o código e pega o e-mail. Não grava nada.
///
/// # Errors
/// [`Erro::Login`].
pub fn entrar(
    client_id: &str,
    client_secret: &str,
    abrir: &dyn Fn(&str),
    receptor: &dyn Receptor,
    provedor: &dyn Provedor,
) -> Resultado<Login> {
    receptor.preparar()?;
    let state = aleatorio();
    let verificador = format!("{}{}", aleatorio(), aleatorio());
    abrir(&url_de_autorizacao(client_id, &state, &verificador));
    let codigo = receptor.esperar(&state)?;
    let tokens = provedor.trocar_codigo(client_id, client_secret, &codigo, &verificador)?;
    let renovacao = tokens.renovacao.ok_or_else(|| {
        Erro::Login(
            "o Google não devolveu o refresh token; revogue o acesso em myaccount.google.com/permissions e entre de novo"
                .to_owned(),
        )
    })?;
    let email = provedor.email(&tokens.acesso)?;
    Ok(Login { renovacao, email })
}

/// O login completo de uma conta: acha o aplicativo (client id/secret), roda o fluxo, guarda o
/// token no chaveiro, o e-mail em `email.json` e tira a marca de vencido. Devolve o e-mail.
///
/// # Errors
/// [`Erro::SemCredenciais`] se faltam o client id e o secret; [`Erro::Argumento`] se a conta
/// não existe ou o tipo não conecta; [`Erro::Login`]; [`Erro::Chaveiro`].
pub fn entrar_e_guardar(
    chaveiro: &sbf_chaveiro::Chaveiro,
    arquivo_da_config: &std::path::Path,
    id: &str,
    abrir: &dyn Fn(&str),
    receptor: &dyn Receptor,
    provedor: &dyn Provedor,
) -> Resultado<String> {
    entrar_e_guardar_com(
        chaveiro,
        arquivo_da_config,
        id,
        abrir,
        receptor,
        provedor,
        &|n| std::env::var(n).ok(),
        &crate::credenciais::raizes_padrao(),
    )
}

/// Como [`entrar_e_guardar`], com o ambiente e as pastas de `.env` à escolha (os testes).
///
/// # Errors
/// Como [`entrar_e_guardar`].
#[allow(clippy::too_many_arguments)]
pub fn entrar_e_guardar_com(
    chaveiro: &sbf_chaveiro::Chaveiro,
    arquivo_da_config: &std::path::Path,
    id: &str,
    abrir: &dyn Fn(&str),
    receptor: &dyn Receptor,
    provedor: &dyn Provedor,
    variavel: &dyn Fn(&str) -> Option<String>,
    raizes: &[std::path::PathBuf],
) -> Resultado<String> {
    use crate::config::{Config, Conta};
    use crate::credenciais::{Alvo, cliente_do_app, guardar_login};
    let legado = crate::credenciais::faltando_em(variavel, raizes, Some(chaveiro), Alvo::legado())
        .is_empty();
    let mut config = Config::carregar_efetiva(arquivo_da_config, legado)?;
    let conta: Conta = config
        .conta(id)
        .cloned()
        .ok_or_else(|| Erro::Argumento(format!("não existe a conta {id}")))?;
    if !conta.tipo.disponivel() {
        return Err(Erro::Argumento(format!(
            "{} ainda está em breve",
            conta.tipo.rotulo()
        )));
    }
    let (cid, segredo) = cliente_do_app(
        variavel,
        raizes,
        Some(chaveiro),
        Alvo {
            id: &conta.id,
            tipo: conta.tipo,
        },
    )?;
    let login = entrar(&cid, &segredo, abrir, receptor, provedor)?;
    guardar_login(chaveiro, id, &login.renovacao)?;
    if let Some(c) = config.conta_mut(id) {
        c.conta = Some(login.email.clone());
    }
    config.salvar(arquivo_da_config)?;
    Ok(login.email)
}

/// O login de verdade (navegador, servidor local, Google).
///
/// # Errors
/// Como [`entrar_e_guardar`].
pub fn entrar_no_navegador(
    chaveiro: &sbf_chaveiro::Chaveiro,
    arquivo_da_config: &std::path::Path,
    id: &str,
    aviso: &dyn Fn(&str),
) -> Resultado<String> {
    let google = Google::novo()?;
    let receptor = ReceptorLocal::default();
    entrar_e_guardar(
        chaveiro,
        arquivo_da_config,
        id,
        &|url| {
            aviso(url);
            abrir_no_navegador(url);
        },
        &receptor,
        &google,
    )
}

/// Abre a URL no navegador padrão (`xdg-open`, `open` ou `start`). Falhar não é erro: a URL
/// também é mostrada a quem chamou.
pub fn abrir_no_navegador(url: &str) {
    let (programa, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("rundll32", vec!["url.dll,FileProtocolHandler", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = std::process::Command::new(programa)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn a_url_leva_pkce_state_e_o_redirect_cadastrado() {
        let u = Url::parse(&url_de_autorizacao("meu-id", "estado123", "verif")).unwrap();
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], REDIRECT_URI);
        assert_eq!(q["scope"], ESCOPO);
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["prompt"], "consent");
        assert_eq!(q["state"], "estado123");
        assert_eq!(q["code_challenge_method"], "S256");
        // O exemplo do RFC 7636.
        assert_eq!(
            desafio("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn retorno_confere_o_state_e_recusa_o_que_nao_e_deste_login() {
        let ok = "GET /oauth2callback?code=4%2F0AX&state=abc&scope=x HTTP/1.1";
        assert_eq!(ler_retorno(ok, "abc").unwrap(), "4/0AX");
        for ruim in [
            "GET /oauth2callback?code=4%2F0AX&state=outro HTTP/1.1",
            "GET /oauth2callback?code=4%2F0AX HTTP/1.1",
            "GET /oauth2callback?state=abc HTTP/1.1",
            "GET /outro?code=1&state=abc HTTP/1.1",
            "POST /oauth2callback?code=1&state=abc HTTP/1.1",
            "lixo",
        ] {
            assert!(ler_retorno(ruim, "abc").is_err(), "{ruim}");
        }
        let Err(Erro::Login(m)) = ler_retorno(
            "GET /oauth2callback?error=access_denied&state=abc HTTP/1.1",
            "abc",
        ) else {
            panic!("devia recusar");
        };
        assert!(m.contains("access_denied"));
        // O erro vem do navegador: só caracteres seguros entram na mensagem.
        let Err(Erro::Login(m)) = ler_retorno(
            "GET /oauth2callback?error=%3Cscript%3Ex&state=abc HTTP/1.1",
            "abc",
        ) else {
            panic!("devia recusar");
        };
        assert!(!m.contains('<'));
    }

    struct Duble {
        codigo: Resultado<String>,
        renovacao: Option<&'static str>,
        vistos: RefCell<Vec<String>>,
    }
    impl Receptor for Duble {
        fn esperar(&self, state: &str) -> Resultado<String> {
            self.vistos.borrow_mut().push(state.to_owned());
            self.codigo.clone()
        }
    }
    impl Provedor for Duble {
        fn trocar_codigo(&self, _: &str, _: &str, c: &str, v: &str) -> Resultado<Tokens> {
            assert_eq!(c, "codigo-ficticio");
            assert!(v.len() >= 43, "verificador PKCE com 43+ caracteres");
            Ok(Tokens {
                acesso: Zeroizing::new("acesso-ficticio".to_owned()),
                renovacao: self.renovacao.map(|r| Zeroizing::new(r.to_owned())),
            })
        }
        fn email(&self, a: &str) -> Resultado<String> {
            assert_eq!(a, "acesso-ficticio");
            Ok("pessoa@exemplo.test".to_owned())
        }
    }

    #[test]
    fn o_fluxo_abre_a_url_troca_o_codigo_e_devolve_o_email() {
        let d = Duble {
            codigo: Ok("codigo-ficticio".to_owned()),
            renovacao: Some("refresh-ficticio"),
            vistos: RefCell::default(),
        };
        let aberta = RefCell::new(String::new());
        let l = entrar(
            "id",
            "segredo",
            &|u| *aberta.borrow_mut() = u.to_owned(),
            &d,
            &d,
        )
        .unwrap();
        assert_eq!(l.email, "pessoa@exemplo.test");
        assert_eq!(l.renovacao.as_str(), "refresh-ficticio");
        let state = d.vistos.borrow()[0].clone();
        assert_eq!(state.len(), 32);
        assert!(aberta.borrow().contains(&format!("state={state}")));
    }

    #[test]
    fn sem_refresh_token_ou_com_retorno_ruim_nada_e_devolvido() {
        let sem = Duble {
            codigo: Ok("codigo-ficticio".to_owned()),
            renovacao: None,
            vistos: RefCell::default(),
        };
        assert!(matches!(
            entrar("i", "s", &|_| {}, &sem, &sem),
            Err(Erro::Login(_))
        ));
        let recusado = Duble {
            codigo: Err(Erro::Login("state diferente".to_owned())),
            renovacao: Some("x"),
            vistos: RefCell::default(),
        };
        assert!(entrar("i", "s", &|_| {}, &recusado, &recusado).is_err());
    }

    #[test]
    fn o_servidor_local_so_aceita_o_state_certo() {
        // Porta efêmera própria, sem usar a 8976 do usuário.
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        ouvinte.set_nonblocking(true).unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let receptor = ReceptorLocal::com_tempo(Duration::from_secs(5));
        let cliente = std::thread::spawn(move || {
            let pedir = |alvo: &str| {
                let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
                write!(s, "GET {alvo} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut r = String::new();
                let _ = s.read_to_string(&mut r);
                r
            };
            let favicon = pedir("/favicon.ico");
            let certo = pedir("/oauth2callback?code=C&state=S");
            (favicon, certo)
        });
        assert_eq!(receptor.esperar_em(&ouvinte, "S").unwrap(), "C");
        let (favicon, certo) = cliente.join().unwrap();
        assert!(favicon.starts_with("HTTP/1.1 404"));
        assert!(certo.starts_with("HTTP/1.1 200"));

        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        ouvinte.set_nonblocking(true).unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let cliente = std::thread::spawn(move || {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
            write!(s, "GET /oauth2callback?code=C&state=FALSO HTTP/1.1\r\n\r\n").unwrap();
            let mut r = String::new();
            let _ = s.read_to_string(&mut r);
            r
        });
        assert!(receptor.esperar_em(&ouvinte, "S").is_err());
        assert!(cliente.join().unwrap().starts_with("HTTP/1.1 400"));
    }

    #[test]
    fn o_login_guarda_token_conta_e_limpa_a_marca_de_vencido() {
        use crate::config::{Config, TipoDeEmail};
        use crate::credenciais::{self, Alvo, Credenciais, guardar_app};
        use bancada_segredos::ParametrosDeDerivacao;
        use bancada_segredos::testes::ChaveiroEmMemoria;
        let d = tempfile::tempdir().unwrap();
        let chaveiro = sbf_chaveiro::Chaveiro::abrir_com_chaveiro(
            d.path(),
            std::sync::Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap();
        let arquivo = d.path().join("email.json");
        let mut cfg = Config::default();
        cfg.adicionar(TipoDeEmail::Gmail, "Casa").unwrap();
        cfg.salvar(&arquivo).unwrap();
        let duble = Duble {
            codigo: Ok("codigo-ficticio".to_owned()),
            renovacao: Some("refresh-novo-ficticio"),
            vistos: RefCell::default(),
        };
        // Sem client id/secret do aplicativo não há login (e nada é gravado).
        let entrar = |duble: &Duble| {
            entrar_e_guardar_com(
                &chaveiro,
                &arquivo,
                "gmail",
                &|_| {},
                duble,
                duble,
                &|_| None,
                &[],
            )
        };
        assert!(matches!(entrar(&duble), Err(Erro::SemCredenciais(_))));
        guardar_app(
            &chaveiro,
            TipoDeEmail::Gmail,
            Some("id-do-app"),
            Some("segredo-do-app"),
        )
        .unwrap();
        // Um token antigo marcado como vencido.
        credenciais::guardar_login(&chaveiro, "gmail", "refresh-velho").unwrap();
        let alvo = Alvo {
            id: "gmail",
            tipo: TipoDeEmail::Gmail,
        };
        let velho = Credenciais::resolver(&|_| None, &[], Some(&chaveiro), alvo).unwrap();
        credenciais::marcar_vencida(&chaveiro, "gmail", &velho).unwrap();
        assert!(velho.vencida(Some(&chaveiro), "gmail"));

        let email = entrar(&duble).unwrap();
        assert_eq!(email, "pessoa@exemplo.test");
        let novo = Credenciais::resolver(&|_| None, &[], Some(&chaveiro), alvo).unwrap();
        assert_eq!(novo.refresh_token.as_str(), "refresh-novo-ficticio");
        assert!(!novo.vencida(Some(&chaveiro), "gmail"));
        let cfg = Config::carregar(&arquivo).unwrap();
        assert_eq!(
            cfg.conta("gmail").unwrap().conta.as_deref(),
            Some("pessoa@exemplo.test")
        );
        assert_eq!(cfg.conta("gmail").unwrap().apelido, "Casa");
        // Conta inexistente ou tipo em breve recusam.
        assert!(
            entrar_e_guardar_com(
                &chaveiro,
                &arquivo,
                "nada",
                &|_| {},
                &duble,
                &duble,
                &|_| None,
                &[]
            )
            .is_err()
        );
    }
}
