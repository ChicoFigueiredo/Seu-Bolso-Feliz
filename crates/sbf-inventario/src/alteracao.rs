//! As alterações que a janela pede ao livro-caixa e o estado da sincronização (spec 07,
//! "A área Sincronização", e grill Q6, Q7, Q11, Q13).
//!
//! Dono: agente da janela de Dinheiro (o contrato), por autorização do integrador. A
//! implementação real é da estação (`sbf-estacao`); a janela tem uma implementação
//! fictícia para o modo demonstração.
//!
//! Vocabulário (`CONTEXT.md`):
//! - **Alteração**: uma mudança que o usuário fez no desktop e que o servidor ainda não
//!   aceitou. O servidor a aplica inteira ou a recusa, nunca pela metade.
//! - **Alteração recusada**: o servidor não aplicou porque o fato mudou depois que ela
//!   foi feita, ou uma regra a reprovou. Fica visível até o usuário resolver: **Refazer
//!   sobre a nova** ou **Descartar**.
//! - **Sincronização**: quem decide o livro-caixa é sempre o servidor; a cópia local é
//!   cache. O inventário não sincroniza.
//!
//! Regra do quadro (a mesma de `comando`): a janela lê por `&self` e escreve por
//! `&mut self`, e nenhuma chamada daqui bloqueia esperando a rede. `propor`, `refazer` e
//! `descartar` só mexem na fila local; quem fala com o servidor é a sincronização, que
//! roda fora do quadro e é acompanhada por [`Sincronizacao::estado`].
//!
//! Nesta fatia a única escrita de ponta a ponta é **mudar a categoria de uma
//! movimentação** (grill Q6), com a versão de base de cada linha tocada (grill Q7:
//! `transactions.updated_at`).

use serde::{Deserialize, Serialize};

use crate::referencia::{Ref, Tipo};

/// O id local de uma alteração (a chave de `Ref { tipo: Alteracao, .. }`).
pub type IdAlteracao = String;

/// O que o usuário pediu. Uma Ação sobre **um** item (grill Q7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedidoDeAlteracao {
    /// Mudar a categoria de uma movimentação. `categoria: None` tira a categoria.
    MudarCategoria {
        movimentacao: Ref,
        categoria: Option<Ref>,
    },
}

impl PedidoDeAlteracao {
    /// O item que a alteração toca.
    pub fn alvo(&self) -> &Ref {
        match self {
            PedidoDeAlteracao::MudarCategoria { movimentacao, .. } => movimentacao,
        }
    }

    /// "Categoria": o que muda, como aparece na coluna "Alteração".
    pub fn o_que_muda(&self) -> &'static str {
        match self {
            PedidoDeAlteracao::MudarCategoria { .. } => "Categoria",
        }
    }
}

/// Onde a alteração está.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoDaAlteracao {
    /// Feita aqui, ainda não aceita pelo servidor. Vai na próxima sincronização.
    Aguardando,
    /// O servidor aplicou. Some de Aguardando; fica no histórico do item.
    Aceita,
    /// O servidor não aplicou. Fica visível até o usuário resolver.
    Recusada,
    /// Recusada e refeita sobre a versão nova: a refeita é outra alteração.
    Refeita,
    /// Recusada e descartada: vale a versão do servidor.
    Descartada,
}

impl EstadoDaAlteracao {
    pub fn rotulo(self) -> &'static str {
        match self {
            EstadoDaAlteracao::Aguardando => "Aguardando",
            EstadoDaAlteracao::Aceita => "Aceita",
            EstadoDaAlteracao::Recusada => "Recusada",
            EstadoDaAlteracao::Refeita => "Refeita",
            EstadoDaAlteracao::Descartada => "Descartada",
        }
    }

    /// Ainda pede atenção do usuário (Aguardando ou Recusada)?
    pub fn em_aberto(self) -> bool {
        matches!(
            self,
            EstadoDaAlteracao::Aguardando | EstadoDaAlteracao::Recusada
        )
    }
}

/// Uma linha do lado a lado: o mesmo campo na sua versão e no servidor agora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampoLadoALado {
    pub rotulo: String,
    /// "Sua versão": o item como ficaria com a sua alteração.
    pub sua: String,
    /// "No servidor agora": o item como o servidor o tem hoje.
    pub servidor: String,
}

impl CampoLadoALado {
    pub fn novo(
        rotulo: impl Into<String>,
        sua: impl Into<String>,
        servidor: impl Into<String>,
    ) -> Self {
        Self {
            rotulo: rotulo.into(),
            sua: sua.into(),
            servidor: servidor.into(),
        }
    }

    /// Os dois lados diferem neste campo (a janela destaca).
    pub fn difere(&self) -> bool {
        self.sua != self.servidor
    }
}

/// Por que o servidor recusou e as duas versões do item **inteiro** (grill Q7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recusa {
    /// "A movimentação mudou no servidor depois que você alterou."
    pub motivo: String,
    /// Instante ISO 8601 da recusa.
    pub quando: String,
    pub campos: Vec<CampoLadoALado>,
}

/// Uma alteração, como a área Sincronização e a página da alteração a mostram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alteracao {
    pub id: IdAlteracao,
    pub pedido: PedidoDeAlteracao,
    /// O nome do item alterado na hora da alteração ("Mercado Delta · 04/11").
    pub rotulo_do_alvo: String,
    /// A área de onde veio ("Movimentações").
    pub area: String,
    /// O valor antes e depois, como texto ("Mercado" → "Casa").
    pub antes: String,
    pub depois: String,
    /// Instante ISO 8601 em que o usuário fez a alteração.
    pub feita_em: String,
    /// A estação onde foi feita.
    pub estacao: Ref,
    pub estado: EstadoDaAlteracao,
    /// A versão de base de cada linha tocada (`tipo:chave` → `updated_at` lido do
    /// cache). O servidor recusa se alguma mudou.
    pub versoes_de_base: Vec<(Ref, String)>,
    /// Só em [`EstadoDaAlteracao::Recusada`] (e nas que saíram dela).
    pub recusa: Option<Recusa>,
    /// Refeita: a alteração nova, feita sobre a versão do servidor.
    pub refeita_em: Option<IdAlteracao>,
}

impl Alteracao {
    pub fn alvo_da_pagina(&self) -> Ref {
        Ref::nova(Tipo::Alteracao, self.id.clone())
    }

    /// O título da linha e da página: "Categoria · Mercado Delta · 04/11".
    pub fn rotulo(&self) -> String {
        format!("{} · {}", self.pedido.o_que_muda(), self.rotulo_do_alvo)
    }
}

/// Uma categoria que a Ação "Mudar categoria" oferece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpcaoDeCategoria {
    pub alvo: Ref,
    pub nome: String,
    /// "Despesa", "Receita": o grupo no seletor.
    pub grupo: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDeAlteracao {
    #[error("item não encontrado: {0}")]
    NaoEncontrado(Ref),
    #[error("a alteração {0} não existe")]
    AlteracaoInexistente(IdAlteracao),
    #[error("a alteração {id} está {estado:?}: só uma recusada pode ser refeita ou descartada")]
    NaoEstaRecusada {
        id: IdAlteracao,
        estado: EstadoDaAlteracao,
    },
    #[error("a categoria já é essa")]
    NadaMuda,
    #[error("esta estação foi revogada: o livro-caixa não aceita alterações dela")]
    EstacaoRevogada,
    #[error("o servidor tem um contrato mais novo: atualize o app para alterar")]
    ContratoMaisNovo,
    #[error("ainda não suportado: {0}")]
    NaoSuportado(String),
    #[error("erro local: {0}")]
    Local(String),
}

pub type ResultadoDeAlteracao<T> = Result<T, ErroDeAlteracao>;

/// As alterações do livro-caixa feitas nesta estação.
pub trait Alteracoes {
    /// Grava a alteração na fila local, como Aguardando. Não fala com a rede: ela vai na
    /// próxima sincronização. Lê a versão de base do cache.
    fn propor(&mut self, pedido: &PedidoDeAlteracao) -> ResultadoDeAlteracao<Alteracao>;

    /// As alterações desta estação, da mais nova para a mais antiga. `so_em_aberto`:
    /// só Aguardando e Recusada (as listas da área Sincronização).
    fn alteracoes(&self, so_em_aberto: bool) -> ResultadoDeAlteracao<Vec<Alteracao>>;

    /// Uma alteração pelo id (a página da alteração).
    fn alteracao(&self, id: &str) -> ResultadoDeAlteracao<Option<Alteracao>>;

    /// As alterações de um item (aba Alterações da movimentação), da mais nova à mais
    /// antiga.
    fn alteracoes_do_item(&self, alvo: &Ref) -> ResultadoDeAlteracao<Vec<Alteracao>> {
        Ok(self
            .alteracoes(false)?
            .into_iter()
            .filter(|a| a.pedido.alvo() == alvo)
            .collect())
    }

    /// "Refazer sobre a nova": a recusada vira Refeita e nasce uma alteração Aguardando
    /// com o mesmo pedido, sobre a versão que o servidor tem agora. Devolve a nova.
    fn refazer(&mut self, id: &str) -> ResultadoDeAlteracao<Alteracao>;

    /// "Descartar": a recusada vira Descartada e vale a versão do servidor.
    fn descartar(&mut self, id: &str) -> ResultadoDeAlteracao<()>;

    /// As categorias que "Mudar categoria" oferece, em ordem de nome.
    fn categorias(&self) -> ResultadoDeAlteracao<Vec<OpcaoDeCategoria>>;
}

/// A credencial de uma estação perante o servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Credencial {
    Ativa,
    Revogada,
}

/// Uma estação do dono, como o servidor a conhece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estacao {
    /// `Ref { tipo: Estacao, chave: <id no servidor> }`.
    pub alvo: Ref,
    pub nome: String,
    /// "Linux", "Windows", "macOS".
    pub sistema: String,
    /// Instante ISO 8601 do último contato com o servidor.
    pub ultima_sincronizacao: Option<String>,
    pub credencial: Credencial,
    /// É esta máquina (a marca "esta").
    pub esta: bool,
}

/// Uma troca com o servidor (o histórico fica na estação, não no servidor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Troca {
    /// Instantes ISO 8601.
    pub inicio: String,
    pub fim: Option<String>,
    /// As coleções que vieram novas (as outras responderam "não mudou").
    pub colecoes: Vec<String>,
    /// Quantas linhas chegaram nas coleções trocadas.
    pub linhas: i64,
    /// Quantas alterações foram enviadas, aceitas e recusadas.
    pub enviadas: i64,
    pub aceitas: i64,
    pub recusadas: i64,
    /// O erro, se a troca não terminou.
    pub erro: Option<String>,
}

/// O estado da rede, como a barra de status o mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoDaRede {
    /// A última troca terminou bem. Barra: "Sincronizado às HH:MM".
    Sincronizado,
    /// Uma troca está em andamento.
    Sincronizando,
    /// Sem rede (ou servidor fora do ar): mostra a cópia. Barra e faixa: "Sem rede,
    /// mostrando a cópia de HH:MM" (grill Q13).
    SemRede,
    /// O servidor respondeu "estação revogada": a estação apagou o cache e a credencial
    /// (grill Q11). Barra: "Estação revogada".
    EstacaoRevogada,
    /// O servidor tem regras mais novas que esta estação (o contrato ainda é aceito).
    /// Barra: "Servidor com regras mais novas".
    RegrasMaisNovas { versao_do_servidor: String },
    /// O servidor recusou o contrato desta estação (grill Q4): "atualize o app". O
    /// cache continua legível.
    AtualizeOApp,
    /// Esta máquina ainda não foi pareada (nunca fez login).
    NaoPareada,
    /// Outro erro da última troca, com a mensagem (sem segredo).
    Falhou(String),
}

/// O que a área Sincronização, a barra de status e a faixa offline mostram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoDaSincronizacao {
    /// Esta estação (nome e `Ref`), quando pareada.
    pub esta_estacao: Option<Estacao>,
    /// Onde está o servidor, para o cabeçalho ("servidor na VPS de São Paulo").
    pub servidor: String,
    /// Instante ISO 8601 da última troca que terminou bem.
    pub ultima_troca: Option<String>,
    /// Instante ISO 8601 da cópia que as áreas de Dinheiro mostram (em geral igual a
    /// `ultima_troca`). `None`: não há cópia (nunca sincronizou, ou foi apagada).
    pub copia_de: Option<String>,
    pub rede: EstadoDaRede,
    /// A versão das regras desta estação (`sbf --version`).
    pub versao_das_regras: String,
}

impl EstadoDaSincronizacao {
    /// A janela deve mostrar a faixa "Sem rede, mostrando a cópia de HH:MM" e a hora ao
    /// lado de Disponível agora?
    pub fn offline(&self) -> bool {
        matches!(self.rede, EstadoDaRede::SemRede)
    }
}

/// A sincronização desta estação. Nada aqui bloqueia o quadro.
pub trait Sincronizacao {
    /// O estado atual. Barato: a janela chama a cada quadro.
    fn estado(&self) -> EstadoDaSincronizacao;

    /// "Sincronizar agora": agenda uma troca e volta na hora. Se uma já está em
    /// andamento, não faz nada.
    fn sincronizar_agora(&mut self);

    /// As estações do dono, da última lista que o servidor mandou (a desta primeiro).
    fn estacoes(&self) -> ResultadoDeAlteracao<Vec<Estacao>>;

    /// "Revogar estação": pede ao servidor na próxima troca e marca a estação como
    /// revogada na lista. Revogar não apaga nada do servidor.
    fn revogar(&mut self, estacao: &Ref) -> ResultadoDeAlteracao<()>;

    /// "Renomear": pede ao servidor na próxima troca e já mostra o nome novo.
    fn renomear(&mut self, estacao: &Ref, nome: &str) -> ResultadoDeAlteracao<()>;

    /// As últimas trocas, da mais nova para a mais antiga.
    fn historico(&self) -> ResultadoDeAlteracao<Vec<Troca>>;
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn lado_a_lado_marca_so_o_que_difere() {
        let iguais = CampoLadoALado::novo("Valor", "R$ 10,00", "R$ 10,00");
        let diferentes = CampoLadoALado::novo("Categoria", "Casa", "Mercado");
        assert!(!iguais.difere());
        assert!(diferentes.difere());
    }

    #[test]
    fn em_aberto_e_aguardando_ou_recusada() {
        assert!(EstadoDaAlteracao::Aguardando.em_aberto());
        assert!(EstadoDaAlteracao::Recusada.em_aberto());
        assert!(!EstadoDaAlteracao::Aceita.em_aberto());
        assert!(!EstadoDaAlteracao::Refeita.em_aberto());
        assert!(!EstadoDaAlteracao::Descartada.em_aberto());
    }

    #[test]
    fn pedido_diz_o_alvo_e_o_que_muda() {
        let mov = Ref::nova(Tipo::Movimentacao, "m1");
        let pedido = PedidoDeAlteracao::MudarCategoria {
            movimentacao: mov.clone(),
            categoria: Some(Ref::nova(Tipo::Categoria, "c1")),
        };
        assert_eq!(pedido.alvo(), &mov);
        assert_eq!(pedido.o_que_muda(), "Categoria");
    }

    #[test]
    fn contrato_ida_e_volta_em_json() {
        let alteracao = Alteracao {
            id: "a1".to_owned(),
            pedido: PedidoDeAlteracao::MudarCategoria {
                movimentacao: Ref::nova(Tipo::Movimentacao, "m1"),
                categoria: None,
            },
            rotulo_do_alvo: "Mercado Exemplo · 04/11".to_owned(),
            area: "Movimentações".to_owned(),
            antes: "Mercado".to_owned(),
            depois: "Sem categoria".to_owned(),
            feita_em: "2026-11-04T12:00:00-03:00".to_owned(),
            estacao: Ref::nova(Tipo::Estacao, "e1"),
            estado: EstadoDaAlteracao::Recusada,
            versoes_de_base: vec![(
                Ref::nova(Tipo::Movimentacao, "m1"),
                "2026-11-04T11:00:00Z".to_owned(),
            )],
            recusa: Some(Recusa {
                motivo: "A movimentação mudou no servidor.".to_owned(),
                quando: "2026-11-04T12:05:00-03:00".to_owned(),
                campos: vec![CampoLadoALado::novo("Categoria", "Sem categoria", "Casa")],
            }),
            refeita_em: None,
        };
        let json = serde_json::to_string(&alteracao).expect("serializa");
        let volta: Alteracao = serde_json::from_str(&json).expect("lê");
        assert_eq!(volta, alteracao);
        assert_eq!(volta.alvo_da_pagina().to_string(), "alteracao:a1");
    }
}
