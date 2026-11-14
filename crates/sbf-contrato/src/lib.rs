//! O contrato HTTP entre a **estação** e o **servidor** do Seu Bolso Feliz (spec 07,
//! fatia 2; ADR-013). Só tipos, com serde, sem lógica: o servidor (`sbf-servidor`) os
//! monta, a estação (`sbf-estacao`) os lê.
//!
//! # Recursos
//!
//! Todos sob `/api/v{N}/`, onde `N` é a versão do contrato da **estação** que pede
//! ([`VERSAO_DO_CONTRATO`] dela). O servidor aceita a versão atual dele e a anterior
//! (grill Q4) e recusa as outras com [`CodigoDeErro::ContratoAntigo`] ou
//! [`CodigoDeErro::ContratoMaisNovo`]. Todo pedido, menos o pareamento, leva
//! `Authorization: Bearer <credencial>`.
//!
//! | Método e caminho                 | Corpo                  | Resposta                             |
//! | -------------------------------- | ---------------------- | ------------------------------------ |
//! | `GET  /estado`                   | —                      | [`Estado`]                           |
//! | `GET  /livro/{colecao}`          | —                      | [`LinhasDaColecao`], com `ETag`      |
//! | `POST /estacoes/pareamento`      | [`PedidoDePareamento`] | [`Pareamento`] (sem `Authorization`) |
//! | `GET  /estacoes`                 | —                      | [`Estacoes`]                         |
//! | `POST /estacoes/{id}/revogar`    | —                      | [`Estacao`] (a revogada)             |
//! | `POST /alteracoes`               | [`PropostaDeAlteracao`]| [`RespostaDaAlteracao`]              |
//! | `GET  /livro/{colecao}?desde=C`  | —                      | [`DeltaDaColecao`] (v2)              |
//! | `POST /documentos`               | multipart (v2)         | [`RespostaDoDocumento`]              |
//!
//! # Versão 2 (fatia 3)
//!
//! O servidor aceita a v1 e a v2. A v2 acrescenta as coleções `lotes-de-rascunhos` e
//! `rascunhos` (só em `/api/v2`), as Ações de rascunho e de edição, o delta por cursor
//! (`?desde=`, só leitura incremental) e o envio de documentos. Uma estação v1 continua
//! como era: coleção inteira com `ETag`, uma Ação.
//!
//! `{colecao}` é o [`Colecao::caminho`]. A coleção vem **inteira** (grill Q5): o `ETag`
//! é forte e muda quando qualquer linha muda; com `If-None-Match` igual, `304` sem corpo.
//!
//! Erro: status HTTP diferente de 2xx com corpo [`ErroDaApi`]. Uma alteração recusada
//! **não** é erro: é `200` com [`RespostaDaAlteracao::Recusada`].
//!
//! # Convenções
//!
//! - Identidade: [`Id`] (UUID). Todo filtro do servidor é pelo **dono** da credencial.
//! - Dinheiro: [`Centavos`] (`i64`). O servidor converte o `numeric` do Postgres sem
//!   ponto flutuante; `R$ 0,10` é `10`.
//! - Datas: [`Data`] em `AAAA-MM-DD`; instantes: [`Instante`] em RFC 3339, UTC.
//! - [`Versao`]: a versão de uma linha (hoje, o `updated_at` dela), **opaca** para a
//!   estação: só se compara por igualdade e se devolve como versão de base.

mod alteracao;
mod colecoes;
mod delta;
mod documento;
mod erro;
mod estacao;

pub use alteracao::{
    Acao, EdicaoDaMovimentacao, ItemAlterado, LinhaAgora, NovoRateio, PropostaDeAlteracao,
    RespostaDaAlteracao, VersaoDeBase,
};
pub use colecoes::*;
pub use delta::{Cursor, DeltaDaColecao};
pub use documento::{
    CAMPO_ARQUIVO, CAMPO_ID_NO_INVENTARIO, CAMPO_NOME_ORIGINAL, CAMPO_SHA256, RespostaDoDocumento,
    TAMANHO_MAXIMO_DO_DOCUMENTO,
};
pub use erro::{CodigoDeErro, ErroDaApi};
pub use estacao::{Estacao, Estacoes, Estado, Pareamento, PedidoDePareamento};

use serde::{Deserialize, Serialize};

/// A versão do contrato desta compilação. Sobe a cada mudança incompatível dos tipos
/// deste crate. O servidor aceita esta e a anterior (grill Q4).
pub const VERSAO_DO_CONTRATO: u32 = 2;

/// A menor versão de contrato que já existiu. O servidor nunca aceita menos que isto,
/// mesmo quando é a "anterior" da atual.
pub const PRIMEIRA_VERSAO_DO_CONTRATO: u32 = 1;

/// O prefixo de todos os recursos para uma versão de contrato: `/api/v1`.
pub fn prefixo(versao: u32) -> String {
    format!("/api/v{versao}")
}

/// A identidade de uma linha do livro-caixa, de uma estação ou de uma alteração.
pub type Id = uuid::Uuid;

/// Dinheiro em centavos. Nunca ponto flutuante.
pub type Centavos = i64;

/// Uma data civil, `AAAA-MM-DD` (sem fuso; é a data como o usuário a vê).
pub type Data = String;

/// Um instante, RFC 3339 em UTC (`2026-11-13T18:04:05.123456Z`).
pub type Instante = String;

/// A versão de uma linha do livro-caixa, opaca para a estação. Hoje é o `updated_at` da
/// linha em microssegundos desde a época, como texto; a fatia 3 pode trocar por um
/// contador sem mudar o contrato.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Versao(pub String);

/// O cabeçalho de autorização: `Authorization: Bearer <credencial>`.
pub const CABECALHO_DE_AUTORIZACAO: &str = "authorization";

/// O esquema do cabeçalho de autorização.
pub const ESQUEMA_DE_AUTORIZACAO: &str = "Bearer";
