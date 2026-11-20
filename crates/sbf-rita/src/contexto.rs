//! O que Rita enxerga: o item na tela, a área e o filtro, os títulos da lista e o cofre
//! (spec 15, D8). Nada além disso entra no primeiro prompt.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Canal, Dicas, Entrada, Origem, TipoDeEntrada, VERSAO_DO_ESQUEMA};
use serde::{Deserialize, Serialize};

use crate::cofre::{FonteDeSimilares, SimilaresDoCofre};

/// O nome do arquivo do catálogo dentro do cofre.
pub const ARQUIVO_DAS_REGRAS: &str = "regras.sqlite";
/// O nome do banco do inventário dentro do cofre.
pub const ARQUIVO_DO_INVENTARIO: &str = "inventario.sqlite";

/// Um título da lista que o usuário vê (só título e identidade, nunca o texto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TituloDaLista {
    pub id: String,
    pub titulo: String,
}

/// O que a tela diz a Rita. Serializável: o servidor MCP do `claude -p` recebe isto por um
/// arquivo privado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DadosDaTela {
    /// A pasta do cofre (com `regras.sqlite` e `inventario.sqlite`).
    pub cofre: PathBuf,
    /// O item selecionado, com o texto guardado.
    pub item: Entrada,
    /// "Caixa de entrada", "A triar", "Documentos", "Padrões" ou "Movimentações".
    pub area: String,
    pub filtro: Option<String>,
    /// Os títulos visíveis na lista (só vão ao modelo quando o usuário pede).
    pub titulos: Vec<TituloDaLista>,
}

impl DadosDaTela {
    /// Um documento do cofre sem lista nem filtro (a CLI).
    pub fn de_documento(cofre: &Path, item: Entrada) -> Self {
        Self {
            cofre: cofre.to_path_buf(),
            item,
            area: "Documentos".to_owned(),
            filtro: None,
            titulos: Vec::new(),
        }
    }
}

/// Os dados da tela mais o que não se serializa: o executor dos scripts e a fonte dos
/// documentos parecidos.
#[derive(Clone)]
pub struct Contexto {
    pub tela: DadosDaTela,
    pub executor: Arc<dyn ExecutorDeScript>,
    pub similares: Arc<dyn FonteDeSimilares>,
}

impl std::fmt::Debug for Contexto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Contexto")
            .field("area", &self.tela.area)
            .field("cofre", &self.tela.cofre)
            .finish_non_exhaustive()
    }
}

impl Contexto {
    /// O contexto de verdade: os parecidos saem do inventário do cofre.
    pub fn novo(tela: DadosDaTela, executor: Arc<dyn ExecutorDeScript>) -> Self {
        let similares = Arc::new(SimilaresDoCofre::novo(&tela.cofre));
        Self {
            tela,
            executor,
            similares,
        }
    }

    /// Troca a fonte dos parecidos (testes, registros da Pluggy).
    #[must_use]
    pub fn com_similares(mut self, similares: Arc<dyn FonteDeSimilares>) -> Self {
        self.similares = similares;
        self
    }

    pub(crate) fn catalogo_caminho(&self) -> PathBuf {
        self.tela.cofre.join(ARQUIVO_DAS_REGRAS)
    }
}

/// A entrada de um documento do cofre (canal `cofre`).
pub fn entrada_de_documento(
    sha256: &str,
    nome: Option<&str>,
    mime: Option<&str>,
    texto: Option<&str>,
) -> Entrada {
    Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: sha256.to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Cofre,
            id_externo: None,
        },
        sha256: Some(sha256.to_owned()),
        mime: mime.map(str::to_owned),
        nome: nome.map(str::to_owned),
        texto: texto.map(str::to_owned),
        dicas: Dicas::default(),
        momento: None,
    }
}
