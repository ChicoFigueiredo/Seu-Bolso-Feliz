//! O ciclo que o `sbf worker` roda: lê `mensageiros.json`, e para cada mensageiro ligado e
//! **pronto** (Telegram com export apontado; WhatsApp com ponte configurada e sessão
//! guardada) coleta só as conversas incluídas. Nada é aberto, conectado ou lido quando o
//! interruptor geral está desligado ou nenhuma conversa foi incluída.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_chaveiro::Chaveiro;

use crate::coletor::{Pastas, Relatorio, coletar};
use crate::config::{Checkpoints, Config};
use crate::telegram::ExportDoTelegram;
use crate::whatsapp::ponte::{PonteDoWhatsapp, ProcessoExterno};
use crate::whatsapp::sessao::SessaoCifrada;
use crate::{Erro, Resultado};

/// `mensageiros.json`.
pub const ARQUIVO_DA_CONFIG: &str = "mensageiros.json";
/// Os checkpoints.
pub const ARQUIVO_DOS_CHECKPOINTS: &str = "mensageiros-checkpoints.json";

/// O que o ciclo fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumo {
    /// Mensagens vistas.
    pub mensagens: usize,
    /// Origens novas.
    pub novos: usize,
    /// Mensageiros que falharam nesta rodada (o outro segue).
    pub falhas: usize,
}

impl Resumo {
    fn somar(&mut self, r: &Relatorio) {
        self.mensagens += r.mensagens;
        self.novos += usize::try_from(r.contadores.novos).unwrap_or(usize::MAX);
    }
}

/// Roda um ciclo. `chaveiro` abre o chaveiro só se o WhatsApp for coletar (o worker não
/// pede senha: sem chaveiro aberto o WhatsApp falha e o Telegram segue).
///
/// # Errors
/// Falha de configuração, de banco ou de disco. Falha de um mensageiro só conta em
/// [`Resumo::falhas`].
pub fn rodar(
    conexao: &Connection,
    pasta_de_dados: &Path,
    cofre: &Path,
    chaveiro: &dyn Fn() -> Result<Chaveiro, Erro>,
    cancelar: &AtomicBool,
) -> Resultado<Resumo> {
    let config = Config::carregar(&pasta_de_dados.join(ARQUIVO_DA_CONFIG))?;
    let mut resumo = Resumo::default();
    if !config.ligado {
        return Ok(resumo);
    }
    let arquivo_de_pontos = pasta_de_dados.join(ARQUIVO_DOS_CHECKPOINTS);
    let mut pontos = Checkpoints::carregar(&arquivo_de_pontos);
    let pastas = Pastas {
        cofre: cofre.to_path_buf(),
    };

    if config.telegram.ligada
        && !config.telegram.incluidas.is_empty()
        && let Some(pasta) = config.export_do_telegram.as_deref()
    {
        match ExportDoTelegram::abrir(pasta) {
            Ok(mut e) => match coletar(&mut e, conexao, &pastas, &config, &mut pontos, cancelar) {
                Ok(r) => resumo.somar(&r),
                Err(Erro::Banco(e)) => return Err(Erro::Banco(e)),
                Err(_) => resumo.falhas += 1,
            },
            Err(_) => resumo.falhas += 1,
        }
    }

    if config.whatsapp.ligada
        && !config.whatsapp.incluidas.is_empty()
        && let Some(ponte) =
            crate::whatsapp::ponte::programa_da_ponte(config.ponte_do_whatsapp.as_ref())
    {
        let r = (|| -> Resultado<Relatorio> {
            let sessao = SessaoCifrada::nova(pasta_de_dados, chaveiro()?);
            if !sessao.existe() {
                return Err(Erro::NaoPronta);
            }
            let mut p = PonteDoWhatsapp::nova(ProcessoExterno::subir(&ponte)?, sessao);
            let r = coletar(&mut p, conexao, &pastas, &config, &mut pontos, cancelar);
            p.encerrar();
            r
        })();
        match r {
            Ok(r) => resumo.somar(&r),
            Err(Erro::Banco(e)) => return Err(Erro::Banco(e)),
            Err(_) => resumo.falhas += 1,
        }
    }

    pontos.salvar(&arquivo_de_pontos)?;
    Ok(resumo)
}
