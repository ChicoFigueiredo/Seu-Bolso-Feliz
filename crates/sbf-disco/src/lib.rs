//! Aviso de disco sem cifra (ADR-015, spec 06).
//!
//! [`cifra_do_volume`] responde se o volume que contém um caminho é cifrado. Ela **nunca
//! falha**: qualquer dúvida (sistema de arquivos de rede, container, ferramenta ausente,
//! saída que não reconhecemos) vira [`Cifra::Desconhecido`]. A barra de status mostra o
//! aviso para `NaoCifrado` e `Desconhecido`; nada é bloqueado.
//!
//! | SO | Como | Verificação |
//! |---|---|---|
//! | Linux | `/proc/self/mountinfo` e `/sys/class/block/*/dm/uuid` (`CRYPT-`), subindo por `slaves/` | testado aqui (árvores de exemplo e a máquina real) |
//! | Windows | PowerShell, propriedade `System.Volume.BitLockerProtection` do Shell, sem administrador | **UNVERIFIED** no Windows real; só o parser é testado |
//! | macOS | `diskutil info` (linha `FileVault:`), com `fdesetup status` de reserva | **UNVERIFIED** no macOS real; só o parser é testado |

use std::path::Path;

pub mod linux;
pub mod macos;
pub mod windows;

/// Estado de cifra do volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cifra {
    /// O volume está sobre dm-crypt/LUKS, BitLocker ou FileVault.
    Cifrado,
    /// Descobrimos o dispositivo e não há cifra nenhuma entre ele e o disco.
    NaoCifrado,
    /// Não deu para saber.
    Desconhecido,
}

impl Cifra {
    /// A barra de status deve mostrar o aviso (`NaoCifrado` e `Desconhecido`).
    #[must_use]
    pub fn pede_aviso(self) -> bool {
        self != Self::Cifrado
    }
}

/// Texto do aviso para a barra de status, ou `None` se o volume é cifrado.
#[must_use]
pub fn texto_do_aviso(cifra: Cifra) -> Option<&'static str> {
    match cifra {
        Cifra::Cifrado => None,
        Cifra::NaoCifrado => Some(
            "O disco do cofre não está cifrado: quem pegar o computador lê seus documentos. \
             Ative BitLocker, FileVault ou LUKS.",
        ),
        Cifra::Desconhecido => Some(
            "Não foi possível saber se o disco do cofre é cifrado. Confira se BitLocker, \
             FileVault ou LUKS estão ativos.",
        ),
    }
}

/// Descobre se o volume que contém `caminho` é cifrado. Nunca falha nem bloqueia.
#[must_use]
pub fn cifra_do_volume(caminho: &Path) -> Cifra {
    #[cfg(target_os = "linux")]
    {
        linux::cifra_do_volume(caminho)
    }
    #[cfg(windows)]
    {
        windows::cifra_do_volume(caminho)
    }
    #[cfg(target_os = "macos")]
    {
        macos::cifra_do_volume(caminho)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = caminho;
        Cifra::Desconhecido
    }
}

/// O máximo guardado da saída de `df`, `diskutil`, `fdesetup` ou do PowerShell: poucas
/// linhas.
#[cfg(any(windows, target_os = "macos", test))]
const SAIDA_MAXIMA: usize = 1024 * 1024;

/// Roda `programa` (resolvido nas pastas do sistema pela regra do `sbf-processo`, nunca pelo
/// `PATH`), com o ambiente mínimo e prazo, e devolve a saída padrão; `None` se não achou,
/// saiu com erro, estourou o tempo ou a saída passou do teto.
#[cfg(any(windows, target_os = "macos", test))]
pub(crate) fn executar_com_limite<S: AsRef<std::ffi::OsStr>>(
    programa: &str,
    argumentos: &[S],
    limite: std::time::Duration,
) -> Option<String> {
    let caminho = sbf_processo::localizar(programa)?;
    let saida = sbf_processo::Chamada::nova(caminho)
        .argumentos(argumentos)
        .com_prazo(limite)
        .com_saida_maxima(SAIDA_MAXIMA)
        .com_erro_maximo(4096)
        .rodar()
        .ok()?;
    (saida.status.success() && !saida.stdout_cortado)
        .then(|| String::from_utf8_lossy(&saida.stdout).into_owned())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aviso_so_para_o_que_nao_e_cifrado() {
        assert!(texto_do_aviso(Cifra::Cifrado).is_none());
        assert!(texto_do_aviso(Cifra::NaoCifrado).is_some());
        assert!(texto_do_aviso(Cifra::Desconhecido).is_some());
        assert!(!Cifra::Cifrado.pede_aviso());
        assert!(Cifra::Desconhecido.pede_aviso());
    }

    #[cfg(unix)]
    #[test]
    fn programa_externo_pelas_pastas_do_sistema_com_prazo() {
        use std::time::Duration;
        let ok = executar_com_limite("sh", &["-c", "echo ok"], Duration::from_secs(10));
        assert_eq!(ok.as_deref(), Some("ok\n"));
        assert_eq!(
            executar_com_limite("sh", &["-c", "exit 3"], Duration::from_secs(10)),
            None,
            "saiu com erro"
        );
        assert_eq!(
            executar_com_limite("sh", &["-c", "sleep 5"], Duration::from_millis(100)),
            None,
            "passou do prazo"
        );
        assert_eq!(
            executar_com_limite::<&str>("nao-existe-sbf-ficticio", &[], Duration::from_secs(1)),
            None
        );
    }
}
