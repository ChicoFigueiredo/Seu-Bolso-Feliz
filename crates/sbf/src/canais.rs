//! O modo `sbf canais` (ADR-014, ADR-015, ADR-017): o bot do Telegram e os alertas, na VPS.
//! Toda a lógica mora em `sbf-canais`; aqui só a ligação ao `main`.

use std::process::ExitCode;

/// O braço `canais` do `main`. `args` vem sem o próprio `canais`.
pub fn rodar(args: &[String]) -> ExitCode {
    sbf_canais::cli::rodar(args)
}
