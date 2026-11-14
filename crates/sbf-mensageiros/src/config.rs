//! `mensageiros.json` e os checkpoints, na pasta de dados do app (nunca no cofre).
//!
//! **Padrão: nenhuma conversa incluída.** O usuário escolhe, conversa a conversa, em
//! Opções › Mensageiros. Uma conversa excluída vale mais que uma incluída (o "não" ganha).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::modelo::{Checkpoint, Plataforma};
use crate::{Erro, Resultado};

/// Quais conversas de uma plataforma entram.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Escolha {
    /// A plataforma está ligada?
    pub ligada: bool,
    /// Ids das conversas incluídas.
    pub incluidas: Vec<String>,
    /// Ids das conversas excluídas (ganham da inclusão).
    pub excluidas: Vec<String>,
}

impl Escolha {
    /// A conversa entra na coleta? Só se foi incluída e não foi excluída.
    #[must_use]
    pub fn coleta(&self, conversa: &str) -> bool {
        self.ligada
            && self.incluidas.iter().any(|c| c == conversa)
            && !self.excluidas.iter().any(|c| c == conversa)
    }

    /// Inclui (ou tira a inclusão de) uma conversa.
    pub fn incluir(&mut self, conversa: &str, incluir: bool) {
        self.incluidas.retain(|c| c != conversa);
        if incluir {
            self.incluidas.push(conversa.to_owned());
            self.excluidas.retain(|c| c != conversa);
        }
    }
}

/// A configuração dos mensageiros.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Interruptor geral: desligado, o worker não toca em mensageiro nenhum.
    pub ligado: bool,
    /// Minutos entre as coletas.
    pub intervalo_minutos: u64,
    /// O WhatsApp.
    pub whatsapp: Escolha,
    /// O Telegram.
    pub telegram: Escolha,
    /// Pasta do export do Telegram Desktop (onde está o `result.json`).
    pub export_do_telegram: Option<PathBuf>,
    /// O executável da ponte do WhatsApp (plano B do ADR-017). Sem ele, o WhatsApp não coleta.
    pub ponte_do_whatsapp: Option<PathBuf>,
    /// Quantas mensagens, no máximo, por conversa e por rodada.
    pub lote: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ligado: false,
            intervalo_minutos: 30,
            whatsapp: Escolha::default(),
            telegram: Escolha::default(),
            export_do_telegram: None,
            ponte_do_whatsapp: None,
            lote: 500,
        }
    }
}

impl Config {
    /// A escolha de uma plataforma.
    #[must_use]
    pub fn escolha(&self, p: Plataforma) -> &Escolha {
        match p {
            Plataforma::Whatsapp => &self.whatsapp,
            Plataforma::Telegram => &self.telegram,
        }
    }

    /// A escolha de uma plataforma, para mudar.
    pub fn escolha_mut(&mut self, p: Plataforma) -> &mut Escolha {
        match p {
            Plataforma::Whatsapp => &mut self.whatsapp,
            Plataforma::Telegram => &mut self.telegram,
        }
    }

    /// Lê `caminho`; arquivo ausente dá o padrão (tudo desligado, nada incluído).
    ///
    /// # Errors
    /// [`Erro::Formato`] se o arquivo existe e não é válido; falha de disco.
    pub fn carregar(caminho: &Path) -> Resultado<Self> {
        match std::fs::read_to_string(caminho) {
            Ok(t) => serde_json::from_str(&t).map_err(|_| Erro::Formato("mensageiros.json".into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava `caminho` por arquivo temporário e renomeação.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_vec_pretty(self).map_err(|e| Erro::Formato(e.to_string()))?;
        escrever_atomico(caminho, &texto)
    }
}

/// Os checkpoints, por conversa (`whatsapp:<conversa>`), em `mensageiros-checkpoints.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoints {
    #[serde(default)]
    pontos: BTreeMap<String, Checkpoint>,
}

impl Checkpoints {
    fn chave(p: Plataforma, conversa: &str) -> String {
        format!("{}:{conversa}", p.nome())
    }

    /// O checkpoint da conversa (o início, se ela nunca foi lida).
    #[must_use]
    pub fn de(&self, p: Plataforma, conversa: &str) -> Checkpoint {
        self.pontos
            .get(&Self::chave(p, conversa))
            .cloned()
            .unwrap_or_default()
    }

    /// Avança o checkpoint da conversa.
    pub fn avancar(&mut self, p: Plataforma, conversa: &str, ponto: Checkpoint) {
        self.pontos.insert(Self::chave(p, conversa), ponto);
    }

    /// Esquece os checkpoints de uma plataforma (ao desparear: o próximo pareamento começa
    /// do zero; a idempotência por sha256 evita duplicar achados).
    pub fn esquecer(&mut self, p: Plataforma) {
        let prefixo = format!("{}:", p.nome());
        self.pontos.retain(|k, _| !k.starts_with(&prefixo));
    }

    /// Lê `caminho`; ausente ou ilegível dá vazio (perder o checkpoint só custa reler).
    #[must_use]
    pub fn carregar(caminho: &Path) -> Self {
        std::fs::read_to_string(caminho)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Grava `caminho`.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_vec(self).map_err(|e| Erro::Formato(e.to_string()))?;
        escrever_atomico(caminho, &texto)
    }
}

/// Escreve por arquivo temporário e renomeação, com permissão 0600.
pub(crate) fn escrever_atomico(caminho: &Path, bytes: &[u8]) -> Resultado<()> {
    use std::io::Write as _;
    if let Some(pasta) = caminho.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
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
        f.flush()?;
    }
    std::fs::rename(&tmp, caminho)?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;

    #[test]
    fn padrao_nao_inclui_nenhuma_conversa() {
        let c = Config::default();
        assert!(!c.ligado);
        assert!(!c.whatsapp.coleta("123"));
        let mut e = Escolha {
            ligada: true,
            ..Escolha::default()
        };
        assert!(!e.coleta("123"), "ligada sem incluir nada não coleta nada");
        e.incluir("123", true);
        assert!(e.coleta("123") && !e.coleta("999"));
    }

    #[test]
    fn exclusao_ganha_da_inclusao_e_incluir_tira_a_exclusao() {
        let mut e = Escolha {
            ligada: true,
            incluidas: vec!["a".into()],
            excluidas: vec!["a".into()],
        };
        assert!(!e.coleta("a"));
        e.incluir("a", true);
        assert!(e.coleta("a"));
        e.incluir("a", false);
        assert!(!e.coleta("a"));
    }

    #[test]
    fn config_e_checkpoints_vao_e_voltam() {
        let d = tempfile::tempdir().unwrap();
        let arq = d.path().join("m.json");
        assert_eq!(Config::carregar(&arq).unwrap(), Config::default());
        let mut c = Config::default();
        c.ligado = true;
        c.telegram.incluir("77", true);
        c.salvar(&arq).unwrap();
        assert_eq!(Config::carregar(&arq).unwrap(), c);
        std::fs::write(&arq, "{").unwrap();
        assert!(Config::carregar(&arq).is_err());

        let arq = d.path().join("p.json");
        let mut p = Checkpoints::default();
        p.avancar(
            Plataforma::Whatsapp,
            "x",
            Checkpoint {
                quando: 5,
                id: Some("m1".into()),
            },
        );
        p.salvar(&arq).unwrap();
        let p2 = Checkpoints::carregar(&arq);
        assert_eq!(p2.de(Plataforma::Whatsapp, "x").quando, 5);
        assert_eq!(p2.de(Plataforma::Telegram, "x"), Checkpoint::default());
        let mut p3 = p2;
        p3.esquecer(Plataforma::Whatsapp);
        assert_eq!(p3.de(Plataforma::Whatsapp, "x"), Checkpoint::default());
    }
}
