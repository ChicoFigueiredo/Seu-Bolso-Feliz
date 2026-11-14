//! O Chrome visível, controlado por CDP, num perfil dedicado do SBF (ADR-016).
//!
//! - Abre o Chrome pela regra única de processo externo (`sbf-processo`: caminho absoluto de
//!   confiança, ambiente mínimo), com `--remote-debugging-port=0` e a porta lida do
//!   `DevToolsActivePort` do perfil. O CDP só escuta em `127.0.0.1`.
//! - Injeta a ponte ([`crate::bloqueio::script_da_ponte`]) em todo documento novo: ela barra
//!   clique e envio perigosos na fase de captura e entrega a página simplificada.
//! - O clique é de verdade (`Input.dispatchMouseEvent` no centro do elemento). A senha só passa
//!   por [`crate::cdp::Cdp::inserir_texto_secreto`].

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

/// O Chrome aberto.
#[derive(Debug)]
pub struct Chrome {
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
        let arquivo_da_porta = o.perfil.join("DevToolsActivePort");
        let _ = std::fs::remove_file(&arquivo_da_porta);
        let mut args: Vec<String> = vec![
            "--remote-debugging-port=0".into(),
            format!("--user-data-dir={}", o.perfil.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--password-store=basic".into(),
            "--disable-background-networking".into(),
            "--disable-features=Translate,PasswordLeakDetection".into(),
            "--disable-save-password-bubble".into(),
        ];
        if o.sem_janela {
            args.push("--headless=new".into());
        }
        if std::env::var_os("SBF_CHROME_SEM_SANDBOX").is_some() {
            args.push("--no-sandbox".into());
        }
        args.push("about:blank".into());
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
        let navegador = Cdp::conectar(&ws_navegador)?;
        let mut pagina = Cdp::conectar(&ws_pagina)?;
        pagina.chamar("Page.enable", &json!({}))?;
        pagina.chamar("Runtime.enable", &json!({}))?;
        pagina.chamar("Runtime.addBinding", &json!({"name": "sbfBloqueio"}))?;
        let ponte = script_da_ponte();
        pagina.chamar(
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({"source": ponte}),
        )?;
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
        while Instant::now() < limite {
            if self.filho.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }

    /// Espera o carregamento terminar (evento de load ou `readyState`).
    fn aguardar_carregar(&mut self) -> Resultado<()> {
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            self.pagina.bombear(Duration::from_millis(100))?;
            self.absorver_eventos();
            let pronto = self.pagina.avaliar("document.readyState")?;
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

    fn ponte(&mut self, chamada: &str) -> Resultado<Value> {
        let expr = format!("(window.__sbfPonte ? window.__sbfPonte.{chamada} : null)");
        let v = self.pagina.avaliar(&expr)?;
        if v.is_null() && self.pagina.avaliar("!!window.__sbfPonte")? == Value::Bool(false) {
            // documento sem a ponte (página de erro, about:blank): injeta e tenta de novo
            self.pagina.avaliar(&script_da_ponte())?;
            return self.pagina.avaliar(&expr);
        }
        Ok(v)
    }

    fn focar(&mut self, seletor: &str) -> Resultado<()> {
        let s = literal(seletor);
        let ok = self.pagina.avaliar(&format!(
            "(()=>{{const e=document.querySelector({s});if(!e)return false;e.scrollIntoView({{block:'center'}});\
             e.focus();if(e.select)e.select();return true}})()"
        ))?;
        if ok == Value::Bool(true) {
            Ok(())
        } else {
            Err(erro("o campo não existe"))
        }
    }
}

impl Navegador for Chrome {
    fn navegar(&mut self, url: &str) -> Resultado<()> {
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
        }))
    }

    fn clicar(&mut self, seletor: &str) -> Resultado<()> {
        let s = literal(seletor);
        let centro = self.pagina.avaliar(&format!(
            "(()=>{{const e=document.querySelector({s});if(!e)return null;e.scrollIntoView({{block:'center'}});\
             const r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()"
        ))?;
        let (Some(x), Some(y)) = (
            centro.get(0).and_then(Value::as_f64),
            centro.get(1).and_then(Value::as_f64),
        ) else {
            return Err(erro("o elemento não existe"));
        };
        self.pagina.eventos.clear();
        for (tipo, botao, cliques) in [
            ("mouseMoved", "none", 0),
            ("mousePressed", "left", 1),
            ("mouseReleased", "left", 1),
        ] {
            self.pagina.chamar(
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
                // espera o tamanho estabilizar
                let t1 = std::fs::metadata(&p)?.len();
                std::thread::sleep(Duration::from_millis(250));
                if std::fs::metadata(&p)?.len() == t1 {
                    return Ok(p);
                }
            }
            if Instant::now() >= limite {
                return Err(erro("o download não terminou a tempo"));
            }
        }
    }

    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>> {
        let s = literal(seletor);
        let v = self.pagina.avaliar(&format!(
            "(()=>{{const t=document.querySelector({s});if(!t)return null;\
             return Array.from(t.rows).map(r=>Array.from(r.cells).map(c=>c.innerText.trim()))}})()"
        ))?;
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
        if v.is_null() {
            return Ok(Pagina::default());
        }
        serde_json::from_value(v).map_err(|_| erro("a página simplificada veio malformada"))
    }

    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>> {
        self.pagina.bombear(Duration::from_millis(30))?;
        self.absorver_eventos();
        // Reserva: o que a ponte guardou na própria página.
        if self.barrados.is_empty() {
            let v = self.pagina.avaliar(
                "(()=>{const b=window.__sbfBloqueios||[];window.__sbfBloqueios=[];return b})()",
            )?;
            if let Some(a) = v.as_array() {
                for b in a {
                    self.barrados.push(Bloqueio {
                        campo: b["campo"].as_str().unwrap_or("").to_owned(),
                        termo: b["termo"].as_str().unwrap_or("").to_owned(),
                    });
                }
            }
        }
        Ok(std::mem::take(&mut self.barrados))
    }

    fn trazer_para_frente(&mut self) {
        let _ = self.pagina.chamar("Page.bringToFront", &json!({}));
    }
}
