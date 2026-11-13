//! Conferência do dígito verificador do CPF, igual à de `workers/inventario/src/cpf.ts`. Só
//! diz se é válido; nunca devolve nem formata o número.

/// Os 11 dígitos conferem com os dois dígitos verificadores, e não são todos iguais.
/// Como no `cpf.ts`, o que não é dígito é ignorado antes.
pub(crate) fn cpf_valido(cpf: &str) -> bool {
    // Capacidade reservada antes: nenhuma realocação deixa cópia dos dígitos para trás.
    let mut bytes = zeroize::Zeroizing::new(Vec::with_capacity(cpf.len()));
    bytes.extend(cpf.bytes().filter(u8::is_ascii_digit));
    if bytes.len() != 11 {
        return false;
    }
    let digito = |i: usize| u32::from(bytes[i] - b'0');
    if (1..11).all(|i| bytes[i] == bytes[0]) {
        return false;
    }
    let verificador = |ate: usize| {
        let soma: u32 = (0..ate)
            .map(|i| digito(i) * (ate as u32 + 1 - i as u32))
            .sum();
        let resto = (soma * 10) % 11;
        if resto == 10 { 0 } else { resto }
    };
    verificador(9) == digito(9) && verificador(10) == digito(10)
}

#[cfg(test)]
mod testes {
    use super::cpf_valido;

    #[test]
    fn os_mesmos_casos_do_cpf_test_ts() {
        assert!(cpf_valido("52998224725"));
        assert!(cpf_valido("11144477735"));
        assert!(!cpf_valido("52998224724"));
        assert!(!cpf_valido("5299822472"));
        assert!(!cpf_valido("11111111111"));
        assert!(cpf_valido("529.982.247-25"));
    }
}
