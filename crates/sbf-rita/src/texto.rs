//! Cortes de texto para saídas curtas.

/// Corta `t` em no máximo `max` caracteres (com a reticência, quando corta).
pub(crate) fn cortar(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        return t.to_owned();
    }
    let mut s: String = t.chars().take(max.saturating_sub(1)).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nunca_passa_do_maximo() {
        assert_eq!(cortar("abc", 3), "abc");
        assert_eq!(cortar("abcd", 3), "ab…");
        assert_eq!(cortar(&"ã".repeat(2000), 1000).chars().count(), 1000);
        assert_eq!(cortar("abc", 0), "…");
    }
}
