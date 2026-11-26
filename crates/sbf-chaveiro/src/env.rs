//! Ler uma chave de um texto no formato `.env`, sem carregar o arquivo no ambiente do processo.

/// O valor de `chave` num texto no formato `.env` (`CHAVE=valor`, `export CHAVE="valor"`,
/// `#` comenta). Só procura a chave pedida.
#[must_use]
pub fn valor_do_env(texto: &str, chave: &str) -> Option<String> {
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.starts_with('#') {
            continue;
        }
        let linha = linha.strip_prefix("export ").unwrap_or(linha).trim_start();
        let Some((k, v)) = linha.split_once('=') else {
            continue;
        };
        if k.trim() != chave {
            continue;
        }
        let v = v.trim();
        let v = if let Some(entre) = v
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .map(|(a, _)| a)
        {
            entre
        } else if let Some(entre) = v
            .strip_prefix('\'')
            .and_then(|r| r.split_once('\''))
            .map(|(a, _)| a)
        {
            entre
        } else {
            v.split(" #").next().unwrap_or(v).trim()
        };
        return (!v.is_empty()).then(|| v.to_owned());
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_formato_env_so_da_chave_pedida() {
        let t = "# comentario\nOUTRA=1\nTELEGRAM_API_ID=12345\nexport TELEGRAM_API_HASH=\"abc123\"\nVAZIA=\n";
        assert_eq!(valor_do_env(t, "TELEGRAM_API_ID").as_deref(), Some("12345"));
        assert_eq!(
            valor_do_env(t, "TELEGRAM_API_HASH").as_deref(),
            Some("abc123")
        );
        assert_eq!(valor_do_env(t, "VAZIA"), None);
        assert_eq!(valor_do_env(t, "NAO_TEM"), None);
        assert_eq!(valor_do_env("K='x y' # c", "K").as_deref(), Some("x y"));
        assert_eq!(valor_do_env("K=x # c", "K").as_deref(), Some("x"));
    }
}
