//! O Chrome visível, controlado por CDP, num perfil dedicado do SBF (ADR-016).
//!
//! - Abre o Chrome pela regra única de processo externo (`sbf-processo`: caminho absoluto de
//!   confiança, ambiente mínimo), com `--remote-debugging-port=0` e a porta lida do
//!   `DevToolsActivePort` do perfil. O CDP só escuta em `127.0.0.1`.
//! - Injeta a ponte ([`crate::bloqueio::script_da_ponte`]) em todo documento novo, em **dois
//!   mundos**: no mundo isolado `sbf` (de onde o SBF lê tudo o que decide algo: o elemento, o
//!   campo, a página que a IA vê) e no mundo principal (só para barrar cedo o clique e o
//!   envio). A página é hostil: o que ela disser em JavaScript (`window.__sbfPonte`, a URL
//!   que ela jura ter) nunca entra numa decisão. A URL do alvo vem do **Chrome**
//!   (`Page.getFrameTree`), não da página.
//! - A rede é vigiada pelo `Fetch` (documento, XHR, fetch e beacons, também nos iframes de
//!   outro site): URL da lista de bloqueio nunca sai, e durante o passo de login só sai envio
//!   que muda estado para o host aprovado.
//! - O clique é de verdade (`Input.dispatchMouseEvent` no centro do elemento), depois de
//!   conferir com `elementFromPoint` que o ponto é do elemento (nada o cobre). A senha só
//!   passa por [`crate::cdp::Cdp::inserir_texto_secreto`].

//!
//! # O perfil, as senhas do Chrome e os cookies
//!
//! O perfil é dedicado ao SBF (fora do cofre, `0700`) e **exclusivo por processo**: um
//! arquivo de trava impede dois Chromes no mesmo perfil. Antes de abrir, o SBF escreve as
//! `Preferences` com o gerenciador de senhas e o preenchimento automático desligados, e apaga
//! os dados de formulário e de login (`Web Data`, `Login Data`) ao abrir e ao fechar: a senha
//! do site vem do chaveiro e nunca fica no Chrome. **O risco que sobra são os cookies**: a
//! sessão do site fica no perfil entre uma execução e outra (é o que evita refazer o login a
//! cada vez) e vale para quem ler a pasta de dados do app. Quem quiser sessão nenhuma apaga a
//! pasta `roteiros/perfil`.
//!
//! # A porta de controle
//!
//! O CDP é falado por `--remote-debugging-port=0` (porta aleatória, só em `127.0.0.1`, lida do
//! `DevToolsActivePort`), e não por `--remote-debugging-pipe`: o pipe pede passar descritores
//! extras ao filho (`pre_exec`), que é `unsafe`, e o workspace proíbe `unsafe`. O Chrome só
//! aceita WebSocket de quem não manda `Origin` (nunca passamos `--remote-allow-origins`), mas
//! qualquer processo do mesmo usuário na máquina pode conectar enquanto a janela está aberta.

use crate::bloqueio::{Bloqueio, Candidato, script_da_ponte};
use crate::cdp::{Cdp, http_json};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::navegador::Navegador;
use crate::pagina::Pagina;
use sbf_chaveiro::Segredo;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

/// O nome do mundo isolado do SBF.
const MUNDO: &str = "sbf";

fn erro(m: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::Navegador(m.into())
}

/// Como abrir o Chrome.
#[derive(Debug, Clone)]
pub struct OpcoesDoChrome {
    /// O executável (senão `SBF_CHROME` ou os nomes de sempre, nas pastas do sistema).
    pub programa: Option<PathBuf>,
    /// O perfil dedicado do SBF (fora do cofre).
    pub perfil: PathBuf,
    /// Sem janela (só os testes e a demonstração; o uso real é visível).
    pub sem_janela: bool,
}

/// Procura o Chrome ou Chromium instalado, só em pastas de confiança.
#[must_use]
pub fn localizar_chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SBF_CHROME").map(PathBuf::from)
        && sbf_processo::confiavel(&p)
    {
        return Some(p);
    }
    for nome in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Some(p) = sbf_processo::localizar(nome) {
            return Some(p);
        }
    }
    #[cfg(target_os = "macos")]
    for p in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ] {
        let p = PathBuf::from(p);
        if sbf_processo::confiavel(&p) {
            return Some(p);
        }
    }
    #[cfg(windows)]
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            let p = PathBuf::from(base).join("Google/Chrome/Application/chrome.exe");
            if sbf_processo::confiavel(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// A trava do perfil: dois Chromes no mesmo perfil brigam pelo `DevToolsActivePort` e pelos
/// cookies, e um apagaria a porta do outro. Fica de pé enquanto o [`Chrome`] existir.
#[derive(Debug)]
pub struct TravaDoPerfil {
    _arquivo: std::fs::File,
}

impl TravaDoPerfil {
    /// Pega a trava exclusiva do perfil.
    ///
    /// # Errors
    /// O perfil já está em uso por outro processo (ou outro Chrome deste).
    pub fn obter(perfil: &Path) -> Resultado<Self> {
        std::fs::create_dir_all(perfil)?;
        let arquivo = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(perfil.join(".sbf-roteiros.lock"))?;
        match arquivo.try_lock() {
            Ok(()) => Ok(Self { _arquivo: arquivo }),
            Err(std::fs::TryLockError::WouldBlock) => Err(erro(
                "o perfil do Chrome dos roteiros já está em uso por outro roteiro",
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

/// Escreve (ou completa) as `Preferences` do perfil com o que o SBF não quer no Chrome: o
/// gerenciador de senhas, o login automático e o preenchimento de endereços e cartões.
///
/// # Errors
/// Disco.
pub fn preparar_preferencias(perfil: &Path) -> Resultado<()> {
    let pasta = perfil.join("Default");
    std::fs::create_dir_all(&pasta)?;
    let arquivo = pasta.join("Preferences");
    let mut v: Value = std::fs::read_to_string(&arquivo)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    v["credentials_enable_service"] = json!(false);
    v["credentials_enable_autosignin"] = json!(false);
    for (grupo, chave) in [
        ("profile", "password_manager_enabled"),
        ("autofill", "profile_enabled"),
        ("autofill", "credit_card_enabled"),
    ] {
        if !v[grupo].is_object() {
            v[grupo] = json!({});
        }
        v[grupo][chave] = json!(false);
    }
    std::fs::write(&arquivo, serde_json::to_vec(&v).unwrap_or_default())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&arquivo, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Apaga o que o Chrome guardou de formulário e de login no perfil (nunca os cookies). Só com o
/// Chrome fechado.
pub fn apagar_dados_de_formulario(perfil: &Path) {
    for nome in [
        "Web Data",
        "Web Data-journal",
        "Login Data",
        "Login Data-journal",
        "Login Data For Account",
        "Login Data For Account-journal",
    ] {
        let _ = std::fs::remove_file(perfil.join("Default").join(nome));
    }
}

/// Os argumentos do Chrome. A porta é aleatória (`0`) e só em `127.0.0.1`; nada de
/// `--remote-allow-origins`.
#[must_use]
pub fn argumentos_do_chrome(perfil: &Path, sem_janela: bool, sem_sandbox: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--remote-debugging-port=0".into(),
        format!("--user-data-dir={}", perfil.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--password-store=basic".into(),
        "--disable-background-networking".into(),
        "--disable-features=Translate,PasswordLeakDetection".into(),
        "--disable-save-password-bubble".into(),
    ];
    if sem_janela {
        args.push("--headless=new".into());
    }
    if sem_sandbox {
        args.push("--no-sandbox".into());
    }
    args.push("about:blank".into());
    args
}

/// O Chrome aberto.
#[derive(Debug)]
pub struct Chrome {
    _trava: TravaDoPerfil,
    perfil: PathBuf,
    filho: Child,
    pagina: Cdp,
    navegador: Cdp,
    barrados: Vec<Bloqueio>,
    ws_pagina: String,
}

fn literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

impl Chrome {
    /// Abre o Chrome e prepara a aba.
    ///
    /// # Errors
    /// Chrome não encontrado, não subiu a tempo, ou falha do CDP.
    pub fn abrir(o: &OpcoesDoChrome) -> Resultado<Self> {
        let programa = o
            .programa
            .clone()
            .or_else(localizar_chrome)
            .ok_or_else(|| erro("não achei o Chrome nem o Chromium instalado"))?;
        std::fs::create_dir_all(&o.perfil)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&o.perfil, std::fs::Permissions::from_mode(0o700));
        }
        // Exclusivo: só quem tem a trava mexe no DevToolsActivePort e nas Preferences.
        let trava = TravaDoPerfil::obter(&o.perfil)?;
        apagar_dados_de_formulario(&o.perfil);
        preparar_preferencias(&o.perfil)?;
        let arquivo_da_porta = o.perfil.join("DevToolsActivePort");
        let _ = std::fs::remove_file(&arquivo_da_porta);
        let args = argumentos_do_chrome(
            &o.perfil,
            o.sem_janela,
            std::env::var_os("SBF_CHROME_SEM_SANDBOX").is_some(),
        );
        let mut cmd = sbf_processo::comando(
            &programa,
            &args,
            &[
                "HOME",
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "XAUTHORITY",
                "XDG_RUNTIME_DIR",
                "XDG_SESSION_TYPE",
                "DBUS_SESSION_BUS_ADDRESS",
            ],
        )
        .map_err(|_| erro("o Chrome não está num lugar de confiança"))?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut filho = cmd
            .spawn()
            .map_err(|e| erro(format!("não abriu o Chrome: {e}")))?;

        let limite = Instant::now() + Duration::from_secs(25);
        let porta = loop {
            if let Ok(t) = std::fs::read_to_string(&arquivo_da_porta)
                && let Some(p) = t.lines().next().and_then(|l| l.trim().parse::<u16>().ok())
            {
                break p;
            }
            if Instant::now() >= limite || filho.try_wait().ok().flatten().is_some() {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(erro("o Chrome não abriu a porta de controle a tempo"));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let montado = Self::conectar(porta, &mut filho);
        match montado {
            Ok((pagina, navegador, ws_pagina)) => Ok(Self {
                _trava: trava,
                perfil: o.perfil.clone(),
                filho,
                pagina,
                navegador,
                barrados: Vec::new(),
                ws_pagina,
            }),
            Err(e) => {
                let _ = filho.kill();
                let _ = filho.wait();
                Err(e)
            }
        }
    }

    fn conectar(porta: u16, _filho: &mut Child) -> Resultado<(Cdp, Cdp, String)> {
        let versao = http_json(porta, "/json/version")?;
        let ws_navegador = versao["webSocketDebuggerUrl"]
            .as_str()
            .ok_or_else(|| erro("o Chrome não informou o alvo do navegador"))?
            .to_owned();
        let limite = Instant::now() + Duration::from_secs(10);
        let ws_pagina = loop {
            let lista = http_json(porta, "/json/list")?;
            if let Some(u) = lista
                .as_array()
                .and_then(|a| a.iter().find(|t| t["type"] == "page"))
                .and_then(|t| t["webSocketDebuggerUrl"].as_str())
            {
                break u.to_owned();
            }
            if Instant::now() >= limite {
                return Err(erro("o Chrome não abriu uma aba"));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut navegador = Cdp::conectar(&ws_navegador)?;
        // O alvo da aba é o último pedaço do endereço (`/devtools/page/<id>`).
        let alvo = ws_pagina.rsplit('/').next().unwrap_or("").to_owned();
        navegador.vigiar_janelas_novas(&alvo)?;
        let mut pagina = Cdp::conectar(&ws_pagina)?;
        pagina.chamar("Page.enable", &json!({}))?;
        pagina.chamar("Runtime.enable", &json!({}))?;
        let (quadro, _) = pagina.quadro_principal()?;
        pagina.definir_mundo(MUNDO, &quadro);
        pagina.chamar("Runtime.addBinding", &json!({"name": "sbfBloqueio"}))?;
        let ponte = script_da_ponte();
        // O mundo isolado primeiro: o ouvinte dele é o primeiro da fila de captura.
        pagina.chamar(
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({"source": ponte, "worldName": MUNDO}),
        )?;
        pagina.chamar(
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({"source": ponte}),
        )?;
        pagina.ativar_rede()?;
        pagina.avaliar_isolado(&ponte)?;
        pagina.avaliar(&ponte)?;
        Ok((pagina, navegador, ws_pagina))
    }

    /// O alvo CDP da aba (`ws://127.0.0.1:...`), para quem precisa de uma segunda conexão (a
    /// pausa dos testes age na página enquanto o executor espera).
    #[must_use]
    pub fn alvo_da_pagina(&self) -> &str {
        &self.ws_pagina
    }

    /// Fecha o Chrome.
    pub fn fechar(mut self) {
        let _ = self.navegador.chamar("Browser.close", &json!({}));
        let limite = Instant::now() + Duration::from_secs(3);
        let mut saiu = false;
        while Instant::now() < limite {
            if self.filho.try_wait().ok().flatten().is_some() {
                saiu = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !saiu {
            let _ = self.filho.kill();
            let _ = self.filho.wait();
        }
        // Com o Chrome fora, nada de formulário nem de login fica no perfil.
        apagar_dados_de_formulario(&self.perfil);
    }

    /// Espera o carregamento terminar (evento de load ou `readyState`).
    fn aguardar_carregar(&mut self) -> Resultado<()> {
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            self.pagina.bombear(Duration::from_millis(100))?;
            self.absorver_eventos();
            let pronto = self
                .pagina
                .avaliar_isolado("document.readyState")
                .unwrap_or(Value::Null);
            if pronto == "complete" {
                // um instante para a ponte do novo documento e a navegação pendente
                std::thread::sleep(Duration::from_millis(80));
                return Ok(());
            }
            if Instant::now() >= limite {
                return Err(erro("a página não terminou de carregar"));
            }
        }
    }

    /// Passa os eventos de bloqueio da fila para `barrados`; devolve se houve navegação.
    fn absorver_eventos(&mut self) -> bool {
        let mut navegou = false;
        // A conexão do navegador só é lida aqui: é onde se fecha a janela nova antes de ela
        // carregar.
        let _ = self.navegador.bombear(Duration::from_millis(1));
        for (campo, termo) in self.navegador.barrados_na_rede.drain(..) {
            self.barrados.push(Bloqueio { campo, termo });
        }
        while let Some(e) = self.pagina.eventos.pop_front() {
            match e["method"].as_str() {
                Some("Runtime.bindingCalled") if e["params"]["name"] == "sbfBloqueio" => {
                    if let Some(b) = e["params"]["payload"]
                        .as_str()
                        .and_then(|t| serde_json::from_str::<Value>(t).ok())
                    {
                        self.barrados.push(Bloqueio {
                            campo: b["campo"].as_str().unwrap_or("").to_owned(),
                            termo: b["termo"].as_str().unwrap_or("").to_owned(),
                        });
                    }
                }
                Some("Page.frameStartedLoading" | "Page.frameNavigated") => navegou = true,
                _ => {}
            }
        }
        // O que a política de rede barrou (vem de dentro do navegador, não da página).
        for (campo, termo) in self.pagina.barrados_na_rede.drain(..) {
            self.barrados.push(Bloqueio { campo, termo });
        }
        navegou
    }

    /// Depois de um clique: junta os eventos e, se começou uma navegação, espera.
    fn depois_da_acao(&mut self) -> Resultado<()> {
        self.pagina.bombear(Duration::from_millis(250))?;
        if self.absorver_eventos() {
            self.aguardar_carregar()?;
        }
        Ok(())
    }

    /// Chama a ponte **no mundo isolado**. Se o documento ainda não a tem (página de erro,
    /// `about:blank`, mundo recriado), injeta e tenta de novo.
    fn ponte(&mut self, chamada: &str) -> Resultado<Value> {
        let expr = format!(
            "(()=>{{const p=window.__sbfPonte;return p?{{ok:true,v:p.{chamada}}}:{{ok:false}}}})()"
        );
        for _ in 0..2 {
            let r = self.pagina.avaliar_isolado(&expr)?;
            if r["ok"] == Value::Bool(true) {
                return Ok(r["v"].clone());
            }
            self.pagina.avaliar_isolado(&script_da_ponte())?;
        }
        Err(erro("a ponte do SBF não subiu na página"))
    }

    /// Uma chamada da aba que um clique pode travar (`window.open` espera a janela nova, que o
    /// Chrome segura até alguém responder): enquanto espera, atende a conexão do navegador,
    /// que fecha a janela.
    fn chamar_atendendo_janelas(&mut self, metodo: &str, params: &Value) -> Resultado<Value> {
        let id = self.pagina.enviar_chamada(metodo, params)?;
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(r) = self.pagina.ler_ate(id, metodo, Duration::from_millis(40))? {
                return Ok(r);
            }
            let _ = self.navegador.bombear(Duration::from_millis(2));
            if Instant::now() >= limite {
                return Err(erro(format!("o Chrome não respondeu a {metodo}")));
            }
        }
    }

    /// A URL que o **Chrome** carregou no quadro principal. É a única em que o executor
    /// confia para decidir onde a senha pode ir.
    fn url_confiavel(&mut self) -> Resultado<String> {
        Ok(self.pagina.quadro_principal()?.1)
    }

    fn focar(&mut self, seletor: &str) -> Resultado<()> {
        let ok = self.ponte(&format!("focarCampo({})", literal(seletor)))?;
        if ok == Value::Bool(true) {
            Ok(())
        } else {
            Err(erro(
                "o campo não existe ou não ficou com o foco (a página o tirou dele)",
            ))
        }
    }
}

impl Navegador for Chrome {
    fn navegar(&mut self, url: &str) -> Resultado<()> {
        if !crate::passo::url_navegavel(url) {
            return Err(erro("só se navega para endereços http e https"));
        }
        self.pagina.eventos.clear();
        let r = self.pagina.chamar("Page.navigate", &json!({"url": url}))?;
        if let Some(t) = r.get("errorText").and_then(Value::as_str) {
            return Err(erro(format!("não abriu a página: {t}")));
        }
        self.aguardar_carregar()
    }

    fn visivel(&mut self, seletor: &str) -> Resultado<bool> {
        Ok(self.ponte(&format!("visivel({})", literal(seletor)))? == Value::Bool(true))
    }

    fn descrever(&mut self, seletor: &str) -> Resultado<Option<Candidato>> {
        let v = self.ponte(&format!("descrever({})", literal(seletor)))?;
        if v.is_null() {
            return Ok(None);
        }
        let tipo = v["tipo"].as_str().unwrap_or("").to_owned();
        let campos = v["campos"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some((
                            p.get(0)?.as_str()?.to_owned(),
                            p.get(1)?.as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(Candidato {
            campos,
            eh_senha: v["eh_senha"] == Value::Bool(true),
            eh_campo: v["eh_campo"] == Value::Bool(true),
            tipo,
        }))
    }

    fn clicar(&mut self, seletor: &str) -> Resultado<()> {
        let alvo = self.ponte(&format!("alvoDeClique({})", literal(seletor)))?;
        if let Some(e) = alvo.get("erro").and_then(Value::as_str) {
            return Err(erro(e));
        }
        let (Some(x), Some(y)) = (
            alvo.get("x").and_then(Value::as_f64),
            alvo.get("y").and_then(Value::as_f64),
        ) else {
            return Err(erro("o elemento não existe"));
        };
        self.pagina.eventos.clear();
        for (tipo, botao, cliques) in [
            ("mouseMoved", "none", 0),
            ("mousePressed", "left", 1),
            ("mouseReleased", "left", 1),
        ] {
            self.chamar_atendendo_janelas(
                "Input.dispatchMouseEvent",
                &json!({"type": tipo, "x": x, "y": y, "button": botao, "clickCount": cliques}),
            )?;
        }
        self.depois_da_acao()
    }

    fn preencher(&mut self, seletor: &str, texto: &str) -> Resultado<()> {
        self.focar(seletor)?;
        self.pagina
            .chamar("Input.insertText", &json!({"text": texto}))?;
        Ok(())
    }

    fn preencher_segredo(&mut self, seletor: &str, segredo: &Segredo) -> Resultado<()> {
        self.focar(seletor)?;
        self.pagina.inserir_texto_secreto(segredo.expor())
    }

    fn clicar_e_baixar(
        &mut self,
        seletor: &str,
        pasta: &Path,
        prazo: Duration,
    ) -> Resultado<PathBuf> {
        std::fs::create_dir_all(pasta)?;
        let pasta = pasta.canonicalize()?;
        self.navegador.chamar(
            "Browser.setDownloadBehavior",
            &json!({"behavior": "allow", "downloadPath": pasta.to_string_lossy()}),
        )?;
        let antes: std::collections::HashSet<std::ffi::OsString> = std::fs::read_dir(&pasta)?
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        self.clicar(seletor)?;
        let limite = Instant::now() + prazo;
        loop {
            self.pagina.bombear(Duration::from_millis(100))?;
            self.absorver_eventos();
            if !self.barrados.is_empty() {
                return Err(erro("o clique foi barrado pela lista de segurança"));
            }
            let novo = std::fs::read_dir(&pasta)?
                .filter_map(Result::ok)
                .filter(|e| !antes.contains(&e.file_name()))
                .map(|e| e.path())
                .find(|p| {
                    let nome = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    !nome.ends_with(".crdownload")
                        && !nome.starts_with('.')
                        && !nome.ends_with(".tmp")
                });
            if let Some(p) = novo {
                // espera o tamanho estabilizar; o Chrome renomeia o arquivo ao terminar, e um
                // nome que some no meio da conferência é só "ainda não terminou"
                if let Ok(t1) = std::fs::metadata(&p).map(|m| m.len()) {
                    std::thread::sleep(Duration::from_millis(250));
                    if std::fs::metadata(&p).map(|m| m.len()).ok() == Some(t1) {
                        return Ok(p);
                    }
                }
            }
            if Instant::now() >= limite {
                return Err(erro("o download não terminou a tempo"));
            }
        }
    }

    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>> {
        let v = self.ponte(&format!("tabela({})", literal(seletor)))?;
        let linhas = v.as_array().ok_or_else(|| erro("a tabela não existe"))?;
        Ok(linhas
            .iter()
            .map(|l| {
                l.as_array()
                    .map(|c| {
                        c.iter()
                            .map(|x| x.as_str().unwrap_or("").to_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect())
    }

    fn pagina(&mut self) -> Resultado<Pagina> {
        let v = self.ponte("pagina()")?;
        let mut p: Pagina = if v.is_null() {
            Pagina::default()
        } else {
            serde_json::from_value(v).map_err(|_| erro("a página simplificada veio malformada"))?
        };
        // A URL é a que o Chrome carregou, não a que o JavaScript da página informa.
        p.url = self.url_confiavel()?;
        Ok(p)
    }

    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>> {
        self.pagina.bombear(Duration::from_millis(30))?;
        self.absorver_eventos();
        // Reserva: o que a ponte do mundo isolado guardou.
        if self.barrados.is_empty()
            && let Ok(v) = self.ponte("pegarBloqueios()")
            && let Some(a) = v.as_array()
        {
            for b in a {
                self.barrados.push(Bloqueio {
                    campo: b["campo"].as_str().unwrap_or("").to_owned(),
                    termo: b["termo"].as_str().unwrap_or("").to_owned(),
                });
            }
        }
        Ok(std::mem::take(&mut self.barrados))
    }

    fn fase_de_login(&mut self, host: Option<&str>) {
        self.pagina.definir_host_de_login(host.map(str::to_owned));
    }

    fn trazer_para_frente(&mut self) {
        let _ = self.pagina.chamar("Page.bringToFront", &json!({}));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn preferencias_desligam_senhas_e_preenchimento_e_guardam_o_resto() {
        let d = tempfile::tempdir().unwrap();
        let pasta = d.path().join("Default");
        std::fs::create_dir_all(&pasta).unwrap();
        std::fs::write(
            pasta.join("Preferences"),
            r#"{"credentials_enable_service":true,"profile":{"name":"SBF","password_manager_enabled":true},"autofill":{"profile_enabled":true},"outra":1}"#,
        )
        .unwrap();
        preparar_preferencias(d.path()).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(pasta.join("Preferences")).unwrap())
                .unwrap();
        assert_eq!(v["credentials_enable_service"], false);
        assert_eq!(v["profile"]["password_manager_enabled"], false);
        assert_eq!(v["autofill"]["profile_enabled"], false);
        assert_eq!(v["autofill"]["credit_card_enabled"], false);
        assert_eq!(v["profile"]["name"], "SBF", "o resto do perfil fica");
        assert_eq!(v["outra"], 1);
        // perfil novo e arquivo corrompido também ficam certos
        let d2 = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d2.path().join("Default")).unwrap();
        std::fs::write(d2.path().join("Default/Preferences"), "{lixo").unwrap();
        preparar_preferencias(d2.path()).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(d2.path().join("Default/Preferences")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["autofill"]["credit_card_enabled"], false);
    }

    #[test]
    fn dados_de_formulario_e_login_sao_apagados_e_os_cookies_ficam() {
        let d = tempfile::tempdir().unwrap();
        let pasta = d.path().join("Default");
        std::fs::create_dir_all(&pasta).unwrap();
        for n in ["Web Data", "Login Data", "Login Data-journal", "Cookies"] {
            std::fs::write(pasta.join(n), "x").unwrap();
        }
        apagar_dados_de_formulario(d.path());
        assert!(!pasta.join("Web Data").exists());
        assert!(!pasta.join("Login Data").exists());
        assert!(!pasta.join("Login Data-journal").exists());
        assert!(pasta.join("Cookies").exists(), "a sessão do site fica");
    }

    #[test]
    fn o_perfil_e_exclusivo_por_processo() {
        let d = tempfile::tempdir().unwrap();
        let primeira = TravaDoPerfil::obter(d.path()).unwrap();
        let segunda = TravaDoPerfil::obter(d.path());
        assert!(segunda.is_err(), "dois Chromes no mesmo perfil");
        drop(primeira);
        assert!(TravaDoPerfil::obter(d.path()).is_ok(), "solta ao fechar");
    }

    #[test]
    fn a_porta_de_controle_e_aleatoria_e_sem_origem_liberada() {
        let args = argumentos_do_chrome(Path::new("/tmp/p"), true, false);
        assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
        assert!(
            args.iter()
                .all(|a| !a.starts_with("--remote-allow-origins"))
        );
        assert!(
            args.iter()
                .all(|a| !a.starts_with("--remote-debugging-address"))
        );
        assert!(args.contains(&"--headless=new".to_owned()));
        assert!(!args.contains(&"--no-sandbox".to_owned()));
        assert!(
            argumentos_do_chrome(Path::new("/tmp/p"), false, true)
                .contains(&"--no-sandbox".to_owned())
        );
    }
}
