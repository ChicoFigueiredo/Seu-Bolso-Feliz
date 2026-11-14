//! O worker local (ADR-014): roda sem janela, como serviço do usuário, e mantém o inventário
//! em dia mesmo com a janela fechada.
//!
//! | Módulo | O que faz |
//! |---|---|
//! | [`config`] | `worker.json`: pastas vigiadas, intervalos, horário da triagem |
//! | [`trava`] | uma instância por pasta de dados (arquivo de lock) |
//! | [`registro`] | o log em arquivo rotacionado, **sem conteúdo de documento nem segredo** |
//! | [`estado`] | `worker-estado.json`, que a janela lê para a barra de status |
//! | [`unidade`] | gerar e ligar a unit systemd do usuário (Linux) |
//! | [`laco`] | o laço: vigia de pastas, reconciliação, nuvem, triagem |
//! | [`banco`] | abrir o `inventario.sqlite` compartilhado (WAL, espera, transações curtas) |

pub mod banco;
pub mod config;
pub mod erro;
pub mod estado;
pub mod laco;
pub mod registro;
pub mod trava;
pub mod unidade;

pub use erro::{Erro, Resultado};
