//! `sbf email`: as contas de e-mail (cada uma, uma Conexão do tipo E-mail). `sbf gmail ...` é o
//! mesmo comando (compatibilidade), e as ações sem `<id>` valem para a única conta.
//!
//! ```text
//! sbf email contas
//! sbf email adicionar --tipo gmail|outlook [--apelido <nome>]
//! sbf email app [--tipo gmail]            guarda o Client ID e o Client secret do aplicativo
//! sbf email entrar [<id>]                 login pelo navegador (OAuth), guarda no chaveiro
//! sbf email sair [<id>]                   apaga as credenciais da conta do chaveiro
//! sbf email remover <id>                  sai e tira a conta da lista
//! sbf email seguir <id> <rótulo>          passa a coletar este rótulo
//! sbf email deixar <id> <rótulo>
//! sbf email rotulos [<id>]                lista (e guarda) os rótulos da conta
//! sbf email coletar [<id>] --inventario <cofre> [--consulta ..] [--desde AAAA-MM-DD] [--limite N]
//! sbf email estado
//! sbf email importar                      cria a conta `gmail` com GOOGLE_MAIL_*/GMAIL_REFRESH_TOKEN
//! ```
//!
//! Segredo nunca vai para argv, log ou saída: o `entrar` imprime só o e-mail da conta; o
//! `app` pergunta o Client ID e o Client secret no terminal (o segredo sem eco).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::Chaveiro;
use sbf_gmail::config::{
    ARQUIVO_DA_CONFIG, Config, Conta, arquivo_dos_rotulos, ler_rotulos_da_conta,
};
use sbf_gmail::credenciais;
use sbf_gmail::{ClienteReal, Erro, FonteDeGmail as _, SituacaoDaConexao, TipoDeEmail};
use zeroize::Zeroizing;

use crate::gmail::{
    coletar_contas, contas, ler_opcoes_da_coleta, pasta_de_dados, resumo_em_linhas,
};

const USO: &str = "uso: sbf email contas | adicionar --tipo gmail|outlook [--apelido <nome>] | app [--tipo gmail] | entrar [<id>] | sair [<id>] | remover <id> | seguir <id> <rótulo> | deixar <id> <rótulo> | rotulos [<id>] | coletar [<id>] --inventario <cofre> [--consulta <q>] [--desde AAAA-MM-DD] [--limite N] | estado | importar";

/// Por que o estado está assim, em uma frase para o usuário.
#[must_use]
pub fn frase_do_estado(s: SituacaoDaConexao, email: Option<&str>) -> String {
    match s {
        SituacaoDaConexao::EmBreve => "em breve".to_owned(),
        SituacaoDaConexao::NaoConectada => "não conectado: rode `sbf email entrar`".to_owned(),
        SituacaoDaConexao::AutorizacaoVencida => {
            "a autorização venceu: rode `sbf email entrar` ou use Conexões › E-mail".to_owned()
        }
        SituacaoDaConexao::Pronta => format!("conectado como {}", email.unwrap_or("(conta)")),
        SituacaoDaConexao::Coletada => {
            format!("conectado como {}; coletado", email.unwrap_or("(conta)"))
        }
        SituacaoDaConexao::ComFalha => format!(
            "conectado como {}; a última coleta falhou",
            email.unwrap_or("(conta)")
        ),
    }
}

fn arquivo_da_config() -> Result<PathBuf, String> {
    pasta_de_dados()
        .map(|d| d.join(ARQUIVO_DA_CONFIG))
        .ok_or_else(|| "não achei a pasta de dados do app".to_owned())
}

fn abrir_chaveiro() -> Result<Chaveiro, String> {
    Chaveiro::abrir_na_pasta_padrao().map_err(|e| format!("chaveiro: {e}"))
}

/// A conta pedida, ou a única (ou a legada) quando não se diz qual.
fn escolher<'a>(config: &'a Config, id: Option<&str>) -> Result<&'a Conta, String> {
    match id {
        Some(i) => config
            .conta(i)
            .ok_or_else(|| format!("não existe a conta {i} (veja `sbf email contas`)")),
        None => match config.contas.as_slice() {
            [so] => Ok(so),
            [] => {
                Err("nenhuma conta de e-mail (rode `sbf email adicionar --tipo gmail`)".to_owned())
            }
            _ => Err("há mais de uma conta: diga qual (`sbf email contas`)".to_owned()),
        },
    }
}

fn gravar(config: &Config) -> Result<(), String> {
    config
        .salvar(&arquivo_da_config()?)
        .map_err(|e| e.to_string())
}

/// Lê uma linha do terminal (na saída de erro, para não sujar a saída).
fn ler_linha(pergunta: &str) -> Result<Zeroizing<String>, String> {
    eprint!("{pergunta}");
    let _ = std::io::stderr().flush();
    let mut linha = Zeroizing::new(String::new());
    std::io::stdin()
        .read_line(&mut linha)
        .map_err(|e| e.kind().to_string())?;
    Ok(Zeroizing::new(linha.trim().to_owned()))
}

fn eco(ligado: bool) {
    let _ = std::process::Command::new("stty")
        .arg(if ligado { "echo" } else { "-echo" })
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::null())
        .status();
}

fn valor<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn tipo_pedido(args: &[String]) -> Result<TipoDeEmail, String> {
    match valor(args, "--tipo") {
        None => Ok(TipoDeEmail::Gmail),
        Some(t) => TipoDeEmail::de_texto(t).ok_or_else(|| format!("tipo desconhecido: {t}")),
    }
}

/// Os argumentos sem as flags (e sem os valores delas).
fn posicionais(args: &[String]) -> Vec<&str> {
    let mut v = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a.starts_with("--") {
            it.next();
        } else {
            v.push(a.as_str());
        }
    }
    v
}

fn cmd_contas() -> Result<(), String> {
    let chaveiro = abrir_chaveiro().ok();
    let dados = pasta_de_dados();
    let config = contas(dados.as_deref(), chaveiro.as_ref())?;
    if config.contas.is_empty() {
        println!("nenhuma conta de e-mail. Rode `sbf email adicionar --tipo gmail`.");
    }
    for c in &config.contas {
        let e = sbf_gmail::estado_da_conta(None, c, chaveiro.as_ref());
        println!(
            "{}\t{}\t{}\t{}",
            c.id,
            c.tipo.rotulo(),
            c.conta.as_deref().unwrap_or("-"),
            frase_do_estado(e.situacao, c.conta.as_deref())
        );
    }
    Ok(())
}

fn cmd_adicionar(args: &[String]) -> Result<(), String> {
    let tipo = tipo_pedido(args)?;
    let mut config = Config::carregar(&arquivo_da_config()?).map_err(|e| e.to_string())?;
    let id = config
        .adicionar(tipo, valor(args, "--apelido").unwrap_or(""))
        .map_err(|e| e.to_string())?;
    gravar(&config)?;
    println!("conta {id} adicionada. Próximo passo: `sbf email entrar {id}`.");
    Ok(())
}

fn cmd_app(args: &[String]) -> Result<(), String> {
    let tipo = tipo_pedido(args)?;
    if !tipo.disponivel() {
        return Err(format!("{} ainda está em breve", tipo.rotulo()));
    }
    let chaveiro = abrir_chaveiro()?;
    eprintln!(
        "Client ID e Client secret do aplicativo {} (Google Cloud Console › APIs e serviços › Credenciais).",
        tipo.rotulo()
    );
    let id = ler_linha("Client ID: ")?;
    eco(false);
    let segredo = ler_linha("Client secret (não aparece): ");
    eco(true);
    eprintln!();
    credenciais::guardar_app(&chaveiro, tipo, Some(id.as_str()), Some(segredo?.as_str()))
        .map_err(|e| e.to_string())?;
    println!("aplicativo {} guardado no chaveiro.", tipo.rotulo());
    Ok(())
}

fn cmd_entrar(id: Option<&str>) -> Result<(), String> {
    let chaveiro = abrir_chaveiro()?;
    let dados = pasta_de_dados();
    let config = contas(dados.as_deref(), Some(&chaveiro))?;
    let conta = escolher(&config, id)?.clone();
    let arquivo = arquivo_da_config()?;
    let email = sbf_gmail::oauth::entrar_no_navegador(&chaveiro, &arquivo, &conta.id, &|url| {
        eprintln!(
            "Abrindo o navegador para autorizar a leitura do {}. Se ele não abrir, acesse:\n{url}",
            conta.tipo.rotulo()
        );
    },
    // No terminal, Ctrl+C encerra o processo (e a porta com ele).
    std::sync::Arc::default(),
    )
    .map_err(|e| match e {
        Erro::SemCredenciais(_) => format!(
            "faltam o Client ID e o Client secret do aplicativo: rode `sbf email app --tipo {}` (ou preencha em Conexões › E-mail).",
            conta.tipo.chave()
        ),
        outro => outro.to_string(),
    })?;
    println!("{} conectado: {email}", conta.tipo.rotulo());
    Ok(())
}

fn cmd_sair(id: Option<&str>, remover: bool) -> Result<(), String> {
    let chaveiro = abrir_chaveiro()?;
    let dados = pasta_de_dados();
    let mut config = contas(dados.as_deref(), Some(&chaveiro))?;
    let id = escolher(&config, id)?.id.clone();
    credenciais::apagar(&chaveiro, &id).map_err(|e| e.to_string())?;
    if remover {
        config.remover(&id);
    } else if let Some(c) = config.conta_mut(&id) {
        c.conta = None;
    }
    gravar(&config)?;
    println!(
        "{}",
        if remover {
            format!("conta {id} removida (o que já entrou no inventário continua lá).")
        } else {
            format!("conta {id} desconectada: credenciais apagadas do chaveiro.")
        }
    );
    Ok(())
}

fn cmd_seguir(args: &[String], seguir: bool) -> Result<(), String> {
    let pos = posicionais(args);
    let (Some(id), Some(rotulo)) = (pos.first(), pos.get(1)) else {
        return Err(format!("faltam <id> e <rótulo>.\n{USO}"));
    };
    let dados = pasta_de_dados().ok_or("não achei a pasta de dados do app")?;
    let da_conta = ler_rotulos_da_conta(&dados.join(arquivo_dos_rotulos(id)));
    let chaveiro = abrir_chaveiro().ok();
    let mut config = contas(Some(&dados), chaveiro.as_ref())?;
    let conta = config
        .conta_mut(id)
        .ok_or_else(|| format!("não existe a conta {id}"))?;
    let mudou = if seguir {
        conta.seguir(rotulo, &da_conta)
    } else {
        conta.deixar(rotulo, &da_conta)
    };
    gravar(&config)?;
    println!(
        "{}",
        match (seguir, mudou) {
            (true, true) => format!("{id}: passa a coletar o rótulo {rotulo}."),
            (true, false) => format!("{id}: já coletava {rotulo}."),
            (false, true) => format!("{id}: deixa de coletar {rotulo}."),
            (false, false) => format!("{id}: não coletava {rotulo}."),
        }
    );
    Ok(())
}

fn cmd_rotulos(id: Option<&str>) -> Result<(), String> {
    let chaveiro = abrir_chaveiro().ok();
    let dados = pasta_de_dados().ok_or("não achei a pasta de dados do app")?;
    let config = contas(Some(&dados), chaveiro.as_ref())?;
    let conta = escolher(&config, id)?.clone();
    let alvo = conta.alvo();
    let rotulos = std::thread::scope(|e| {
        e.spawn(|| ClienteReal::resolver(chaveiro.as_ref(), alvo).and_then(|c| c.rotulos()))
            .join()
            .unwrap_or_else(|_| Err(Erro::Gmail("parou de repente".to_owned())))
    })
    .map_err(|e| e.to_string())?;
    let _ = sbf_gmail::config::guardar_rotulos_da_conta(
        &dados.join(arquivo_dos_rotulos(&conta.id)),
        &rotulos,
    );
    let nomes = ler_rotulos_da_conta(&dados.join(arquivo_dos_rotulos(&conta.id)));
    let seguidos = conta.seguidos_para_mostrar(&nomes);
    for n in &nomes {
        println!(
            "{} {n}",
            if seguidos.iter().any(|s| s.eq_ignore_ascii_case(n)) {
                "*"
            } else {
                " "
            }
        );
    }
    Ok(())
}

fn cmd_coletar(args: &[String]) -> ExitCode {
    let pos = posicionais(args);
    let so = pos.first().copied();
    let flags: Vec<String> = {
        // `coletar <id> --inventario ...`: tira o id antes de ler as opções.
        let mut v = Vec::new();
        let mut it = args.iter().peekable();
        while let Some(a) = it.next() {
            if a.starts_with("--") {
                v.push(a.clone());
                if let Some(x) = it.next() {
                    v.push(x.clone());
                }
            }
        }
        v
    };
    let o = match ler_opcoes_da_coleta(&flags) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sbf email: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    let Some(cofre) = o.cofre else {
        eprintln!("sbf email: falta --inventario.\n{USO}");
        return ExitCode::from(2);
    };
    let resultados = coletar_contas(&cofre, so, &o.coleta, &AtomicBool::new(false));
    let mut saida = ExitCode::SUCCESS;
    for (id, r) in &resultados {
        match r {
            Ok(r) => {
                println!("{id}: {}", resumo_em_linhas(r));
                if r.falhas > 0 {
                    saida = ExitCode::from(1);
                }
            }
            Err(Erro::Autorizacao) => {
                eprintln!(
                    "{id}: a autorização venceu: rode `sbf email entrar {id}` ou use Conexões › E-mail."
                );
                saida = ExitCode::from(1);
            }
            Err(Erro::SemCredenciais(nomes)) => {
                eprintln!(
                    "{id}: faltam credenciais ({nomes}). Rode `sbf email app` e `sbf email entrar {id}`."
                );
                saida = ExitCode::from(2);
            }
            Err(e) => {
                eprintln!("{id}: {e}");
                saida = ExitCode::from(1);
            }
        }
    }
    if resultados.is_empty() {
        eprintln!("sbf email: nenhuma conta conectada. Rode `sbf email entrar`.");
        return ExitCode::from(2);
    }
    saida
}

fn cmd_estado(cofre: Option<&Path>) -> Result<(), String> {
    let chaveiro = abrir_chaveiro().ok();
    let dados = pasta_de_dados();
    let config = contas(dados.as_deref(), chaveiro.as_ref())?;
    if config.contas.is_empty() {
        println!("nenhuma conta de e-mail.");
    }
    for c in &config.contas {
        let e = sbf_gmail::estado_da_conta(cofre, c, chaveiro.as_ref());
        println!(
            "{}: {} ({} origens, {} documentos; última coleta {})",
            c.id,
            frase_do_estado(e.situacao, e.email.as_deref()),
            e.origens,
            e.documentos,
            e.ultima_coleta.as_deref().unwrap_or("nunca"),
        );
    }
    Ok(())
}

fn cmd_importar() -> Result<(), String> {
    let chaveiro = abrir_chaveiro()?;
    credenciais::importar(
        &|n| std::env::var(n).ok(),
        &credenciais::raizes_padrao(),
        &chaveiro,
    )
    .map_err(|e| match e {
        Erro::SemCredenciais(nomes) => format!(
            "o ambiente e o .env não têm as três credenciais (faltam: {nomes}); nada foi copiado."
        ),
        outro => outro.to_string(),
    })?;
    let arquivo = arquivo_da_config()?;
    let mut config = Config::carregar(&arquivo).map_err(|e| e.to_string())?;
    if config.conta("gmail").is_none() {
        config.contas.push(Conta::legada());
        config.salvar(&arquivo).map_err(|e| e.to_string())?;
    }
    println!("conta gmail criada a partir do ambiente/.env; credenciais copiadas para o chaveiro.");
    Ok(())
}

/// O ponto de entrada de `sbf email` e de `sbf gmail`.
pub fn rodar(args: &[String]) -> ExitCode {
    let Some(sub) = args.first().map(String::as_str) else {
        eprintln!("sbf email: {USO}");
        return ExitCode::from(2);
    };
    let resto = &args[1..];
    let id = posicionais(resto).first().copied();
    let r = match sub {
        "coletar" => return cmd_coletar(resto),
        "contas" => cmd_contas(),
        "adicionar" => cmd_adicionar(resto),
        "app" => cmd_app(resto),
        "entrar" => cmd_entrar(id),
        "sair" => cmd_sair(id, false),
        "remover" => match id {
            Some(_) => cmd_sair(id, true),
            None => Err(format!("falta o <id>.\n{USO}")),
        },
        "seguir" => cmd_seguir(resto, true),
        "deixar" => cmd_seguir(resto, false),
        "rotulos" => cmd_rotulos(id),
        "estado" => cmd_estado(valor(resto, "--inventario").map(Path::new)),
        "importar" => cmd_importar(),
        _ => Err(USO.to_owned()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf email: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn posicionais_pulam_as_flags_e_seus_valores() {
        let args = a(&["gmail-2", "--inventario", "/cofre", "Comprovantes"]);
        assert_eq!(posicionais(&args), ["gmail-2", "Comprovantes"]);
        assert_eq!(valor(&args, "--inventario"), Some("/cofre"));
    }

    #[test]
    fn tipo_padrao_e_gmail_e_o_desconhecido_recusa() {
        assert_eq!(tipo_pedido(&a(&[])).unwrap(), TipoDeEmail::Gmail);
        assert_eq!(
            tipo_pedido(&a(&["--tipo", "Outlook"])).unwrap(),
            TipoDeEmail::Outlook
        );
        assert!(tipo_pedido(&a(&["--tipo", "yahoo"])).is_err());
    }

    #[test]
    fn escolher_exige_o_id_quando_ha_varias() {
        let mut c = Config::default();
        assert!(escolher(&c, None).is_err());
        c.adicionar(TipoDeEmail::Gmail, "").unwrap();
        assert_eq!(escolher(&c, None).unwrap().id, "gmail");
        c.adicionar(TipoDeEmail::Gmail, "").unwrap();
        assert!(escolher(&c, None).is_err());
        assert_eq!(escolher(&c, Some("gmail-2")).unwrap().id, "gmail-2");
        assert!(escolher(&c, Some("x")).is_err());
    }

    #[test]
    fn as_frases_do_estado_nao_mostram_variaveis() {
        let f = frase_do_estado(SituacaoDaConexao::AutorizacaoVencida, None);
        assert!(f.contains("autorização venceu") && !f.contains("GOOGLE"));
        assert_eq!(
            frase_do_estado(SituacaoDaConexao::Pronta, Some("a@exemplo.test")),
            "conectado como a@exemplo.test"
        );
    }
}
