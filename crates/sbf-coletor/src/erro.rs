//! Os erros das coletas. Nenhuma mensagem leva segredo: só o serviço, o status HTTP e um
//! trecho curto do corpo de erro que o próprio serviço devolveu.

/// Falha de uma coleta.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeColeta {
    /// Variável de ambiente ausente ou valor inválido.
    #[error("configuração: {0}")]
    Configuracao(String),
    /// O Postgres recusou ou não respondeu.
    #[error("banco: {0}")]
    Banco(String),
    /// A rede falhou antes de haver resposta.
    #[error("{servico}: falha de rede: {detalhe}")]
    Rede {
        servico: &'static str,
        detalhe: String,
    },
    /// O serviço respondeu com status de erro.
    #[error("{servico}: HTTP {status} em {caminho}: {corpo}")]
    Api {
        servico: &'static str,
        status: u16,
        caminho: String,
        corpo: String,
    },
    /// A resposta veio, mas não tem a forma esperada.
    #[error("{servico}: resposta inesperada: {detalhe}")]
    Resposta {
        servico: &'static str,
        detalhe: String,
    },
    /// O armazenamento de blobs recusou o arquivo.
    #[error("armazenamento: {0}")]
    Armazenamento(String),
    /// Dado de entrada inválido (data, argumento).
    #[error("{0}")]
    Entrada(String),
}

impl From<tokio_postgres::Error> for ErroDeColeta {
    fn from(e: tokio_postgres::Error) -> Self {
        // `Display` do erro do driver só diz "db error"; a mensagem do servidor está na fonte.
        match e.as_db_error() {
            Some(db) => Self::Banco(format!("{} ({})", db.message(), db.code().code())),
            None => Self::Banco(e.to_string()),
        }
    }
}

impl From<deadpool_postgres::PoolError> for ErroDeColeta {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::Banco(e.to_string())
    }
}

/// Corta o corpo de erro de um serviço para caber numa linha de log.
pub(crate) fn aparar(corpo: &str) -> String {
    const LIMITE: usize = 300;
    let limpo = corpo.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpo.chars().count() <= LIMITE {
        limpo
    } else {
        format!("{}…", limpo.chars().take(LIMITE).collect::<String>())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn corpo_longo_e_cortado_e_sem_quebras() {
        let corpo = format!("a\n{}", "b".repeat(500));
        let c = aparar(&corpo);
        assert!(c.chars().count() <= 301);
        assert!(!c.contains('\n'));
    }
}
