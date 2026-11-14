//! Fornecedor por alias exato (`resolveSupplierForTransaction` de `@sbf/domain`): casa o nome
//! bruto do comerciante com um alias ativo, por igualdade depois de [`normalizar_nome`]. Sem
//! casamento, o draft nasce sem fornecedor e a revisão humana resolve.

use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

/// `normalizeName`: minúsculas, sem acento, tudo que não é `[a-z0-9]` vira `_`, repetidos
/// colapsam e as pontas são aparadas.
pub fn normalizar_nome(nome: &str) -> String {
    let mut saida = String::with_capacity(nome.len());
    for c in nome
        .to_lowercase()
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
    {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '_'
        };
        if c == '_' && saida.ends_with('_') {
            continue;
        }
        saida.push(c);
    }
    saida.trim_matches('_').to_owned()
}

#[derive(Debug, Clone)]
pub struct Apelido {
    pub fornecedor: Uuid,
    pub nome: String,
}

/// O fornecedor do primeiro apelido ativo que casa; `None` é `needs_review`.
pub fn resolver_fornecedor(nome_bruto: &str, apelidos: &[Apelido]) -> Option<Uuid> {
    let procurado = normalizar_nome(nome_bruto);
    if procurado.is_empty() {
        return None;
    }
    apelidos
        .iter()
        .find(|a| normalizar_nome(&a.nome) == procurado)
        .map(|a| a.fornecedor)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_como_o_domain() {
        assert_eq!(
            normalizar_nome("Padaria São João LTDA."),
            "padaria_sao_joao_ltda"
        );
        assert_eq!(
            normalizar_nome("  --PIX*Mercado  Bom--  "),
            "pix_mercado_bom"
        );
        assert_eq!(normalizar_nome("Çà é ñ"), "ca_e_n");
        assert_eq!(normalizar_nome("日本"), "");
        assert_eq!(normalizar_nome(""), "");
    }

    #[test]
    fn resolve_por_igualdade_exata_do_nome_normalizado() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let apelidos = vec![
            Apelido {
                fornecedor: a,
                nome: "Padaria São João".into(),
            },
            Apelido {
                fornecedor: b,
                nome: "Mercado Bom".into(),
            },
        ];
        assert_eq!(resolver_fornecedor("PADARIA SAO JOAO", &apelidos), Some(a));
        assert_eq!(resolver_fornecedor("mercado-bom", &apelidos), Some(b));
        assert_eq!(resolver_fornecedor("Mercado Bom Demais", &apelidos), None);
        assert_eq!(resolver_fornecedor("", &apelidos), None);
        assert_eq!(resolver_fornecedor("***", &apelidos), None);
    }
}
