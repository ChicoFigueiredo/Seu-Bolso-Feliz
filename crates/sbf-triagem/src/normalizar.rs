//! Normalização de texto: fornecedor sem acento e sem ruído, comparação de respostas e
//! truncamento de texto longo.

/// Tira o acento de uma letra latina comum em português.
fn sem_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        outro => outro,
    }
}

/// Minúsculas, sem acento, só letras e dígitos separados por um espaço.
/// `"  Mercado  Délta, 123 "` vira `"mercado delta 123"`.
pub fn normalizar(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut separar = false;
    for c in texto.chars().flat_map(char::to_lowercase) {
        let c = sem_acento(c);
        if c.is_alphanumeric() {
            if separar && !saida.is_empty() {
                saida.push(' ');
            }
            separar = false;
            saida.push(c);
        } else {
            separar = true;
        }
    }
    saida
}

/// A chave de fornecedor das regras: normalizada e sem os sufixos societários.
pub fn normalizar_fornecedor(nome: &str) -> String {
    const SUFIXOS: [&str; 6] = ["ltda", "sa", "s a", "me", "epp", "eireli"];
    let mut n = normalizar(nome);
    for s in SUFIXOS {
        if let Some(resto) = n.strip_suffix(&format!(" {s}")) {
            n = resto.to_owned();
        }
    }
    n
}

/// Quantos caracteres do começo e do fim do texto vão ao modelo (documentado em
/// [`truncar_texto`]).
pub const TEXTO_INICIO: usize = 3000;
pub const TEXTO_FIM: usize = 1000;
/// O limite de um texto de achado: início mais fim.
pub const LIMITE_DO_TEXTO: usize = TEXTO_INICIO + TEXTO_FIM;

/// Junta linhas em branco e espaços repetidos (o texto de PDF vem cheio deles).
pub fn compactar_espacos(texto: &str) -> String {
    let mut linhas: Vec<String> = Vec::new();
    let mut anterior_vazia = false;
    for linha in texto.lines() {
        let l = linha.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            if !anterior_vazia && !linhas.is_empty() {
                linhas.push(String::new());
            }
            anterior_vazia = true;
        } else {
            linhas.push(l);
            anterior_vazia = false;
        }
    }
    while linhas.last().is_some_and(String::is_empty) {
        linhas.pop();
    }
    linhas.join("\n")
}

/// O texto que vai ao modelo no nível Completo. Se o texto compactado cabe em
/// [`LIMITE_DO_TEXTO`] (4000 caracteres), vai inteiro. Senão vão os primeiros
/// [`TEXTO_INICIO`] (cabeçalho, emissor, tipo do documento) e os últimos [`TEXTO_FIM`]
/// (totais, vencimento, linha digitável), com um marcador do que ficou de fora.
pub fn truncar_texto(texto: &str) -> String {
    let t = compactar_espacos(texto);
    let total = t.chars().count();
    if total <= LIMITE_DO_TEXTO {
        return t;
    }
    let inicio: String = t.chars().take(TEXTO_INICIO).collect();
    let fim: String = t.chars().skip(total - TEXTO_FIM).collect();
    format!(
        "{inicio}\n[... {} caracteres omitidos ...]\n{fim}",
        total - LIMITE_DO_TEXTO
    )
}

/// Corta em `max` caracteres (não bytes), com reticências quando cortou.
pub fn cortar(texto: &str, max: usize) -> String {
    if texto.chars().count() <= max {
        return texto.to_owned();
    }
    let mut c: String = texto.chars().take(max.saturating_sub(1)).collect();
    c.push('…');
    c
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_acento_caixa_e_pontuacao() {
        assert_eq!(normalizar("  Mercado  Délta, 123 "), "mercado delta 123");
        assert_eq!(normalizar("AÇAÍ & CIA."), "acai cia");
    }

    #[test]
    fn fornecedor_perde_sufixo_societario() {
        assert_eq!(
            normalizar_fornecedor("Energia Exemplo S.A."),
            "energia exemplo"
        );
        assert_eq!(normalizar_fornecedor("Padaria Beta LTDA"), "padaria beta");
        assert_eq!(normalizar_fornecedor("Mercado Delta"), "mercado delta");
    }

    #[test]
    fn texto_curto_vai_inteiro_e_compactado() {
        assert_eq!(truncar_texto("a  b\n\n\n\nc   d\n"), "a b\n\nc d");
    }

    #[test]
    fn texto_longo_mantem_inicio_e_fim() {
        let texto = format!(
            "{}{}{}",
            "I".repeat(3000),
            "#".repeat(5000),
            "F".repeat(1000)
        );
        let t = truncar_texto(&texto);
        assert!(t.starts_with(&"I".repeat(3000)));
        assert!(t.ends_with(&"F".repeat(1000)));
        assert!(t.contains("[... 5000 caracteres omitidos ...]"));
        assert!(!t.contains('#'));
    }

    #[test]
    fn corta_por_caracteres_nao_por_bytes() {
        assert_eq!(cortar("ação", 3), "aç…");
        assert_eq!(cortar("ação", 4), "ação");
    }
}
