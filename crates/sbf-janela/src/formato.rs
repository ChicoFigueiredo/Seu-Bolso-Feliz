//! Formatos de exibição: dinheiro em reais, datas, ciclos e o texto puro de uma célula.

use sbf_inventario::consulta::{Celula, Centavos, Fonte};

/// A marca de todo número calculado a partir dos registros do inventário (`CONTEXT.md`).
pub const A_CONFERIR: &str = "a conferir";

/// `123456` → `R$ 1.234,56`; negativo → `-R$ 1.234,56`.
pub fn dinheiro(centavos: Centavos) -> String {
    let negativo = centavos < 0;
    let abs = centavos.unsigned_abs();
    let reais = abs / 100;
    let cent = abs % 100;
    let digitos = reais.to_string();
    let mut com_pontos = String::new();
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i).is_multiple_of(3) {
            com_pontos.push('.');
        }
        com_pontos.push(c);
    }
    format!(
        "{}R$ {com_pontos},{cent:02}",
        if negativo { "-" } else { "" }
    )
}

/// O valor com a marca da fonte: `R$ 312,44 · a conferir` no inventário.
pub fn dinheiro_com_fonte(centavos: Centavos, fonte: Fonte) -> String {
    match fonte {
        Fonte::Inventario => format!("{} · {A_CONFERIR}", dinheiro(centavos)),
        Fonte::LivroCaixa => dinheiro(centavos),
    }
}

/// `1234567` → `1.234.567`.
pub fn inteiro(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut r = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            r.push('.');
        }
        r.push(c);
    }
    if n < 0 { format!("-{r}") } else { r }
}

/// `2026-10-28` → `28/10/2026`; `2026-10-02T21:41:00` → `02/10/2026 21:41`.
/// Texto em outro formato volta como veio.
pub fn data(texto: &str) -> String {
    let (dia, hora) = match texto.split_once(['T', ' ']) {
        Some((d, h)) => (d, Some(h)),
        None => (texto, None),
    };
    let partes: Vec<&str> = dia.split('-').collect();
    if partes.len() != 3 || partes.iter().any(|p| p.parse::<u32>().is_err()) {
        return texto.to_owned();
    }
    let base = format!("{}/{}/{}", partes[2], partes[1], partes[0]);
    match hora {
        Some(h) if h.len() >= 5 => format!("{base} {}", &h[..5]),
        _ => base,
    }
}

/// O texto puro de uma célula (pesquisa, Copiar rastro, tooltips).
pub fn texto_da_celula(c: &Celula) -> String {
    match c {
        Celula::Vazia => String::new(),
        Celula::Texto(t) => t.clone(),
        Celula::Link { texto, .. } => texto.clone(),
        Celula::Data(d) => data(d),
        Celula::Valor {
            centavos, fonte, ..
        } => dinheiro_com_fonte(*centavos, *fonte),
        Celula::Contagem { n, .. } => inteiro(*n),
        Celula::Estado { rotulo, quem } => match quem {
            Some(q) => format!("{rotulo} · por {}", q.rotulo()),
            None => rotulo.clone(),
        },
        Celula::Links(l) => l
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Chave de ordenação de uma célula: números como número, o resto como texto.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Ordem {
    Vazia,
    Numero(i64),
    Texto(String),
}

pub fn ordem_da_celula(c: &Celula) -> Ordem {
    match c {
        Celula::Vazia => Ordem::Vazia,
        Celula::Valor { centavos, .. } => Ordem::Numero(*centavos),
        Celula::Contagem { n, .. } => Ordem::Numero(*n),
        // `AAAA-MM-DD` ordena certo como texto.
        Celula::Data(d) => Ordem::Texto(d.clone()),
        outra => Ordem::Texto(texto_da_celula(outra).to_lowercase()),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn dinheiro_em_reais() {
        assert_eq!(dinheiro(31244), "R$ 312,44");
        assert_eq!(dinheiro(-31244), "-R$ 312,44");
        assert_eq!(dinheiro(123456789), "R$ 1.234.567,89");
        assert_eq!(dinheiro(5), "R$ 0,05");
        assert_eq!(
            dinheiro_com_fonte(-100, Fonte::Inventario),
            "-R$ 1,00 · a conferir"
        );
    }

    #[test]
    fn datas_no_formato_brasileiro() {
        assert_eq!(data("2026-10-28"), "28/10/2026");
        assert_eq!(data("2026-10-02T21:41:00-03:00"), "02/10/2026 21:41");
        assert_eq!(data("2026-10"), "2026-10");
    }

    #[test]
    fn inteiros_com_ponto() {
        assert_eq!(inteiro(181217), "181.217");
        assert_eq!(inteiro(87), "87");
    }
}
