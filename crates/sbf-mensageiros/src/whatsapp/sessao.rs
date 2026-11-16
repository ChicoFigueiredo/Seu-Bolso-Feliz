//! A sessão do WhatsApp guardada **cifrada**. A ponte entrega a sessão como um bloco opaco
//! (evento `sessao`); aqui ela é cifrada com AES-256-GCM e gravada num arquivo da pasta de
//! dados do app (nunca no cofre). A chave de 256 bits é sorteada uma vez e fica **só no
//! chaveiro** (rótulo `mensageiro:whatsapp`), não ao lado do arquivo. Perder o chaveiro
//! equivale a desparear: o usuário pareia de novo pelo QR.
//!
//! Formato do arquivo: `SBFWA1` + nonce (12 bytes) + texto cifrado com a etiqueta.

use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use sbf_chaveiro::{Chaveiro, Segredo};
use zeroize::Zeroizing;

use crate::config::escrever_atomico;
use crate::{Erro, Resultado};

const CABECALHO: &[u8] = b"SBFWA1";
const AAD: &[u8] = b"sbf-mensageiros/whatsapp/v1";
const NOME_NO_CHAVEIRO: &str = "whatsapp";
/// O nome do arquivo na pasta de dados.
pub const ARQUIVO: &str = "whatsapp.sessao";

/// A sessão cifrada de um WhatsApp.
#[derive(Debug, Clone)]
pub struct SessaoCifrada {
    arquivo: PathBuf,
    chaveiro: Chaveiro,
}

fn chaveiro_erro(e: impl std::fmt::Display) -> Erro {
    Erro::Chaveiro(e.to_string())
}

fn de_hex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut saida = [0_u8; 32];
    for (i, par) in s.as_bytes().chunks(2).enumerate() {
        saida[i] = u8::from_str_radix(std::str::from_utf8(par).ok()?, 16).ok()?;
    }
    Some(saida)
}

impl SessaoCifrada {
    /// A sessão guardada em `pasta_de_dados/whatsapp.sessao`, com a chave no `chaveiro`.
    #[must_use]
    pub fn nova(pasta_de_dados: &Path, chaveiro: Chaveiro) -> Self {
        Self {
            arquivo: pasta_de_dados.join(ARQUIVO),
            chaveiro,
        }
    }

    /// Há sessão guardada (o WhatsApp está pareado)?
    #[must_use]
    pub fn existe(&self) -> bool {
        self.arquivo.is_file()
    }

    fn chave(&self, criar: bool) -> Resultado<Zeroizing<[u8; 32]>> {
        if self.chaveiro.tem_segredo_de_mensageiro(NOME_NO_CHAVEIRO) {
            let s = self
                .chaveiro
                .segredo_de_mensageiro(NOME_NO_CHAVEIRO)
                .map_err(chaveiro_erro)?;
            return de_hex(s.expor())
                .map(Zeroizing::new)
                .ok_or(Erro::SessaoInvalida);
        }
        if !criar {
            return Err(Erro::SessaoInvalida);
        }
        let mut bruta = Zeroizing::new([0_u8; 32]);
        getrandom::fill(bruta.as_mut_slice()).map_err(|e| Erro::Chaveiro(e.to_string()))?;
        let hex: String = bruta.iter().map(|b| format!("{b:02x}")).collect();
        self.chaveiro
            .guardar_segredo_de_mensageiro(NOME_NO_CHAVEIRO, Segredo::novo(hex))
            .map_err(chaveiro_erro)?;
        Ok(bruta)
    }

    /// Cifra e grava `bloco` (substitui a sessão anterior).
    ///
    /// # Errors
    /// Chaveiro indisponível ou falha de disco.
    pub fn salvar(&self, bloco: &[u8]) -> Resultado<()> {
        let chave = self.chave(true)?;
        let cifra =
            Aes256Gcm::new_from_slice(chave.as_slice()).map_err(|_| Erro::SessaoInvalida)?;
        let mut nonce_bruto = [0_u8; 12];
        getrandom::fill(&mut nonce_bruto).map_err(|e| Erro::Chaveiro(e.to_string()))?;
        let nonce = Nonce::<aes_gcm::aead::consts::U12>::try_from(nonce_bruto.as_slice())
            .map_err(|_| Erro::SessaoInvalida)?;
        let cifrado = cifra
            .encrypt(
                &nonce,
                Payload {
                    msg: bloco,
                    aad: AAD,
                },
            )
            .map_err(|_| Erro::SessaoInvalida)?;
        let mut saida = Vec::with_capacity(CABECALHO.len() + 12 + cifrado.len());
        saida.extend_from_slice(CABECALHO);
        saida.extend_from_slice(&nonce_bruto);
        saida.extend_from_slice(&cifrado);
        escrever_atomico(&self.arquivo, &saida)
    }

    /// Abre a sessão guardada, em claro e só em memória. `None` se nunca pareou.
    ///
    /// # Errors
    /// [`Erro::SessaoInvalida`] se o arquivo foi adulterado ou a chave não confere.
    pub fn abrir(&self) -> Resultado<Option<Zeroizing<Vec<u8>>>> {
        let bytes = match std::fs::read(&self.arquivo) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let resto = bytes.strip_prefix(CABECALHO).ok_or(Erro::SessaoInvalida)?;
        if resto.len() < 12 + 16 {
            return Err(Erro::SessaoInvalida);
        }
        let (nonce_bruto, cifrado) = resto.split_at(12);
        let nonce = Nonce::<aes_gcm::aead::consts::U12>::try_from(nonce_bruto)
            .map_err(|_| Erro::SessaoInvalida)?;
        let chave = self.chave(false)?;
        let cifra =
            Aes256Gcm::new_from_slice(chave.as_slice()).map_err(|_| Erro::SessaoInvalida)?;
        let claro = cifra
            .decrypt(
                &nonce,
                Payload {
                    msg: cifrado,
                    aad: AAD,
                },
            )
            .map_err(|_| Erro::SessaoInvalida)?;
        Ok(Some(Zeroizing::new(claro)))
    }

    /// Apaga a sessão e a chave (despareamento local).
    ///
    /// # Errors
    /// Falha de disco ou do chaveiro.
    pub fn apagar(&self) -> Resultado<()> {
        match std::fs::remove_file(&self.arquivo) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.chaveiro
            .apagar_segredo_de_mensageiro(NOME_NO_CHAVEIRO)
            .map_err(chaveiro_erro)
    }
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;
    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;
    use std::sync::Arc;

    fn chaveiro(pasta: &Path) -> Chaveiro {
        Chaveiro::abrir_com_chaveiro(
            pasta,
            Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap()
    }

    #[test]
    fn sessao_vai_cifrada_e_volta_igual() {
        let d = tempfile::tempdir().unwrap();
        let s = SessaoCifrada::nova(d.path(), chaveiro(&d.path().join("chaveiro")));
        assert!(!s.existe() && s.abrir().unwrap().is_none());
        let bloco = b"sessao-ficticia: chaves e tokens do aparelho vinculado";
        s.salvar(bloco).unwrap();
        assert!(s.existe());
        let em_disco = std::fs::read(d.path().join(ARQUIVO)).unwrap();
        assert!(
            !em_disco.windows(8).any(|w| w == b"sessao-f"),
            "nada em claro no disco"
        );
        assert_eq!(s.abrir().unwrap().unwrap().as_slice(), bloco);
        // trocar a sessão substitui, com nonce novo
        s.salvar(b"outra").unwrap();
        assert_ne!(std::fs::read(d.path().join(ARQUIVO)).unwrap(), em_disco);
        assert_eq!(s.abrir().unwrap().unwrap().as_slice(), b"outra");
    }

    #[test]
    fn arquivo_adulterado_ou_chave_perdida_nao_abre() {
        let d = tempfile::tempdir().unwrap();
        let c = chaveiro(&d.path().join("chaveiro"));
        let s = SessaoCifrada::nova(d.path(), c.clone());
        s.salvar(b"abc").unwrap();
        let caminho = d.path().join(ARQUIVO);
        let mut b = std::fs::read(&caminho).unwrap();
        let n = b.len();
        b[n - 1] ^= 1;
        std::fs::write(&caminho, &b).unwrap();
        assert!(matches!(s.abrir(), Err(Erro::SessaoInvalida)));

        s.salvar(b"abc").unwrap();
        c.apagar_segredo_de_mensageiro("whatsapp").unwrap();
        assert!(matches!(s.abrir(), Err(Erro::SessaoInvalida)));
    }

    #[test]
    fn apagar_remove_arquivo_e_chave() {
        let d = tempfile::tempdir().unwrap();
        let c = chaveiro(&d.path().join("chaveiro"));
        let s = SessaoCifrada::nova(d.path(), c.clone());
        s.salvar(b"abc").unwrap();
        assert!(c.tem_segredo_de_mensageiro("whatsapp"));
        s.apagar().unwrap();
        assert!(!s.existe() && !c.tem_segredo_de_mensageiro("whatsapp"));
        s.apagar().unwrap(); // idempotente
    }
}
