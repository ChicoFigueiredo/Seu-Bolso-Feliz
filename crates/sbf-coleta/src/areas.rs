//! E1: em que camada cai cada pasta, e o que fazer com cada arquivo. Porte fiel de
//! `workers/inventario/src/areas.ts` (regras confirmadas na abertura da E1, 2026-11-11).
//!
//! - **A**: áreas pessoais; hash de documentos e fotos, documentos copiados ao cofre.
//! - **B**: o resto; hash de documentos, fotos só contadas por pasta.
//! - **C**: fora de escopo, com motivo: só contagem. Um arquivo com nome financeiro
//!   (comprovante, fatura, contracheque...) é resgatado mesmo aqui.
//!
//! Precedência para uma pasta: regra pelo caminho exato, depois regra pelo nome da pasta,
//! depois a classe da pasta-mãe. Regra de nome A não tira ninguém de C.
//!
//! Os caminhos são os da máquina do dono (a tabela é dado, não lógica). A coleta de nuvem
//! classifica o item como se estivesse na primeira cópia local (espelho) da nuvem.

use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization as _;

/// Camada de uma pasta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Camada {
    /// Áreas pessoais.
    A,
    /// O resto.
    B,
    /// Fora de escopo.
    C,
}

/// Regra de uma pasta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regra {
    /// A camada.
    pub camada: Camada,
    /// Por que está fora (camada C).
    pub motivo: Option<&'static str>,
    /// Nem entra para contar (o próprio cofre).
    pub ignorar: bool,
}

/// Classe de uma pasta: a regra e onde ela foi decidida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classe {
    /// A regra aplicada.
    pub regra: Regra,
    /// Pasta onde a classe foi decidida.
    pub raiz: String,
}

impl Classe {
    /// A camada da classe.
    #[must_use]
    pub fn camada(&self) -> Camada {
        self.regra.camada
    }
}

/// O que fazer com um arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tratamento {
    /// Hash e cópia ao cofre.
    Copiar,
    /// Só hash; a cópia espera a E3 confirmar que é documento.
    Hash,
    /// Foto fora de área pessoal: contada por pasta.
    FotoContada,
    /// Em área C: contado no agregado da área.
    Contar,
    /// Não é candidato (executável, áudio...).
    Ignorar,
}

impl Tratamento {
    /// O nome que o resumo da varredura usa (o mesmo do TypeScript).
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Copiar => "copiar",
            Self::Hash => "hash",
            Self::FotoContada => "foto-contada",
            Self::Contar => "contar",
            Self::Ignorar => "ignorar",
        }
    }
}

const A: Regra = Regra {
    camada: Camada::A,
    motivo: None,
    ignorar: false,
};

const fn c(motivo: &'static str) -> Regra {
    Regra {
        camada: Camada::C,
        motivo: Some(motivo),
        ignorar: false,
    }
}

const REGRAS_POR_CAMINHO: &[(&str, Camada, Option<&str>, bool)] = &[
    ("/mnt/d/Chico/financas", Camada::C, Some("cofre"), true),
    ("/mnt/e2/Pessoal/Chico", Camada::A, None, false),
    ("/mnt/e2/Pessoal/Pessoal", Camada::A, None, false),
    ("/mnt/p/OneDrive/Pessoal", Camada::A, None, false),
    ("/mnt/p/OneDrive/Documentos", Camada::A, None, false),
    ("/mnt/p/OneDrive/Mayara", Camada::A, None, false),
    ("/mnt/p2/$Nuvem/OneDrive", Camada::A, None, false),
    ("/mnt/p2/$Nuvem/GoogleDrive", Camada::A, None, false),
    ("/mnt/p2/$Nuvem/MegaSync", Camada::A, None, false),
    ("/mnt/p2/$Nuvem/DropBox", Camada::A, None, false),
    ("/mnt/p2/$Nuvem/Sync", Camada::A, None, false),
    ("/mnt/bak/$Nuvem/OneDrive", Camada::A, None, false),
    ("/mnt/bak/$Nuvem/GoogleDrive", Camada::A, None, false),
    ("/mnt/bak/$Nuvem/MegaSync", Camada::A, None, false),
    ("/mnt/bak/$Nuvem/DropBox", Camada::A, None, false),
    ("/mnt/bak/$Nuvem/Sync", Camada::A, None, false),
    ("/mnt/bak/Pessoal", Camada::A, None, false),
    ("/mnt/p2/zzz_delete_Pessoal", Camada::A, None, false),
    ("/home/chico/Downloads", Camada::A, None, false),
    ("/mnt/p2/Downloads", Camada::A, None, false),
    ("/mnt/e2/Downloads", Camada::A, None, false),
    ("/mnt/e/Livros", Camada::C, Some("livros"), false),
    ("/mnt/e2/Livros", Camada::C, Some("livros"), false),
    (
        "/mnt/e2/Livros_Catalogação",
        Camada::C,
        Some("livros"),
        false,
    ),
    ("/mnt/e2/Calibre-Bak", Camada::C, Some("livros"), false),
    ("/mnt/bak/Livros", Camada::C, Some("livros"), false),
    (
        "/mnt/bak/Livros_Catalogação",
        Camada::C,
        Some("livros"),
        false,
    ),
    ("/mnt/bak/Calibre-Bak", Camada::C, Some("livros"), false),
    (
        "/mnt/bak/Calibre-Bak-ANTIGO",
        Camada::C,
        Some("livros"),
        false,
    ),
    ("/mnt/bak/AudioLivros", Camada::C, Some("livros"), false),
    (
        "/mnt/p2/zzz_delete_Livros",
        Camada::C,
        Some("livros"),
        false,
    ),
    ("/mnt/p/OneDrive/Livros", Camada::C, Some("livros"), false),
    ("/mnt/e", Camada::C, Some("estudo"), false),
    ("/mnt/e/_classifica", Camada::B, None, false),
    ("/mnt/e/TEMP", Camada::B, None, false),
    ("/mnt/e2/Estudar", Camada::C, Some("estudo"), false),
    ("/mnt/p2/Cursos", Camada::C, Some("estudo"), false),
    ("/mnt/bak/Cursos", Camada::C, Some("estudo"), false),
    ("/mnt/p/Estudos", Camada::C, Some("estudo"), false),
    ("/mnt/p/OneDrive/Cursos", Camada::C, Some("estudo"), false),
    ("/mnt/p/OneDrive/Estudar", Camada::C, Some("estudo"), false),
    ("/mnt/p2/Jogos", Camada::C, Some("jogos"), false),
    ("/mnt/p2/SteamLibrary", Camada::C, Some("jogos"), false),
    ("/mnt/p2/Emuladores", Camada::C, Some("jogos"), false),
    ("/mnt/bak/Emuladores", Camada::C, Some("jogos"), false),
    ("/mnt/p/PS5", Camada::C, Some("jogos"), false),
    ("/mnt/d/Chico", Camada::C, Some("codigo"), false),
    ("/mnt/d/dev", Camada::C, Some("codigo"), false),
    ("/mnt/d/ia", Camada::C, Some("codigo"), false),
    ("/mnt/d/open-design", Camada::C, Some("codigo"), false),
    ("/mnt/p/dev", Camada::C, Some("codigo"), false),
    ("/mnt/p2/Dev", Camada::C, Some("codigo"), false),
    ("/mnt/bak/Dev", Camada::C, Some("codigo"), false),
    ("/mnt/p/OpenSource", Camada::C, Some("codigo"), false),
    ("/mnt/p/ComfyUI", Camada::C, Some("codigo"), false),
    ("/mnt/p/ComfyUI-audio", Camada::C, Some("codigo"), false),
    ("/mnt/p/ai-toolkit", Camada::C, Some("codigo"), false),
    ("/mnt/p/huggingface", Camada::C, Some("codigo"), false),
    ("/mnt/p/IA", Camada::C, Some("codigo"), false),
    ("/mnt/p/worker-siric", Camada::C, Some("codigo"), false),
    ("/mnt/p2/LINUX", Camada::C, Some("codigo"), false),
    ("/mnt/p/VirtualBox", Camada::C, Some("codigo"), false),
    ("/mnt/e2/VHDX_EXT4_WSL", Camada::C, Some("codigo"), false),
    ("/home/chico/snap", Camada::C, Some("codigo"), false),
    ("/mnt/p2/Musica", Camada::C, Some("midia"), false),
    ("/mnt/e2/Musica", Camada::C, Some("midia"), false),
    ("/mnt/bak/Musica", Camada::C, Some("midia"), false),
    ("/mnt/e2/Video", Camada::C, Some("midia"), false),
    ("/mnt/bak/Video", Camada::C, Some("midia"), false),
    ("/mnt/bak/Podcast", Camada::C, Some("midia"), false),
    ("/mnt/p/OneDrive/Music", Camada::C, Some("midia"), false),
    ("/mnt/p/OneDrive/Videos", Camada::C, Some("midia"), false),
    ("/mnt/p/CAIXA", Camada::C, Some("trabalho"), false),
    ("/mnt/d/CAIXA", Camada::C, Some("trabalho"), false),
    ("/mnt/bak/CAIXA", Camada::C, Some("trabalho"), false),
    (
        "/mnt/bak/Trabalho.CAIXA",
        Camada::C,
        Some("trabalho"),
        false,
    ),
    ("/mnt/p2/$Nuvem/CAIXA", Camada::C, Some("trabalho"), false),
    ("/mnt/p/OneDrive/CAIXA", Camada::C, Some("trabalho"), false),
    (
        "/mnt/p/OneDrive/Trabalho",
        Camada::C,
        Some("trabalho"),
        false,
    ),
    ("/mnt/p/Daiane", Camada::C, Some("outra-pessoa"), false),
    (
        "/mnt/e2/Pessoal/Daiane",
        Camada::C,
        Some("outra-pessoa"),
        false,
    ),
    (
        "/mnt/p2/$Nuvem/OneDrive.Dai",
        Camada::C,
        Some("outra-pessoa"),
        false,
    ),
    (
        "/mnt/bak/$Nuvem/OneDrive.Dai",
        Camada::C,
        Some("outra-pessoa"),
        false,
    ),
    ("/home/pipoka", Camada::C, Some("outra-pessoa"), false),
    (
        "/mnt/p2/Program Files (x86)",
        Camada::C,
        Some("sistema"),
        false,
    ),
    ("/mnt/p/OneDriveTemp", Camada::C, Some("sistema"), false),
    ("/mnt/p2/OneDriveTemp", Camada::C, Some("sistema"), false),
    ("/mnt/p/OneDrive/AppData", Camada::C, Some("sistema"), false),
    ("/mnt/bak/Install", Camada::C, Some("sistema"), false),
    ("/mnt/bak/Portable", Camada::C, Some("sistema"), false),
    (
        "/mnt/p/_backup_credenciais",
        Camada::C,
        Some("credenciais"),
        false,
    ),
];

const NOMES_DE_SISTEMA: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".next",
    ".cache",
    "__pycache__",
    ".venv",
    "venv",
    "site-packages",
    "AppData",
    "Program Files",
    "Program Files (x86)",
    "Windows",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".Trash-1000",
    ".npm",
    ".bun",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    ".vscode-server",
    "steamapps",
];

/// Os caminhos de camada A dentro de `raiz` (a própria `raiz` não conta): o varredor
/// passa por eles primeiro, como o `raizesA` do `scan-discos.ts`.
#[must_use]
pub fn raizes_a_dentro_de(raiz: &str) -> Vec<&'static str> {
    REGRAS_POR_CAMINHO
        .iter()
        .filter(|(p, camada, ..)| {
            *camada == Camada::A
                && *p != raiz
                && (*p == raiz || p.strip_prefix(raiz).is_some_and(|r| r.starts_with('/')))
        })
        .map(|(p, ..)| *p)
        .collect()
}

/// Há regra exata para este caminho (e não só pelo nome da pasta)?
#[must_use]
pub fn tem_regra_exata(caminho: &str) -> bool {
    regra_por_caminho(caminho).is_some()
}

fn regra_por_caminho(caminho: &str) -> Option<Regra> {
    REGRAS_POR_CAMINHO
        .iter()
        .find(|(p, ..)| *p == caminho)
        .map(|(_, camada, motivo, ignorar)| Regra {
            camada: *camada,
            motivo: *motivo,
            ignorar: *ignorar,
        })
}

fn regra_por_nome(nome: &str) -> Option<Regra> {
    match nome {
        "Mayara" => Some(A),
        "Daiane" | "OneDrive.Dai" => Some(c("outra-pessoa")),
        _ if NOMES_DE_SISTEMA.contains(&nome) => Some(c("sistema")),
        _ => None,
    }
}

static DENTRO_DE_IR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/(ir|irpf[0-9]*|impostorenda|imposto[ _.-]de[ _.-]renda)/")
        .expect("regex fixa")
});

/// Último componente, como o `basename` do Node (ignora barras no fim).
fn nome_da_pasta(caminho: &str) -> &str {
    let sem = caminho.trim_end_matches('/');
    sem.rsplit('/').next().unwrap_or(sem)
}

/// Classe de uma pasta, dada a da pasta-mãe (`None` na raiz do disco).
#[must_use]
pub fn classificar_pasta(caminho: &str, mae: Option<&Classe>) -> Classe {
    if let Some(exata) = regra_por_caminho(caminho) {
        return Classe {
            regra: exata,
            raiz: caminho.to_owned(),
        };
    }
    let pela_pasta = regra_por_nome(nome_da_pasta(caminho));
    // Comprovantes de dependente dentro do SEU imposto de renda são seus (decisão da
    // abertura da E2): a pasta herda a classe da mãe.
    if let (Some(p), Some(m)) = (pela_pasta, mae)
        && p.motivo == Some("outra-pessoa")
        && m.camada() == Camada::A
        && DENTRO_DE_IR.is_match(caminho)
    {
        return m.clone();
    }
    if let Some(p) = pela_pasta
        && (p.camada == Camada::C || mae.is_none_or(|m| m.camada() != Camada::C))
    {
        return Classe {
            regra: p,
            raiz: caminho.to_owned(),
        };
    }
    mae.cloned().unwrap_or_else(|| Classe {
        regra: Regra {
            camada: Camada::B,
            motivo: None,
            ignorar: false,
        },
        raiz: caminho.to_owned(),
    })
}

/// Classe de um caminho qualquer, descendo a partir da raiz do sistema.
#[must_use]
pub fn classe_do_caminho(caminho: &str) -> Classe {
    let partes: Vec<&str> = caminho.split('/').filter(|p| !p.is_empty()).collect();
    let mut classe: Option<Classe> = None;
    for i in 1..=partes.len() {
        let atual = format!("/{}", partes[..i].join("/"));
        classe = Some(classificar_pasta(&atual, classe.as_ref()));
    }
    classe.unwrap_or_else(|| Classe {
        regra: Regra {
            camada: Camada::B,
            motivo: None,
            ignorar: false,
        },
        raiz: "/".to_owned(),
    })
}

const DOCUMENTOS: &[&str] = &[
    "pdf", "ofx", "ofc", "qif", "xls", "xlsx", "xlsm", "csv", "ods", "xml", "eml", "msg", "mbox",
    "docx", "doc", "odt", "rtf", "zip", "rar", "7z",
];
/// Texto solto só interessa em área pessoal (exportações, notas, SMS).
const TEXTOS_PESSOAIS: &[&str] = &["txt", "html", "htm", "json"];
const FOTOS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "bmp", "tif", "tiff",
];

static NOME_FINANCEIRO: LazyLock<Regex> = LazyLock::new(|| {
    // `\b` do JS é ASCII; `(?-u:\b)` é o mesmo no Rust.
    Regex::new(concat!(
        r"(comprovante|fatura|extrato|boleto|recibo|nota[ _.-]?fiscal|(?-u:\b)nfs?-?e(?-u:\b)|danfe|",
        r"contra[ _.-]?cheque|holerite|irpf|informes?[ _.-]*(de[ _.-]*)?rendimento|darf|iptu|ipva|",
        r"condominio|pensao|consignad|emprestimo|financiamento|(?-u:\b)pix(?-u:\b))"
    ))
    .expect("regex fixa")
});

/// `\p{Diacritic}` do JS depois de `normalize("NFD")`: marcas combinantes e os símbolos
/// de acento soltos.
fn e_diacritico(c: char) -> bool {
    matches!(
        c,
        '^' | '`' | '\u{00A8}' | '\u{00AF}' | '\u{00B4}' | '\u{00B7}' | '\u{00B8}' | '\u{02B0}'
            ..='\u{036F}'
    )
}

fn sem_acento(texto: &str) -> String {
    texto
        .nfd()
        .filter(|c| !e_diacritico(*c))
        .collect::<String>()
        .to_lowercase()
}

/// O nome do arquivo parece financeiro (comprovante, fatura, contracheque...)?
#[must_use]
pub fn nome_financeiro(nome_do_arquivo: &str) -> bool {
    NOME_FINANCEIRO.is_match(&sem_acento(nome_do_arquivo))
}

/// Extensão em minúsculas, sem o ponto, como o `extname` do Node (`.bashrc` não tem).
#[must_use]
pub fn extensao_de(nome_do_arquivo: &str) -> String {
    match nome_do_arquivo.rfind('.') {
        Some(i) if i > 0 => nome_do_arquivo[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// O que fazer com um arquivo numa pasta de classe `classe`.
#[must_use]
pub fn tratamento_do_arquivo(nome: &str, classe: &Classe) -> Tratamento {
    let ext = extensao_de(nome);
    let e = ext.as_str();
    let documento = DOCUMENTOS.contains(&e);
    let texto = TEXTOS_PESSOAIS.contains(&e);
    let foto = FOTOS.contains(&e);
    let candidato = documento || foto || texto;
    match classe.camada() {
        Camada::C => {
            if candidato && nome_financeiro(nome) {
                Tratamento::Hash
            } else {
                Tratamento::Contar
            }
        }
        Camada::A => {
            if documento || texto {
                Tratamento::Copiar
            } else if foto {
                Tratamento::Hash
            } else {
                Tratamento::Ignorar
            }
        }
        Camada::B => {
            if documento || (candidato && nome_financeiro(nome)) {
                Tratamento::Hash
            } else if foto {
                Tratamento::FotoContada
            } else {
                Tratamento::Ignorar
            }
        }
    }
}
