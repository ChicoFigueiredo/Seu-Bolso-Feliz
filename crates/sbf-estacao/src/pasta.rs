//! Onde o cache mora: a pasta de dados do app (XDG_DATA_HOME, %APPDATA% ou Application
//! Support), **nunca** dentro do cofre e nunca no `inventario.sqlite` (spec 07, EST-19).

use crate::cache::ARQUIVO_DO_CACHE;
use std::path::{Path, PathBuf};

/// A subpasta dos caches do livro-caixa dentro da pasta de dados do app.
pub const SUBPASTA_DO_LIVRO_CAIXA: &str = "livro-caixa";

/// `<pasta de dados>/livro-caixa/<host>/livro-caixa.sqlite`. O host vira nome de pasta com
/// só `[a-z0-9.-_]` (o `:` da porta vira `_`).
pub fn caminho_do_cache(pasta_de_dados: &Path, host: &str) -> PathBuf {
    let nome: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let nome = if nome.is_empty() || nome.starts_with('.') {
        format!("_{nome}")
    } else {
        nome
    };
    pasta_de_dados
        .join(SUBPASTA_DO_LIVRO_CAIXA)
        .join(nome)
        .join(ARQUIVO_DO_CACHE)
}

/// A pasta de dados padrão do app (a mesma do chaveiro).
///
/// # Errors
/// O sistema não diz onde fica a pasta de dados.
pub fn pasta_de_dados_padrao() -> Result<PathBuf, sbf_chaveiro::ErroDoChaveiro> {
    sbf_chaveiro::pasta_de_dados_do_app()
}

/// Recusa uma pasta de dados dentro do cofre (o cache nunca fica junto dos documentos).
///
/// # Errors
/// [`sbf_chaveiro::ErroDoChaveiro::PastaDentroDoCofre`].
pub fn garantir_fora_do_cofre(
    pasta_de_dados: &Path,
    raiz_do_cofre: &Path,
) -> Result<(), sbf_chaveiro::ErroDoChaveiro> {
    sbf_chaveiro::garantir_fora_do_cofre(pasta_de_dados, raiz_do_cofre)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_cache_fica_na_pasta_de_dados_por_servidor() {
        let c = caminho_do_cache(Path::new("/dados"), "sbf.exemplo.com.br:8443");
        assert_eq!(
            c,
            Path::new("/dados/livro-caixa/sbf.exemplo.com.br_8443/livro-caixa.sqlite")
        );
        let c = caminho_do_cache(Path::new("/dados"), "../../etc");
        assert!(c.starts_with("/dados/livro-caixa/"));
        assert_eq!(c.components().count(), 5);
    }

    #[test]
    fn dentro_do_cofre_e_recusado() {
        assert!(garantir_fora_do_cofre(Path::new("/cofre/dados"), Path::new("/cofre")).is_err());
        assert!(garantir_fora_do_cofre(Path::new("/dados"), Path::new("/cofre")).is_ok());
    }
}
