//! A credencial da estação (grill Q3): token opaco com 256 bits aleatórios. O servidor
//! guarda só o SHA-256 do segredo.
//!
//! Formato: `sbfe_<id da estação em 32 hex>_<segredo: 32 bytes em base64url sem
//! preenchimento>`. O id deixa o servidor achar a estação pela chave primária e
//! responder "estação revogada" depois de apagar o hash; o segredo é o que autentica.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

const PREFIXO: &str = "sbfe_";

/// Uma credencial recém-emitida ou recebida num pedido. Nunca aparece em `Debug`.
pub struct Credencial {
    pub estacao: Uuid,
    segredo: Zeroizing<[u8; 32]>,
}

impl fmt::Debug for Credencial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credencial")
            .field("estacao", &self.estacao)
            .field("segredo", &"<oculto>")
            .finish()
    }
}

/// Falha ao sortear os bytes do segredo.
#[derive(Debug, thiserror::Error)]
#[error("não deu para sortear a credencial: {0}")]
pub struct ErroDeSorteio(String);

impl Credencial {
    /// Sorteia uma credencial nova para a estação `estacao`.
    pub fn nova(estacao: Uuid) -> Result<Self, ErroDeSorteio> {
        let mut segredo = Zeroizing::new([0u8; 32]);
        getrandom::fill(segredo.as_mut()).map_err(|e| ErroDeSorteio(e.to_string()))?;
        Ok(Self { estacao, segredo })
    }

    /// O texto que a estação guarda no chaveiro e manda em `Authorization: Bearer`.
    pub fn texto(&self) -> Zeroizing<String> {
        let mut s = Zeroizing::new(String::with_capacity(5 + 32 + 1 + 43));
        s.push_str(PREFIXO);
        s.push_str(&self.estacao.simple().to_string());
        s.push('_');
        URL_SAFE_NO_PAD.encode_string(self.segredo.as_ref(), &mut s);
        s
    }

    /// Lê o texto de uma credencial. `None` para qualquer formato diferente.
    pub fn ler(texto: &str) -> Option<Self> {
        let resto = texto.strip_prefix(PREFIXO)?;
        let (id, segredo) = resto.split_once('_')?;
        if id.len() != 32 {
            return None;
        }
        let estacao = Uuid::try_parse(id).ok()?;
        let bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(segredo).ok()?);
        let segredo: [u8; 32] = bytes.as_slice().try_into().ok()?;
        Some(Self {
            estacao,
            segredo: Zeroizing::new(segredo),
        })
    }

    /// O SHA-256 do segredo: o que o banco guarda.
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.segredo.as_ref()).into()
    }

    /// Confere o segredo contra o hash guardado, em tempo constante.
    pub fn confere(&self, guardado: &[u8]) -> bool {
        let calculado = self.hash();
        if guardado.len() != calculado.len() {
            return false;
        }
        calculado
            .iter()
            .zip(guardado)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

/// Tira o token do cabeçalho `Authorization: Bearer <token>`.
pub fn do_cabecalho(valor: &str) -> Option<&str> {
    let (esquema, token) = valor.trim().split_once(' ')?;
    if !esquema.eq_ignore_ascii_case(sbf_contrato::ESQUEMA_DE_AUTORIZACAO) {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_pelo_texto() {
        let id = Uuid::new_v4();
        let c = Credencial::nova(id).unwrap();
        let texto = c.texto();
        assert!(texto.starts_with("sbfe_"));
        assert_eq!(texto.len(), 5 + 32 + 1 + 43);
        let lida = Credencial::ler(&texto).unwrap();
        assert_eq!(lida.estacao, id);
        assert!(lida.confere(&c.hash()));
    }

    #[test]
    fn duas_credenciais_nunca_sao_iguais() {
        let id = Uuid::new_v4();
        let a = Credencial::nova(id).unwrap();
        let b = Credencial::nova(id).unwrap();
        assert_ne!(a.hash(), b.hash());
        assert!(!b.confere(&a.hash()));
    }

    #[test]
    fn formato_errado_nao_e_credencial() {
        let id = Uuid::new_v4();
        let boa = Credencial::nova(id).unwrap().texto();
        for ruim in [
            "",
            "sbfe_",
            "Bearer x",
            &boa[1..],
            &boa[..boa.len() - 1],
            &format!("{}A", *boa),
            &boa.replace("sbfe_", "sbfx_"),
        ] {
            assert!(Credencial::ler(ruim).is_none(), "{ruim:?}");
        }
    }

    #[test]
    fn hash_de_outro_tamanho_nao_confere() {
        let c = Credencial::nova(Uuid::new_v4()).unwrap();
        assert!(!c.confere(&[]));
        assert!(!c.confere(&c.hash()[..31]));
    }

    #[test]
    fn debug_nao_mostra_o_segredo() {
        let c = Credencial::nova(Uuid::new_v4()).unwrap();
        let texto = c.texto();
        let segredo = texto.rsplit('_').next().unwrap();
        assert!(!format!("{c:?}").contains(segredo));
    }

    #[test]
    fn cabecalho_bearer() {
        assert_eq!(do_cabecalho("Bearer abc"), Some("abc"));
        assert_eq!(do_cabecalho("bearer  abc "), Some("abc"));
        assert_eq!(do_cabecalho("Basic abc"), None);
        assert_eq!(do_cabecalho("Bearer "), None);
        assert_eq!(do_cabecalho("abc"), None);
    }
}
