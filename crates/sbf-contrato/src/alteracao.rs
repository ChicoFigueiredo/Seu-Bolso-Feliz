//! A **alteração** (grill Q6 e Q7): uma Ação do usuário sobre um item, com a versão de
//! base de cada linha tocada. O servidor a aplica inteira ou a recusa, nunca pela metade.
//!
//! `POST /alteracoes` com [`PropostaDeAlteracao`] → `200` com [`RespostaDaAlteracao`].
//! Reenviar a mesma proposta (mesmo `id`) devolve a mesma resposta: a estação pode
//! repetir depois de um corte de rede sem medo de a alteração cair como recusa contra a
//! própria versão.

use serde::{Deserialize, Serialize};

use crate::{CodigoDeErro, Colecao, Id, Instante, Movimentacao, Rateio, Versao};

/// O corpo de `POST /alteracoes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropostaDeAlteracao {
    /// Gerado pela estação (UUID v4); torna o envio idempotente.
    pub id: Id,
    pub acao: Acao,
    /// O item sobre o qual a Ação age (a página que o usuário tinha aberta).
    pub item: ItemAlterado,
    /// A versão de base de **cada** linha que a Ação toca, como a estação as viu no
    /// cache. Faltar uma linha tocada é pedido inválido.
    pub versoes_de_base: Vec<VersaoDeBase>,
    /// Quando o usuário fez a Ação, pelo relógio da estação. Só informativo.
    pub feita_em: Instante,
}

/// As Ações aceitas. No fio: `{"tipo":"mudar_categoria_da_movimentacao", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Acao {
    /// Troca a categoria de uma movimentação. `categoria: null` tira a categoria.
    /// Toca uma linha: a movimentação.
    MudarCategoriaDaMovimentacao {
        movimentacao: Id,
        categoria: Option<Id>,
    },
}

/// Um item do livro-caixa: a coleção e o `id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemAlterado {
    pub colecao: Colecao,
    pub id: Id,
}

/// A versão de uma linha: a de base, na proposta; a nova, no aceite.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersaoDeBase {
    pub colecao: Colecao,
    pub id: Id,
    pub versao: Versao,
}

/// Uma linha como está no servidor agora, marcada pela coleção.
///
/// No fio: `{"colecao":"movimentacoes","linha":{...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "colecao", content = "linha", rename_all = "kebab-case")]
pub enum LinhaAgora {
    Movimentacoes(Box<Movimentacao>),
    Rateios(Rateio),
}

/// A resposta de `POST /alteracoes`. No fio: `{"resultado":"aceita", ...}` ou
/// `{"resultado":"recusada", ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resultado", rename_all = "snake_case")]
pub enum RespostaDaAlteracao {
    /// Aplicada inteira. `linhas` traz cada linha tocada como ficou, com a versão nova:
    /// a estação as grava no cache sem esperar a próxima sincronização.
    Aceita { id: Id, linhas: Vec<LinhaAgora> },
    /// Recusada inteira; nada mudou no servidor. A área Sincronização mostra lado a
    /// lado "Sua versão" (a proposta, aplicada pela estação sobre a cópia de base dela)
    /// e "No servidor agora", com Refazer sobre a nova e Descartar.
    Recusada {
        id: Id,
        /// [`CodigoDeErro::VersaoDeBaseDivergente`], [`CodigoDeErro::ItemNaoEncontrado`]
        /// ou [`CodigoDeErro::ReferenciaNaoEncontrada`].
        motivo: CodigoDeErro,
        mensagem: String,
        /// A proposta como a estação a mandou.
        sua_versao: Box<PropostaDeAlteracao>,
        /// Cada linha tocada como está no servidor agora (vazio se o item sumiu). Refazer
        /// usa as versões destas linhas como nova base.
        no_servidor_agora: Vec<LinhaAgora>,
    },
}

impl RespostaDaAlteracao {
    /// O `id` da proposta respondida.
    pub fn id(&self) -> Id {
        match self {
            RespostaDaAlteracao::Aceita { id, .. } | RespostaDaAlteracao::Recusada { id, .. } => {
                *id
            }
        }
    }
}
