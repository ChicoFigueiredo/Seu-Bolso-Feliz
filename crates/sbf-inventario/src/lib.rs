//! Domínio e dados do inventário do Seu Bolso Feliz (spec 06, fatia 1). Sem egui.
//!
//! Cada módulo tem um dono nesta rodada; ninguém escreve no módulo de outro.
//!
//! | Módulo | Dono |
//! |---|---|
//! | `referencia`, `consulta`, `comando`, `esquema` | integrador (as costuras) |
//! | `alteracao` | agente da janela de Dinheiro (o contrato), implementado pela estação |
//! | `banco`, `migracoes`, `decisao`, `projecao`, `lote`, `pendencia` | agente do núcleo |
//! | `movimento`, `vistas`, `visao_geral`, `relatorios`, `relacoes`, `rastro`, `leitor` | agente de leitura |
//! | `protegidos` | agente do chaveiro e do PDF |
//! | `manutencao` | comandos de linha de comando (`sbf reprojetar`) |

pub mod alteracao;
pub mod comando;
pub mod consulta;
pub mod esquema;
pub mod referencia;

// Núcleo: abrir, migrar, decidir, projetar, desfazer, lotes, pendências.
pub mod banco;
pub mod decisao;
pub mod lote;
pub mod migracoes;
pub mod pendencia;
pub mod projecao;

// Leitura: vistas, Visão geral, Relatórios do §11, relações, totais e rastro.
pub mod conciliacao;
pub mod leitor;
pub mod movimento;
pub mod rastro;
pub mod relacoes;
pub mod relatorios;
pub mod visao_geral;
pub mod vistas;

// Triagem por IA (spec 09): obsolescência das sugestões e o registro do que saiu.
pub mod sugestao;

// PDF protegido: alvos, regras, tentativas e pendência por falha.
pub mod protegidos;

// O texto extraído de um achado e o `canonical_fingerprint`.
pub mod texto;

// Manutenção pela linha de comando.
pub mod manutencao;

pub use alteracao::{Alteracoes, Sincronizacao};
pub use comando::{Acao, Comandos, ErroDeComando, IdDecisao, PedidoDeDecisao};
pub use consulta::{Consulta, ErroDeConsulta};
pub use referencia::{Ref, Tipo};
