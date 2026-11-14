//! A linha de comando do `sbf canais` (ligada ao `main` do binário `sbf`).
//!
//! ```text
//! sbf canais                              o bot e a agenda de alertas, até SIGTERM
//! sbf canais vincular-telegram            gera o código de uso único (vale 10 minutos)
//! sbf canais alertas [--simular]          roda as regras uma vez (--simular só imprime, sem rede)
//! sbf canais nivel <chat_id> <nivel>      mascarado, parcial ou completo
//! sbf canais desvincular <chat_id>
//! ```
//!
//! Token e URL do banco vêm só de variável de ambiente (ver [`crate::config`]).

use std::process::ExitCode;
use std::sync::Arc;

use sbf_coletor::config::{exigir, opcional};
use sbf_coletor::dono::resolver_dono;
use sbf_servidor::armazenamento::{Armazenamento, ArmazenamentoAusente, VercelBlob};
use tokio::sync::watch;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::agenda::{ContextoDeAlertas, agora_unix, laco, relogio_de_sao_paulo, rodar_uma_vez};
use crate::alertas::avaliar;
use crate::bot::Bot;
use crate::confianca::Nivel;
use crate::config::{ConfigAlertas, ConfigTelegram};
use crate::erro::ErroDeCanal;
use crate::repo;
use crate::telegram::ClienteTelegram;
use crate::texto::linha;

const AJUDA: &str = "uso: sbf canais | sbf canais vincular-telegram | sbf canais alertas [--simular] | sbf canais nivel <chat_id> <mascarado|parcial|completo> | sbf canais desvincular <chat_id>\n\
     variáveis: SBF_DATABASE_URL, SBF_TELEGRAM_TOKEN, SBF_TELEGRAM_CHATS, BLOB_READ_WRITE_TOKEN, SBF_ALERTAS_* (ver docs/desktop/telegram-e-alertas.md)";

#[derive(Debug, PartialEq, Eq)]
pub enum Comando {
    Servir,
    Vincular,
    Alertas { simular: bool },
    Nivel { chat: i64, nivel: Nivel },
    Desvincular { chat: i64 },
}

pub fn ler_argumentos(args: &[String]) -> Result<Comando, String> {
    let chat = |t: Option<&String>| -> Result<i64, String> {
        t.ok_or("falta o chat_id")?
            .parse()
            .map_err(|_| "o chat_id precisa ser um número".to_owned())
    };
    match args.first().map(String::as_str) {
        None => Ok(Comando::Servir),
        Some("vincular-telegram") if args.len() == 1 => Ok(Comando::Vincular),
        Some("alertas") => match &args[1..] {
            [] => Ok(Comando::Alertas { simular: false }),
            [f] if f == "--simular" => Ok(Comando::Alertas { simular: true }),
            _ => Err("alertas aceita só --simular".into()),
        },
        Some("nivel") if args.len() == 3 => Ok(Comando::Nivel {
            chat: chat(args.get(1))?,
            nivel: Nivel::de_codigo(&args[2]).map_err(|e| e.to_string())?,
        }),
        Some("desvincular") if args.len() == 2 => Ok(Comando::Desvincular {
            chat: chat(args.get(1))?,
        }),
        Some(outro) => Err(format!("argumento desconhecido: {outro}")),
    }
}

async fn ate_o_sinal(parar: watch::Sender<bool>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            let _ = parar.send(true);
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
    let _ = parar.send(true);
}

async fn dono_do_ambiente(banco: &deadpool_postgres::Pool) -> Result<Uuid, ErroDeCanal> {
    let c = banco.get().await?;
    Ok(resolver_dono(&**c, opcional("LOCAL_USER_ID").as_deref()).await?)
}

async fn rodar_async(comando: Comando) -> Result<ExitCode, ErroDeCanal> {
    let url = Zeroizing::new(exigir("SBF_DATABASE_URL")?);
    let banco = sbf_servidor::banco::abrir_pool(&url, 4)
        .map_err(|e| ErroDeCanal::Configuracao(e.to_string()))?;
    match comando {
        Comando::Vincular => {
            let dono = dono_do_ambiente(&banco).await?;
            let codigo = repo::gerar_codigo(&banco, dono).await?;
            println!(
                "Código de vinculação: {codigo}\nVale {} minutos e só uma vez. No Telegram, mande ao bot:\n/vincular {codigo}",
                repo::VALIDADE_DO_CODIGO_EM_MINUTOS
            );
            Ok(ExitCode::SUCCESS)
        }
        Comando::Nivel { chat, nivel } => {
            repo::definir_nivel(&banco, chat, nivel).await?;
            println!("nível de confiança do chat {chat}: {}", nivel.codigo());
            Ok(ExitCode::SUCCESS)
        }
        Comando::Desvincular { chat } => {
            let revogado = repo::revogar(&banco, chat).await?;
            println!(
                "{}",
                if revogado {
                    "desvinculado"
                } else {
                    "esse chat não estava vinculado (um chat da lista SBF_TELEGRAM_CHATS sai da variável)"
                }
            );
            Ok(ExitCode::SUCCESS)
        }
        Comando::Alertas { simular: true } => {
            let dono = dono_do_ambiente(&banco).await?;
            let config = ConfigAlertas::do_ambiente()?;
            let livro = repo::carregar_livro(&banco, dono).await?;
            let (hoje, _) = relogio_de_sao_paulo(agora_unix());
            let alertas = avaliar(&livro, hoje, &config.limites, &[]);
            if alertas.is_empty() {
                println!("nenhum alerta vale hoje");
            }
            for a in &alertas {
                println!("[{}] {}", a.tipo.codigo(), linha(a, Nivel::Mascarado));
            }
            Ok(ExitCode::SUCCESS)
        }
        Comando::Alertas { simular: false } => {
            let dono = dono_do_ambiente(&banco).await?;
            let tg = ConfigTelegram::do_ambiente()?;
            let ctx = contexto(&banco, &tg, dono)?;
            let (hoje, _) = relogio_de_sao_paulo(agora_unix());
            let n = rodar_uma_vez(&ctx, hoje, false).await?;
            println!("{n} alerta(s) avisado(s)");
            Ok(ExitCode::SUCCESS)
        }
        Comando::Servir => {
            let dono = dono_do_ambiente(&banco).await?;
            let tg = ConfigTelegram::do_ambiente()?;
            let armazenamento: Arc<dyn Armazenamento> = match VercelBlob::do_ambiente() {
                Some(v) => Arc::new(v),
                None => Arc::new(ArmazenamentoAusente),
            };
            let ctx = contexto(&banco, &tg, dono)?;
            let mut bot = Bot::novo(
                banco.clone(),
                ctx.telegram.clone(),
                armazenamento,
                tg.chats_autorizados.clone(),
                dono,
            );
            bot.alertas = Some(ctx.clone());
            let (parar, ouvir) = watch::channel(false);
            tokio::spawn(ate_o_sinal(parar));
            println!("sbf canais: no ar");
            let agenda = tokio::spawn(laco(ctx, ouvir.clone()));
            bot.laco(ouvir).await;
            let _ = agenda.await;
            println!("sbf canais: encerrado");
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn contexto(
    banco: &deadpool_postgres::Pool,
    tg: &ConfigTelegram,
    dono: Uuid,
) -> Result<ContextoDeAlertas, ErroDeCanal> {
    Ok(ContextoDeAlertas {
        banco: banco.clone(),
        telegram: ClienteTelegram::novo(&tg.base, tg.token.clone())?,
        dono,
        chats_configurados: tg.chats_autorizados.iter().copied().collect(),
        config: ConfigAlertas::do_ambiente()?,
        extras: Arc::new(Vec::new()),
    })
}

/// O braço `canais` do `main`. `args` vem sem o próprio `canais`.
pub fn rodar(args: &[String]) -> ExitCode {
    let comando = match ler_argumentos(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sbf canais: {e}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sbf canais: não deu para iniciar o runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(rodar_async(comando)) {
        Ok(codigo) => codigo,
        Err(e) => {
            eprintln!("sbf canais: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn argumentos() {
        assert_eq!(ler_argumentos(&[]), Ok(Comando::Servir));
        assert_eq!(
            ler_argumentos(&a(&["vincular-telegram"])),
            Ok(Comando::Vincular)
        );
        assert_eq!(
            ler_argumentos(&a(&["alertas", "--simular"])),
            Ok(Comando::Alertas { simular: true })
        );
        assert_eq!(
            ler_argumentos(&a(&["nivel", "-100", "parcial"])),
            Ok(Comando::Nivel {
                chat: -100,
                nivel: Nivel::Parcial
            })
        );
        assert!(ler_argumentos(&a(&["nivel", "x", "parcial"])).is_err());
        assert!(ler_argumentos(&a(&["nivel", "1", "tudo"])).is_err());
        assert!(ler_argumentos(&a(&["outro"])).is_err());
    }
}
