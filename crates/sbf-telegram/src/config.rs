//! `telegram.json`: as conversas **seguidas** (padrão: nenhuma). Só dado do usuário, nunca
//! segredo: a sessão fica no chaveiro e `TELEGRAM_API_*` no ambiente.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::arquivos::escrever_atomico;
use crate::modelo::Conversa;
use crate::{Erro, Resultado};

/// O arquivo da configuração, na pasta de dados do app.
pub const ARQUIVO_DA_CONFIG: &str = "telegram.json";
/// O arquivo da última lista de conversas (para a janela escolher sem rede).
pub const ARQUIVO_DAS_CONVERSAS: &str = "telegram-conversas.json";

/// A configuração do Telegram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// As conversas seguidas (o nome é o da hora em que passou a seguir).
    pub seguidas: Vec<Conversa>,
    /// Quantas mídias, no máximo, por conversa e por rodada.
    pub lote: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            seguidas: Vec::new(),
            lote: 200,
        }
    }
}

impl Config {
    /// Lê `caminho`; ausente dá o padrão (nada seguido).
    ///
    /// # Errors
    /// [`Erro::Formato`] se o arquivo existe e não é válido.
    pub fn carregar(caminho: &Path) -> Resultado<Self> {
        match std::fs::read_to_string(caminho) {
            Ok(t) => serde_json::from_str(&t).map_err(|_| Erro::Formato(ARQUIVO_DA_CONFIG.into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava `caminho`.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_vec_pretty(self).map_err(|e| Erro::Formato(e.to_string()))?;
        escrever_atomico(caminho, &texto)
    }

    /// A conversa é seguida?
    #[must_use]
    pub fn segue(&self, id: &str) -> bool {
        self.seguidas.iter().any(|c| c.id == id)
    }

    /// Passa a seguir (idempotente). Devolve `true` se mudou.
    pub fn seguir(&mut self, conversa: &Conversa) -> bool {
        if self.segue(&conversa.id) {
            return false;
        }
        self.seguidas.push(conversa.clone());
        true
    }

    /// Deixa de seguir. Devolve `true` se mudou.
    pub fn deixar(&mut self, id: &str) -> bool {
        let antes = self.seguidas.len();
        self.seguidas.retain(|c| c.id != id);
        self.seguidas.len() != antes
    }
}

/// A última lista de conversas que o Telegram devolveu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListaDeConversas {
    /// Quando foi lida (ISO 8601 UTC).
    pub atualizada_em: Option<String>,
    /// As conversas.
    pub conversas: Vec<Conversa>,
}

impl ListaDeConversas {
    /// Lê `caminho`; ausente ou ilegível dá vazio (só custa listar de novo).
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

/// Acha a conversa por id exato, nome exato (sem diferenciar maiúsculas) ou pedaço único do
/// nome.
///
/// # Errors
/// [`Erro::ConversaDesconhecida`] ou [`Erro::ConversaAmbigua`].
pub fn resolver<'a>(conversas: &'a [Conversa], pedido: &str) -> Resultado<&'a Conversa> {
    let pedido = pedido.trim();
    if let Some(c) = conversas.iter().find(|c| c.id == pedido) {
        return Ok(c);
    }
    let minusculo = pedido.to_lowercase();
    let exatas: Vec<&Conversa> = conversas
        .iter()
        .filter(|c| c.nome.to_lowercase() == minusculo)
        .collect();
    match exatas.as_slice() {
        [c] => return Ok(c),
        [] => {}
        varias => return Err(Erro::ConversaAmbigua(varias.len())),
    }
    if minusculo.is_empty() {
        return Err(Erro::ConversaDesconhecida);
    }
    let parciais: Vec<&Conversa> = conversas
        .iter()
        .filter(|c| c.nome.to_lowercase().contains(&minusculo))
        .collect();
    match parciais.as_slice() {
        [c] => Ok(c),
        [] => Err(Erro::ConversaDesconhecida),
        varias => Err(Erro::ConversaAmbigua(varias.len())),
    }
}

/// O nome seguro de pasta de uma conversa: `<nome limpo>-<id sem sinal>`. Estável: vem do
/// nome guardado ao seguir.
#[must_use]
pub fn pasta_da_conversa(raiz: &Path, conversa: &Conversa) -> PathBuf {
    let limpo: String = conversa
        .nome
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(40)
        .collect();
    let limpo = limpo.trim_matches('_');
    let id = conversa.id.trim_start_matches('-');
    let nome = if limpo.is_empty() {
        format!("c{id}")
    } else {
        format!("{limpo}-{id}")
    };
    raiz.join(nome)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use crate::modelo::Tipo;

    fn c(id: &str, nome: &str) -> Conversa {
        Conversa {
            id: id.into(),
            nome: nome.into(),
            tipo: Tipo::Grupo,
        }
    }

    #[test]
    fn seguir_e_deixar_sao_idempotentes_e_vao_e_voltam() {
        let d = tempfile::tempdir().unwrap();
        let arq = d.path().join(ARQUIVO_DA_CONFIG);
        assert_eq!(Config::carregar(&arq).unwrap(), Config::default());
        let mut cfg = Config::default();
        assert!(cfg.seguir(&c("-100", "Contas")));
        assert!(!cfg.seguir(&c("-100", "Contas")));
        cfg.salvar(&arq).unwrap();
        let lida = Config::carregar(&arq).unwrap();
        assert!(lida.segue("-100") && !lida.segue("5"));
        let mut lida = lida;
        assert!(lida.deixar("-100"));
        assert!(!lida.deixar("-100"));
        std::fs::write(&arq, "{").unwrap();
        assert!(Config::carregar(&arq).is_err());
    }

    #[test]
    fn resolve_por_id_nome_e_pedaco() {
        let lista = vec![
            c("-1", "Contas da Casa"),
            c("-2", "Trabalho"),
            c("7", "Contas"),
        ];
        assert_eq!(resolver(&lista, "-2").unwrap().nome, "Trabalho");
        assert_eq!(resolver(&lista, "contas").unwrap().id, "7");
        assert_eq!(resolver(&lista, "traba").unwrap().id, "-2");
        assert!(matches!(
            resolver(&lista, "casa").map(|c| c.id.as_str()),
            Ok("-1")
        ));
        assert!(matches!(
            resolver(&lista, "a"),
            Err(Erro::ConversaAmbigua(3))
        ));
        assert!(matches!(
            resolver(&lista, "zzz"),
            Err(Erro::ConversaDesconhecida)
        ));
    }

    #[test]
    fn pasta_da_conversa_e_segura_e_estavel() {
        let raiz = Path::new("/cofre/entrada/telegram");
        let p = pasta_da_conversa(raiz, &c("-1001", "../Contas / Casa"));
        assert_eq!(p, raiz.join("Contas___Casa-1001"));
        assert_eq!(pasta_da_conversa(raiz, &c("55", "///")), raiz.join("c55"));
    }
}
