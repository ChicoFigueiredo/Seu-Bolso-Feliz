//! O texto extraído, tratado como o `e3-extrair.ts` trata: contagem de letras (`\p{L}`),
//! `canonical_fingerprint` (`computeCanonicalFingerprint` de `@sbf/operations`) e o corte de
//! 20 KB do `texto.conteudo`.
//!
//! O JS e o Rust divergem nas bordas (`toLowerCase`, `\s`, `trim`, `slice`), então cada uma é
//! reimplementada com a regra do JS e conferida contra saídas geradas pelo próprio TS
//! (`tests/fixtures/impressao-canonica.json`).

use sha2::{Digest, Sha256};
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

/// Abaixo disso, o PDF é tratado como imagem (`MIN_LETRAS` do `e3-extrair.ts`).
pub const MINIMO_DE_LETRAS: usize = 15;
/// O tamanho máximo de `texto.conteudo`, em unidades UTF-16 (`LIMITE_TEXTO` do `e3-extrair.ts`,
/// aplicado com `String.prototype.slice`).
pub const LIMITE_DO_TEXTO: usize = 20 * 1024;

/// Quantas letras (`\p{L}`, categoria geral Letter) o texto tem.
pub fn letras(texto: &str) -> usize {
    texto
        .chars()
        .filter(|c| c.general_category_group() == GeneralCategoryGroup::Letter)
        .count()
}

/// O `\s` (e o que o `trim` tira) do JavaScript: WhiteSpace e LineTerminator do ECMAScript.
/// Difere de `char::is_whitespace`: inclui U+FEFF e não inclui U+0085.
pub fn espaco_do_js(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `texto.toLowerCase().replace(/\s+/g, " ").trim()`.
pub fn normalizar(texto: &str) -> String {
    let minusculo = texto.to_lowercase();
    let mut saida = String::with_capacity(minusculo.len());
    let mut em_espaco = false;
    for c in minusculo.chars() {
        if espaco_do_js(c) {
            if !em_espaco {
                saida.push(' ');
                em_espaco = true;
            }
        } else {
            saida.push(c);
            em_espaco = false;
        }
    }
    // Depois do replace só sobra U+0020 como espaço: o trim tira os das pontas.
    saida.trim_matches(' ').to_owned()
}

/// O `canonical_fingerprint`: SHA-256 hex do texto normalizado, igual ao do Bun.
pub fn impressao_canonica(texto: &str) -> String {
    let digest = Sha256::digest(normalizar(texto).as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest.iter() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// `texto.slice(0, limite)` do JS (unidades UTF-16). Um par substituto cortado ao meio vira
/// U+FFFD, como o SQLite do Bun grava a metade solta.
pub fn cortar_como_o_js(texto: &str, limite: usize) -> String {
    let mut unidades = 0;
    for (i, c) in texto.char_indices() {
        let largura = c.len_utf16();
        if unidades + largura > limite {
            let mut saida = texto[..i].to_owned();
            if unidades < limite {
                saida.push('\u{FFFD}');
            }
            return saida;
        }
        unidades += largura;
    }
    texto.to_owned()
}

/// A saída do `pdftotext` como o Bun a lê (`new Response(stdout).text()`): UTF-8 com
/// substituição dos bytes inválidos e sem o BOM inicial.
pub fn decodificar_saida(bytes: &[u8]) -> String {
    let sem_bom = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8_lossy(sem_bom).into_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn conta_letras_como_o_js() {
        // º (U+00BA) é Lo: conta como letra, como no \p{L} do JS.
        assert_eq!(letras("Fatura nº 12 — ção"), 11);
        assert_eq!(letras("123 456,78"), 0);
    }

    #[test]
    fn normaliza_como_o_js() {
        assert_eq!(
            normalizar("  Olá\u{00A0}\u{00A0}MUNDO\r\n\tx \u{FEFF}"),
            "olá mundo x"
        );
        // U+0085 não é \s no JS.
        assert_eq!(normalizar("a\u{0085}b"), "a\u{0085}b");
    }

    #[test]
    fn corta_em_unidades_utf16() {
        assert_eq!(cortar_como_o_js("abc", 2), "ab");
        assert_eq!(cortar_como_o_js("abc", 5), "abc");
        assert_eq!(cortar_como_o_js("ação", 3), "açã");
        // 🔑 ocupa duas unidades: cortar no meio deixa a metade solta (U+FFFD).
        assert_eq!(cortar_como_o_js("a🔑b", 2), "a\u{FFFD}");
        assert_eq!(cortar_como_o_js("a🔑b", 3), "a🔑");
    }

    #[test]
    fn decodifica_como_o_bun() {
        assert_eq!(decodificar_saida(b"\xEF\xBB\xBFoi"), "oi");
        assert_eq!(decodificar_saida(b"a\xFFb"), "a\u{FFFD}b");
    }
}
