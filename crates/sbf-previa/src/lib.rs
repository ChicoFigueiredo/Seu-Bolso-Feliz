//! Prévia do arquivo (spec 06, "Prévia do arquivo"; glossário: Prévia).
//!
//! Desenha o arquivo de um documento ou achado como ele é, **em memória**: páginas de PDF
//! (inclusive protegido, decifrado em memória), imagens, e-mail, texto, Office (convertido
//! em PDF numa pasta 0700 em tmpfs, apagada na hora) e a lista de um zip. Não depende de
//! egui: devolve [`Imagem`] RGBA e texto; a janela transforma em textura.
//!
//! Costura entre dois agentes (2026-11-13): o agente da prévia implementa este crate; o
//! agente da janela escreve contra estas assinaturas. Mudar uma assinatura pública exige
//! avisar o outro lado (pelo integrador).
//!
//! As chamadas são **síncronas e podem ser lentas** (pdftoppm ~0,1 a 0,3 s por página,
//! LibreOffice alguns segundos): a janela chama numa thread.
//!
//! Por tipo:
//! - **PDF**: o `pdfinfo` conta as páginas e o `pdftoppm` desenha uma página por vez, os dois
//!   lendo o PDF pela entrada padrão e o `pdftoppm` escrevendo a imagem na saída padrão.
//!   Nenhum arquivo é criado.
//! - **PDF protegido**: o `lopdf` decifra em memória com as [`Candidata`]s do pedido (como o
//!   `sbf-pdf`); o PDF decifrado fica num buffer zerável e vai pela entrada padrão do
//!   `pdftoppm`. Fallback: `qpdf --password-file=-` em pipe. A senha nunca vai em argumento.
//! - **Imagens**: o crate `image`, reduzidas a [`LADO_MAXIMO`] px no lado maior; acima de
//!   [`MEGAPIXELS_MAXIMOS`] dá [`ErroDaPrevia::GrandeDemais`]. HEIC pelo `ffmpeg` em pipe.
//! - **E-mail** `.eml` pelo `mail-parser`; `.msg` ainda sem prévia.
//! - **Texto**: os primeiros [`TEXTO_MAXIMO`] bytes, UTF-8 ou Latin-1; HTML vira texto.
//! - **Office**: `soffice --convert-to pdf` numa pasta 0700 dentro de `$XDG_RUNTIME_DIR`,
//!   apagada na hora; uma conversão por vez. [`faxinar_sobras`] limpa o que um app morto
//!   deixou.
//! - **Zip**: a lista do diretório central, sem extrair.
//!
//! Nenhum arquivo acima de [`TAMANHO_MAXIMO`] é lido ([`ErroDaPrevia::GrandeDemais`]).

use std::io::Read;
use std::path::Path;
use zeroize::Zeroizing;

mod cache;
mod email;
mod imagem;
mod lista_zip;
mod office;
mod pdf;
mod texto;

pub use cache::Cache;
pub use office::faxinar_sobras;
pub use sbf_pdf::Candidata;

/// O maior arquivo que a prévia lê (o mesmo teto do PDF protegido no `sbf-pdf`).
pub const TAMANHO_MAXIMO: u64 = sbf_pdf::abertura::TAMANHO_MAXIMO_DO_PDF;
/// O lado maior de uma imagem depois de reduzida para a tela.
pub const LADO_MAXIMO: u32 = 2400;
/// A maior imagem aceita, em pixels (64 megapixels).
pub const MEGAPIXELS_MAXIMOS: u64 = 64_000_000;
/// A maior largura de página que [`Paginas::desenhar`] aceita; acima disto, é cortada.
pub const LARGURA_MAXIMA: u32 = 4096;
/// Quanto de um arquivo de texto (ou do corpo de um e-mail) a prévia mostra.
pub const TEXTO_MAXIMO: usize = 256 * 1024;

/// Uma imagem RGBA8 (não pré-multiplicada), linha a linha, `largura * altura * 4` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Imagem {
    pub largura: u32,
    pub altura: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Imagem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Imagem({}x{})", self.largura, self.altura)
    }
}

impl Imagem {
    /// Bytes ocupados, para o limite do cache.
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// A família do arquivo, pela extensão (sem ponto, qualquer caixa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Familia {
    Pdf,
    Imagem,
    Heic,
    Email,
    Texto,
    Office,
    Zip,
    Outro,
}

/// A família pela extensão: pdf; png/jpg/jpeg/webp/gif/bmp/tif/tiff; heic/heif/avif;
/// eml/msg; xml/ofx/ofc/html/htm/csv/txt/json; xls/xlsx/xlsm/doc/docx/rtf/odt/ods; zip; o
/// resto. Um ponto no começo é ignorado.
pub fn familia(extensao: &str) -> Familia {
    match extensao
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => Familia::Pdf,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => Familia::Imagem,
        "heic" | "heif" | "avif" => Familia::Heic,
        "eml" | "msg" => Familia::Email,
        "xml" | "ofx" | "ofc" | "html" | "htm" | "csv" | "txt" | "json" => Familia::Texto,
        "xls" | "xlsx" | "xlsm" | "doc" | "docx" | "rtf" | "odt" | "ods" => Familia::Office,
        "zip" => Familia::Zip,
        _ => Familia::Outro,
    }
}

/// O que abrir.
#[derive(Debug)]
pub struct Pedido<'a> {
    /// O arquivo do acervo (caminho absoluto dentro do cofre).
    pub arquivo: &'a Path,
    /// A extensão do achado (`achado.extensao`); manda sobre a do nome. Vazia: vale a do
    /// nome do arquivo.
    pub extensao: &'a str,
    /// Para PDF protegido: as senhas candidatas, na ordem das regras (`sbf_pdf`), geradas
    /// em memória a partir do chaveiro. Vazio num PDF protegido → [`Conteudo::Protegido`].
    pub candidatas: Vec<Candidata>,
}

/// Um e-mail aberto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub de: String,
    pub para: String,
    pub assunto: String,
    /// Como veio no cabeçalho.
    pub data: String,
    /// O corpo em texto (o HTML vira texto).
    pub corpo: String,
    pub anexos: Vec<Anexo>,
}

/// Um anexo de e-mail. Com `sha256`, a janela liga ao achado de mesmo `content_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anexo {
    pub nome: String,
    pub bytes: u64,
    /// O sha256 do conteúdo decodificado do anexo, em hexadecimal minúsculo.
    pub sha256: Option<String>,
}

/// Uma entrada de um zip (não extraída).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaDoZip {
    pub nome: String,
    /// O tamanho descompactado.
    pub bytes: u64,
}

/// Um documento paginado aberto (PDF, ou Office convertido em PDF). Guarda o PDF em
/// memória (decifrado, se era protegido, sempre em memória zerável).
pub struct Paginas {
    total: usize,
    pdf: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Paginas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Paginas({})", self.total)
    }
}

impl Paginas {
    /// Para o agente da prévia montar.
    pub fn novas(total: usize, pdf: Zeroizing<Vec<u8>>) -> Self {
        Self { total, pdf }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    /// Bytes do PDF em memória, para o limite do cache.
    pub fn bytes(&self) -> usize {
        self.pdf.len()
    }

    /// Desenha a página `pagina` (começa em 1) com `largura_px` de largura (a altura segue a
    /// proporção da página; a largura vai de 1 a [`LARGURA_MAXIMA`]). Uma chamada ao
    /// `pdftoppm` por página, com o PDF pela entrada padrão.
    ///
    /// # Errors
    /// Página fora do documento ou PDF que o poppler não lê: [`ErroDaPrevia::Ilegivel`];
    /// sem `pdftoppm`: [`ErroDaPrevia::FaltaPrograma`]; passou de 30 s:
    /// [`ErroDaPrevia::TempoEsgotado`].
    pub fn desenhar(&self, pagina: usize, largura_px: u32) -> Result<Imagem, ErroDaPrevia> {
        if pagina == 0 || pagina > self.total {
            return Err(ErroDaPrevia::Ilegivel(format!(
                "a página {pagina} não existe (o documento tem {})",
                self.total
            )));
        }
        pdf::desenhar(&self.pdf, pagina, largura_px.clamp(1, LARGURA_MAXIMA))
    }
}

/// O conteúdo de uma prévia.
#[derive(Debug)]
pub enum Conteudo {
    Paginas(Paginas),
    Imagem(Imagem),
    Email(Email),
    Texto(String),
    ListaDoZip(Vec<EntradaDoZip>),
    /// PDF protegido sem senha que abra (sem regra, chaveiro trancado).
    Protegido {
        motivo: String,
    },
    /// Sem prévia para este tipo, ou arquivo grande demais: mostrar os metadados e o motivo.
    SemPrevia {
        motivo: String,
    },
}

impl Conteudo {
    /// Bytes ocupados em memória (aproximado), para o limite do [`Cache`].
    pub fn bytes(&self) -> usize {
        match self {
            Self::Paginas(p) => p.bytes(),
            Self::Imagem(i) => i.bytes(),
            Self::Email(e) => {
                e.de.len()
                    + e.para.len()
                    + e.assunto.len()
                    + e.data.len()
                    + e.corpo.len()
                    + e.anexos
                        .iter()
                        .map(|a| a.nome.len() + 64 + 8)
                        .sum::<usize>()
            }
            Self::Texto(t) => t.len(),
            Self::ListaDoZip(l) => l.iter().map(|e| e.nome.len() + 8).sum(),
            Self::Protegido { motivo } | Self::SemPrevia { motivo } => motivo.len(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDaPrevia {
    #[error("o arquivo não existe no acervo")]
    SemArquivo,
    #[error("arquivo grande demais para a prévia")]
    GrandeDemais,
    #[error("a conversão passou do prazo")]
    TempoEsgotado,
    #[error("falta o programa {0}")]
    FaltaPrograma(String),
    #[error("arquivo corrompido ou ilegível: {0}")]
    Ilegivel(String),
    #[error("ainda não implementado")]
    NaoImplementado,
}

impl ErroDaPrevia {
    /// A falha de um programa externo, com o nome dele.
    pub(crate) fn de_processo(programa: &str, falha: sbf_processo::FalhaDeProcesso) -> Self {
        use sbf_processo::FalhaDeProcesso as F;
        match falha {
            F::NaoEncontrado => Self::FaltaPrograma(programa.to_owned()),
            F::TempoEsgotado => Self::TempoEsgotado,
            outra => Self::Ilegivel(format!("{programa}: {outra}")),
        }
    }
}

/// Lê o arquivo para a memória, num buffer zerável, recusando acima de [`TAMANHO_MAXIMO`]
/// antes de ler.
pub(crate) fn ler(caminho: &Path) -> Result<Zeroizing<Vec<u8>>, ErroDaPrevia> {
    let arquivo = std::fs::File::open(caminho).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ErroDaPrevia::SemArquivo,
        _ => ErroDaPrevia::Ilegivel(format!("não abriu: {e}")),
    })?;
    let tamanho = arquivo
        .metadata()
        .map_err(|e| ErroDaPrevia::Ilegivel(e.to_string()))?
        .len();
    if tamanho > TAMANHO_MAXIMO {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    // Reserva o tamanho certo: o buffer não cresce e não deixa cópias para trás.
    let capacidade = usize::try_from(tamanho).unwrap_or(0).saturating_add(1);
    let mut bytes = Zeroizing::new(Vec::with_capacity(capacidade));
    // O tamanho pode mudar entre o `metadata` e a leitura: o `take` é o teto de fato.
    arquivo
        .take(TAMANHO_MAXIMO + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ErroDaPrevia::Ilegivel(e.to_string()))?;
    if bytes.len() as u64 > TAMANHO_MAXIMO {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    Ok(bytes)
}

/// Abre o arquivo e devolve o conteúdo da prévia. Lento: chame numa thread.
///
/// # Errors
/// [`ErroDaPrevia::SemArquivo`], [`ErroDaPrevia::GrandeDemais`] (acima de
/// [`TAMANHO_MAXIMO`], sem ler; imagem acima de [`MEGAPIXELS_MAXIMOS`]),
/// [`ErroDaPrevia::FaltaPrograma`], [`ErroDaPrevia::TempoEsgotado`] ou
/// [`ErroDaPrevia::Ilegivel`].
pub fn abrir(pedido: Pedido<'_>) -> Result<Conteudo, ErroDaPrevia> {
    let extensao = if pedido.extensao.trim().is_empty() {
        pedido
            .arquivo
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    } else {
        pedido.extensao
    };
    let extensao = extensao.trim_start_matches('.').to_ascii_lowercase();
    match familia(&extensao) {
        Familia::Pdf => pdf::abrir(ler(pedido.arquivo)?, pedido.arquivo, &pedido.candidatas),
        Familia::Imagem => imagem::decodificar(&ler(pedido.arquivo)?).map(Conteudo::Imagem),
        Familia::Heic => imagem::pelo_ffmpeg(ler(pedido.arquivo)?).map(Conteudo::Imagem),
        Familia::Email if extensao == "msg" => {
            if !pedido.arquivo.exists() {
                return Err(ErroDaPrevia::SemArquivo);
            }
            Ok(Conteudo::SemPrevia {
                motivo: "e-mail do Outlook (.msg) ainda não tem prévia: abra no programa do \
                         sistema"
                    .to_owned(),
            })
        }
        Familia::Email => email::abrir(&ler(pedido.arquivo)?).map(Conteudo::Email),
        Familia::Texto => texto::abrir(pedido.arquivo, matches!(extensao.as_str(), "html" | "htm"))
            .map(Conteudo::Texto),
        Familia::Office => office::converter(pedido.arquivo, &extensao),
        Familia::Zip => lista_zip::listar(pedido.arquivo).map(Conteudo::ListaDoZip),
        Familia::Outro => {
            if !pedido.arquivo.exists() {
                return Err(ErroDaPrevia::SemArquivo);
            }
            let tipo = if extensao.is_empty() {
                "arquivo sem extensão".to_owned()
            } else {
                format!("tipo .{extensao}")
            };
            Ok(Conteudo::SemPrevia {
                motivo: format!("{tipo} sem prévia: abra no programa do sistema"),
            })
        }
    }
}

/// A miniatura (a primeira página ou a imagem) com `largura_px` de largura. Para e-mail,
/// texto e zip devolve `None`: a janela mostra o texto.
///
/// # Errors
/// Os de [`Paginas::desenhar`].
pub fn miniatura(conteudo: &Conteudo, largura_px: u32) -> Result<Option<Imagem>, ErroDaPrevia> {
    let largura = largura_px.clamp(1, LARGURA_MAXIMA);
    match conteudo {
        Conteudo::Paginas(p) => p.desenhar(1, largura).map(Some),
        Conteudo::Imagem(i) => imagem::na_largura(i, largura).map(Some),
        _ => Ok(None),
    }
}
