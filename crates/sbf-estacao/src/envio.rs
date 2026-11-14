//! O envio de um lote aprovado do inventário ao servidor (spec 08).
//!
//! Para cada documento do lote: lê o original do cofre, confere o SHA-256 contra o
//! `content_hash` do achado, envia (`POST /documentos`, idempotente pelo SHA-256) e, **a cada
//! aceite**, grava a decisão de promover os registros daquele documento pelo [`Comandos`] do
//! inventário (`decidido_por = usuario`). Um envio pela metade retoma de onde parou: o que já
//! está `promovido` não é enviado de novo, e o que foi enviado e não promovido (a rede caiu
//! entre o aceite e a gravação) é reenviado sem criar nada novo no servidor.
//!
//! Quando todos os documentos foram aceitos, o lote ganha `promovido_em`. Registros que não
//! vêm de documento (conexões bancárias) não passam por aqui e ficam aprovados; o resumo os
//! conta em [`ResumoDoEnvio::sem_documento`].

use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sbf_inventario::banco::Inventario;
use sbf_inventario::referencia::{Ref, Tipo};
use sha2::{Digest, Sha256};

use crate::servidor::{ClienteDoServidor, ErroDoServidor};

/// De onde vêm os bytes de um original do acervo.
pub trait LeitorDeOriginais {
    /// Lê o arquivo de `caminho_acervo` (relativo à raiz do cofre).
    ///
    /// # Errors
    /// Mensagem curta, sem segredo (arquivo que não existe, sem permissão…).
    fn ler(&self, caminho_acervo: &str) -> Result<Vec<u8>, String>;
}

/// O cofre em disco: uma raiz e os caminhos do acervo dentro dela.
#[derive(Debug, Clone)]
pub struct LeitorDeCofre {
    raiz: PathBuf,
}

impl LeitorDeCofre {
    pub fn novo(raiz: impl Into<PathBuf>) -> Self {
        Self { raiz: raiz.into() }
    }
}

impl LeitorDeOriginais for LeitorDeCofre {
    fn ler(&self, caminho_acervo: &str) -> Result<Vec<u8>, String> {
        let relativo = Path::new(caminho_acervo);
        if relativo.is_absolute()
            || relativo
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("o caminho do acervo sai da raiz do cofre".to_owned());
        }
        std::fs::read(self.raiz.join(relativo))
            .map_err(|e| format!("não deu para ler: {}", e.kind()))
    }
}

/// Um documento do lote e o que falta dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentoDoLote {
    pub content_hash: String,
    /// O nome com que o documento vai ao servidor.
    pub nome: String,
    pub tipo_mime: String,
    pub caminho_acervo: Option<String>,
    /// Registros deste documento no lote que ainda não estão `promovido`.
    pub registros_a_promover: Vec<String>,
    /// Registros deste documento no lote que já estão `promovido`.
    pub registros_promovidos: usize,
    /// Registros deste documento no lote que não estão `aprovado` nem `promovido`
    /// (`extraido`, `conferido`, `divergente`): não são enviados nem promovidos.
    pub registros_pulados: usize,
    /// O achado foi `descartado`: o documento não é enviado.
    pub descartado: bool,
}

impl DocumentoDoLote {
    /// Nada mais a fazer: há registros no lote e todos já foram promovidos.
    pub fn concluido(&self) -> bool {
        self.registros_a_promover.is_empty() && self.registros_promovidos > 0
    }

    /// Não entra no envio: achado descartado, ou só tem registros que o usuário não aprovou.
    pub fn pulado(&self) -> bool {
        self.descartado
            || (self.registros_pulados > 0
                && self.registros_a_promover.is_empty()
                && self.registros_promovidos == 0)
    }
}

/// Por que um documento não foi enviado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalhaDoEnvio {
    pub content_hash: String,
    pub nome: String,
    pub motivo: String,
}

/// Onde o envio está (chega à janela a cada passo).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AndamentoDoEnvio {
    /// Quantos documentos o lote tem.
    pub total: usize,
    /// Quantos já terminaram (enviados agora ou retomados).
    pub feitos: usize,
    /// O documento em andamento.
    pub atual: Option<String>,
    pub falhas: usize,
}

/// O que o envio fez.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResumoDoEnvio {
    pub total: usize,
    /// Enviados e aceitos agora.
    pub enviados: usize,
    /// Dos enviados, quantos o servidor já tinha (reenvio).
    pub ja_no_servidor: usize,
    /// Pulados porque já estavam promovidos (envio anterior).
    pub retomados: usize,
    /// Registros promovidos agora.
    pub promovidos: usize,
    /// Registros do lote que não vêm de documento.
    pub sem_documento: usize,
    /// Documentos que não foram enviados porque o achado foi descartado ou os registros
    /// não estão aprovados (não é falha: o lote termina sem eles).
    pub pulados: usize,
    pub falhas: Vec<FalhaDoEnvio>,
    /// O lote ganhou `promovido_em` (todos os documentos aceitos).
    pub lote_promovido: bool,
    /// A rede caiu: nada se perdeu e o mesmo comando retoma.
    pub interrompido: bool,
}

/// Por que o envio nem começou ou teve de parar.
#[derive(Debug, thiserror::Error)]
pub enum ErroDoEnvio {
    #[error("o lote não existe")]
    LoteInexistente,
    #[error("o lote ainda não foi aprovado: aprove antes de enviar")]
    LoteNaoAprovado,
    #[error("erro do inventário: {0}")]
    Inventario(String),
    #[error("{0}")]
    Servidor(ErroDoServidor),
}

impl From<rusqlite::Error> for ErroDoEnvio {
    fn from(e: rusqlite::Error) -> Self {
        Self::Inventario(e.to_string())
    }
}

fn nome_do_arquivo(filename: Option<String>, hash: &str, extensao: Option<&str>) -> String {
    let bruto = filename
        .map(|f| f.rsplit(['/', '\\']).next().unwrap_or("").trim().to_owned())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| hash.to_owned());
    let tem_extensao = Path::new(&bruto).extension().is_some();
    match extensao.filter(|e| !e.is_empty()) {
        Some(e) if !tem_extensao => format!("{bruto}.{}", e.trim_start_matches('.')),
        _ => bruto,
    }
}

/// Os documentos do lote (achados e documentos dos registros), com o que falta de cada um.
///
/// # Errors
/// Lote inexistente ou erro do banco.
pub fn documentos_do_lote(
    conn: &Connection,
    lote: &Ref,
) -> Result<(Vec<DocumentoDoLote>, usize), ErroDoEnvio> {
    let id: i64 = lote
        .chave
        .parse()
        .map_err(|_| ErroDoEnvio::LoteInexistente)?;
    let existe: Option<i64> = conn
        .query_row("SELECT 1 FROM lote WHERE id = ?1", [id], |l| l.get(0))
        .optional()?;
    if existe.is_none() {
        return Err(ErroDoEnvio::LoteInexistente);
    }
    let mut st = conn.prepare(
        "WITH docs AS (
           SELECT alvo_id AS h FROM lote_item WHERE lote_id = ?1 AND alvo_tipo = 'achado'
           UNION
           SELECT r.content_hash FROM lote_item i JOIN registro r ON r.id = CAST(i.alvo_id AS INTEGER)
            WHERE i.lote_id = ?1 AND i.alvo_tipo = 'registro' AND r.content_hash IS NOT NULL)
         SELECT a.content_hash, a.mime_type, a.extensao, a.caminho_acervo, a.estado,
                (SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                  ORDER BY o.id LIMIT 1)
           FROM docs JOIN achado a ON a.content_hash = docs.h ORDER BY a.content_hash",
    )?;
    type Achado = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    );
    let base: Vec<Achado> = st
        .query_map([id], |l| {
            Ok((
                l.get(0)?,
                l.get(1)?,
                l.get(2)?,
                l.get(3)?,
                l.get(4)?,
                l.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut documentos = Vec::with_capacity(base.len());
    for (hash, mime, extensao, caminho, estado_do_achado, filename) in base {
        let mut st = conn.prepare(
            "SELECT r.id, r.estado FROM lote_item i JOIN registro r ON r.id = CAST(i.alvo_id AS INTEGER)
              WHERE i.lote_id = ?1 AND i.alvo_tipo = 'registro' AND r.content_hash = ?2
              ORDER BY r.id",
        )?;
        let registros: Vec<(i64, String)> = st
            .query_map(params![id, hash], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<_, _>>()?;
        documentos.push(DocumentoDoLote {
            nome: nome_do_arquivo(filename, &hash, extensao.as_deref()),
            tipo_mime: mime.unwrap_or_else(|| "application/octet-stream".to_owned()),
            caminho_acervo: caminho,
            registros_promovidos: registros.iter().filter(|(_, e)| e == "promovido").count(),
            registros_pulados: registros
                .iter()
                .filter(|(_, e)| e != "promovido" && e != "aprovado")
                .count(),
            descartado: estado_do_achado == "descartado",
            registros_a_promover: registros
                .into_iter()
                .filter(|(_, e)| e == "aprovado")
                .map(|(r, _)| r.to_string())
                .collect(),
            content_hash: hash,
        });
    }
    let sem_documento: i64 = conn.query_row(
        "SELECT count(*) FROM lote_item i JOIN registro r ON r.id = CAST(i.alvo_id AS INTEGER)
          WHERE i.lote_id = ?1 AND i.alvo_tipo = 'registro' AND r.content_hash IS NULL",
        [id],
        |l| l.get(0),
    )?;
    Ok((documentos, usize::try_from(sem_documento).unwrap_or(0)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn promover(inv: &mut Inventario, alvos: Vec<Ref>, motivo: String) -> Result<(), ErroDoEnvio> {
    inv.promover(&alvos, &motivo)
        .map(|_| ())
        .map_err(|e| ErroDoEnvio::Inventario(e.to_string()))
}

/// Envia o lote aprovado. Bloqueia (rede e disco): rode fora da thread da janela; o
/// `progresso` é chamado a cada documento.
///
/// # Errors
/// [`ErroDoEnvio`]. Rede que cai **não** é erro: volta `Ok` com `interrompido`.
pub fn enviar_lote(
    cliente: &ClienteDoServidor,
    inventario: &mut Inventario,
    lote: &Ref,
    leitor: &dyn LeitorDeOriginais,
    progresso: &mut dyn FnMut(&AndamentoDoEnvio),
) -> Result<ResumoDoEnvio, ErroDoEnvio> {
    let id: i64 = lote
        .chave
        .parse()
        .map_err(|_| ErroDoEnvio::LoteInexistente)?;
    let (aprovado_em, promovido_em): (Option<String>, Option<String>) = inventario
        .conexao()
        .query_row(
            "SELECT aprovado_em, promovido_em FROM lote WHERE id = ?1",
            [id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?
        .ok_or(ErroDoEnvio::LoteInexistente)?;
    if aprovado_em.is_none() {
        return Err(ErroDoEnvio::LoteNaoAprovado);
    }
    let (documentos, sem_documento) = documentos_do_lote(inventario.conexao(), lote)?;
    let mut resumo = ResumoDoEnvio {
        total: documentos.len(),
        sem_documento,
        ..ResumoDoEnvio::default()
    };
    let mut andamento = AndamentoDoEnvio {
        total: documentos.len(),
        ..AndamentoDoEnvio::default()
    };
    progresso(&andamento);
    for doc in &documentos {
        andamento.atual = Some(doc.nome.clone());
        if doc.concluido() {
            resumo.retomados += 1;
            andamento.feitos += 1;
            progresso(&andamento);
            continue;
        }
        if doc.pulado() {
            resumo.pulados += 1;
            andamento.feitos += 1;
            progresso(&andamento);
            continue;
        }
        let falha = |motivo: String| FalhaDoEnvio {
            content_hash: doc.content_hash.clone(),
            nome: doc.nome.clone(),
            motivo,
        };
        let bytes = match doc.caminho_acervo.as_deref() {
            None => {
                resumo
                    .falhas
                    .push(falha("o documento não foi copiado para o cofre".into()));
                andamento.falhas += 1;
                progresso(&andamento);
                continue;
            }
            Some(caminho) => match leitor.ler(caminho) {
                Ok(b) => b,
                Err(e) => {
                    resumo.falhas.push(falha(e));
                    andamento.falhas += 1;
                    progresso(&andamento);
                    continue;
                }
            },
        };
        if !hex(&Sha256::digest(&bytes)).eq_ignore_ascii_case(&doc.content_hash) {
            resumo.falhas.push(falha(
                "o arquivo do cofre não é o do achado (o SHA-256 mudou)".into(),
            ));
            andamento.falhas += 1;
            progresso(&andamento);
            continue;
        }
        let sha = doc.content_hash.to_ascii_lowercase();
        match cliente.enviar_documento(
            &sha,
            &doc.nome,
            Some(&doc.content_hash),
            &doc.tipo_mime,
            &bytes,
        ) {
            Ok(resposta) => {
                resumo.enviados += 1;
                if resposta.ja_existia {
                    resumo.ja_no_servidor += 1;
                }
                if !doc.registros_a_promover.is_empty() {
                    let alvos: Vec<Ref> = doc
                        .registros_a_promover
                        .iter()
                        .map(|r| Ref::nova(Tipo::Registro, r.clone()))
                        .collect();
                    let quantos = alvos.len();
                    // O servidor já aceitou o documento: se o inventário recusar promover
                    // (estado mudou no meio), vira falha deste documento, não aborta o lote.
                    match promover(
                        inventario,
                        alvos,
                        format!(
                            "o servidor aceitou o documento {} no envio do lote {}",
                            resposta.source_document_id, lote.chave
                        ),
                    ) {
                        Ok(()) => resumo.promovidos += quantos,
                        Err(e) => {
                            resumo.falhas.push(falha(e.to_string()));
                            andamento.falhas += 1;
                        }
                    }
                }
                andamento.feitos += 1;
            }
            Err(ErroDoServidor::SemRede(_)) => {
                resumo.interrompido = true;
                progresso(&andamento);
                return Ok(resumo);
            }
            Err(
                e @ (ErroDoServidor::EstacaoRevogada
                | ErroDoServidor::NaoAutenticado
                | ErroDoServidor::ContratoAntigo
                | ErroDoServidor::ContratoMaisNovo),
            ) => return Err(ErroDoEnvio::Servidor(e)),
            Err(e) => {
                resumo.falhas.push(falha(e.to_string()));
                andamento.falhas += 1;
            }
        }
        progresso(&andamento);
    }
    if resumo.falhas.is_empty() && promovido_em.is_none() {
        promover(
            inventario,
            vec![lote.clone()],
            format!(
                "todos os documentos do lote {} foram aceitos pelo servidor",
                lote.chave
            ),
        )?;
        resumo.lote_promovido = true;
    }
    andamento.atual = None;
    progresso(&andamento);
    Ok(resumo)
}
