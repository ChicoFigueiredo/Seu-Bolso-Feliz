//! Uma instância do worker por pasta de dados: um arquivo de lock com `flock`
//! (`File::try_lock`, da biblioteca padrão). O lock é do sistema e some com o processo, mesmo
//! em queda: não fica trava velha.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write as _;
use std::path::Path;

use crate::{Erro, Resultado};

/// A trava; solte para liberar.
#[derive(Debug)]
pub struct Trava {
    _arquivo: File,
}

impl Trava {
    /// Toma a trava em `caminho` ou diz [`Erro::JaRodando`].
    ///
    /// # Errors
    /// [`Erro::JaRodando`] se outro processo a tem; falha de disco.
    pub fn tomar(caminho: &Path) -> Resultado<Self> {
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(caminho)?;
        match f.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(Erro::JaRodando),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        f.set_len(0)?;
        write!(f, "{}", std::process::id())?;
        Ok(Self { _arquivo: f })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_uma_instancia_e_a_trava_solta_ao_cair() {
        let d = tempfile::tempdir().unwrap();
        let caminho = d.path().join("worker.lock");
        let primeira = Trava::tomar(&caminho).unwrap();
        assert!(matches!(Trava::tomar(&caminho), Err(Erro::JaRodando)));
        drop(primeira);
        assert!(Trava::tomar(&caminho).is_ok());
    }
}
