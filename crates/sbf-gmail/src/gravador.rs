//! O que a coleta grava no cofre: a pasta `entrada/gmail/`, e no `inventario.sqlite` a fonte
//! `gmail`, a varredura (com o checkpoint no `cursor`), o achado (por sha256) e a origem.
//!
//! Idempotente: a chave da origem é única e o achado é por conteúdo, então ler a mesma
//! mensagem duas vezes não duplica nada, e o mesmo PDF em dois e-mails é um achado com duas
//! origens. Transações curtas: nada fica aberto durante a rede ou o reconhecimento.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension as _, params};
use sha2::{Digest as _, Sha256};

use crate::erro::{Erro, Resultado};

/// A fonte do Gmail no inventário.
pub const FONTE: &str = "gmail";
/// A pasta, relativa ao cofre, onde os anexos baixados ficam.
pub const PASTA_DE_ENTRADA: &str = "entrada/gmail";
const RESPONSAVEL: &str = "sbf-gmail@1";

/// De que mensagem vem o que está sendo gravado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaMensagem {
    pub id: String,
    pub conversa: String,
    pub de: String,
    pub assunto: String,
    /// ISO 8601 em UTC (`2026-11-17T12:00:00Z`).
    pub data: String,
}

/// O que um arquivo (ou texto) a gravar é.
#[derive(Debug, Clone, Copy)]
pub struct Arquivo<'a> {
    /// `gmail:<mensagem>:<parte>` ou `gmail:<mensagem>:corpo`.
    pub chave: &'a str,
    /// O nome original (vai para `origem.filename`).
    pub nome: &'a str,
    /// O tipo declarado, só para o `mime_type` do achado.
    pub mime: &'a str,
    pub bytes: &'a [u8],
    /// `anexo` ou `eml` (o corpo do e-mail), como as vistas do inventário esperam.
    pub papel: &'a str,
    /// O id da parte, para `gmail_attachment_id`.
    pub parte: Option<&'a str>,
}

/// O que gravar produziu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gravado {
    pub sha256: String,
    /// O conteúdo não existia no inventário.
    pub achado_novo: bool,
    /// O arquivo no cofre (caminho absoluto).
    pub caminho: PathBuf,
}

/// O gravador: uma conexão e a pasta do cofre.
#[derive(Debug)]
pub struct Gravador {
    conn: Connection,
    cofre: PathBuf,
}

/// O sha256 em hexadecimal.
#[must_use]
pub fn sha256_de(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn extensao(nome: &str) -> String {
    nome.rfind('.')
        .map(|i| nome[i + 1..].to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_default()
}

fn mime_limpo(mime: &str) -> Option<String> {
    let m = mime.split(';').next().unwrap_or("").trim().to_lowercase();
    (!m.is_empty() && m != "application/octet-stream").then_some(m)
}

impl Gravador {
    /// Abre o `inventario.sqlite` do cofre (que já tem de existir e estar migrado).
    ///
    /// # Errors
    /// [`Erro::Banco`] se não abre.
    pub fn abrir(cofre: &Path) -> Resultado<Self> {
        let banco = cofre.join("inventario.sqlite");
        if !banco.is_file() {
            return Err(Erro::Banco(
                "não achei o inventario.sqlite no cofre (nada foi criado)".to_owned(),
            ));
        }
        let conn = Connection::open(&banco)?;
        conn.busy_timeout(Duration::from_secs(30))?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
        conn.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao, escopo)
             VALUES (?1, 'email', 'Gmail (anexos e avisos por e-mail)', 'dentro')",
            [FONTE],
        )?;
        Ok(Self {
            conn,
            cofre: cofre.to_path_buf(),
        })
    }

    /// O checkpoint: o `internalDate` (ms) da última mensagem processada em ordem, na última
    /// varredura que o guardou.
    ///
    /// # Errors
    /// [`Erro::Banco`].
    pub fn cursor(&self) -> Resultado<Option<i64>> {
        let texto: Option<String> = self
            .conn
            .query_row(
                "SELECT cursor FROM varredura
                  WHERE fonte_id = ?1 AND cursor IS NOT NULL
                  ORDER BY id DESC LIMIT 1",
                [FONTE],
                |l| l.get(0),
            )
            .optional()?;
        Ok(texto.and_then(|t| t.parse().ok()))
    }

    /// Abre a varredura desta coleta (`escopo` é a consulta, sem conteúdo de e-mail).
    ///
    /// # Errors
    /// [`Erro::Banco`].
    pub fn abrir_varredura(&self, consulta: &str) -> Resultado<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel)
             VALUES (?1, 'E2', ?2, ?3) RETURNING id",
            params![FONTE, consulta, RESPONSAVEL],
            |l| l.get(0),
        )?)
    }

    /// Fecha a varredura com os contadores e, se avançou, o novo checkpoint.
    ///
    /// # Errors
    /// [`Erro::Banco`].
    #[allow(clippy::too_many_arguments)]
    pub fn fechar_varredura(
        &self,
        id: i64,
        status: &str,
        cursor: Option<i64>,
        vistos: usize,
        novos: usize,
        repetidos: usize,
        erros: usize,
        resumo: &str,
    ) -> Resultado<()> {
        let n = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        self.conn.execute(
            "UPDATE varredura SET status = ?1, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
               cursor = coalesce(?2, cursor), vistos = ?3, novos = ?4, repetidos = ?5,
               erros = ?6, resumo = ?7 WHERE id = ?8",
            params![
                status,
                cursor.map(|c| c.to_string()),
                n(vistos),
                n(novos),
                n(repetidos),
                n(erros),
                resumo,
                id
            ],
        )?;
        Ok(())
    }

    /// Esta origem já está no inventário?
    ///
    /// # Errors
    /// [`Erro::Banco`].
    pub fn origem_existe(&self, chave: &str) -> Resultado<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM origem WHERE origin_key = ?1", [chave], |l| {
                l.get::<_, i64>(0)
            })
            .optional()?
            .is_some())
    }

    /// Grava o arquivo em `entrada/gmail/` (se o conteúdo é novo), o achado e a origem.
    ///
    /// # Errors
    /// [`Erro::Disco`] ou [`Erro::Banco`]; o arquivo recém-criado é apagado se o banco recusa.
    pub fn gravar(&self, varredura: i64, m: &DaMensagem, a: &Arquivo<'_>) -> Resultado<Gravado> {
        let hash = sha256_de(a.bytes);
        let existente: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
                [&hash],
                |l| l.get(0),
            )
            .optional()?;
        let achado_novo = existente.is_none();
        let relativo_atual = existente.flatten();

        let (relativo, escrito_agora) = match relativo_atual {
            Some(r) if self.cofre.join(&r).is_file() => (r, false),
            _ => (self.escrever(&hash, a.nome, a.bytes)?, true),
        };

        let ext = extensao(a.nome);
        let tamanho = i64::try_from(a.bytes.len()).unwrap_or(i64::MAX);
        let dia = m.data.get(..10).unwrap_or("");
        let resultado = (|| -> Resultado<()> {
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
            let dentro = (|| -> Resultado<()> {
                if achado_novo {
                    self.conn.execute(
                        "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao,
                           camada, caminho_acervo, data_chave, data_chave_tipo)
                         VALUES (?1, ?2, ?3, ?4, 'A', ?5, ?6, 'mensagem')",
                        params![
                            hash,
                            tamanho,
                            mime_limpo(a.mime),
                            (!ext.is_empty()).then_some(&ext),
                            relativo,
                            (!dia.is_empty()).then_some(dia),
                        ],
                    )?;
                } else if escrito_agora {
                    self.conn.execute(
                        "UPDATE achado SET caminho_acervo = ?1 WHERE content_hash = ?2",
                        params![relativo, hash],
                    )?;
                }
                self.conn.execute(
                    "INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id,
                       origin_type, origin_key, filename, file_size_bytes, gmail_message_id,
                       gmail_thread_id, gmail_attachment_id, gmail_from, gmail_subject,
                       gmail_date, mensagem_ref)
                     VALUES (?1, ?2, ?3, 'gmail', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        hash,
                        FONTE,
                        varredura,
                        a.chave,
                        a.nome,
                        tamanho,
                        m.id,
                        (!m.conversa.is_empty()).then_some(&m.conversa),
                        a.parte,
                        m.de,
                        m.assunto,
                        m.data,
                        a.papel,
                    ],
                )?;
                Ok(())
            })();
            match dentro {
                Ok(()) => {
                    self.conn.execute_batch("COMMIT")?;
                    Ok(())
                }
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })();
        if let Err(e) = resultado {
            if escrito_agora && achado_novo {
                let _ = std::fs::remove_file(self.cofre.join(&relativo));
            }
            return Err(e);
        }
        Ok(Gravado {
            sha256: hash,
            achado_novo,
            caminho: self.cofre.join(relativo),
        })
    }

    /// Escreve os bytes em `entrada/gmail/<sha12>-<nome>` (por arquivo temporário e
    /// renomeação) e devolve o caminho relativo ao cofre.
    fn escrever(&self, hash: &str, nome: &str, bytes: &[u8]) -> Resultado<String> {
        let seguro = sbf_coletor::gmail::mensagem::nome_seguro(nome);
        let seguro = if seguro.is_empty() {
            "anexo".to_owned()
        } else {
            seguro.chars().take(120).collect()
        };
        let relativo = format!("{PASTA_DE_ENTRADA}/{}-{seguro}", &hash[..12]);
        let destino = self.cofre.join(&relativo);
        let pasta = destino.parent().unwrap_or(&self.cofre);
        std::fs::create_dir_all(pasta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700));
        }
        let tmp = destino.with_extension(format!("parcial-{}", std::process::id()));
        let feito = (|| -> std::io::Result<()> {
            std::fs::write(&tmp, bytes)?;
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
        Ok(relativo)
    }
}
