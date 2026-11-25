//! O carimbo de versão do binário: qual commit do SBF e da bancada (dependência por caminho
//! `../bancada`) foram compilados, e se a árvore tinha alterações não commitadas. Os valores
//! vêm do `build.rs`; aqui só se monta o texto.

/// O que o `build.rs` gravou no binário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carimbo {
    pub versao: String,
    pub sbf_rev: String,
    pub sbf_sujo: bool,
    pub bancada_rev: String,
    pub bancada_suja: bool,
    pub compilado_em: String,
}

impl Carimbo {
    /// O carimbo deste binário.
    pub fn do_build() -> Self {
        Self {
            versao: env!("CARGO_PKG_VERSION").to_owned(),
            sbf_rev: env!("SBF_REV").to_owned(),
            sbf_sujo: env!("SBF_SUJO") == "1",
            bancada_rev: env!("BANCADA_REV").to_owned(),
            bancada_suja: env!("BANCADA_SUJA") == "1",
            compilado_em: env!("SBF_COMPILADO_EM").to_owned(),
        }
    }

    /// A linha única: `sbf 0.1.0 · rev <sbf>[+alterações] · bancada <rev>[+alterações] ·
    /// compilado em <data>`.
    pub fn linha(&self) -> String {
        let alt = |suja: bool| if suja { "+alterações" } else { "" };
        format!(
            "sbf {} · rev {}{} · bancada {}{} · compilado em {}",
            self.versao,
            self.sbf_rev,
            alt(self.sbf_sujo),
            self.bancada_rev,
            alt(self.bancada_suja),
            self.compilado_em
        )
    }

    /// O aviso de build com alterações não commitadas (`None` se as duas árvores estavam limpas).
    pub fn aviso(&self) -> Option<String> {
        let onde = match (self.sbf_sujo, self.bancada_suja) {
            (false, false) => return None,
            (true, false) => "do SBF",
            (false, true) => "da bancada",
            (true, true) => "do SBF e da bancada",
        };
        Some(format!(
            "Este binário foi compilado com alterações não commitadas {onde}: o commit mostrado não descreve o que está rodando."
        ))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn limpo() -> Carimbo {
        Carimbo {
            versao: "0.1.0".into(),
            sbf_rev: "abcdef123456".into(),
            sbf_sujo: false,
            bancada_rev: "123456abcdef".into(),
            bancada_suja: false,
            compilado_em: "2026-11-24".into(),
        }
    }

    #[test]
    fn arvore_limpa() {
        let c = limpo();
        assert_eq!(
            c.linha(),
            "sbf 0.1.0 · rev abcdef123456 · bancada 123456abcdef · compilado em 2026-11-24"
        );
        assert_eq!(c.aviso(), None);
    }

    #[test]
    fn arvore_suja() {
        let c = Carimbo {
            sbf_sujo: true,
            bancada_suja: true,
            ..limpo()
        };
        assert_eq!(
            c.linha(),
            "sbf 0.1.0 · rev abcdef123456+alterações · bancada 123456abcdef+alterações · compilado em 2026-11-24"
        );
        assert!(c.aviso().is_some_and(|a| a.contains("do SBF e da bancada")));
        let so_bancada = Carimbo {
            bancada_suja: true,
            ..limpo()
        };
        assert!(so_bancada.aviso().is_some_and(|a| a.contains("da bancada")));
        assert!(!so_bancada.linha().contains("rev abcdef123456+"));
    }

    #[test]
    fn desconhecida() {
        let c = Carimbo {
            sbf_rev: "desconhecida".into(),
            bancada_rev: "desconhecida".into(),
            ..limpo()
        };
        assert_eq!(
            c.linha(),
            "sbf 0.1.0 · rev desconhecida · bancada desconhecida · compilado em 2026-11-24"
        );
        assert_eq!(c.aviso(), None);
    }
}
