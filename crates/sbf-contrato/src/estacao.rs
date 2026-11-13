//! A estação perante o servidor: `/estado`, o pareamento e a lista de estações.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, Instante};

/// O corpo de `GET /estado`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estado {
    /// A versão de contrato do servidor ([`crate::VERSAO_DO_CONTRATO`] dele).
    pub contrato: u32,
    /// As versões de contrato que o servidor aceita agora (a atual e a anterior).
    pub contratos_aceitos: Vec<u32>,
    /// A versão das regras: a versão do binário do servidor (ADR-012). A barra de status
    /// da estação avisa quando a dela é diferente.
    pub regras: String,
    /// A hora do servidor.
    pub hora_do_servidor: Instante,
    /// O dia de corte do ciclo do dono (`user_financial_preferences`; padrão 20, Q14).
    pub dia_de_corte: u8,
    /// O fuso em que o ciclo é calculado.
    pub fuso: String,
    /// Quantos documentos do dono esperam revisão no servidor (`ingestion_jobs` em
    /// `pending_review` ou `drafted`, como `contarPendencias` do web): "Precisam de
    /// revisão" em Para resolver. Não é coleção porque a fila não sincroniza.
    pub documentos_a_revisar: u32,
    /// A estação que chamou.
    pub estacao: Estacao,
}

/// Uma estação do dono, como o servidor a vê.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estacao {
    pub id: Id,
    pub nome: String,
    /// O sistema que a estação declarou no pareamento (`linux`, `windows`, `macos`…).
    pub sistema: String,
    pub pareada_em: Instante,
    /// O último pedido autenticado desta estação.
    pub ultimo_contato: Option<Instante>,
    pub revogada: bool,
    pub revogada_em: Option<Instante>,
}

/// O corpo de `GET /estacoes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estacoes {
    pub estacoes: Vec<Estacao>,
    /// A estação que pediu (a marca "esta" da área Sincronização).
    pub esta: Id,
}

/// O corpo de `POST /estacoes/pareamento`: a prova do login Google trocada por uma
/// credencial de estação.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoDePareamento {
    /// O `id_token` (JWT) que o Google devolveu ao loopback da estação.
    pub id_token: String,
    /// O `nonce` que a estação mandou no pedido de autorização; o servidor confere que é
    /// o mesmo de dentro do `id_token`.
    pub nonce: String,
    /// O nome da estação (o usuário pode renomear depois).
    pub nome: String,
    pub sistema: String,
}

impl fmt::Debug for PedidoDePareamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PedidoDePareamento")
            .field("id_token", &"<oculto>")
            .field("nonce", &"<oculto>")
            .field("nome", &self.nome)
            .field("sistema", &self.sistema)
            .finish()
    }
}

/// A resposta do pareamento. A `credencial` aparece **só aqui**, uma vez: o servidor
/// guarda o hash dela. A estação a grava no chaveiro (`estacao:<servidor>`), nunca em
/// arquivo, log ou SQLite.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pareamento {
    pub estacao: Estacao,
    /// Token opaco de 256 bits (grill Q3), em base64url sem preenchimento, com o
    /// prefixo `sbfe_`. Vai em `Authorization: Bearer`.
    pub credencial: String,
}

impl fmt::Debug for Pareamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pareamento")
            .field("estacao", &self.estacao)
            .field("credencial", &"<oculta>")
            .finish()
    }
}
