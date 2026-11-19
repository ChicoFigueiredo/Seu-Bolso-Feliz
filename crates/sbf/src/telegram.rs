//! `sbf telegram`: a conta do Telegram do usuário como fonte do inventário (só leitura,
//! ADR-017), mais a cola com o worker, o MCP e a janela.
//!
//! ```text
//! sbf telegram entrar                          login: número, código (e senha 2FA); o DONO roda
//! sbf telegram sair                            encerra a sessão e apaga a guardada
//! sbf telegram conversas                       lista as conversas (id, tipo, nome, se é seguida)
//! sbf telegram seguir <id|nome>                passa a coletar a conversa
//! sbf telegram deixar <id|nome>                deixa de coletar
//! sbf telegram coletar --inventario <cofre> [--desde AAAA-MM-DD] [--conversa <id|nome>]
//! sbf telegram estado                          o que a janela mostra
//! ```
//!
//! A sessão fica no chaveiro (rótulo `mensageiro:telegram`); `TELEGRAM_API_ID` e
//! `TELEGRAM_API_HASH` vêm do ambiente ou do `.env` da raiz do repositório (e, depois do
//! primeiro `entrar`, do chaveiro). Nada disso, nem número, nem código, nem senha, vai para log
//! ou mensagem de erro.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::Chaveiro;
use sbf_mcp::telegram::PonteDoTelegram;
use sbf_reconhecimento::{Reconhecedor as _, ReconhecedorDoCofre};
use sbf_telegram::cliente::{ClienteReal, Entrou, Perguntas};
use sbf_telegram::coleta::{Opcoes, Reconhecer, Relatorio, coletar};
use sbf_telegram::config::{
    ARQUIVO_DA_CONFIG, ARQUIVO_DAS_CONVERSAS, Config, ListaDeConversas, resolver,
};
use sbf_telegram::credenciais::Credenciais;
use sbf_telegram::datas::desde_iso;
use sbf_telegram::estado::{ARQUIVO_DO_ESTADO, Estado};
use sbf_telegram::modelo::Conversa;
use sbf_telegram::numero::{mascarar_numero, normalizar_numero};
use sbf_telegram::painel::Painel;
use sbf_telegram::{Erro, FonteTelegram as _, Resultado, sessao};
use sbf_worker::laco::{ColetorDeTelegram, Dados, ResumoDoTelegram};
use sbf_worker::registro::Falha;
use serde_json::{Value, json};
use zeroize::Zeroizing;

const USO: &str = "uso: sbf telegram entrar | sair | conversas | seguir <id|nome> | deixar <id|nome> | coletar --inventario <pasta-do-cofre> [--desde AAAA-MM-DD] [--conversa <id|nome>] | estado";

// ---------------------------------------------------------------------------------------
// Onde as coisas ficam
// ---------------------------------------------------------------------------------------

/// A pasta de dados do app, fora do cofre (se o cofre é conhecido, confere).
fn pasta_de_dados(cofre: Option<&Path>) -> Result<PathBuf, String> {
    let pasta = sbf_chaveiro::pasta_de_dados_do_app().map_err(|e| e.to_string())?;
    if let Some(c) = cofre {
        sbf_chaveiro::garantir_fora_do_cofre(&pasta, c).map_err(|e| e.to_string())?;
    }
    Ok(pasta)
}

fn chaveiro() -> Resultado<Chaveiro> {
    Chaveiro::abrir_na_pasta_padrao().map_err(|e| Erro::Chaveiro(e.to_string()))
}

/// Abre o cliente com a sessão guardada. Sem sessão guardada: precisa entrar (sem rede).
fn cliente_com_sessao(chaveiro: &Chaveiro) -> Resultado<(ClienteReal, String)> {
    let Some(sessao) = sessao::ler(chaveiro)? else {
        return Err(Erro::PrecisaEntrar);
    };
    let antes = sessao.exportar()?;
    let credenciais = Credenciais::do_ambiente(Some(chaveiro))?;
    Ok((ClienteReal::novo(&credenciais, Some(sessao))?, antes))
}

/// Guarda a sessão de volta se o Telegram mexeu nela (chave nova de datacenter).
fn guardar_se_mudou(chaveiro: &Chaveiro, cliente: &ClienteReal, antes: &str) {
    if let Ok(depois) = cliente.sessao().exportar()
        && depois != antes
    {
        let _ = sessao::guardar(chaveiro, cliente.sessao());
    }
}

fn agora_iso() -> String {
    sbf_coleta::pastas::iso_de(std::time::SystemTime::now())
}

// ---------------------------------------------------------------------------------------
// O reconhecimento de cada arquivo novo
// ---------------------------------------------------------------------------------------

/// O `ReconhecedorDoCofre` (com as regras de reconhecimento) como [`Reconhecer`]. Sem IA: o
/// que o leitor local e as regras não leem fica para a triagem.
struct ReconhecedorDoSbf {
    r: ReconhecedorDoCofre,
    cancelar: AtomicBool,
}

impl Reconhecer for ReconhecedorDoSbf {
    fn reconhecer(&mut self, caminho: &Path) -> Result<(), String> {
        self.r
            .reconhecer(caminho, &mut |_| {}, &self.cancelar)
            .map(|_| ())
            .map_err(|_| "o reconhecimento não leu o arquivo".to_owned())
    }
}

// ---------------------------------------------------------------------------------------
// As operações (a CLI, o worker, o MCP e a janela chamam estas)
// ---------------------------------------------------------------------------------------

/// Coleta já as conversas seguidas (ou só `opcoes.so_conversa`). Sem conversa seguida, nem
/// abre sessão. Grava o estado.
///
/// # Errors
/// Sem sessão, sem credenciais, banco, disco ou rede.
pub fn coletar_agora(
    cofre: &Path,
    chaveiro: Option<Chaveiro>,
    opcoes: &Opcoes,
    cancelar: &AtomicBool,
) -> Resultado<Relatorio> {
    let pasta = pasta_de_dados(Some(cofre)).map_err(Erro::Formato)?;
    let config = Config::carregar(&pasta.join(ARQUIVO_DA_CONFIG))?;
    if config.seguidas.is_empty() {
        return Ok(Relatorio::default());
    }
    let chaveiro = match chaveiro {
        Some(c) => c,
        None => self::chaveiro()?,
    };
    let (mut cliente, antes) = cliente_com_sessao(&chaveiro)?;
    let conexao = sbf_worker::banco::abrir(&cofre.join("inventario.sqlite"))
        .map_err(|e| Erro::Formato(e.to_string()))?;
    let segredos: sbf_reconhecimento::Segredos = Arc::new(chaveiro.clone());
    let mut reconhecedor = ReconhecedorDoSbf {
        r: ReconhecedorDoCofre::novo(cofre, segredos, None)
            .com_regras(crate::script::executor_de_script().ok()),
        cancelar: AtomicBool::new(false),
    };
    let arquivo_do_estado = pasta.join(ARQUIVO_DO_ESTADO);
    let mut estado = Estado::carregar(&arquivo_do_estado);
    let r = coletar(
        &mut cliente,
        &conexao,
        cofre,
        &config,
        &mut estado,
        opcoes,
        &mut reconhecedor,
        cancelar,
    );
    // O estado e a sessão voltam para o disco mesmo que a coleta tenha parado no meio.
    let salvo = estado.salvar(&arquivo_do_estado);
    guardar_se_mudou(&chaveiro, &cliente, &antes);
    let rel = r?;
    salvo?;
    Ok(rel)
}

/// Lista as conversas perguntando ao Telegram e guarda a lista para a janela.
fn atualizar_conversas(pasta: &Path, chaveiro: &Chaveiro) -> Resultado<Vec<Conversa>> {
    let (mut cliente, antes) = cliente_com_sessao(chaveiro)?;
    let r = cliente.listar_conversas();
    guardar_se_mudou(chaveiro, &cliente, &antes);
    let conversas = r?;
    ListaDeConversas {
        atualizada_em: Some(agora_iso()),
        conversas: conversas.clone(),
    }
    .salvar(&pasta.join(ARQUIVO_DAS_CONVERSAS))?;
    Ok(conversas)
}

/// As conversas conhecidas: a lista guardada, ou a do Telegram se `atualizar` ou se não há.
fn conversas_conhecidas(
    pasta: &Path,
    chaveiro: Option<&Chaveiro>,
    atualizar: bool,
) -> Resultado<Vec<Conversa>> {
    let guardada = ListaDeConversas::carregar(&pasta.join(ARQUIVO_DAS_CONVERSAS)).conversas;
    if !atualizar && !guardada.is_empty() {
        return Ok(guardada);
    }
    match chaveiro {
        Some(c) => atualizar_conversas(pasta, c),
        None if !guardada.is_empty() => Ok(guardada),
        None => Err(Erro::Chaveiro("chaveiro fechado".into())),
    }
}

/// Segue ou deixa a conversa (id ou nome). Resolve pela lista guardada e, se não achar, pela
/// do Telegram.
fn mudar_seguida(
    pasta: &Path,
    chaveiro: Option<&Chaveiro>,
    pedido: &str,
    seguir: bool,
) -> Resultado<(Conversa, bool)> {
    let arquivo = pasta.join(ARQUIVO_DA_CONFIG);
    let mut config = Config::carregar(&arquivo)?;
    // Deixar uma já seguida não precisa de rede: resolve entre as seguidas.
    let achada = if seguir {
        None
    } else {
        resolver(&config.seguidas, pedido).ok().cloned()
    };
    let conversa = match achada {
        Some(c) => c,
        None => {
            let guardadas = conversas_conhecidas(pasta, chaveiro, false)?;
            match resolver(&guardadas, pedido) {
                Ok(c) => c.clone(),
                Err(Erro::ConversaDesconhecida) if chaveiro.is_some() => {
                    let novas = conversas_conhecidas(pasta, chaveiro, true)?;
                    resolver(&novas, pedido)?.clone()
                }
                Err(e) => return Err(e),
            }
        }
    };
    let mudou = if seguir {
        config.seguir(&conversa)
    } else {
        config.deixar(&conversa.id)
    };
    if mudou {
        config.salvar(&arquivo)?;
    }
    Ok((conversa, mudou))
}

fn json_da_conversa(c: &Conversa, seguida: bool) -> Value {
    json!({ "id": c.id, "nome": c.nome, "tipo": c.tipo.rotulo(), "seguida": seguida })
}

fn json_do_relatorio(r: &Relatorio) -> Value {
    json!({
        "conversas": r.conversas,
        "novos": r.novos,
        "repetidos": r.repetidos,
        "reconhecidos": r.reconhecidos,
        "falhas_de_reconhecimento": r.falhas_de_reconhecimento,
        "erros": r.erros,
        "interrompida": r.interrompido,
        "precisa_entrar": r.precisa_entrar,
    })
}

/// O texto para a janela e o terminal, sem conteúdo de conversa.
#[must_use]
pub fn texto_do_relatorio(r: &Relatorio) -> String {
    if r.precisa_entrar {
        return "Precisa entrar: rode `sbf telegram entrar` no terminal.".to_owned();
    }
    let mut t = format!(
        "{} conversa(s), {} arquivo(s) novo(s), {} já conhecido(s)",
        r.conversas, r.novos, r.repetidos
    );
    if r.falhas_de_reconhecimento > 0 {
        t.push_str(&format!(
            ", {} sem leitura do reconhecimento",
            r.falhas_de_reconhecimento
        ));
    }
    if r.erros > 0 {
        t.push_str(&format!(", {} conversa(s) com falha", r.erros));
    }
    t.push('.');
    t
}

// ---------------------------------------------------------------------------------------
// O MCP
// ---------------------------------------------------------------------------------------

/// A ponte do MCP (`telegram_conversas`, `telegram_seguir`, `telegram_coletar`).
#[derive(Debug, Clone)]
pub struct PonteReal {
    /// A pasta do cofre.
    pub cofre: PathBuf,
}

fn texto_do_erro(e: &Erro) -> String {
    e.to_string()
}

impl PonteDoTelegram for PonteReal {
    fn conversas(&self, atualizar: bool) -> Result<Value, String> {
        let pasta = pasta_de_dados(Some(&self.cofre))?;
        let ch = if atualizar
            || ListaDeConversas::carregar(&pasta.join(ARQUIVO_DAS_CONVERSAS))
                .conversas
                .is_empty()
        {
            Some(chaveiro().map_err(|e| texto_do_erro(&e))?)
        } else {
            chaveiro().ok()
        };
        let conversas =
            conversas_conhecidas(&pasta, ch.as_ref(), atualizar).map_err(|e| texto_do_erro(&e))?;
        let config =
            Config::carregar(&pasta.join(ARQUIVO_DA_CONFIG)).map_err(|e| texto_do_erro(&e))?;
        Ok(json!({
            "conversas": conversas
                .iter()
                .map(|c| json_da_conversa(c, config.segue(&c.id)))
                .collect::<Vec<_>>(),
        }))
    }

    fn seguir(&self, conversa: &str, seguir: bool) -> Result<Value, String> {
        let pasta = pasta_de_dados(Some(&self.cofre))?;
        let ch = chaveiro().ok();
        let (c, mudou) =
            mudar_seguida(&pasta, ch.as_ref(), conversa, seguir).map_err(|e| texto_do_erro(&e))?;
        Ok(json!({ "conversa": json_da_conversa(&c, seguir), "mudou": mudou }))
    }

    fn coletar(&self, conversa: Option<&str>, desde: Option<&str>) -> Result<Value, String> {
        let pasta = pasta_de_dados(Some(&self.cofre))?;
        let so_conversa = match conversa {
            Some(c) => {
                let config = Config::carregar(&pasta.join(ARQUIVO_DA_CONFIG))
                    .map_err(|e| texto_do_erro(&e))?;
                Some(
                    resolver(&config.seguidas, c)
                        .map_err(|e| texto_do_erro(&e))?
                        .id
                        .clone(),
                )
            }
            None => None,
        };
        let opcoes = Opcoes {
            desde_data: desde.and_then(desde_iso),
            so_conversa,
        };
        let r = coletar_agora(&self.cofre, None, &opcoes, &AtomicBool::new(false))
            .map_err(|e| texto_do_erro(&e))?;
        Ok(json_do_relatorio(&r))
    }
}

// ---------------------------------------------------------------------------------------
// O worker
// ---------------------------------------------------------------------------------------

/// O coletor do worker: as conversas seguidas, de `telegram_minutos` em `telegram_minutos`.
/// Sem conversa seguida ou sem sessão guardada, não faz nada (ainda não foi ligado).
#[derive(Debug)]
pub struct TelegramReal {
    /// Onde o worker trabalha.
    pub dados: Dados,
}

impl ColetorDeTelegram for TelegramReal {
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoTelegram, Falha> {
        let Ok(config) = Config::carregar(&self.dados.pasta.join(ARQUIVO_DA_CONFIG)) else {
            return Err(Falha::Disco);
        };
        if config.seguidas.is_empty() {
            return Ok(ResumoDoTelegram::default());
        }
        // O worker não pede senha: sem chaveiro aberto ou sem sessão, só espera.
        let Ok(ch) = chaveiro() else {
            return Ok(ResumoDoTelegram::default());
        };
        if !sessao::existe_no_chaveiro(&ch) {
            return Ok(ResumoDoTelegram::default());
        }
        match coletar_agora(&self.dados.cofre, Some(ch), &Opcoes::default(), cancelar) {
            Ok(r) if r.precisa_entrar => Err(Falha::Externo),
            Ok(r) => {
                if r.erros > 0 && r.novos == 0 {
                    return Err(Falha::Externo);
                }
                Ok(ResumoDoTelegram {
                    conversas: r.conversas,
                    novos: usize::try_from(r.novos).unwrap_or(usize::MAX),
                })
            }
            Err(Erro::Banco(_)) => Err(Falha::Banco),
            Err(Erro::Io(_)) => Err(Falha::Disco),
            Err(_) => Err(Falha::Externo),
        }
    }
}

// ---------------------------------------------------------------------------------------
// A janela
// ---------------------------------------------------------------------------------------

/// "Coletar agora" da área Conexões: a mesma coleta do terminal e do worker.
#[derive(Debug, Clone)]
pub struct ColetaDaJanela {
    /// A pasta do cofre.
    pub cofre: PathBuf,
    /// O chaveiro que a janela já abriu, se abriu.
    pub chaveiro: Option<Chaveiro>,
}

impl sbf_janela::conexoes_ui::ColetaDoTelegram for ColetaDaJanela {
    fn coletar(&self) -> Result<String, String> {
        coletar_agora(
            &self.cofre,
            self.chaveiro.clone(),
            &Opcoes::default(),
            &AtomicBool::new(false),
        )
        .map(|r| texto_do_relatorio(&r))
        .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------------------
// O terminal
// ---------------------------------------------------------------------------------------

/// As perguntas do login no terminal. `enviado` guarda o número normalizado que foi pedido
/// ao Telegram, para confirmar (mascarado) depois que o código foi enviado.
#[derive(Default)]
struct NoTerminal {
    enviado: Option<String>,
}

fn ler_linha(pergunta: &str) -> Resultado<String> {
    eprint!("{pergunta}");
    let _ = std::io::stderr().flush();
    let mut linha = String::new();
    std::io::stdin().read_line(&mut linha)?;
    Ok(linha.trim().to_owned())
}

/// Liga ou desliga o eco do terminal (só Unix; sem `stty`, segue com eco).
fn eco(ligado: bool) {
    let _ = std::process::Command::new("stty")
        .arg(if ligado { "echo" } else { "-echo" })
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::null())
        .status();
}

impl Perguntas for NoTerminal {
    fn numero(&mut self) -> Resultado<String> {
        let digitado = ler_linha("Número do Telegram (DDD + número; o +55 é colocado sozinho): ")?;
        let normalizado = normalizar_numero(&digitado);
        self.enviado = Some(normalizado.clone());
        Ok(digitado)
    }

    fn codigo(&mut self) -> Resultado<Zeroizing<String>> {
        if let Some(n) = &self.enviado {
            eprintln!("Número enviado ao Telegram: {}", mascarar_numero(n));
        }
        eprintln!("O Telegram mandou um código para o app (não por SMS).");
        ler_linha("Código: ").map(Zeroizing::new)
    }

    fn senha(&mut self, dica: Option<&str>) -> Resultado<Zeroizing<String>> {
        if let Some(d) = dica {
            eprintln!("Dica da senha: {d}");
        }
        eco(false);
        let r = ler_linha("Senha da verificação em duas etapas (não aparece): ");
        eco(true);
        eprintln!();
        r.map(Zeroizing::new)
    }
}

fn entrar() -> Resultado<()> {
    let ch = chaveiro()?;
    let credenciais = Credenciais::do_ambiente(Some(&ch))?;
    let existente = sessao::ler(&ch)?;
    let mut cliente = ClienteReal::novo(&credenciais, existente)?;
    let entrou = cliente.entrar(&mut NoTerminal::default())?;
    sessao::guardar(&ch, cliente.sessao())?;
    // O worker, sem `.env`, acha as credenciais aqui.
    credenciais.guardar(&ch)?;
    match entrou {
        Entrou::JaEstava => println!("Já havia uma sessão válida (guardada de novo no chaveiro)."),
        Entrou::Agora => println!("Entrou. A sessão está guardada no chaveiro."),
    }
    Ok(())
}

fn sair() -> Resultado<()> {
    let ch = chaveiro()?;
    if let Some(s) = sessao::ler(&ch)?
        && let Ok(c) = Credenciais::do_ambiente(Some(&ch))
        && let Ok(mut cliente) = ClienteReal::novo(&c, Some(s))
    {
        // Se o Telegram não responder, ainda apagamos a nossa cópia.
        let _ = cliente.sair();
    }
    sessao::apagar(&ch)?;
    println!("Saiu. A sessão guardada foi apagada.");
    Ok(())
}

fn conversas() -> Resultado<()> {
    let ch = chaveiro()?;
    let pasta = pasta_de_dados(None).map_err(Erro::Formato)?;
    let lista = conversas_conhecidas(&pasta, Some(&ch), true)?;
    let config = Config::carregar(&pasta.join(ARQUIVO_DA_CONFIG))?;
    for c in &lista {
        println!(
            "{:>16}  {:<8} {}  {}",
            c.id,
            c.tipo.rotulo(),
            if config.segue(&c.id) {
                "[seguida]"
            } else {
                "         "
            },
            c.nome
        );
    }
    println!("{} conversa(s).", lista.len());
    Ok(())
}

fn estado() -> Resultado<()> {
    let pasta = pasta_de_dados(None).map_err(Erro::Formato)?;
    let ch = Chaveiro::abrir_na_pasta_padrao().ok();
    let p = Painel::ler(&pasta, ch.as_ref());
    println!("Telegram: {}", p.situacao.rotulo());
    println!(
        "Última coleta: {}; {} arquivo(s) no total.",
        p.ultima_coleta.as_deref().unwrap_or("nunca"),
        p.arquivos
    );
    if let Some(e) = &p.ultimo_erro {
        println!("Último problema: {e}");
    }
    for s in &p.seguidas {
        println!(
            "  seguida: {} ({}), {} arquivo(s)",
            s.conversa.nome, s.conversa.id, s.arquivos
        );
    }
    if p.seguidas.is_empty() {
        println!("Nenhuma conversa seguida (veja `sbf telegram conversas`).");
    }
    Ok(())
}

fn valor<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn seguir_ou_deixar(args: &[String], seguir: bool) -> Resultado<()> {
    let Some(pedido) = args.first().filter(|a| !a.starts_with("--")) else {
        return Err(Erro::Formato("diga o id ou o nome da conversa".into()));
    };
    let pasta = pasta_de_dados(None).map_err(Erro::Formato)?;
    let ch = chaveiro().ok();
    let (c, mudou) = mudar_seguida(&pasta, ch.as_ref(), pedido, seguir)?;
    let verbo = if seguir { "seguindo" } else { "deixada" };
    println!(
        "{verbo}: {} ({}){}",
        c.nome,
        c.id,
        if mudou { "" } else { " (já estava assim)" }
    );
    Ok(())
}

fn coletar_na_linha_de_comando(args: &[String]) -> Result<(), String> {
    let cofre = valor(args, "--inventario").ok_or("falta --inventario <pasta-do-cofre>")?;
    let desde = match valor(args, "--desde") {
        Some(d) => Some(desde_iso(d).ok_or("--desde deve ser AAAA-MM-DD")?),
        None => None,
    };
    let pasta = pasta_de_dados(Some(Path::new(cofre)))?;
    let so_conversa = match valor(args, "--conversa") {
        Some(c) => {
            let config =
                Config::carregar(&pasta.join(ARQUIVO_DA_CONFIG)).map_err(|e| e.to_string())?;
            Some(
                resolver(&config.seguidas, c)
                    .map_err(|e| e.to_string())?
                    .id
                    .clone(),
            )
        }
        None => None,
    };
    let parar = Arc::new(AtomicBool::new(false));
    {
        let p = Arc::clone(&parar);
        let _ = ctrlc::set_handler(move || p.store(true, std::sync::atomic::Ordering::SeqCst));
    }
    let r = coletar_agora(
        Path::new(cofre),
        None,
        &Opcoes {
            desde_data: desde,
            so_conversa,
        },
        &parar,
    )
    .map_err(|e| e.to_string())?;
    println!("{}", texto_do_relatorio(&r));
    if r.precisa_entrar {
        return Err("a sessão não vale mais".to_owned());
    }
    Ok(())
}

/// O ponto de entrada de `sbf telegram`.
pub fn rodar(args: &[String]) -> ExitCode {
    let resto = args.get(1..).unwrap_or(&[]);
    let r: Result<(), String> = match args.first().map(String::as_str) {
        Some("entrar") => entrar().map_err(|e| e.to_string()),
        Some("sair") => sair().map_err(|e| e.to_string()),
        Some("conversas") => conversas().map_err(|e| e.to_string()),
        Some("estado") => estado().map_err(|e| e.to_string()),
        Some("seguir") => seguir_ou_deixar(resto, true).map_err(|e| e.to_string()),
        Some("deixar") => seguir_ou_deixar(resto, false).map_err(|e| e.to_string()),
        Some("coletar") => coletar_na_linha_de_comando(resto),
        _ => {
            eprintln!("{USO}");
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf telegram: {e}");
            ExitCode::FAILURE
        }
    }
}
