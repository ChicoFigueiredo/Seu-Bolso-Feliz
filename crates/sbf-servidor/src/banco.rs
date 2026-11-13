//! O Postgres: o pool de conexões e a leitura das linhas.
//!
//! A URL do banco chega por variável de ambiente (`SBF_DATABASE_URL`) e vive só aqui,
//! em `Zeroizing`, fora de `Debug`, de log e de mensagem de erro. Com `sslmode=disable`
//! a conexão é sem TLS (o Postgres de teste em 127.0.0.1); com qualquer outro, rustls
//! com as raízes do sistema (o Neon exige TLS).

use std::str::FromStr;
use std::sync::Arc;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::Row;
use tokio_postgres::config::SslMode;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::dinheiro::centavos_do_numeric;
use sbf_contrato::{Centavos, Versao};

/// Falha ao abrir o pool. A mensagem nunca leva a URL.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeConexao {
    #[error("a URL do banco não é válida")]
    UrlInvalida,
    #[error("não deu para montar o TLS do banco: {0}")]
    Tls(String),
    #[error("não deu para montar o pool: {0}")]
    Pool(String),
}

/// Abre o pool de conexões (até `maximo` conexões).
pub fn abrir_pool(url: &Zeroizing<String>, maximo: usize) -> Result<Pool, ErroDeConexao> {
    let config = tokio_postgres::Config::from_str(url).map_err(|_| ErroDeConexao::UrlInvalida)?;
    let gerente = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = if config.get_ssl_mode() == SslMode::Disable {
        Manager::from_config(config, tokio_postgres::NoTls, gerente)
    } else {
        Manager::from_config(config, tls()?, gerente)
    };
    Pool::builder(manager)
        .max_size(maximo)
        .build()
        .map_err(|e| ErroDeConexao::Pool(e.to_string()))
}

fn tls() -> Result<tokio_postgres_rustls::MakeRustlsConnect, ErroDeConexao> {
    let mut raizes = rustls::RootCertStore::empty();
    let nativas = rustls_native_certs::load_native_certs();
    for cert in nativas.certs {
        // Um certificado do sistema que o rustls não entende fica de fora.
        let _ = raizes.add(cert);
    }
    if raizes.is_empty() {
        return Err(ErroDeConexao::Tls(
            "nenhum certificado raiz no sistema".into(),
        ));
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| ErroDeConexao::Tls(e.to_string()))?
    .with_root_certificates(raizes)
    .with_no_client_auth();
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

// ───────────────────────────── trechos de SQL ─────────────────────────────

/// A versão de uma linha: `updated_at` em microssegundos desde a época, como texto.
/// Exato: `extract(epoch …)` é `numeric` desde o Postgres 14.
macro_rules! sql_versao {
    ($coluna:literal) => {
        concat!(
            "(extract(epoch from ",
            $coluna,
            ") * 1000000)::bigint::text"
        )
    };
}
pub(crate) use sql_versao;

/// Um instante em RFC 3339 UTC com microssegundos.
macro_rules! sql_instante {
    ($coluna:literal) => {
        concat!(
            "to_char(",
            $coluna,
            " AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')"
        )
    };
}
pub(crate) use sql_instante;

/// Uma data em `AAAA-MM-DD`.
macro_rules! sql_data {
    ($coluna:literal) => {
        concat!("to_char(", $coluna, ", 'YYYY-MM-DD')")
    };
}
pub(crate) use sql_data;

// ───────────────────────────── leitura de linhas ─────────────────────────────

/// Um valor do banco que o contrato não sabe representar.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeLeitura {
    #[error("coluna {coluna}: {detalhe}")]
    Coluna {
        coluna: &'static str,
        detalhe: String,
    },
    #[error("coluna {coluna}: valor fora do contrato: {valor:?}")]
    ValorDesconhecido { coluna: &'static str, valor: String },
}

/// Lê colunas de uma linha com a mensagem certa quando falha.
pub(crate) struct Leitor<'a>(pub &'a Row);

impl Leitor<'_> {
    pub fn valor<'b, T>(&'b self, coluna: &'static str) -> Result<T, ErroDeLeitura>
    where
        T: tokio_postgres::types::FromSql<'b>,
    {
        self.0
            .try_get::<_, T>(coluna)
            .map_err(|e| ErroDeLeitura::Coluna {
                coluna,
                detalhe: e.to_string(),
            })
    }

    pub fn id(&self, coluna: &'static str) -> Result<Uuid, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn id_opc(&self, coluna: &'static str) -> Result<Option<Uuid>, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn texto(&self, coluna: &'static str) -> Result<String, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn texto_opc(&self, coluna: &'static str) -> Result<Option<String>, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn booleano(&self, coluna: &'static str) -> Result<bool, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn inteiro_opc(&self, coluna: &'static str) -> Result<Option<i32>, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn inteiro(&self, coluna: &'static str) -> Result<i32, ErroDeLeitura> {
        self.valor(coluna)
    }

    pub fn ids(&self, coluna: &'static str) -> Result<Vec<Uuid>, ErroDeLeitura> {
        self.valor(coluna)
    }

    /// Dinheiro: a coluna vem como texto (`::text` no SQL).
    pub fn centavos(&self, coluna: &'static str) -> Result<Centavos, ErroDeLeitura> {
        let texto: String = self.valor(coluna)?;
        centavos_do_numeric(&texto).map_err(|e| ErroDeLeitura::Coluna {
            coluna,
            detalhe: e.to_string(),
        })
    }

    pub fn centavos_opc(&self, coluna: &'static str) -> Result<Option<Centavos>, ErroDeLeitura> {
        let texto: Option<String> = self.valor(coluna)?;
        texto
            .map(|t| centavos_do_numeric(&t))
            .transpose()
            .map_err(|e| ErroDeLeitura::Coluna {
                coluna,
                detalhe: e.to_string(),
            })
    }

    pub fn versao(&self) -> Result<Versao, ErroDeLeitura> {
        Ok(Versao(self.texto("versao")?))
    }

    pub fn versao_opc(&self) -> Result<Option<Versao>, ErroDeLeitura> {
        Ok(self.texto_opc("versao")?.map(Versao))
    }

    /// Um valor fechado do banco, traduzido para o enum do contrato.
    pub fn fechado<E>(
        &self,
        coluna: &'static str,
        traduzir: fn(&str) -> Option<E>,
    ) -> Result<E, ErroDeLeitura> {
        let texto = self.texto(coluna)?;
        traduzir(&texto).ok_or(ErroDeLeitura::ValorDesconhecido {
            coluna,
            valor: texto,
        })
    }

    pub fn fechado_opc<E>(
        &self,
        coluna: &'static str,
        traduzir: fn(&str) -> Option<E>,
    ) -> Result<Option<E>, ErroDeLeitura> {
        match self.texto_opc(coluna)? {
            None => Ok(None),
            Some(texto) => traduzir(&texto)
                .map(Some)
                .ok_or(ErroDeLeitura::ValorDesconhecido {
                    coluna,
                    valor: texto,
                }),
        }
    }
}
