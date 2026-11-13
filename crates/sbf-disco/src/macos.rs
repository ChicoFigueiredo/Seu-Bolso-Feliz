//! FileVault/APFS cifrado. **UNVERIFIED no macOS real** (esta máquina é Linux): só os
//! parsers são testados, com saídas de exemplo.
//!
//! 1. `df -P <caminho>` dá o ponto de montagem.
//! 2. `diskutil info <ponto>` tem a linha `FileVault: Yes|No` nos volumes APFS.
//! 3. Sem essa linha, `fdesetup status` (sem administrador) diz `FileVault is On.`/`Off.`

use crate::Cifra;

/// Ponto de montagem na saída do `df -P` (a última linha; o ponto é o que vem depois da
/// quinta coluna e pode ter espaços).
#[must_use]
pub fn ponto_de_montagem(saida_df: &str) -> Option<String> {
    let linha = saida_df.lines().rfind(|l| !l.trim().is_empty())?;
    let mut resto = linha;
    for _ in 0..5 {
        resto = resto.trim_start();
        let fim = resto.find(char::is_whitespace)?;
        resto = &resto[fim..];
    }
    let ponto = resto.trim();
    (ponto.starts_with('/')).then(|| ponto.to_owned())
}

/// Lê a linha `FileVault:` do `diskutil info`.
#[must_use]
pub fn interpretar_diskutil(saida: &str) -> Cifra {
    for linha in saida.lines() {
        if let Some(valor) = linha.trim().strip_prefix("FileVault:") {
            return match valor.trim().to_ascii_lowercase().as_str() {
                "yes" => Cifra::Cifrado,
                "no" => Cifra::NaoCifrado,
                _ => Cifra::Desconhecido,
            };
        }
    }
    Cifra::Desconhecido
}

/// Lê a resposta do `fdesetup status`.
#[must_use]
pub fn interpretar_fdesetup(saida: &str) -> Cifra {
    let texto = saida.to_ascii_lowercase();
    if texto.contains("filevault is on") {
        Cifra::Cifrado
    } else if texto.contains("filevault is off") {
        Cifra::NaoCifrado
    } else {
        Cifra::Desconhecido
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn cifra_do_volume(caminho: &std::path::Path) -> Cifra {
    use std::ffi::OsStr;
    use std::time::Duration;
    let limite = Duration::from_secs(10);
    let Some(ponto) =
        crate::executar_com_limite("df", &[OsStr::new("-P"), caminho.as_os_str()], limite)
            .and_then(|s| ponto_de_montagem(&s))
    else {
        return Cifra::Desconhecido;
    };
    let por_volume = crate::executar_com_limite(
        "diskutil",
        &[OsStr::new("info"), OsStr::new(&ponto)],
        limite,
    )
    .map_or(Cifra::Desconhecido, |s| interpretar_diskutil(&s));
    if por_volume != Cifra::Desconhecido {
        return por_volume;
    }
    crate::executar_com_limite("fdesetup", &["status"], limite)
        .map_or(Cifra::Desconhecido, |s| interpretar_fdesetup(&s))
}

#[cfg(test)]
mod testes {
    use super::*;

    const DF: &str = "Filesystem   512-blocks      Used Available Capacity  Mounted on\n\
/dev/disk3s5 1953595632 800000000 900000000    47%    /System/Volumes/Data\n";

    #[test]
    fn ponto_de_montagem_do_df() {
        assert_eq!(
            ponto_de_montagem(DF).as_deref(),
            Some("/System/Volumes/Data")
        );
        let com_espaco = "Filesystem 512-blocks Used Available Capacity Mounted on\n\
/dev/disk4s1 100 1 99 1% /Volumes/Disco Externo\n";
        assert_eq!(
            ponto_de_montagem(com_espaco).as_deref(),
            Some("/Volumes/Disco Externo")
        );
        assert_eq!(ponto_de_montagem(""), None);
        assert_eq!(ponto_de_montagem("lixo"), None);
    }

    #[test]
    fn linha_filevault_do_diskutil() {
        let sim = "   Volume Name:  Macintosh HD - Data\n   FileVault:                Yes\n   Encrypted:   No\n";
        let nao = "   Volume Name:  Externo\n   FileVault:                No\n";
        assert_eq!(interpretar_diskutil(sim), Cifra::Cifrado);
        assert_eq!(interpretar_diskutil(nao), Cifra::NaoCifrado);
        assert_eq!(
            interpretar_diskutil("   Volume Name:  HFS\n"),
            Cifra::Desconhecido
        );
        assert_eq!(
            interpretar_diskutil("   FileVault:  Locked\n"),
            Cifra::Desconhecido
        );
    }

    #[test]
    fn resposta_do_fdesetup() {
        assert_eq!(interpretar_fdesetup("FileVault is On.\n"), Cifra::Cifrado);
        assert_eq!(
            interpretar_fdesetup("FileVault is Off.\n"),
            Cifra::NaoCifrado
        );
        assert_eq!(interpretar_fdesetup("Error: algo\n"), Cifra::Desconhecido);
    }
}
