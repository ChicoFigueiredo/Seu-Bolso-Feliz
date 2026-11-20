//! Escrita atômica com permissão 0600, para os arquivos de configuração e estado.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::Resultado;

/// Escreve por arquivo temporário e renomeação, com permissão 0600.
pub(crate) fn escrever_atomico(caminho: &Path, bytes: &[u8]) -> Resultado<()> {
    if let Some(pasta) = caminho.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, caminho)?;
    Ok(())
}
