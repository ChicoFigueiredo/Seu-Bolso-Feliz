//! A coleta de e-mail para o cofre (spec 13): os anexos entram no cofre e passam pelas regras
//! de reconhecimento. Este módulo tem a coleta das contas (CLI, worker e MCP); o CLI de
//! contas é o [`crate::email`] (`sbf email ...`; `sbf gmail ...` é o mesmo comando).
//!
//! ```text
//! sbf email coletar [<id>] --inventario <cofre> [--consulta "<query do Gmail>"] [--desde AAAA-MM-DD]
//!                   [--limite N]
//! ```
//!
//! Cada conta conectada segue os seus rótulos (`email.json`); baixa os anexos (PDF, imagens,
//! XML de NF-e...) de `has:attachment` + os termos extras, desde o checkpoint **do rótulo**
//! (90 dias na primeira vez), para `<cofre>/entrada/gmail/` (a conta `gmail`) ou
//! `<cofre>/entrada/email/<id>/` (as outras), registra cada um no inventário com origem
//! `<fonte>:<mensagem>:<parte>`, deduplica por sha256 e roda o reconhecimento com as regras em
//! cada documento novo. Nada vai para o planejamento: as regras só propõem. `--consulta` é um
//! teste avulso (sem rótulo) e nunca vira a consulta da conta.
//!
//! As credenciais vêm do chaveiro, do ambiente e do `.env` (veja `sbf_gmail::credenciais`),
//! nunca de argumento de linha de comando. A saída mostra só contagens.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::Chaveiro;
use sbf_gmail::config::{ARQUIVO_DA_CONFIG, Config, Conta, arquivo_dos_rotulos};
use sbf_gmail::credenciais::{Alvo, Credenciais, marcar_vencida, presentes};
use sbf_gmail::{ClienteReal, Erro, OpcoesDaColeta, ReconhecimentoDoCofre, ResumoDaColeta};
use sbf_worker::laco::{ColetorDeGmail, Dados, ResumoDoGmail};
use sbf_worker::registro::Falha;

/// A pasta de dados do app (onde ficam `email.json` e a lista de rótulos), se existe.
pub fn pasta_de_dados() -> Option<PathBuf> {
    sbf_chaveiro::pasta_de_dados_do_app().ok()
}

/// As contas de e-mail (veja `sbf_gmail::contas::contas_efetivas`).
pub fn contas(dados: Option<&Path>, chaveiro: Option<&Chaveiro>) -> Config {
    sbf_gmail::contas::contas_efetivas(dados, chaveiro)
}

/// A conta é coletável (o tipo conecta e as três credenciais estão à mão)?
fn coletavel(c: &Conta, chaveiro: Option<&Chaveiro>) -> bool {
    c.tipo.disponivel()
        && presentes(
            chaveiro,
            Alvo {
                id: &c.id,
                tipo: c.tipo,
            },
        )
}

/// A conta tem credenciais e o provedor não as recusou (vale tentar)?
fn tentavel(c: &Conta, chaveiro: Option<&Chaveiro>) -> bool {
    let alvo = Alvo {
        id: &c.id,
        tipo: c.tipo,
    };
    c.tipo.disponivel()
        && Credenciais::do_ambiente(chaveiro, alvo).is_ok_and(|cr| !cr.vencida(chaveiro, &c.id))
}

/// Há alguma conta coletável?
pub fn ha_contas_coletaveis() -> bool {
    let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
    let dados = pasta_de_dados();
    contas(dados.as_deref(), chaveiro.as_ref())
        .contas
        .iter()
        .any(|c| coletavel(c, chaveiro.as_ref()))
}

/// Coleta uma conta: credenciais, cliente, reconhecimento do cofre. Autorização vencida
/// (sem rede, se já se sabe; ou `invalid_grant` agora) marca a conta no chaveiro.
fn coletar_uma(
    cofre: &Path,
    dados: Option<&Path>,
    chaveiro: Option<&Chaveiro>,
    conta: &Conta,
    opcoes: &OpcoesDaColeta,
    cancelar: &AtomicBool,
) -> Result<ResumoDaColeta, Erro> {
    let alvo = Alvo {
        id: &conta.id,
        tipo: conta.tipo,
    };
    let cred = Credenciais::do_ambiente(chaveiro, alvo)?;
    if cred.vencida(chaveiro, &conta.id) {
        return Err(Erro::Autorizacao);
    }
    let fonte = ClienteReal::novo(&cred)?;
    let reconhecedor =
        crate::reconhecer::montar(cofre, chaveiro.cloned(), false).map_err(Erro::Reconhecimento)?;
    let reconhecimento = ReconhecimentoDoCofre::novo(reconhecedor);
    let rotulos = dados.map(|d| d.join(arquivo_dos_rotulos(&conta.id)));
    let r = sbf_gmail::contas::coletar_conta(
        cofre,
        conta,
        &fonte,
        &reconhecimento,
        opcoes,
        rotulos.as_deref(),
        cancelar,
    );
    match &r {
        Err(Erro::Autorizacao) => {
            if let Some(c) = chaveiro {
                let _ = marcar_vencida(c, &conta.id, &cred);
            }
        }
        Ok(_) => guardar_email_da_conta(dados, &fonte, &conta.id, conta.conta.is_none()),
        Err(_) => {}
    }
    r
}

/// Guarda o e-mail da conta em `email.json` se ainda não se sabe (uma chamada, a primeira vez).
fn guardar_email_da_conta(dados: Option<&Path>, fonte: &ClienteReal, id: &str, falta: bool) {
    let (true, Some(d)) = (falta, dados) else {
        return;
    };
    let Ok(email) = fonte.conta() else {
        return;
    };
    let arquivo = d.join(ARQUIVO_DA_CONFIG);
    let mut cfg = Config::carregar(&arquivo).unwrap_or_default();
    if cfg.contas.is_empty() {
        cfg.contas.push(Conta::legada());
    }
    if let Some(c) = cfg.conta_mut(id)
        && c.conta.is_none()
    {
        c.conta = Some(email);
        let _ = cfg.salvar(&arquivo);
    }
}

/// Coleta as contas coletáveis (ou só a `so`), uma de cada vez, cada falha isolada. Roda numa
/// thread própria, porque o cliente cria o seu runtime e isso não pode acontecer dentro de
/// outro.
pub fn coletar_contas(
    cofre: &Path,
    so: Option<&str>,
    opcoes: &OpcoesDaColeta,
    cancelar: &AtomicBool,
) -> Vec<(String, Result<ResumoDaColeta, Erro>)> {
    std::thread::scope(|escopo| {
        escopo
            .spawn(|| {
                let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
                let dados = pasta_de_dados();
                let config = contas(dados.as_deref(), chaveiro.as_ref());
                config
                    .contas
                    .iter()
                    .filter(|c| so.is_none_or(|id| id == c.id))
                    .filter(|c| so.is_some() || coletavel(c, chaveiro.as_ref()))
                    .map(|c| {
                        let r = if c.tipo.disponivel() {
                            coletar_uma(
                                cofre,
                                dados.as_deref(),
                                chaveiro.as_ref(),
                                c,
                                opcoes,
                                cancelar,
                            )
                        } else {
                            Err(Erro::Argumento(format!(
                                "{} ainda está em breve",
                                c.tipo.rotulo()
                            )))
                        };
                        (c.id.clone(), r)
                    })
                    .collect()
            })
            .join()
            .unwrap_or_default()
    })
}

/// Junta o resultado das contas: soma as que deram certo; só é erro se **nenhuma** deu (a
/// autorização vencida vence as outras causas só quando todas estão vencidas).
///
/// # Errors
/// [`Erro::SemCredenciais`] se não há conta; senão o primeiro erro (ou [`Erro::Autorizacao`] se
/// todas estão vencidas).
pub fn juntar(
    resultados: Vec<(String, Result<ResumoDaColeta, Erro>)>,
) -> Result<ResumoDaColeta, Erro> {
    if resultados.is_empty() {
        return Err(Erro::SemCredenciais(
            "nenhuma conta de e-mail conectada".to_owned(),
        ));
    }
    let mut total = ResumoDaColeta::default();
    let mut algum = false;
    let mut erros: Vec<Erro> = Vec::new();
    for (_, r) in resultados {
        match r {
            Ok(r) => {
                algum = true;
                total.somar(&r);
            }
            Err(e) => erros.push(e),
        }
    }
    if algum {
        total.falhas += erros.len();
        return Ok(total);
    }
    if erros.iter().all(|e| *e == Erro::Autorizacao) {
        return Err(Erro::Autorizacao);
    }
    Err(erros
        .into_iter()
        .find(|e| *e != Erro::Autorizacao)
        .unwrap_or(Erro::Autorizacao))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Opcoes {
    pub(crate) cofre: Option<PathBuf>,
    pub(crate) coleta: OpcoesDaColeta,
}

pub(crate) fn ler_opcoes_da_coleta(args: &[String]) -> Result<Opcoes, String> {
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
pub(crate) fn resumo_em_linhas(r: &ResumoDaColeta) -> String {
    format!(
        "{} mensagens lidas; {} anexos novos ({} documentos novos, {} repetidos de outro e-mail, {} já conhecidos); \
         {} e-mails sem anexo viraram mensagem; regras: {} reconhecidos, {} ambíguos, {} sem regra; {} falhas; {} ficaram para a próxima coleta; {} rótulos seguidos não existem mais na conta",
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
        r.rotulos_ausentes,
    )
}

/// Coleta todas as contas conectadas (a falha de uma não para as outras) e soma. Serve ao
/// worker e ao MCP.
///
/// # Errors
/// Como [`juntar`].
pub fn coletar_do_cofre(
    cofre: &Path,
    opcoes: &OpcoesDaColeta,
    cancelar: &AtomicBool,
) -> Result<ResumoDaColeta, Erro> {
    juntar(coletar_contas(cofre, None, opcoes, cancelar))
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

/// O e-mail do worker: a coleta de `gmail_minutos` em `gmail_minutos`, de todas as contas
/// conectadas, cada rótulo no seu checkpoint.
pub struct GmailReal {
    pub dados: Dados,
}

/// O erro da coleta como categoria do log do worker.
fn falha_de(e: &Erro) -> Falha {
    match e {
        Erro::Banco(_) => Falha::Banco,
        Erro::Disco(_) => Falha::Disco,
        Erro::Autorizacao => Falha::Autorizacao,
        Erro::SemCredenciais(_) | Erro::Gmail(_) | Erro::Login(_) | Erro::Chaveiro(_) => {
            Falha::Externo
        }
        _ => Falha::Outra,
    }
}

impl ColetorDeGmail for GmailReal {
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoGmail, Falha> {
        let resultados = coletar_contas(
            &self.dados.cofre,
            None,
            &OpcoesDaColeta::default(),
            cancelar,
        );
        if resultados.is_empty() {
            // Nenhuma conta conectada (ainda): não é falha.
            return Ok(ResumoDoGmail::default());
        }
        let r = juntar(resultados).map_err(|e| falha_de(&e))?;
        Ok(ResumoDoGmail {
            mensagens: r.mensagens,
            novos: r.novos(),
        })
    }

    fn pode_tentar_de_novo(&mut self) -> bool {
        let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
        let dados = pasta_de_dados();
        contas(dados.as_deref(), chaveiro.as_ref())
            .contas
            .iter()
            .any(|c| tentavel(c, chaveiro.as_ref()))
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
        let o = ler_opcoes_da_coleta(&a(&[
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
        assert!(ler_opcoes_da_coleta(&a(&["--limite", "muitos"])).is_err());
        assert!(ler_opcoes_da_coleta(&a(&["--consulta"])).is_err());
        assert!(ler_opcoes_da_coleta(&a(&["--senha", "x"])).is_err());
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

    fn ok(mensagens: usize) -> Result<ResumoDaColeta, Erro> {
        Ok(ResumoDaColeta {
            mensagens,
            ..ResumoDaColeta::default()
        })
    }

    #[test]
    fn juntar_isola_as_contas_uma_falha_nao_derruba_as_outras() {
        let r = juntar(vec![
            ("gmail".into(), Err(Erro::Autorizacao)),
            ("gmail-2".into(), ok(3)),
            ("gmail-3".into(), Err(Erro::Gmail("rede".into()))),
        ])
        .unwrap();
        assert_eq!((r.mensagens, r.falhas), (3, 2));
    }

    #[test]
    fn juntar_so_e_autorizacao_quando_todas_estao_vencidas() {
        assert_eq!(
            juntar(vec![
                ("a".into(), Err(Erro::Autorizacao)),
                ("b".into(), Err(Erro::Autorizacao)),
            ])
            .unwrap_err(),
            Erro::Autorizacao
        );
        // Uma vencida e outra com erro de rede: o erro que dá para tentar de novo vence.
        assert!(matches!(
            juntar(vec![
                ("a".into(), Err(Erro::Autorizacao)),
                ("b".into(), Err(Erro::Gmail("rede".into()))),
            ]),
            Err(Erro::Gmail(_))
        ));
        assert!(matches!(juntar(vec![]), Err(Erro::SemCredenciais(_))));
    }

    #[test]
    fn invalid_grant_vira_a_causa_autorizacao_no_log_do_worker() {
        assert_eq!(falha_de(&Erro::Autorizacao), Falha::Autorizacao);
        assert_eq!(falha_de(&Erro::Gmail("x".into())), Falha::Externo);
        assert_eq!(falha_de(&Erro::Banco("x".into())), Falha::Banco);
        assert_eq!(Falha::Autorizacao.to_string(), "autorizacao");
    }
}
