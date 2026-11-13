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

/// Roda um comando com limite de tempo e devolve a saída padrão, ou `None` se falhou,
/// saiu com erro ou estourou o tempo.
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn executar_com_limite(
    mut comando: std::process::Command,
    limite: std::time::Duration,
) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;
    let mut filho = comando
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut saida = filho.stdout.take()?;
    let leitor = std::thread::spawn(move || {
        let mut texto = String::new();
        let _ = saida.read_to_string(&mut texto);
        texto
    });
    let inicio = Instant::now();
    loop {
        match filho.try_wait() {
            Ok(Some(estado)) => {
                let texto = leitor.join().ok()?;
                return estado.success().then_some(texto);
            }
            Ok(None) if inicio.elapsed() < limite => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = filho.kill();
                let _ = filho.wait();
                return None;
            }
        }
    }
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
}
