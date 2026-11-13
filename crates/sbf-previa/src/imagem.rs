//! Imagens: png, jpg, webp, gif, bmp e tiff pelo crate `image`, reduzidas para a tela; HEIC
//! (e AVIF) pelo `ffmpeg` do sistema, entrada e saída por pipe.

use crate::{ErroDaPrevia, Imagem, LADO_MAXIMO, MEGAPIXELS_MAXIMOS};
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbaImage};
use sbf_processo::{Chamada, localizar_ou_nome};
use std::io::Cursor;
use std::time::Duration;
use zeroize::Zeroizing;

/// O máximo que o decodificador pode alocar (64 MP em RGBA de 16 bits = 512 MiB).
const ALOCACAO_MAXIMA: u64 = 1024 * 1024 * 1024;

fn ilegivel(erro: impl std::fmt::Display) -> ErroDaPrevia {
    ErroDaPrevia::Ilegivel(erro.to_string())
}

fn leitor(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, ErroDaPrevia> {
    let mut leitor = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ilegivel)?;
    let mut limites = Limits::default();
    limites.max_alloc = Some(ALOCACAO_MAXIMA);
    leitor.limits(limites);
    Ok(leitor)
}

/// Decodifica (o formato pelo conteúdo, não pela extensão), recusa acima de
/// [`MEGAPIXELS_MAXIMOS`] antes de decodificar, gira pela orientação EXIF e reduz para
/// [`LADO_MAXIMO`] no lado maior.
pub(crate) fn decodificar(bytes: &[u8]) -> Result<Imagem, ErroDaPrevia> {
    let (largura, altura) = leitor(bytes)?.into_dimensions().map_err(ilegivel)?;
    if u64::from(largura) * u64::from(altura) > MEGAPIXELS_MAXIMOS {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    let mut decodificador = leitor(bytes)?.into_decoder().map_err(ilegivel)?;
    let orientacao = decodificador
        .orientation()
        .unwrap_or(Orientation::NoTransforms);
    let mut imagem = DynamicImage::from_decoder(decodificador).map_err(ilegivel)?;
    imagem.apply_orientation(orientacao);
    Ok(reduzir(imagem, LADO_MAXIMO))
}

/// Reduz para caber em `lado` x `lado` (nunca aumenta) e converte em RGBA8.
fn reduzir(imagem: DynamicImage, lado: u32) -> Imagem {
    let imagem = if imagem.width().max(imagem.height()) > lado {
        imagem.resize(lado, lado, FilterType::Triangle)
    } else {
        imagem
    };
    rgba(imagem.into_rgba8())
}

fn rgba(imagem: RgbaImage) -> Imagem {
    Imagem {
        largura: imagem.width(),
        altura: imagem.height(),
        rgba: imagem.into_raw(),
    }
}

/// A saída do `pdftoppm` (PPM) em RGBA.
pub(crate) fn de_ppm(bytes: &[u8]) -> Result<Imagem, ErroDaPrevia> {
    let imagem = image::load_from_memory_with_format(bytes, ImageFormat::Pnm).map_err(ilegivel)?;
    Ok(rgba(imagem.into_rgba8()))
}

/// A imagem com `largura` px de largura, na proporção (aumenta ou reduz).
pub(crate) fn na_largura(imagem: &Imagem, largura: u32) -> Result<Imagem, ErroDaPrevia> {
    if imagem.largura == largura {
        return Ok(imagem.clone());
    }
    let fonte = RgbaImage::from_raw(imagem.largura, imagem.altura, imagem.rgba.clone())
        .ok_or_else(|| ilegivel("imagem com tamanho inconsistente"))?;
    let altura = (u64::from(imagem.altura) * u64::from(largura))
        .div_ceil(u64::from(imagem.largura.max(1)))
        .clamp(1, u64::from(u32::MAX)) as u32;
    Ok(rgba(image::imageops::resize(
        &fonte,
        largura,
        altura,
        FilterType::Triangle,
    )))
}

/// HEIC (e AVIF) pelo `ffmpeg`: o arquivo pela entrada padrão, o primeiro quadro em PNG pela
/// saída padrão. Nada em disco.
pub(crate) fn pelo_ffmpeg(bytes: Zeroizing<Vec<u8>>) -> Result<Imagem, ErroDaPrevia> {
    let saida = Chamada::nova(localizar_ou_nome("ffmpeg"))
        .argumentos([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ])
        .com_entrada(bytes)
        .com_prazo(Duration::from_secs(30))
        .com_saida_maxima(512 * 1024 * 1024)
        .rodar()
        .map_err(|f| ErroDaPrevia::de_processo("ffmpeg", f))?;
    if saida.stdout_cortado {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    if !saida.status.success() || saida.stdout.is_empty() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        let ultima = erro.lines().last().unwrap_or("sem saída").trim();
        return Err(ErroDaPrevia::Ilegivel(format!("ffmpeg: {ultima}")));
    }
    decodificar(&saida.stdout)
}
