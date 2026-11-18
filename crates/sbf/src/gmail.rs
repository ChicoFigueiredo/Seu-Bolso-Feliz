//! `sbf gmail coletar`: os anexos de e-mail entram no cofre e passam pelas regras de
//! reconhecimento (spec 13).
//!
//! ```text
//! sbf gmail coletar --inventario <cofre> [--consulta "<query do Gmail>"] [--desde AAAA-MM-DD]
//!                   [--limite N]
//! ```
//!
//! Baixa os anexos (PDF, imagens, XML de NF-e...) de `has:attachment newer_than:30d
//! -category:promotions` (ou da `--consulta`) para `<cofre>/entrada/gmail/`, registra cada um
//! no inventário com origem `gmail:<mensagem>:<parte>`, deduplica por sha256 e roda o
//! reconhecimento com as regras em cada documento novo. E-mail sem anexo vira entrada
//! Mensagem. Um checkpoint (o `internalDate` da última mensagem) faz a próxima coleta trazer
//! só o que é novo; `--desde` ignora o checkpoint. Nada vai para o planejamento: as regras só
//! propõem.
//!
//! As credenciais OAuth2 vêm das variáveis de ambiente `GOOGLE_MAIL_CLIENT_ID`,
//! `GOOGLE_MAIL_CLIENT_SECRET` e `GMAIL_REFRESH_TOKEN` (as mesmas do `sbf coletor-vps gmail`),
//! nunca de argumento de linha de comando. A saída mostra só contagens.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use sbf_gmail::{ClienteReal, Erro, OpcoesDaColeta, ReconhecimentoDoCofre, ResumoDaColeta};
use sbf_worker::laco::{ColetorDeGmail, Dados, ResumoDoGmail};
use sbf_worker::registro::Falha;

const USO: &str = "uso: sbf gmail coletar --inventario <cofre> [--consulta \"<query do Gmail>\"] [--desde AAAA-MM-DD] [--limite N]\n\
     credenciais: variáveis de ambiente GOOGLE_MAIL_CLIENT_ID, GOOGLE_MAIL_CLIENT_SECRET e GMAIL_REFRESH_TOKEN";

#[derive(Debug, Default, PartialEq, Eq)]
struct Opcoes {
    cofre: Option<PathBuf>,
    coleta: OpcoesDaColeta,
}

fn ler_opcoes(args: &[String]) -> Result<Opcoes, String> {
    let mut o = Opcoes::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} pede um valor"))
        };
        match a.as_str() {
            "--inventario" => o.cofre = Some(PathBuf::from(valor()?)),
            "--consulta" => o.coleta.consulta = Some(valor()?),
            "--desde" => o.coleta.desde = Some(valor()?),
            "--limite" => {
                o.coleta.limite = Some(
                    valor()?
                        .parse::<usize>()
                        .map_err(|_| "--limite precisa ser um número inteiro".to_owned())?,
                );
            }
            outro => return Err(format!("argumento desconhecido: {outro}")),
        }
    }
    Ok(o)
}

/// A linha de resumo: só contagens.
fn resumo_em_linhas(r: &ResumoDaColeta) -> String {
    format!(
        "gmail: {} mensagens lidas; {} anexos novos ({} documentos novos, {} repetidos de outro e-mail, {} já conhecidos); \
         {} e-mails sem anexo viraram mensagem; regras: {} reconhecidos, {} ambíguos, {} sem regra; {} falhas; {} ficaram para a próxima coleta",
        r.mensagens,
        r.anexos_novos,
        r.documentos_novos,
        r.duplicados,
        r.ja_conhecidos,
        r.mensagens_de_texto,
        r.reconhecidos,
        r.ambiguos,
        r.sem_regra,
        r.falhas,
        r.restantes,
    )
}

/// Roda uma coleta no cofre com o cliente real e o reconhecimento do cofre (as regras ligadas,
/// o chaveiro para PDF protegido, sem IA). Serve ao CLI, ao worker e ao MCP: roda numa thread
/// própria, porque o cliente cria o seu runtime e isso não pode acontecer dentro de outro.
///
/// # Errors
/// [`Erro`] da coleta; falta de credenciais vem como [`Erro::SemCredenciais`] com os nomes.
pub fn coletar_do_cofre(
    cofre: &Path,
    opcoes: &OpcoesDaColeta,
    cancelar: &AtomicBool,
) -> Result<ResumoDaColeta, Erro> {
    std::thread::scope(|escopo| {
        escopo
            .spawn(|| {
                let fonte = ClienteReal::do_ambiente()?;
                let chaveiro = sbf_chaveiro::Chaveiro::abrir_na_pasta_padrao().ok();
                let reconhecedor = crate::reconhecer::montar(cofre, chaveiro, false)
                    .map_err(Erro::Reconhecimento)?;
                let reconhecimento = ReconhecimentoDoCofre::novo(reconhecedor);
                sbf_gmail::coletar(cofre, &fonte, &reconhecimento, opcoes, cancelar)
            })
            .join()
            .unwrap_or_else(|_| Err(Erro::Gmail("a coleta parou de repente".to_owned())))
    })
}

/// O resumo para o MCP: só contagens.
pub fn resumo_em_json(r: &ResumoDaColeta) -> serde_json::Value {
    serde_json::json!({
        "consulta": r.consulta,
        "mensagens": r.mensagens,
        "anexos_novos": r.anexos_novos,
        "documentos_novos": r.documentos_novos,
        "repetidos_de_outro_email": r.duplicados,
        "ja_conhecidos": r.ja_conhecidos,
        "emails_sem_anexo_como_mensagem": r.mensagens_de_texto,
        "reconhecidos": r.reconhecidos,
        "ambiguos": r.ambiguos,
        "sem_regra": r.sem_regra,
        "falhas": r.falhas,
        "restantes_para_a_proxima": r.restantes,
    })
}

/// O Gmail do worker: a coleta de `gmail_minutos` em `gmail_minutos`, com a consulta padrão.
pub struct GmailReal {
    pub dados: Dados,
}

impl ColetorDeGmail for GmailReal {
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoGmail, Falha> {
        let r = coletar_do_cofre(&self.dados.cofre, &OpcoesDaColeta::default(), cancelar).map_err(
            |e| match e {
                Erro::Banco(_) => Falha::Banco,
                Erro::Disco(_) => Falha::Disco,
                Erro::SemCredenciais(_) | Erro::Gmail(_) => Falha::Externo,
                _ => Falha::Outra,
            },
        )?;
        Ok(ResumoDoGmail {
            mensagens: r.mensagens,
            novos: r.novos(),
        })
    }
}

pub fn rodar(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) != Some("coletar") {
        eprintln!("sbf gmail: {USO}");
        return ExitCode::from(2);
    }
    let o = match ler_opcoes(&args[1..]) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sbf gmail: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    let Some(cofre) = o.cofre else {
        eprintln!("sbf gmail: falta --inventario.\n{USO}");
        return ExitCode::from(2);
    };
    match coletar_do_cofre(&cofre, &o.coleta, &AtomicBool::new(false)) {
        Ok(r) => {
            println!("{}", resumo_em_linhas(&r));
            if r.falhas > 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(Erro::SemCredenciais(nomes)) => {
            eprintln!(
                "sbf gmail: faltam variáveis de ambiente com as credenciais: {nomes}.\n{USO}"
            );
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("sbf gmail: {e}");
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
    fn opcoes_do_comando() {
        let o = ler_opcoes(&a(&[
            "--inventario",
            "/cofre",
            "--consulta",
            "from:banco",
            "--desde",
            "2026-10-01",
            "--limite",
            "50",
        ]))
        .unwrap();
        assert_eq!(o.cofre, Some(PathBuf::from("/cofre")));
        assert_eq!(o.coleta.consulta.as_deref(), Some("from:banco"));
        assert_eq!(o.coleta.desde.as_deref(), Some("2026-10-01"));
        assert_eq!(o.coleta.limite, Some(50));
        assert!(ler_opcoes(&a(&["--limite", "muitos"])).is_err());
        assert!(ler_opcoes(&a(&["--consulta"])).is_err());
        assert!(ler_opcoes(&a(&["--senha", "x"])).is_err());
    }

    #[test]
    fn o_resumo_so_tem_contagens() {
        let r = ResumoDaColeta {
            consulta: "from:segredo@exemplo.test".to_owned(),
            mensagens: 3,
            ..ResumoDaColeta::default()
        };
        assert!(resumo_em_linhas(&r).contains("3 mensagens lidas"));
        assert!(!resumo_em_linhas(&r).contains("segredo"));
    }
}
