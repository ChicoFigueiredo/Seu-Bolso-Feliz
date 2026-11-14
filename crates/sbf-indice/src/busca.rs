//! O trait [`Busca`] (o que a triagem e o MCP precisam) e a [`BuscaLocal`] (índice + embedder).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::embedder::Embedder;
use crate::erro::{ErroDoIndice, Resultado};
use crate::indice::{Indice, ItemRef, Parecido};

/// Busca por semelhança de texto. Quem usa não sabe de vetor nem de modelo.
pub trait Busca: Send + Sync {
    /// Os `k` trechos mais parecidos com `texto`, sem os que são do item `exceto`.
    ///
    /// # Errors
    /// Falha do índice ou do embedder.
    fn parecidos(
        &self,
        texto: &str,
        k: usize,
        exceto: Option<&ItemRef>,
    ) -> Resultado<Vec<Parecido>>;
}

/// O índice em arquivo com o embedder. Abre o arquivo a cada busca (o worker escreve noutro
/// processo), então sempre enxerga a última atualização.
pub struct BuscaLocal {
    caminho: PathBuf,
    embedder: Arc<dyn Embedder>,
    trava: Mutex<()>,
}

impl std::fmt::Debug for BuscaLocal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuscaLocal")
            .field("caminho", &self.caminho)
            .field("modelo", &self.embedder.nome())
            .finish_non_exhaustive()
    }
}

impl BuscaLocal {
    #[must_use]
    pub fn nova(caminho: impl Into<PathBuf>, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            caminho: caminho.into(),
            embedder,
            trava: Mutex::new(()),
        }
    }
}

impl Busca for BuscaLocal {
    fn parecidos(
        &self,
        texto: &str,
        k: usize,
        exceto: Option<&ItemRef>,
    ) -> Resultado<Vec<Parecido>> {
        let _g = self
            .trava
            .lock()
            .map_err(|_| ErroDoIndice::Outro("busca envenenada".into()))?;
        if !self.caminho.exists() {
            return Ok(Vec::new());
        }
        let indice = Indice::abrir(&self.caminho)?;
        let vetor = self.embedder.embutir_consulta(texto)?;
        // Pede um pouco mais para poder tirar os trechos do próprio item.
        let mut v = indice.buscar(&vetor, k + 8)?;
        if let Some(x) = exceto {
            v.retain(|p| &p.item != x);
        }
        v.truncate(k);
        Ok(v)
    }
}

/// Busca sobre um [`Indice`] em memória (demonstração e testes): sem arquivo.
pub struct BuscaEmMemoria {
    indice: Mutex<Indice>,
    embedder: Arc<dyn Embedder>,
}

impl std::fmt::Debug for BuscaEmMemoria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuscaEmMemoria").finish_non_exhaustive()
    }
}

impl BuscaEmMemoria {
    #[must_use]
    pub fn nova(indice: Indice, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            indice: Mutex::new(indice),
            embedder,
        }
    }
}

impl Busca for BuscaEmMemoria {
    fn parecidos(
        &self,
        texto: &str,
        k: usize,
        exceto: Option<&ItemRef>,
    ) -> Resultado<Vec<Parecido>> {
        let vetor = self.embedder.embutir_consulta(texto)?;
        let indice = self
            .indice
            .lock()
            .map_err(|_| ErroDoIndice::Outro("busca envenenada".into()))?;
        let mut v = indice.buscar(&vetor, k + 8)?;
        if let Some(x) = exceto {
            v.retain(|p| &p.item != x);
        }
        v.truncate(k);
        Ok(v)
    }
}
