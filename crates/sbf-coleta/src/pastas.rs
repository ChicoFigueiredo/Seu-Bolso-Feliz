//! Coleta de pastas locais: porte fiel de `workers/inventario/src/scan-discos.ts` (a
//! varredura em três camadas) mais o vigia de pastas do `sbf worker`.
//!
//! - [`Varredor`]: percorre uma raiz (áreas A, depois o resto B, por último a contagem das
//!   áreas C), grava `achado` e `origem`, copia ao cofre os documentos de área pessoal e
//!   registra as áreas excluídas. Reexecutar é seguro: origem já vista (mesmo caminho e
//!   mtime) é pulada sem reler o arquivo, e é por isso que a varredura periódica de
//!   reconciliação do worker é barata.
//! - [`Vigia`]: observa as raízes com `notify` e entrega, em lotes, os arquivos que mudaram.
//!   O vigia **só avisa**; quem indexa é [`indexar_arquivo`], com as mesmas regras de área.
//!
//! Respeita `areas.rs`: o cofre (`ignorar`), as áreas C (só contagem) e "outra pessoa".
//! Nunca escreve, move nem renomeia nada fora do cofre. O texto extraído (`texto`) não é
//! gravado aqui: no Bun ele é a etapa E3, que continua separada.

use crate::areas::{
    Camada, Classe, Tratamento, classe_do_caminho, classificar_pasta, extensao_de,
    raizes_a_dentro_de, tratamento_do_arquivo,
};
use crate::erro::Resultado;
use rusqlite::{Connection, OptionalExtension as _, params};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::Metadata;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RESPONSAVEL: &str = "sbf-coleta-pastas@1";
/// Acima disto o arquivo fica só com hash, sem cópia ao cofre (decisão da E1).
pub const LIMITE_COPIA_BYTES: u64 = 100 * 1024 * 1024;
/// Acima disto a origem é registrada sem hash (vira pendência).
pub const LIMITE_HASH_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Contadores de uma varredura (os mesmos do `scan-discos.ts`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contadores {
    /// Arquivos vistos.
    pub vistos: u64,
    /// Origens novas.
    pub novos: u64,
    /// Origens já vistas (mesmo caminho e mtime).
    pub repetidos: u64,
    /// Arquivos de área C, só contados.
    pub excluidos: u64,
    /// Erros de leitura ou cópia.
    pub erros: u64,
    /// Conteúdos que ainda não existiam.
    pub conteudos_novos: u64,
    /// Cópias feitas ao cofre.
    pub copiados: u64,
    /// Fotos de área B, só contadas por pasta.
    pub fotos_contadas: u64,
    /// Arquivos vazios.
    pub vazios: u64,
    /// Arquivos acima do limite de hash.
    pub grandes: u64,
    /// Links simbólicos (nunca seguidos).
    pub symlinks: u64,
}

/// Opções da varredura.
#[derive(Debug, Clone, Default)]
pub struct Opcoes {
    /// Pastas que outro processo ainda escreve: ficam para depois.
    pub pular: Vec<PathBuf>,
    /// Só as áreas A dentro da raiz.
    pub so_areas_a: bool,
    /// Material trazido para o cofre (celular, exportações): tudo A.
    pub entrada: bool,
    /// Raiz do cofre: nunca varrida, mesmo que `areas.rs` não a conheça (outra máquina).
    pub cofre: PathBuf,
}

/// O que aconteceu com um arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    /// Origem nova gravada.
    Novo,
    /// Já visto.
    Repetido,
    /// Contado em área C ou foto de B.
    Contado,
    /// Ignorado (extensão que não é candidata, vazio, link).
    Ignorado,
    /// Erro de leitura.
    Erro,
}

fn mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "ofx" => "application/x-ofx",
        "ofc" => "application/x-ofc",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "eml" => "message/rfc822",
        "msg" => "application/vnd.ms-outlook",
        "zip" => "application/zip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        _ => return None,
    })
}

fn dentro_de(caminho: &str, raiz: &str) -> bool {
    caminho == raiz || caminho.starts_with(&format!("{}/", raiz.trim_end_matches('/')))
}

/// Pasta onde as fotos de B são agregadas: até quatro níveis.
fn pasta_de_agregacao(dir: &str) -> String {
    let partes: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).take(4).collect();
    format!("/{}", partes.join("/"))
}

/// `acervo/ab/<hash>.<ext>`, como `caminhoNoAcervo` de `cofre.ts`.
#[must_use]
pub fn caminho_no_acervo(hash: &str, ext: &str) -> String {
    let ext = ext.trim_start_matches('.').to_lowercase();
    if ext.is_empty() {
        format!("acervo/{}/{hash}", &hash[..2])
    } else {
        format!("acervo/{}/{hash}.{ext}", &hash[..2])
    }
}

/// O `mtimeMs` do Node (segundos x 1000 + nanossegundos / 1e6, em `f64`), que entra na chave
/// da origem (`local:<caminho>:<mtimeMs>`). Escrito como o JavaScript: o inteiro sem `.0`.
#[must_use]
pub fn mtime_ms(m: &Metadata) -> f64 {
    let d = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .unwrap_or_default();
    d.as_secs() as f64 * 1000.0 + f64::from(d.subsec_nanos()) / 1e6
}

/// `toISOString()` de um instante: `2026-11-12T10:00:00.123Z`.
#[must_use]
pub fn iso_de(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seg = i64::try_from(d.as_secs()).unwrap_or(0);
    let (dias, resto) = (seg.div_euclid(86_400), seg.rem_euclid(86_400));
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}.{:03}Z",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60,
        d.subsec_millis()
    )
}

fn sha256_do_arquivo(caminho: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(caminho)?;
    let mut h = Sha256::new();
    let mut buf = vec![0_u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Transação curta: o banco é compartilhado com a janela, os scripts Bun e a triagem, então
/// a escrita não segura o lock durante muito tempo. Abre sob demanda e fecha a cada
/// [`LoteCurto::MAX_ARQUIVOS`] arquivos, [`LoteCurto::MAX_BYTES`] lidos ou
/// [`LoteCurto::MAX_TEMPO`]; cai com `ROLLBACK` se for solta aberta (erro no meio).
struct LoteCurto<'a> {
    conn: &'a Connection,
    aberto: bool,
    arquivos: u32,
    bytes: u64,
    desde: std::time::Instant,
}

impl<'a> LoteCurto<'a> {
    const MAX_ARQUIVOS: u32 = 200;
    const MAX_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_TEMPO: Duration = Duration::from_secs(1);

    fn novo(conn: &'a Connection) -> Self {
        Self {
            conn,
            aberto: false,
            arquivos: 0,
            bytes: 0,
            desde: std::time::Instant::now(),
        }
    }

    fn antes(&mut self) -> Resultado<()> {
        if !self.aberto {
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
            self.aberto = true;
            self.arquivos = 0;
            self.bytes = 0;
            self.desde = std::time::Instant::now();
        }
        Ok(())
    }

    fn depois(&mut self, bytes: u64) -> Resultado<()> {
        self.arquivos += 1;
        self.bytes += bytes;
        if self.arquivos >= Self::MAX_ARQUIVOS
            || self.bytes >= Self::MAX_BYTES
            || self.desde.elapsed() >= Self::MAX_TEMPO
        {
            self.fechar()?;
        }
        Ok(())
    }

    fn fechar(&mut self) -> Resultado<()> {
        if self.aberto {
            self.aberto = false;
            self.conn.execute_batch("COMMIT")?;
        }
        Ok(())
    }
}

impl Drop for LoteCurto<'_> {
    fn drop(&mut self) {
        if self.aberto {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

/// O miolo: indexa **um** arquivo (hash, achado, origem, cópia ao cofre).
struct Nucleo<'a> {
    conn: &'a Connection,
    cofre: &'a Path,
    fonte: &'a str,
    varredura_id: i64,
}

impl Nucleo<'_> {
    /// Uma origem gravada pelo Bun pode ter a chave com a ultima casa do `mtimeMs` diferente
    /// (precisao do float). Casa por (caminho, tamanho, mtime em ms, truncado ou arredondado,
    /// como o `toISOString` do Node) para nao criar uma segunda origem do mesmo arquivo.
    fn origem_equivalente(&self, texto: &str, tamanho: u64, meta: &Metadata) -> Resultado<bool> {
        let Ok(t) = meta.modified() else {
            return Ok(false);
        };
        let truncado = iso_de(t);
        let arredondado = iso_de(t + std::time::Duration::from_micros(500));
        let achou: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM origem WHERE local_filepath = ?1 AND file_size_bytes = ?2
                   AND local_mtime IN (?3, ?4) LIMIT 1",
                params![
                    texto,
                    i64::try_from(tamanho).unwrap_or(i64::MAX),
                    truncado,
                    arredondado
                ],
                |l| l.get(0),
            )
            .optional()?;
        Ok(achou.is_some())
    }

    /// `classe`: a classe da pasta do arquivo. `agregar`: onde somar o que for só contado.
    #[allow(clippy::too_many_arguments)]
    fn arquivo(
        &self,
        caminho: &Path,
        nome: &str,
        classe: &Classe,
        c: &mut Contadores,
        agregados: &mut BTreeMap<(String, String), (u64, u64)>,
        ignorados: &mut BTreeMap<String, u64>,
        erros: &mut Vec<String>,
    ) -> Resultado<Destino> {
        c.vistos += 1;
        let tratamento = tratamento_do_arquivo(nome, classe);
        let ext = extensao_de(nome);
        if tratamento == Tratamento::Ignorar {
            *ignorados
                .entry(if ext.is_empty() { "(sem)".into() } else { ext })
                .or_default() += 1;
            return Ok(Destino::Ignorado);
        }
        let meta = match std::fs::metadata(caminho) {
            Ok(m) => m,
            Err(e) => {
                c.erros += 1;
                erros.push(format!("ERRO {}: {e}", caminho.display()));
                return Ok(Destino::Erro);
            }
        };
        let tamanho = meta.len();
        if tratamento == Tratamento::Contar {
            c.excluidos += 1;
            let motivo = classe.regra.motivo.unwrap_or("fora").to_owned();
            let a = agregados.entry((motivo, classe.raiz.clone())).or_default();
            a.0 += 1;
            a.1 += tamanho;
            return Ok(Destino::Contado);
        }
        let texto = caminho.to_string_lossy().into_owned();
        if tratamento == Tratamento::FotoContada {
            c.fotos_contadas += 1;
            let pasta = caminho
                .parent()
                .map_or(String::new(), |p| pasta_de_agregacao(&p.to_string_lossy()));
            let a = agregados
                .entry(("fotos-fora-de-area-pessoal".into(), pasta))
                .or_default();
            a.0 += 1;
            a.1 += tamanho;
            return Ok(Destino::Contado);
        }
        let chave = format!("local:{texto}:{}", mtime_ms(&meta));
        let ja: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM origem WHERE origin_key = ?1",
                [&chave],
                |l| l.get(0),
            )
            .optional()?;
        if ja.is_some() || self.origem_equivalente(&texto, tamanho, &meta)? {
            c.repetidos += 1;
            return Ok(Destino::Repetido);
        }
        if tamanho == 0 {
            c.vazios += 1;
            return Ok(Destino::Ignorado);
        }
        let mtime = meta.modified().map_or_else(|_| iso_de(UNIX_EPOCH), iso_de);
        let inserir_origem = |hash: Option<&str>, erro: Option<&str>| {
            self.conn.execute(
                "INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type,
                   origin_key, local_filepath, local_mtime, filename, file_size_bytes, erro)
                 VALUES (?1, ?2, ?3, 'local_file', ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    hash,
                    self.fonte,
                    self.varredura_id,
                    chave,
                    texto,
                    mtime,
                    nome,
                    i64::try_from(tamanho).unwrap_or(i64::MAX),
                    erro
                ],
            )
        };
        if tamanho > LIMITE_HASH_BYTES {
            c.grandes += 1;
            inserir_origem(None, Some("acima-de-2GB"))?;
            return Ok(Destino::Novo);
        }
        let hash = match sha256_do_arquivo(caminho) {
            Ok(h) => h,
            Err(e) => {
                c.erros += 1;
                erros.push(format!("ERRO {texto}: {e}"));
                return Ok(Destino::Erro);
            }
        };
        let camada = if classe.camada() == Camada::A {
            "A"
        } else {
            "B"
        };
        let existente: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
                [&hash],
                |l| l.get(0),
            )
            .optional()?;
        match &existente {
            None => {
                self.conn.execute(
                    "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        hash,
                        i64::try_from(tamanho).unwrap_or(i64::MAX),
                        mime(&ext),
                        (!ext.is_empty()).then_some(&ext),
                        camada
                    ],
                )?;
                c.conteudos_novos += 1;
            }
            Some(_) if camada == "A" => {
                self.conn.execute(
                    "UPDATE achado SET camada = 'A' WHERE content_hash = ?1 AND camada = 'B'",
                    [&hash],
                )?;
            }
            Some(_) => {}
        }
        inserir_origem(Some(&hash), None)?;
        c.novos += 1;
        let sem_copia = existente.is_none_or(|e| e.is_none());
        if tratamento == Tratamento::Copiar && sem_copia && tamanho <= LIMITE_COPIA_BYTES {
            if let Err(e) = self.copiar(caminho, &hash, &ext, tamanho, &meta) {
                c.erros += 1;
                erros.push(format!("ERRO {texto}: {e}"));
            } else {
                c.copiados += 1;
            }
        }
        Ok(Destino::Novo)
    }

    /// Copia para um nome temporário e renomeia: dois discos com o mesmo conteúdo (os
    /// espelhos) nunca deixam um arquivo final pela metade.
    fn copiar(
        &self,
        origem: &Path,
        hash: &str,
        ext: &str,
        tamanho: u64,
        meta: &Metadata,
    ) -> Resultado<()> {
        let relativo = caminho_no_acervo(hash, ext);
        let destino = self.cofre.join(&relativo);
        let temporario = destino.with_extension(format!("parcial-{}", std::process::id()));
        let pasta = destino.parent().unwrap_or(self.cofre);
        std::fs::create_dir_all(pasta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700));
        }
        let feito = (|| -> std::io::Result<()> {
            if !destino.exists() || std::fs::metadata(&destino)?.len() != tamanho {
                let mut de = std::fs::File::open(origem)?;
                let mut para = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporario)?;
                std::io::copy(&mut de, &mut para)?;
                para.flush()?;
                if para.metadata()?.len() != tamanho {
                    return Err(std::io::Error::other("cópia com tamanho diferente"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    para.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                }
                if let Ok(m) = meta.modified() {
                    let _ = para.set_modified(m);
                }
                drop(para);
                std::fs::rename(&temporario, &destino)?;
            }
            Ok(())
        })();
        if let Err(e) = feito {
            let _ = std::fs::remove_file(&temporario);
            return Err(e.into());
        }
        self.conn.execute(
            "UPDATE achado SET caminho_acervo = ?1 WHERE content_hash = ?2",
            params![relativo, hash],
        )?;
        Ok(())
    }
}

/// Garante a fonte (`disco:<nome>`) antes de varrer.
fn garantir_fonte(conn: &Connection, fonte: &str, raiz: &Path) -> Resultado<()> {
    conn.execute(
        "INSERT OR IGNORE INTO fonte (id, tipo, descricao, raiz, escopo)
         VALUES (?1, 'disco', ?2, ?3, 'dentro')",
        params![
            fonte,
            format!("Pasta vigiada ({fonte})"),
            raiz.to_string_lossy()
        ],
    )?;
    Ok(())
}

/// A varredura de uma raiz.
#[derive(Debug)]
pub struct Varredor<'a> {
    conn: &'a Connection,
    cofre: PathBuf,
    fonte: String,
    varredura_id: i64,
    raiz: PathBuf,
    opcoes: Opcoes,
    raizes_a: BTreeSet<String>,
    fila_c: Vec<(PathBuf, Classe)>,
    agregados: BTreeMap<(String, String), (u64, u64)>,
    ignorados: BTreeMap<String, u64>,
    /// Contadores até agora.
    pub contadores: Contadores,
    /// Mensagens de erro (caminho e causa), para o log do cofre. Nunca vão ao log do worker.
    pub erros: Vec<String>,
}

impl<'a> Varredor<'a> {
    /// Abre uma varredura nova de `raiz` na fonte `fonte`. Marca como interrompida a anterior
    /// da mesma fonte que ficou `rodando` (morreu sem avisar).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn novo(conn: &'a Connection, fonte: &str, raiz: &Path, opcoes: Opcoes) -> Resultado<Self> {
        garantir_fonte(conn, fonte, raiz)?;
        let etapa = if opcoes.entrada { "E2" } else { "E1" };
        conn.execute(
            "UPDATE varredura SET status = 'interrompida'
              WHERE fonte_id = ?1 AND etapa = ?2 AND status = 'rodando'",
            params![fonte, etapa],
        )?;
        let parametros = format!(
            "{{\"limiteHashBytes\":{LIMITE_HASH_BYTES},\"limiteCopiaBytes\":{LIMITE_COPIA_BYTES}}}"
        );
        let varredura_id: i64 = conn.query_row(
            "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, parametros)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
            params![
                fonte,
                etapa,
                raiz.to_string_lossy(),
                RESPONSAVEL,
                parametros
            ],
            |l| l.get(0),
        )?;
        let raiz_txt = raiz.to_string_lossy().into_owned();
        let raizes_a = raizes_a_dentro_de(&raiz_txt)
            .into_iter()
            .filter(|p| Path::new(p).exists())
            .map(str::to_owned)
            .collect();
        Ok(Self {
            conn,
            cofre: opcoes.cofre.clone(),
            fonte: fonte.to_owned(),
            varredura_id,
            raiz: raiz.to_path_buf(),
            opcoes,
            raizes_a,
            fila_c: Vec::new(),
            agregados: BTreeMap::new(),
            ignorados: BTreeMap::new(),
            contadores: Contadores::default(),
            erros: Vec::new(),
        })
    }

    /// O id da varredura gravada.
    #[must_use]
    pub fn varredura_id(&self) -> i64 {
        self.varredura_id
    }

    /// Percorre: áreas A, depois o resto (B), por último a contagem das áreas C. Para entre
    /// diretórios quando `cancelar` vira `true` (a varredura fica `interrompida`).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn varrer(&mut self, cancelar: &AtomicBool) -> Resultado<bool> {
        let raiz = self.raiz.clone();
        if self.opcoes.entrada {
            let classe = Classe {
                regra: crate::areas::Regra {
                    camada: Camada::A,
                    motivo: None,
                    ignorar: false,
                },
                raiz: raiz.to_string_lossy().into_owned(),
            };
            return self.andar(&raiz, classe, 'A', cancelar);
        }
        for raiz_a in self.raizes_a.clone() {
            let classe = classe_do_caminho(&raiz_a);
            if !self.andar(Path::new(&raiz_a), classe, 'A', cancelar)? {
                return Ok(false);
            }
        }
        if self.opcoes.so_areas_a {
            return Ok(true);
        }
        let classe_raiz = classe_do_caminho(&raiz.to_string_lossy());
        if classe_raiz.regra.ignorar {
            return Ok(true);
        }
        if classe_raiz.camada() == Camada::C {
            self.fila_c.push((raiz.clone(), classe_raiz));
        } else if !self.andar(&raiz, classe_raiz, 'B', cancelar)? {
            return Ok(false);
        }
        while let Some((dir, classe)) = self.fila_c.pop() {
            if !self.andar(&dir, classe, 'C', cancelar)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `false` se foi cancelada.
    fn andar(
        &mut self,
        inicio: &Path,
        classe: Classe,
        fase: char,
        cancelar: &AtomicBool,
    ) -> Resultado<bool> {
        let mut pilha = vec![(inicio.to_path_buf(), classe)];
        while let Some((dir, classe)) = pilha.pop() {
            if cancelar.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let entradas = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) => {
                    self.contadores.erros += 1;
                    self.erros.push(format!("ERRO {}: {e}", dir.display()));
                    continue;
                }
            };
            let mut lote = LoteCurto::novo(self.conn);
            for ent in entradas.flatten() {
                let caminho = ent.path();
                let Ok(tipo) = ent.file_type() else { continue };
                let nome = ent.file_name().to_string_lossy().into_owned();
                let texto = caminho.to_string_lossy().into_owned();
                if !self.cofre.as_os_str().is_empty()
                    && dentro_de(&texto, &self.cofre.to_string_lossy())
                {
                    continue; // o cofre nunca é varrido
                }
                if tipo.is_symlink() {
                    self.contadores.symlinks += 1;
                } else if tipo.is_dir() {
                    if self.opcoes.pular.contains(&caminho) {
                        self.erros.push(format!("PULADA {texto}"));
                        continue;
                    }
                    let mut sub = classificar_pasta(&texto, Some(&classe));
                    if sub.regra.ignorar {
                        continue;
                    }
                    if sub.camada() == Camada::C && !crate::areas::tem_regra_exata(&texto) {
                        // C decidido pelo nome da pasta: conta na área-mãe.
                        sub = if classe.camada() == Camada::C {
                            classe.clone()
                        } else {
                            Classe {
                                raiz: pasta_de_agregacao(&texto),
                                ..sub
                            }
                        };
                    }
                    if fase != 'A' && self.raizes_a.contains(&texto) {
                        continue; // já varrida na fase A
                    }
                    if fase == 'B' && sub.camada() == Camada::C && classe.camada() != Camada::C {
                        self.fila_c.push((caminho, sub));
                        continue;
                    }
                    pilha.push((caminho, sub));
                } else if tipo.is_file() {
                    lote.antes()?;
                    let nucleo = Nucleo {
                        conn: self.conn,
                        cofre: &self.cofre,
                        fonte: &self.fonte,
                        varredura_id: self.varredura_id,
                    };
                    nucleo.arquivo(
                        &caminho,
                        &nome,
                        &classe,
                        &mut self.contadores,
                        &mut self.agregados,
                        &mut self.ignorados,
                        &mut self.erros,
                    )?;
                    lote.depois(std::fs::metadata(&caminho).map_or(0, |m| m.len()))?;
                }
            }
            lote.fechar()?;
            self.salvar_progresso(&dir)?;
        }
        Ok(true)
    }

    fn salvar_progresso(&self, cursor: &Path) -> Resultado<()> {
        let c = &self.contadores;
        self.conn.execute(
            "UPDATE varredura SET vistos = ?1, novos = ?2, repetidos = ?3, excluidos = ?4,
               erros = ?5, cursor = ?6 WHERE id = ?7",
            params![
                c.vistos as i64,
                c.novos as i64,
                c.repetidos as i64,
                c.excluidos as i64,
                c.erros as i64,
                cursor.to_string_lossy(),
                self.varredura_id
            ],
        )?;
        Ok(())
    }

    /// Fecha a varredura: grava as áreas excluídas e o resumo.
    ///
    /// # Errors
    /// Falha do banco.
    pub fn finalizar(self, status: &str) -> Resultado<Contadores> {
        let tx = self.conn.unchecked_transaction()?;
        for ((motivo, caminho), (arquivos, bytes)) in &self.agregados {
            tx.execute(
                "INSERT OR REPLACE INTO area_excluida (fonte_id, varredura_id, caminho, motivo, arquivos, bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![self.fonte, self.varredura_id, caminho, motivo, *arquivos as i64, *bytes as i64],
            )?;
        }
        let mut ign: Vec<(&String, &u64)> = self.ignorados.iter().collect();
        ign.sort_by(|a, b| b.1.cmp(a.1));
        let ignorados: BTreeMap<&String, &u64> = ign.into_iter().take(40).collect();
        let c = &self.contadores;
        let resumo = serde_json::json!({
            "vistos": c.vistos, "novos": c.novos, "repetidos": c.repetidos,
            "excluidos": c.excluidos, "erros": c.erros, "conteudosNovos": c.conteudos_novos,
            "copiados": c.copiados, "fotosContadas": c.fotos_contadas, "vazios": c.vazios,
            "grandes": c.grandes, "symlinks": c.symlinks, "ignorados": ignorados,
        });
        tx.execute(
            "UPDATE varredura SET status = ?1, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
               vistos = ?2, novos = ?3, repetidos = ?4, excluidos = ?5, erros = ?6, resumo = ?7
             WHERE id = ?8",
            params![
                status,
                c.vistos as i64,
                c.novos as i64,
                c.repetidos as i64,
                c.excluidos as i64,
                c.erros as i64,
                resumo.to_string(),
                self.varredura_id
            ],
        )?;
        tx.commit()?;
        Ok(self.contadores)
    }
}

/// Indexa **um** arquivo que o vigia avisou, com as mesmas regras de área da varredura (a
/// classe vem do caminho da pasta, descendo da raiz). Devolve o destino. Usa a varredura
/// `varredura_id` (a "vigia" da fonte, ver [`abrir_varredura_do_vigia`]).
///
/// # Errors
/// Falha do banco.
pub fn indexar_arquivo(
    conn: &Connection,
    cofre: &Path,
    fonte: &str,
    varredura_id: i64,
    caminho: &Path,
    contadores: &mut Contadores,
) -> Resultado<Destino> {
    let texto = caminho.to_string_lossy();
    if !cofre.as_os_str().is_empty() && dentro_de(&texto, &cofre.to_string_lossy()) {
        return Ok(Destino::Ignorado);
    }
    let Ok(meta) = std::fs::symlink_metadata(caminho) else {
        return Ok(Destino::Ignorado); // sumiu entre o aviso e a leitura
    };
    if !meta.is_file() {
        return Ok(Destino::Ignorado);
    }
    let Some(pasta) = caminho.parent() else {
        return Ok(Destino::Ignorado);
    };
    let classe = classe_do_caminho(&pasta.to_string_lossy());
    if classe.regra.ignorar {
        return Ok(Destino::Ignorado);
    }
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nucleo = Nucleo {
        conn,
        cofre,
        fonte,
        varredura_id,
    };
    nucleo.arquivo(
        caminho,
        &nome,
        &classe,
        contadores,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut Vec::new(),
    )
}

/// A varredura que agrega o que o vigia indexa (etapa `E1`, responsável `sbf-vigia`). Uma por
/// fonte; reaproveita a que já existe aberta e `rodando`.
///
/// # Errors
/// Falha do banco.
pub fn abrir_varredura_do_vigia(conn: &Connection, fonte: &str, raiz: &Path) -> Resultado<i64> {
    garantir_fonte(conn, fonte, raiz)?;
    let existente: Option<i64> = conn
        .query_row(
            "SELECT id FROM varredura WHERE fonte_id = ?1 AND responsavel = 'sbf-vigia'
               AND status = 'rodando' ORDER BY id DESC LIMIT 1",
            [fonte],
            |l| l.get(0),
        )
        .optional()?;
    if let Some(id) = existente {
        return Ok(id);
    }
    Ok(conn.query_row(
        "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES (?1, 'E1', ?2, 'sbf-vigia')
         RETURNING id",
        params![fonte, raiz.to_string_lossy()],
        |l| l.get(0),
    )?)
}

/// O vigia de pastas: `notify` avisa, o worker indexa.
pub struct Vigia {
    // Mantido vivo: soltá-lo para de vigiar.
    _vigia: notify::RecommendedWatcher,
    rx: Receiver<notify::Result<notify::Event>>,
    /// Raízes que de fato estão sendo vigiadas.
    pub vigiadas: Vec<PathBuf>,
}

impl std::fmt::Debug for Vigia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vigia")
            .field("vigiadas", &self.vigiadas)
            .finish_non_exhaustive()
    }
}

impl Vigia {
    /// Começa a vigiar `raizes` (recursivo). Raiz que não existe é pulada.
    ///
    /// # Errors
    /// Falha do `notify` ao criar o observador.
    pub fn iniciar(raizes: &[PathBuf]) -> notify::Result<Self> {
        use notify::Watcher as _;
        let (tx, rx) = channel();
        let mut vigia = notify::recommended_watcher(tx)?;
        let mut vigiadas = Vec::new();
        for raiz in raizes {
            if raiz.is_dir() && vigia.watch(raiz, notify::RecursiveMode::Recursive).is_ok() {
                vigiadas.push(raiz.clone());
            }
        }
        Ok(Self {
            _vigia: vigia,
            rx,
            vigiadas,
        })
    }

    /// Espera até `prazo` por mudanças e devolve os arquivos (sem repetir) que foram criados
    /// ou alterados. Eventos de remoção e de pasta não entram.
    pub fn mudancas(&self, prazo: Duration) -> Vec<PathBuf> {
        use notify::EventKind::{Create, Modify};
        let mut vistos = HashSet::new();
        let mut saida = Vec::new();
        let fim = std::time::Instant::now() + prazo;
        loop {
            let resta = fim.saturating_duration_since(std::time::Instant::now());
            let Ok(ev) = self.rx.recv_timeout(resta) else {
                break;
            };
            let Ok(ev) = ev else { continue };
            if matches!(ev.kind, Create(_) | Modify(_)) {
                for p in ev.paths {
                    if p.is_file() {
                        if vistos.insert(p.clone()) {
                            saida.push(p);
                        }
                    } else if p.is_dir() {
                        // Pasta nova: o que caiu nela antes do `notify` passar a vigiá-la
                        // não gera evento próprio.
                        for f in arquivos_de(&p) {
                            if vistos.insert(f.clone()) {
                                saida.push(f);
                            }
                        }
                    }
                }
            }
        }
        saida
    }
}

/// Os arquivos (sem seguir links) de uma pasta recém-criada, até um teto que protege de uma
/// árvore enorme colada de uma vez (a varredura de reconciliação pega o resto).
fn arquivos_de(pasta: &Path) -> Vec<PathBuf> {
    const TETO: usize = 5000;
    let mut saida = Vec::new();
    let mut pilha = vec![pasta.to_path_buf()];
    while let Some(d) = pilha.pop() {
        let Ok(entradas) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entradas.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                pilha.push(e.path());
            } else if t.is_file() {
                saida.push(e.path());
                if saida.len() >= TETO {
                    return saida;
                }
            }
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn iso_como_o_javascript() {
        let t = UNIX_EPOCH + Duration::from_millis(1_731_412_345_678);
        assert_eq!(iso_de(t), "2024-11-12T11:52:25.678Z");
        assert_eq!(iso_de(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn acervo_como_o_cofre_ts() {
        let h = "ab".repeat(32);
        assert_eq!(caminho_no_acervo(&h, ".PDF"), format!("acervo/ab/{h}.pdf"));
        assert_eq!(caminho_no_acervo(&h, ""), format!("acervo/ab/{h}"));
    }

    #[test]
    fn agregacao_em_quatro_niveis() {
        assert_eq!(
            pasta_de_agregacao("/mnt/p/OneDrive/Familia/2024/x"),
            "/mnt/p/OneDrive/Familia"
        );
    }
}
