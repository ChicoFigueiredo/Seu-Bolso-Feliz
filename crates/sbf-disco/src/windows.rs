//! BitLocker, sem exigir administrador. **UNVERIFIED no Windows real** (esta máquina é Linux):
//! só o parser é testado, com saídas de exemplo.
//!
//! A propriedade `System.Volume.BitLockerProtection` do Shell lê o estado sem elevar
//! privilégio (ao contrário de `manage-bde`/`Get-BitLockerVolume`). Valores: `1` = proteção
//! ligada; `2` = desligada; qualquer outro (inclusive suspensa, que deixa a chave em claro)
//! conta como desconhecido, para o aviso aparecer.

use crate::Cifra;

/// Letra da unidade (`C`) de um caminho como `C:\Users\...` ou `c:/x`; `None` para UNC etc.
#[must_use]
pub fn letra_da_unidade(caminho: &str) -> Option<char> {
    let mut chars = caminho.chars();
    let letra = chars.next()?;
    (letra.is_ascii_alphabetic() && chars.next() == Some(':')).then(|| letra.to_ascii_uppercase())
}

/// Interpreta a saída do PowerShell (um número numa linha).
#[must_use]
pub fn interpretar_protecao(saida: &str) -> Cifra {
    match saida.trim().parse::<i64>() {
        Ok(1) => Cifra::Cifrado,
        Ok(2) => Cifra::NaoCifrado,
        _ => Cifra::Desconhecido,
    }
}

#[cfg(windows)]
pub(crate) fn cifra_do_volume(caminho: &std::path::Path) -> Cifra {
    use std::time::Duration;
    let texto = caminho.to_string_lossy();
    let Some(letra) = letra_da_unidade(texto.trim_start_matches(r"\\?\")) else {
        return Cifra::Desconhecido;
    };
    let script = format!(
        "(New-Object -ComObject Shell.Application).NameSpace('{letra}:').Self.ExtendedProperty('System.Volume.BitLockerProtection')"
    );
    crate::executar_com_limite(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script.as_str()],
        Duration::from_secs(10),
    )
    .map_or(Cifra::Desconhecido, |saida| interpretar_protecao(&saida))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn letra_da_unidade_de_caminhos() {
        assert_eq!(letra_da_unidade(r"C:\Users\x"), Some('C'));
        assert_eq!(letra_da_unidade("d:/financas"), Some('D'));
        assert_eq!(letra_da_unidade(r"\\servidor\pasta"), None);
        assert_eq!(letra_da_unidade("/mnt/d"), None);
        assert_eq!(letra_da_unidade(""), None);
    }

    #[test]
    fn protecao_do_bitlocker() {
        assert_eq!(interpretar_protecao("1\r\n"), Cifra::Cifrado);
        assert_eq!(interpretar_protecao("2\r\n"), Cifra::NaoCifrado);
        assert_eq!(interpretar_protecao("0"), Cifra::Desconhecido);
        assert_eq!(interpretar_protecao("3"), Cifra::Desconhecido);
        assert_eq!(interpretar_protecao(""), Cifra::Desconhecido);
        assert_eq!(interpretar_protecao("erro qualquer"), Cifra::Desconhecido);
    }
}
