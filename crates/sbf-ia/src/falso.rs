//! Provedor falso, roteirizado: para os testes de quem usa o motor. Nunca sai da memória.

use crate::{ErroDeIa, Pedido, Provedor, Resposta, ResultadoDeIa};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

type Funcao = dyn Fn(&Pedido) -> ResultadoDeIa<Resposta> + Send + Sync;

/// Responde na ordem do roteiro; com o roteiro vazio, usa a função (se houver) ou dá
/// [`ErroDeIa::Outro`]. Guarda os pedidos recebidos para o teste conferir.
pub struct Falso {
    nome: &'static str,
    roteiro: Mutex<VecDeque<ResultadoDeIa<Resposta>>>,
    funcao: Option<Box<Funcao>>,
    pedidos: Mutex<Vec<Pedido>>,
    chamadas: AtomicU64,
    rodadas: AtomicU64,
    dentro_do_teto: AtomicBool,
}

impl std::fmt::Debug for Falso {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Falso")
            .field("nome", &self.nome)
            .field("chamadas", &self.chamadas())
            .finish_non_exhaustive()
    }
}

impl Falso {
    /// Responde `roteiro` na ordem, uma por chamada.
    pub fn com_roteiro(roteiro: Vec<ResultadoDeIa<Resposta>>) -> Self {
        Self {
            nome: "falso",
            roteiro: Mutex::new(roteiro.into()),
            funcao: None,
            pedidos: Mutex::new(Vec::new()),
            chamadas: AtomicU64::new(0),
            rodadas: AtomicU64::new(0),
            dentro_do_teto: AtomicBool::new(true),
        }
    }

    /// Responde cada chamada com `funcao` (depois do roteiro, se ele existir).
    pub fn com_funcao(
        funcao: impl Fn(&Pedido) -> ResultadoDeIa<Resposta> + Send + Sync + 'static,
    ) -> Self {
        let mut falso = Self::com_roteiro(Vec::new());
        falso.funcao = Some(Box::new(funcao));
        falso
    }

    /// Outro nome em `Saida.provedor` e na chave do cache (para testar reserva).
    #[must_use]
    pub fn chamado(mut self, nome: &'static str) -> Self {
        self.nome = nome;
        self
    }

    /// Acrescenta uma resposta ao fim do roteiro.
    pub fn empilhar(&self, resposta: ResultadoDeIa<Resposta>) {
        self.roteiro
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(resposta);
    }

    /// Quantas vezes `pedir` foi chamado.
    pub fn chamadas(&self) -> u64 {
        self.chamadas.load(Ordering::SeqCst)
    }

    /// Quantas vezes `nova_rodada` foi chamado.
    pub fn rodadas(&self) -> u64 {
        self.rodadas.load(Ordering::SeqCst)
    }

    /// Os pedidos recebidos, em ordem.
    pub fn pedidos(&self) -> Vec<Pedido> {
        self.pedidos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// O que `dentro_do_teto` responde (padrão `true`).
    pub fn definir_dentro_do_teto(&self, dentro: bool) {
        self.dentro_do_teto.store(dentro, Ordering::SeqCst);
    }
}

impl Provedor for Falso {
    fn nome(&self) -> &'static str {
        self.nome
    }

    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        self.chamadas.fetch_add(1, Ordering::SeqCst);
        self.pedidos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(pedido.clone());
        let proxima = self
            .roteiro
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front();
        match (proxima, &self.funcao) {
            (Some(resposta), _) => resposta,
            (None, Some(funcao)) => funcao(pedido),
            (None, None) => Err(ErroDeIa::Outro("roteiro do falso esgotado".to_owned())),
        }
    }

    fn nova_rodada(&self) {
        self.rodadas.fetch_add(1, Ordering::SeqCst);
    }

    fn dentro_do_teto(&self) -> bool {
        self.dentro_do_teto.load(Ordering::SeqCst)
    }
}
