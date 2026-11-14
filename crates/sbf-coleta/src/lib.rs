//! Coleta do inventário em fontes externas. Sem egui; síncrona, cancelável e com progresso,
//! para o futuro `sbf worker` e para a janela chamarem do mesmo jeito.
//!
//! - [`areas`]: em que camada cai cada pasta e o que fazer com cada arquivo (porte fiel de
//!   `workers/inventario/src/areas.ts`).
//! - [`pasta_sbf`]: a pasta `SBF/` da nuvem é do próprio app e nunca é coletada.
//! - [`nuvem`]: Google Drive e OneDrive via rclone (porte de `coletar-nuvem.ts`).

pub mod areas;
pub mod erro;
pub mod nuvem;
pub mod pasta_sbf;

pub use erro::{Erro, Resultado};
pub use nuvem::{Andamento, ColetaDeNuvem, Fase, Item, Nuvem, Relatorio};
