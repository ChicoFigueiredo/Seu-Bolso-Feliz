//! A pasta `SBF/` da nuvem é do próprio Seu Bolso Feliz (estante e cópia de segurança,
//! spec 06). A coleta nunca a lê de volta como se fosse material novo. Porte de `pasta-sbf.ts`.

/// Nome da pasta do app na raiz da nuvem.
pub const PASTA_DO_SBF: &str = "SBF";

/// `true` se o caminho (relativo à raiz do remoto) está dentro de `SBF/` ou é a própria pasta.
#[must_use]
pub fn dentro_da_pasta_do_sbf(caminho: &str) -> bool {
    let normal = caminho.replace('\\', "/");
    let normal = normal.trim_start_matches('/');
    normal == PASTA_DO_SBF || normal.starts_with("SBF/")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_a_pasta_sbf_exata_da_raiz() {
        assert!(dentro_da_pasta_do_sbf("SBF"));
        assert!(dentro_da_pasta_do_sbf("SBF/Estante/x.pdf"));
        assert!(dentro_da_pasta_do_sbf("/SBF/a"));
        assert!(dentro_da_pasta_do_sbf("SBF\\Estante"));
        assert!(!dentro_da_pasta_do_sbf("SBFx/a"));
        assert!(!dentro_da_pasta_do_sbf("sbf/a"));
        assert!(!dentro_da_pasta_do_sbf("Docs/SBF/a"));
    }
}
