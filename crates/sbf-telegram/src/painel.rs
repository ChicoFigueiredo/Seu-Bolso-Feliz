//! O que a janela mostra do Telegram: lê só o que existe (config, estado, lista de conversas
//! e se há sessão no chaveiro), sem rede e sem abrir conexão.

use std::path::Path;

use sbf_chaveiro::Chaveiro;

use crate::config::{ARQUIVO_DA_CONFIG, ARQUIVO_DAS_CONVERSAS, Config, ListaDeConversas};
use crate::estado::{ARQUIVO_DO_ESTADO, Estado};
use crate::modelo::Conversa;
use crate::{Erro, sessao};

/// Em que pé está a conexão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    /// Há sessão guardada e a última coleta não foi recusada.
    Conectado,
    /// Sem sessão, ou o Telegram a recusou: rode `sbf telegram entrar` no terminal.
    PrecisaEntrar,
    /// Não deu para saber (chaveiro fechado).
    Desconhecida,
}

impl Situacao {
    /// Para o selo da janela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Conectado => "conectado",
            Self::PrecisaEntrar => "precisa entrar",
            Self::Desconhecida => "chaveiro fechado",
        }
    }
}

/// Uma conversa seguida e o que já entrou dela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seguida {
    /// A conversa.
    pub conversa: Conversa,
    /// Arquivos novos que já entraram.
    pub arquivos: u64,
    /// Última coleta (ISO 8601 UTC).
    pub ultima_coleta: Option<String>,
}

/// O painel do Telegram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Painel {
    /// A situação da conexão.
    pub situacao: Situacao,
    /// As conversas seguidas.
    pub seguidas: Vec<Seguida>,
    /// Última coleta de qualquer conversa.
    pub ultima_coleta: Option<String>,
    /// Arquivos novos de todas as conversas.
    pub arquivos: u64,
    /// O que deu errado na última rodada.
    pub ultimo_erro: Option<String>,
    /// A última lista de conversas (para escolher quais seguir, sem rede).
    pub conhecidas: Vec<Conversa>,
}

impl Painel {
    /// Lê da pasta de dados do app. `chaveiro` é só para saber se há sessão.
    #[must_use]
    pub fn ler(pasta_de_dados: &Path, chaveiro: Option<&Chaveiro>) -> Self {
        let config = Config::carregar(&pasta_de_dados.join(ARQUIVO_DA_CONFIG)).unwrap_or_default();
        let estado = Estado::carregar(&pasta_de_dados.join(ARQUIVO_DO_ESTADO));
        let lista = ListaDeConversas::carregar(&pasta_de_dados.join(ARQUIVO_DAS_CONVERSAS));
        let recusada = estado
            .ultimo_erro
            .as_deref()
            .is_some_and(|e| e == Erro::PrecisaEntrar.to_string());
        let situacao = match chaveiro {
            None => Situacao::Desconhecida,
            Some(c) if sessao::existe_no_chaveiro(c) && !recusada => Situacao::Conectado,
            Some(_) => Situacao::PrecisaEntrar,
        };
        let seguidas = config
            .seguidas
            .iter()
            .map(|c| {
                let e = estado.conversas.get(&c.id);
                Seguida {
                    conversa: c.clone(),
                    arquivos: e.map_or(0, |e| e.arquivos),
                    ultima_coleta: e.and_then(|e| e.ultima_coleta.clone()),
                }
            })
            .collect();
        Self {
            situacao,
            seguidas,
            ultima_coleta: estado.ultima_coleta.clone(),
            arquivos: estado.arquivos(),
            ultimo_erro: estado.ultimo_erro.clone(),
            conhecidas: lista.conversas,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use crate::modelo::Tipo;

    fn conv(id: &str) -> Conversa {
        Conversa {
            id: id.into(),
            nome: format!("C{id}"),
            tipo: Tipo::Grupo,
        }
    }

    #[test]
    fn sem_nada_o_painel_e_vazio_e_sem_chaveiro_nao_sabe() {
        let d = tempfile::tempdir().unwrap();
        let p = Painel::ler(d.path(), None);
        assert_eq!(p.situacao, Situacao::Desconhecida);
        assert!(p.seguidas.is_empty() && p.conhecidas.is_empty());
        assert_eq!((p.arquivos, p.ultima_coleta), (0, None));
    }

    #[test]
    fn junta_config_estado_e_lista() {
        let d = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.seguir(&conv("-1"));
        cfg.salvar(&d.path().join(ARQUIVO_DA_CONFIG)).unwrap();
        let mut est = Estado::default();
        est.conversas.entry("-1".into()).or_default().arquivos = 3;
        est.conversas.entry("-1".into()).or_default().ultima_coleta =
            Some("2026-11-18T10:00:00Z".into());
        est.ultima_coleta = Some("2026-11-18T10:00:00Z".into());
        est.salvar(&d.path().join(ARQUIVO_DO_ESTADO)).unwrap();
        ListaDeConversas {
            atualizada_em: None,
            conversas: vec![conv("-1"), conv("-2")],
        }
        .salvar(&d.path().join(ARQUIVO_DAS_CONVERSAS))
        .unwrap();
        let p = Painel::ler(d.path(), None);
        assert_eq!(p.seguidas.len(), 1);
        assert_eq!(p.seguidas[0].arquivos, 3);
        assert_eq!(p.arquivos, 3);
        assert_eq!(p.conhecidas.len(), 2);
    }
}
