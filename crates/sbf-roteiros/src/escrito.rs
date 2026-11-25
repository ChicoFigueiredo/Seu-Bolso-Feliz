//! O **roteiro escrito** (Luau) sobre os verbos da bancada (ADR-022, spec 14).
//!
//! O host Luau (`bancada_navegador::luau`) só conhece o trait [`Verbos`]; [`SbfVerbos`] o
//! implementa por cima do [`Navegador`], da lista de bloqueio, do chaveiro, do e-mail, da
//! pausa e do inventário. É aqui que mora a política:
//!
//! - **Hosts.** `abrir` só vai para `https` e para um host do manifesto; depois de abrir ou
//!   clicar, a página em que o navegador caiu (inclusive por redirecionamento) tem de estar
//!   na allowlist, senão o roteiro termina. Os verbos de segredo recusam página fora dela.
//! - **Bloqueio.** `clicar` e `baixar` conferem o elemento na lista de bloqueio (e o navegador
//!   confere de novo no clique). A classe do manifesto decide quem pode clicar em "Atualizar".
//! - **Segredo.** O script passa só o rótulo da senha (que tem de ser o do manifesto); o CPF é
//!   o único do chaveiro e não leva rótulo. Senha, CPF e código do e-mail vão do
//!   chaveiro/Gmail direto ao campo, **só no host aprovado**, e entram na lista de limpeza: todo
//!   texto que volta ao script, ao log, ao banco ou a um erro passa por ela. A conferência do
//!   host não basta (a página pode redirecionar entre a conferência e a digitação): o motor a
//!   refaz imediatamente antes de digitar, pelo predicado de [`SbfVerbos::url_aprovada`], e
//!   aborta sem digitar se a página mudou.
//! - **Pausa.** CAPTCHA e MFA de token (detectados na página) e `navegador.pausa` esperam o
//!   usuário.
//! - **Download.** Vai para o inventário (origem `roteiro:<nome>/<execução>/<arquivo>`); o
//!   script recebe só o nome.
//!
//! O **modo teste** ([`OpcoesEscrito`]) deixa o roteiro abrir `https://portal.exemplo.test/…`
//! e navegar de fato para `http://127.0.0.1:<porta>/…` (o site falso), mas toda conferência
//! (allowlist, host da senha) usa o host LÓGICO. Só o `testar` liga isto; produção nunca.

use crate::banco::BancoDeRoteiros;
use crate::bloqueio::{self, Bloqueio};
use crate::codigo_email::{BuscaDeCodigo, CaixaDeEmail, ErroDeCodigo, Relogio, esperar_codigo};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::executor::{Decisao, Destino, FonteDeSenhas, Opcoes, Pausa, Recebido, limpar_texto};
use crate::manifesto::{Manifesto, Mfa};
use crate::navegador::{Navegador, TipoDeCampo};
use bancada_navegador::ErroDoNavegador;
use bancada_navegador::luau::{
    ErroDeRoteiro as ErroDoLuau, ErroDeVerbo, ResultadoDeVerbo, Saida, Verbos,
};
use bancada_navegador::url::host_de;
use sbf_chaveiro::Segredo;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Quanto antes do último clique o e-mail com o código ainda vale (o site pode mandar o código
/// um instante antes de o clique terminar de voltar).
const FOLGA_DO_PEDIDO: Duration = Duration::from_secs(10);
/// O fuso do usuário: UTC−3 fixo (São Paulo não tem horário de verão desde 2019).
const FUSO_EM_SEGUNDOS: i64 = -3 * 3600;
/// O que o roteiro vê quando a caixa de e-mail não tem autorização (sem dado nenhum).
const MENSAGEM_SEM_AUTORIZACAO_DO_GMAIL: &str =
    "Autorização do Gmail ausente: entre de novo em Conexões › E-mail";
/// Quantos registros do roteiro se guardam no relatório.
const MAXIMO_DE_REGISTROS: usize = 500;

/// O modo teste: o que o `testar` liga para rodar contra o site falso local. **Produção nunca
/// usa**: [`Default`] é "tudo desligado".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpcoesEscrito {
    /// Pares (host lógico, base real): quando o roteiro abre `https://<host lógico>/x`, o
    /// navegador vai para `<base real>/x` (por exemplo `http://127.0.0.1:4101`). Toda
    /// conferência usa o host lógico; a página que o navegador devolve é traduzida de volta.
    pub mapa_de_hosts: Vec<(String, String)>,
    /// Aceita `http://` (só para o próprio computador: `127.0.0.1`, `localhost`) quando o host
    /// está no manifesto.
    pub aceitar_http_loopback: bool,
}

impl OpcoesEscrito {
    /// As opções servem a este manifesto? O host lógico tem de estar em `hosts` e a base real
    /// tem de ser `http://127.0.0.1` ou `http://localhost`, sem caminho.
    ///
    /// # Errors
    /// [`ErroDeRoteiro::Invalido`].
    pub fn validar(&self, manifesto: &Manifesto) -> Resultado<()> {
        for (logico, base) in &self.mapa_de_hosts {
            if !manifesto.hosts.iter().any(|h| h == logico) {
                return Err(ErroDeRoteiro::Invalido(
                    "modo teste: o host lógico não está em `hosts` do manifesto".into(),
                ));
            }
            let b = base.trim_end_matches('/').to_ascii_lowercase();
            let resto = b.strip_prefix("http://").unwrap_or("");
            let loopback = matches!(host_de(&b).as_deref(), Some("127.0.0.1" | "localhost"));
            if !loopback || resto.contains('/') || resto.contains('@') {
                return Err(ErroDeRoteiro::Invalido(
                    "modo teste: a base real tem de ser http://127.0.0.1:<porta> (ou localhost)"
                        .into(),
                ));
            }
        }
        Ok(())
    }
}

/// A pausa de verdade do roteiro escrito: olha o campo `continuar` de `execucao_escrita`, que
/// a janela grava.
#[derive(Debug)]
pub struct PausaDoEscrito<'a> {
    pub banco: &'a BancoDeRoteiros,
    pub prazo: Duration,
    pub intervalo: Duration,
    pub cancelar: &'a AtomicBool,
}

impl Pausa for PausaDoEscrito<'_> {
    fn aguardar(&self, execucao: i64, _motivo: &str) -> Decisao {
        let inicio = Instant::now();
        loop {
            if self.cancelar.load(Ordering::SeqCst) {
                return Decisao::Cancelar;
            }
            match self.banco.pedido_do_usuario_escrita(execucao) {
                Ok(1) => return Decisao::Continuar,
                Ok(0) | Err(_) => {}
                Ok(_) => return Decisao::Cancelar,
            }
            if inicio.elapsed() >= self.prazo {
                return Decisao::Expirou;
            }
            std::thread::sleep(self.intervalo);
        }
    }
}

/// Tudo de que os verbos precisam. Quem monta é o serviço (`servico::executar_escrito_em`).
pub struct Entradas<'a> {
    pub manifesto: &'a Manifesto,
    pub execucao: i64,
    pub navegador: &'a mut dyn Navegador,
    pub senhas: &'a dyn FonteDeSenhas,
    /// A caixa de onde vem o código do MFA por e-mail (`None` onde não há e-mail).
    pub caixa: Option<&'a dyn CaixaDeEmail>,
    pub relogio: &'a dyn Relogio,
    pub pausa: &'a dyn Pausa,
    pub destino: &'a mut dyn Destino,
    pub banco: &'a BancoDeRoteiros,
    /// A pasta onde os downloads desta execução caem antes de irem ao inventário.
    pub pasta: PathBuf,
    pub opcoes: Opcoes,
    pub teste: OpcoesEscrito,
    pub cancelar: &'a AtomicBool,
}

impl std::fmt::Debug for Entradas<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entradas")
            .field("execucao", &self.execucao)
            .finish_non_exhaustive()
    }
}

/// Os verbos do roteiro escrito sobre o navegador, a política e os serviços do SBF.
pub struct SbfVerbos<'a> {
    e: Entradas<'a>,
    /// Todo segredo usado (senha, CPF, código): é com eles que se limpa o que sai.
    usados: Vec<Segredo>,
    inicio: SystemTime,
    ultimo_clique: Option<SystemTime>,
    /// A página onde o usuário acabou de resolver uma pausa (não detecta de novo ali).
    liberada_em: Option<String>,
    chamadas: u32,
    baixados: u32,
    novos: u32,
    pausas: u32,
    registros: Vec<String>,
    ultimo_bloqueio: Option<Bloqueio>,
    cancelado: bool,
    expirou: bool,
    /// Quanto tempo o roteiro ficou parado numa pausa **automática** (CAPTCHA, código do
    /// aplicativo): o host Luau o desconta do tempo total.
    pausado: Duration,
}

impl std::fmt::Debug for SbfVerbos<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nada de segredo, caminho de arquivo nem estado do navegador.
        f.debug_struct("SbfVerbos")
            .field("execucao", &self.e.execucao)
            .field("chamadas", &self.chamadas)
            .finish_non_exhaustive()
    }
}

fn falhou(m: impl Into<String>) -> ErroDeVerbo {
    ErroDeVerbo::Falhou(m.into())
}

fn do_banco(_: ErroDeRoteiro) -> ErroDeVerbo {
    falhou("falha ao gravar o histórico da execução")
}

/// O endereço sem consulta nem fragmento (podem levar chave de sessão), para o histórico.
fn resumo_da_url(url: &str) -> String {
    let u = url.split(['?', '#']).next().unwrap_or("");
    u.chars().take(160).collect()
}

fn e_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// A URL real traduzida para o host lógico (modo teste); fora dele, a própria URL.
fn url_logica(teste: &OpcoesEscrito, url: &str) -> String {
    let minuscula = url.to_ascii_lowercase();
    for (logico, base) in &teste.mapa_de_hosts {
        let base = base.trim_end_matches('/').to_ascii_lowercase();
        if let Some(resto) = minuscula.strip_prefix(&base)
            && (resto.is_empty() || resto.starts_with(['/', '?', '#']))
        {
            return format!("https://{logico}{}", &url[base.len()..]);
        }
    }
    url.to_owned()
}

/// A URL (lógica) é de um dos `hosts`, por `https` (ou `http` no próprio computador, só no modo
/// teste), sem usuário nem senha embutidos?
fn url_aprovada_em(hosts: &[String], teste: &OpcoesEscrito, url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    let autoridade = u
        .split_once("://")
        .map_or("", |(_, r)| r.split(['/', '?', '#']).next().unwrap_or(""));
    if autoridade.contains('@') {
        return false;
    }
    let Some(host) = host_de(&u) else {
        return false;
    };
    if !hosts.contains(&host) {
        return false;
    }
    u.starts_with("https://")
        || (teste.aceitar_http_loopback && u.starts_with("http://") && e_loopback(&host))
}

fn resumo_do_seletor(seletor: &str) -> String {
    seletor.chars().take(80).collect()
}

impl<'a> SbfVerbos<'a> {
    /// Os verbos, prontos para o [`bancada_navegador::luau::executar`].
    #[must_use]
    pub fn novo(entradas: Entradas<'a>) -> Self {
        let inicio = entradas.relogio.agora();
        Self {
            e: entradas,
            usados: Vec::new(),
            inicio,
            ultimo_clique: None,
            liberada_em: None,
            chamadas: 0,
            baixados: 0,
            novos: 0,
            pausas: 0,
            registros: Vec::new(),
            ultimo_bloqueio: None,
            cancelado: false,
            expirou: false,
            pausado: Duration::ZERO,
        }
    }

    /// O tempo que o roteiro passou em pausas automáticas (CAPTCHA, código do aplicativo
    /// detectado em `abrir`, `clicar` e `esperar`) desde a última leitura. **Zera ao ler.** O
    /// host Luau desconta isto do tempo total do roteiro: o usuário que demora a resolver o
    /// CAPTCHA não pode estourar o teto de execução.
    pub fn tempo_pausado_automatico(&mut self) -> Duration {
        std::mem::take(&mut self.pausado)
    }

    /// Arquivos entregues ao inventário nesta execução.
    #[must_use]
    pub fn baixados(&self) -> u32 {
        self.baixados
    }

    /// Quantos deles eram achados novos.
    #[must_use]
    pub fn novos(&self) -> u32 {
        self.novos
    }

    /// Quantas pausas houve.
    #[must_use]
    pub fn pausas(&self) -> u32 {
        self.pausas
    }

    /// O que o roteiro registrou, já limpo de segredo.
    #[must_use]
    pub fn registros(&self) -> &[String] {
        &self.registros
    }

    /// O bloqueio que a lista barrou, se foi ele que parou o roteiro.
    #[must_use]
    pub fn ultimo_bloqueio(&self) -> Option<&Bloqueio> {
        self.ultimo_bloqueio.as_ref()
    }

    /// O usuário não voltou da pausa a tempo?
    #[must_use]
    pub fn pausa_expirou(&self) -> bool {
        self.expirou
    }

    /// O roteiro parou porque foi cancelado (sinal de cancelar ou "Abortar" da pausa)?
    #[must_use]
    pub fn foi_cancelado(&self) -> bool {
        self.cancelado
    }

    /// Tira dos textos todo segredo já usado (senha, CPF, código).
    #[must_use]
    pub fn limpo(&self, texto: &str) -> String {
        limpar_texto(texto, &self.usados)
    }

    // ---------------------------------------------------------------- infraestrutura

    /// Começo de todo verbo: cancelamento e contagem.
    fn entrar(&mut self) -> ResultadoDeVerbo<()> {
        self.conferir_cancelado()?;
        self.chamadas = self.chamadas.saturating_add(1);
        self.e
            .banco
            .atualizar_chamada_escrita(self.e.execucao, self.chamadas)
            .map_err(do_banco)
    }

    fn conferir_cancelado(&mut self) -> ResultadoDeVerbo<()> {
        if self.e.cancelar.load(Ordering::SeqCst) {
            self.cancelado = true;
            return Err(ErroDeVerbo::Abortado);
        }
        Ok(())
    }

    fn anotar(&self, tipo: &str, mensagem: &str) {
        // Falha do histórico não derruba o roteiro.
        let _ = self
            .e
            .banco
            .evento_escrito(self.e.execucao, tipo, &self.limpo(mensagem));
    }

    fn do_navegador(&self, e: &ErroDoNavegador) -> ErroDeVerbo {
        falhou(self.limpo(&e.to_string()))
    }

    fn bloquear(&mut self, b: Bloqueio) -> ErroDeVerbo {
        let motivo = b.to_string();
        self.anotar("bloqueio", &motivo);
        self.ultimo_bloqueio = Some(b);
        ErroDeVerbo::Bloqueado { motivo }
    }

    // ---------------------------------------------------------------- hosts

    /// A URL real (a que o navegador devolve) traduzida para o host lógico, no modo teste.
    fn logica(&self, url: &str) -> String {
        url_logica(&self.e.teste, url)
    }

    /// A URL lógica traduzida para onde o navegador deve ir, no modo teste.
    fn real(&self, url: &str) -> String {
        let minuscula = url.to_ascii_lowercase();
        for (logico, base) in &self.e.teste.mapa_de_hosts {
            let prefixo = format!("https://{logico}");
            if let Some(resto) = minuscula.strip_prefix(&prefixo)
                && (resto.is_empty() || resto.starts_with(['/', '?', '#']))
            {
                return format!("{}{}", base.trim_end_matches('/'), &url[prefixo.len()..]);
            }
        }
        url.to_owned()
    }

    /// A URL (lógica) é de um host do manifesto, por `https` (ou `http` no próprio computador,
    /// só no modo teste), sem usuário nem senha embutidos?
    fn url_permitida(&self, url: &str) -> bool {
        url_aprovada_em(&self.e.manifesto.hosts, &self.e.teste, url)
    }

    /// O predicado que o navegador reconfere **dentro do motor**, imediatamente antes de digitar
    /// um segredo: recebe a URL REAL que o Chrome carregou, traduz para o host lógico (modo
    /// teste) e aplica a mesma regra de [`Self::url_permitida`]. Dono dos dados (não empresta
    /// o `self`, que o navegador está usando).
    pub fn url_aprovada(&self) -> impl Fn(&str) -> bool + use<> {
        let hosts = self.e.manifesto.hosts.clone();
        let teste = self.e.teste.clone();
        move |url: &str| url_aprovada_em(&hosts, &teste, &url_logica(&teste, url))
    }

    /// A página em que o navegador está agora é permitida e não é de bloqueio? Com `estrito`,
    /// não conseguir ler a página também é falha (o `abrir` não segue às cegas).
    fn conferir_pagina_atual(&mut self, estrito: bool) -> ResultadoDeVerbo<()> {
        let p = match self.e.navegador.pagina() {
            Ok(p) => p,
            Err(e) if estrito => return Err(self.do_navegador(&e)),
            Err(_) => return Ok(()),
        };
        let logica = self.logica(&p.url);
        if !self.url_permitida(&logica) {
            self.anotar("host", "a página saiu dos hosts do manifesto");
            return Err(ErroDeVerbo::HostNaoPermitido);
        }
        if let Some(b) = bloqueio::avaliar_url(&logica) {
            return Err(self.bloquear(b));
        }
        Ok(())
    }

    /// O script do navegador barrou algo no clique?
    fn conferir_barrado(&mut self) -> ResultadoDeVerbo<()> {
        match self.e.navegador.bloqueios() {
            Ok(v) => match v.into_iter().next() {
                Some(b) => Err(self.bloquear(b)),
                None => Ok(()),
            },
            Err(e) => Err(self.do_navegador(&e)),
        }
    }

    /// Segredo só vai para a página de um host do manifesto. Devolve o host REAL da página (é
    /// o que o navegador compara nas requisições da fase de login).
    fn conferir_host_do_segredo(&mut self) -> ResultadoDeVerbo<String> {
        let p = match self.e.navegador.pagina() {
            Ok(p) => p,
            Err(e) => return Err(self.do_navegador(&e)),
        };
        let logica = self.logica(&p.url);
        if !self.url_permitida(&logica) {
            return Err(ErroDeVerbo::HostNaoPermitido);
        }
        host_de(&p.url).ok_or(ErroDeVerbo::HostNaoPermitido)
    }

    /// O campo onde o segredo cai: a senha só num `input type=password`; o CPF e o código só
    /// num campo de texto (nunca de senha, botão ou área).
    fn conferir_campo_do_segredo(&mut self, seletor: &str, senha: bool) -> ResultadoDeVerbo<()> {
        let cand = self.descrever(seletor)?;
        if senha {
            if cand.eh_senha && cand.tipo == "password" {
                return Ok(());
            }
            return Err(falhou(
                "a senha do chaveiro só vai num campo de senha (input type=password)",
            ));
        }
        if cand.eh_campo
            && !cand.eh_senha
            && matches!(cand.tipo.as_str(), "text" | "tel" | "number" | "search")
        {
            return Ok(());
        }
        Err(falhou("o CPF e o código só vão num campo de texto"))
    }

    /// A digitação do segredo falhou no navegador. Se foi porque a página saiu dos hosts do
    /// manifesto (um redirecionamento entre a conferência e a digitação), o roteiro TERMINA
    /// (`HostNaoPermitido`, que o `pcall` do script não captura); senão, é falha comum. Nunca
    /// leva o segredo.
    fn falha_de_digitacao(&mut self, e: &ErroDoNavegador) -> ErroDeVerbo {
        if matches!(
            self.conferir_host_do_segredo(),
            Err(ErroDeVerbo::HostNaoPermitido)
        ) {
            self.anotar(
                "host",
                "a página saiu dos hosts antes de o segredo ser digitado",
            );
            return ErroDeVerbo::HostNaoPermitido;
        }
        falhou(limpar_texto(&e.to_string(), &self.usados))
    }

    // ---------------------------------------------------------------- elementos

    fn descrever(&mut self, seletor: &str) -> ResultadoDeVerbo<bloqueio::Candidato> {
        match self.e.navegador.descrever(seletor) {
            Ok(Some(c)) => Ok(c),
            Ok(None) => Err(falhou(format!(
                "o elemento \"{}\" não existe",
                resumo_do_seletor(seletor)
            ))),
            Err(e) => Err(self.do_navegador(&e)),
        }
    }

    /// Espera o seletor ficar visível. `false` se o prazo acabou.
    fn esperar_ate(&mut self, seletor: &str, prazo: Duration) -> ResultadoDeVerbo<bool> {
        let inicio = Instant::now();
        loop {
            match self.e.navegador.visivel(seletor) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(e) => return Err(self.do_navegador(&e)),
            }
            self.conferir_cancelado()?;
            if inicio.elapsed() >= prazo {
                return Ok(false);
            }
            std::thread::sleep(self.e.opcoes.intervalo);
        }
    }

    fn exigir_visivel(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        if self.esperar_ate(seletor, self.e.opcoes.espera)? {
            Ok(())
        } else {
            Err(falhou(format!(
                "o elemento \"{}\" não apareceu",
                resumo_do_seletor(seletor)
            )))
        }
    }

    /// A lista de bloqueio, com a exceção da classe do manifesto, antes de clicar.
    fn conferir_clique(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        let cand = self.descrever(seletor)?;
        if let Some(b) = bloqueio::avaliar_para_classe(&cand, self.e.manifesto.classe) {
            return Err(self.bloquear(b));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- pausa

    /// Passa a vez ao usuário e espera. `Err(Abortado)` se ele cancela ou não volta a tempo.
    fn pausar(&mut self, motivo: &str) -> ResultadoDeVerbo<()> {
        self.pausas += 1;
        let motivo: String = self.limpo(motivo).chars().take(200).collect();
        self.e
            .banco
            .marcar_aguardando_escrita(self.e.execucao, &motivo)
            .map_err(do_banco)?;
        self.anotar("pausa", &motivo);
        self.e.navegador.trazer_para_frente();
        match self.e.pausa.aguardar(self.e.execucao, &motivo) {
            Decisao::Continuar => {
                self.e
                    .banco
                    .marcar_rodando_escrita(self.e.execucao)
                    .map_err(do_banco)?;
                self.anotar("retomada", "o usuário continuou");
                Ok(())
            }
            Decisao::Cancelar => {
                self.cancelado = true;
                Err(ErroDeVerbo::Abortado)
            }
            Decisao::Expirou => {
                self.expirou = true;
                Err(ErroDeVerbo::Abortado)
            }
        }
    }

    /// Depois de abrir ou clicar: a página pede CAPTCHA, confirmação de identidade ou MFA de
    /// token? Então espera o usuário. `true` se houve pausa. No MFA por e-mail o código vem do
    /// próprio roteiro (`preencher_codigo_do_email`), então a página de código não pausa.
    fn vigiar_pausa(&mut self) -> ResultadoDeVerbo<bool> {
        let Ok(p) = self.e.navegador.pagina() else {
            return Ok(false);
        };
        if self.liberada_em.as_deref() == Some(p.url.as_str()) {
            return Ok(false);
        }
        let Some(motivo) = p.pausa_necessaria() else {
            return Ok(false);
        };
        if motivo == crate::pagina::MotivoDePausa::Mfa
            && matches!(self.e.manifesto.mfa, Mfa::Email { .. })
        {
            return Ok(false);
        }
        let comeco = Instant::now();
        let r = self.pausar(motivo.frase());
        self.pausado = self.pausado.saturating_add(comeco.elapsed());
        r?;
        self.liberada_em = Some(p.url);
        Ok(true)
    }

    // ---------------------------------------------------------------- download

    fn nome_do_arquivo(caminho: &std::path::Path, nome: Option<&str>) -> String {
        match nome {
            Some(n) => {
                let ext = caminho.extension().and_then(|e| e.to_str()).unwrap_or("");
                let base = crate::executor::nome_de_arquivo_seguro(n);
                if ext.is_empty()
                    || base
                        .to_lowercase()
                        .ends_with(&format!(".{}", ext.to_lowercase()))
                {
                    base
                } else {
                    format!("{base}.{ext}")
                }
            }
            None => crate::executor::nome_de_arquivo_seguro(
                caminho
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("arquivo"),
            ),
        }
    }
}

impl Verbos for SbfVerbos<'_> {
    fn abrir(&mut self, url: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        let u = url.trim();
        if !self.url_permitida(u) {
            self.anotar("host", "abrir recusado: fora dos hosts do manifesto");
            return Err(ErroDeVerbo::HostNaoPermitido);
        }
        if let Some(b) = bloqueio::avaliar_url(u) {
            return Err(self.bloquear(b));
        }
        self.anotar("abrir", &resumo_da_url(u));
        self.e.navegador.fase_de_login(None);
        let real = self.real(u);
        if let Err(e) = self.e.navegador.navegar(&real) {
            return Err(self.do_navegador(&e));
        }
        self.conferir_barrado()?;
        self.conferir_pagina_atual(true)?;
        self.vigiar_pausa()?;
        Ok(())
    }

    fn esperar(&mut self, seletor: &str, prazo: Duration) -> ResultadoDeVerbo<bool> {
        self.entrar()?;
        if self.esperar_ate(seletor, prazo)? {
            return Ok(true);
        }
        // Não apareceu: talvez a página esteja pedindo CAPTCHA ou MFA. Se o usuário resolveu,
        // espera de novo uma vez.
        if self.vigiar_pausa()? {
            return self.esperar_ate(seletor, prazo);
        }
        Ok(false)
    }

    fn existe(&mut self, seletor: &str) -> ResultadoDeVerbo<bool> {
        self.entrar()?;
        match self.e.navegador.visivel(seletor) {
            Ok(v) => Ok(v),
            Err(e) => Err(self.do_navegador(&e)),
        }
    }

    fn clicar(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        self.exigir_visivel(seletor)?;
        self.conferir_clique(seletor)?;
        self.anotar("clicar", &resumo_do_seletor(seletor));
        self.ultimo_clique = Some(self.e.relogio.agora());
        let r = self.e.navegador.clicar(seletor);
        // O clique que envia o login acabou: a fase de login termina com ele.
        self.e.navegador.fase_de_login(None);
        self.conferir_barrado()?;
        if let Err(e) = r {
            return Err(self.do_navegador(&e));
        }
        self.conferir_pagina_atual(false)?;
        self.vigiar_pausa()?;
        Ok(())
    }

    fn texto(&mut self, seletor: &str) -> ResultadoDeVerbo<String> {
        self.entrar()?;
        let visivel = match self.e.navegador.visivel(seletor) {
            Ok(v) => v,
            Err(e) => return Err(self.do_navegador(&e)),
        };
        if !visivel {
            return Err(falhou(format!(
                "o elemento \"{}\" não está visível",
                resumo_do_seletor(seletor)
            )));
        }
        let cand = self.descrever(seletor)?;
        if cand.eh_campo || cand.eh_senha {
            return Err(falhou("texto não lê campo de formulário"));
        }
        match self.e.navegador.texto(seletor) {
            Ok(t) => Ok(self.limpo(&t)),
            Err(e) => Err(self.do_navegador(&e)),
        }
    }

    fn tabela(&mut self, seletor: &str) -> ResultadoDeVerbo<Vec<Vec<String>>> {
        self.entrar()?;
        self.exigir_visivel(seletor)?;
        let linhas = match self.e.navegador.ler_tabela(seletor) {
            Ok(l) => l,
            Err(e) => return Err(self.do_navegador(&e)),
        };
        Ok(linhas
            .into_iter()
            .map(|l| l.into_iter().map(|c| self.limpo(&c)).collect())
            .collect())
    }

    fn preencher(&mut self, seletor: &str, texto: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        self.exigir_visivel(seletor)?;
        let cand = self.descrever(seletor)?;
        if let Some(b) = bloqueio::avaliar(&cand) {
            return Err(self.bloquear(b));
        }
        if cand.eh_senha {
            return Err(falhou(
                "campo de senha só se preenche com preencher_senha (a senha do chaveiro)",
            ));
        }
        self.anotar("preencher", &resumo_do_seletor(seletor));
        match self.e.navegador.preencher(seletor, texto) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.do_navegador(&e)),
        }
    }

    fn preencher_senha(&mut self, seletor: &str, rotulo: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        let manifesto = self.e.manifesto;
        let Some(esperado) = manifesto.rotulo_senha.as_deref() else {
            return Err(falhou("o manifesto não declara `rotulo_senha`"));
        };
        if rotulo != esperado {
            return Err(falhou("o rótulo da senha não é o do manifesto"));
        }
        self.exigir_visivel(seletor)?;
        let host = self.conferir_host_do_segredo()?;
        self.conferir_campo_do_segredo(seletor, true)?;
        self.e.navegador.fase_de_login(Some(&host));
        let segredo = self
            .e
            .senhas
            .senha_de_site(rotulo)
            .map_err(|_| falhou("a senha do site não está no chaveiro"))?;
        // Entra na lista de limpeza ANTES de ir ao campo: se uma falha repetir o valor, some.
        self.usados.push(segredo);
        self.anotar("preencher_senha", &resumo_do_seletor(seletor));
        let permitida = self.url_aprovada();
        let Some(segredo) = self.usados.last() else {
            return Err(ErroDeVerbo::Abortado);
        };
        let r =
            self.e
                .navegador
                .preencher_segredo(seletor, segredo, TipoDeCampo::Senha, &permitida);
        r.map_err(|e| self.falha_de_digitacao(&e))
    }

    fn preencher_cpf(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        // O CPF é o único do chaveiro (`FonteDeSenhas::cpf`): não leva rótulo. Só vai ao campo
        // de texto de uma página do host aprovado, conferido de novo pelo motor ao digitar.
        self.exigir_visivel(seletor)?;
        let host = self.conferir_host_do_segredo()?;
        self.conferir_campo_do_segredo(seletor, false)?;
        self.e.navegador.fase_de_login(Some(&host));
        let segredo = self
            .e
            .senhas
            .cpf()
            .map_err(|_| falhou("o CPF não está no chaveiro"))?;
        self.usados.push(segredo);
        self.anotar("preencher_cpf", &resumo_do_seletor(seletor));
        let permitida = self.url_aprovada();
        let Some(segredo) = self.usados.last() else {
            return Err(ErroDeVerbo::Abortado);
        };
        let r =
            self.e
                .navegador
                .preencher_segredo(seletor, segredo, TipoDeCampo::Texto, &permitida);
        r.map_err(|e| self.falha_de_digitacao(&e))
    }

    fn preencher_codigo_do_email(
        &mut self,
        seletor: &str,
        janela: Duration,
    ) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        let manifesto = self.e.manifesto;
        let Mfa::Email { remetente, formato } = &manifesto.mfa else {
            return Err(falhou("o manifesto não declara MFA por e-mail"));
        };
        // Revalida em tempo de execução: o manifesto já foi validado ao importar, mas o código
        // do e-mail nunca vai a um roteiro que movimenta dinheiro.
        if manifesto.movimenta_dinheiro {
            return Err(ErroDeVerbo::Bloqueado {
                motivo: "MFA por e-mail só vale para roteiro que não movimenta dinheiro".into(),
            });
        }
        let busca = BuscaDeCodigo::nova(remetente, formato)
            .map_err(|_| falhou("o remetente ou o formato do código no manifesto é inválido"))?
            .com_janela(janela);
        let Some(caixa) = self.e.caixa else {
            return Err(falhou("a caixa de e-mail não está disponível"));
        };
        self.exigir_visivel(seletor)?;
        self.conferir_host_do_segredo()?;
        self.conferir_campo_do_segredo(seletor, false)?;
        // O código só vale se chegou depois do login: o instante do último clique, com folga.
        let pedido_em = self
            .ultimo_clique
            .unwrap_or(self.inicio)
            .checked_sub(FOLGA_DO_PEDIDO)
            .unwrap_or(UNIX_EPOCH);
        self.anotar("codigo_do_email", "esperando o código chegar");
        let relogio = RelogioCancelavel {
            dentro: self.e.relogio,
            cancelar: self.e.cancelar,
        };
        let r = esperar_codigo(caixa, &busca, pedido_em, &relogio);
        self.conferir_cancelado()?;
        let codigo = r.map_err(|e| match e {
            ErroDeCodigo::Expirou => ErroDeVerbo::Prazo(e.to_string()),
            ErroDeCodigo::Autorizacao => falhou(MENSAGEM_SEM_AUTORIZACAO_DO_GMAIL),
            outro => falhou(outro.to_string()),
        })?;
        // A espera pode ter levado minutos: o site pode ter redirecionado, trocado de página
        // ou de campo. Reconfere tudo ANTES de digitar (o motor ainda reconfere ao digitar).
        let host = self.conferir_host_do_segredo().inspect_err(|_| {
            self.anotar("host", "a página mudou durante a espera do código");
        })?;
        self.conferir_campo_do_segredo(seletor, false)?;
        self.e.navegador.fase_de_login(Some(&host));
        self.usados.push(codigo);
        let permitida = self.url_aprovada();
        let Some(codigo) = self.usados.last() else {
            return Err(ErroDeVerbo::Abortado);
        };
        let r = self
            .e
            .navegador
            .preencher_segredo(seletor, codigo, TipoDeCampo::Texto, &permitida);
        r.map_err(|e| self.falha_de_digitacao(&e))
    }

    fn baixar(&mut self, seletor: &str, nome: Option<&str>) -> ResultadoDeVerbo<String> {
        self.entrar()?;
        self.exigir_visivel(seletor)?;
        self.conferir_clique(seletor)?;
        std::fs::create_dir_all(&self.e.pasta)
            .map_err(|_| falhou("não consegui criar a pasta dos downloads"))?;
        self.anotar("baixar", &resumo_do_seletor(seletor));
        self.ultimo_clique = Some(self.e.relogio.agora());
        let r = self
            .e
            .navegador
            .clicar_e_baixar(seletor, &self.e.pasta, self.e.opcoes.download);
        self.e.navegador.fase_de_login(None);
        // O bloqueio do navegador vale mais que a falha que ele causou (nada baixou).
        self.conferir_barrado()?;
        let caminho = match r {
            Ok(c) => c,
            Err(e) => return Err(self.do_navegador(&e)),
        };
        let nome_final = Self::nome_do_arquivo(&caminho, nome);
        let recebido = match self.e.destino.receber(&caminho, &nome_final) {
            Ok(r) => r,
            Err(e) => return Err(falhou(self.limpo(&e.to_string()))),
        };
        // O arquivo já está no inventário (ou já estava): a cópia temporária não fica.
        let _ = std::fs::remove_file(&caminho);
        self.baixados += 1;
        if recebido == Recebido::Novo {
            self.novos += 1;
        }
        self.anotar("download", &nome_final);
        Ok(nome_final)
    }

    fn pausa(&mut self, motivo: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        self.pausar(motivo)
    }

    fn registrar(&mut self, mensagem: &str) -> ResultadoDeVerbo<()> {
        self.entrar()?;
        let m = self.limpo(mensagem);
        self.anotar("registro", &m);
        if self.registros.len() < MAXIMO_DE_REGISTROS {
            self.registros.push(m);
        }
        Ok(())
    }

    fn hoje(&mut self) -> ResultadoDeVerbo<String> {
        self.entrar()?;
        Ok(data_de_hoje(self.e.relogio.agora()))
    }

    fn somar_meses(&mut self, data: &str, meses: i32) -> ResultadoDeVerbo<String> {
        self.entrar()?;
        somar_meses(data, meses).map_err(falhou)
    }
}

/// O relógio que, depois de cancelado o roteiro, anda para o fim do prazo: a espera do código
/// termina na próxima volta.
struct RelogioCancelavel<'a> {
    dentro: &'a dyn Relogio,
    cancelar: &'a AtomicBool,
}

impl Relogio for RelogioCancelavel<'_> {
    fn agora(&self) -> SystemTime {
        let t = self.dentro.agora();
        if self.cancelar.load(Ordering::SeqCst) {
            t + Duration::from_secs(24 * 3600)
        } else {
            t
        }
    }

    fn esperar(&self, duracao: Duration) {
        self.dentro.esperar(duracao);
    }
}

// ------------------------------------------------------------------------------------------
// Datas (UTC−3), sem `Utc::now().date` puro
// ------------------------------------------------------------------------------------------

/// Dias desde 1970-01-01 para (ano, mês, dia) do calendário gregoriano (a contraprova de
/// [`data_dos_dias`], nos testes).
#[cfg(test)]
fn dias_da_data(a: i64, m: i64, d: i64) -> i64 {
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let ano_da_era = a.rem_euclid(400);
    let dia_do_ano = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
    era * 146_097 + dia_da_era - 719_468
}

/// (ano, mês, dia) a partir de dias desde 1970-01-01.
fn data_dos_dias(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let dia_da_era = z.rem_euclid(146_097);
    let ano_da_era =
        (dia_da_era - dia_da_era / 1_460 + dia_da_era / 36_524 - dia_da_era / 146_096) / 365;
    let dia_do_ano = dia_da_era - (365 * ano_da_era + ano_da_era / 4 - ano_da_era / 100);
    let mp = (5 * dia_do_ano + 2) / 153;
    let d = dia_do_ano - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = ano_da_era + era * 400 + i64::from(m <= 2);
    (a, m, d)
}

fn dias_do_mes(a: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (a % 4 == 0 && a % 100 != 0) || a % 400 == 0 => 29,
        _ => 28,
    }
}

/// A data de `agora` no fuso do usuário (UTC−3), `AAAA-MM-DD`. Às 22h de 24/11 em São Paulo o
/// UTC já é 25/11; o usuário ainda está no dia 24.
#[must_use]
pub fn data_de_hoje(agora: SystemTime) -> String {
    let segundos = match agora.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX / 2),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(0),
    };
    let (a, m, d) = data_dos_dias((segundos + FUSO_EM_SEGUNDOS).div_euclid(86_400));
    format!("{a:04}-{m:02}-{d:02}")
}

fn ler_data(texto: &str) -> Result<(i64, i64, i64), String> {
    let invalida = || "a data tem de ser AAAA-MM-DD (por exemplo 2026-11-24)".to_owned();
    let b = texto.as_bytes();
    let ok_forma = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if !ok_forma {
        return Err(invalida());
    }
    let n = |r: std::ops::Range<usize>| texto[r].parse::<i64>().map_err(|_| invalida());
    let (a, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    if a < 1 || !(1..=12).contains(&m) || d < 1 || d > dias_do_mes(a, m) {
        return Err(invalida());
    }
    Ok((a, m, d))
}

/// Soma `meses` (pode ser negativo) a uma data `AAAA-MM-DD`. O dia que não existe no mês de
/// chegada vira o último dia dele (31/01 + 1 mês = 28/02 ou 29/02).
fn somar_meses(texto: &str, meses: i32) -> Result<String, String> {
    let (a, m, d) = ler_data(texto)?;
    let total = a * 12 + (m - 1) + i64::from(meses);
    let (a2, m2) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if !(1..=9999).contains(&a2) {
        return Err("a data resultante está fora do intervalo (anos 1 a 9999)".to_owned());
    }
    let d2 = d.min(dias_do_mes(a2, m2));
    Ok(format!("{a2:04}-{m2:02}-{d2:02}"))
}

// ------------------------------------------------------------------------------------------
// Como o roteiro terminou
// ------------------------------------------------------------------------------------------

/// Traduz o resultado do host Luau em [`crate::executor::Fim`], com o que os verbos sabem
/// (o bloqueio que barrou, a pausa que expirou, o cancelamento). Nenhum texto sai com segredo.
#[must_use]
pub fn fim_do_roteiro(
    resultado: &Result<Saida, ErroDoLuau>,
    verbos: &SbfVerbos<'_>,
) -> crate::executor::Fim {
    use crate::executor::Fim;
    let Err(erro) = resultado else {
        return Fim::Concluida;
    };
    let linha = |l: Option<u32>| l.map_or(0, |n| usize::try_from(n).unwrap_or(0));
    let motivo = |t: &str| verbos.limpo(t);
    match erro {
        ErroDoLuau::Verbo { erro: v, linha: l } => match v {
            ErroDeVerbo::Abortado if verbos.pausa_expirou() => {
                Fim::PausaExpirou { passo: linha(*l) }
            }
            ErroDeVerbo::Abortado => Fim::Cancelada,
            ErroDeVerbo::Bloqueado { motivo: m } => match verbos.ultimo_bloqueio() {
                Some(b) => Fim::Bloqueada {
                    passo: linha(*l),
                    bloqueio: b.clone(),
                },
                None => Fim::Falhou {
                    passo: linha(*l),
                    motivo: motivo(m),
                },
            },
            ErroDeVerbo::HostNaoPermitido => Fim::Falhou {
                passo: linha(*l),
                motivo: "o roteiro tentou sair dos hosts do manifesto".to_owned(),
            },
            outro => Fim::Falhou {
                passo: linha(*l),
                motivo: motivo(&outro.to_string()),
            },
        },
        // O script engoliu o cancelamento com `pcall`: o sinal vale mais.
        _ if verbos.foi_cancelado() => Fim::Cancelada,
        outro => {
            let l = match outro {
                ErroDoLuau::Sintaxe { linha: l, .. } | ErroDoLuau::Execucao { linha: l, .. } => *l,
                _ => None,
            };
            Fim::Falhou {
                passo: linha(l),
                motivo: motivo(&outro.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    fn t(segundos: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(u64::try_from(segundos).unwrap_or(0))
    }

    #[test]
    fn as_22h_de_24_11_a_data_ainda_e_24_e_nao_25() {
        // 2026-11-25T01:00:00Z = 22:00 de 24/11 em UTC−3.
        let utc = dias_da_data(2026, 11, 25) * 86_400 + 3_600;
        assert_eq!(data_de_hoje(t(utc)), "2026-11-24");
        // 2026-11-25T03:00:00Z = 00:00 de 25/11 no fuso do usuário.
        let utc = dias_da_data(2026, 11, 25) * 86_400 + 3 * 3_600;
        assert_eq!(data_de_hoje(t(utc)), "2026-11-25");
    }

    #[test]
    fn ida_e_volta_do_calendario() {
        for (a, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 11, 24), (2100, 3, 1)] {
            assert_eq!(data_dos_dias(dias_da_data(a, m, d)), (a, m, d));
        }
        assert_eq!(dias_da_data(1970, 1, 1), 0);
    }

    #[test]
    fn somar_meses_acerta_virada_de_ano_dia_31_e_bissexto() {
        let soma = |d: &str, n: i32| somar_meses(d, n).unwrap_or_default();
        assert_eq!(soma("2026-11-24", 1), "2026-12-24");
        assert_eq!(soma("2026-11-24", 2), "2027-01-24");
        assert_eq!(soma("2026-01-24", -1), "2025-12-24");
        assert_eq!(soma("2026-01-31", 1), "2026-02-28");
        assert_eq!(soma("2024-01-31", 1), "2024-02-29");
        assert_eq!(soma("2026-03-31", -1), "2026-02-28");
        assert_eq!(soma("2026-11-24", 0), "2026-11-24");
        assert_eq!(soma("2026-11-24", -12), "2025-11-24");
    }

    #[test]
    fn data_mal_formada_ou_inexistente_e_erro_claro() {
        for ruim in [
            "",
            "2026-13-01",
            "2026-02-30",
            "26-11-24",
            "2026/11/24",
            "2026-11-2x",
        ] {
            assert!(somar_meses(ruim, 1).is_err(), "{ruim}");
        }
        assert!(somar_meses("9999-12-31", 1).is_err());
    }
}
