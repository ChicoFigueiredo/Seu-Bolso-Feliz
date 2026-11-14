//! A linha de comando do `sbf coletor-vps` (ligada ao `main` do binário `sbf`).
//!
//! ```text
//! sbf coletor-vps                       agenda: Gmail e Pluggy em laço, até SIGTERM
//! sbf coletor-vps gmail   [--rotulo N] [--consulta Q] [--de D] [--ate D] [--limite N]
//!                         [--lote N] [--completa] [--simular]
//! sbf coletor-vps pluggy  [--de D] [--ate D] [--backfill] [--simular]
//! sbf coletor-vps comparar [--dias N]   só lê: contagens para o corte do Bun
//! ```
//!
//! Tudo o que é segredo vem de variável de ambiente (nunca de argv, que aparece em `ps`):
//! `SBF_DATABASE_URL`, `GOOGLE_MAIL_CLIENT_ID`, `GOOGLE_MAIL_CLIENT_SECRET`,
//! `GMAIL_REFRESH_TOKEN`, `PLUGGY_CLIENT_ID`, `PLUGGY_CLIENT_SECRET`,
//! `BLOB_READ_WRITE_TOKEN`, e opcionalmente `LOCAL_USER_ID`.

use std::process::ExitCode;
use std::sync::Arc;

use sbf_servidor::armazenamento::{Armazenamento, ArmazenamentoAusente, VercelBlob};
use tokio::sync::watch;
use zeroize::Zeroizing;

use crate::agenda::{Coletor, ConfigDaAgenda, executar};
use crate::comparador::comparar;
use crate::config::{ConfigGmail, ConfigPluggy, exigir, opcional};
use crate::dono::resolver_dono;
use crate::erro::ErroDeColeta;
use crate::gmail::cliente::ClienteGmail;
use crate::gmail::varredura::{OpcoesDaVarredura, varrer};
use crate::pluggy::cliente::ClientePluggy;
use crate::pluggy::sincronizacao::{OpcoesDaSincronizacao, sincronizar};

const AJUDA: &str = "uso: sbf coletor-vps | sbf coletor-vps gmail [--rotulo N] [--consulta Q] [--de AAAA-MM-DD] [--ate AAAA-MM-DD] [--limite N] [--lote N] [--completa] [--simular] | sbf coletor-vps pluggy [--de D] [--ate D] [--backfill] [--simular] | sbf coletor-vps comparar [--dias N]\n\
     variáveis: SBF_DATABASE_URL (obrigatória), GOOGLE_MAIL_CLIENT_ID, GOOGLE_MAIL_CLIENT_SECRET, GMAIL_REFRESH_TOKEN, PLUGGY_CLIENT_ID, PLUGGY_CLIENT_SECRET, BLOB_READ_WRITE_TOKEN, LOCAL_USER_ID (opcional), SBF_COLETOR_* (agenda)";

#[derive(Debug, PartialEq, Eq)]
pub enum Comando {
    Agenda,
    Gmail(OpcoesDaVarredura),
    Pluggy(OpcoesDaSincronizacao),
    Comparar { dias: i64 },
}

fn valor<'a>(it: &mut std::slice::Iter<'a, String>, opcao: &str) -> Result<&'a String, String> {
    it.next().ok_or_else(|| format!("falta o valor de {opcao}"))
}

fn numero(texto: &str, opcao: &str) -> Result<usize, String> {
    texto
        .parse::<usize>()
        .map_err(|_| format!("{opcao} precisa ser um número inteiro"))
}

pub fn ler_argumentos(args: &[String]) -> Result<Comando, String> {
    let Some(primeiro) = args.first() else {
        return Ok(Comando::Agenda);
    };
    let mut it = args[1..].iter();
    match primeiro.as_str() {
        "gmail" => {
            let mut o = OpcoesDaVarredura::default();
            while let Some(opcao) = it.next() {
                match opcao.as_str() {
                    "--rotulo" => o.rotulo = valor(&mut it, opcao)?.clone(),
                    "--consulta" => o.consulta = valor(&mut it, opcao)?.clone(),
                    "--de" => o.de = Some(valor(&mut it, opcao)?.clone()),
                    "--ate" => o.ate = Some(valor(&mut it, opcao)?.clone()),
                    "--limite" => o.limite = Some(numero(valor(&mut it, opcao)?, opcao)?),
                    "--lote" => {
                        o.tamanho_do_lote = u32::try_from(numero(valor(&mut it, opcao)?, opcao)?)
                            .map_err(|_| "--lote grande demais".to_owned())?;
                    }
                    "--completa" => o.completa = true,
                    "--simular" => o.simulacao = true,
                    outra => return Err(format!("flag desconhecida: {outra}")),
                }
            }
            Ok(Comando::Gmail(o))
        }
        "pluggy" => {
            let mut o = OpcoesDaSincronizacao::default();
            while let Some(opcao) = it.next() {
                match opcao.as_str() {
                    "--de" => o.de = Some(valor(&mut it, opcao)?.clone()),
                    "--ate" => o.ate = Some(valor(&mut it, opcao)?.clone()),
                    "--backfill" => o.backfill = true,
                    "--simular" => o.simulacao = true,
                    outra => return Err(format!("flag desconhecida: {outra}")),
                }
            }
            Ok(Comando::Pluggy(o))
        }
        "comparar" => {
            let mut dias = 8;
            while let Some(opcao) = it.next() {
                match opcao.as_str() {
                    "--dias" => {
                        dias = i64::try_from(numero(valor(&mut it, opcao)?, opcao)?)
                            .map_err(|_| "--dias grande demais".to_owned())?;
                    }
                    outra => return Err(format!("flag desconhecida: {outra}")),
                }
            }
            Ok(Comando::Comparar {
                dias: dias.clamp(1, 366),
            })
        }
        outro => Err(format!("argumento desconhecido: {outro}")),
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

async fn rodar_async(comando: Comando) -> Result<ExitCode, ErroDeColeta> {
    let url = Zeroizing::new(exigir("SBF_DATABASE_URL")?);
    let banco = sbf_servidor::banco::abrir_pool(&url, 4)
        .map_err(|e| ErroDeColeta::Configuracao(e.to_string()))?;
    let dono = {
        let c = banco.get().await?;
        resolver_dono(&**c, opcional("LOCAL_USER_ID").as_deref()).await?
    };
    let armazenamento: Arc<dyn Armazenamento> = match VercelBlob::do_ambiente() {
        Some(v) => Arc::new(v),
        None => Arc::new(ArmazenamentoAusente),
    };
    match comando {
        Comando::Comparar { dias } => {
            print!("{}", comparar(&banco, dono, dias).await?);
            Ok(ExitCode::SUCCESS)
        }
        Comando::Gmail(opcoes) => {
            let gmail = ClienteGmail::novo(ConfigGmail::do_ambiente()?);
            if !opcoes.simulacao && opcional("BLOB_READ_WRITE_TOKEN").is_none() {
                return Err(ErroDeColeta::Configuracao(
                    "a variável BLOB_READ_WRITE_TOKEN é obrigatória (use --simular para só listar)"
                        .to_owned(),
                ));
            }
            let e = varrer(&gmail, &banco, armazenamento.as_ref(), dono, &opcoes).await?;
            println!(
                "\nmensagens listadas: {}\nmensagens com anexos: {}\nanexos encontrados: {}\nanexos enviados ao armazenamento: {}\njobs de ingestão criados: {}\njá processados: {} (por mensagem+nome: {}, por conteúdo: {})\nerros: {}",
                e.messages_listed,
                e.messages_with_attachments,
                e.attachments_found,
                e.attachments_uploaded,
                e.jobs_created,
                e.skipped_already_processed,
                e.skipped_by_msg_filename,
                e.skipped_by_content_hash,
                e.errors
            );
            Ok(if e.errors > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Comando::Pluggy(opcoes) => {
            let pluggy = ClientePluggy::novo(ConfigPluggy::do_ambiente()?);
            let r = sincronizar(&banco, &pluggy, dono, &opcoes).await?;
            if opcoes.simulacao {
                println!(
                    "simulação: {} draft(s) seriam criados, {} já existem",
                    r.criados, r.ja_sincronizados
                );
            }
            Ok(if r.conexoes_com_falha > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Comando::Agenda => {
            let agenda = ConfigDaAgenda::do_ambiente()?;
            // Só pede credencial de quem a agenda vai usar: falta de variável aparece na largada,
            // não 15 minutos depois.
            let gmail = match agenda.gmail {
                Some(_) => Some(ClienteGmail::novo(ConfigGmail::do_ambiente()?)),
                None => None,
            };
            let pluggy = match agenda.pluggy {
                Some(_) => Some(ClientePluggy::novo(ConfigPluggy::do_ambiente()?)),
                None => None,
            };
            if gmail.is_some() && opcional("BLOB_READ_WRITE_TOKEN").is_none() {
                return Err(ErroDeColeta::Configuracao(
                    "a variável BLOB_READ_WRITE_TOKEN é obrigatória para a coleta do Gmail"
                        .to_owned(),
                ));
            }
            if gmail.is_none() && pluggy.is_none() {
                return Err(ErroDeColeta::Configuracao(
                    "as duas coletas estão desligadas (intervalo 0): não há o que fazer".to_owned(),
                ));
            }
            let coletor = Coletor {
                banco,
                armazenamento,
                dono,
                gmail,
                pluggy,
            };
            let (parar, ouvir) = watch::channel(false);
            tokio::spawn(ate_o_sinal(parar));
            println!("sbf coletor-vps: no ar");
            executar(&coletor, &agenda, ouvir).await;
            println!("sbf coletor-vps: encerrado");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// O braço `coletor-vps` do `main`. `args` vem sem o próprio `coletor-vps`.
pub fn rodar(args: &[String]) -> ExitCode {
    let comando = match ler_argumentos(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sbf coletor-vps: {e}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sbf coletor-vps: não deu para iniciar o runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(rodar_async(comando)) {
        Ok(codigo) => codigo,
        Err(e) => {
            eprintln!("sbf coletor-vps: {e}");
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
    fn sem_argumentos_e_a_agenda() {
        assert_eq!(ler_argumentos(&[]), Ok(Comando::Agenda));
    }

    #[test]
    fn gmail_com_flags() {
        let c = ler_argumentos(&a(&[
            "gmail",
            "--rotulo",
            "Notas",
            "--consulta",
            "has:attachment",
            "--de",
            "2026-01-01",
            "--ate",
            "2026-02-01",
            "--limite",
            "10",
            "--lote",
            "25",
            "--completa",
            "--simular",
        ]))
        .expect("gmail");
        let Comando::Gmail(o) = c else {
            panic!("não é gmail")
        };
        assert_eq!(o.rotulo, "Notas");
        assert_eq!(o.consulta, "has:attachment");
        assert_eq!(
            (o.de.as_deref(), o.ate.as_deref()),
            (Some("2026-01-01"), Some("2026-02-01"))
        );
        assert_eq!((o.limite, o.tamanho_do_lote), (Some(10), 25));
        assert!(o.completa && o.simulacao);
    }

    #[test]
    fn pluggy_e_comparar() {
        assert_eq!(
            ler_argumentos(&a(&["pluggy", "--backfill", "--de", "2025-08-24"])),
            Ok(Comando::Pluggy(OpcoesDaSincronizacao {
                de: Some("2025-08-24".into()),
                backfill: true,
                ..Default::default()
            }))
        );
        assert_eq!(
            ler_argumentos(&a(&["comparar"])),
            Ok(Comando::Comparar { dias: 8 })
        );
        assert_eq!(
            ler_argumentos(&a(&["comparar", "--dias", "999"])),
            Ok(Comando::Comparar { dias: 366 })
        );
    }

    #[test]
    fn flag_desconhecida_e_falta_de_valor_sao_erros() {
        assert!(ler_argumentos(&a(&["gmail", "--nada"])).is_err());
        assert!(ler_argumentos(&a(&["gmail", "--rotulo"])).is_err());
        assert!(ler_argumentos(&a(&["gmail", "--limite", "x"])).is_err());
        assert!(ler_argumentos(&a(&["pluggy", "--dry"])).is_err());
        assert!(ler_argumentos(&a(&["outra"])).is_err());
    }
}
