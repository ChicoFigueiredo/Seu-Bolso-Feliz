//! PDF protegido no banco (spec 06, "PDF protegido", itens 1, 2, 5 e 7). Dono: agente do
//! chaveiro e do PDF. Ver `lib.rs`.
//!
//! Só dados: quem decifra e monta a candidata é o `sbf-pdf`, que chama estas funções. Escreve
//! direto nas tabelas que o esquema do Bun já tem:
//!
//! - **alvos**: `achado.protegido_senha = 1`, sem linha em `texto`, sem pendência de PDF
//!   protegido aberta, fora `descartado`, e com menos falhas que o limite;
//! - **regras** (`regra_senha`) do par (fornecedor, tipo) por prioridade, depois as do
//!   fornecedor com tipo `*`;
//! - **tentativas** (`tentativa_senha`): uma linha por tentativa, com a estratégia e se abriu,
//!   **nunca a senha**; e o contador da regra (`sucessos`/`falhas`, `ultimo_sucesso_em`/
//!   `ultima_falha_em`);
//! - no sucesso, `texto` e `achado.canonical_fingerprint`;
//! - a **pendência** "PDF protegido sem regra" (`etapa='desktop'`,
//!   `tipo='pdf_protegido_sem_regra'`, `referencia='achado:<hash>'`), depois da qual as
//!   tentativas param. Responder a pendência libera uma nova rodada: as falhas contam a partir
//!   da última resposta.
//!
//! Toda escrita é uma transação IMMEDIATE curta, com espera de 30 s, sem nada lento dentro.
//!
//! A tabela `tentativa_senha` do Bun só tem `sucesso` (0/1): não há coluna para o desfecho
//! (corrompido, cifra não suportada). Só desfechos de senha viram tentativa; os outros viram
//! pendência própria.

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::time::Duration;

use crate::pendencia::ETAPA_DA_JANELA;
use crate::texto;

/// A prioridade de uma regra de senha nova quando a decisão não diz outra (o padrão da coluna
/// `regra_senha.prioridade` no esquema do Bun). Menor tenta primeiro.
pub const PRIORIDADE_PADRAO_DA_REGRA: i64 = 100;
/// Falhas de senha por documento antes da pendência (spec 06, item 7).
pub const LIMITE_PADRAO_DE_FALHAS: u32 = 6;
/// A pendência "PDF protegido sem regra".
pub const TIPO_SEM_REGRA: &str = "pdf_protegido_sem_regra";
/// O PDF não é legível, nem com a senha certa.
pub const TIPO_CORROMPIDO: &str = "pdf_protegido_corrompido";
/// A cifra não é suportada pelo `lopdf` e o fallback do `qpdf` não está disponível.
pub const TIPO_CIFRA_NAO_SUPORTADA: &str = "pdf_protegido_cifra_nao_suportada";
/// Os tipos de pendência que tiram o achado dos alvos enquanto abertos.
pub const TIPOS_DE_PENDENCIA: [&str; 3] =
    [TIPO_SEM_REGRA, TIPO_CORROMPIDO, TIPO_CIFRA_NAO_SUPORTADA];
/// O `texto.metodo` do PDF aberto (o texto é o mesmo do `pdftotext` do Bun).
pub const METODO_PDFTOTEXT: &str = "pdftotext";
/// O `texto.metodo` do PDF que abriu e não tem texto (igual ao do `e3-extrair.ts`).
pub const METODO_SEM_TEXTO: &str = "pdf-sem-texto";
/// A espera por um banco ocupado (LICOES.md).
const ESPERA: Duration = Duration::from_secs(30);

/// Falhas de senha desde a última resposta a uma pendência de PDF protegido do achado.
const SQL_FALHAS: &str = "SELECT count(*) FROM tentativa_senha t
  WHERE t.content_hash = ?1 AND t.sucesso = 0
    AND t.em > COALESCE((SELECT max(p.resolvida_em) FROM pendencia p
                          WHERE p.referencia = ?2 AND p.tipo IN (?3, ?4, ?5)), '')";

/// `pendencia.referencia` de um achado.
pub fn referencia_do_achado(content_hash: &str) -> String {
    format!("achado:{content_hash}")
}

/// Um achado protegido a abrir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlvoProtegido {
    pub content_hash: String,
    /// Relativo à raiz do cofre.
    pub caminho_acervo: Option<String>,
    /// `origem.local_filepath` com o tamanho registrado, para quando o acervo não tem cópia.
    pub caminhos_locais: Vec<(String, Option<i64>)>,
    pub file_size_bytes: i64,
    pub fornecedor: Option<String>,
    pub tipo_de_documento: Option<String>,
    /// Falhas de senha desde a última resposta a uma pendência de PDF protegido.
    pub falhas: u32,
}

/// Uma linha ativa de `regra_senha`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegraDeSenha {
    pub id: i64,
    pub fornecedor: String,
    pub tipo_de_documento: String,
    pub estrategia: String,
    pub prioridade: i64,
    pub sucessos: i64,
}

/// O resultado de senha de uma tentativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultadoDaTentativa {
    /// A senha abriu.
    Abriu,
    /// A senha não confere.
    SenhaErrada,
}

/// Uma tentativa feita: a estratégia (nome canônico) e a regra que a pediu, se houver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TentativaFeita {
    pub estrategia: String,
    pub regra: Option<i64>,
    pub resultado: ResultadoDaTentativa,
}

/// Como a rodada terminou para o documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conclusao {
    /// Abriu com texto: `conteudo` já cortado como o Bun corta, `impressao` sobre o texto
    /// inteiro.
    Aberto { conteudo: String, impressao: String },
    /// Abriu e não tem texto (candidato a OCR): `texto` vazio com `pdf-sem-texto`.
    SemTexto,
    /// Nada abriu e não há mais o que tentar: pendência "PDF protegido sem regra".
    SemRegra { pergunta: String },
    /// Outra pendência (corrompido, cifra não suportada).
    Pendencia {
        tipo: &'static str,
        pergunta: String,
    },
    /// Só as tentativas (ainda há o que tentar, ou a ferramenta falhou).
    SoTentativas,
}

/// O que [`registrar`] gravou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registro {
    /// Falhas de senha do documento depois desta rodada.
    pub falhas: u32,
    /// A pendência aberta (nova ou que já existia), se houver.
    pub pendencia: Option<i64>,
}

fn espera(conexao: &Connection) -> rusqlite::Result<()> {
    conexao.busy_timeout(ESPERA)
}

fn contar_falhas(conexao: &Connection, content_hash: &str) -> rusqlite::Result<u32> {
    let n: i64 = conexao.query_row(
        SQL_FALHAS,
        params![
            content_hash,
            referencia_do_achado(content_hash),
            TIPO_SEM_REGRA,
            TIPO_CORROMPIDO,
            TIPO_CIFRA_NAO_SUPORTADA
        ],
        |l| l.get(0),
    )?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Falhas de senha do achado desde a última resposta a uma pendência de PDF protegido.
pub fn falhas_desde_a_ultima_resposta(
    conexao: &Connection,
    content_hash: &str,
) -> rusqlite::Result<u32> {
    espera(conexao)?;
    contar_falhas(conexao, content_hash)
}

/// Os achados protegidos a abrir, do mais recente ao mais antigo.
pub fn alvos(conexao: &Connection, limite_de_falhas: u32) -> rusqlite::Result<Vec<AlvoProtegido>> {
    espera(conexao)?;
    let mut consulta = conexao.prepare(
        "SELECT a.content_hash, a.caminho_acervo, a.file_size_bytes, a.supplier_name_raw,
                a.document_type
           FROM achado a
          WHERE a.protegido_senha = 1
            AND a.estado <> 'descartado'
            AND NOT EXISTS (SELECT 1 FROM texto t WHERE t.content_hash = a.content_hash)
            AND NOT EXISTS (SELECT 1 FROM pendencia p
                             WHERE p.referencia = 'achado:' || a.content_hash
                               AND p.tipo IN (?1, ?2, ?3) AND p.resolvida_em IS NULL)
          ORDER BY a.primeiro_visto_em DESC, a.content_hash",
    )?;
    let linhas = consulta
        .query_map(
            params![TIPO_SEM_REGRA, TIPO_CORROMPIDO, TIPO_CIFRA_NAO_SUPORTADA],
            |l| {
                Ok(AlvoProtegido {
                    content_hash: l.get(0)?,
                    caminho_acervo: l.get(1)?,
                    caminhos_locais: Vec::new(),
                    file_size_bytes: l.get(2)?,
                    fornecedor: l.get(3)?,
                    tipo_de_documento: l.get(4)?,
                    falhas: 0,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut origens = conexao.prepare(
        "SELECT local_filepath, file_size_bytes FROM origem
          WHERE content_hash = ?1 AND local_filepath IS NOT NULL ORDER BY id",
    )?;
    let mut saida = Vec::new();
    for mut alvo in linhas {
        alvo.falhas = contar_falhas(conexao, &alvo.content_hash)?;
        if alvo.falhas >= limite_de_falhas {
            continue;
        }
        alvo.caminhos_locais = origens
            .query_map([&alvo.content_hash], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        saida.push(alvo);
    }
    Ok(saida)
}

/// Fornecedor igual, sem diferença de caixa nem de espaço nas pontas.
fn mesmo_fornecedor(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// As regras ativas, em ordem de tentativa: as do par (fornecedor, tipo) por prioridade
/// (menor primeiro; empate: mais sucessos, depois a mais antiga), depois as do fornecedor com
/// tipo `*` na mesma ordem. Sem fornecedor, nenhuma.
pub fn regras_para(
    conexao: &Connection,
    fornecedor: Option<&str>,
    tipo_de_documento: Option<&str>,
) -> rusqlite::Result<Vec<RegraDeSenha>> {
    let Some(fornecedor) = fornecedor.filter(|f| !f.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    espera(conexao)?;
    let mut consulta = conexao.prepare(
        "SELECT id, fornecedor, document_type, estrategia, prioridade, sucessos
           FROM regra_senha WHERE ativa = 1
          ORDER BY prioridade, sucessos DESC, id",
    )?;
    let todas = consulta
        .query_map([], |l| {
            Ok(RegraDeSenha {
                id: l.get(0)?,
                fornecedor: l.get(1)?,
                tipo_de_documento: l.get(2)?,
                estrategia: l.get(3)?,
                prioridade: l.get(4)?,
                sucessos: l.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let do_fornecedor = |r: &RegraDeSenha| mesmo_fornecedor(&r.fornecedor, fornecedor);
    let tipo = tipo_de_documento.filter(|t| *t != "*");
    let mut saida: Vec<RegraDeSenha> = todas
        .iter()
        .filter(|r| do_fornecedor(r) && tipo.is_some_and(|t| r.tipo_de_documento == t))
        .cloned()
        .collect();
    saida.extend(
        todas
            .into_iter()
            .filter(|r| do_fornecedor(r) && r.tipo_de_documento == "*"),
    );
    Ok(saida)
}

fn pendencia_aberta(
    tx: &Transaction<'_>,
    referencia: &str,
    tipo: &str,
) -> rusqlite::Result<Option<i64>> {
    tx.query_row(
        "SELECT id FROM pendencia WHERE referencia = ?1 AND tipo = ?2 AND resolvida_em IS NULL
          ORDER BY id LIMIT 1",
        params![referencia, tipo],
        |l| l.get(0),
    )
    .optional()
}

fn abrir_pendencia(
    tx: &Transaction<'_>,
    referencia: &str,
    tipo: &str,
    pergunta: &str,
) -> rusqlite::Result<i64> {
    if let Some(id) = pendencia_aberta(tx, referencia, tipo)? {
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO pendencia (etapa, tipo, referencia, pergunta) VALUES (?1, ?2, ?3, ?4)",
        params![ETAPA_DA_JANELA, tipo, referencia, pergunta],
    )?;
    Ok(tx.last_insert_rowid())
}

/// A pergunta da pendência "PDF protegido sem regra" (sem nenhum segredo).
pub fn pergunta_sem_regra(
    fornecedor: Option<&str>,
    tipo_de_documento: Option<&str>,
    falhas: u32,
) -> String {
    let quem = match (fornecedor, tipo_de_documento) {
        (Some(f), Some(t)) => format!("{t} de {f}"),
        (Some(f), None) => format!("documento de {f}"),
        (None, Some(t)) => format!("{t} sem fornecedor conhecido"),
        (None, None) => "documento sem fornecedor conhecido".to_owned(),
    };
    let porque = if falhas == 0 {
        "não há estratégia de senha para tentar".to_owned()
    } else {
        format!("{falhas} tentativa(s) de senha falharam")
    };
    format!(
        "PDF protegido sem regra ({quem}): {porque}. Qual estratégia abre este documento, ou \
         qual senha guardar no chaveiro?"
    )
}

/// Grava, numa transação IMMEDIATE curta, as tentativas de um documento, os contadores das
/// regras, o desfecho (`texto` e `canonical_fingerprint`, ou a pendência) e, se as falhas
/// chegaram ao limite sem abrir, a pendência "PDF protegido sem regra".
pub fn registrar(
    conexao: &Connection,
    alvo: &AlvoProtegido,
    tentativas: &[TentativaFeita],
    conclusao: &Conclusao,
    limite_de_falhas: u32,
) -> rusqlite::Result<Registro> {
    espera(conexao)?;
    let tx = Transaction::new_unchecked(conexao, TransactionBehavior::Immediate)?;
    let hash = alvo.content_hash.as_str();
    let referencia = referencia_do_achado(hash);
    let agora: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |l| {
        l.get(0)
    })?;
    for t in tentativas {
        let sucesso = t.resultado == ResultadoDaTentativa::Abriu;
        tx.execute(
            "INSERT INTO tentativa_senha (content_hash, estrategia, sucesso, em)
             VALUES (?1, ?2, ?3, ?4)",
            params![hash, t.estrategia, sucesso, agora],
        )?;
        let Some(regra) = t.regra else { continue };
        let sql = if sucesso {
            "UPDATE regra_senha SET sucessos = sucessos + 1, ultimo_sucesso_em = ?2 WHERE id = ?1"
        } else {
            "UPDATE regra_senha SET falhas = falhas + 1, ultima_falha_em = ?2 WHERE id = ?1"
        };
        tx.execute(sql, params![regra, agora])?;
    }
    let mut pendencia = None;
    let abriu = match conclusao {
        Conclusao::Aberto {
            conteudo,
            impressao,
        } => {
            texto::gravar_texto(&tx, hash, METODO_PDFTOTEXT, conteudo, Some(impressao))?;
            true
        }
        Conclusao::SemTexto => {
            texto::gravar_texto(&tx, hash, METODO_SEM_TEXTO, "", None)?;
            true
        }
        Conclusao::SemRegra { pergunta } => {
            pendencia = Some(abrir_pendencia(&tx, &referencia, TIPO_SEM_REGRA, pergunta)?);
            false
        }
        Conclusao::Pendencia { tipo, pergunta } => {
            pendencia = Some(abrir_pendencia(&tx, &referencia, tipo, pergunta)?);
            false
        }
        Conclusao::SoTentativas => false,
    };
    let falhas = contar_falhas(&tx, hash)?;
    if !abriu && pendencia.is_none() && falhas >= limite_de_falhas {
        let pergunta = pergunta_sem_regra(
            alvo.fornecedor.as_deref(),
            alvo.tipo_de_documento.as_deref(),
            falhas,
        );
        pendencia = Some(abrir_pendencia(
            &tx,
            &referencia,
            TIPO_SEM_REGRA,
            &pergunta,
        )?);
    }
    tx.commit()?;
    Ok(Registro { falhas, pendencia })
}
