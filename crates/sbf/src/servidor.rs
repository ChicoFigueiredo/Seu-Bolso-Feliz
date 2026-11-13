//! O modo `sbf servidor` (spec 07, ADR-013), na VPS atrás do Caddy:
//!
//! - `sbf servidor [--endereco 127.0.0.1:8787]`: a API do livro-caixa;
//! - `sbf servidor estacoes`: lista as estações de todos os donos (administração);
//! - `sbf servidor revogar <id-da-estacao>`: revoga uma estação, para quando a única
//!   estação foi roubada e não há outra para revogar pela área Sincronização.
//! - `sbf servidor emitir-credencial --email <e-mail> --nome <estação> [--sistema linux]`:
//!   pareamento administrativo, sem Google; imprime só a credencial no stdout.
//!
//! A URL do banco vem **só** de `SBF_DATABASE_URL` (nunca de argv, que aparece em `ps`),
//! e o client id do Google de `SBF_GOOGLE_CLIENT_ID`. O servidor escuta só em loopback:
//! o TLS e a porta pública são do Caddy (grill Q1).

use std::net::SocketAddr;
use std::process::ExitCode;

use zeroize::Zeroizing;

const ENDERECO_PADRAO: &str = "127.0.0.1:8787";
const AJUDA: &str = "uso: sbf servidor [--endereco 127.0.0.1:PORTA] | sbf servidor estacoes | sbf servidor revogar <id-da-estacao> | sbf servidor emitir-credencial --email <e-mail> --nome <estação> [--sistema linux]\n\
     variáveis: SBF_DATABASE_URL (obrigatória), SBF_GOOGLE_CLIENT_ID (obrigatória para servir)";

#[derive(Debug, PartialEq, Eq)]
enum Comando {
    Servir(SocketAddr),
    ListarEstacoes,
    Revogar(uuid::Uuid),
    Emitir {
        email: String,
        nome: String,
        sistema: String,
    },
}

fn ler_argumentos(args: &[String]) -> Result<Comando, String> {
    match args.first().map(String::as_str) {
        None => Ok(Comando::Servir(endereco(ENDERECO_PADRAO)?)),
        Some("--endereco") => {
            let valor = args.get(1).ok_or("falta o endereço depois de --endereco")?;
            if args.len() > 2 {
                return Err("argumentos a mais".into());
            }
            Ok(Comando::Servir(endereco(valor)?))
        }
        Some("estacoes") if args.len() == 1 => Ok(Comando::ListarEstacoes),
        Some("revogar") if args.len() == 2 => uuid::Uuid::try_parse(&args[1])
            .map(Comando::Revogar)
            .map_err(|_| "o id da estação não é um UUID".into()),
        Some("emitir-credencial") => ler_emitir(&args[1..]),
        Some(outro) => Err(format!("argumento desconhecido: {outro}")),
    }
}

fn ler_emitir(args: &[String]) -> Result<Comando, String> {
    let (mut email, mut nome, mut sistema) = (None, None, "linux".to_owned());
    let mut it = args.iter();
    while let Some(opcao) = it.next() {
        let destino = match opcao.as_str() {
            "--email" => &mut email,
            "--nome" => &mut nome,
            "--sistema" => {
                sistema = it.next().ok_or("falta o valor de --sistema")?.clone();
                continue;
            }
            outra => return Err(format!("argumento desconhecido: {outra}")),
        };
        *destino = Some(
            it.next()
                .ok_or_else(|| format!("falta o valor de {opcao}"))?
                .clone(),
        );
    }
    Ok(Comando::Emitir {
        email: email.ok_or("falta --email")?,
        nome: nome.ok_or("falta --nome")?,
        sistema,
    })
}

fn endereco(texto: &str) -> Result<SocketAddr, String> {
    let e: SocketAddr = texto
        .parse()
        .map_err(|_| format!("endereço inválido: {texto} (use IP:PORTA)"))?;
    if !e.ip().is_loopback() {
        return Err(format!(
            "{texto} não é loopback: o sbf servidor fala HTTP simples e só escuta em 127.0.0.1 ou ::1, atrás do Caddy"
        ));
    }
    Ok(e)
}

fn variavel(nome: &str) -> Result<Zeroizing<String>, String> {
    match std::env::var(nome) {
        Ok(v) if !v.trim().is_empty() => Ok(Zeroizing::new(v)),
        _ => Err(format!("a variável {nome} é obrigatória")),
    }
}

/// O braço `servidor` do `main`. `args` vem sem o próprio `servidor`.
pub fn rodar(args: &[String]) -> ExitCode {
    let comando = match ler_argumentos(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sbf servidor: {e}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    let url = match variavel("SBF_DATABASE_URL") {
        Ok(u) => u,
        Err(e) => {
            eprintln!("sbf servidor: {e}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sbf servidor: não deu para iniciar o runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let resultado = runtime.block_on(async move {
        match comando {
            Comando::Servir(endereco) => {
                let google_client_id = variavel("SBF_GOOGLE_CLIENT_ID")
                    .map_err(sbf_servidor::ErroDoServidor::Comando)?
                    .to_string();
                sbf_servidor::servir(sbf_servidor::Configuracao {
                    endereco,
                    url_do_banco: url,
                    google_client_id,
                    regras: env!("CARGO_PKG_VERSION").into(),
                })
                .await
            }
            Comando::Emitir {
                email,
                nome,
                sistema,
            } => emitir(&url, &email, &nome, &sistema).await,
            Comando::ListarEstacoes => administrar(&url, None).await,
            Comando::Revogar(id) => administrar(&url, Some(id)).await,
        }
    });
    match resultado {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf servidor: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn emitir(
    url: &Zeroizing<String>,
    email: &str,
    nome: &str,
    sistema: &str,
) -> Result<(), sbf_servidor::ErroDoServidor> {
    use sbf_servidor::ErroDoServidor::Comando as Erro;
    let banco = sbf_servidor::abrir_banco(url).await?;
    let mut cliente = banco.get().await.map_err(|e| Erro(e.to_string()))?;
    let (estacao, credencial) =
        sbf_servidor::estacoes::emitir_credencial(&mut cliente, email, nome, sistema)
            .await
            .map_err(|e| Erro(e.corpo.mensagem))?;
    eprintln!(
        "estação {} ({}) criada: a credencial abaixo não será mostrada de novo; guarde-a só no chaveiro (sbf estacao importar-credencial)",
        estacao.id, estacao.nome
    );
    println!("{}", credencial.as_str());
    Ok(())
}

async fn administrar(
    url: &Zeroizing<String>,
    revogar: Option<uuid::Uuid>,
) -> Result<(), sbf_servidor::ErroDoServidor> {
    use sbf_servidor::ErroDoServidor::Comando as Erro;
    let banco = sbf_servidor::abrir_banco(url).await?;
    let cliente = banco.get().await.map_err(|e| Erro(e.to_string()))?;
    match revogar {
        Some(id) => {
            let estacao = sbf_servidor::estacoes::revogar_estacao(&**cliente, None, id)
                .await
                .map_err(|e| Erro(e.corpo.mensagem))?
                .ok_or_else(|| Erro(format!("estação {id} não existe")))?;
            println!(
                "revogada: {} ({}, {})",
                estacao.id, estacao.nome, estacao.sistema
            );
        }
        None => {
            let estacoes = sbf_servidor::estacoes::listar_todas(&**cliente)
                .await
                .map_err(|e| Erro(e.corpo.mensagem))?;
            for e in estacoes {
                println!(
                    "{}\t{}\t{}\tpareada {}\túltimo contato {}\t{}",
                    e.id,
                    e.nome,
                    e.sistema,
                    e.pareada_em,
                    e.ultimo_contato.as_deref().unwrap_or("nunca"),
                    if e.revogada { "revogada" } else { "ativa" }
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sem_argumentos_serve_no_endereco_padrao() {
        assert_eq!(
            ler_argumentos(&[]),
            Ok(Comando::Servir("127.0.0.1:8787".parse().unwrap()))
        );
    }

    #[test]
    fn endereco_so_em_loopback() {
        assert_eq!(
            ler_argumentos(&a(&["--endereco", "127.0.0.1:9000"])),
            Ok(Comando::Servir("127.0.0.1:9000".parse().unwrap()))
        );
        assert!(ler_argumentos(&a(&["--endereco", "[::1]:9000"])).is_ok());
        assert!(ler_argumentos(&a(&["--endereco", "0.0.0.0:9000"])).is_err());
        assert!(ler_argumentos(&a(&["--endereco", "192.168.0.10:9000"])).is_err());
        assert!(ler_argumentos(&a(&["--endereco"])).is_err());
        assert!(ler_argumentos(&a(&["--endereco", "localhost"])).is_err());
    }

    #[test]
    fn a_url_do_banco_nao_entra_por_argv() {
        assert!(ler_argumentos(&a(&["--url", "postgres://x"])).is_err());
        assert!(ler_argumentos(&a(&["--endereco", "127.0.0.1:1", "postgres://x"])).is_err());
    }

    #[test]
    fn comandos_de_administracao() {
        assert_eq!(
            ler_argumentos(&a(&["estacoes"])),
            Ok(Comando::ListarEstacoes)
        );
        let id = uuid::Uuid::new_v4();
        assert_eq!(
            ler_argumentos(&a(&["revogar", &id.to_string()])),
            Ok(Comando::Revogar(id))
        );
        assert!(ler_argumentos(&a(&["revogar", "nao-e-uuid"])).is_err());
        assert!(ler_argumentos(&a(&["revogar"])).is_err());
    }

    #[test]
    fn emitir_credencial_pede_email_e_nome() {
        assert_eq!(
            ler_argumentos(&a(&[
                "emitir-credencial",
                "--email",
                "a@b.c",
                "--nome",
                "Casa"
            ])),
            Ok(Comando::Emitir {
                email: "a@b.c".into(),
                nome: "Casa".into(),
                sistema: "linux".into()
            })
        );
        assert!(ler_argumentos(&a(&["emitir-credencial", "--email", "a@b.c"])).is_err());
        assert!(ler_argumentos(&a(&["emitir-credencial", "--nome", "x", "--email"])).is_err());
        assert!(ler_argumentos(&a(&["emitir-credencial", "--nome", "x", "--url", "y"])).is_err());
    }
}
