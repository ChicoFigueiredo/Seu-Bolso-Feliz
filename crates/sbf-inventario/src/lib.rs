//! Domínio e dados do inventário do Seu Bolso Feliz (spec 06, fatia 1). Sem egui.
//!
//! Cada módulo tem um dono nesta rodada; ninguém escreve no módulo de outro.
//!
//! | Módulo | Dono |
//! |---|---|
//! | `referencia`, `consulta`, `comando`, `esquema` | integrador (as costuras) |
//! | `banco`, `migracoes`, `decisao`, `projecao`, `lote`, `pendencia` | agente do núcleo |
//! | `movimento`, `vistas`, `visao_geral`, `relatorios`, `relacoes`, `rastro`, `leitor` | agente de leitura |
//! | `protegidos` | agente do chaveiro e do PDF |
//! | `manutencao` | comandos de linha de comando (`sbf reprojetar`) |

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
pub mod leitor;
pub mod movimento;
pub mod rastro;
pub mod relacoes;
pub mod relatorios;
pub mod visao_geral;
pub mod vistas;

// PDF protegido: alvos, regras, tentativas e pendência por falha.
pub mod protegidos;

// Manutenção pela linha de comando.
pub mod manutencao;

pub use comando::{Acao, Comandos, ErroDeComando, IdDecisao, PedidoDeDecisao};
pub use consulta::{Consulta, ErroDeConsulta};
pub use referencia::{Ref, Tipo};
