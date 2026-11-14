//! O [`Embedder`]: texto vira vetor. Há três: o falso (testes), o FastEmbed (feature
//! `modelo-real`, o que o app usa) e o que o chamador quiser escrever.

use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest, Sha256};

use crate::erro::Resultado;

/// Transforma texto em vetor. Síncrono e `Send + Sync`: o `sbf worker` e a janela o usam
/// em threads próprias.
pub trait Embedder: Send + Sync {
    /// Nome estável do modelo. Entra no índice: trocar de modelo refaz os vetores.
    fn nome(&self) -> &str;
    /// Quantas dimensões tem cada vetor.
    fn dimensao(&self) -> usize;
    /// Vetores dos trechos que vão para o índice (um por texto, na ordem).
    ///
    /// # Errors
    /// Falha do modelo.
    fn embutir_trechos(&self, textos: &[String]) -> Resultado<Vec<Vec<f32>>>;
    /// O vetor de uma pergunta. Alguns modelos (E5) tratam pergunta e trecho de forma
    /// diferente; por padrão é igual.
    ///
    /// # Errors
    /// Falha do modelo.
    fn embutir_consulta(&self, texto: &str) -> Resultado<Vec<f32>> {
        let mut v = self.embutir_trechos(&[texto.to_owned()])?;
        v.pop()
            .ok_or_else(|| crate::ErroDoIndice::Embedder("o modelo não devolveu vetor".into()))
    }
}

/// Dimensão do embedder falso.
pub const DIMENSAO_DO_FALSO: usize = 256;

/// Determinístico e sem modelo: cada palavra (minúscula, sem acento comum) soma 1 numa das
/// 256 posições, escolhida pelo SHA-256 da palavra; o vetor sai normalizado. Textos com
/// palavras em comum ficam próximos, o que basta para os testes de busca. Conta quantos
/// textos embutiu, para os testes de atualização incremental (RG-02).
#[derive(Debug, Default)]
pub struct EmbedderFalso {
    embutidos: AtomicUsize,
}

impl EmbedderFalso {
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Quantos textos já passaram por aqui.
    #[must_use]
    pub fn embutidos(&self) -> usize {
        self.embutidos.load(Ordering::SeqCst)
    }

    fn vetor(texto: &str) -> Vec<f32> {
        let mut v = vec![0.0_f32; DIMENSAO_DO_FALSO];
        let minusculo = texto.to_lowercase();
        for palavra in minusculo
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| p.chars().count() > 1)
        {
            let h = Sha256::digest(sem_acento(palavra).as_bytes());
            let pos = usize::from(u16::from_le_bytes([h[0], h[1]])) % DIMENSAO_DO_FALSO;
            v[pos] += 1.0;
        }
        normalizar(&mut v);
        v
    }
}

fn sem_acento(p: &str) -> String {
    p.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            outro => outro,
        })
        .collect()
}

/// Escala o vetor para norma 1 (vetor nulo fica como está).
pub fn normalizar(v: &mut [f32]) {
    let norma = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norma > 0.0 {
        for x in v {
            *x /= norma;
        }
    }
}

impl Embedder for EmbedderFalso {
    fn nome(&self) -> &str {
        "falso-256"
    }

    fn dimensao(&self) -> usize {
        DIMENSAO_DO_FALSO
    }

    fn embutir_trechos(&self, textos: &[String]) -> Resultado<Vec<Vec<f32>>> {
        self.embutidos.fetch_add(textos.len(), Ordering::SeqCst);
        Ok(textos.iter().map(|t| Self::vetor(t)).collect())
    }
}

#[cfg(feature = "modelo-real")]
pub use real::{FastEmbedder, modelo_baixado};

#[cfg(feature = "modelo-real")]
mod real {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

    use super::{Embedder, normalizar};
    use crate::erro::{ErroDoIndice, Resultado};

    /// O modelo multilíngue (português incluso), de 384 dimensões, ~120 MB.
    const NOME: &str = "multilingual-e5-small";
    const DIMENSAO: usize = 384;

    /// A pasta já tem um modelo baixado? É a pergunta que a janela faz antes de avisar
    /// "vou baixar ~120 MB na primeira vez". O download só acontece em
    /// [`FastEmbedder::novo`], chamado pelo app real depois do aviso.
    #[must_use]
    pub fn modelo_baixado(pasta: &Path) -> bool {
        std::fs::read_dir(pasta).is_ok_and(|mut e| {
            e.any(|x| {
                x.is_ok_and(|x| {
                    x.file_name().to_string_lossy().starts_with("models--")
                        && x.path().join("snapshots").is_dir()
                })
            })
        })
    }

    /// FastEmbed (ONNX Runtime) com `multilingual-e5-small`. E5 pede os prefixos
    /// `passage: ` nos trechos e `query: ` na pergunta.
    pub struct FastEmbedder {
        modelo: Mutex<TextEmbedding>,
    }

    impl std::fmt::Debug for FastEmbedder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FastEmbedder")
                .field("modelo", &NOME)
                .finish()
        }
    }

    impl FastEmbedder {
        /// Carrega o modelo de `pasta_do_modelo`, **baixando-o se não estiver lá** (a rede
        /// só é usada nesse caso). Quem chama avisa o usuário antes.
        ///
        /// # Errors
        /// Falha de download ou de carga.
        pub fn novo(pasta_do_modelo: impl Into<PathBuf>) -> Resultado<Self> {
            let opcoes = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
                .with_cache_dir(pasta_do_modelo.into())
                .with_show_download_progress(false);
            let modelo = TextEmbedding::try_new(opcoes)
                .map_err(|e| ErroDoIndice::Embedder(e.to_string()))?;
            Ok(Self {
                modelo: Mutex::new(modelo),
            })
        }

        fn embutir(&self, prefixo: &str, textos: &[String]) -> Resultado<Vec<Vec<f32>>> {
            let com_prefixo: Vec<String> = textos.iter().map(|t| format!("{prefixo}{t}")).collect();
            let mut m = self
                .modelo
                .lock()
                .map_err(|_| ErroDoIndice::Embedder("modelo envenenado".into()))?;
            let mut v = m
                .embed(&com_prefixo, Some(16))
                .map_err(|e| ErroDoIndice::Embedder(e.to_string()))?;
            for x in &mut v {
                normalizar(x);
            }
            Ok(v)
        }
    }

    impl Embedder for FastEmbedder {
        fn nome(&self) -> &str {
            NOME
        }
        fn dimensao(&self) -> usize {
            DIMENSAO
        }
        fn embutir_trechos(&self, textos: &[String]) -> Resultado<Vec<Vec<f32>>> {
            self.embutir("passage: ", textos)
        }
        fn embutir_consulta(&self, texto: &str) -> Resultado<Vec<f32>> {
            self.embutir("query: ", &[texto.to_owned()])?
                .pop()
                .ok_or_else(|| ErroDoIndice::Embedder("sem vetor".into()))
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn falso_e_deterministico_e_normalizado() {
        let e = EmbedderFalso::novo();
        let a = e.embutir_consulta("Conta de energia Cemig").expect("a");
        let b = e.embutir_consulta("conta de ENERGIA cemig").expect("b");
        assert_eq!(a, b);
        let norma: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norma - 1.0).abs() < 1e-5);
        assert_eq!(a.len(), e.dimensao());
    }

    #[test]
    fn palavras_em_comum_aproximam() {
        let e = EmbedderFalso::novo();
        let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let q = e.embutir_consulta("fatura de energia").expect("q");
        let p = e.embutir_consulta("fatura de energia março").expect("p");
        let l = e.embutir_consulta("mensalidade academia").expect("l");
        assert!(cos(&q, &p) > cos(&q, &l));
    }
}
