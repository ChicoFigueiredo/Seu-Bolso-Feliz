//! O backup automático do MCP (D15). Como o servidor passou a escrever direto, antes da
//! **primeira** escrita de cada banco, em cada sessão, o banco inteiro é copiado para
//! `<cofre>/backups/mcp/<banco>-AAAAMMDD-HHMMSS.sqlite` (UTC).
//!
//! - A cópia usa a **API de backup do SQLite** (`rusqlite::backup`), que é consistente com o
//!   WAL. Copiar o arquivo cru perderia o que ainda está no `-wal`.
//! - Se o backup falha, a escrita é recusada, com erro claro, e a próxima tentativa tenta de
//!   novo. Nunca se escreve sem rede de segurança.
//! - Ficam no máximo [`MAXIMO_DE_BACKUPS`] backups por banco; os mais antigos **desta
//!   pasta** são apagados. Nada fora de `backups/mcp/` é tocado, nem arquivo que não siga o
//!   nome de um backup do banco.
//! - O arquivo e a pasta são privados (0600 e 0700 em Unix): o cofre tem dado financeiro.
//! - Nada de conteúdo vai a log: as respostas trazem só nomes e tamanhos.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, backup::Backup};
use serde_json::{Value, json};

/// Quantos backups o MCP guarda por banco.
pub const MAXIMO_DE_BACKUPS: usize = 20;
const PASSOS_POR_VEZ: i32 = 256;
const PAUSA_ENTRE_PASSOS: Duration = Duration::from_millis(5);
const ESPERA_OCUPADO: Duration = Duration::from_secs(10);

/// Os bancos que o MCP escreve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Qual {
    Inventario,
    Planejamento,
    Regras,
    /// O `roteiros.sqlite` da pasta de dados do app (fora do cofre): a cópia também vai para
    /// `<cofre>/backups/mcp/`.
    Roteiros,
}

type Relogio = dyn Fn() -> String + Send + Sync;

struct Estado {
    caminhos: HashMap<Qual, PathBuf>,
    /// Os bancos já copiados nesta sessão.
    feitos: BTreeSet<PathBuf>,
}

/// O guarda dos backups. Barato de clonar: os clones compartilham o estado da sessão.
#[derive(Clone)]
pub struct Backups {
    pasta: PathBuf,
    cofre: PathBuf,
    estado: Arc<Mutex<Estado>>,
    relogio: Arc<Relogio>,
}

impl std::fmt::Debug for Backups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backups")
            .field("pasta", &self.pasta)
            .finish_non_exhaustive()
    }
}

fn agora_para_nome() -> String {
    // "2026-11-18T15:02:50Z" -> "20261118-150250".
    let t = sbf_ia::agora_rfc3339();
    let d: String = t.chars().filter(char::is_ascii_digit).collect();
    format!(
        "{}-{}",
        &d[..8.min(d.len())],
        d.get(8..14).unwrap_or("000000")
    )
}

fn falha(motivo: &str) -> String {
    format!(
        "não consegui fazer o backup do banco antes de escrever ({motivo}): a escrita foi recusada e nada mudou"
    )
}

impl Backups {
    /// O guarda do cofre `cofre` (a pasta que tem o `inventario.sqlite`).
    #[must_use]
    pub fn novo(cofre: &Path) -> Self {
        Self {
            pasta: cofre.join("backups").join("mcp"),
            cofre: cofre.to_path_buf(),
            estado: Arc::new(Mutex::new(Estado {
                caminhos: HashMap::new(),
                feitos: BTreeSet::new(),
            })),
            relogio: Arc::new(agora_para_nome),
        }
    }

    /// Troca o relógio (devolve `AAAAMMDD-HHMMSS`): os testes fixam o instante.
    #[must_use]
    pub fn com_relogio(mut self, relogio: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.relogio = Arc::new(relogio);
        self
    }

    /// Diz onde está um dos bancos. Sem isso, escrever nele não faz backup (não há o que
    /// copiar): o `Nucleo` registra os que existem.
    pub fn registrar(&self, qual: Qual, caminho: PathBuf) {
        if let Ok(mut e) = self.estado.lock() {
            e.caminhos.insert(qual, caminho);
        }
    }

    /// A pasta dos backups.
    #[must_use]
    pub fn pasta(&self) -> &Path {
        &self.pasta
    }

    /// Chame antes de **toda** ferramenta que escreve em `qual`. Na primeira vez da sessão
    /// copia o banco; nas outras não faz nada.
    ///
    /// # Errors
    /// O backup falhou: a escrita deve ser recusada com esta mensagem.
    pub fn antes_de_escrever(&self, qual: Qual) -> Result<(), String> {
        let mut e = self
            .estado
            .lock()
            .map_err(|_| falha("o controle de backups travou"))?;
        let Some(caminho) = e.caminhos.get(&qual).cloned() else {
            return Ok(());
        };
        if e.feitos.contains(&caminho) {
            return Ok(());
        }
        self.fazer(&caminho)?;
        e.feitos.insert(caminho);
        Ok(())
    }

    /// Copia `banco` agora, sem olhar se já houve backup nesta sessão, e apara os antigos.
    ///
    /// # Errors
    /// Banco ilegível, pasta sem permissão, disco cheio...
    pub fn fazer(&self, banco: &Path) -> Result<PathBuf, String> {
        let stem = banco
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| falha("nome de banco inválido"))?
            .to_owned();
        if !banco.is_file() {
            return Err(falha("o banco não existe"));
        }
        preparar_pasta(&self.pasta)?;
        let momento = (self.relogio)();
        let destino = self.nome_livre(&stem, &momento);
        let parcial = destino.with_extension("sqlite.parcial");
        let r = copiar(banco, &parcial).and_then(|()| {
            fs::rename(&parcial, &destino).map_err(|_| falha("não deu para guardar a cópia"))
        });
        if let Err(e) = r {
            let _ = fs::remove_file(&parcial);
            return Err(e);
        }
        self.aparar(&stem);
        Ok(destino)
    }

    fn nome_livre(&self, stem: &str, momento: &str) -> PathBuf {
        let base = self.pasta.join(format!("{stem}-{momento}.sqlite"));
        if !base.exists() {
            return base;
        }
        (2_u32..)
            .map(|n| self.pasta.join(format!("{stem}-{momento}-{n}.sqlite")))
            .find(|p| !p.exists())
            .unwrap_or(base)
    }

    /// Apaga os mais antigos de `stem` além do limite, só nesta pasta e só os que têm o nome
    /// de um backup deste banco.
    fn aparar(&self, stem: &str) {
        let Ok(rd) = fs::read_dir(&self.pasta) else {
            return;
        };
        let mut meus: Vec<((String, u32), PathBuf)> = rd
            .filter_map(Result::ok)
            .filter_map(|e| {
                let nome = e.file_name().to_string_lossy().into_owned();
                Some((chave_de_backup(stem, &nome)?, e.path()))
            })
            .collect();
        meus.sort();
        let sobra = meus.len().saturating_sub(MAXIMO_DE_BACKUPS);
        for (_, caminho) in meus.into_iter().take(sobra) {
            let _ = fs::remove_file(caminho);
        }
    }

    /// `fazer_backup`: copia todos os bancos do cofre (os `.sqlite` da pasta do cofre e os
    /// registrados), sob demanda.
    ///
    /// # Errors
    /// Qualquer um dos bancos falhou: a resposta diz qual.
    pub fn fazer_todos(&self) -> Result<Value, String> {
        let mut bancos: BTreeSet<PathBuf> = BTreeSet::new();
        if let Ok(rd) = fs::read_dir(&self.cofre) {
            for e in rd.filter_map(Result::ok) {
                let p = e.path();
                if p.is_file() && p.extension().is_some_and(|x| x == "sqlite") {
                    bancos.insert(p);
                }
            }
        }
        if let Ok(e) = self.estado.lock() {
            bancos.extend(e.caminhos.values().filter(|p| p.is_file()).cloned());
        }
        if bancos.is_empty() {
            return Err("não achei nenhum banco no cofre para copiar".to_owned());
        }
        let mut feitos = Vec::new();
        for banco in bancos {
            let nome = banco
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("banco")
                .to_owned();
            let destino = self.fazer(&banco).map_err(|e| format!("{nome}: {e}"))?;
            let bytes = fs::metadata(&destino).map_or(0, |m| m.len());
            feitos.push(json!({
                "banco": nome,
                "arquivo": destino.file_name().and_then(|n| n.to_str()),
                "bytes": bytes,
            }));
        }
        Ok(json!({
            "feitos": feitos,
            "pasta": self.pasta.display().to_string(),
            "mantidos_por_banco": MAXIMO_DE_BACKUPS,
            "mensagem": "backup feito com a API de backup do SQLite (consistente com o WAL); os mais antigos além do limite foram apagados só desta pasta",
        }))
    }
}

/// `Some((momento, n))` se `nome` é um backup de `stem`: `<stem>-AAAAMMDD-HHMMSS[-N].sqlite`.
fn chave_de_backup(stem: &str, nome: &str) -> Option<(String, u32)> {
    let resto = nome.strip_prefix(stem)?.strip_prefix('-')?;
    let resto = resto.strip_suffix(".sqlite")?;
    let momento = resto.get(..15)?;
    let cauda = &resto[15..];
    let n = if cauda.is_empty() {
        1
    } else {
        cauda.strip_prefix('-')?.parse().ok()?
    };
    let no_formato = momento.bytes().enumerate().all(|(i, c)| {
        if i == 8 {
            c == b'-'
        } else {
            c.is_ascii_digit()
        }
    });
    no_formato.then(|| (momento.to_owned(), n))
}

#[cfg(unix)]
fn preparar_pasta(pasta: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(pasta)
        .map_err(|_| falha("não deu para criar a pasta de backups"))?;
    // `recursive` só aplica o modo às pastas novas; as duas de baixo (`backups` e `mcp`)
    // ficam privadas também.
    for p in [Some(pasta), pasta.parent()].into_iter().flatten() {
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

#[cfg(not(unix))]
fn preparar_pasta(pasta: &Path) -> Result<(), String> {
    fs::create_dir_all(pasta).map_err(|_| falha("não deu para criar a pasta de backups"))
}

#[cfg(unix)]
fn criar_privado(caminho: &Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(caminho)
        .map(|_| ())
        .map_err(|_| falha("não deu para criar o arquivo de backup"))
}

#[cfg(not(unix))]
fn criar_privado(caminho: &Path) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(caminho)
        .map(|_| ())
        .map_err(|_| falha("não deu para criar o arquivo de backup"))
}

/// A cópia consistente: abre a origem só para leitura e usa a API de backup do SQLite.
fn copiar(origem: &Path, destino: &Path) -> Result<(), String> {
    let de = Connection::open_with_flags(origem, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| falha("não deu para abrir o banco"))?;
    de.busy_timeout(ESPERA_OCUPADO)
        .map_err(|_| falha("não deu para esperar o banco"))?;
    criar_privado(destino)?;
    let mut para =
        Connection::open(destino).map_err(|_| falha("não deu para abrir o arquivo de backup"))?;
    {
        let b = Backup::new(&de, &mut para).map_err(|_| falha("a API de backup recusou"))?;
        b.run_to_completion(PASSOS_POR_VEZ, PAUSA_ENTRE_PASSOS, None)
            .map_err(|_| falha("a cópia não terminou"))?;
    }
    // A cópia fica num arquivo só, sem `-wal` ao lado.
    let _ = para.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()));
    para.close()
        .map_err(|_| falha("não deu para fechar a cópia"))
}
