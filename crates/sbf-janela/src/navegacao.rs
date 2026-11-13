//! O modelo de navegação da janela: o que está no centro, o histórico Voltar/Avançar e
//! os painéis flutuantes (Ctrl+clique). Puro, sem egui, para testar e para a casca ligar.
//!
//! Regras (spec 06, "Navegação relacional"):
//! - clique abre no centro e entra no histórico; abrir depois de voltar descarta o futuro;
//! - Ctrl+clique abre num painel flutuante e **não** mexe no histórico do centro;
//! - abrir o mesmo destino que já está no centro não cria passo novo.

use sbf_inventario::comando::{IdDecisao, PedidoDeDecisao};
use sbf_inventario::consulta::{Area, PedidoDeTabela};
use sbf_inventario::{Ref, Tipo};

use crate::acoes::IdAcao;

/// O que pode estar no centro ou num painel flutuante.
#[derive(Debug, Clone, PartialEq)]
pub enum Destino {
    /// Uma área do grupo Inventário, ou Relatórios.
    Area(Area),
    /// Uma área de fatia futura (o id da tabela de [`crate::areas::AREAS_FUTURAS`]).
    AreaFutura(&'static str),
    /// A página de um item.
    Item(Ref),
    /// A lista do que soma (ou conta) um número clicável.
    Total {
        titulo: String,
        pedido: PedidoDeTabela,
    },
}

impl Destino {
    /// O tipo do item, quando o destino é uma página de item.
    pub fn tipo(&self) -> Option<Tipo> {
        match self {
            Destino::Item(r) => Some(r.tipo),
            _ => None,
        }
    }
}

/// Tudo o que a interface pode pedir. As vistas só emitem pedidos; quem os aplica é
/// [`crate::Janela::processar`], fora do desenho (nenhuma escrita atravessa um quadro).
#[derive(Debug, Clone, PartialEq)]
pub enum Pedido {
    /// Clique: abre no centro.
    Abrir(Destino),
    /// Ctrl+clique: abre num painel flutuante.
    AbrirFlutuante(Destino),
    FecharFlutuante(u64),
    Voltar,
    Avancar,
    /// Uma Ação da lista única, sobre a seleção atual.
    Executar(IdAcao),
    /// Uma decisão já montada (por um diálogo ou por um botão de página).
    Decidir(PedidoDeDecisao),
    /// Desfazer uma decisão específica (painel Histórico de decisões).
    Desfazer(IdDecisao),
    /// Novo lote com este nome.
    CriarLote(String),
    /// Escolhe o lote atual (Lotes: Adicionar seleção, Remover, Aprovar).
    EscolherLote(Ref),
    /// Copia para a área de transferência (Copiar rastro, Copiar hash).
    Copiar(String),
}

impl Pedido {
    /// Abre no centro ou, com Ctrl, num painel flutuante.
    pub fn abrir(destino: Destino, flutuante: bool) -> Pedido {
        if flutuante {
            Pedido::AbrirFlutuante(destino)
        } else {
            Pedido::Abrir(destino)
        }
    }
}

/// Um painel flutuante aberto por Ctrl+clique.
#[derive(Debug, Clone, PartialEq)]
pub struct Flutuante {
    pub id: u64,
    pub destino: Destino,
}

/// O histórico do centro e os flutuantes.
#[derive(Debug, Clone, PartialEq)]
pub struct Navegacao {
    passos: Vec<Destino>,
    atual: usize,
    flutuantes: Vec<Flutuante>,
    proximo_flutuante: u64,
}

impl Navegacao {
    pub fn nova(inicio: Destino) -> Self {
        Self {
            passos: vec![inicio],
            atual: 0,
            flutuantes: Vec::new(),
            proximo_flutuante: 1,
        }
    }

    /// O que está no centro.
    pub fn atual(&self) -> &Destino {
        // `passos` nunca fica vazio: nasce com um e só cresce ou é truncado depois de `atual`.
        &self.passos[self.atual.min(self.passos.len() - 1)]
    }

    pub fn pode_voltar(&self) -> bool {
        self.atual > 0
    }

    pub fn pode_avancar(&self) -> bool {
        self.atual + 1 < self.passos.len()
    }

    /// Os passos do histórico, do mais antigo ao mais novo, e o índice do atual.
    pub fn passos(&self) -> (&[Destino], usize) {
        (&self.passos, self.atual)
    }

    pub fn flutuantes(&self) -> &[Flutuante] {
        &self.flutuantes
    }

    pub fn abrir(&mut self, destino: Destino) {
        if *self.atual() == destino {
            return;
        }
        self.passos.truncate(self.atual + 1);
        self.passos.push(destino);
        self.atual = self.passos.len() - 1;
    }

    pub fn voltar(&mut self) -> bool {
        if self.pode_voltar() {
            self.atual -= 1;
            true
        } else {
            false
        }
    }

    pub fn avancar(&mut self) -> bool {
        if self.pode_avancar() {
            self.atual += 1;
            true
        } else {
            false
        }
    }

    pub fn abrir_flutuante(&mut self, destino: Destino) -> u64 {
        let id = self.proximo_flutuante;
        self.proximo_flutuante += 1;
        self.flutuantes.push(Flutuante { id, destino });
        id
    }

    pub fn fechar_flutuante(&mut self, id: u64) {
        self.flutuantes.retain(|f| f.id != id);
    }

    /// Aplica um pedido de navegação. Devolve `false` para os pedidos que não são de
    /// navegação (Ações, decisões, lotes), que ficam para quem chamou.
    pub fn aplicar(&mut self, pedido: &Pedido) -> bool {
        match pedido {
            Pedido::Abrir(d) => self.abrir(d.clone()),
            Pedido::AbrirFlutuante(d) => {
                self.abrir_flutuante(d.clone());
            }
            Pedido::FecharFlutuante(id) => self.fechar_flutuante(*id),
            Pedido::Voltar => {
                self.voltar();
            }
            Pedido::Avancar => {
                self.avancar();
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn forn(nome: &str) -> Destino {
        Destino::Item(Ref::nova(Tipo::Fornecedor, nome))
    }

    #[test]
    fn abrir_entra_no_historico_e_voltar_refaz_o_caminho() {
        let mut n = Navegacao::nova(Destino::Area(Area::Fornecedores));
        n.abrir(forn("Energia Exemplo"));
        n.abrir(Destino::Item(Ref::nova(Tipo::Registro, "7")));
        assert!(n.voltar());
        assert_eq!(n.atual(), &forn("Energia Exemplo"));
        assert!(n.voltar());
        assert_eq!(n.atual(), &Destino::Area(Area::Fornecedores));
        assert!(!n.voltar());
        assert!(n.avancar());
        assert!(n.avancar());
        assert_eq!(n.atual(), &Destino::Item(Ref::nova(Tipo::Registro, "7")));
        assert!(!n.avancar());
    }

    #[test]
    fn navegar_depois_de_voltar_descarta_o_futuro() {
        let mut n = Navegacao::nova(Destino::Area(Area::ATriar));
        n.abrir(forn("A"));
        n.abrir(forn("B"));
        n.voltar();
        n.abrir(forn("C"));
        assert!(!n.pode_avancar());
        let (passos, atual) = n.passos();
        assert_eq!(passos.len(), 3);
        assert_eq!(passos[atual], forn("C"));
        n.voltar();
        assert_eq!(n.atual(), &forn("A"));
    }

    #[test]
    fn abrir_o_mesmo_destino_nao_cria_passo() {
        let mut n = Navegacao::nova(Destino::Area(Area::ATriar));
        n.abrir(forn("A"));
        n.abrir(forn("A"));
        assert_eq!(n.passos().0.len(), 2);
    }

    #[test]
    fn flutuante_nao_mexe_no_historico_do_centro() {
        let mut n = Navegacao::nova(Destino::Area(Area::Registros));
        n.abrir(forn("A"));
        let antes = n.passos().0.to_vec();
        assert!(n.aplicar(&Pedido::AbrirFlutuante(forn("B"))));
        assert!(n.aplicar(&Pedido::AbrirFlutuante(forn("C"))));
        assert_eq!(n.passos().0, antes.as_slice());
        assert_eq!(n.atual(), &forn("A"));
        assert_eq!(n.flutuantes().len(), 2);
        let id = n.flutuantes()[0].id;
        n.aplicar(&Pedido::FecharFlutuante(id));
        assert_eq!(n.flutuantes().len(), 1);
        assert_eq!(n.flutuantes()[0].destino, forn("C"));
        assert_eq!(n.passos().0, antes.as_slice());
    }

    #[test]
    fn pedidos_que_nao_sao_de_navegacao_ficam_para_quem_chamou() {
        let mut n = Navegacao::nova(Destino::Area(Area::Registros));
        assert!(!n.aplicar(&Pedido::Executar(IdAcao::Aprovar)));
        assert!(!n.aplicar(&Pedido::Desfazer(3)));
    }
}
