//! Os erros da API: o corpo de toda resposta que não é 2xx.

use serde::{Deserialize, Serialize};

/// O corpo de erro. A `mensagem` é curta, em português, sem segredo e sem detalhe
/// interno; quem decide o que fazer é o `codigo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErroDaApi {
    pub codigo: CodigoDeErro,
    pub mensagem: String,
    /// Para [`CodigoDeErro::ContratoAntigo`] e [`CodigoDeErro::ContratoMaisNovo`]: as
    /// versões que o servidor aceita.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contratos_aceitos: Option<Vec<u32>>,
}

/// Os códigos de erro, estáveis entre versões do contrato. A estação reage ao código.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodigoDeErro {
    /// `401`. Sem credencial, ou credencial que o servidor não conhece. Sem detalhe.
    NaoAutenticado,
    /// `401`. A credencial era desta estação, mas a estação foi revogada. A estação
    /// para de sincronizar, apaga o cache e a credencial e avisa (grill Q11).
    EstacaoRevogada,
    /// `426`. A estação fala um contrato mais velho do que o servidor aceita:
    /// "atualize o app".
    ContratoAntigo,
    /// `426`. A estação fala um contrato mais novo do que o servidor conhece: o servidor
    /// ainda não foi atualizado. A estação continua lendo o cache.
    ContratoMaisNovo,
    /// `403`. O login Google não foi aceito no pareamento (token inválido, e-mail não
    /// verificado, usuário desconhecido, conta Google diferente da vinculada).
    PareamentoRecusado,
    /// `404`. A coleção, a estação ou o recurso não existe (para este dono).
    NaoEncontrado,
    /// `400`/`422`. O corpo não segue o contrato.
    PedidoInvalido,
    /// `422`. A ação da alteração existe no contrato mas o servidor ainda não a aceita.
    AcaoAindaNaoAceita,
    /// Motivo de [`crate::RespostaDaAlteracao::Recusada`]: alguma linha tocada mudou no
    /// servidor depois da versão de base da estação.
    VersaoDeBaseDivergente,
    /// Motivo de recusa: o item alterado não existe (mais) no servidor.
    ItemNaoEncontrado,
    /// Motivo de recusa: a alteração aponta para outra linha que não existe (ex.: a
    /// categoria nova).
    ReferenciaNaoEncontrada,
    /// Motivo de recusa (contrato v2): o banco ou a regra de lançamento recusou (rascunho
    /// que não pode virar movimentação, restrição violada). Nada mudou; a mensagem diz o
    /// que falta.
    RegraViolada,
    /// `413`. O arquivo é maior que o servidor aceita.
    ArquivoGrande,
    /// `429`. Pedidos demais.
    PedidosDemais,
    /// `500`/`503`. Falha do servidor ou do banco. Tente de novo mais tarde.
    Interno,
}
