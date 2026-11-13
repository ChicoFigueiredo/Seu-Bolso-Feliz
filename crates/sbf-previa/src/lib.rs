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

use std::path::Path;

pub use sbf_pdf::Candidata;

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

/// A família pela extensão: pdf; png/jpg/jpeg/webp/gif/bmp/tif/tiff; heic/heif; eml/msg;
/// xml/ofx/ofc/html/htm/csv/txt/json; xls/xlsx/xlsm/doc/docx/rtf/odt/ods; zip; o resto.
pub fn familia(extensao: &str) -> Familia {
    match extensao.to_ascii_lowercase().as_str() {
        "pdf" => Familia::Pdf,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => Familia::Imagem,
        "heic" | "heif" => Familia::Heic,
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
    /// A extensão do achado (`achado.extensao`); manda sobre a do nome.
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
    pub sha256: Option<String>,
}

/// Uma entrada de um zip (não extraída).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaDoZip {
    pub nome: String,
    pub bytes: u64,
}

/// Um documento paginado aberto (PDF, ou Office convertido em PDF). Guarda o PDF em
/// memória (decifrado, se era protegido, sempre em memória zerável).
pub struct Paginas {
    total: usize,
    pdf: zeroize::Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Paginas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Paginas({})", self.total)
    }
}

impl Paginas {
    /// Para o agente da prévia montar.
    pub fn novas(total: usize, pdf: zeroize::Zeroizing<Vec<u8>>) -> Self {
        Self { total, pdf }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    /// Bytes do PDF em memória, para o limite do cache.
    pub fn bytes(&self) -> usize {
        self.pdf.len()
    }

    /// Desenha a página `pagina` (começa em 1) com `largura_px` de largura.
    pub fn desenhar(&self, pagina: usize, largura_px: u32) -> Result<Imagem, ErroDaPrevia> {
        let _ = (pagina, largura_px, &self.pdf);
        Err(ErroDaPrevia::NaoImplementado)
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

/// Abre o arquivo e devolve o conteúdo da prévia. Lento: chame numa thread.
pub fn abrir(pedido: Pedido<'_>) -> Result<Conteudo, ErroDaPrevia> {
    let _ = pedido;
    Err(ErroDaPrevia::NaoImplementado)
}

/// A miniatura (a primeira página ou a imagem) com `largura_px` de largura. Para e-mail,
/// texto e zip devolve `None`: a janela mostra o texto.
pub fn miniatura(conteudo: &Conteudo, largura_px: u32) -> Result<Option<Imagem>, ErroDaPrevia> {
    let _ = (conteudo, largura_px);
    Err(ErroDaPrevia::NaoImplementado)
}

/// Cache rotativo **só em memória**: as últimas `max_documentos` prévias ou até
/// `max_bytes`, o que vier primeiro (padrão 30 e 256 MB). A chave é o `content_hash`.
#[derive(Debug)]
pub struct Cache {
    max_documentos: usize,
    max_bytes: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self::novo(30, 256 * 1024 * 1024)
    }
}

impl Cache {
    pub fn novo(max_documentos: usize, max_bytes: usize) -> Self {
        Self {
            max_documentos,
            max_bytes,
        }
    }

    pub fn limites(&self) -> (usize, usize) {
        (self.max_documentos, self.max_bytes)
    }

    /// O conteúdo aberto de um documento, marcando-o como o mais recente.
    pub fn obter(&mut self, chave: &str) -> Option<std::sync::Arc<Conteudo>> {
        let _ = chave;
        None
    }

    /// Guarda o conteúdo e descarta os mais antigos além dos limites.
    pub fn guardar(&mut self, chave: String, conteudo: Conteudo) -> std::sync::Arc<Conteudo> {
        let _ = chave;
        std::sync::Arc::new(conteudo)
    }

    /// Uma página (ou a imagem) já desenhada nessa largura.
    pub fn imagem(
        &mut self,
        chave: &str,
        pagina: usize,
        largura_px: u32,
    ) -> Option<std::sync::Arc<Imagem>> {
        let _ = (chave, pagina, largura_px);
        None
    }

    /// Guarda uma página desenhada; conta no limite de bytes do documento.
    pub fn guardar_imagem(
        &mut self,
        chave: &str,
        pagina: usize,
        largura_px: u32,
        imagem: Imagem,
    ) -> std::sync::Arc<Imagem> {
        let _ = (chave, pagina, largura_px);
        std::sync::Arc::new(imagem)
    }

    /// Bytes ocupados agora.
    pub fn bytes(&self) -> usize {
        0
    }
}
