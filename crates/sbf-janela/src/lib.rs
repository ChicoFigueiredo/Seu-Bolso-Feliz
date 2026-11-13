//! A janela do Seu Bolso Feliz (spec 06, ticket T9, mais as interfaces de Lotes, T10, e
//! de Relatórios, T14): áreas, tabela densa, páginas de item, rastro, Ações do §10,
//! lotes, painéis do dock e o modo demonstração.
//!
//! Escrita contra as costuras de `sbf-inventario`: lê por `&dyn Consulta` e escreve por
//! `&mut dyn Comandos`. Não depende da `bancada-casca`: a [`moldura`] é provisória e o
//! integrador liga as peças de [`Janela`] à casca.

pub mod acoes;
pub mod areas;
pub mod componentes;
pub mod demonstracao;
pub mod dock;
pub mod formato;
pub mod icones;
pub mod janela;
pub mod leitura;
pub mod moldura;
pub mod navegacao;
pub mod pagina;
pub mod painel;
pub mod tabela;
pub mod tema;

pub use acoes::{ACOES, Aba, ContextoDasAcoes, DefinicaoDeAcao, Disponibilidade, IdAcao};
pub use dock::PainelDoDock;
pub use janela::{Janela, Ligacoes};
pub use moldura::{EstadoDaBarra, MolduraProvisoria};
pub use navegacao::{Destino, Navegacao, Pedido};
