//! Executor dos scripts Luau das regras de reconhecimento (spec 13 §4, ADR-021).
//!
//! Dois sabores de [`sbf_regras::script::ExecutorDeScript`]:
//!
//! - [`ExecutorNoProcesso`]: a VM Luau no processo atual (sandbox e limites de VM). É o que
//!   roda dentro do filho e nos testes.
//! - [`ExecutorFilho`]: o que o app usa. Sobe um processo filho (`sbf script --servir` ou o
//!   binário `sbf-script-filho`), endurecido pelo SO, e fala com ele em linhas JSON
//!   ([`servir`]); o pai mata o filho no prazo.

mod ajuda;
mod endurecer;
mod filho;
mod no_processo;
mod protocolo;

pub use ajuda::{
    centavos, data, dias_da_semana_no_mes, mul_div_arred, nome_do_dia_da_semana, normalizar, reais,
};
pub use endurecer::{Estado, RelatorioDeEndurecimento, endurecer};
pub use filho::ExecutorFilho;
pub use no_processo::ExecutorNoProcesso;
pub use protocolo::{servir, servir_endurecido};
