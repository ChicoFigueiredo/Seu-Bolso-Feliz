//! O catálogo de regras: `regras.sqlite` (spec 13, D3).
//!
//! Guarda as regras, as **versões imutáveis** (definição + script + checksum; o conteúdo
//! nunca muda, só o estado), o histórico de estados, os exemplos, as execuções (sem o texto
//! do documento), os retornos do usuário, os parâmetros versionados e as propostas.
//!
//! Ciclo de uma versão: `rascunho` → (`ativar`) `ativa` → (`consolidar`) `consolidada`;
//! `desativada` quando sai de cena. Só [`Quem::Usuario`] ativa, consolida, desativa e
//! reverte (D14). A única ativação sem usuário é a de [`Catalogo::semear_fabrica`], que
//! reproduz o comportamento que o sbf já tinha (D10).

use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};

use crate::definicao::{Definicao, parse_competencia};
use crate::motor::Motor;
use crate::script::ExecutorDeScript;
use crate::{
    Autor, DeOnde, Decisao, Entrada, ErroDasRegras, Estado, IdDaRegra, LIMIAR_DE_CONSOLIDACAO,
    Leitura, Quem, Reconhecimento, ResultadoDaExecucao, Retorno, Valor, fabrica,
};

type Resultado<T> = Result<T, ErroDasRegras>;

fn banco(e: impl Display) -> ErroDasRegras {
    ErroDasRegras::Banco(e.to_string())
}

// ---------------------------------------------------------------------------------------
// Tipos
// ---------------------------------------------------------------------------------------

/// O que o editor, a correção ou a fábrica entregam para virar uma versão-rascunho.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovaVersao {
    pub regra: IdDaRegra,
    /// Nome para mostrar.
    pub nome: String,
    pub definicao: Definicao,
    pub script: Option<String>,
    pub autor: Autor,
    pub motivo: String,
}

/// Uma versão gravada. O conteúdo (`definicao`, `script`, `checksum`) é imutável; o resto é
/// o estado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersaoDeRegra {
    pub regra: IdDaRegra,
    pub versao: u32,
    pub definicao: Definicao,
    pub script: Option<String>,
    pub checksum: String,
    pub autor: Autor,
    pub motivo: String,
    pub criada_em: String,
    pub estado: Estado,
    /// Propostas aceitas sem correção em sequência (D4).
    pub aceitas_seguidas: u32,
    pub ativada_em: Option<String>,
    pub consolidada_em: Option<String>,
    pub desativada_em: Option<String>,
}

impl VersaoDeRegra {
    /// Uma versão fora do banco (testes, simulação de um rascunho ainda não salvo).
    ///
    /// # Errors
    /// [`ErroDasRegras::Definicao`] se a definição é inválida.
    pub fn avulsa(nova: NovaVersao, versao: u32, estado: Estado) -> Resultado<Self> {
        nova.definicao.validar()?;
        let checksum = checksum_da_versao(&nova.definicao, nova.script.as_deref());
        Ok(Self {
            regra: nova.regra,
            versao,
            definicao: nova.definicao,
            script: nova.script,
            checksum,
            autor: nova.autor,
            motivo: nova.motivo,
            criada_em: agora(),
            estado,
            aceitas_seguidas: 0,
            ativada_em: None,
            consolidada_em: None,
            desativada_em: None,
        })
    }
}

/// Uma linha da lista de regras (D12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoDaRegra {
    pub id: IdDaRegra,
    pub nome: String,
    pub tipo_documental: String,
    /// A versão ativa ou consolidada, se há.
    pub versao_em_vigor: Option<u32>,
    pub estado: Option<Estado>,
    pub versoes: u32,
    pub execucoes: u64,
    pub retornos_aceitos: u64,
    /// Aceito + corrigido + rejeitado.
    pub retornos_total: u64,
}

impl ResumoDaRegra {
    /// Aceitos sobre o total de retornos; `None` sem retornos.
    #[allow(clippy::cast_precision_loss)]
    pub fn taxa_de_acerto(&self) -> Option<f64> {
        (self.retornos_total > 0).then(|| self.retornos_aceitos as f64 / self.retornos_total as f64)
    }
}

/// Um parâmetro versionado (tabela do IRRF, faixas), vigente por competência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parametro {
    pub nome: String,
    pub versao: u32,
    /// `AAAA-MM`.
    pub vigencia_inicio: String,
    /// `AAAA-MM`; `None` é aberto.
    pub vigencia_fim: Option<String>,
    pub valor: Json,
    /// De onde veio (a norma, a URL oficial); para a conferência.
    pub fonte: String,
    pub checksum: String,
    pub criado_em: String,
}

impl Parametro {
    /// Vale na competência `AAAA-MM`?
    pub fn vale_em(&self, competencia: &str) -> bool {
        self.vigencia_inicio.as_str() <= competencia
            && self
                .vigencia_fim
                .as_deref()
                .is_none_or(|fim| competencia <= fim)
    }
}

/// O parâmetro de maior versão que vale na competência.
pub(crate) fn parametro_vigente<'a>(
    parametros: &'a [Parametro],
    nome: &str,
    competencia: &str,
) -> Option<&'a Parametro> {
    parametros
        .iter()
        .filter(|p| p.nome == nome && p.vale_em(competencia))
        .max_by_key(|p| p.versao)
}

/// O que um exemplo espera do motor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "esperado", rename_all = "snake_case")]
pub enum Esperado {
    /// A regra não deve se aplicar a esta entrada.
    NaoSeAplica,
    /// A regra reconhece com este tipo e estes campos (igualdade; campos a mais no
    /// resultado não falham).
    Reconhece {
        tipo_documental: String,
        campos: BTreeMap<String, Valor>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExemploDeRegra {
    pub id: i64,
    pub regra: IdDaRegra,
    pub nome: String,
    pub entrada: Entrada,
    pub esperado: Esperado,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultadoDoExemplo {
    pub nome: String,
    pub passou: bool,
    /// Vazio quando passou; senão, o que divergiu.
    pub detalhe: String,
}

/// O resultado de rodar os exemplos de uma versão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatorioDosExemplos {
    pub regra: IdDaRegra,
    pub versao: u32,
    pub resultados: Vec<ResultadoDoExemplo>,
}

impl RelatorioDosExemplos {
    pub fn aprovado(&self) -> bool {
        self.resultados.iter().all(|r| r.passou)
    }

    /// Os nomes dos exemplos que falharam.
    pub fn falhas(&self) -> Vec<String> {
        self.resultados
            .iter()
            .filter(|r| !r.passou)
            .map(|r| r.nome.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaProposta {
    Aberta,
    Aceita,
    Recusada,
    Desfeita,
}

/// Uma proposta gravada (idempotente pela chave, AC-LUA-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropostaGravada {
    pub chave: String,
    pub entrada: String,
    pub regra: IdDaRegra,
    pub versao: u32,
    pub conteudo: Reconhecimento,
    pub estado: EstadoDaProposta,
    pub criada_em: String,
    pub decidida_em: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------------------

/// JSON com as chaves ordenadas em todos os níveis: a serialização é estável mesmo que
/// alguma dependência ligue `preserve_order` no `serde_json`.
pub(crate) fn json_canonico(v: &Json) -> String {
    fn ordenar(v: &Json) -> Json {
        match v {
            Json::Object(m) => {
                let ordenado: BTreeMap<&String, Json> =
                    m.iter().map(|(k, v)| (k, ordenar(v))).collect();
                Json::Object(ordenado.into_iter().map(|(k, v)| (k.clone(), v)).collect())
            }
            Json::Array(a) => Json::Array(a.iter().map(ordenar).collect()),
            outro => outro.clone(),
        }
    }
    ordenar(v).to_string()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// O sha256 hex do JSON canônico da definição, `\n`, e o script (vazio se não há).
pub fn checksum_da_versao(definicao: &Definicao, script: Option<&str>) -> String {
    let json = serde_json::to_value(definicao).unwrap_or(Json::Null);
    let mut base = json_canonico(&json);
    base.push('\n');
    base.push_str(script.unwrap_or(""));
    sha256_hex(base.as_bytes())
}

/// Agora, em UTC, `AAAA-MM-DDTHH:MM:SSZ`.
pub(crate) fn agora() -> String {
    let seg = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dias = i64::try_from(seg / 86_400).unwrap_or(0);
    let resto = seg % 86_400;
    // Dias desde 1970-01-01 para data civil (algoritmo de Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
}

fn para_texto<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn de_texto<T: serde::de::DeserializeOwned>(s: &str) -> Resultado<T> {
    serde_json::from_value(Json::String(s.to_owned())).map_err(banco)
}

fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

// ---------------------------------------------------------------------------------------
// Esquema
// ---------------------------------------------------------------------------------------

const ESQUEMA_1: &str = "
CREATE TABLE regra (
    id TEXT PRIMARY KEY,
    nome TEXT NOT NULL,
    tipo_documental TEXT NOT NULL,
    criada_em TEXT NOT NULL
);
CREATE TABLE versao_de_regra (
    regra TEXT NOT NULL REFERENCES regra(id),
    versao INTEGER NOT NULL,
    definicao TEXT NOT NULL,
    script TEXT,
    checksum TEXT NOT NULL,
    autor TEXT NOT NULL,
    motivo TEXT NOT NULL,
    criada_em TEXT NOT NULL,
    estado TEXT NOT NULL,
    aceitas_seguidas INTEGER NOT NULL DEFAULT 0,
    ativada_em TEXT,
    consolidada_em TEXT,
    desativada_em TEXT,
    PRIMARY KEY (regra, versao)
);
CREATE TRIGGER versao_de_regra_imutavel BEFORE UPDATE ON versao_de_regra
WHEN NEW.definicao IS NOT OLD.definicao OR NEW.script IS NOT OLD.script
  OR NEW.checksum IS NOT OLD.checksum OR NEW.regra IS NOT OLD.regra
  OR NEW.versao IS NOT OLD.versao OR NEW.autor IS NOT OLD.autor
BEGIN SELECT RAISE(ABORT, 'o conteúdo de uma versão de regra é imutável'); END;
CREATE TRIGGER versao_de_regra_sem_apagar BEFORE DELETE ON versao_de_regra
BEGIN SELECT RAISE(ABORT, 'versão de regra não se apaga'); END;
CREATE TABLE historico_de_estado (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    regra TEXT NOT NULL,
    versao INTEGER NOT NULL,
    de TEXT,
    para TEXT NOT NULL,
    quem TEXT NOT NULL,
    motivo TEXT NOT NULL,
    em TEXT NOT NULL
);
CREATE TABLE exemplo_de_regra (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    regra TEXT NOT NULL REFERENCES regra(id),
    nome TEXT NOT NULL,
    entrada TEXT NOT NULL,
    esperado TEXT NOT NULL,
    criado_em TEXT NOT NULL,
    atualizado_em TEXT NOT NULL,
    UNIQUE (regra, nome)
);
CREATE TABLE historico_de_exemplo (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    regra TEXT NOT NULL,
    nome TEXT NOT NULL,
    acao TEXT NOT NULL,
    entrada TEXT NOT NULL,
    esperado TEXT NOT NULL,
    em TEXT NOT NULL
);
CREATE TABLE execucao (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entrada TEXT NOT NULL,
    regra TEXT NOT NULL,
    versao INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    chave TEXT NOT NULL UNIQUE,
    resultado TEXT NOT NULL,
    motivo TEXT,
    duracao_us INTEGER NOT NULL,
    em TEXT NOT NULL
);
CREATE INDEX execucao_por_regra ON execucao (regra, versao);
CREATE TABLE retorno (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chave TEXT NOT NULL,
    regra TEXT NOT NULL,
    versao INTEGER NOT NULL,
    retorno TEXT NOT NULL,
    quem TEXT NOT NULL,
    motivo TEXT NOT NULL,
    em TEXT NOT NULL
);
CREATE INDEX retorno_por_regra ON retorno (regra, versao);
CREATE TABLE parametro (
    nome TEXT NOT NULL,
    versao INTEGER NOT NULL,
    vigencia_inicio TEXT NOT NULL,
    vigencia_fim TEXT,
    valor TEXT NOT NULL,
    fonte TEXT NOT NULL,
    checksum TEXT NOT NULL,
    criado_em TEXT NOT NULL,
    PRIMARY KEY (nome, versao)
);
CREATE TRIGGER parametro_imutavel BEFORE UPDATE ON parametro
BEGIN SELECT RAISE(ABORT, 'parâmetro versionado é imutável'); END;
CREATE TRIGGER parametro_sem_apagar BEFORE DELETE ON parametro
BEGIN SELECT RAISE(ABORT, 'parâmetro versionado não se apaga'); END;
CREATE TABLE proposta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chave TEXT NOT NULL UNIQUE,
    entrada TEXT NOT NULL,
    regra TEXT NOT NULL,
    versao INTEGER NOT NULL,
    conteudo TEXT NOT NULL,
    estado TEXT NOT NULL,
    criada_em TEXT NOT NULL,
    decidida_em TEXT
);
";

/// A marcação das listas e a passada de fundo (spec 15, D3 e D4). Tudo com `IF NOT EXISTS`.
///
/// - `decisao_da_entrada`: a última decisão do conjunto de padrões sobre a entrada ser
///   Ambíguo. A tabela `execucao` sozinha não distingue o âmbar: as candidatas de um empate
///   e as de uma decisão clara têm o mesmo `resultado` (`reconhecido`), e o motivo é texto.
/// - `passada_de_fundo`: o ponto de parada (rowid de `texto` no inventário) de cada versão,
///   chaveado pelo checksum: versão nova ou alterada começa do zero, as outras seguem.
/// - O índice por entrada serve à consulta em lote da janela.
/// - Recupera o âmbar do que já estava gravado: um empate deixou o motivo
///   `regras empatadas ... discordam em ...` nas execuções das candidatas.
const ESQUEMA_2: &str = "
CREATE INDEX IF NOT EXISTS execucao_por_entrada ON execucao (entrada, regra);
CREATE TABLE IF NOT EXISTS decisao_da_entrada (
    entrada TEXT PRIMARY KEY,
    ambiguo INTEGER NOT NULL,
    em TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS passada_de_fundo (
    regra TEXT NOT NULL,
    versao INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    ponto INTEGER NOT NULL DEFAULT 0,
    feitos INTEGER NOT NULL DEFAULT 0,
    atualizada_em TEXT NOT NULL,
    PRIMARY KEY (regra, versao, checksum)
);
INSERT OR IGNORE INTO decisao_da_entrada (entrada, ambiguo, em)
SELECT entrada, 1, MAX(em) FROM execucao
 WHERE resultado = 'reconhecido' AND motivo LIKE 'regras empatadas%discordam em%'
 GROUP BY entrada;
";

/// As migrações, em ordem; `PRAGMA user_version` guarda quantas já rodaram.
const MIGRACOES: &[&str] = &[ESQUEMA_1, ESQUEMA_2];

const COLUNAS_DA_VERSAO: &str = "regra, versao, definicao, script, checksum, autor, motivo, \
     criada_em, estado, aceitas_seguidas, ativada_em, consolidada_em, desativada_em";

fn linha_da_versao(r: &rusqlite::Row<'_>) -> rusqlite::Result<VersaoDeRegra> {
    let conv = |i: usize, e: Box<dyn std::error::Error + Send + Sync>| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e)
    };
    let def: String = r.get(2)?;
    let autor: String = r.get(5)?;
    let estado: String = r.get(8)?;
    Ok(VersaoDeRegra {
        regra: r.get(0)?,
        versao: r.get(1)?,
        definicao: serde_json::from_str(&def).map_err(|e| conv(2, Box::new(e)))?,
        script: r.get(3)?,
        checksum: r.get(4)?,
        autor: serde_json::from_value(Json::String(autor)).map_err(|e| conv(5, Box::new(e)))?,
        motivo: r.get(6)?,
        criada_em: r.get(7)?,
        estado: serde_json::from_value(Json::String(estado)).map_err(|e| conv(8, Box::new(e)))?,
        aceitas_seguidas: r.get(9)?,
        ativada_em: r.get(10)?,
        consolidada_em: r.get(11)?,
        desativada_em: r.get(12)?,
    })
}

fn carregar_versao(conn: &Connection, regra: &str, versao: u32) -> Resultado<VersaoDeRegra> {
    conn.query_row(
        &format!(
            "SELECT {COLUNAS_DA_VERSAO} FROM versao_de_regra WHERE regra = ?1 AND versao = ?2"
        ),
        params![regra, versao],
        linha_da_versao,
    )
    .optional()
    .map_err(banco)?
    .ok_or_else(|| ErroDasRegras::NaoEncontrada(format!("regra `{regra}` versão {versao}")))
}

fn versao_vigente(conn: &Connection, regra: &str) -> Resultado<Option<VersaoDeRegra>> {
    conn.query_row(
        &format!(
            "SELECT {COLUNAS_DA_VERSAO} FROM versao_de_regra
             WHERE regra = ?1 AND estado IN ('ativa', 'consolidada')
             ORDER BY versao DESC LIMIT 1"
        ),
        params![regra],
        linha_da_versao,
    )
    .optional()
    .map_err(banco)
}

fn registrar_historico(
    conn: &Connection,
    regra: &str,
    versao: u32,
    de: Option<Estado>,
    para: Estado,
    quem: &str,
    motivo: &str,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO historico_de_estado (regra, versao, de, para, quem, motivo, em)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            regra,
            versao,
            de.map(|e| para_texto(&e)),
            para_texto(&para),
            quem,
            motivo,
            agora()
        ],
    )
    .map_err(banco)?;
    Ok(())
}

/// Troca o estado de uma versão e grava o histórico. `aceitas_seguidas` zera ao ativar.
fn trocar_estado(
    conn: &Connection,
    v: &VersaoDeRegra,
    para: Estado,
    quem: &str,
    motivo: &str,
) -> Resultado<()> {
    let agora = agora();
    let sql = match para {
        Estado::Ativa => {
            "UPDATE versao_de_regra SET estado = ?3, ativada_em = ?4, desativada_em = NULL,
             consolidada_em = NULL, aceitas_seguidas = 0 WHERE regra = ?1 AND versao = ?2"
        }
        Estado::Consolidada => {
            "UPDATE versao_de_regra SET estado = ?3, consolidada_em = ?4
             WHERE regra = ?1 AND versao = ?2"
        }
        Estado::Desativada => {
            "UPDATE versao_de_regra SET estado = ?3, desativada_em = ?4
             WHERE regra = ?1 AND versao = ?2"
        }
        Estado::Rascunho => {
            "UPDATE versao_de_regra SET estado = ?3 WHERE regra = ?1 AND versao = ?2 AND ?4 = ?4"
        }
    };
    conn.execute(sql, params![v.regra, v.versao, para_texto(&para), agora])
        .map_err(banco)?;
    registrar_historico(conn, &v.regra, v.versao, Some(v.estado), para, quem, motivo)
}

fn contagem(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<u64> {
    r.get::<_, i64>(i).map(|n| u64::try_from(n).unwrap_or(0))
}

fn quem_texto(q: &Quem) -> &'static str {
    match q {
        Quem::Usuario => "usuario",
        Quem::Ia => "ia",
        Quem::Mcp => "mcp",
        Quem::Sistema => "sistema",
    }
}

/// Quem decide o ciclo de vida: o usuário e, desde a D15, o MCP.
fn exigir_decisor(quem: &Quem, acao: &str) -> Resultado<()> {
    if matches!(quem, Quem::Usuario | Quem::Mcp) {
        Ok(())
    } else {
        Err(ErroDasRegras::NaoPermitido(format!(
            "só o usuário (ou o MCP) pode {acao} uma regra"
        )))
    }
}

/// "pelo usuário" ou "pelo mcp", para o motivo gravado no histórico.
fn pelo(quem: &Quem) -> &'static str {
    match quem {
        Quem::Mcp => "pelo mcp",
        _ => "pelo usuário",
    }
}

// ---------------------------------------------------------------------------------------
// O catálogo
// ---------------------------------------------------------------------------------------

/// O `regras.sqlite`.
#[derive(Debug)]
pub struct Catalogo {
    pub(crate) conn: Connection,
}

impl Catalogo {
    /// Abre (e cria, e migra) o catálogo em `caminho`, em modo WAL.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn abrir(caminho: &Path) -> Resultado<Self> {
        let conn = Connection::open(caminho).map_err(banco)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))
            .map_err(banco)?;
        Self::iniciar(conn)
    }

    /// Um catálogo só na memória (testes).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn em_memoria() -> Resultado<Self> {
        Self::iniciar(Connection::open_in_memory().map_err(banco)?)
    }

    fn iniciar(conn: Connection) -> Resultado<Self> {
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(banco)?;
        let atual: usize = conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .map_err(banco)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))?;
        if atual > MIGRACOES.len() {
            return Err(ErroDasRegras::Banco(format!(
                "regras.sqlite é de uma versão mais nova ({atual}) que este sbf ({})",
                MIGRACOES.len()
            )));
        }
        for (i, sql) in MIGRACOES.iter().enumerate().skip(atual) {
            conn.execute_batch(&format!(
                "BEGIN; {sql} PRAGMA user_version = {}; COMMIT;",
                i + 1
            ))
            .map_err(banco)?;
        }
        Ok(Self { conn })
    }

    /// A versão do esquema gravada (`PRAGMA user_version`).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn versao_do_esquema(&self) -> Resultado<u32> {
        self.conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(banco)
    }

    // ---- versões ----

    /// Salva um rascunho novo (cria a regra se preciso; versão = maior + 1; estado
    /// rascunho). Salvar o mesmo conteúdo (mesmo checksum) da última versão devolve essa
    /// versão, sem criar outra.
    ///
    /// O checksum é o sha256 hex do JSON canônico da definição, `\n` e o script.
    ///
    /// # Errors
    /// [`ErroDasRegras::Definicao`] se a definição ou o id são inválidos;
    /// [`ErroDasRegras::Banco`].
    pub fn salvar_rascunho(&mut self, nova: NovaVersao) -> Resultado<VersaoDeRegra> {
        if !id_valido(&nova.regra) {
            return Err(ErroDasRegras::Definicao(format!(
                "id de regra inválido `{}` (use minúsculas, dígitos e hífen)",
                nova.regra
            )));
        }
        nova.definicao.validar()?;
        let checksum = checksum_da_versao(&nova.definicao, nova.script.as_deref());
        let tx = self.conn.transaction().map_err(banco)?;
        let ultima: Option<(u32, String)> = tx
            .query_row(
                "SELECT versao, checksum FROM versao_de_regra WHERE regra = ?1
                 ORDER BY versao DESC LIMIT 1",
                params![nova.regra],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(banco)?;
        if let Some((v, c)) = &ultima
            && *c == checksum
        {
            return carregar_versao(&tx, &nova.regra, *v);
        }
        let versao = ultima.map_or(1, |(v, _)| v + 1);
        let agora = agora();
        tx.execute(
            "INSERT INTO regra (id, nome, tipo_documental, criada_em) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET nome = excluded.nome,
                 tipo_documental = excluded.tipo_documental",
            params![nova.regra, nova.nome, nova.definicao.tipo_documental, agora],
        )
        .map_err(banco)?;
        let json = serde_json::to_value(&nova.definicao).map_err(banco)?;
        tx.execute(
            "INSERT INTO versao_de_regra (regra, versao, definicao, script, checksum, autor,
                 motivo, criada_em, estado) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'rascunho')",
            params![
                nova.regra,
                versao,
                json_canonico(&json),
                nova.script,
                checksum,
                para_texto(&nova.autor),
                nova.motivo,
                agora
            ],
        )
        .map_err(banco)?;
        registrar_historico(
            &tx,
            &nova.regra,
            versao,
            None,
            Estado::Rascunho,
            &para_texto(&nova.autor),
            &nova.motivo,
        )?;
        let v = carregar_versao(&tx, &nova.regra, versao)?;
        tx.commit().map_err(banco)?;
        Ok(v)
    }

    /// Todas as versões de uma regra, da primeira à última.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn versoes(&self, regra: &str) -> Resultado<Vec<VersaoDeRegra>> {
        let mut st = self
            .conn
            .prepare(&format!(
                "SELECT {COLUNAS_DA_VERSAO} FROM versao_de_regra WHERE regra = ?1 ORDER BY versao"
            ))
            .map_err(banco)?;
        st.query_map(params![regra], linha_da_versao)
            .map_err(banco)?
            .collect::<Result<_, _>>()
            .map_err(banco)
    }

    /// Uma versão.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoEncontrada`].
    pub fn versao(&self, regra: &str, versao: u32) -> Resultado<VersaoDeRegra> {
        carregar_versao(&self.conn, regra, versao)
    }

    /// A lista de regras com a versão em vigor, execuções e taxa de acerto.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn regras(&self) -> Resultado<Vec<ResumoDaRegra>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT r.id, r.nome, r.tipo_documental,
                    (SELECT COUNT(*) FROM versao_de_regra v WHERE v.regra = r.id),
                    (SELECT COUNT(*) FROM execucao e WHERE e.regra = r.id),
                    (SELECT COUNT(*) FROM retorno t WHERE t.regra = r.id AND t.retorno = 'aceito'),
                    (SELECT COUNT(*) FROM retorno t WHERE t.regra = r.id
                        AND t.retorno IN ('aceito', 'corrigido', 'rejeitado')),
                    (SELECT versao FROM versao_de_regra v WHERE v.regra = r.id
                        AND v.estado IN ('ativa', 'consolidada') ORDER BY versao DESC LIMIT 1),
                    (SELECT estado FROM versao_de_regra v WHERE v.regra = r.id
                        AND v.estado IN ('ativa', 'consolidada') ORDER BY versao DESC LIMIT 1)
                 FROM regra r ORDER BY r.id",
            )
            .map_err(banco)?;
        let linhas = st
            .query_map([], |r| {
                Ok((
                    ResumoDaRegra {
                        id: r.get(0)?,
                        nome: r.get(1)?,
                        tipo_documental: r.get(2)?,
                        versoes: r.get(3)?,
                        execucoes: contagem(r, 4)?,
                        retornos_aceitos: contagem(r, 5)?,
                        retornos_total: contagem(r, 6)?,
                        versao_em_vigor: r.get(7)?,
                        estado: None,
                    },
                    r.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(banco)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(banco)?;
        linhas
            .into_iter()
            .map(|(mut resumo, estado)| {
                resumo.estado = estado.as_deref().map(de_texto).transpose()?;
                Ok(resumo)
            })
            .collect()
    }

    /// As versões ativas e consolidadas (as que o motor usa), uma por regra.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn vigentes(&self) -> Resultado<Vec<VersaoDeRegra>> {
        let mut st = self
            .conn
            .prepare(&format!(
                "SELECT {COLUNAS_DA_VERSAO} FROM versao_de_regra
                 WHERE estado IN ('ativa', 'consolidada') ORDER BY regra, versao"
            ))
            .map_err(banco)?;
        st.query_map([], linha_da_versao)
            .map_err(banco)?
            .collect::<Result<_, _>>()
            .map_err(banco)
    }

    /// O histórico de estados de uma versão: (de, para, quem, motivo, em).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    #[allow(clippy::type_complexity)]
    pub fn historico_de_estado(
        &self,
        regra: &str,
        versao: u32,
    ) -> Resultado<Vec<(Option<String>, String, String, String, String)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT de, para, quem, motivo, em FROM historico_de_estado
                 WHERE regra = ?1 AND versao = ?2 ORDER BY id",
            )
            .map_err(banco)?;
        st.query_map(params![regra, versao], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(banco)?
        .collect::<Result<_, _>>()
        .map_err(banco)
    }

    // ---- ciclo de vida ----

    /// Roda os exemplos da regra com um [`Motor`] só desta versão.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoEncontrada`], [`ErroDasRegras::Banco`].
    pub fn testar(
        &self,
        regra: &str,
        versao: u32,
        executor: Option<&dyn ExecutorDeScript>,
    ) -> Resultado<RelatorioDosExemplos> {
        let v = self.versao(regra, versao)?;
        let motor = Motor::novo(vec![v], self.parametros()?, None);
        let mut resultados = Vec::new();
        for ex in self.exemplos(regra)? {
            let leitura = motor.ler_com(&ex.entrada, executor);
            let detalhe = conferir(&ex.esperado, &leitura);
            resultados.push(ResultadoDoExemplo {
                nome: ex.nome,
                passou: detalhe.is_none(),
                detalhe: detalhe.unwrap_or_default(),
            });
        }
        Ok(RelatorioDosExemplos {
            regra: regra.to_owned(),
            versao,
            resultados,
        })
    }

    /// Ativa uma versão (rascunho ou desativada). Só [`Quem::Usuario`] ou [`Quem::Mcp`] (D15). Bloqueia se algum
    /// exemplo falha (D12) ou se há script e não há executor que o verifique. A versão
    /// vigente anterior vira desativada; tudo numa transação.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoPermitido`], [`ErroDasRegras::Exemplos`],
    /// [`ErroDasRegras::Script`], [`ErroDasRegras::NaoEncontrada`].
    pub fn ativar(
        &mut self,
        regra: &str,
        versao: u32,
        quem: &Quem,
        executor: Option<&dyn ExecutorDeScript>,
    ) -> Resultado<()> {
        exigir_decisor(quem, "ativar")?;
        let motivo = format!("ativada {}", pelo(quem));
        self.ativar_como(regra, versao, quem, &motivo, executor)
    }

    fn ativar_como(
        &mut self,
        regra: &str,
        versao: u32,
        quem: &Quem,
        motivo: &str,
        executor: Option<&dyn ExecutorDeScript>,
    ) -> Resultado<()> {
        let v = self.versao(regra, versao)?;
        if !matches!(v.estado, Estado::Rascunho | Estado::Desativada) {
            return Err(ErroDasRegras::NaoPermitido(format!(
                "a versão {versao} de `{regra}` já está em vigor"
            )));
        }
        if let Some(fonte) = &v.script {
            let Some(ex) = executor else {
                return Err(ErroDasRegras::Script(
                    crate::script::ErroDoScript::Processo(
                        "a versão tem script e não há executor para verificá-lo".to_owned(),
                    ),
                ));
            };
            ex.verificar(fonte)?;
        }
        let relatorio = self.testar(regra, versao, executor)?;
        if !relatorio.aprovado() {
            return Err(ErroDasRegras::Exemplos(relatorio.falhas()));
        }
        let q = quem_texto(quem);
        let tx = self.conn.transaction().map_err(banco)?;
        if let Some(anterior) = versao_vigente(&tx, regra)? {
            trocar_estado(
                &tx,
                &anterior,
                Estado::Desativada,
                q,
                &format!("substituída pela versão {versao}"),
            )?;
        }
        trocar_estado(&tx, &v, Estado::Ativa, q, motivo)?;
        tx.commit().map_err(banco)
    }

    /// Desativa a versão em vigor da regra. Só [`Quem::Usuario`] ou [`Quem::Mcp`] (D15).
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoPermitido`], [`ErroDasRegras::NaoEncontrada`].
    pub fn desativar(&mut self, regra: &str, quem: &Quem) -> Resultado<()> {
        exigir_decisor(quem, "desativar")?;
        let tx = self.conn.transaction().map_err(banco)?;
        let v = versao_vigente(&tx, regra)?.ok_or_else(|| {
            ErroDasRegras::NaoEncontrada(format!("`{regra}` não tem versão em vigor"))
        })?;
        trocar_estado(
            &tx,
            &v,
            Estado::Desativada,
            quem_texto(quem),
            &format!("desativada {}", pelo(quem)),
        )?;
        tx.commit().map_err(banco)
    }

    /// Rollback: reativa a versão que estava em vigor antes da atual (a mais recente que
    /// foi ativada e hoje está desativada) e desativa a atual. Preserva todas as versões
    /// e não roda os exemplos de novo (a versão já esteve em vigor). Reverter duas vezes
    /// volta à versão de antes. Só [`Quem::Usuario`] ou [`Quem::Mcp`] (D15).
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoPermitido`], [`ErroDasRegras::NaoEncontrada`].
    pub fn reverter(&mut self, regra: &str, quem: &Quem) -> Resultado<()> {
        exigir_decisor(quem, "reverter")?;
        let tx = self.conn.transaction().map_err(banco)?;
        let atual = versao_vigente(&tx, regra)?.ok_or_else(|| {
            ErroDasRegras::NaoEncontrada(format!("`{regra}` não tem versão em vigor"))
        })?;
        let anterior: Option<u32> = tx
            .query_row(
                "SELECT h.versao FROM historico_de_estado h
                 JOIN versao_de_regra v ON v.regra = h.regra AND v.versao = h.versao
                 WHERE h.regra = ?1 AND h.para = 'ativa' AND h.versao <> ?2
                   AND v.estado = 'desativada'
                 ORDER BY h.id DESC LIMIT 1",
                params![regra, atual.versao],
                |r| r.get(0),
            )
            .optional()
            .map_err(banco)?;
        let anterior = anterior.ok_or_else(|| {
            ErroDasRegras::NaoEncontrada(format!("`{regra}` não tem versão anterior para voltar"))
        })?;
        let alvo = carregar_versao(&tx, regra, anterior)?;
        let q = quem_texto(quem);
        trocar_estado(
            &tx,
            &atual,
            Estado::Desativada,
            q,
            &format!("revertida para a versão {anterior}"),
        )?;
        trocar_estado(
            &tx,
            &alvo,
            Estado::Ativa,
            q,
            &format!("reativada no rollback da versão {}", atual.versao),
        )?;
        tx.commit().map_err(banco)
    }

    /// Consolida uma versão ativa que tem [`LIMIAR_DE_CONSOLIDACAO`] aceitas seguidas.
    /// Só [`Quem::Usuario`] ou [`Quem::Mcp`] (D15).
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoPermitido`].
    pub fn consolidar(&mut self, regra: &str, versao: u32, quem: &Quem) -> Resultado<()> {
        exigir_decisor(quem, "consolidar")?;
        let tx = self.conn.transaction().map_err(banco)?;
        let v = carregar_versao(&tx, regra, versao)?;
        if v.estado != Estado::Ativa {
            return Err(ErroDasRegras::NaoPermitido(
                "só uma versão ativa pode ser consolidada".to_owned(),
            ));
        }
        if v.aceitas_seguidas < LIMIAR_DE_CONSOLIDACAO {
            return Err(ErroDasRegras::NaoPermitido(format!(
                "faltam propostas aceitas seguidas: {} de {LIMIAR_DE_CONSOLIDACAO}",
                v.aceitas_seguidas
            )));
        }
        trocar_estado(
            &tx,
            &v,
            Estado::Consolidada,
            quem_texto(quem),
            &format!("consolidada {}", pelo(quem)),
        )?;
        tx.commit().map_err(banco)
    }

    /// As versões ativas com aceitas seguidas suficientes para sugerir consolidar.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn sugestoes_de_consolidacao(&self) -> Resultado<Vec<(IdDaRegra, u32)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT regra, aceitas_seguidas FROM versao_de_regra
                 WHERE estado = 'ativa' AND aceitas_seguidas >= ?1 ORDER BY regra",
            )
            .map_err(banco)?;
        st.query_map(params![LIMIAR_DE_CONSOLIDACAO], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(banco)?
        .collect::<Result<_, _>>()
        .map_err(banco)
    }

    // ---- execuções, propostas e retornos ----

    /// Grava as execuções de uma leitura (sem texto do documento). Idempotente pela chave;
    /// as que "não se aplicam" não são gravadas (seriam uma linha por regra por entrada).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn registrar_execucao(&mut self, leitura: &Leitura) -> Resultado<()> {
        self.gravar_execucoes(leitura)?;
        self.registrar_decisao(leitura)
    }

    /// Como [`Catalogo::registrar_execucao`], mas **só** a execução: não mexe na decisão da
    /// entrada (o âmbar). É o que a passada de fundo do worker usa, lendo uma versão por vez:
    /// uma leitura parcial não enxerga o empate com as outras versões (spec 15, D4).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn registrar_execucao_de_fundo(&mut self, leitura: &Leitura) -> Resultado<()> {
        self.gravar_execucoes(leitura)
    }

    fn gravar_execucoes(&mut self, leitura: &Leitura) -> Resultado<()> {
        let mut chaves: BTreeMap<(String, u32), String> = BTreeMap::new();
        match &leitura.decisao {
            Decisao::Reconhecido(r) => {
                chaves.insert((r.regra.clone(), r.versao), r.chave.clone());
            }
            Decisao::Ambiguo { candidatas, .. } => {
                for r in candidatas {
                    chaves.insert((r.regra.clone(), r.versao), r.chave.clone());
                }
            }
            Decisao::Nenhuma => {}
        }
        let tx = self.conn.transaction().map_err(banco)?;
        for e in &leitura.execucoes {
            if e.resultado == ResultadoDaExecucao::NaoSeAplica {
                continue;
            }
            let checksum = carregar_versao(&tx, &e.regra, e.versao)
                .map(|v| v.checksum)
                .unwrap_or_default();
            let chave = chaves
                .get(&(e.regra.clone(), e.versao))
                .cloned()
                .unwrap_or_else(|| {
                    crate::motor::chave_de_idempotencia(
                        &leitura.entrada,
                        &e.regra,
                        e.versao,
                        &checksum,
                        &[],
                    )
                });
            tx.execute(
                "INSERT OR IGNORE INTO execucao (entrada, regra, versao, checksum, chave,
                     resultado, motivo, duracao_us, em) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    leitura.entrada,
                    e.regra,
                    e.versao,
                    checksum,
                    chave,
                    para_texto(&e.resultado),
                    e.motivo,
                    i64::try_from(e.duracao_us).unwrap_or(i64::MAX),
                    agora()
                ],
            )
            .map_err(banco)?;
        }
        tx.commit().map_err(banco)
    }

    /// Quantas execuções estão gravadas.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn total_de_execucoes(&self) -> Resultado<u64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM execucao", [], |r| contagem(r, 0))
            .map_err(banco)
    }

    /// Grava a proposta de um reconhecimento, uma vez por `chave` (AC-LUA-09).
    /// Devolve `true` se gravou, `false` se a chave já existia.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn registrar_proposta(&mut self, r: &Reconhecimento, entrada: &str) -> Resultado<bool> {
        // As linhas de `exibir` são só para a tela e podem ter texto do documento: não ficam
        // gravadas.
        // O CPF/CNPJ da contraparte nunca fica gravado, nem como campo extraído.
        let mut sem_exibir = Reconhecimento {
            exibir: Vec::new(),
            ..r.clone()
        };
        sem_exibir.campos.retain(
            |_, c| !matches!(&c.de_onde, DeOnde::Dica { nome } if nome == "contraparte_doc"),
        );
        let conteudo = serde_json::to_string(&sem_exibir).map_err(banco)?;
        let n = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO proposta (chave, entrada, regra, versao, conteudo, estado,
                     criada_em) VALUES (?1, ?2, ?3, ?4, ?5, 'aberta', ?6)",
                params![r.chave, entrada, r.regra, r.versao, conteudo, agora()],
            )
            .map_err(banco)?;
        Ok(n > 0)
    }

    /// As propostas gravadas, da mais antiga à mais nova.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn propostas(&self, estado: Option<EstadoDaProposta>) -> Resultado<Vec<PropostaGravada>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT chave, entrada, regra, versao, conteudo, estado, criada_em, decidida_em
                 FROM proposta WHERE (?1 IS NULL OR estado = ?1) ORDER BY id",
            )
            .map_err(banco)?;
        let filtro = estado.map(|e| para_texto(&e));
        let linhas = st
            .query_map(params![filtro], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(banco)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(banco)?;
        linhas
            .into_iter()
            .map(
                |(chave, entrada, regra, versao, conteudo, estado, criada_em, decidida_em)| {
                    Ok(PropostaGravada {
                        chave,
                        entrada,
                        regra,
                        versao,
                        conteudo: serde_json::from_str(&conteudo).map_err(banco)?,
                        estado: de_texto(&estado)?,
                        criada_em,
                        decidida_em,
                    })
                },
            )
            .collect()
    }

    /// O que o usuário fez com a proposta de `chave`. `Aceito` soma uma aceita seguida;
    /// `Corrigido`, `Rejeitado` e `Desfeito` zeram e, se a versão era consolidada,
    /// a rebaixam para ativa (D4) (pode ser pedido por [`Quem::Sistema`]). A IA não registra
    /// retornos pelo usuário.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoEncontrada`] (chave), [`ErroDasRegras::NaoPermitido`].
    pub fn registrar_retorno(
        &mut self,
        chave: &str,
        retorno: Retorno,
        quem: &Quem,
        motivo: &str,
    ) -> Resultado<()> {
        if matches!(quem, Quem::Ia | Quem::Mcp) {
            return Err(ErroDasRegras::NaoPermitido(
                "a IA e o MCP não registram retorno do usuário".to_owned(),
            ));
        }
        let tx = self.conn.transaction().map_err(banco)?;
        let (regra, versao, estado): (String, u32, String) = tx
            .query_row(
                "SELECT regra, versao, estado FROM proposta WHERE chave = ?1",
                params![chave],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(banco)?
            .ok_or_else(|| ErroDasRegras::NaoEncontrada(format!("proposta {chave}")))?;
        let estado: EstadoDaProposta = de_texto(&estado)?;
        let (exigido, novo) = match retorno {
            Retorno::Aceito | Retorno::Corrigido => {
                (EstadoDaProposta::Aberta, EstadoDaProposta::Aceita)
            }
            Retorno::Rejeitado => (EstadoDaProposta::Aberta, EstadoDaProposta::Recusada),
            Retorno::Desfeito => (EstadoDaProposta::Aceita, EstadoDaProposta::Desfeita),
        };
        if estado != exigido {
            return Err(ErroDasRegras::NaoPermitido(format!(
                "a proposta está {} e não aceita o retorno {}",
                para_texto(&estado),
                para_texto(&retorno)
            )));
        }
        let agora = agora();
        tx.execute(
            "INSERT INTO retorno (chave, regra, versao, retorno, quem, motivo, em)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                chave,
                regra,
                versao,
                para_texto(&retorno),
                quem_texto(quem),
                motivo,
                agora
            ],
        )
        .map_err(banco)?;
        tx.execute(
            "UPDATE proposta SET estado = ?2, decidida_em = ?3 WHERE chave = ?1",
            params![chave, para_texto(&novo), agora],
        )
        .map_err(banco)?;
        if retorno == Retorno::Aceito {
            tx.execute(
                "UPDATE versao_de_regra SET aceitas_seguidas = aceitas_seguidas + 1
                 WHERE regra = ?1 AND versao = ?2",
                params![regra, versao],
            )
            .map_err(banco)?;
        } else {
            tx.execute(
                "UPDATE versao_de_regra SET aceitas_seguidas = 0 WHERE regra = ?1 AND versao = ?2",
                params![regra, versao],
            )
            .map_err(banco)?;
            let porque = match retorno {
                Retorno::Rejeitado => "rejeitada pelo usuário",
                Retorno::Corrigido => "corrigida pelo usuário",
                _ => "conciliação automática desfeita",
            };
            let v = carregar_versao(&tx, &regra, versao)?;
            if v.estado == Estado::Consolidada {
                // `trocar_estado(Ativa)` limparia a data de consolidação e o contador,
                // que é o que se quer num rebaixamento.
                trocar_estado(&tx, &v, Estado::Ativa, quem_texto(quem), porque)?;
            }
        }
        tx.commit().map_err(banco)
    }

    // ---- exemplos ----

    /// Cria ou substitui o exemplo `nome` da regra; o histórico guarda cada versão dele.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoEncontrada`] se a regra não existe.
    pub fn salvar_exemplo(
        &mut self,
        regra: &str,
        nome: &str,
        entrada: &Entrada,
        esperado: Esperado,
    ) -> Resultado<()> {
        let tx = self.conn.transaction().map_err(banco)?;
        let existe: bool = tx
            .query_row("SELECT 1 FROM regra WHERE id = ?1", params![regra], |_| {
                Ok(true)
            })
            .optional()
            .map_err(banco)?
            .unwrap_or(false);
        if !existe {
            return Err(ErroDasRegras::NaoEncontrada(format!("regra `{regra}`")));
        }
        let e = serde_json::to_string(entrada).map_err(banco)?;
        let x = serde_json::to_string(&esperado).map_err(banco)?;
        let agora = agora();
        let n = tx
            .execute(
                "UPDATE exemplo_de_regra SET entrada = ?3, esperado = ?4, atualizado_em = ?5
                 WHERE regra = ?1 AND nome = ?2",
                params![regra, nome, e, x, agora],
            )
            .map_err(banco)?;
        let acao = if n == 0 {
            tx.execute(
                "INSERT INTO exemplo_de_regra (regra, nome, entrada, esperado, criado_em,
                     atualizado_em) VALUES (?1,?2,?3,?4,?5,?5)",
                params![regra, nome, e, x, agora],
            )
            .map_err(banco)?;
            "criado"
        } else {
            "alterado"
        };
        tx.execute(
            "INSERT INTO historico_de_exemplo (regra, nome, acao, entrada, esperado, em)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![regra, nome, acao, e, x, agora],
        )
        .map_err(banco)?;
        tx.commit().map_err(banco)
    }

    /// Os exemplos de uma regra, pela ordem de criação.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn exemplos(&self, regra: &str) -> Resultado<Vec<ExemploDeRegra>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, regra, nome, entrada, esperado FROM exemplo_de_regra
                 WHERE regra = ?1 ORDER BY id",
            )
            .map_err(banco)?;
        let linhas = st
            .query_map(params![regra], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(banco)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(banco)?;
        linhas
            .into_iter()
            .map(|(id, regra, nome, entrada, esperado)| {
                Ok(ExemploDeRegra {
                    id,
                    regra,
                    nome,
                    entrada: serde_json::from_str(&entrada).map_err(banco)?,
                    esperado: serde_json::from_str(&esperado).map_err(banco)?,
                })
            })
            .collect()
    }

    /// Apaga um exemplo, deixando o conteúdo dele no histórico.
    ///
    /// # Errors
    /// [`ErroDasRegras::NaoEncontrada`].
    pub fn apagar_exemplo(&mut self, regra: &str, nome: &str) -> Resultado<()> {
        let tx = self.conn.transaction().map_err(banco)?;
        let atual: Option<(String, String)> = tx
            .query_row(
                "SELECT entrada, esperado FROM exemplo_de_regra WHERE regra = ?1 AND nome = ?2",
                params![regra, nome],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(banco)?;
        let (e, x) = atual.ok_or_else(|| {
            ErroDasRegras::NaoEncontrada(format!("exemplo `{nome}` de `{regra}`"))
        })?;
        tx.execute(
            "INSERT INTO historico_de_exemplo (regra, nome, acao, entrada, esperado, em)
             VALUES (?1,?2,'apagado',?3,?4,?5)",
            params![regra, nome, e, x, agora()],
        )
        .map_err(banco)?;
        tx.execute(
            "DELETE FROM exemplo_de_regra WHERE regra = ?1 AND nome = ?2",
            params![regra, nome],
        )
        .map_err(banco)?;
        tx.commit().map_err(banco)
    }

    /// Quantas linhas o histórico de exemplos de uma regra tem.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn total_do_historico_de_exemplos(&self, regra: &str) -> Resultado<u64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM historico_de_exemplo WHERE regra = ?1",
                params![regra],
                |r| contagem(r, 0),
            )
            .map_err(banco)
    }

    // ---- parâmetros ----

    /// Grava uma versão nova do parâmetro `nome` (versão = maior + 1), vigente de
    /// `vigencia_inicio` a `vigencia_fim` (`AAAA-MM`; fim `None` é aberto). Imutável.
    ///
    /// # Errors
    /// [`ErroDasRegras::Definicao`] se o nome ou as competências são inválidos.
    pub fn salvar_parametro(
        &mut self,
        nome: &str,
        vigencia_inicio: &str,
        vigencia_fim: Option<&str>,
        valor: Json,
        fonte: &str,
    ) -> Resultado<Parametro> {
        let competencia_ok = |c: &str| parse_competencia(c).as_deref() == Some(c);
        if !id_valido(&nome.replace('_', "-")) {
            return Err(ErroDasRegras::Definicao(format!(
                "nome de parâmetro inválido `{nome}`"
            )));
        }
        if !competencia_ok(vigencia_inicio) || vigencia_fim.is_some_and(|f| !competencia_ok(f)) {
            return Err(ErroDasRegras::Definicao(
                "vigência deve ser AAAA-MM".to_owned(),
            ));
        }
        if vigencia_fim.is_some_and(|f| f < vigencia_inicio) {
            return Err(ErroDasRegras::Definicao(
                "vigência termina antes de começar".to_owned(),
            ));
        }
        let tx = self.conn.transaction().map_err(banco)?;
        let versao: u32 = tx
            .query_row(
                "SELECT COALESCE(MAX(versao), 0) + 1 FROM parametro WHERE nome = ?1",
                params![nome],
                |r| r.get(0),
            )
            .map_err(banco)?;
        let base = serde_json::json!([nome, versao, vigencia_inicio, vigencia_fim, valor]);
        let checksum = sha256_hex(json_canonico(&base).as_bytes());
        let criado_em = agora();
        tx.execute(
            "INSERT INTO parametro (nome, versao, vigencia_inicio, vigencia_fim, valor, fonte,
                 checksum, criado_em) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                nome,
                versao,
                vigencia_inicio,
                vigencia_fim,
                json_canonico(&valor),
                fonte,
                checksum,
                criado_em
            ],
        )
        .map_err(banco)?;
        tx.commit().map_err(banco)?;
        Ok(Parametro {
            nome: nome.to_owned(),
            versao,
            vigencia_inicio: vigencia_inicio.to_owned(),
            vigencia_fim: vigencia_fim.map(str::to_owned),
            valor,
            fonte: fonte.to_owned(),
            checksum,
            criado_em,
        })
    }

    /// Todos os parâmetros de todas as versões.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn parametros(&self) -> Resultado<Vec<Parametro>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT nome, versao, vigencia_inicio, vigencia_fim, valor, fonte, checksum,
                        criado_em FROM parametro ORDER BY nome, versao",
            )
            .map_err(banco)?;
        let linhas = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(banco)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(banco)?;
        linhas
            .into_iter()
            .map(
                |(nome, versao, ini, fim, valor, fonte, checksum, criado_em)| {
                    Ok(Parametro {
                        nome,
                        versao,
                        vigencia_inicio: ini,
                        vigencia_fim: fim,
                        valor: serde_json::from_str(&valor).map_err(banco)?,
                        fonte,
                        checksum,
                        criado_em,
                    })
                },
            )
            .collect()
    }

    /// Para cada nome, a maior versão que vale na competência `AAAA-MM`.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn parametros_vigentes(&self, competencia: &str) -> Resultado<Vec<Parametro>> {
        let todos = self.parametros()?;
        let mut nomes: Vec<&str> = todos.iter().map(|p| p.nome.as_str()).collect();
        nomes.dedup();
        Ok(nomes
            .into_iter()
            .filter_map(|n| parametro_vigente(&todos, n, competencia).cloned())
            .collect())
    }

    // ---- fábrica ----

    /// Grava as regras de [`fabrica::regras_de_fabrica`] que faltam (idempotente pelo
    /// checksum) e ativa as marcadas como ativas que nunca foram ativadas. **É a única
    /// ativação sem usuário** (feita por [`Quem::Sistema`], só para autor
    /// [`Autor::Fabrica`]): reproduz o comportamento que o sbf já tinha (D10). Uma regra
    /// de fábrica que o usuário desativou não volta sozinha.
    ///
    /// # Errors
    /// [`ErroDasRegras`].
    pub fn semear_fabrica(&mut self) -> Resultado<()> {
        for (nova, estado) in fabrica::regras_de_fabrica() {
            if nova.autor != Autor::Fabrica {
                return Err(ErroDasRegras::NaoPermitido(
                    "só regra de autor Fabrica é semeada".to_owned(),
                ));
            }
            // Uma regra pode ter várias versões de fábrica: acha pela identidade do
            // conteúdo (checksum), não pela última versão.
            let checksum = checksum_da_versao(&nova.definicao, nova.script.as_deref());
            let existente = self
                .versoes(&nova.regra)?
                .into_iter()
                .find(|v| v.checksum == checksum);
            let v = match existente {
                Some(v) => v,
                None => self.salvar_rascunho(nova)?,
            };
            if estado == Estado::Ativa && v.estado == Estado::Rascunho {
                self.ativar_como(&v.regra, v.versao, &Quem::Sistema, "regra de fábrica", None)?;
            }
        }
        for p in fabrica::parametros_de_fabrica() {
            let ja = self.parametros()?.into_iter().any(|x| {
                x.nome == p.nome
                    && x.vigencia_inicio == p.vigencia_inicio
                    && serde_json::from_str::<Json>(p.valor).is_ok_and(|v| v == x.valor)
            });
            if !ja {
                let valor: Json = serde_json::from_str(p.valor)
                    .map_err(|e| ErroDasRegras::Definicao(format!("parâmetro de fábrica: {e}")))?;
                self.salvar_parametro(p.nome, p.vigencia_inicio, None, valor, p.fonte)?;
            }
        }
        for (regra, nome, entrada, esperado) in fabrica::exemplos_de_fabrica() {
            if !self.exemplos(&regra)?.iter().any(|e| e.nome == nome) {
                self.salvar_exemplo(&regra, &nome, &entrada, esperado)?;
            }
        }
        Ok(())
    }

    /// Só para os testes de imutabilidade: executa SQL cru.
    #[doc(hidden)]
    pub fn sql_cru_para_testes(&self, sql: &str) -> Result<usize, String> {
        self.conn.execute(sql, []).map_err(|e| e.to_string())
    }
}

/// Compara o esperado com a leitura; `None` se confere, senão o que divergiu.
fn conferir(esperado: &Esperado, leitura: &Leitura) -> Option<String> {
    match (esperado, &leitura.decisao) {
        (Esperado::NaoSeAplica, Decisao::Nenhuma) => None,
        (Esperado::NaoSeAplica, outra) => Some(format!(
            "esperava que não se aplicasse, mas {}",
            descrever(outra, leitura)
        )),
        (Esperado::Reconhece { .. }, Decisao::Nenhuma) => Some(format!(
            "esperava reconhecer, mas {}",
            descrever(&Decisao::Nenhuma, leitura)
        )),
        (Esperado::Reconhece { .. }, Decisao::Ambiguo { motivo, .. }) => {
            Some(format!("ambíguo: {motivo}"))
        }
        (
            Esperado::Reconhece {
                tipo_documental,
                campos,
            },
            Decisao::Reconhecido(r),
        ) => {
            if r.tipo_documental != *tipo_documental {
                return Some(format!(
                    "tipo `{}` em vez de `{tipo_documental}`",
                    r.tipo_documental
                ));
            }
            let mut erros = Vec::new();
            for (nome, valor) in campos {
                match r.campos.get(nome) {
                    Some(c) if c.valor == *valor => {}
                    // Com leitor nativo, os campos do leitor só chegam em `completar`.
                    None if r.leitor.is_some() => {}
                    Some(c) => erros.push(format!("`{nome}` = {}, esperado {valor}", c.valor)),
                    None => erros.push(format!("`{nome}` não foi extraído")),
                }
            }
            (!erros.is_empty()).then(|| erros.join("; "))
        }
    }
}

fn descrever(d: &Decisao, leitura: &Leitura) -> String {
    match d {
        Decisao::Reconhecido(r) => format!("reconheceu `{}`", r.tipo_documental),
        Decisao::Ambiguo { motivo, .. } => format!("ficou ambíguo: {motivo}"),
        Decisao::Nenhuma => {
            let erros: Vec<String> = leitura
                .execucoes
                .iter()
                .filter_map(|e| e.motivo.clone())
                .collect();
            if erros.is_empty() {
                "nenhuma regra se aplicou".to_owned()
            } else {
                format!("nenhuma regra se aplicou ({})", erros.join("; "))
            }
        }
    }
}
