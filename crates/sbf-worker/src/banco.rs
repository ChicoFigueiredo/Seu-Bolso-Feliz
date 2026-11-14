//! O `inventario.sqlite` é compartilhado entre a janela, o worker e (por ora) os scripts Bun.
//! As regras da convivência (LICOES.md do Bun e ADR-014):
//!
//! - **WAL**: leitor não bloqueia escritor e o contrário;
//! - **`busy_timeout` de 30 s** (o mesmo do Bun e do `sbf-inventario`);
//! - **transações curtas**: cada passo do worker abre, grava e fecha; nada de transação
//!   aberta durante `rclone`, `claude` ou espera. O varredor de pastas confirma por
//!   diretório, e a triagem grava por pedaço.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::Resultado;

/// A espera por um lock antes de `SQLITE_BUSY`.
pub const ESPERA: Duration = Duration::from_secs(30);

/// Abre uma conexão de escrita com os PRAGMAs do inventário.
///
/// # Errors
/// Falha do SQLite.
pub fn abrir(caminho: &Path) -> Resultado<Connection> {
    let c = Connection::open(caminho)?;
    c.busy_timeout(ESPERA)?;
    c.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    c.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
    Ok(c)
}
