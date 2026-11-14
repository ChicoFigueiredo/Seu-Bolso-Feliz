//! Grava no `inventario.sqlite` o que veio de um mensageiro: fonte, varredura, achado (por
//! sha256) e origem. Idempotente: a chave da origem é única e o achado é por conteúdo, então
//! ler a mesma mensagem duas vezes não duplica nada.

use std::io::Read as _;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension as _, params};
use sbf_coleta::pastas::{LIMITE_COPIA_BYTES, caminho_no_acervo, iso_de};
use sha2::{Digest as _, Sha256};

use crate::Resultado;
use crate::mapeamento::referencia;
use crate::modelo::{Mensagem, Plataforma};

const RESPONSAVEL: &str = "sbf-mensageiros@1";

/// O que aconteceu com um item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    /// Origem nova gravada.
    Novo,
    /// A origem já existia.
    Repetido,
}

/// Contadores de uma coleta.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contadores {
    /// Itens vistos (anexos e textos).
    pub vistos: u64,
    /// Origens novas.
    pub novos: u64,
    /// Origens já vistas.
    pub repetidos: u64,
    /// Conteúdos (sha256) que ainda não existiam.
    pub conteudos_novos: u64,
    /// Cópias feitas ao acervo.
    pub copiados: u64,
    /// Itens que falharam (mídia expirada, disco).
    pub erros: u64,
}

/// O gravador de uma plataforma.
#[derive(Debug)]
pub struct Gravador<'a> {
    conn: &'a Connection,
    cofre: &'a Path,
    plataforma: Plataforma,
    varredura: Option<i64>,
    /// Os contadores até agora.
    pub contadores: Contadores,
}

impl<'a> Gravador<'a> {
    /// Gravador sobre `conn`, copiando ao acervo de `cofre`. A varredura só é aberta no
    /// primeiro item: uma rodada sem novidade não deixa varredura vazia.
    #[must_use]
    pub fn novo(conn: &'a Connection, cofre: &'a Path, plataforma: Plataforma) -> Self {
        Self {
            conn,
            cofre,
            plataforma,
            varredura: None,
            contadores: Contadores::default(),
        }
    }

    /// Já há origem com esta chave?
    ///
    /// # Errors
    /// Falha do banco.
    pub fn ja_tem(&self, chave: &str) -> Resultado<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM origem WHERE origin_key = ?1", [chave], |l| {
                l.get::<_, i64>(0)
            })
            .optional()?
            .is_some())
    }

    fn varredura(&mut self) -> Resultado<i64> {
        if let Some(v) = self.varredura {
            return Ok(v);
        }
        let p = self.plataforma;
        self.conn.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao, escopo) VALUES (?1, 'mensageiro', ?2, 'dentro')",
            params![p.fonte(), p.descricao()],
        )?;
        let id: i64 = self.conn.query_row(
            "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES (?1, 'E2', ?2, ?3) RETURNING id",
            params![p.fonte(), p.nome(), RESPONSAVEL],
            |l| l.get(0),
        )?;
        self.varredura = Some(id);
        Ok(id)
    }

    /// Grava o arquivo `caminho` (já baixado ou escrito pelo chamador) como origem `chave`.
    ///
    /// # Errors
    /// Falha do banco ou do disco.
    pub fn gravar(
        &mut self,
        mensagem: &Mensagem,
        chave: &str,
        nome: &str,
        extensao: &str,
        caminho: &Path,
    ) -> Resultado<Destino> {
        self.contadores.vistos += 1;
        let ja: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM origem WHERE origin_key = ?1", [chave], |l| {
                l.get(0)
            })
            .optional()?;
        if ja.is_some() {
            self.contadores.repetidos += 1;
            return Ok(Destino::Repetido);
        }
        let tamanho = std::fs::metadata(caminho)?.len();
        let hash = sha256_do_arquivo(caminho)?;
        let varredura = self.varredura()?;
        let existente: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
                [&hash],
                |l| l.get(0),
            )
            .optional()?;
        let quando =
            iso_de(std::time::UNIX_EPOCH + std::time::Duration::from_secs(mensagem.quando));
        if existente.is_none() {
            self.conn.execute(
                "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada,
                   data_chave, data_chave_tipo)
                 VALUES (?1, ?2, ?3, ?4, 'A', ?5, 'mensagem')",
                params![
                    hash,
                    i64::try_from(tamanho).unwrap_or(i64::MAX),
                    mime(extensao),
                    (!extensao.is_empty()).then_some(extensao),
                    &quando[..10],
                ],
            )?;
            self.contadores.conteudos_novos += 1;
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type,
               origin_key, local_mtime, filename, file_size_bytes, mensagem_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                hash,
                self.plataforma.fonte(),
                varredura,
                self.plataforma.nome(),
                chave,
                quando,
                nome,
                i64::try_from(tamanho).unwrap_or(i64::MAX),
                referencia(mensagem),
            ],
        )?;
        self.contadores.novos += 1;
        let sem_copia = existente.is_none_or(|e| e.is_none());
        if sem_copia && tamanho <= LIMITE_COPIA_BYTES {
            self.copiar(caminho, &hash, extensao, tamanho)?;
            self.contadores.copiados += 1;
        }
        Ok(Destino::Novo)
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
            let tmp = destino.with_extension(format!("parcial-{}", std::process::id()));
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

    /// Fecha a varredura (se abriu alguma).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn fechar(self, status: &str) -> Resultado<Contadores> {
        if let Some(id) = self.varredura {
            let c = &self.contadores;
            self.conn.execute(
                "UPDATE varredura SET status = ?1, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   vistos = ?2, novos = ?3, repetidos = ?4, erros = ?5 WHERE id = ?6",
                params![
                    status,
                    c.vistos as i64,
                    c.novos as i64,
                    c.repetidos as i64,
                    c.erros as i64,
                    id
                ],
            )?;
        }
        Ok(self.contadores)
    }
}

fn mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "gif" => "image/gif",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "ofx" => "application/x-ofx",
        "txt" => "text/plain",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "zip" => "application/zip",
        _ => return None,
    })
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
