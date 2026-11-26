//! O tesseract e a preparação da imagem.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::imageops::{self, FilterType};
use image::{DynamicImage, GrayImage, ImageFormat, ImageReader, Limits};
use sbf_processo::{Chamada, FalhaDeProcesso};
use zeroize::Zeroizing;

/// As extensões que passam pelo OCR (em minúsculas).
pub const EXTENSOES_DE_IMAGEM: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// O idioma do tesseract (`tesseract --list-langs`).
pub const IDIOMA_PADRAO: &str = "por";
/// A maior imagem que o OCR lê (15 MiB): acima disto é foto de câmera ou anexo hostil.
pub const LIMITE_DA_IMAGEM: u64 = 15 * 1024 * 1024;
/// O máximo guardado da saída do tesseract (200 KB). O resto é descartado.
pub const LIMITE_DA_SAIDA: usize = 200 * 1024;
/// O prazo do tesseract: passado, ele é morto.
pub const PRAZO_DO_OCR: Duration = Duration::from_secs(30);

/// Abaixo desta maior dimensão (em pixels) a imagem é ampliada 2x: letra de print de app é
/// pequena demais para o tesseract.
const AMPLIAR_ATE: u32 = 1600;
/// Brilho médio (0 a 255) abaixo do qual o fundo é escuro e a imagem é invertida.
const BRILHO_ESCURO: u64 = 128;
/// Teto de largura e altura ao decodificar (uma imagem de 15 MiB bem comprimida pode
/// declarar bilhões de pixels).
const DIMENSAO_MAXIMA: u32 = 12_000;
const MEMORIA_MAXIMA: u64 = 512 * 1024 * 1024;

/// Por que o OCR não entregou o texto. Sem caminho nem conteúdo dentro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum FalhaDoOcr {
    /// Não há `tesseract` confiável neste sistema.
    #[error("OCR indisponível")]
    Indisponivel,
    /// A imagem passa de [`LIMITE_DA_IMAGEM`].
    #[error("a imagem passa do limite do OCR")]
    ImagemGrande,
    /// Não deu para ler o arquivo.
    #[error("não deu para ler a imagem")]
    ArquivoIlegivel,
    /// O arquivo não é uma imagem que se decodifique.
    #[error("a imagem não é legível")]
    ImagemIlegivel,
    /// O tesseract passou do prazo e foi morto.
    #[error("o tesseract passou do limite de tempo")]
    TempoEsgotado,
    /// O tesseract terminou com erro.
    #[error("o tesseract falhou")]
    Falhou,
}

impl FalhaDoOcr {
    /// A causa genérica, para o log e o banco: só uma palavra, sem caminho nem conteúdo.
    #[must_use]
    pub const fn causa(self) -> &'static str {
        match self {
            Self::Indisponivel => "indisponivel",
            Self::ImagemGrande => "imagem_grande",
            Self::ArquivoIlegivel => "arquivo_ilegivel",
            Self::ImagemIlegivel => "formato",
            Self::TempoEsgotado => "prazo",
            Self::Falhou => "tesseract",
        }
    }
}

/// A extensão do arquivo é de uma imagem que o OCR lê?
#[must_use]
pub fn e_imagem(caminho: &Path) -> bool {
    caminho
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| EXTENSOES_DE_IMAGEM.contains(&e.as_str()))
}

/// O tesseract do sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ocr {
    programa: Option<PathBuf>,
    idioma: String,
    prazo: Duration,
}

impl Ocr {
    /// O `tesseract` das pastas do sistema ([`sbf_processo::localizar`]), nunca pelo `PATH`.
    #[must_use]
    pub fn do_sistema() -> Self {
        Self {
            programa: sbf_processo::localizar("tesseract"),
            idioma: IDIOMA_PADRAO.to_owned(),
            prazo: PRAZO_DO_OCR,
        }
    }

    /// Um tesseract em outro caminho (Opções; os testes). Um caminho relativo, ausente ou em
    /// pasta gravável por outros não roda: [`FalhaDoOcr::Indisponivel`].
    #[must_use]
    pub fn com_programa(programa: PathBuf) -> Self {
        Self {
            programa: Some(programa),
            idioma: IDIOMA_PADRAO.to_owned(),
            prazo: PRAZO_DO_OCR,
        }
    }

    /// Troca o prazo ([`PRAZO_DO_OCR`]).
    #[must_use]
    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    /// Há um tesseract confiável para rodar?
    #[must_use]
    pub fn disponivel(&self) -> bool {
        self.programa
            .as_deref()
            .is_some_and(sbf_processo::confiavel)
    }

    /// O texto da imagem. Vazio quando o tesseract não achou nenhum.
    ///
    /// # Errors
    /// [`FalhaDoOcr`]: o limite de tamanho vem primeiro, depois a leitura e a decodificação,
    /// e só então a falta do programa, o prazo ou o erro do tesseract.
    pub fn ler(&self, imagem: &Path) -> Result<String, FalhaDoOcr> {
        let meta = std::fs::metadata(imagem).map_err(|_| FalhaDoOcr::ArquivoIlegivel)?;
        if meta.len() > LIMITE_DA_IMAGEM {
            return Err(FalhaDoOcr::ImagemGrande);
        }
        let Some(programa) = self.programa.clone().filter(|p| sbf_processo::confiavel(p)) else {
            return Err(FalhaDoOcr::Indisponivel);
        };
        let bytes = std::fs::read(imagem).map_err(|_| FalhaDoOcr::ArquivoIlegivel)?;
        let png = preparar_em_png(&bytes)?;
        let saida = Chamada::nova(programa)
            // `stdin` e `stdout` são o arquivo e o resultado: o caminho da imagem não vai
            // para o argv (que aparece no `ps`).
            .argumentos(["stdin", "stdout", "-l", &self.idioma, "--psm", "6"])
            // Uma imagem pequena roda mais depressa numa thread só.
            .com_variavel("OMP_THREAD_LIMIT", Zeroizing::new("1".to_owned()))
            .herdando(&["TESSDATA_PREFIX"])
            .com_entrada(Zeroizing::new(png))
            .com_prazo(self.prazo)
            .com_saida_maxima(LIMITE_DA_SAIDA)
            .rodar()
            .map_err(|f| match f {
                FalhaDeProcesso::TempoEsgotado => FalhaDoOcr::TempoEsgotado,
                FalhaDeProcesso::NaoEncontrado => FalhaDoOcr::Indisponivel,
                _ => FalhaDoOcr::Falhou,
            })?;
        if !saida.status.success() {
            return Err(FalhaDoOcr::Falhou);
        }
        let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
        // Um corte no meio de um caractere deixa um U+FFFD no fim.
        Ok(texto.trim_end_matches('\u{FFFD}').trim().to_owned())
    }
}

fn limites() -> Limits {
    let mut l = Limits::default();
    l.max_image_width = Some(DIMENSAO_MAXIMA);
    l.max_image_height = Some(DIMENSAO_MAXIMA);
    l.max_alloc = Some(MEMORIA_MAXIMA);
    l
}

fn preparar_em_png(bytes: &[u8]) -> Result<Vec<u8>, FalhaDoOcr> {
    let mut leitor = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| FalhaDoOcr::ImagemIlegivel)?;
    leitor.limits(limites());
    let imagem = leitor.decode().map_err(|_| FalhaDoOcr::ImagemIlegivel)?;
    let pronta = preparar(&imagem);
    let mut png = Vec::new();
    pronta
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|_| FalhaDoOcr::ImagemIlegivel)?;
    Ok(png)
}

/// A imagem como o tesseract a lê melhor: em tons de cinza, **invertida se o fundo é
/// escuro** (brilho médio abaixo de 128) e ampliada 2x se a maior dimensão tem menos de
/// 1600 pixels.
#[must_use]
pub fn preparar(imagem: &DynamicImage) -> GrayImage {
    let mut cinza = imagem.to_luma8();
    let pixels = u64::from(cinza.width()) * u64::from(cinza.height());
    let soma: u64 = cinza.pixels().map(|p| u64::from(p.0[0])).sum();
    if soma.checked_div(pixels).is_some_and(|m| m < BRILHO_ESCURO) {
        imageops::invert(&mut cinza);
    }
    if cinza.width().max(cinza.height()) < AMPLIAR_ATE {
        cinza = imageops::resize(
            &cinza,
            cinza.width() * 2,
            cinza.height() * 2,
            FilterType::Lanczos3,
        );
    }
    cinza
}

#[cfg(test)]
mod testes {
    use super::*;
    use image::{Luma, Rgb, RgbImage};

    fn lisa(w: u32, h: u32, cor: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb(cor)))
    }

    #[test]
    fn fundo_escuro_e_invertido() {
        let p = preparar(&lisa(2000, 10, [10, 10, 10]));
        assert_eq!(p.get_pixel(0, 0), &Luma([245]));
    }

    #[test]
    fn fundo_claro_fica_como_esta() {
        let p = preparar(&lisa(2000, 10, [250, 250, 250]));
        assert_eq!(p.get_pixel(0, 0), &Luma([250]));
    }

    #[test]
    fn imagem_pequena_dobra_e_a_grande_nao() {
        let pequena = preparar(&lisa(300, 200, [250, 250, 250]));
        assert_eq!((pequena.width(), pequena.height()), (600, 400));
        let grande = preparar(&lisa(1600, 200, [250, 250, 250]));
        assert_eq!((grande.width(), grande.height()), (1600, 200));
    }

    #[test]
    fn imagem_vazia_nao_derruba() {
        let p = preparar(&lisa(0, 0, [0, 0, 0]));
        assert_eq!((p.width(), p.height()), (0, 0));
    }

    #[test]
    fn so_as_extensoes_de_imagem_passam() {
        for ok in ["a.png", "a.PNG", "a.jpg", "a.jpeg", "a.WebP"] {
            assert!(e_imagem(Path::new(ok)), "{ok}");
        }
        for nao in ["a.pdf", "a.txt", "a.heic", "a", "a.png.zip"] {
            assert!(!e_imagem(Path::new(nao)), "{nao}");
        }
    }
}
