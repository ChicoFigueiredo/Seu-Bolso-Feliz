//! O estado de uma conta de e-mail, para a área Conexões da janela.
//!
//! `estado_da_conta(cofre, conta, chaveiro)` só **lê**: as credenciais (só se existem, nunca o
//! valor) e o que o inventário do cofre sabe das coletas da conta. Não toca na rede.

use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension as _};
use sbf_chaveiro::Chaveiro;

use crate::config::{Conta, TipoDeEmail};
use crate::credenciais::{Alvo, Credenciais, faltando};
use crate::gravador::Destino;

/// Em que pé está a conexão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituacaoDaConexao {
    /// O tipo ainda não conecta (Outlook, em breve).
    EmBreve,
    /// Não há login (falta o refresh token) ou faltam o client id e o secret do aplicativo.
    NaoConectada,
    /// O provedor recusou o refresh token (`invalid_grant`): é preciso entrar de novo.
    AutorizacaoVencida,
    /// Credenciais presentes e nenhuma coleta ainda.
    Pronta,
    /// A última coleta terminou bem.
    Coletada,
    /// A última coleta falhou (ou interrompeu antes de terminar).
    ComFalha,
}

/// O estado de uma conta de e-mail como a área Conexões mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDaConexao {
    pub situacao: SituacaoDaConexao,
    /// O e-mail da conta, se já se sabe.
    pub email: Option<String>,
    /// As variáveis de credencial que faltam (só os nomes; detalhe técnico).
    pub faltando: Vec<String>,
    /// Quando a última coleta terminou bem (ISO 8601 UTC).
    pub ultima_coleta: Option<String>,
    /// Quantas origens esta conta tem no inventário (anexos e textos de e-mail).
    pub origens: u64,
    /// Quantos documentos distintos vieram desta conta.
    pub documentos: u64,
    /// A categoria da última falha, sem conteúdo.
    pub ultima_falha: Option<String>,
}

/// O estado de uma conta neste cofre. Só lê; não toca na rede.
#[must_use]
pub fn estado_da_conta(
    cofre: &Path,
    conta: &Conta,
    chaveiro: Option<&Chaveiro>,
) -> EstadoDaConexao {
    let alvo = Alvo {
        id: &conta.id,
        tipo: conta.tipo,
    };
    let faltando: Vec<String> = if conta.tipo == TipoDeEmail::Gmail {
        faltando(chaveiro, alvo)
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    let mut e = EstadoDaConexao {
        situacao: SituacaoDaConexao::NaoConectada,
        email: conta.conta.clone(),
        faltando,
        ultima_coleta: None,
        origens: 0,
        documentos: 0,
        ultima_falha: None,
    };
    let fonte = Destino::da_conta(&conta.id).fonte;
    let banco = cofre.join("inventario.sqlite");
    if let Ok(conn) = Connection::open_with_flags(&banco, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        let _ = ler(&conn, &fonte, &mut e);
    }
    let vencida = Credenciais::do_ambiente(chaveiro, alvo)
        .ok()
        .is_some_and(|c| c.vencida(chaveiro, &conta.id));
    e.situacao = if !conta.tipo.disponivel() {
        SituacaoDaConexao::EmBreve
    } else if !e.faltando.is_empty() {
        SituacaoDaConexao::NaoConectada
    } else if vencida {
        SituacaoDaConexao::AutorizacaoVencida
    } else if e.ultima_falha.is_some() {
        SituacaoDaConexao::ComFalha
    } else if e.ultima_coleta.is_some() {
        SituacaoDaConexao::Coletada
    } else {
        SituacaoDaConexao::Pronta
    };
    e
}

fn ler(conn: &Connection, fonte: &str, e: &mut EstadoDaConexao) -> rusqlite::Result<()> {
    e.origens = conn.query_row(
        "SELECT count(*) FROM origem WHERE fonte_id = ?1",
        [fonte],
        |l| l.get::<_, i64>(0),
    )? as u64;
    e.documentos = conn.query_row(
        "SELECT count(DISTINCT content_hash) FROM origem WHERE fonte_id = ?1",
        [fonte],
        |l| l.get::<_, i64>(0),
    )? as u64;
    // A última varredura fechada decide: boa → coletada; falhou/interrompida → com falha.
    let ultima: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT status, concluida_em FROM varredura
              WHERE fonte_id = ?1 AND status <> 'rodando' ORDER BY id DESC LIMIT 1",
            [fonte],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?;
    if let Some((status, quando)) = ultima {
        if status == "concluida" {
            e.ultima_coleta = quando;
        } else {
            e.ultima_falha = Some(match status.as_str() {
                "interrompida" => "a última coleta foi interrompida".to_owned(),
                _ => "a última coleta falhou".to_owned(),
            });
            e.ultima_coleta = conn
                .query_row(
                    "SELECT concluida_em FROM varredura
                      WHERE fonte_id = ?1 AND status = 'concluida' ORDER BY id DESC LIMIT 1",
                    [fonte],
                    |l| l.get(0),
                )
                .optional()?
                .flatten();
        }
    }
    Ok(())
}
