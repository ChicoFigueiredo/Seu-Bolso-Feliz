//! O modo `sbf coletor-vps` (ADR-014): as coletas que rodam 24 h na VPS. Toda a lógica mora em
//! `sbf-coletor`; aqui só a ligação ao `main`.

use std::process::ExitCode;

/// O braço `coletor-vps` do `main`. `args` vem sem o próprio `coletor-vps`.
pub fn rodar(args: &[String]) -> ExitCode {
    sbf_coletor::cli::rodar(args)
}
