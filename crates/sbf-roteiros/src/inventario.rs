//! O que foi baixado vira achado no `inventario.sqlite` (RO-05), com a mesma costura da coleta
//! de pastas e dos mensageiros: fonte `roteiro:<nome>` (tipo `conexao`), varredura, achado por
//! sha256 e origem `roteiro:<nome>/<execução>/<arquivo>`. Os roteiros gravam direto no cofre
//! (ADR-014). Idempotente: o mesmo conteúdo não vira um segundo achado, nem uma segunda
//! origem da mesma fonte.

use crate::erro::{ErroDeRoteiro, Resultado};
use crate::executor::{Destino, Recebido};
use rusqlite::{Connection, OptionalExtension as _, params};
use sbf_coleta::pastas::{LIMITE_COPIA_BYTES, caminho_no_acervo};
use sha2::{Digest as _, Sha256};
use std::io::Read as _;
use std::path::Path;

const RESPONSAVEL: &str = "sbf-roteiros@1";

/// As extensões que um download de roteiro pode ter para entrar no acervo. Um site (ou um
/// roteiro) hostil não faz o roteiro trazer um executável, uma página ou um script para o
/// cofre. Outra extensão (ou nenhuma) é recusada e o arquivo é descartado.
pub const EXTENSOES_ACEITAS: &[&str] = &[
    "pdf", "csv", "xml", "ofx", "txt", "xlsx", "zip", "png", "jpg", "jpeg",
];

/// A chave da origem: `roteiro:<nome>/<execução>/<arquivo>`.
#[must_use]
pub fn chave_da_origem(roteiro: &str, execucao: i64, arquivo: &str) -> String {
    format!("roteiro:{roteiro}/{execucao}/{arquivo}")
}

/// O destino real: o inventário.
#[derive(Debug)]
pub struct NoInventario<'a> {
    conn: &'a Connection,
    cofre: &'a Path,
    roteiro: String,
    execucao: i64,
    varredura: Option<i64>,
    vistos: u64,
    novos: u64,
    repetidos: u64,
}

impl<'a> NoInventario<'a> {
    /// Um destino para a execução `execucao` do roteiro `roteiro`.
    #[must_use]
    pub fn novo(conn: &'a Connection, cofre: &'a Path, roteiro: &str, execucao: i64) -> Self {
        Self {
            conn,
            cofre,
            roteiro: roteiro.to_owned(),
            execucao,
            varredura: None,
            vistos: 0,
            novos: 0,
            repetidos: 0,
        }
    }

    fn fonte(&self) -> String {
        format!("roteiro:{}", self.roteiro)
    }

    /// Fecha a varredura (se abriu alguma).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn fechar(self, status: &str) -> Resultado<()> {
        if let Some(id) = self.varredura {
            self.conn.execute(
                "UPDATE varredura SET status = ?1, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                   vistos = ?2, novos = ?3, repetidos = ?4 WHERE id = ?5",
                params![
                    status,
                    i64::try_from(self.vistos).unwrap_or(0),
                    i64::try_from(self.novos).unwrap_or(0),
                    i64::try_from(self.repetidos).unwrap_or(0),
                    id
                ],
            )?;
        }
        Ok(())
    }

    fn copiar(&self, origem: &Path, hash: &str, ext: &str, tamanho: u64) -> Resultado<()> {
        let relativo = caminho_no_acervo(hash, ext);
        let destino = self.cofre.join(&relativo);
        let pasta = destino.parent().unwrap_or(self.cofre);
        std::fs::create_dir_all(pasta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700));
        }
        if !destino.exists() || std::fs::metadata(&destino)?.len() != tamanho {
            // nome único: duas threads do mesmo processo podem copiar o mesmo conteúdo
            let unico = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let tmp = destino.with_extension(format!("parcial-{}-{unico}", std::process::id()));
            let feito = (|| -> std::io::Result<()> {
                std::fs::copy(origem, &tmp)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
                }
                std::fs::rename(&tmp, &destino)
            })();
            if let Err(e) = feito {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        self.conn.execute(
            "UPDATE achado SET caminho_acervo = ?1 WHERE content_hash = ?2",
            params![relativo, hash],
        )?;
        Ok(())
    }
}

impl Destino for NoInventario<'_> {
    fn receber(&mut self, arquivo: &Path, nome: &str) -> Resultado<Recebido> {
        self.vistos += 1;
        let ext = arquivo
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !EXTENSOES_ACEITAS.contains(&ext.as_str()) {
            // Descarta sem hashear nem gravar: nada entra no acervo. O que a página chamou o
            // arquivo não é repetido (só se a extensão é curta e simples).
            let _ = std::fs::remove_file(arquivo);
            let vista = if !ext.is_empty()
                && ext.len() <= 8
                && ext.bytes().all(|b| b.is_ascii_alphanumeric())
            {
                format!(".{ext}")
            } else if ext.is_empty() {
                "nenhuma".to_owned()
            } else {
                "desconhecida".to_owned()
            };
            return Err(ErroDeRoteiro::Invalido(format!(
                "download recusado: a extensão ({vista}) não é aceita; só {}. O arquivo foi descartado.",
                EXTENSOES_ACEITAS.join(", ")
            )));
        }
        let chave = chave_da_origem(&self.roteiro, self.execucao, nome);
        let tamanho = std::fs::metadata(arquivo)?.len();
        let hash = sha256_do_arquivo(arquivo)?;
        let fonte = self.fonte();
        // Conferir e gravar numa transação só, com a trava de escrita já tomada: a janela e o
        // worker podem receber o mesmo conteúdo ao mesmo tempo e só um deles o cria.
        let tx = rusqlite::Transaction::new_unchecked(
            self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let ja_chave: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM origem WHERE origin_key = ?1",
                [&chave],
                |l| l.get(0),
            )
            .optional()?;
        // O mesmo conteúdo já veio deste roteiro: não vira outra origem.
        let ja_conteudo: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM origem WHERE content_hash = ?1 AND fonte_id = ?2 LIMIT 1",
                params![hash, fonte],
                |l| l.get(0),
            )
            .optional()?;
        if ja_chave.is_some() || ja_conteudo.is_some() {
            drop(tx);
            self.repetidos += 1;
            return Ok(Recebido::Repetido);
        }
        let varredura = match self.varredura {
            Some(v) => v,
            None => {
                tx.execute(
                    "INSERT OR IGNORE INTO fonte (id, tipo, descricao, escopo) VALUES (?1, 'conexao', ?2, 'dentro')",
                    params![fonte, format!("Roteiro {}", self.roteiro)],
                )?;
                tx.query_row(
                    "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES (?1, 'E2', ?2, ?3) RETURNING id",
                    params![fonte, format!("execucao {}", self.execucao), RESPONSAVEL],
                    |l| l.get(0),
                )?
            }
        };
        let existente: Option<Option<String>> = tx
            .query_row(
                "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
                [&hash],
                |l| l.get(0),
            )
            .optional()?;
        let novo_conteudo = existente.is_none();
        if novo_conteudo {
            tx.execute(
                "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada,
                   data_chave, data_chave_tipo)
                 VALUES (?1, ?2, ?3, ?4, 'A', date('now'), 'arquivo')
                 ON CONFLICT(content_hash) DO NOTHING",
                params![
                    hash,
                    i64::try_from(tamanho).unwrap_or(i64::MAX),
                    mime(&ext),
                    (!ext.is_empty()).then_some(&ext),
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type,
               origin_key, local_mtime, filename, file_size_bytes)
             VALUES (?1, ?2, ?3, 'roteiro', ?4, strftime('%Y-%m-%dT%H:%M:%fZ','now'), ?5, ?6)
             ON CONFLICT(origin_key) DO NOTHING",
            params![
                hash,
                fonte,
                varredura,
                chave,
                nome,
                i64::try_from(tamanho).unwrap_or(i64::MAX)
            ],
        )?;
        tx.commit()?;
        self.varredura = Some(varredura);
        let sem_copia = existente.is_none_or(|e| e.is_none());
        if sem_copia && tamanho <= LIMITE_COPIA_BYTES {
            self.copiar(arquivo, &hash, &ext, tamanho)?;
        }
        self.novos += 1;
        Ok(if novo_conteudo {
            Recebido::Novo
        } else {
            Recebido::Repetido
        })
    }
}

fn mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "ofx" => "application/x-ofx",
        "txt" => "text/plain",
        "zip" => "application/zip",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return None,
    })
}

fn sha256_do_arquivo(caminho: &Path) -> Resultado<String> {
    let mut f = std::fs::File::open(caminho).map_err(ErroDeRoteiro::Disco)?;
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
