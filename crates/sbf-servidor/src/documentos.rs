//! `POST /api/v2/documentos` (spec 08): recebe os bytes de um documento do inventário, grava
//! no [`crate::armazenamento::Armazenamento`] e cria o `source_documents` e o `ingestion_jobs`
//! em `discovered`, no formato que o worker Bun espera (o mesmo que `local-scanner` cria):
//!
//! - `source_documents`: `origin_type = 'manual_upload'`, `origin_key = upload:{nome}:{sha256}`,
//!   `storage_path = {dono}/{uuid}/{nome}` (o worker lê `ingestion-originals/{storage_path}`),
//!   `status = 'active'`, `content_hash` = SHA-256 hexadecimal dos bytes;
//! - `ingestion_runs`: uma execução `manual_upload` por documento, `running`;
//! - `ingestion_jobs`: `status = 'discovered'`, `requires` pelas mesmas regras do scanner
//!   (sem `pasta-local`: o servidor não lê pasta).
//!
//! Idempotente pelo SHA-256 do dono, sob um `pg_advisory_xact_lock`: duas chamadas
//! simultâneas com o mesmo conteúdo criam um documento só.

use axum::extract::{Multipart, State};
use axum::{Extension, Json};
use sbf_contrato::{
    CAMPO_ARQUIVO, CAMPO_ID_NO_INVENTARIO, CAMPO_NOME_ORIGINAL, CAMPO_SHA256, CodigoDeErro,
    RespostaDoDocumento, TAMANHO_MAXIMO_DO_DOCUMENTO,
};
use sha2::{Digest, Sha256};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::EstadoDoServidor;
use crate::armazenamento::Armazenamento;
use crate::estacoes::EstacaoAutenticada;
use crate::http::{ContratoDoPedido, ErroHttp};

/// As extensões que o pipeline abre (`FORMATOS` de `@sbf/contracts`), com o tipo MIME.
const FORMATOS: [(&str, &str); 9] = [
    (".pdf", "application/pdf"),
    (".csv", "text/csv"),
    (
        ".xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (".ofx", "application/x-ofx"),
    (".xml", "application/xml"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".webp", "image/webp"),
];

const BYTES_DE_CAUDA: usize = 8192;

fn extensao(nome: &str) -> String {
    nome.rfind('.')
        .map(|i| nome[i..].to_lowercase())
        .unwrap_or_default()
}

/// O nome sem pastas, sem controle e sem nada que quebre um caminho de blob.
fn nome_seguro(bruto: &str) -> String {
    let ultimo = bruto.rsplit(['/', '\\']).next().unwrap_or(bruto);
    let limpo = ultimo
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();
    let limpo = limpo.trim().trim_start_matches('.');
    if limpo.chars().count() <= TAMANHO_MAXIMO_DO_NOME {
        return limpo.to_owned();
    }
    // Corta o miolo e preserva a extensão (ela decide o formato aceito e o tipo MIME).
    let ext = limpo
        .rfind('.')
        .map(|i| &limpo[i..])
        .filter(|e| e.chars().count() <= 16)
        .unwrap_or("");
    let miolo = &limpo[..limpo.len() - ext.len()];
    let sobra = TAMANHO_MAXIMO_DO_NOME - ext.chars().count();
    format!("{}{ext}", miolo.chars().take(sobra).collect::<String>())
}

const TAMANHO_MAXIMO_DO_NOME: usize = 160;

/// As capacidades que o worker precisa ter (`resolveFileRequires` do scanner, sem a da
/// pasta local).
fn exigencias(nome: &str, bytes: &[u8]) -> Vec<String> {
    let ext = extensao(nome);
    let e_pdf = ext == ".pdf";
    let e_imagem = matches!(ext.as_str(), ".png" | ".jpg" | ".jpeg" | ".webp");
    let mut requires = Vec::new();
    if e_pdf || e_imagem {
        requires.push("ocr".to_owned());
    }
    if e_pdf {
        let inicio = bytes.len().saturating_sub(BYTES_DE_CAUDA);
        if bytes[inicio..].windows(8).any(|w| w == b"/Encrypt") {
            requires.push("senha-pdf".to_owned());
        }
    }
    requires
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn e_sha256(texto: &str) -> bool {
    texto.len() == 64
        && texto
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

struct Pedido {
    sha256: String,
    nome: String,
    id_no_inventario: Option<String>,
    bytes: Vec<u8>,
}

fn falha_do_multipart(e: &axum::extract::multipart::MultipartError) -> ErroHttp {
    if e.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        return ErroHttp::novo(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            CodigoDeErro::ArquivoGrande,
            "o arquivo é maior que o servidor aceita",
        );
    }
    ErroHttp::pedido_invalido("o envio não é um multipart válido")
}

async fn ler_pedido(mut partes: Multipart) -> Result<Pedido, ErroHttp> {
    let mut sha256 = None;
    let mut nome = None;
    let mut id_no_inventario = None;
    let mut arquivo: Option<Vec<u8>> = None;
    while let Some(campo) = partes
        .next_field()
        .await
        .map_err(|e| falha_do_multipart(&e))?
    {
        let nome_do_campo = campo.name().unwrap_or_default().to_owned();
        match nome_do_campo.as_str() {
            CAMPO_ARQUIVO => {
                // O tipo que o cliente declara é ignorado: o MIME sai da extensão.
                let bytes = campo.bytes().await.map_err(|e| falha_do_multipart(&e))?;
                arquivo = Some(bytes.to_vec());
            }
            CAMPO_SHA256 => {
                sha256 = Some(campo.text().await.map_err(|e| falha_do_multipart(&e))?);
            }
            CAMPO_NOME_ORIGINAL => {
                nome = Some(campo.text().await.map_err(|e| falha_do_multipart(&e))?);
            }
            CAMPO_ID_NO_INVENTARIO => {
                id_no_inventario = Some(campo.text().await.map_err(|e| falha_do_multipart(&e))?);
            }
            _ => {}
        }
    }
    let (Some(sha256), Some(nome), Some(bytes)) = (sha256, nome, arquivo) else {
        return Err(ErroHttp::pedido_invalido(
            "faltam campos: sha256, nome_original e arquivo",
        ));
    };
    Ok(Pedido {
        sha256: sha256.trim().to_lowercase(),
        nome: nome_seguro(&nome),
        id_no_inventario: id_no_inventario.filter(|i| !i.is_empty() && i.len() <= 200),
        bytes,
    })
}

/// `POST /documentos`.
pub async fn enviar(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    Extension(contrato): Extension<ContratoDoPedido>,
    partes: Multipart,
) -> Result<Json<RespostaDoDocumento>, ErroHttp> {
    if contrato.0 < 2 {
        return Err(ErroHttp::nao_encontrado("recurso desconhecido"));
    }
    let pedido = ler_pedido(partes).await?;
    if !e_sha256(&pedido.sha256) {
        return Err(ErroHttp::pedido_invalido(
            "o sha256 precisa ter 64 hexadecimais minúsculos",
        ));
    }
    if hex(&Sha256::digest(&pedido.bytes)) != pedido.sha256 {
        return Err(ErroHttp::pedido_invalido(
            "o sha256 não é o dos bytes enviados",
        ));
    }
    let origem = OrigemDoEnvio {
        origem: "desktop",
        detalhe: serde_json::json!({ "origem": "desktop", "estacao": quem.id }),
    };
    let resposta = guardar(
        &estado.banco,
        estado.armazenamento.as_ref(),
        quem.dono,
        &origem,
        &pedido.nome,
        pedido.id_no_inventario,
        pedido.bytes,
    )
    .await?;
    Ok(Json(resposta))
}

/// De onde o documento veio: o rótulo vai para `metadata.origin` do job e o detalhe para
/// `metadata` da execução.
#[derive(Debug, Clone)]
pub struct OrigemDoEnvio {
    pub origem: &'static str,
    pub detalhe: serde_json::Value,
}

/// O caminho único de entrada de um documento (o `POST /documentos` e o bot do Telegram):
/// valida formato e tamanho, grava os bytes e cria `source_documents` + `ingestion_jobs`,
/// idempotente pelo SHA-256 do dono.
pub async fn guardar(
    banco: &deadpool_postgres::Pool,
    armazenamento: &dyn Armazenamento,
    dono: Uuid,
    origem: &OrigemDoEnvio,
    nome: &str,
    id_no_inventario: Option<String>,
    bytes: Vec<u8>,
) -> Result<RespostaDoDocumento, ErroHttp> {
    let pedido = Pedido {
        sha256: hex(&Sha256::digest(&bytes)),
        nome: nome_seguro(nome),
        id_no_inventario,
        bytes,
    };
    if pedido.bytes.is_empty() {
        return Err(ErroHttp::pedido_invalido("o arquivo está vazio"));
    }
    if pedido.bytes.len() > TAMANHO_MAXIMO_DO_DOCUMENTO {
        return Err(ErroHttp::novo(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            CodigoDeErro::ArquivoGrande,
            "o arquivo é maior que o servidor aceita",
        ));
    }
    if pedido.nome.is_empty() {
        return Err(ErroHttp::pedido_invalido("o nome do arquivo está vazio"));
    }
    let ext = extensao(&pedido.nome);
    let Some((_, mime_da_extensao)) = FORMATOS.iter().find(|(e, _)| *e == ext) else {
        return Err(ErroHttp::pedido_invalido(
            "formato não aceito: use PDF, CSV, XLSX, OFX, XML, PNG, JPG ou WEBP",
        ));
    };
    let mut cliente = banco.get().await?;
    // Caminho rápido: já existe, com job.
    if let Some(resposta) = existente(&**cliente, dono, &pedido).await? {
        return Ok(resposta);
    }

    // O PUT no Blob vem ANTES da transação (e da trava): não se segura transação aberta
    // durante a rede. A chave sai do sha256 (identifica o conteúdo), então repetir o PUT
    // (concorrência, falha depois dele) regrava os mesmos bytes no mesmo lugar e não deixa
    // blob órfão. Se o documento já existe (só sem job), o blob dele já está lá.
    let ja_tem_documento = documento_do_dono(&**cliente, dono, &pedido.sha256)
        .await?
        .is_some();
    let chave = format!("{}/{}/{}", dono, pedido.sha256, pedido.nome);
    if !ja_tem_documento {
        armazenamento
            .gravar(&chave, &pedido.bytes, mime_da_extensao)
            .await
            .map_err(|e| {
                ErroHttp::novo(
                    axum::http::StatusCode::BAD_GATEWAY,
                    CodigoDeErro::Interno,
                    format!("não deu para guardar o arquivo: {e}"),
                )
            })?;
    }

    let transacao = cliente.transaction().await?;
    // Duas chamadas com o mesmo conteúdo esperam aqui; a segunda vê o que a primeira criou.
    transacao
        .execute(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&format!("{}:{}", dono, pedido.sha256)],
        )
        .await?;
    if let Some(resposta) = existente(&*transacao, dono, &pedido).await? {
        return Ok(resposta);
    }

    // Um documento do mesmo conteúdo que ficou sem job (o pipeline o descartou) ganha o job
    // dele, sem novo `source_documents`.
    let documento = match documento_do_dono(&*transacao, dono, &pedido.sha256).await? {
        Some(id) => id,
        None => {
            let documento = Uuid::new_v4();
            let origem = format!("upload:{}:{}", pedido.nome, pedido.sha256);
            let tamanho = i64::try_from(pedido.bytes.len()).unwrap_or(i64::MAX);
            transacao
                .execute(
                    "INSERT INTO public.source_documents \
                     (id, user_id, origin_type, origin_key, filename, mime_type, file_size_bytes, storage_path, status, content_hash) \
                     VALUES ($1, $2, 'manual_upload', $3, $4, $5, $6, $7, 'active', $8)",
                    &[
                        &documento,
                        &dono,
                        &origem,
                        &pedido.nome,
                        &mime_da_extensao,
                        &tamanho,
                        &chave,
                        &pedido.sha256,
                    ],
                )
                .await?;
            documento
        }
    };
    let execucao: Uuid = transacao
        .query_one(
            "INSERT INTO public.ingestion_runs (user_id, source_type, status, metadata) \
             VALUES ($1, 'manual_upload', 'running', $2) RETURNING id",
            &[&dono, &origem.detalhe],
        )
        .await?
        .try_get(0)?;
    let requires = exigencias(&pedido.nome, &pedido.bytes);
    let job: Uuid = transacao
        .query_one(
            "INSERT INTO public.ingestion_jobs (run_id, user_id, source_document_id, status, requires, metadata) \
             VALUES ($1, $2, $3, 'discovered', $4, $5) RETURNING id",
            &[
                &execucao,
                &dono,
                &documento,
                &requires,
                &serde_json::json!({ "filename": pedido.nome, "origin": origem.origem }),
            ],
        )
        .await?
        .try_get(0)?;
    transacao.commit().await?;
    Ok(RespostaDoDocumento {
        source_document_id: documento,
        job_id: job,
        ja_existia: false,
        sha256: pedido.sha256,
        id_no_inventario: pedido.id_no_inventario,
    })
}

/// O `source_documents` do dono com este conteúdo, tenha job ou não.
async fn documento_do_dono<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    sha256: &str,
) -> Result<Option<Uuid>, ErroHttp> {
    let linha = cliente
        .query_opt(
            "SELECT id FROM public.source_documents WHERE user_id = $1 AND content_hash = $2 \
             ORDER BY created_at, id LIMIT 1",
            &[&dono, &sha256],
        )
        .await?;
    linha
        .map(|l| l.try_get("id").map_err(ErroHttp::from))
        .transpose()
}

/// O documento do dono com este conteúdo e o job dele (cria o job se o documento ficou sem).
async fn existente<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    pedido: &Pedido,
) -> Result<Option<RespostaDoDocumento>, ErroHttp> {
    let Some(documento) = documento_do_dono(cliente, dono, &pedido.sha256).await? else {
        return Ok(None);
    };
    let job = cliente
        .query_opt(
            "SELECT id FROM public.ingestion_jobs WHERE user_id = $1 AND source_document_id = $2 \
             ORDER BY created_at, id LIMIT 1",
            &[&dono, &documento],
        )
        .await?;
    let Some(job) = job else {
        // Um documento sem job: o chamador cria só o job, sem novo `source_documents`.
        return Ok(None);
    };
    Ok(Some(RespostaDoDocumento {
        source_document_id: documento,
        job_id: job.try_get("id")?,
        ja_existia: true,
        sha256: pedido.sha256.clone(),
        id_no_inventario: pedido.id_no_inventario.clone(),
    }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nome_perde_pastas_e_pontos_iniciais() {
        assert_eq!(nome_seguro("../../etc/passwd"), "passwd");
        assert_eq!(nome_seguro("C:\\Docs\\fatura.pdf"), "fatura.pdf");
        assert_eq!(nome_seguro(" .oculto.pdf "), "oculto.pdf");
        assert_eq!(nome_seguro(""), "");
    }

    #[test]
    fn nome_longo_perde_o_miolo_e_guarda_a_extensao() {
        let longo = format!("{}.pdf", "a".repeat(300));
        let n = nome_seguro(&longo);
        assert_eq!(n.chars().count(), 160);
        assert!(n.ends_with(".pdf"));
        let acentuado = format!("{}.xlsx", "é".repeat(300));
        let n = nome_seguro(&acentuado);
        assert_eq!(n.chars().count(), 160);
        assert!(n.ends_with(".xlsx"));
        assert_eq!(nome_seguro("curto.pdf"), "curto.pdf");
    }

    #[test]
    fn exigencias_do_pdf_e_da_imagem() {
        assert_eq!(exigencias("a.pdf", b"%PDF-1.4"), vec!["ocr"]);
        assert_eq!(exigencias("a.PNG", b"x"), vec!["ocr"]);
        assert!(exigencias("a.csv", b"x").is_empty());
        let protegido = [b"%PDF-1.7 ".as_slice(), b"/Encrypt 5 0 R"].concat();
        assert_eq!(exigencias("a.pdf", &protegido), vec!["ocr", "senha-pdf"]);
    }

    #[test]
    fn sha256_em_hexadecimal_minusculo() {
        assert!(e_sha256(&hex(&Sha256::digest(b"x"))));
        assert!(!e_sha256("ABC"));
        assert!(!e_sha256(&"G".repeat(64)));
    }
}
