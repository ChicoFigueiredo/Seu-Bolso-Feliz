//! Rita (spec 15): a assistente de IA dos padrões de reconhecimento. Sem interface gráfica.
//!
//! - [`contexto`]: o que Rita enxerga (o item na tela, a área, o filtro, a lista).
//! - [`ferramentas`]: as oito ferramentas e o teto de 8 chamadas por pedido ([`Sessao`]).
//! - [`pre_teste`]: o ensaio de um rascunho sobre o item, os exemplos e o cofre (D15).
//! - [`laco`]: o laço do agente sobre um [`sbf_ia::ProvedorDeConversa`].
//! - [`prompt`]: as instruções e o bloco de dados delimitado.
//!
//! Rita só escreve no `regras.sqlite`, só **rascunhos**, e nunca ativa, consolida, reverte,
//! vincula nem concilia.

pub mod claude;
pub mod cofre;
pub mod contexto;
pub mod diagnostico;
pub mod esqueleto;
pub mod ferramentas;
pub mod laco;
pub mod mcp;
pub mod pre_teste;
pub mod prompt;
pub mod provedores;
pub mod registro;
pub(crate) mod texto;

pub use claude::ClaudeDeRita;
pub use cofre::{FonteDeSimilares, SimilaresDoCofre, SimilaresFixos};
pub use contexto::{
    Contexto, DadosDaTela, TituloDaLista, entrada_de_documento, entrada_de_registro,
};
pub use esqueleto::{Esqueleto, esqueleto_resumido};
pub use ferramentas::{
    ChamadaFeita, EstadoDaSessao, FERRAMENTAS, MAXIMO_DE_CHAMADAS, RascunhoCriado, Sessao,
    Tentativa, definicoes, motivo_do_pedido, rastro_resumido,
};
pub use laco::{Evento, Parada, Relatorio, Rita};
pub use pre_teste::{PreTeste, pre_testar};
pub use provedores::{
    ComCache, ConfiguracaoDeRita, GuardaDeProvedores, GuardaEmMemoria, ProvedorConfigurado,
    ProvedorDaRita, TipoDeProvedor, construir,
};
pub use registro::{DESTINATARIO, abrir_registro};
