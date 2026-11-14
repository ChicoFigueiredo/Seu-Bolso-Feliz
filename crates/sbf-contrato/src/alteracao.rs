//! A **alteração** (grill Q6 e Q7): uma Ação do usuário sobre um item, com a versão de
//! base de cada linha tocada. O servidor a aplica inteira ou a recusa, nunca pela metade.
//!
//! `POST /alteracoes` com [`PropostaDeAlteracao`] → `200` com [`RespostaDaAlteracao`].
//! Reenviar a mesma proposta (mesmo `id`) devolve a mesma resposta: a estação pode
//! repetir depois de um corte de rede sem medo de a alteração cair como recusa contra a
//! própria versão.

use serde::{Deserialize, Serialize};

use crate::{
    Centavos, CodigoDeErro, Colecao, Data, Id, Instante, LoteDeRascunhos, Movimentacao, Rascunho,
    Rateio, Versao,
};

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
    /// Contrato v2. Grava o `draft_data` v1 inteiro, `status = corrected` e
    /// `corrections.manual_edit`. Toca uma linha: o rascunho.
    CorrigirRascunho {
        rascunho: Id,
        dados: serde_json::Value,
    },
    /// Contrato v2. `approved` e, na mesma transação, `fn_materialize_draft_record`: o
    /// rascunho vira movimentação. Toca uma linha: o rascunho.
    AprovarRascunho { rascunho: Id },
    /// Contrato v2. `rejected`, com o motivo em `corrections.rejection_reason`. Toca uma
    /// linha: o rascunho.
    RecusarRascunho { rascunho: Id, motivo: String },
    /// Contrato v2. Aprova e lança, um a um, todos os rascunhos pendentes do lote; **tudo
    /// ou nada**. Toca o lote e cada rascunho pendente dele (a estação manda a versão de
    /// base de cada um).
    AprovarLoteDeRascunhos { lote: Id },
    /// Contrato v2. Troca os campos editáveis da movimentação. Toca uma linha.
    EditarMovimentacao {
        movimentacao: Id,
        edicao: EdicaoDaMovimentacao,
    },
    /// Contrato v2. Apaga a movimentação e os rateios dela. Toca a movimentação.
    ExcluirMovimentacao { movimentacao: Id },
    /// Contrato v2. Troca o conjunto de rateios inteiro; a soma tem de bater com o valor
    /// da movimentação (senão é pedido inválido). Toca a movimentação (o servidor muda o
    /// `updated_at` dela a cada mudança de rateio, então a versão dela guarda o conjunto).
    RatearMovimentacao {
        movimentacao: Id,
        rateios: Vec<NovoRateio>,
    },
    /// Contrato v2. O rascunho é a mesma coisa que uma movimentação que já existe:
    /// `reconciliation_status = confirmed_duplicate` e aponta a movimentação. Toca o
    /// rascunho e a movimentação.
    ConciliarRascunho { rascunho: Id, movimentacao: Id },
}

/// Os campos que `EditarMovimentacao` grava (todos de uma vez: a estação manda o que o
/// formulário mostra, não só o que mudou).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdicaoDaMovimentacao {
    pub descricao: Option<String>,
    pub data: Data,
    /// Centavos, positivo (o sinal vem do tipo da movimentação).
    pub valor: Centavos,
    pub conta: Id,
    pub fornecedor: Option<Id>,
    /// `Some`: troca os rateios por um só, deste centro, com o valor inteiro. `None`:
    /// os rateios ficam como estão (e a soma deles não pode passar do valor novo).
    pub centro: Option<Id>,
    pub notas: Option<String>,
}

/// Um rateio de `RatearMovimentacao`. O `id` é da estação, para a cópia local já ter a
/// linha definitiva.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRateio {
    pub id: Id,
    pub centro: Id,
    pub valor: Centavos,
    pub nota: Option<String>,
}

impl Acao {
    /// O `tipo` da Ação no fio.
    pub fn tipo(&self) -> &'static str {
        match self {
            Acao::MudarCategoriaDaMovimentacao { .. } => "mudar_categoria_da_movimentacao",
            Acao::CorrigirRascunho { .. } => "corrigir_rascunho",
            Acao::AprovarRascunho { .. } => "aprovar_rascunho",
            Acao::RecusarRascunho { .. } => "recusar_rascunho",
            Acao::AprovarLoteDeRascunhos { .. } => "aprovar_lote_de_rascunhos",
            Acao::EditarMovimentacao { .. } => "editar_movimentacao",
            Acao::ExcluirMovimentacao { .. } => "excluir_movimentacao",
            Acao::RatearMovimentacao { .. } => "ratear_movimentacao",
            Acao::ConciliarRascunho { .. } => "conciliar_rascunho",
        }
    }

    /// Os `tipo` que o contrato v1 conhece.
    pub const TIPOS_DA_V1: [&'static str; 1] = ["mudar_categoria_da_movimentacao"];

    /// Todos os `tipo` que o contrato v2 conhece.
    pub const TIPOS: [&'static str; 9] = [
        "mudar_categoria_da_movimentacao",
        "corrigir_rascunho",
        "aprovar_rascunho",
        "recusar_rascunho",
        "aprovar_lote_de_rascunhos",
        "editar_movimentacao",
        "excluir_movimentacao",
        "ratear_movimentacao",
        "conciliar_rascunho",
    ];
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
    /// Contrato v2.
    LotesDeRascunhos(LoteDeRascunhos),
    /// Contrato v2.
    Rascunhos(Box<Rascunho>),
}

/// A resposta de `POST /alteracoes`. No fio: `{"resultado":"aceita", ...}` ou
/// `{"resultado":"recusada", ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resultado", rename_all = "snake_case")]
pub enum RespostaDaAlteracao {
    /// Aplicada inteira. `linhas` traz cada linha tocada como ficou, com a versão nova:
    /// a estação as grava no cache sem esperar a próxima sincronização.
    Aceita {
        id: Id,
        linhas: Vec<LinhaAgora>,
        /// Contrato v2: as linhas que a Ação apagou (a movimentação e os rateios dela, na
        /// exclusão; os rateios velhos, no rateio). A estação as tira do cache.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        excluidas: Vec<ItemAlterado>,
    },
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
