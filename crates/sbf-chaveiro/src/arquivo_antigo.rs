//! Leitura do `segredos/cofre.enc` que o `workers/inventario/src/segredos.ts` gravava.
//!
//! Formato (JSON): `{"versao":1,"kdf":{"alg":"scrypt","N":…,"r":…,"p":…,"sal":"<b64>"},
//! "iv":"<b64>","tag":"<b64>","dados":"<b64>"}`. A chave são 32 bytes de scrypt da senha-mestra
//! (UTF-8) com o sal e os N, r e p do próprio arquivo; o conteúdo é AES-256-GCM com IV de 12
//! bytes, etiqueta de 16 e AAD `sbf-inventario-segredos-v1`. Decifrado, é o JSON
//! `{"cpf":"…","senhas":{"<rotulo>":"<senha>"}}`.
//!
//! Qualquer falha depois de ler os bytes (JSON do envelope, base64, parâmetros, etiqueta do
//! GCM, JSON decifrado) vira o mesmo [`ErroDoChaveiro::SenhaOuArquivoInvalido`].

use crate::erro::ErroDoChaveiro;
use aes_gcm::aead::{AeadInOut, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce, Tag};
use bancada_segredos::Segredo;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use zeroize::Zeroizing;

/// O dado associado do GCM, igual ao de `segredos.ts`.
const AAD: &[u8] = b"sbf-inventario-segredos-v1";
/// O teto de memória do scrypt, igual ao `maxmem` de `segredos.ts` (256 MiB): um arquivo que
/// peça mais é tratado como adulterado, e não trava a máquina.
const MEMORIA_MAXIMA_DO_SCRYPT: u64 = 256 * 1024 * 1024;
/// Paralelismo máximo aceito do scrypt (o `segredos.ts` grava 1).
const PARALELISMO_MAXIMO: u32 = 16;

/// O que o `cofre.enc` guardava. O `Debug` mostra só os rótulos.
#[derive(Debug)]
pub(crate) struct SegredosAntigos {
    pub(crate) cpf: Segredo,
    pub(crate) senhas: BTreeMap<String, Segredo>,
}

#[derive(Deserialize)]
struct Envelope {
    versao: u32,
    kdf: Kdf,
    iv: String,
    tag: String,
    dados: String,
}

#[derive(Deserialize)]
struct Kdf {
    alg: String,
    #[serde(rename = "N")]
    n: u64,
    r: u32,
    p: u32,
    sal: String,
}

/// Um texto do JSON decifrado: emprestado do buffer zerável quando não tem escape. Só um
/// texto com escape (`\"`, `\\`, `\uXXXX`) passa pelo buffer interno do `serde_json`, que
/// não é zerado (limite registrado no relatório da fatia).
struct Texto<'a>(Cow<'a, str>);

impl<'de: 'a, 'a> Deserialize<'de> for Texto<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitante<'a>(std::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for Visitante<'a> {
            type Value = Texto<'a>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("um texto")
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
                Ok(Texto(Cow::Borrowed(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Texto(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Texto(Cow::Owned(v)))
            }
        }
        deserializer.deserialize_str(Visitante(std::marker::PhantomData))
    }
}

impl Texto<'_> {
    /// Vira segredo: o `String` próprio é movido (e zerado ao descartar); o emprestado é
    /// copiado com a capacidade exata.
    fn em_segredo(self) -> Segredo {
        match self.0 {
            Cow::Borrowed(v) => Segredo::novo(v.to_owned()),
            Cow::Owned(v) => Segredo::novo(v),
        }
    }

    fn em_rotulo(self) -> String {
        self.0.into_owned()
    }
}

/// O mapa `senhas` do JSON, lido como pares para que nenhum `BTreeMap` intermediário segure
/// valores fora de um [`Segredo`].
fn pares<'de: 'a, 'a, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(Texto<'a>, Texto<'a>)>, D::Error> {
    struct Visitante<'a>(std::marker::PhantomData<&'a ()>);
    impl<'de: 'a, 'a> Visitor<'de> for Visitante<'a> {
        type Value = Vec<(Texto<'a>, Texto<'a>)>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("um objeto rótulo → senha")
        }
        fn visit_map<M: de::MapAccess<'de>>(self, mut mapa: M) -> Result<Self::Value, M::Error> {
            let mut saida = Vec::new();
            while let Some(par) = mapa.next_entry::<Texto<'a>, Texto<'a>>()? {
                saida.push(par);
            }
            Ok(saida)
        }
    }
    deserializer.deserialize_map(Visitante(std::marker::PhantomData))
}

#[derive(Deserialize)]
struct Claro<'a> {
    #[serde(borrow)]
    cpf: Texto<'a>,
    #[serde(borrow, default, deserialize_with = "pares")]
    senhas: Vec<(Texto<'a>, Texto<'a>)>,
}

fn invalido<T>(_: T) -> ErroDoChaveiro {
    ErroDoChaveiro::SenhaOuArquivoInvalido
}

/// `log2(n)` se `n` é potência de 2 maior que 1.
fn log2_exato(n: u64) -> Option<u8> {
    (n > 1 && n.is_power_of_two()).then(|| n.trailing_zeros() as u8)
}

fn parametros(kdf: &Kdf) -> Result<scrypt::Params, ErroDoChaveiro> {
    if kdf.alg != "scrypt" || kdf.p == 0 || kdf.p > PARALELISMO_MAXIMO || kdf.r == 0 {
        return Err(ErroDoChaveiro::SenhaOuArquivoInvalido);
    }
    let log_n = log2_exato(kdf.n).ok_or(ErroDoChaveiro::SenhaOuArquivoInvalido)?;
    let memoria = 128u64
        .checked_mul(kdf.n)
        .and_then(|m| m.checked_mul(u64::from(kdf.r)))
        .ok_or(ErroDoChaveiro::SenhaOuArquivoInvalido)?;
    if memoria > MEMORIA_MAXIMA_DO_SCRYPT {
        return Err(ErroDoChaveiro::SenhaOuArquivoInvalido);
    }
    scrypt::Params::new(log_n, kdf.r, kdf.p).map_err(invalido)
}

/// Decifra os bytes de um `cofre.enc` com a senha-mestra.
pub(crate) fn decifrar(
    bytes: &[u8],
    senha_mestra: &Segredo,
) -> Result<SegredosAntigos, ErroDoChaveiro> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(invalido)?;
    if envelope.versao != 1 {
        return Err(ErroDoChaveiro::SenhaOuArquivoInvalido);
    }
    let parametros = parametros(&envelope.kdf)?;
    let sal = STANDARD.decode(&envelope.kdf.sal).map_err(invalido)?;
    let iv = STANDARD.decode(&envelope.iv).map_err(invalido)?;
    let etiqueta = STANDARD.decode(&envelope.tag).map_err(invalido)?;
    let nonce = Nonce::<aes_gcm::aead::consts::U12>::try_from(iv.as_slice()).map_err(invalido)?;
    let etiqueta =
        Tag::<aes_gcm::aead::consts::U16>::try_from(etiqueta.as_slice()).map_err(invalido)?;
    // O conteúdo vira texto claro no próprio buffer, que é zerado ao descartar.
    let mut conteudo = Zeroizing::new(STANDARD.decode(&envelope.dados).map_err(invalido)?);

    let mut chave = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(
        senha_mestra.expor().as_bytes(),
        &sal,
        &parametros,
        chave.as_mut_slice(),
    )
    .map_err(invalido)?;
    let cifra = Aes256Gcm::new_from_slice(chave.as_slice()).map_err(invalido)?;
    cifra
        .decrypt_inout_detached(&nonce, AAD, conteudo.as_mut_slice().into(), &etiqueta)
        .map_err(invalido)?;
    drop(cifra);

    let claro: Claro<'_> = serde_json::from_slice(&conteudo).map_err(invalido)?;
    let mut senhas = BTreeMap::new();
    for (rotulo, senha) in claro.senhas {
        senhas.insert(rotulo.em_rotulo(), senha.em_segredo());
    }
    Ok(SegredosAntigos {
        cpf: claro.cpf.em_segredo(),
        senhas,
    })
}

/// Lê e decifra o `cofre.enc` do caminho.
pub(crate) fn ler(
    caminho: &Path,
    senha_mestra: &Segredo,
) -> Result<SegredosAntigos, ErroDoChaveiro> {
    let bytes = std::fs::read(caminho).map_err(|e| ErroDoChaveiro::disco(&e))?;
    decifrar(&bytes, senha_mestra)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn log2_so_de_potencia_de_dois() {
        assert_eq!(log2_exato(1024), Some(10));
        assert_eq!(log2_exato(1 << 17), Some(17));
        assert_eq!(log2_exato(1000), None);
        assert_eq!(log2_exato(1), None);
        assert_eq!(log2_exato(0), None);
    }

    #[test]
    fn parametros_absurdos_sao_arquivo_invalido() {
        let base = |n, r, p| Kdf {
            alg: "scrypt".into(),
            n,
            r,
            p,
            sal: String::new(),
        };
        assert!(parametros(&base(1 << 17, 8, 1)).is_ok());
        for kdf in [
            base(1 << 20, 8, 1), // 1 GiB: passa do teto
            base(1000, 8, 1),
            base(1024, 0, 1),
            base(1024, 8, 0),
            base(1024, 8, 99),
        ] {
            assert_eq!(
                parametros(&kdf).err(),
                Some(ErroDoChaveiro::SenhaOuArquivoInvalido)
            );
        }
        let mut outro = base(1024, 8, 1);
        outro.alg = "pbkdf2".into();
        assert!(parametros(&outro).is_err());
    }
}
