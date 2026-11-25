//! Carimba o binário com o commit do SBF, o commit da bancada (dependência por caminho
//! `../bancada`) e a data da compilação. Nada aqui pode quebrar o build: sem git, sem a pasta
//! da bancada (CI, worktree solta) ou com qualquer erro, o carimbo vira `desconhecida`.
//!
//! Exporta por `cargo:rustc-env`: `SBF_REV`, `SBF_SUJO` (0/1), `BANCADA_REV`, `BANCADA_SUJA`
//! (0/1) e `SBF_COMPILADO_EM` (AAAA-MM-DD, UTC).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const DESCONHECIDA: &str = "desconhecida";

/// Roda `git` em `pasta`; `None` se não há git, a pasta não existe ou o comando falha.
fn git(pasta: &Path, args: &[&str]) -> Option<String> {
    if !pasta.is_dir() {
        return None;
    }
    let saida = Command::new("git")
        .arg("-C")
        .arg(pasta)
        .args(args)
        .output()
        .ok()?;
    if !saida.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&saida.stdout).trim().to_owned())
}

/// O commit curto e se a árvore tem alterações (inclui arquivos novos não rastreados).
fn carimbo_do_repo(pasta: &Path) -> (String, bool) {
    let Some(rev) = git(pasta, &["rev-parse", "--short=12", "HEAD"]).filter(|r| !r.is_empty())
    else {
        return (DESCONHECIDA.to_owned(), false);
    };
    let suja = git(pasta, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    (rev, suja)
}

/// Pede ao cargo para refazer o carimbo quando o repositório em `pasta` muda: o HEAD, a
/// referência que ele aponta e o índice (resolvidos com `--git-path`, que funciona em
/// worktree, onde `.git` é um arquivo), e as pastas de código, para a marca de "sujo" não
/// ficar velha quando um arquivo é editado sem ir para o índice.
fn vigiar(pasta: &Path, codigo: &[&str]) {
    for nome in codigo {
        let caminho = pasta.join(nome);
        if caminho.exists() {
            println!("cargo:rerun-if-changed={}", caminho.display());
        }
    }
    let gancho = |caminho: &str| {
        if let Some(c) = git(pasta, &["rev-parse", "--git-path", caminho]) {
            let c = PathBuf::from(c);
            let c = if c.is_absolute() { c } else { pasta.join(c) };
            println!("cargo:rerun-if-changed={}", c.display());
        }
    };
    gancho("HEAD");
    gancho("index");
    if let Some(r) = git(pasta, &["symbolic-ref", "-q", "HEAD"]).filter(|r| !r.is_empty()) {
        gancho(&r);
    }
}

/// AAAA-MM-DD (UTC) de um instante em segundos desde 1970, sem dependência de calendário
/// (algoritmo `civil_from_days` de Howard Hinnant).
fn data_utc(segundos: u64) -> String {
    let dias = (segundos / 86_400) as i64;
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let ano = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = if mes <= 2 { ano + 1 } else { ano };
    format!("{ano:04}-{mes:02}-{dia:02}")
}

fn agora_em_segundos() -> u64 {
    // Compilação reproduzível: respeita SOURCE_DATE_EPOCH quando existe.
    if let Some(s) = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        return s;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let manifesto = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    // crates/sbf -> raiz do SBF; a bancada é irmã da raiz (mesmo caminho do Cargo.toml).
    let raiz = manifesto.join("../..");
    let bancada = raiz.join("../bancada");

    let (sbf_rev, sbf_sujo) = carimbo_do_repo(&raiz);
    let (bancada_rev, bancada_suja) = carimbo_do_repo(&bancada);
    vigiar(&raiz, &["crates", "Cargo.toml", "Cargo.lock"]);
    vigiar(&bancada, &["crates", "Cargo.toml"]);

    let marca = |b: bool| if b { "1" } else { "0" };
    println!("cargo:rustc-env=SBF_REV={sbf_rev}");
    println!("cargo:rustc-env=SBF_SUJO={}", marca(sbf_sujo));
    println!("cargo:rustc-env=BANCADA_REV={bancada_rev}");
    println!("cargo:rustc-env=BANCADA_SUJA={}", marca(bancada_suja));
    println!(
        "cargo:rustc-env=SBF_COMPILADO_EM={}",
        data_utc(agora_em_segundos())
    );
}
