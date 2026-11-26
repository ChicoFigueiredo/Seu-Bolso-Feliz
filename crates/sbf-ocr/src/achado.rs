//! O OCR de um achado do inventário: reaproveita o texto que já existe, senão lê a imagem e
//! grava em `texto`.

use std::path::Path;

use rusqlite::Connection;
use sbf_inventario::texto;
use sbf_pdf::texto::{LIMITE_DO_TEXTO, cortar_como_o_js, impressao_canonica};

use crate::leitura::{FalhaDoOcr, Ocr};
use crate::{METODO_OCR, METODO_OCR_SEM_TEXTO};

/// O que o OCR de um achado deu.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DesfechoDoOcr {
    /// O inventário já tinha texto deste conteúdo (de qualquer método); nada foi lido.
    /// Vem o texto gravado.
    JaHavia(String),
    /// O tesseract leu e o texto foi gravado com o método [`METODO_OCR`]. Vem o texto
    /// inteiro lido (o gravado é cortado em 20 KB, como o do PDF).
    Lido(String),
    /// O tesseract rodou e não achou texto: gravado vazio com [`METODO_OCR_SEM_TEXTO`], para
    /// não refazer.
    SemTexto,
    /// Não há tesseract. Nada foi gravado: tenta de novo quando ele for instalado.
    Indisponivel,
    /// A imagem não pôde ser lida (grande demais, ilegível, prazo, erro do tesseract).
    /// Nada foi gravado.
    Falhou(FalhaDoOcr),
}

/// Lê o texto da imagem do achado `sha` e o grava em `texto` (a primeira extração vale).
///
/// # Errors
/// Falha do SQLite. Falha do OCR não é erro: vem em [`DesfechoDoOcr`].
pub fn ocr_do_achado(
    conexao: &Connection,
    sha: &str,
    imagem: &Path,
    ocr: &Ocr,
) -> rusqlite::Result<DesfechoDoOcr> {
    if let Some(t) = texto::ler_texto(conexao, sha)? {
        return Ok(DesfechoDoOcr::JaHavia(t.conteudo));
    }
    match ocr.ler(imagem) {
        Ok(lido) if lido.trim().is_empty() => {
            texto::registrar_texto(conexao, sha, METODO_OCR_SEM_TEXTO, "", None)?;
            Ok(DesfechoDoOcr::SemTexto)
        }
        Ok(lido) => {
            texto::registrar_texto(
                conexao,
                sha,
                METODO_OCR,
                &cortar_como_o_js(&lido, LIMITE_DO_TEXTO),
                Some(&impressao_canonica(&lido)),
            )?;
            Ok(DesfechoDoOcr::Lido(lido))
        }
        Err(FalhaDoOcr::Indisponivel) => Ok(DesfechoDoOcr::Indisponivel),
        Err(f) => Ok(DesfechoDoOcr::Falhou(f)),
    }
}
