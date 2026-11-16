//! O cache local do livro-caixa (`livro-caixa.sqlite`, spec 07): as coleções de
//! `GET /livro/*` com a `ETag` de cada uma, o `/estado`, a última lista de estações, o
//! histórico de trocas e a fila de alterações.
//!
//! - **Dono do esquema:** a estação, pela escada do `bancada-armazem`. O cache é
//!   descartável: banco de versão mais nova ou migração que falha é apagado e criado de novo.
//! - **Troca atômica:** as coleções que mudaram numa sincronização entram numa transação só
//!   ([`CacheDoLivro::trocar`]); a tela nunca vê metade de uma troca.
//! - **Leitura e escrita:** cada `CacheDoLivro` em arquivo tem uma conexão de escrita e
//!   uma de leitura (a do `Armazem`); a sincronização abre o seu, a janela o dela.
//! - **Nada secreto:** nenhuma credencial, token ou senha entra aqui.

use bancada_armazem::{Armazem, Erro as ErroDoArmazem, Escada, Migracao, Pragmas};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sbf_contrato::{
    Colecao, Cursor, DeltaDaColecao, Estacoes, Estado, ItemAlterado, LinhaAgora, LinhasDaColecao,
    PropostaDeAlteracao,
};
use sbf_inventario::alteracao::{Alteracao, Troca};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// O nome do arquivo do cache.
pub const ARQUIVO_DO_CACHE: &str = "livro-caixa.sqlite";

const MIGRACOES: &[Migracao] = &[
    Migracao::sql(
        "cache do livro-caixa, trocas e alteracoes",
        "CREATE TABLE meta (
        chave TEXT PRIMARY KEY,
        valor TEXT NOT NULL
    ) WITHOUT ROWID;
    CREATE TABLE colecao (
        nome        TEXT PRIMARY KEY,
        etag        TEXT,
        trocada_em  TEXT NOT NULL,
        linhas      INTEGER NOT NULL
    ) WITHOUT ROWID;
    CREATE TABLE linha (
        colecao TEXT NOT NULL,
        id      TEXT NOT NULL,
        versao  TEXT,
        dados   TEXT NOT NULL,
        PRIMARY KEY (colecao, id)
    ) WITHOUT ROWID;
    CREATE TABLE troca (
        id         INTEGER PRIMARY KEY,
        inicio     TEXT NOT NULL,
        fim        TEXT,
        colecoes   TEXT NOT NULL DEFAULT '[]',
        linhas     INTEGER NOT NULL DEFAULT 0,
        enviadas   INTEGER NOT NULL DEFAULT 0,
        aceitas    INTEGER NOT NULL DEFAULT 0,
        recusadas  INTEGER NOT NULL DEFAULT 0,
        erro       TEXT
    );
    CREATE TABLE alteracao (
        id                 TEXT PRIMARY KEY,
        estado             TEXT NOT NULL,
        feita_em           TEXT NOT NULL,
        alvo               TEXT NOT NULL,
        dados              TEXT NOT NULL,
        proposta           TEXT NOT NULL,
        no_servidor_agora  TEXT
    );
    CREATE INDEX alteracao_por_alvo ON alteracao (alvo);
    CREATE TABLE pedido_de_revogacao (
        estacao TEXT PRIMARY KEY
    ) WITHOUT ROWID;",
    ),
    Migracao::sql(
        "delta por cursor e fila de escritas da fatia 3",
        "ALTER TABLE colecao ADD COLUMN cursor TEXT;
    CREATE TABLE escrita (
        id         TEXT PRIMARY KEY,
        estado     TEXT NOT NULL,
        feita_em   TEXT NOT NULL,
        dados      TEXT NOT NULL
    );",
    ),
    Migracao::sql(
        "carga completa periodica do delta",
        "ALTER TABLE colecao ADD COLUMN completa_em_ms INTEGER;
    ALTER TABLE colecao ADD COLUMN deltas_seguidos INTEGER NOT NULL DEFAULT 0;",
    ),
];

/// Depois de quantas horas desde a última carga completa a estação pede a coleção inteira.
pub const HORAS_ATE_A_CARGA_COMPLETA: i64 = 24;

/// Depois de quantos deltas seguidos a estação pede a coleção inteira.
pub const SINCRONIZACOES_ATE_A_CARGA_COMPLETA: u32 = 50;

/// Erro do cache (texto do SQLite ou do JSON; nunca dado secreto).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cache do livro-caixa: {0}")]
pub struct ErroDoCache(pub String);

impl From<rusqlite::Error> for ErroDoCache {
    fn from(e: rusqlite::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<ErroDoArmazem> for ErroDoCache {
    fn from(e: ErroDoArmazem) -> Self {
        Self(e.to_string())
    }
}

impl From<serde_json::Error> for ErroDoCache {
    fn from(e: serde_json::Error) -> Self {
        Self(format!("JSON: {e}"))
    }
}

pub type ResultadoDoCache<T> = Result<T, ErroDoCache>;

/// Uma alteração como a fila guarda: a vista da janela, a proposta do contrato e, depois da
/// recusa, as linhas como estão no servidor.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistroDeAlteracao {
    pub alteracao: Alteracao,
    pub proposta: PropostaDeAlteracao,
    pub no_servidor_agora: Option<Vec<LinhaAgora>>,
}

/// Apaga o arquivo do cache e os do WAL. Apagar o que não existe não é erro.
///
/// # Errors
/// Falha de disco (por exemplo, no Windows, com o arquivo ainda aberto).
pub fn apagar_arquivos(caminho: &Path) -> std::io::Result<()> {
    for sufixo in ["", "-wal", "-shm", "-journal"] {
        let mut nome = caminho.as_os_str().to_owned();
        nome.push(sufixo);
        match std::fs::remove_file(PathBuf::from(nome)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn pragmas() -> Pragmas {
    Pragmas {
        // A sincronização e a janela no mesmo arquivo: espera longa (LICOES.md da fatia 1).
        espera_ocupado: Duration::from_secs(30),
        ..Pragmas::default()
    }
}

/// Linhas de uma coleção prontas para gravar: `(id, versão, JSON)`.
fn linhas_serializadas(
    linhas: &LinhasDaColecao,
) -> ResultadoDoCache<Vec<(String, Option<String>, String)>> {
    fn com_versao<T: Serialize>(
        v: &[T],
        id: impl Fn(&T) -> String,
        versao: impl Fn(&T) -> Option<String>,
    ) -> ResultadoDoCache<Vec<(String, Option<String>, String)>> {
        v.iter()
            .map(|l| Ok((id(l), versao(l), serde_json::to_string(l)?)))
            .collect()
    }
    macro_rules! comum {
        ($v:expr) => {
            com_versao($v, |l| l.id.to_string(), |l| Some(l.versao.0.clone()))
        };
    }
    match linhas {
        LinhasDaColecao::Contas(v) => comum!(v),
        LinhasDaColecao::Instituicoes(v) => comum!(v),
        LinhasDaColecao::Cartoes(v) => comum!(v),
        LinhasDaColecao::Faturas(v) => comum!(v),
        LinhasDaColecao::ItensDeFatura(v) => comum!(v),
        LinhasDaColecao::Movimentacoes(v) => comum!(v),
        LinhasDaColecao::Transferencias(v) => comum!(v),
        LinhasDaColecao::Rateios(v) => com_versao(v, |l| l.id.to_string(), |_| None),
        LinhasDaColecao::Categorias(v) => comum!(v),
        LinhasDaColecao::Tags(v) => comum!(v),
        LinhasDaColecao::Centros(v) => comum!(v),
        LinhasDaColecao::Fornecedores(v) => comum!(v),
        LinhasDaColecao::Apelidos(v) => comum!(v),
        LinhasDaColecao::Contratos(v) => comum!(v),
        LinhasDaColecao::Dividas(v) => comum!(v),
        LinhasDaColecao::Parcelas(v) => comum!(v),
        LinhasDaColecao::Recorrentes(v) => comum!(v),
        LinhasDaColecao::Ocorrencias(v) => comum!(v),
        LinhasDaColecao::Documentos(v) => com_versao(
            v,
            |l| l.id.to_string(),
            |l| l.versao.as_ref().map(|x| x.0.clone()),
        ),
        LinhasDaColecao::LotesDeRascunhos(v) => comum!(v),
        LinhasDaColecao::Rascunhos(v) => comum!(v),
    }
}

pub(crate) fn linha_agora(
    linha: &LinhaAgora,
) -> ResultadoDoCache<(Colecao, String, Option<String>, String)> {
    Ok(match linha {
        LinhaAgora::Movimentacoes(m) => (
            Colecao::Movimentacoes,
            m.id.to_string(),
            Some(m.versao.0.clone()),
            serde_json::to_string(m)?,
        ),
        LinhaAgora::Rateios(r) => (
            Colecao::Rateios,
            r.id.to_string(),
            None,
            serde_json::to_string(r)?,
        ),
        LinhaAgora::LotesDeRascunhos(l) => (
            Colecao::LotesDeRascunhos,
            l.id.to_string(),
            Some(l.versao.0.clone()),
            serde_json::to_string(l)?,
        ),
        LinhaAgora::Rascunhos(r) => (
            Colecao::Rascunhos,
            r.id.to_string(),
            Some(r.versao.0.clone()),
            serde_json::to_string(r)?,
        ),
    })
}

fn gravar_meta(tx: &Transaction<'_>, chave: &str, valor: &str) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO meta (chave, valor) VALUES (?1, ?2)
         ON CONFLICT (chave) DO UPDATE SET valor = excluded.valor",
        params![chave, valor],
    )?;
    Ok(())
}

fn subir_geracao(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO meta (chave, valor) VALUES ('geracao', '1')
         ON CONFLICT (chave) DO UPDATE SET valor = CAST(valor AS INTEGER) + 1",
        [],
    )?;
    Ok(())
}

/// O cache aberto.
#[derive(Debug)]
pub struct CacheDoLivro {
    pub(crate) armazem: Armazem,
}

impl CacheDoLivro {
    /// Abre (criando) o cache em `caminho`. Um banco que não migra (de versão mais nova,
    /// corrompido) é apagado e criado de novo: o cache é descartável.
    ///
    /// # Errors
    /// Falha de disco ao criar a pasta ou o banco novo.
    pub fn abrir(caminho: &Path) -> ResultadoDoCache<Self> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| ErroDoCache(e.to_string()))?;
        }
        match Self::abrir_e_migrar(caminho) {
            Ok(c) => Ok(c),
            Err(_) => {
                apagar_arquivos(caminho).map_err(|e| ErroDoCache(e.to_string()))?;
                Self::abrir_e_migrar(caminho)
            }
        }
    }

    fn abrir_e_migrar(caminho: &Path) -> ResultadoDoCache<Self> {
        let mut armazem = Armazem::abrir(caminho, &pragmas())?;
        armazem.migrar(&Escada::nova(MIGRACOES))?;
        Ok(Self { armazem })
    }

    /// Cache em memória (testes).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn em_memoria() -> ResultadoDoCache<Self> {
        let mut armazem = Armazem::em_memoria(&pragmas())?;
        armazem.migrar(&Escada::nova(MIGRACOES))?;
        Ok(Self { armazem })
    }

    pub fn caminho(&self) -> Option<&Path> {
        self.armazem.caminho()
    }

    /// A conexão de leitura (a de escrita, em memória).
    pub(crate) fn ler(&self) -> &Connection {
        self.armazem
            .leitura()
            .unwrap_or_else(|_| self.armazem.conexao())
    }

    // ---------------- meta ----------------

    fn meta(&self, chave: &str) -> ResultadoDoCache<Option<String>> {
        Ok(self
            .ler()
            .query_row("SELECT valor FROM meta WHERE chave = ?1", [chave], |l| {
                l.get(0)
            })
            .optional()?)
    }

    /// O último `/estado` do servidor.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn estado_do_servidor(&self) -> ResultadoDoCache<Option<Estado>> {
        self.meta("estado")?
            .map(|t| serde_json::from_str(&t).map_err(ErroDoCache::from))
            .transpose()
    }

    /// O instante (RFC 3339) da cópia que as áreas mostram: o fim da última troca que deu
    /// certo.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn copia_de(&self) -> ResultadoDoCache<Option<String>> {
        self.meta("copia_de")
    }

    /// Sobe a cada troca que muda alguma linha: quem guarda a cópia em memória a relê.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn geracao(&self) -> ResultadoDoCache<i64> {
        Ok(self
            .meta("geracao")?
            .and_then(|g| g.parse().ok())
            .unwrap_or(0))
    }

    /// As `ETag`s guardadas, por coleção.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn etags(&self) -> ResultadoDoCache<HashMap<Colecao, String>> {
        let mut st = self
            .ler()
            .prepare("SELECT nome, etag FROM colecao WHERE etag IS NOT NULL")?;
        let linhas = st.query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)))?;
        let mut mapa = HashMap::new();
        for linha in linhas {
            let (nome, etag) = linha?;
            if let Some(c) = Colecao::do_caminho(&nome) {
                mapa.insert(c, etag);
            }
        }
        Ok(mapa)
    }

    // ---------------- a troca ----------------

    /// Grava o `/estado`, substitui as coleções que mudaram e marca a cópia, **numa transação
    /// só**. Devolve quantas linhas chegaram. Se o `/estado` é de outra estação (pareamento
    /// novo), o que havia é apagado antes, na mesma transação.
    ///
    /// # Errors
    /// [`ErroDoCache`]; nada muda.
    pub fn trocar(
        &mut self,
        estado: &Estado,
        novas: &[(LinhasDaColecao, Option<String>)],
        copia_de: &str,
    ) -> ResultadoDoCache<usize> {
        let estado_json = serde_json::to_string(estado)?;
        let estacao = estado.estacao.id.to_string();
        let mut preparadas = Vec::with_capacity(novas.len());
        for (linhas, etag) in novas {
            preparadas.push((linhas.colecao(), linhas_serializadas(linhas)?, etag.clone()));
        }
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<usize> {
                let anterior: Option<String> = tx
                    .query_row("SELECT valor FROM meta WHERE chave = 'estacao'", [], |l| {
                        l.get(0)
                    })
                    .optional()?;
                if anterior.as_deref().is_some_and(|a| a != estacao) {
                    tx.execute_batch(
                        "DELETE FROM linha; DELETE FROM colecao; DELETE FROM alteracao; DELETE FROM escrita;
                     DELETE FROM pedido_de_revogacao; DELETE FROM meta WHERE chave <> 'geracao';",
                    )?;
                }
                gravar_meta(tx, "estacao", &estacao)?;
                gravar_meta(tx, "estado", &estado_json)?;
                let mut total = 0;
                for (colecao, linhas, etag) in &preparadas {
                    tx.execute("DELETE FROM linha WHERE colecao = ?1", [colecao.caminho()])?;
                    let mut st = tx.prepare_cached(
                        "INSERT INTO linha (colecao, id, versao, dados) VALUES (?1, ?2, ?3, ?4)",
                    )?;
                    for (id, versao, dados) in linhas {
                        st.execute(params![colecao.caminho(), id, versao, dados])?;
                    }
                    tx.execute(
                    "INSERT INTO colecao (nome, etag, trocada_em, linhas) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (nome) DO UPDATE SET etag = excluded.etag,
                        trocada_em = excluded.trocada_em, linhas = excluded.linhas",
                    params![
                        colecao.caminho(),
                        etag,
                        copia_de,
                        i64::try_from(linhas.len()).unwrap_or(i64::MAX)
                    ],
                )?;
                    total += linhas.len();
                }
                gravar_meta(tx, "copia_de", copia_de)?;
                if !preparadas.is_empty() {
                    subir_geracao(tx)?;
                }
                Ok(total)
            })
    }

    /// Grava as linhas que o servidor devolveu ao aceitar uma alteração (sem esperar a
    /// próxima sincronização). A `ETag` da coleção é esquecida: a próxima troca a baixa.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn gravar_linhas(&mut self, linhas: &[LinhaAgora]) -> ResultadoDoCache<()> {
        let preparadas: Vec<_> = linhas.iter().map(linha_agora).collect::<Result<_, _>>()?;
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                for (colecao, id, versao, dados) in &preparadas {
                    tx.execute(
                        "INSERT INTO linha (colecao, id, versao, dados) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (colecao, id) DO UPDATE SET versao = excluded.versao,
                        dados = excluded.dados",
                        params![colecao.caminho(), id, versao, dados],
                    )?;
                    tx.execute(
                        "UPDATE colecao SET etag = NULL WHERE nome = ?1",
                        [colecao.caminho()],
                    )?;
                }
                if !preparadas.is_empty() {
                    subir_geracao(tx)?;
                }
                Ok(())
            })
    }

    /// Apaga linhas do cache (a Ação apagou, ou o servidor diz que não existem mais).
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn apagar_linhas(&mut self, itens: &[ItemAlterado]) -> ResultadoDoCache<()> {
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                for i in itens {
                    tx.execute(
                        "DELETE FROM linha WHERE colecao = ?1 AND id = ?2",
                        params![i.colecao.caminho(), i.id.to_string()],
                    )?;
                }
                if !itens.is_empty() {
                    subir_geracao(tx)?;
                }
                Ok(())
            })
    }

    /// A versão (do servidor) de cada linha pedida, como o cache a tem. `None`: a linha não
    /// está no cache.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn versoes_das_linhas(
        &self,
        itens: &[ItemAlterado],
    ) -> ResultadoDoCache<Vec<(ItemAlterado, Option<String>)>> {
        let mut saida = Vec::with_capacity(itens.len());
        for i in itens {
            let v: Option<Option<String>> = self
                .ler()
                .query_row(
                    "SELECT versao FROM linha WHERE colecao = ?1 AND id = ?2",
                    params![i.colecao.caminho(), i.id.to_string()],
                    |l| l.get(0),
                )
                .optional()?;
            saida.push((i.clone(), v.flatten()));
        }
        Ok(saida)
    }

    /// Os cursores do delta, por coleção.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn cursores(&self) -> ResultadoDoCache<HashMap<Colecao, Cursor>> {
        let mut st = self
            .ler()
            .prepare("SELECT nome, cursor FROM colecao WHERE cursor IS NOT NULL")?;
        let linhas = st.query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)))?;
        let mut mapa = HashMap::new();
        for linha in linhas {
            let (nome, cursor) = linha?;
            if let Some(c) = Colecao::do_caminho(&nome) {
                mapa.insert(c, Cursor(cursor));
            }
        }
        Ok(mapa)
    }

    /// Os cursores que ainda valem para pedir um delta: a coleção que passou de
    /// [`HORAS_ATE_A_CARGA_COMPLETA`] desde a última carga completa, ou que já teve
    /// [`SINCRONIZACOES_ATE_A_CARGA_COMPLETA`] deltas seguidos, fica sem cursor (o servidor
    /// devolve a coleção inteira e cura o que uma transação longa tenha deixado de fora).
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn cursores_para_pedir(&self, agora_ms: i64) -> ResultadoDoCache<HashMap<Colecao, Cursor>> {
        let limite_ms = HORAS_ATE_A_CARGA_COMPLETA * 3_600_000;
        let mut st = self.ler().prepare(
            "SELECT nome, cursor FROM colecao WHERE cursor IS NOT NULL
             AND completa_em_ms IS NOT NULL AND ?1 - completa_em_ms < ?2 AND ?1 >= completa_em_ms
             AND deltas_seguidos < ?3",
        )?;
        let linhas = st.query_map(
            params![
                agora_ms,
                limite_ms,
                i64::from(SINCRONIZACOES_ATE_A_CARGA_COMPLETA)
            ],
            |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
        )?;
        let mut mapa = HashMap::new();
        for linha in linhas {
            let (nome, cursor) = linha?;
            if let Some(c) = Colecao::do_caminho(&nome) {
                mapa.insert(c, Cursor(cursor));
            }
        }
        Ok(mapa)
    }

    /// Como [`CacheDoLivro::trocar`], mas com deltas (contrato v2): cada coleção traz as
    /// linhas novas ou mudadas, os ids apagados e o cursor novo. `completo` troca a coleção
    /// inteira. Uma transação só.
    ///
    /// # Errors
    /// [`ErroDoCache`]; nada muda.
    pub fn trocar_com_deltas(
        &mut self,
        estado: &Estado,
        deltas: &[DeltaDaColecao],
        copia_de: &str,
        agora_ms: i64,
    ) -> ResultadoDoCache<usize> {
        let estado_json = serde_json::to_string(estado)?;
        let estacao = estado.estacao.id.to_string();
        let mut preparadas = Vec::with_capacity(deltas.len());
        for d in deltas {
            preparadas.push((
                d.linhas.colecao(),
                linhas_serializadas(&d.linhas)?,
                d.excluidas
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                d.completo,
                d.cursor.0.clone(),
            ));
        }
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<usize> {
                let anterior: Option<String> = tx
                    .query_row("SELECT valor FROM meta WHERE chave = 'estacao'", [], |l| {
                        l.get(0)
                    })
                    .optional()?;
                if anterior.as_deref().is_some_and(|a| a != estacao) {
                    tx.execute_batch(
                        "DELETE FROM linha; DELETE FROM colecao; DELETE FROM alteracao; DELETE FROM escrita;
                     DELETE FROM pedido_de_revogacao; DELETE FROM meta WHERE chave <> 'geracao';",
                    )?;
                }
                gravar_meta(tx, "estacao", &estacao)?;
                gravar_meta(tx, "estado", &estado_json)?;
                let mut total = 0;
                let mut mudou = false;
                for (colecao, linhas, excluidas, completo, cursor) in &preparadas {
                    let nome = colecao.caminho();
                    if *completo {
                        tx.execute("DELETE FROM linha WHERE colecao = ?1", [nome])?;
                    }
                    for id in excluidas {
                        tx.execute(
                            "DELETE FROM linha WHERE colecao = ?1 AND id = ?2",
                            params![nome, id],
                        )?;
                    }
                    let mut st = tx.prepare_cached(
                        "INSERT INTO linha (colecao, id, versao, dados) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (colecao, id) DO UPDATE SET versao = excluded.versao,
                            dados = excluded.dados",
                    )?;
                    for (id, versao, dados) in linhas {
                        st.execute(params![nome, id, versao, dados])?;
                    }
                    let quantas: i64 = tx.query_row(
                        "SELECT count(*) FROM linha WHERE colecao = ?1",
                        [nome],
                        |l| l.get(0),
                    )?;
                    tx.execute(
                        "INSERT INTO colecao (nome, etag, cursor, trocada_em, linhas, completa_em_ms, deltas_seguidos)
                         VALUES (?1, NULL, ?2, ?3, ?4, ?5, 0)
                         ON CONFLICT (nome) DO UPDATE SET etag = NULL, cursor = excluded.cursor,
                            trocada_em = excluded.trocada_em, linhas = excluded.linhas,
                            completa_em_ms = CASE WHEN ?6 THEN excluded.completa_em_ms
                                                  ELSE colecao.completa_em_ms END,
                            deltas_seguidos = CASE WHEN ?6 THEN 0 ELSE colecao.deltas_seguidos + 1 END",
                        params![nome, cursor, copia_de, quantas, agora_ms, *completo],
                    )?;
                    total += linhas.len() + excluidas.len();
                    mudou |= !linhas.is_empty() || !excluidas.is_empty() || *completo;
                }
                gravar_meta(tx, "copia_de", copia_de)?;
                if mudou {
                    subir_geracao(tx)?;
                }
                Ok(total)
            })
    }

    // ---------------- leitura das coleções ----------------

    /// Todas as linhas de uma coleção, na ordem do id.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn linhas<T: DeserializeOwned>(&self, colecao: Colecao) -> ResultadoDoCache<Vec<T>> {
        let mut st = self
            .ler()
            .prepare_cached("SELECT dados FROM linha WHERE colecao = ?1 ORDER BY id")?;
        let textos = st.query_map([colecao.caminho()], |l| l.get::<_, String>(0))?;
        let mut saida = Vec::new();
        for texto in textos {
            saida.push(serde_json::from_str(&texto?)?);
        }
        Ok(saida)
    }

    /// Uma linha pelo id.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn linha<T: DeserializeOwned>(
        &self,
        colecao: Colecao,
        id: &str,
    ) -> ResultadoDoCache<Option<T>> {
        let texto: Option<String> = self
            .ler()
            .query_row(
                "SELECT dados FROM linha WHERE colecao = ?1 AND id = ?2",
                params![colecao.caminho(), id],
                |l| l.get(0),
            )
            .optional()?;
        texto
            .map(|t| serde_json::from_str(&t).map_err(ErroDoCache::from))
            .transpose()
    }

    /// Quantas linhas há em cada coleção trocada.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn contagens(&self) -> ResultadoDoCache<Vec<(String, i64)>> {
        let mut st = self
            .ler()
            .prepare("SELECT nome, linhas FROM colecao ORDER BY nome")?;
        let linhas = st.query_map([], |l| Ok((l.get(0)?, l.get(1)?)))?;
        Ok(linhas.collect::<Result<_, _>>()?)
    }

    // ---------------- estações ----------------

    /// Guarda a última lista de estações.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn gravar_estacoes(&mut self, estacoes: &Estacoes) -> ResultadoDoCache<()> {
        let json = serde_json::to_string(estacoes)?;
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                Ok(gravar_meta(tx, "estacoes", &json)?)
            })
    }

    /// A última lista de estações.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn estacoes(&self) -> ResultadoDoCache<Option<Estacoes>> {
        self.meta("estacoes")?
            .map(|t| serde_json::from_str(&t).map_err(ErroDoCache::from))
            .transpose()
    }

    /// Pede a revogação de outra estação na próxima troca.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn pedir_revogacao(&mut self, estacao: &str) -> ResultadoDoCache<()> {
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                tx.execute(
                    "INSERT OR IGNORE INTO pedido_de_revogacao (estacao) VALUES (?1)",
                    [estacao],
                )?;
                Ok(())
            })
    }

    /// As revogações pedidas e ainda não enviadas.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn revogacoes_pedidas(&self) -> ResultadoDoCache<Vec<String>> {
        let mut st = self
            .ler()
            .prepare("SELECT estacao FROM pedido_de_revogacao")?;
        let linhas = st.query_map([], |l| l.get(0))?;
        Ok(linhas.collect::<Result<_, _>>()?)
    }

    /// A revogação foi enviada (ou não se aplica mais).
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn esquecer_revogacao(&mut self, estacao: &str) -> ResultadoDoCache<()> {
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                tx.execute(
                    "DELETE FROM pedido_de_revogacao WHERE estacao = ?1",
                    [estacao],
                )?;
                Ok(())
            })
    }

    // ---------------- histórico de trocas ----------------

    /// Começa o registro de uma troca; devolve o id.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn iniciar_troca(&mut self, inicio: &str) -> ResultadoDoCache<i64> {
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<i64> {
                tx.execute("INSERT INTO troca (inicio) VALUES (?1)", [inicio])?;
                Ok(tx.last_insert_rowid())
            })
    }

    /// Fecha o registro de uma troca.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn terminar_troca(&mut self, id: i64, troca: &Troca) -> ResultadoDoCache<()> {
        let colecoes = serde_json::to_string(&troca.colecoes)?;
        self.armazem
            .transacao_imediata(|tx| -> ResultadoDoCache<()> {
                tx.execute(
                    "UPDATE troca SET fim = ?2, colecoes = ?3, linhas = ?4, enviadas = ?5,
                    aceitas = ?6, recusadas = ?7, erro = ?8 WHERE id = ?1",
                    params![
                        id,
                        troca.fim,
                        colecoes,
                        troca.linhas,
                        troca.enviadas,
                        troca.aceitas,
                        troca.recusadas,
                        troca.erro
                    ],
                )?;
                // O histórico guarda as últimas 500 trocas.
                tx.execute(
                    "DELETE FROM troca WHERE id <= (SELECT max(id) - 500 FROM troca)",
                    [],
                )?;
                Ok(())
            })
    }

    /// As últimas `limite` trocas, da mais nova para a mais antiga.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn historico(&self, limite: usize) -> ResultadoDoCache<Vec<Troca>> {
        let mut st = self.ler().prepare(
            "SELECT inicio, fim, colecoes, linhas, enviadas, aceitas, recusadas, erro
             FROM troca ORDER BY id DESC LIMIT ?1",
        )?;
        let linhas = st.query_map([i64::try_from(limite).unwrap_or(i64::MAX)], |l| {
            Ok((
                Troca {
                    inicio: l.get(0)?,
                    fim: l.get(1)?,
                    colecoes: Vec::new(),
                    linhas: l.get(3)?,
                    enviadas: l.get(4)?,
                    aceitas: l.get(5)?,
                    recusadas: l.get(6)?,
                    erro: l.get(7)?,
                },
                l.get::<_, String>(2)?,
            ))
        })?;
        let mut saida = Vec::new();
        for linha in linhas {
            let (mut troca, colecoes) = linha?;
            troca.colecoes = serde_json::from_str(&colecoes)?;
            saida.push(troca);
        }
        Ok(saida)
    }

    // ---------------- a fila de alterações ----------------

    /// Grava (ou regrava) uma alteração.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn gravar_alteracao(&mut self, registro: &RegistroDeAlteracao) -> ResultadoDoCache<()> {
        self.gravar_alteracoes(std::slice::from_ref(registro))
    }

    /// Grava várias alterações numa transação só (Refazer muda duas).
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn gravar_alteracoes(&mut self, registros: &[RegistroDeAlteracao]) -> ResultadoDoCache<()> {
        let mut preparados = Vec::with_capacity(registros.len());
        for r in registros {
            preparados.push((
                r.alteracao.id.clone(),
                r.alteracao.estado.rotulo().to_owned(),
                r.alteracao.feita_em.clone(),
                r.alteracao.pedido.alvo().to_string(),
                serde_json::to_string(&r.alteracao)?,
                serde_json::to_string(&r.proposta)?,
                r.no_servidor_agora
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
            ));
        }
        self.armazem.transacao_imediata(|tx| -> ResultadoDoCache<()> {
            for p in &preparados {
                tx.execute(
                    "INSERT INTO alteracao (id, estado, feita_em, alvo, dados, proposta, no_servidor_agora)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (id) DO UPDATE SET estado = excluded.estado,
                        dados = excluded.dados, proposta = excluded.proposta,
                        no_servidor_agora = excluded.no_servidor_agora",
                    params![p.0, p.1, p.2, p.3, p.4, p.5, p.6],
                )?;
            }
            Ok(())
        })
    }

    /// Todas as alterações, da mais nova para a mais antiga.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn alteracoes(&self) -> ResultadoDoCache<Vec<RegistroDeAlteracao>> {
        // A ordem de inserção (rowid; regravar não muda): a mais nova primeiro.
        let mut st = self.ler().prepare(
            "SELECT dados, proposta, no_servidor_agora FROM alteracao ORDER BY rowid DESC",
        )?;
        let linhas = st.query_map([], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut saida = Vec::new();
        for linha in linhas {
            let (dados, proposta, agora) = linha?;
            saida.push(RegistroDeAlteracao {
                alteracao: serde_json::from_str(&dados)?,
                proposta: serde_json::from_str(&proposta)?,
                no_servidor_agora: agora.map(|a| serde_json::from_str(&a)).transpose()?,
            });
        }
        Ok(saida)
    }

    /// Uma alteração pelo id.
    ///
    /// # Errors
    /// [`ErroDoCache`].
    pub fn alteracao(&self, id: &str) -> ResultadoDoCache<Option<RegistroDeAlteracao>> {
        Ok(self
            .alteracoes()?
            .into_iter()
            .find(|r| r.alteracao.id == id))
    }
}
