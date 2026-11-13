//! As escritas que a janela pede: as Ações do §10, desfazer e lotes.
//!
//! Dono: integrador (o contrato). A implementação sobre o SQLite é do agente do núcleo
//! (`Inventario` em `banco`); a janela tem uma implementação fictícia para o modo
//! demonstração.
//!
//! Toda chamada grava uma ou mais linhas em `decisao` e atualiza a projeção na
//! **mesma transação** (spec 06, "O banco"). A tabela "Como cada decisão do §10 cai no
//! banco" diz o `alvo_tipo`, o `campo` e a projeção de cada [`Acao`].

use serde::{Deserialize, Serialize};

use crate::consulta::Centavos;
use crate::referencia::Ref;

/// O id de uma linha de `decisao`.
pub type IdDecisao = i64;

/// Uma Ação do §10 (spec 06), na ordem dos grupos da aba Triagem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acao {
    // Triagem: achado / estado
    MarcarDocumento,
    Descartar,
    EmDuvida,
    // Corrigir
    CorrigirFornecedor(String),
    CorrigirCategoria(String),
    /// Achado: `document_type`. Registro: `tipo`.
    CorrigirTipo(String),
    /// Recalcula `ciclo_id` quando a data muda.
    CorrigirDataEValor {
        data: Option<String>,
        valor_centavos: Option<Centavos>,
    },
    // Relacionar
    MarcarDuplicidade {
        com: Ref,
    },
    UnirConciliados {
        com: Ref,
    },
    /// O alvo é um fornecedor.
    ConfirmarRecorrencia,
    // Interpretação: registro / estado
    Aprovar,
    Rejeitar,
    // Pendência
    /// Abre uma pendência sobre o alvo.
    Sinalizar {
        pergunta: String,
    },
    /// O alvo é a pendência.
    Responder {
        resposta: String,
    },
    // PDF protegido (spec 06, item 6): só com a confirmação do usuário.
    /// O alvo é o fornecedor. `tipo_de_documento` é o tipo do achado, ou `*` para todos.
    GuardarRegraDeSenha {
        tipo_de_documento: String,
        estrategia: String,
        /// Menor tenta primeiro. `None`: a regra nova fica com a prioridade padrão
        /// ([`crate::protegidos::PRIORIDADE_PADRAO_DA_REGRA`], o padrão da coluna
        /// `regra_senha.prioridade`), e a que já existe mantém a dela.
        prioridade: Option<i64>,
    },
}

/// Uma Ação aplicada a um ou mais alvos (a seleção da tabela).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoDeDecisao {
    pub acao: Acao,
    pub alvos: Vec<Ref>,
    /// Obrigatório em Rejeitar; opcional no resto.
    pub motivo: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDeComando {
    #[error("item não encontrado: {0}")]
    NaoEncontrado(Ref),
    #[error("ação {acao} não se aplica a {alvo}")]
    NaoSeAplica { acao: String, alvo: Ref },
    #[error("motivo obrigatório")]
    MotivoObrigatorio,
    #[error("decisão {0} já desfeita ou inexistente")]
    NadaADesfazer(IdDecisao),
    #[error("o lote {0} já foi aprovado: desaprove antes de mudar")]
    LoteAprovado(Ref),
    #[error("erro do banco: {0}")]
    Banco(String),
}

impl From<rusqlite::Error> for ErroDeComando {
    fn from(erro: rusqlite::Error) -> Self {
        ErroDeComando::Banco(erro.to_string())
    }
}

pub type ResultadoDeComando<T> = Result<T, ErroDeComando>;

/// As escritas do usuário. Toda escrita é uma decisão com `autor = 'usuario'`.
pub trait Comandos {
    /// Aplica a Ação a cada alvo. Devolve as decisões gravadas, na ordem dos alvos.
    fn executar(&mut self, pedido: &PedidoDeDecisao) -> ResultadoDeComando<Vec<IdDecisao>>;
    /// Grava uma nova decisão com `desfaz` preenchido, que restaura o `antes`.
    fn desfazer(&mut self, decisao: IdDecisao) -> ResultadoDeComando<IdDecisao>;
    /// "Desfazer decisão" da Barra de Acesso Rápido: a última decisão do usuário
    /// ainda não desfeita. `None` quando não há.
    fn desfazer_ultima(&mut self) -> ResultadoDeComando<Option<IdDecisao>>;

    // Lotes (spec 06, "Lotes")
    fn criar_lote(&mut self, nome: &str) -> ResultadoDeComando<Ref>;
    /// Aceita achados em estado Documento e registros.
    fn adicionar_ao_lote(&mut self, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()>;
    fn remover_do_lote(&mut self, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()>;
    /// Grava `aprovado_em` e muda os registros do lote para `aprovado`.
    fn aprovar_lote(&mut self, lote: &Ref) -> ResultadoDeComando<()>;
    /// Volta o lote para editável, por uma nova decisão.
    fn desaprovar_lote(&mut self, lote: &Ref) -> ResultadoDeComando<()>;
}
