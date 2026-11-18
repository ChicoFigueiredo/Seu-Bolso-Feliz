//! `worker.json`, na pasta de dados do app (nunca no cofre). Tudo tem padrão: sem arquivo, o
//! worker vigia nada, coleta a nuvem se a janela ligou a coleta e **não** roda a triagem (ela
//! gasta a assinatura; o usuário liga).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Erro, Resultado};

/// Uma pasta vigiada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastaVigiada {
    /// A fonte no inventário (`disco:p`).
    pub fonte: String,
    /// A raiz a vigiar.
    pub raiz: PathBuf,
}

/// A triagem contínua.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigDaTriagem {
    /// O agente roda?
    pub ligada: bool,
    /// Hora local (0 a 23) em que a janela de trabalho começa.
    pub hora_inicial: u32,
    /// Hora local em que termina (exclusiva; 24 = fim do dia). `inicial > final` atravessa a noite.
    pub hora_final: u32,
    /// Itens por lote.
    pub lote: usize,
    /// Minutos entre um lote e outro.
    pub intervalo_minutos: u64,
}

impl Default for ConfigDaTriagem {
    fn default() -> Self {
        Self {
            ligada: false,
            hora_inicial: 0,
            hora_final: 24,
            lote: 25,
            intervalo_minutos: 15,
        }
    }
}

impl ConfigDaTriagem {
    /// A hora local `h` (0 a 23) está dentro da janela de trabalho?
    #[must_use]
    pub fn na_janela(&self, h: u32) -> bool {
        let (i, f) = (self.hora_inicial, self.hora_final);
        if i == f {
            return true;
        }
        if i < f {
            h >= i && h < f
        } else {
            h >= i || h < f
        }
    }
}

/// A configuração do worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// As pastas vigiadas.
    pub pastas: Vec<PastaVigiada>,
    /// Minutos entre as varreduras de reconciliação das pastas.
    pub reconciliar_minutos: u64,
    /// Minutos entre as coletas de nuvem (se a coleta estiver ligada em Opções › Nuvens).
    pub nuvem_minutos: u64,
    /// Minutos entre as coletas do Gmail (anexos e avisos por e-mail). `0` desliga. Só roda se
    /// as credenciais OAuth estão no ambiente do worker.
    pub gmail_minutos: u64,
    /// A triagem contínua.
    pub triagem: ConfigDaTriagem,
    /// Diferença do fuso local para o UTC, em horas (Brasil: -3).
    pub fuso_horas: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pastas: Vec::new(),
            reconciliar_minutos: 30,
            nuvem_minutos: 60,
            gmail_minutos: 30,
            triagem: ConfigDaTriagem::default(),
            fuso_horas: -3,
        }
    }
}

impl Config {
    /// Lê `caminho`; arquivo ausente dá o padrão.
    ///
    /// # Errors
    /// [`Erro::Configuracao`] se o arquivo existe e não é válido.
    pub fn carregar(caminho: &Path) -> Resultado<Self> {
        match std::fs::read_to_string(caminho) {
            Ok(t) => serde_json::from_str(&t).map_err(|_| Erro::Configuracao),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava `caminho` (por arquivo temporário e renomeação).
    ///
    /// # Errors
    /// Falha de disco.
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_string_pretty(self).map_err(|_| Erro::Configuracao)?;
        crate::estado::escrever_atomico(caminho, texto.as_bytes())
    }
}

/// Por que uma pasta não pode ser vigiada.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PastaRecusada {
    /// O caminho não é absoluto.
    #[error("A pasta precisa ser um caminho absoluto.")]
    NaoAbsoluta,
    /// É (ou contém, ou está dentro d)o cofre.
    #[error("O cofre nunca é vigiado.")]
    Cofre,
    /// Área excluída (sistema, lixo, backups...).
    #[error("Esta pasta é uma área excluída ({0}) e não pode ser vigiada.")]
    AreaExcluida(String),
    /// Pasta de outra pessoa.
    #[error("Esta pasta é de outra pessoa e não pode ser vigiada.")]
    OutraPessoa,
    /// Já está na lista.
    #[error("Esta pasta já é vigiada.")]
    Repetida,
}

/// Confere se `raiz` pode ser vigiada: absoluta, fora do cofre, fora das áreas excluídas e
/// das pastas de outras pessoas (`sbf_coleta::areas`).
///
/// # Errors
/// O motivo da recusa.
pub fn validar_pasta(raiz: &Path, cofre: &Path) -> Result<(), PastaRecusada> {
    if !raiz.is_absolute() {
        return Err(PastaRecusada::NaoAbsoluta);
    }
    if raiz.starts_with(cofre) || cofre.starts_with(raiz) {
        return Err(PastaRecusada::Cofre);
    }
    let classe = sbf_coleta::areas::classe_do_caminho(&raiz.to_string_lossy());
    if classe.regra.ignorar || classe.regra.motivo == Some("cofre") {
        return Err(PastaRecusada::Cofre);
    }
    match classe.regra.motivo {
        Some("outra-pessoa") => Err(PastaRecusada::OutraPessoa),
        Some(m) => Err(PastaRecusada::AreaExcluida(m.to_owned())),
        None => Ok(()),
    }
}

impl Config {
    /// Acrescenta a pasta vigiada `raiz` (fonte `disco:<nome>`), depois de validá-la.
    ///
    /// # Errors
    /// O motivo da recusa.
    pub fn adicionar_pasta(&mut self, raiz: &Path, cofre: &Path) -> Result<(), PastaRecusada> {
        validar_pasta(raiz, cofre)?;
        if self.pastas.iter().any(|p| p.raiz == raiz) {
            return Err(PastaRecusada::Repetida);
        }
        let nome = raiz
            .file_name()
            .map_or_else(|| "raiz".to_owned(), |n| n.to_string_lossy().to_lowercase());
        self.pastas.push(PastaVigiada {
            fonte: format!("disco:{nome}"),
            raiz: raiz.to_path_buf(),
        });
        Ok(())
    }

    /// Tira a pasta `raiz` da lista; `false` se não estava.
    pub fn remover_pasta(&mut self, raiz: &Path) -> bool {
        let antes = self.pastas.len();
        self.pastas.retain(|p| p.raiz != raiz);
        self.pastas.len() != antes
    }
}

/// A hora local (0 a 23) de um instante unix.
#[must_use]
pub fn hora_local(agora: u64, fuso_horas: i64) -> u32 {
    let s = i64::try_from(agora).unwrap_or(0) + fuso_horas * 3600;
    u32::try_from(s.rem_euclid(86_400) / 3600).unwrap_or(0)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn janela_de_trabalho_normal_e_que_atravessa_a_noite() {
        let dia = ConfigDaTriagem {
            hora_inicial: 9,
            hora_final: 18,
            ..ConfigDaTriagem::default()
        };
        assert!(dia.na_janela(9) && dia.na_janela(17) && !dia.na_janela(18) && !dia.na_janela(3));
        let noite = ConfigDaTriagem {
            hora_inicial: 22,
            hora_final: 6,
            ..ConfigDaTriagem::default()
        };
        assert!(noite.na_janela(23) && noite.na_janela(2) && !noite.na_janela(12));
        assert!(
            ConfigDaTriagem::default().na_janela(13),
            "padrão: o dia todo"
        );
    }

    #[test]
    fn hora_local_com_fuso() {
        // 2026-11-14 01:00 UTC é 22:00 do dia 13 em UTC-3.
        let t = 1_794_618_000;
        assert_eq!(
            hora_local(t, -3),
            (((t / 3600) % 24) as i64 - 3).rem_euclid(24) as u32
        );
        assert_eq!(hora_local(0, -3), 21);
    }

    #[test]
    fn arquivo_ausente_e_padrao_e_json_ruim_e_erro() {
        let d = tempfile::tempdir().unwrap();
        let c = Config::carregar(&d.path().join("nada.json")).unwrap();
        assert!(!c.triagem.ligada && c.pastas.is_empty());
        std::fs::write(d.path().join("x.json"), "{").unwrap();
        assert!(Config::carregar(&d.path().join("x.json")).is_err());
        std::fs::write(d.path().join("y.json"), r#"{"triagem":{"ligada":true}}"#).unwrap();
        let c = Config::carregar(&d.path().join("y.json")).unwrap();
        assert!(c.triagem.ligada);
        assert_eq!(c.triagem.lote, 25);
    }

    #[test]
    fn pasta_vigiada_recusa_cofre_area_excluida_e_outra_pessoa_e_salva_atomico() {
        let d = tempfile::tempdir().unwrap();
        let cofre = d.path().join("cofre");
        std::fs::create_dir_all(&cofre).unwrap();
        let mut c = Config::default();
        assert_eq!(
            c.adicionar_pasta(&cofre.join("acervo"), &cofre),
            Err(PastaRecusada::Cofre)
        );
        assert_eq!(
            c.adicionar_pasta(Path::new("/mnt/d/Chico/financas"), &cofre),
            Err(PastaRecusada::Cofre)
        );
        assert_eq!(
            c.adicionar_pasta(Path::new("relativa"), &cofre),
            Err(PastaRecusada::NaoAbsoluta)
        );
        assert!(matches!(
            c.adicionar_pasta(Path::new("/mnt/e2/Pessoal/Chico/node_modules"), &cofre),
            Err(PastaRecusada::AreaExcluida(_) | PastaRecusada::OutraPessoa)
        ));
        let boa = d.path().join("Downloads");
        c.adicionar_pasta(&boa, &cofre).unwrap();
        assert_eq!(c.pastas[0].fonte, "disco:downloads");
        assert_eq!(
            c.adicionar_pasta(&boa, &cofre),
            Err(PastaRecusada::Repetida)
        );
        let arq = d.path().join("worker.json");
        c.salvar(&arq).unwrap();
        assert_eq!(Config::carregar(&arq).unwrap(), c);
        assert!(
            std::fs::read_dir(d.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp"))
        );
        assert!(c.remover_pasta(&boa) && !c.remover_pasta(&boa));
    }
}
