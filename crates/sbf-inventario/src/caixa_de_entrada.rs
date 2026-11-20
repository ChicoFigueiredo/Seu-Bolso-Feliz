//! A **Caixa de entrada** (CONTEXT.md): a fila de documentos recebidos e ainda não
//! resolvidos, independente de origem. Cada vez que o reconhecimento roda num arquivo (o
//! worker, o Telegram, o Gmail, a pasta vigiada, `sbf reconhecer`, arrastar na janela) ele
//! registra aqui uma linha por `content_hash`; a janela lista e o usuário resolve ou
//! dispensa.
//!
//! Mora no `inventario.sqlite` (migração 4) porque o conteúdo já é um achado dele e porque o
//! worker, a CLI e a janela já abrem esse banco; a fila não precisa de um segundo arquivo
//! nem de uma segunda conexão. **Nunca guarda conteúdo de documento**: só o nome do arquivo,
//! o tipo reconhecido e uma linha de resumo (o texto está em `texto`).
//!
//! **Canal não é Origem.** No glossário, Origem é cada lugar onde o conteúdo foi achado (um
//! documento tem várias). A coluna `canal` guarda só por onde ele chegou **da primeira vez**.
//!
//! Os estados são os do glossário. A **Caixa de entrada** lista os reconhecidos
//! ([`Estado::e_reconhecido`]: Pronto, Precisa revisar, Possível duplicidade); **A triar**
//! lista como "sem padrão de reconhecimento" os outros dois ([`Estado::sem_padrao`]: Não
//! reconhecido e Protegido por senha). Processando não está em nenhuma lista nem no contador
//! dos reconhecidos: é um indicador à parte ([`Contagens::processando`]). "Reconhecer de novo"
//! troca o estado, e o item muda de lista sozinho, sem reabrir o que o usuário decidiu.
//!
//! Resolver, dispensar e reabrir são **decisões do usuário no log** (`decisao`, alvo
//! `achado`, campo `caixa_entrada`): entram no Rastro e o Desfazer as desfaz como a qualquer
//! outra. A coluna `situacao` é a projeção delas.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension as _, Transaction, params};
use serde_json::{Value, json};

use crate::comando::{IdDecisao, ResultadoDeComando};
use crate::decisao::{self, Efeito};
use crate::esquema::MIGRACAO_4_CAIXA_DE_ENTRADA;
use crate::projecao::{AlvoTipo, campo};

/// A fonte que o reconhecimento cria quando copia um arquivo solto para o cofre (o mesmo
/// valor de `sbf_coleta::pastas::FONTE_ARRASTADO`; um teste do reconhecimento confere).
pub const FONTE_ARRASTADO: &str = "arrastado";

/// Por onde o documento chegou da primeira vez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Canal {
    Telegram,
    Gmail,
    /// O espelho do celular (a pasta vigiada cuja fonte é `celular`).
    Celular,
    Pasta,
    Arrastado,
    Cli,
}

impl Canal {
    pub const TODOS: [Canal; 6] = [
        Canal::Telegram,
        Canal::Gmail,
        Canal::Celular,
        Canal::Pasta,
        Canal::Arrastado,
        Canal::Cli,
    ];

    /// O valor gravado na coluna `canal`.
    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Canal::Telegram => "telegram",
            Canal::Gmail => "gmail",
            Canal::Celular => "celular",
            Canal::Pasta => "pasta",
            Canal::Arrastado => "arrastado",
            Canal::Cli => "cli",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Canal> {
        Canal::TODOS.into_iter().find(|o| o.codigo() == c)
    }

    /// O nome na interface.
    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Canal::Telegram => "Telegram",
            Canal::Gmail => "Gmail",
            Canal::Celular => "Celular",
            Canal::Pasta => "Pasta",
            Canal::Arrastado => "Arrastado",
            Canal::Cli => "Linha de comando",
        }
    }

    /// O canal de uma pasta vigiada, pela fonte do `worker.json`: o espelho do celular tem a
    /// fonte `celular`; as demais são pastas.
    #[must_use]
    pub fn da_fonte_da_pasta(fonte: &str) -> Canal {
        if fonte == "celular" {
            Canal::Celular
        } else {
            Canal::Pasta
        }
    }
}

/// Os estados do documento que o usuário vê.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Estado {
    Pronto,
    PrecisaRevisar,
    PossivelDuplicidade,
    ProtegidoPorSenha,
    NaoReconhecido,
    Processando,
}

impl Estado {
    pub const TODOS: [Estado; 6] = [
        Estado::Pronto,
        Estado::PrecisaRevisar,
        Estado::PossivelDuplicidade,
        Estado::ProtegidoPorSenha,
        Estado::NaoReconhecido,
        Estado::Processando,
    ];

    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Estado::Pronto => "pronto",
            Estado::PrecisaRevisar => "precisa_revisar",
            Estado::PossivelDuplicidade => "possivel_duplicidade",
            Estado::ProtegidoPorSenha => "protegido",
            Estado::NaoReconhecido => "nao_reconhecido",
            Estado::Processando => "processando",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Estado> {
        Estado::TODOS.into_iter().find(|e| e.codigo() == c)
    }

    /// O nome do glossário.
    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Estado::Pronto => "Pronto",
            Estado::PrecisaRevisar => "Precisa revisar",
            Estado::PossivelDuplicidade => "Possível duplicidade",
            Estado::ProtegidoPorSenha => "Protegido por senha",
            Estado::NaoReconhecido => "Não reconhecido",
            Estado::Processando => "Processando",
        }
    }

    /// Algum leitor ou regra casou (ou a leitura falhou e pede revisão): o documento é da
    /// Caixa de entrada.
    #[must_use]
    pub fn e_reconhecido(self) -> bool {
        matches!(
            self,
            Estado::Pronto | Estado::PrecisaRevisar | Estado::PossivelDuplicidade
        )
    }

    /// Nenhum padrão casou: o documento é de A triar ("sem padrão de reconhecimento").
    #[must_use]
    pub fn sem_padrao(self) -> bool {
        matches!(self, Estado::ProtegidoPorSenha | Estado::NaoReconhecido)
    }

    /// O reconhecimento ainda não terminou.
    #[must_use]
    pub fn e_processando(self) -> bool {
        self == Estado::Processando
    }
}

/// Os códigos dos estados que passam em `quais`, prontos para um `IN (...)` do SQL: a única
/// fonte da verdade das listas é o enum.
fn estados_em_sql(quais: impl Fn(Estado) -> bool) -> String {
    Estado::TODOS
        .into_iter()
        .filter(|e| quais(*e))
        .map(|e| format!("'{}'", e.codigo()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// O que o usuário decidiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    Aberta,
    /// "Resolver": aplicou o que queria.
    Resolvida,
    /// "Dispensar": não é nada.
    Dispensada,
}

impl Situacao {
    pub const TODAS: [Situacao; 3] = [Situacao::Aberta, Situacao::Resolvida, Situacao::Dispensada];

    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Situacao::Aberta => "aberta",
            Situacao::Resolvida => "resolvida",
            Situacao::Dispensada => "dispensada",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Situacao> {
        Situacao::TODAS.into_iter().find(|s| s.codigo() == c)
    }
}

/// Uma linha da fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrada {
    pub content_hash: String,
    pub nome: String,
    /// `AAAA-MM-DDTHH:MM:SS.mmmZ` (UTC).
    pub chegou_em: String,
    pub atualizado_em: String,
    /// Por onde chegou da primeira vez.
    pub canal: Canal,
    pub tipo: Option<String>,
    pub resumo: Option<String>,
    pub estado: Estado,
    pub situacao: Situacao,
    pub vezes: i64,
}

/// O que o reconhecimento concluiu sobre um conteúdo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leitura {
    pub content_hash: String,
    pub nome: String,
    pub tipo: Option<String>,
    pub resumo: Option<String>,
    pub estado: Estado,
}

/// Como o conteúdo chegou, para quem o registra pela primeira vez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chegada {
    pub canal: Canal,
    /// Reabre o que o usuário já tinha resolvido ou dispensado: só uma ação explícita dele
    /// (arrastar de novo, `sbf reconhecer`) faz isso; a chegada automática de um conteúdo
    /// repetido não.
    pub reabrir: bool,
}

/// Quais linhas listar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grupo {
    /// Os reconhecidos (Pronto, Precisa revisar, Possível duplicidade): a Caixa de entrada.
    Reconhecidos,
    /// Os sem padrão (Não reconhecido, Protegido por senha): A triar.
    SemPadrao,
    Todos,
}

/// O filtro da lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filtro {
    pub grupo: Grupo,
    /// `true`: só o que o usuário ainda não resolveu nem dispensou.
    pub so_abertas: bool,
    pub canal: Option<Canal>,
    pub limite: u32,
}

impl Filtro {
    /// O padrão da Caixa de entrada: reconhecidos ainda abertos.
    #[must_use]
    pub fn caixa() -> Self {
        Self {
            grupo: Grupo::Reconhecidos,
            so_abertas: true,
            canal: None,
            limite: 500,
        }
    }

    /// O padrão da seção "Sem padrão de reconhecimento" de A triar.
    #[must_use]
    pub fn sem_padrao() -> Self {
        Self {
            grupo: Grupo::SemPadrao,
            so_abertas: true,
            canal: None,
            limite: 500,
        }
    }
}

/// Os contadores (só o que está aberto).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contagens {
    /// Reconhecidos ainda não resolvidos (o contador da Caixa de entrada).
    pub reconhecidos: i64,
    /// Sem padrão ainda não resolvidos (o "N sem padrão" de A triar).
    pub sem_padrao: i64,
    /// O reconhecimento ainda roda: indicador à parte, fora das duas listas.
    pub processando: i64,
}

/// Cria a tabela quando ainda não existe (o banco não passou pela migração 4). Idempotente.
///
/// # Errors
/// Falha do SQLite.
pub fn garantir_tabela(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(MIGRACAO_4_CAIXA_DE_ENTRADA)
}

/// Quantas origens o conteúdo tem, fora a fonte `arrastado` (que o próprio reconhecimento
/// cria ao copiar um arquivo solto para o cofre). Serve para o estado Possível duplicidade:
/// o mesmo conteúdo já achado em outro lugar.
///
/// # Errors
/// Falha do SQLite.
pub fn origens_de_fora(conn: &Connection, sha256: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM origem WHERE content_hash = ?1 AND fonte_id <> ?2",
        params![sha256, FONTE_ARRASTADO],
        |l| l.get(0),
    )
}

/// Marca o conteúdo como Processando (cria a linha se for a primeira vez; nas seguintes só
/// troca o estado, e o tipo e o resumo anteriores seguem até o fim). Não conta como uma
/// rodada: quem conta é [`registrar`].
///
/// # Errors
/// Falha do SQLite.
pub fn marcar_processando(
    conn: &Connection,
    sha256: &str,
    nome: &str,
    canal: Canal,
) -> rusqlite::Result<()> {
    garantir_tabela(conn)?;
    conn.execute(
        "INSERT INTO caixa_entrada (content_hash, nome, canal, estado, vezes)
         VALUES (?1, ?2, ?3, 'processando', 0)
         ON CONFLICT(content_hash) DO UPDATE SET
           estado = 'processando',
           atualizado_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![sha256, nome, canal.codigo()],
    )?;
    Ok(())
}

/// Registra (ou atualiza) o resultado do reconhecimento. O canal e a chegada são os da
/// primeira vez. A situação só volta a Aberta quando `chegada.reabrir`.
///
/// # Errors
/// Falha do SQLite.
pub fn registrar(conn: &Connection, l: &Leitura, chegada: &Chegada) -> rusqlite::Result<()> {
    garantir_tabela(conn)?;
    conn.execute(
        "INSERT INTO caixa_entrada (content_hash, nome, canal, tipo, resumo, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(content_hash) DO UPDATE SET
           nome = excluded.nome,
           tipo = excluded.tipo,
           resumo = excluded.resumo,
           estado = excluded.estado,
           atualizado_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
           vezes = vezes + 1,
           situacao = CASE WHEN ?7 THEN 'aberta' ELSE situacao END,
           resolvida_em = CASE WHEN ?7 THEN NULL ELSE resolvida_em END",
        params![
            l.content_hash,
            l.nome,
            chegada.canal.codigo(),
            l.tipo,
            l.resumo,
            l.estado.codigo(),
            chegada.reabrir,
        ],
    )?;
    Ok(())
}

/// "Reconhecer de novo": atualiza a linha que já existe com o que o reconhecimento achou
/// agora, sem criar linha e **sem mexer na situação** (não reabre o que o usuário decidiu).
/// `false` quando a linha não existe.
///
/// # Errors
/// Falha do SQLite.
pub fn atualizar(conn: &Connection, l: &Leitura) -> rusqlite::Result<bool> {
    garantir_tabela(conn)?;
    let n = conn.execute(
        "UPDATE caixa_entrada
            SET nome = ?2, tipo = ?3, resumo = ?4, estado = ?5,
                atualizado_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                vezes = vezes + 1
          WHERE content_hash = ?1",
        params![l.content_hash, l.nome, l.tipo, l.resumo, l.estado.codigo()],
    )?;
    Ok(n > 0)
}

fn conversao(coluna: usize, motivo: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(coluna, rusqlite::types::Type::Text, motivo.into())
}

/// A situação de um conteúdo; `None` quando não está na Caixa de entrada.
///
/// # Errors
/// Falha do SQLite, ou uma situação gravada que este código não conhece.
pub fn situacao_de(conn: &Connection, sha256: &str) -> rusqlite::Result<Option<Situacao>> {
    garantir_tabela(conn)?;
    let texto: Option<String> = conn
        .query_row(
            "SELECT situacao FROM caixa_entrada WHERE content_hash = ?1",
            [sha256],
            |l| l.get(0),
        )
        .optional()?;
    texto
        .map(|t| {
            Situacao::de_codigo(&t)
                .ok_or_else(|| conversao(0, format!("situação desconhecida: {t}")))
        })
        .transpose()
}

/// Resolver, dispensar ou reabrir como **decisão do usuário** no log (Rastro, Desfazer): grava
/// a decisão e a projeção na mesma transação. Devolve o id da decisão.
///
/// # Errors
/// O item não está na Caixa de entrada, ou falha do banco.
pub fn decidir(
    conn: &Connection,
    sha256: &str,
    situacao: Situacao,
) -> ResultadoDeComando<IdDecisao> {
    garantir_tabela(conn)?;
    let tx = conn.unchecked_transaction()?;
    if situacao_de(&tx, sha256)?.is_none() {
        return Err(crate::projecao::erro_de_banco(
            "o item não está na Caixa de entrada",
        ));
    }
    let em = decisao::agora(&tx)?;
    let depois = json!({"situacao": situacao.codigo()});
    let efeito = Efeito::novo(AlvoTipo::Achado, sha256, campo::CAIXA_ENTRADA, depois);
    let id = decisao::gravar(&tx, &efeito, None, None, &em)?;
    tx.commit()?;
    Ok(id)
}

/// O valor atual do campo `caixa_entrada` do achado, na forma do log (o `antes`).
pub(crate) fn forma_atual(tx: &Transaction<'_>, sha256: &str) -> ResultadoDeComando<Value> {
    let atual = situacao_de(tx, sha256)?.unwrap_or(Situacao::Aberta);
    Ok(json!({"situacao": atual.codigo()}))
}

/// Leva a projeção ao valor do log (decisão, desfazer ou reprojeção). Sem linha, nada a
/// projetar.
pub(crate) fn aplicar_forma(
    tx: &Transaction<'_>,
    sha256: &str,
    valor: Option<&Value>,
    em: &str,
) -> ResultadoDeComando<()> {
    let texto = valor
        .and_then(|v| v.get("situacao"))
        .and_then(Value::as_str)
        .ok_or_else(|| crate::projecao::erro_de_banco("decisão da caixa sem `situacao`"))?;
    let situacao = Situacao::de_codigo(texto)
        .ok_or_else(|| crate::projecao::erro_de_banco(format!("situação desconhecida: {texto}")))?;
    garantir_tabela(tx)?;
    tx.execute(
        "UPDATE caixa_entrada
            SET situacao = ?2,
                resolvida_em = CASE WHEN ?2 = 'aberta' THEN NULL ELSE ?3 END
          WHERE content_hash = ?1",
        params![sha256, situacao.codigo(), em],
    )?;
    Ok(())
}

fn de_linha(l: &rusqlite::Row<'_>) -> rusqlite::Result<Entrada> {
    let canal: String = l.get(4)?;
    let estado: String = l.get(7)?;
    let situacao: String = l.get(8)?;
    Ok(Entrada {
        content_hash: l.get(0)?,
        nome: l.get(1)?,
        chegou_em: l.get(2)?,
        atualizado_em: l.get(3)?,
        canal: Canal::de_codigo(&canal)
            .ok_or_else(|| conversao(4, format!("canal desconhecido: {canal}")))?,
        tipo: l.get(5)?,
        resumo: l.get(6)?,
        estado: Estado::de_codigo(&estado)
            .ok_or_else(|| conversao(7, format!("estado desconhecido: {estado}")))?,
        situacao: Situacao::de_codigo(&situacao)
            .ok_or_else(|| conversao(8, format!("situação desconhecida: {situacao}")))?,
        vezes: l.get(9)?,
    })
}

const COLUNAS: &str = "content_hash, nome, chegou_em, atualizado_em, canal, tipo, resumo, \
                       estado, situacao, vezes";

/// Uma linha da fila.
///
/// # Errors
/// Falha do SQLite, ou um código gravado que este código não conhece.
pub fn obter(conn: &Connection, sha256: &str) -> rusqlite::Result<Option<Entrada>> {
    garantir_tabela(conn)?;
    conn.query_row(
        &format!("SELECT {COLUNAS} FROM caixa_entrada WHERE content_hash = ?1"),
        [sha256],
        de_linha,
    )
    .optional()
}

/// A lista, das mais recentes para as mais antigas.
///
/// # Errors
/// Falha do SQLite, ou um código gravado que este código não conhece.
pub fn listar(conn: &Connection, filtro: &Filtro) -> rusqlite::Result<Vec<Entrada>> {
    garantir_tabela(conn)?;
    let grupo = match filtro.grupo {
        Grupo::Reconhecidos => format!("estado IN ({})", estados_em_sql(Estado::e_reconhecido)),
        Grupo::SemPadrao => format!("estado IN ({})", estados_em_sql(Estado::sem_padrao)),
        Grupo::Todos => "1 = 1".to_owned(),
    };
    let abertas = if filtro.so_abertas {
        "situacao = 'aberta'"
    } else {
        "1 = 1"
    };
    let sql = format!(
        "SELECT {COLUNAS} FROM caixa_entrada
          WHERE {grupo} AND {abertas} AND (?1 IS NULL OR canal = ?1)
          ORDER BY chegou_em DESC, content_hash LIMIT ?2"
    );
    let mut q = conn.prepare(&sql)?;
    q.query_map(
        params![filtro.canal.map(Canal::codigo), filtro.limite],
        de_linha,
    )?
    .collect()
}

/// Os contadores das listas (só o que está aberto).
///
/// # Errors
/// Falha do SQLite.
pub fn contagens(conn: &Connection) -> rusqlite::Result<Contagens> {
    garantir_tabela(conn)?;
    let sql = format!(
        "SELECT
           COALESCE(SUM(estado IN ({})), 0),
           COALESCE(SUM(estado IN ({})), 0),
           COALESCE(SUM(estado IN ({})), 0)
         FROM caixa_entrada WHERE situacao = 'aberta'",
        estados_em_sql(Estado::e_reconhecido),
        estados_em_sql(Estado::sem_padrao),
        estados_em_sql(Estado::e_processando),
    );
    conn.query_row(&sql, [], |l| {
        Ok(Contagens {
            reconhecidos: l.get(0)?,
            sem_padrao: l.get(1)?,
            processando: l.get(2)?,
        })
    })
}

/// O caminho do arquivo no acervo (relativo ao cofre) do conteúdo, quando há cópia.
///
/// # Errors
/// Falha do SQLite.
pub fn caminho_no_acervo(conn: &Connection, sha256: &str) -> rusqlite::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
            [sha256],
            |l| l.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// O sha256 do conteúdo da origem local mais recente com este caminho.
///
/// # Errors
/// Falha do SQLite.
pub fn sha_da_origem_local(conn: &Connection, caminho: &Path) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT content_hash FROM origem
          WHERE local_filepath = ?1 AND content_hash IS NOT NULL
          ORDER BY id DESC LIMIT 1",
        [caminho.to_string_lossy()],
        |l| l.get::<_, String>(0),
    )
    .optional()
}

/// Um achado novo de uma pasta vigiada que o reconhecimento ainda não viu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchadoPendente {
    pub sha256: String,
    pub caminho: String,
}

/// Os achados **novos** da fonte (vistos a partir da criação da Caixa de entrada neste banco
/// **e** com o arquivo de até um dia antes dela em diante) que ainda não têm linha nela: o que
/// a reconciliação do worker indexou enquanto o vigia estava parado. Nunca o acervo antigo,
/// nem o arquivo antigo que só agora foi indexado (a primeira varredura de uma pasta nova).
///
/// # Errors
/// Falha do SQLite.
pub fn pendentes_de_reconhecer(
    conn: &Connection,
    fonte: &str,
    limite: u32,
) -> rusqlite::Result<Vec<AchadoPendente>> {
    garantir_tabela(conn)?;
    let mut q = conn.prepare(
        "SELECT a.content_hash, o.local_filepath, MAX(o.id)
           FROM achado a JOIN origem o ON o.content_hash = a.content_hash
          WHERE o.fonte_id = ?1 AND o.local_filepath IS NOT NULL
            AND a.primeiro_visto_em >=
                (SELECT valor FROM meta WHERE chave = 'caixa_entrada_desde')
            -- Indexado agora não é chegar agora: a primeira varredura de uma pasta nova (o
            -- espelho do celular) indexa anos de arquivos. Só conta o arquivo cuja data é de
            -- perto da criação da Caixa em diante (um dia de folga para relógio e sincronia).
            AND o.local_mtime >= strftime('%Y-%m-%dT%H:%M:%fZ',
                (SELECT valor FROM meta WHERE chave = 'caixa_entrada_desde'), '-1 day')
            AND NOT EXISTS (SELECT 1 FROM caixa_entrada c WHERE c.content_hash = a.content_hash)
          GROUP BY a.content_hash
          ORDER BY a.primeiro_visto_em
          LIMIT ?2",
    )?;
    q.query_map(params![fonte, limite], |l| {
        Ok(AchadoPendente {
            sha256: l.get(0)?,
            caminho: l.get(1)?,
        })
    })?
    .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().expect("memória");
        c.execute_batch(crate::esquema::MIGRACAO_1_BASE_BUN)
            .expect("base");
        c
    }

    fn leitura(sha: &str, estado: Estado) -> Leitura {
        Leitura {
            content_hash: sha.to_owned(),
            nome: format!("{sha}.pdf"),
            tipo: Some("Boleto".to_owned()),
            resumo: Some("Boleto · Fornecedor Exemplo".to_owned()),
            estado,
        }
    }

    fn chegada(canal: Canal) -> Chegada {
        Chegada {
            canal,
            reabrir: false,
        }
    }

    /// Um banco que também tem o log de decisões (migração 2) e os triggers de só acréscimo.
    fn inventario_completo() -> (tempfile::TempDir, crate::banco::Inventario) {
        let pasta = tempfile::tempdir().expect("pasta");
        let mut inv =
            crate::banco::Inventario::abrir(pasta.path().join("i.sqlite")).expect("abrir");
        inv.migrar(pasta.path().join("copia.sqlite"))
            .expect("migrar");
        (pasta, inv)
    }

    #[test]
    fn registra_lista_e_conta_por_grupo() {
        let c = conn();
        registrar(&c, &leitura("a", Estado::Pronto), &chegada(Canal::Telegram)).expect("a");
        registrar(
            &c,
            &leitura("b", Estado::PrecisaRevisar),
            &chegada(Canal::Gmail),
        )
        .expect("b");
        registrar(
            &c,
            &leitura("c", Estado::NaoReconhecido),
            &chegada(Canal::Pasta),
        )
        .expect("c");
        registrar(
            &c,
            &leitura("d", Estado::ProtegidoPorSenha),
            &chegada(Canal::Pasta),
        )
        .expect("d");
        marcar_processando(&c, "e", "e.pdf", Canal::Celular).expect("e");
        assert_eq!(listar(&c, &Filtro::caixa()).expect("caixa").len(), 2);
        assert_eq!(listar(&c, &Filtro::sem_padrao()).expect("sem").len(), 2);
        assert_eq!(
            contagens(&c).expect("contagens"),
            Contagens {
                reconhecidos: 2,
                sem_padrao: 2,
                processando: 1
            }
        );
        let so_gmail = listar(
            &c,
            &Filtro {
                canal: Some(Canal::Gmail),
                ..Filtro::caixa()
            },
        )
        .expect("gmail");
        assert_eq!(so_gmail.len(), 1);
        assert_eq!(so_gmail[0].content_hash, "b");
    }

    #[test]
    fn processando_nao_entra_em_lista_nenhuma_e_o_contador_concorda_com_a_lista() {
        let c = conn();
        marcar_processando(&c, "a", "a.pdf", Canal::Cli).expect("a");
        for f in [Filtro::caixa(), Filtro::sem_padrao()] {
            assert!(listar(&c, &f).expect("lista").is_empty());
        }
        let n = contagens(&c).expect("contagens");
        assert_eq!((n.reconhecidos, n.sem_padrao, n.processando), (0, 0, 1));
    }

    #[test]
    fn idempotente_guarda_o_primeiro_canal_e_conta_as_vezes() {
        let c = conn();
        registrar(
            &c,
            &leitura("a", Estado::NaoReconhecido),
            &chegada(Canal::Telegram),
        )
        .expect("1");
        let mut l = leitura("a", Estado::Pronto);
        l.resumo = Some("novo".to_owned());
        registrar(&c, &l, &chegada(Canal::Gmail)).expect("2");
        let e = obter(&c, "a").expect("obter").expect("existe");
        assert_eq!(e.canal, Canal::Telegram, "o canal é o da chegada");
        assert_eq!(e.estado, Estado::Pronto);
        assert_eq!(e.vezes, 2);
        assert!(listar(&c, &Filtro::sem_padrao()).expect("lista").is_empty());
    }

    #[test]
    fn so_a_chegada_que_pede_reabre_e_atualizar_nunca_reabre() {
        let (_p, inv) = inventario_completo();
        let c = inv.conexao();
        registrar(c, &leitura("a", Estado::Pronto), &chegada(Canal::Telegram)).expect("a");
        decidir(c, "a", Situacao::Resolvida).expect("resolver");
        assert!(listar(c, &Filtro::caixa()).expect("lista").is_empty());
        registrar(
            c,
            &leitura("a", Estado::PossivelDuplicidade),
            &chegada(Canal::Gmail),
        )
        .expect("automática");
        assert!(listar(c, &Filtro::caixa()).expect("lista").is_empty());
        // Reconhecer de novo só muda o estado (e, com ele, de lista).
        assert!(atualizar(c, &leitura("a", Estado::NaoReconhecido)).expect("atualizar"));
        assert_eq!(
            situacao_de(c, "a").expect("situação"),
            Some(Situacao::Resolvida)
        );
        assert!(!atualizar(c, &leitura("inexistente", Estado::Pronto)).expect("nada"));
        // Arrastar de novo é decisão do usuário: reabre.
        let reabre = Chegada {
            canal: Canal::Arrastado,
            reabrir: true,
        };
        registrar(c, &leitura("a", Estado::Pronto), &reabre).expect("arrastar");
        assert_eq!(listar(c, &Filtro::caixa()).expect("lista").len(), 1);
    }

    #[test]
    fn decidir_grava_no_log_e_o_desfazer_restaura() {
        use crate::comando::Comandos as _;
        let (_p, mut inv) = inventario_completo();
        registrar(
            inv.conexao(),
            &leitura("a", Estado::Pronto),
            &chegada(Canal::Cli),
        )
        .expect("a");
        let id = decidir(inv.conexao(), "a", Situacao::Dispensada).expect("dispensar");
        let d = decisao::ler_decisao(inv.conexao(), id)
            .expect("ler")
            .expect("existe");
        assert_eq!(
            (d.alvo_tipo.as_str(), d.campo.as_str()),
            ("achado", "caixa_entrada")
        );
        assert_eq!(d.autor, "usuario");
        assert_eq!(d.antes, Some(json!({"situacao": "aberta"})));
        assert_eq!(
            situacao_de(inv.conexao(), "a").expect("situação"),
            Some(Situacao::Dispensada)
        );
        // O Desfazer de sempre devolve o item à Caixa de entrada.
        inv.desfazer(id).expect("desfazer");
        assert_eq!(
            situacao_de(inv.conexao(), "a").expect("situação"),
            Some(Situacao::Aberta)
        );
        assert!(decidir(inv.conexao(), "inexistente", Situacao::Resolvida).is_err());
    }

    #[test]
    fn decidir_nao_conta_como_decisao_de_triagem_do_achado() {
        let (_p, inv) = inventario_completo();
        let c = inv.conexao();
        c.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES ('a', 1, 'A')",
            [],
        )
        .expect("achado");
        registrar(c, &leitura("a", Estado::Pronto), &chegada(Canal::Cli)).expect("a");
        decidir(c, "a", Situacao::Resolvida).expect("resolver");
        let por: Option<String> = c
            .query_row(
                "SELECT decidido_por FROM achado WHERE content_hash = 'a'",
                [],
                |l| l.get(0),
            )
            .expect("decidido_por");
        assert_ne!(
            por.as_deref(),
            Some("usuario"),
            "o estado do achado não mudou"
        );
    }

    #[test]
    fn a_tabela_nasce_sozinha_num_banco_sem_a_migracao() {
        let c = conn();
        registrar(&c, &leitura("a", Estado::Pronto), &chegada(Canal::Pasta)).expect("a");
        garantir_tabela(&c).expect("de novo");
        assert_eq!(contagens(&c).expect("contagens").reconhecidos, 1);
    }

    #[test]
    fn codigo_desconhecido_e_erro_nao_um_valor_qualquer() {
        let c = conn();
        registrar(&c, &leitura("a", Estado::Pronto), &chegada(Canal::Pasta)).expect("a");
        assert_eq!(Canal::de_codigo("fax"), None);
        assert_eq!(Estado::de_codigo("sumiu"), None);
        assert_eq!(Situacao::de_codigo("talvez"), None);
        // Um banco com um canal que esta build não conhece não vira "Cli" em silêncio.
        c.execute_batch("PRAGMA ignore_check_constraints = ON")
            .expect("pragma");
        c.execute("UPDATE caixa_entrada SET canal = 'fax'", [])
            .expect("update");
        assert!(listar(&c, &Filtro::caixa()).is_err());
    }

    #[test]
    fn as_listas_do_sql_saem_do_enum() {
        assert_eq!(
            estados_em_sql(Estado::e_reconhecido),
            "'pronto', 'precisa_revisar', 'possivel_duplicidade'"
        );
        assert_eq!(
            estados_em_sql(Estado::sem_padrao),
            "'protegido', 'nao_reconhecido'"
        );
        // Todo estado está em exatamente um dos três grupos.
        for e in Estado::TODOS {
            let n = [e.e_reconhecido(), e.sem_padrao(), e.e_processando()]
                .into_iter()
                .filter(|x| *x)
                .count();
            assert_eq!(n, 1, "{e:?}");
        }
    }

    #[test]
    fn os_rotulos_sao_os_do_glossario() {
        let r: Vec<&str> = Estado::TODOS.iter().map(|e| e.rotulo()).collect();
        assert_eq!(
            r,
            [
                "Pronto",
                "Precisa revisar",
                "Possível duplicidade",
                "Protegido por senha",
                "Não reconhecido",
                "Processando"
            ]
        );
        for e in Estado::TODOS {
            assert_eq!(Estado::de_codigo(e.codigo()), Some(e));
        }
    }

    #[test]
    fn a_pasta_do_celular_e_o_canal_celular() {
        assert_eq!(Canal::da_fonte_da_pasta("celular"), Canal::Celular);
        assert_eq!(Canal::da_fonte_da_pasta("disco:e2"), Canal::Pasta);
    }

    #[test]
    fn so_o_achado_novo_sem_linha_e_pendente() {
        let c = conn();
        c.execute(
            "INSERT INTO fonte (id, tipo, descricao, raiz, escopo)
             VALUES ('disco:t', 'disco', 't', '/x', 'dentro')",
            [],
        )
        .expect("fonte");
        c.execute(
            "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel, parametros)
             VALUES (1, 'disco:t', 'E1', '/x', 't', '{}')",
            [],
        )
        .expect("varredura");
        let achado_em = |sha: &str, visto: &str, caminho: &str, mtime: &str| {
            c.execute(
                "INSERT INTO achado (content_hash, file_size_bytes, camada, primeiro_visto_em)
                 VALUES (?1, 1, 'A', ?2)",
                params![sha, visto],
            )
            .expect("achado");
            c.execute(
                "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type,
                   origin_key, local_filepath, local_mtime, filename)
                 VALUES (?1, 'disco:t', 1, 'local_file', ?1, ?2, ?3, 'x')",
                params![sha, caminho, mtime],
            )
            .expect("origem");
        };
        let achado = |sha: &str, visto: &str, caminho: &str| {
            achado_em(sha, visto, caminho, visto);
        };
        achado("antigo", "2000-01-01T00:00:00.000Z", "/x/antigo.pdf");
        garantir_tabela(&c).expect("tabela");
        achado("novo", "2999-01-01T00:00:00.000Z", "/x/novo.pdf");
        achado("novo2", "2999-01-02T00:00:00.000Z", "/x/novo2.pdf");
        // Indexado agora, mas o arquivo é de anos atrás (primeira varredura do celular).
        achado_em(
            "velho-indexado-agora",
            "2999-01-03T00:00:00.000Z",
            "/x/velho.jpg",
            "2020-05-01T10:00:00.000Z",
        );
        registrar(
            &c,
            &leitura("novo2", Estado::Pronto),
            &chegada(Canal::Pasta),
        )
        .expect("novo2");
        let p = pendentes_de_reconhecer(&c, "disco:t", 10).expect("pendentes");
        assert_eq!(
            p,
            vec![AchadoPendente {
                sha256: "novo".to_owned(),
                caminho: "/x/novo.pdf".to_owned()
            }]
        );
        assert!(
            pendentes_de_reconhecer(&c, "outra", 10)
                .expect("outra")
                .is_empty()
        );
        assert_eq!(
            sha_da_origem_local(&c, Path::new("/x/novo.pdf")).expect("sha"),
            Some("novo".to_owned())
        );
    }
}
