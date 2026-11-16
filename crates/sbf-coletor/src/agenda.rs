//! A agenda do `sbf coletor-vps` sem argumentos: o Gmail e a Pluggy rodam em laços
//! independentes, cada um no seu intervalo, até o processo receber SIGTERM/Ctrl-C.
//!
//! Escolha de desenho (justificada em `docs/desktop/coletas-vps.md`): um modo próprio, ao lado
//! do `sbf servidor` e não dentro dele. O servidor HTTP e as coletas têm ciclos de vida,
//! credenciais e falhas diferentes; separados, uma coleta travada não derruba a API da
//! estação, o `systemd` reinicia cada um sozinho e o corte do Bun é desligar uma unit.
//!
//! O primeiro ciclo do Gmail é sempre uma varredura COMPLETA (pega mensagens rotuladas depois
//! da data delas); as seguintes usam o checkpoint (`after:`), e uma completa volta a cada
//! `completa_a_cada`. Erro num ciclo vai para o log e o laço segue no próximo intervalo.

use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::Pool;
use sbf_servidor::armazenamento::Armazenamento;
use tokio::sync::watch;
use tokio::time::Instant;
use uuid::Uuid;

use crate::config::opcional;
use crate::erro::ErroDeColeta;
use crate::gmail::cliente::ClienteGmail;
use crate::gmail::varredura::{OpcoesDaVarredura, varrer};
use crate::pluggy::cliente::ClientePluggy;
use crate::pluggy::sincronizacao::{OpcoesDaSincronizacao, sincronizar};

pub const GMAIL_INTERVALO_PADRAO_MIN: u64 = 15;
pub const PLUGGY_INTERVALO_PADRAO_MIN: u64 = 360;
pub const COMPLETA_PADRAO_HORAS: u64 = 168;

/// O que a agenda roda e de quanto em quanto tempo. `None` desliga a coleta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDaAgenda {
    pub gmail: Option<Duration>,
    pub pluggy: Option<Duration>,
    pub completa_a_cada: Duration,
    pub rotulo: String,
    pub consulta: String,
}

fn minutos(
    ler: &impl Fn(&str) -> Option<String>,
    nome: &str,
    padrao: u64,
) -> Result<Option<Duration>, ErroDeColeta> {
    let n = match ler(nome) {
        None => padrao,
        Some(t) => t.trim().parse::<u64>().map_err(|_| {
            ErroDeColeta::Configuracao(format!("{nome} precisa ser um número inteiro de minutos"))
        })?,
    };
    Ok((n > 0).then(|| Duration::from_secs(n * 60)))
}

impl ConfigDaAgenda {
    /// Das variáveis `SBF_COLETOR_*` (0 desliga a coleta).
    pub fn do_ambiente() -> Result<Self, ErroDeColeta> {
        Self::de_variaveis(opcional)
    }

    pub fn de_variaveis(ler: impl Fn(&str) -> Option<String>) -> Result<Self, ErroDeColeta> {
        let horas = match ler("SBF_COLETOR_GMAIL_COMPLETA_HORAS") {
            None => COMPLETA_PADRAO_HORAS,
            Some(t) => t.trim().parse::<u64>().map_err(|_| {
                ErroDeColeta::Configuracao(
                    "SBF_COLETOR_GMAIL_COMPLETA_HORAS precisa ser um número inteiro de horas"
                        .to_owned(),
                )
            })?,
        };
        Ok(Self {
            gmail: minutos(
                &ler,
                "SBF_COLETOR_GMAIL_INTERVALO_MIN",
                GMAIL_INTERVALO_PADRAO_MIN,
            )?,
            pluggy: minutos(
                &ler,
                "SBF_COLETOR_PLUGGY_INTERVALO_MIN",
                PLUGGY_INTERVALO_PADRAO_MIN,
            )?,
            completa_a_cada: Duration::from_secs(horas.max(1) * 3600),
            rotulo: ler("SBF_COLETOR_GMAIL_ROTULO").unwrap_or_else(|| "Comprovantes".to_owned()),
            consulta: ler("SBF_COLETOR_GMAIL_CONSULTA").unwrap_or_default(),
        })
    }
}

/// O que as coletas usam.
#[derive(Debug)]
pub struct Coletor {
    pub banco: Pool,
    pub armazenamento: Arc<dyn Armazenamento>,
    pub dono: Uuid,
    pub gmail: Option<ClienteGmail>,
    pub pluggy: Option<ClientePluggy>,
}

/// Espera `duracao` ou o pedido de parada; `true` se foi pedido parar.
async fn esperar(duracao: Duration, parar: &mut watch::Receiver<bool>) -> bool {
    if *parar.borrow() {
        return true;
    }
    tokio::select! {
        () = tokio::time::sleep(duracao) => false,
        _ = parar.changed() => true,
    }
}

async fn laco_do_gmail(
    c: &Coletor,
    gmail: &ClienteGmail,
    intervalo: Duration,
    agenda: &ConfigDaAgenda,
    mut parar: watch::Receiver<bool>,
) {
    let mut ultima_completa: Option<Instant> = None;
    loop {
        let completa = ultima_completa.is_none_or(|t| t.elapsed() >= agenda.completa_a_cada);
        let opcoes = OpcoesDaVarredura {
            rotulo: agenda.rotulo.clone(),
            consulta: agenda.consulta.clone(),
            completa,
            ..OpcoesDaVarredura::default()
        };
        match varrer(gmail, &c.banco, c.armazenamento.as_ref(), c.dono, &opcoes).await {
            Ok(e) => {
                if completa && e.errors == 0 {
                    ultima_completa = Some(Instant::now());
                }
                println!(
                    "gmail ({}): {} mensagem(ns), {} job(s) criado(s), {} já processado(s), {} erro(s)",
                    if completa { "completa" } else { "incremental" },
                    e.messages_listed,
                    e.jobs_created,
                    e.skipped_already_processed,
                    e.errors
                );
            }
            Err(e) => eprintln!("gmail: o ciclo falhou: {e}"),
        }
        if esperar(intervalo, &mut parar).await {
            break;
        }
    }
}

async fn laco_da_pluggy(
    c: &Coletor,
    pluggy: &ClientePluggy,
    intervalo: Duration,
    mut parar: watch::Receiver<bool>,
) {
    loop {
        match sincronizar(&c.banco, pluggy, c.dono, &OpcoesDaSincronizacao::default()).await {
            Ok(r) => println!(
                "pluggy: {} conexão(ões), {} draft(s) criado(s), {} já sincronizado(s), {} falha(s)",
                r.conexoes, r.criados, r.ja_sincronizados, r.conexoes_com_falha
            ),
            Err(e) => eprintln!("pluggy: o ciclo falhou: {e}"),
        }
        if esperar(intervalo, &mut parar).await {
            break;
        }
    }
}

/// Roda os laços até `parar` virar `true`.
pub async fn executar(c: &Coletor, agenda: &ConfigDaAgenda, parar: watch::Receiver<bool>) {
    let gmail = async {
        if let (Some(cliente), Some(intervalo)) = (&c.gmail, agenda.gmail) {
            laco_do_gmail(c, cliente, intervalo, agenda, parar.clone()).await;
        }
    };
    let pluggy = async {
        if let (Some(cliente), Some(intervalo)) = (&c.pluggy, agenda.pluggy) {
            laco_da_pluggy(c, cliente, intervalo, parar.clone()).await;
        }
    };
    tokio::join!(gmail, pluggy);
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;

    fn de(pares: &[(&str, &str)]) -> Result<ConfigDaAgenda, ErroDeColeta> {
        let mapa: HashMap<String, String> = pares
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        ConfigDaAgenda::de_variaveis(|n| mapa.get(n).cloned())
    }

    #[test]
    fn padroes_e_desligamento() {
        let c = de(&[]).expect("padrão");
        assert_eq!(c.gmail, Some(Duration::from_secs(15 * 60)));
        assert_eq!(c.pluggy, Some(Duration::from_secs(6 * 3600)));
        assert_eq!(c.completa_a_cada, Duration::from_secs(168 * 3600));
        assert_eq!(c.rotulo, "Comprovantes");
        let c = de(&[
            ("SBF_COLETOR_GMAIL_INTERVALO_MIN", "0"),
            ("SBF_COLETOR_PLUGGY_INTERVALO_MIN", "30"),
            ("SBF_COLETOR_GMAIL_ROTULO", "Notas"),
        ])
        .expect("config");
        assert_eq!(c.gmail, None);
        assert_eq!(c.pluggy, Some(Duration::from_secs(1800)));
        assert_eq!(c.rotulo, "Notas");
    }

    #[test]
    fn numero_invalido_e_erro_claro() {
        let e = de(&[("SBF_COLETOR_GMAIL_INTERVALO_MIN", "quinze")]).expect_err("inválido");
        assert!(e.to_string().contains("SBF_COLETOR_GMAIL_INTERVALO_MIN"));
    }
}
