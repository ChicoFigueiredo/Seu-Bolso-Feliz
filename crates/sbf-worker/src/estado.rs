//! `worker-estado.json`: o que o worker escreve e a janela lê para a barra de status
//! (rodando ou parado, última coleta, fila da triagem). Escrita atômica (arquivo temporário
//! e renomeação): a janela nunca lê um arquivo pela metade. Só números, instantes e rótulos
//! fixos; nada de conteúdo.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::Resultado;

/// Depois de quantos segundos sem atualizar o worker é dado como parado.
pub const VALIDADE_S: u64 = 45;

/// Segundos unix agora.
#[must_use]
pub fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A última coleta que trouxe algo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UltimaColeta {
    /// Quando (segundos unix).
    pub quando: u64,
    /// De onde: `pastas`, `nuvem`, `mensageiros` ou `telegram`.
    pub origem: String,
    /// Quantas origens novas.
    pub novos: u64,
}

/// O estado do worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EstadoDoWorker {
    /// O processo.
    pub pid: u32,
    /// A versão do binário.
    pub versao: String,
    /// Quando subiu.
    pub iniciado_em: u64,
    /// Última escrita deste arquivo.
    pub atualizado_em: u64,
    /// Pastas vigiadas.
    pub vigiando: usize,
    /// A última coleta com novidade.
    pub ultima_coleta: Option<UltimaColeta>,
    /// Itens na fila da triagem.
    pub fila_triagem: usize,
    /// Quando rodou o último lote da triagem.
    pub ultima_triagem: Option<u64>,
    /// O agente de triagem está ligado?
    pub triagem_ligada: bool,
    /// A coleta de nuvem está ligada?
    pub nuvem_ligada: bool,
    /// A coleta de mensageiros está ligada?
    pub mensageiros_ligados: bool,
    /// Quantas contas de e-mail estão com a autorização vencida (`invalid_grant`): o worker não
    /// tenta essas até a pessoa entrar de novo (`sbf email entrar` ou Conexões › E-mail); as
    /// outras contas seguem coletando.
    pub gmail_contas_vencidas: u32,
    /// Falhas desde que subiu.
    pub falhas: u32,
}

/// Escreve `bytes` em `caminho` por arquivo temporário e renomeação.
///
/// # Errors
/// Falha de disco.
pub fn escrever_atomico(caminho: &Path, bytes: &[u8]) -> Resultado<()> {
    use std::io::Write as _;
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, caminho)?;
    Ok(())
}

impl EstadoDoWorker {
    /// Grava em `caminho`, atualizando `atualizado_em`.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn gravar(&mut self, caminho: &Path) -> Resultado<()> {
        self.atualizado_em = agora();
        let texto = serde_json::to_vec_pretty(self).map_err(|_| crate::Erro::Configuracao)?;
        escrever_atomico(caminho, &texto)
    }

    /// Lê `caminho`; `None` se não existe ou não é válido.
    #[must_use]
    pub fn ler(caminho: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(caminho).ok()?).ok()
    }

    /// O worker está rodando agora? O arquivo é recente **e** (no Linux) o processo existe.
    #[must_use]
    pub fn rodando(&self, agora: u64) -> bool {
        if agora.saturating_sub(self.atualizado_em) > VALIDADE_S {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            Path::new(&format!("/proc/{}", self.pid)).exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }
}

/// "há 5 min", "há 2 h", "agora".
#[must_use]
pub fn ha_quanto_tempo(quando: u64, agora: u64) -> String {
    let s = agora.saturating_sub(quando);
    match s {
        0..=59 => "agora".to_owned(),
        60..=3599 => format!("há {} min", s / 60),
        3600..=86_399 => format!("há {} h", s / 3600),
        _ => format!("há {} d", s / 86_400),
    }
}

/// O texto do segmento da barra de status: `Worker: rodando · coleta há 5 min · fila 12`
/// ou `Worker: parado`.
#[must_use]
pub fn texto_da_barra(estado: Option<&EstadoDoWorker>, agora: u64) -> String {
    let Some(e) = estado.filter(|e| e.rodando(agora)) else {
        return "Worker: parado".to_owned();
    };
    let coleta = e.ultima_coleta.as_ref().map_or_else(
        || "sem coleta ainda".to_owned(),
        |c| format!("coleta {}", ha_quanto_tempo(c.quando, agora)),
    );
    let fila = if e.triagem_ligada {
        format!(" · fila {}", e.fila_triagem)
    } else {
        String::new()
    };
    let gmail = match e.gmail_contas_vencidas {
        0 => String::new(),
        1 => " · E-mail: 1 conta com autorização vencida".to_owned(),
        n => format!(" · E-mail: {n} contas com autorização vencida"),
    };
    format!("Worker: rodando · {coleta}{fila}{gmail}")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn grava_le_e_diz_se_esta_rodando() {
        let d = tempfile::tempdir().unwrap();
        let c = d.path().join("worker-estado.json");
        let mut e = EstadoDoWorker {
            pid: std::process::id(),
            triagem_ligada: true,
            fila_triagem: 12,
            ..EstadoDoWorker::default()
        };
        e.gravar(&c).unwrap();
        let lido = EstadoDoWorker::ler(&c).unwrap();
        assert_eq!(lido, e);
        assert!(lido.rodando(agora()));
        assert!(
            !lido.rodando(agora() + VALIDADE_S + 5),
            "arquivo velho = parado"
        );
        assert!(EstadoDoWorker::ler(&d.path().join("nao.json")).is_none());
        // Sem processo vivo, mesmo recente, está parado (Linux).
        #[cfg(target_os = "linux")]
        {
            let morto = EstadoDoWorker {
                pid: u32::MAX - 1,
                atualizado_em: agora(),
                ..EstadoDoWorker::default()
            };
            assert!(!morto.rodando(agora()));
        }
    }

    #[test]
    fn a_barra_avisa_quando_a_autorizacao_de_contas_de_email_venceu() {
        let agora_ = agora();
        let e = EstadoDoWorker {
            pid: std::process::id(),
            atualizado_em: agora_,
            gmail_contas_vencidas: 1,
            ..EstadoDoWorker::default()
        };
        assert!(
            texto_da_barra(Some(&e), agora_).ends_with("E-mail: 1 conta com autorização vencida")
        );
        let dois = EstadoDoWorker {
            gmail_contas_vencidas: 2,
            ..e
        };
        assert!(
            texto_da_barra(Some(&dois), agora_)
                .ends_with("E-mail: 2 contas com autorização vencida")
        );
    }

    #[test]
    fn texto_do_segmento_da_barra() {
        let agora_ = agora();
        assert_eq!(texto_da_barra(None, agora_), "Worker: parado");
        let e = EstadoDoWorker {
            pid: std::process::id(),
            atualizado_em: agora_,
            ultima_coleta: Some(UltimaColeta {
                quando: agora_ - 300,
                origem: "pastas".into(),
                novos: 3,
            }),
            triagem_ligada: true,
            fila_triagem: 12,
            ..EstadoDoWorker::default()
        };
        assert_eq!(
            texto_da_barra(Some(&e), agora_),
            "Worker: rodando · coleta há 5 min · fila 12"
        );
        let sem = EstadoDoWorker {
            triagem_ligada: false,
            ultima_coleta: None,
            ..e
        };
        assert_eq!(
            texto_da_barra(Some(&sem), agora_),
            "Worker: rodando · sem coleta ainda"
        );
    }
}
