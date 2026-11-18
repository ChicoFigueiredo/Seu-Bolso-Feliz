//! `TELEGRAM_API_ID` e `TELEGRAM_API_HASH`: as credenciais do **aplicativo** (de
//! `my.telegram.org`), não da conta. Vêm, nesta ordem, do ambiente, do `.env` da raiz do
//! repositório (só estas duas chaves são lidas) e do chaveiro (onde `sbf telegram entrar`
//! as deixa, para o worker, que roda sem `.env`, também achá-las).
//!
//! Nada daqui vai para log, `Debug` ou mensagem de erro.

use std::path::{Path, PathBuf};

use sbf_chaveiro::{Chaveiro, Segredo};
use zeroize::Zeroizing;

use crate::sessao::NOME_DAS_CREDENCIAIS;
use crate::{Erro, Resultado};

/// O nome da variável do id.
pub const VARIAVEL_DO_ID: &str = "TELEGRAM_API_ID";
/// O nome da variável do hash.
pub const VARIAVEL_DO_HASH: &str = "TELEGRAM_API_HASH";

/// As credenciais do aplicativo.
#[derive(Clone)]
pub struct Credenciais {
    /// O id.
    pub api_id: i32,
    /// O hash (segredo).
    pub api_hash: Zeroizing<String>,
}

impl std::fmt::Debug for Credenciais {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credenciais").finish_non_exhaustive()
    }
}

impl Credenciais {
    fn de_texto(id: &str, hash: &str) -> Option<Self> {
        let api_id = id.trim().parse::<i32>().ok().filter(|i| *i > 0)?;
        let hash = hash.trim();
        (!hash.is_empty()).then(|| Self {
            api_id,
            api_hash: Zeroizing::new(hash.to_owned()),
        })
    }

    /// Procura no ambiente (`variavel`), nos `.env` de `raizes` e, por último, no chaveiro.
    ///
    /// # Errors
    /// [`Erro::SemCredenciais`] se nenhuma fonte tem as duas.
    pub fn resolver(
        variavel: &dyn Fn(&str) -> Option<String>,
        raizes: &[PathBuf],
        chaveiro: Option<&Chaveiro>,
    ) -> Resultado<Self> {
        if let (Some(id), Some(hash)) = (variavel(VARIAVEL_DO_ID), variavel(VARIAVEL_DO_HASH))
            && let Some(c) = Self::de_texto(&id, &hash)
        {
            return Ok(c);
        }
        for raiz in raizes {
            if let Ok(texto) = std::fs::read_to_string(raiz.join(".env")) {
                let texto = Zeroizing::new(texto);
                if let (Some(id), Some(hash)) = (
                    valor_do_env(&texto, VARIAVEL_DO_ID),
                    valor_do_env(&texto, VARIAVEL_DO_HASH),
                ) && let Some(c) = Self::de_texto(&id, &hash)
                {
                    return Ok(c);
                }
            }
        }
        if let Some(c) = chaveiro
            && c.tem_segredo_de_mensageiro(NOME_DAS_CREDENCIAIS)
            && let Ok(s) = c.segredo_de_mensageiro(NOME_DAS_CREDENCIAIS)
            && let Some((id, hash)) = s.expor().split_once(':')
            && let Some(c) = Self::de_texto(id, hash)
        {
            return Ok(c);
        }
        Err(Erro::SemCredenciais)
    }

    /// Do ambiente real: as variáveis do processo e o `.env` do diretório atual e dos de
    /// cima (a raiz do repositório).
    ///
    /// # Errors
    /// Como [`Self::resolver`].
    pub fn do_ambiente(chaveiro: Option<&Chaveiro>) -> Resultado<Self> {
        let mut raizes: Vec<PathBuf> = Vec::new();
        if let Ok(atual) = std::env::current_dir() {
            raizes.extend(atual.ancestors().take(6).map(Path::to_path_buf));
        }
        Self::resolver(&|n| std::env::var(n).ok(), &raizes, chaveiro)
    }

    /// Deixa as credenciais no chaveiro, para o worker (que não tem `.env`) as achar.
    ///
    /// # Errors
    /// [`Erro::Chaveiro`] se ele recusar.
    pub fn guardar(&self, chaveiro: &Chaveiro) -> Resultado<()> {
        chaveiro
            .guardar_segredo_de_mensageiro(
                NOME_DAS_CREDENCIAIS,
                Segredo::novo(format!("{}:{}", self.api_id, self.api_hash.as_str())),
            )
            .map_err(|e| Erro::Chaveiro(e.to_string()))
    }
}

/// O valor de `chave` num texto no formato `.env` (`CHAVE=valor`, `export CHAVE="valor"`,
/// `#` comenta). Só procura a chave pedida.
#[must_use]
pub fn valor_do_env(texto: &str, chave: &str) -> Option<String> {
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.starts_with('#') {
            continue;
        }
        let linha = linha.strip_prefix("export ").unwrap_or(linha).trim_start();
        let Some((k, v)) = linha.split_once('=') else {
            continue;
        };
        if k.trim() != chave {
            continue;
        }
        let v = v.trim();
        let v = if let Some(entre) = v
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .map(|(a, _)| a)
        {
            entre
        } else if let Some(entre) = v
            .strip_prefix('\'')
            .and_then(|r| r.split_once('\''))
            .map(|(a, _)| a)
        {
            entre
        } else {
            v.split(" #").next().unwrap_or(v).trim()
        };
        return (!v.is_empty()).then(|| v.to_owned());
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    #[test]
    fn le_o_formato_env_so_da_chave_pedida() {
        let t = "# comentario\nOUTRA=1\nTELEGRAM_API_ID=12345\nexport TELEGRAM_API_HASH=\"abc123\"\nVAZIA=\n";
        assert_eq!(valor_do_env(t, "TELEGRAM_API_ID").as_deref(), Some("12345"));
        assert_eq!(
            valor_do_env(t, "TELEGRAM_API_HASH").as_deref(),
            Some("abc123")
        );
        assert_eq!(valor_do_env(t, "VAZIA"), None);
        assert_eq!(valor_do_env(t, "NAO_TEM"), None);
        assert_eq!(valor_do_env("K='x y' # c", "K").as_deref(), Some("x y"));
        assert_eq!(valor_do_env("K=x # c", "K").as_deref(), Some("x"));
    }

    #[test]
    fn ambiente_ganha_do_env_e_falta_dá_erro() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join(".env"),
            "TELEGRAM_API_ID=111\nTELEGRAM_API_HASH=do-env\n",
        )
        .unwrap();
        let raizes = vec![d.path().to_path_buf()];
        let so_env = Credenciais::resolver(&|_| None, &raizes, None).unwrap();
        assert_eq!((so_env.api_id, so_env.api_hash.as_str()), (111, "do-env"));
        let amb = |n: &str| match n {
            VARIAVEL_DO_ID => Some("222".to_owned()),
            VARIAVEL_DO_HASH => Some("do-ambiente".to_owned()),
            _ => None,
        };
        let c = Credenciais::resolver(&amb, &raizes, None).unwrap();
        assert_eq!((c.api_id, c.api_hash.as_str()), (222, "do-ambiente"));
        assert!(matches!(
            Credenciais::resolver(&|_| None, &[], None),
            Err(Erro::SemCredenciais)
        ));
        // Id que não é número não vale.
        let ruim = |n: &str| match n {
            VARIAVEL_DO_ID => Some("abc".to_owned()),
            VARIAVEL_DO_HASH => Some("x".to_owned()),
            _ => None,
        };
        assert!(Credenciais::resolver(&ruim, &[], None).is_err());
    }

    #[test]
    fn debug_nao_mostra_o_hash() {
        let c = Credenciais::de_texto("1", "hash-sigiloso").unwrap();
        assert!(!format!("{c:?}").contains("hash-sigiloso"));
    }
}
