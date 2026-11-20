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
//! Os estados são os do glossário. Os que **a Caixa de entrada** lista são os reconhecidos
//! ([`Estado::e_reconhecido`]: Pronto, Precisa revisar, Possível duplicidade); os que **A
//! triar** lista como "sem padrão de reconhecimento" são os outros dois
//! ([`Estado::sem_padrao`]: Não reconhecido e Protegido por senha). "Reconhecer de novo"
//! troca o estado, e o item muda de lista sozinho.

use rusqlite::{Connection, OptionalExtension as _, params};

use crate::esquema::MIGRACAO_4_CAIXA_DE_ENTRADA;

/// Por onde o documento chegou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Origem {
    Telegram,
    Gmail,
    Celular,
    Pasta,
    Arrastado,
    Cli,
}

impl Origem {
    pub const TODAS: [Origem; 6] = [
        Origem::Telegram,
        Origem::Gmail,
        Origem::Celular,
        Origem::Pasta,
        Origem::Arrastado,
        Origem::Cli,
    ];

    /// O valor gravado na coluna `origem`.
    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Origem::Telegram => "telegram",
            Origem::Gmail => "gmail",
            Origem::Celular => "celular",
            Origem::Pasta => "pasta",
            Origem::Arrastado => "arrastado",
            Origem::Cli => "cli",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Origem> {
        Origem::TODAS.into_iter().find(|o| o.codigo() == c)
    }

    /// O nome na interface.
    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Origem::Telegram => "Telegram",
            Origem::Gmail => "Gmail",
            Origem::Celular => "Celular",
            Origem::Pasta => "Pasta",
            Origem::Arrastado => "Arrastado",
            Origem::Cli => "Linha de comando",
        }
    }

    /// Uma ação explícita do usuário (arrastar, `sbf reconhecer`) reabre o que ele já tinha
    /// resolvido; a chegada automática não.
    #[must_use]
    pub fn e_acao_do_usuario(self) -> bool {
        matches!(self, Origem::Arrastado | Origem::Cli)
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

    /// Algum leitor ou regra casou: o documento é da Caixa de entrada.
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
    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Situacao::Aberta => "aberta",
            Situacao::Resolvida => "resolvida",
            Situacao::Dispensada => "dispensada",
        }
    }

    fn de_codigo(c: &str) -> Situacao {
        match c {
            "resolvida" => Situacao::Resolvida,
            "dispensada" => Situacao::Dispensada,
            _ => Situacao::Aberta,
        }
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
    pub origem: Origem,
    pub tipo: Option<String>,
    pub resumo: Option<String>,
    pub estado: Estado,
    pub situacao: Situacao,
    pub vezes: i64,
}

/// O que o reconhecimento registra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registro {
    pub content_hash: String,
    pub nome: String,
    pub origem: Origem,
    pub tipo: Option<String>,
    pub resumo: Option<String>,
    pub estado: Estado,
}

/// Quais linhas listar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grupo {
    /// Os reconhecidos (Pronto, Precisa revisar, Possível duplicidade) e o que ainda
    /// processa: a Caixa de entrada.
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
    pub origem: Option<Origem>,
    pub limite: u32,
}

impl Filtro {
    /// O padrão da Caixa de entrada: reconhecidos ainda abertos.
    #[must_use]
    pub fn caixa() -> Self {
        Self {
            grupo: Grupo::Reconhecidos,
            so_abertas: true,
            origem: None,
            limite: 500,
        }
    }

    /// O padrão da seção "Sem padrão de reconhecimento" de A triar.
    #[must_use]
    pub fn sem_padrao() -> Self {
        Self {
            grupo: Grupo::SemPadrao,
            so_abertas: true,
            origem: None,
            limite: 500,
        }
    }
}

/// Os contadores das áreas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contagens {
    /// Reconhecidos ainda não resolvidos (o contador da Caixa de entrada).
    pub reconhecidos: i64,
    /// Sem padrão ainda não resolvidos (o "N sem padrão" de A triar).
    pub sem_padrao: i64,
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
        "SELECT COUNT(*) FROM origem WHERE content_hash = ?1 AND fonte_id <> 'arrastado'",
        [sha256],
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
    origem: Origem,
) -> rusqlite::Result<()> {
    garantir_tabela(conn)?;
    conn.execute(
        "INSERT INTO caixa_entrada (content_hash, nome, origem, estado, vezes)
         VALUES (?1, ?2, ?3, 'processando', 0)
         ON CONFLICT(content_hash) DO UPDATE SET
           estado = 'processando',
           atualizado_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![sha256, nome, origem.codigo()],
    )?;
    Ok(())
}

/// Registra (ou atualiza) o resultado do reconhecimento. A origem e a chegada são as da
/// primeira vez. Uma ação do usuário ([`Origem::e_acao_do_usuario`]) reabre o que estava
/// resolvido ou dispensado; a chegada automática de um conteúdo repetido não.
///
/// # Errors
/// Falha do SQLite.
pub fn registrar(conn: &Connection, r: &Registro) -> rusqlite::Result<()> {
    garantir_tabela(conn)?;
    conn.execute(
        "INSERT INTO caixa_entrada (content_hash, nome, origem, tipo, resumo, estado)
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
            r.content_hash,
            r.nome,
            r.origem.codigo(),
            r.tipo,
            r.resumo,
            r.estado.codigo(),
            r.origem.e_acao_do_usuario(),
        ],
    )?;
    Ok(())
}

/// Resolver, dispensar ou reabrir. `false` quando a linha não existe.
///
/// # Errors
/// Falha do SQLite.
pub fn definir_situacao(
    conn: &Connection,
    sha256: &str,
    situacao: Situacao,
) -> rusqlite::Result<bool> {
    garantir_tabela(conn)?;
    let n = conn.execute(
        "UPDATE caixa_entrada
            SET situacao = ?2,
                resolvida_em = CASE WHEN ?2 = 'aberta' THEN NULL
                               ELSE strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END
          WHERE content_hash = ?1",
        params![sha256, situacao.codigo()],
    )?;
    Ok(n > 0)
}

fn de_linha(l: &rusqlite::Row<'_>) -> rusqlite::Result<Entrada> {
    let origem: String = l.get(4)?;
    let estado: String = l.get(7)?;
    let situacao: String = l.get(8)?;
    Ok(Entrada {
        content_hash: l.get(0)?,
        nome: l.get(1)?,
        chegou_em: l.get(2)?,
        atualizado_em: l.get(3)?,
        origem: Origem::de_codigo(&origem).unwrap_or(Origem::Cli),
        tipo: l.get(5)?,
        resumo: l.get(6)?,
        estado: Estado::de_codigo(&estado).unwrap_or(Estado::NaoReconhecido),
        situacao: Situacao::de_codigo(&situacao),
        vezes: l.get(9)?,
    })
}

const COLUNAS: &str = "content_hash, nome, chegou_em, atualizado_em, origem, tipo, resumo, \
                       estado, situacao, vezes";

/// Uma linha da fila.
///
/// # Errors
/// Falha do SQLite.
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
/// Falha do SQLite.
pub fn listar(conn: &Connection, filtro: &Filtro) -> rusqlite::Result<Vec<Entrada>> {
    garantir_tabela(conn)?;
    let grupo = match filtro.grupo {
        Grupo::Reconhecidos => {
            "estado IN ('pronto', 'precisa_revisar', 'possivel_duplicidade', 'processando')"
        }
        Grupo::SemPadrao => "estado IN ('nao_reconhecido', 'protegido')",
        Grupo::Todos => "1 = 1",
    };
    let abertas = if filtro.so_abertas {
        "situacao = 'aberta'"
    } else {
        "1 = 1"
    };
    let sql = format!(
        "SELECT {COLUNAS} FROM caixa_entrada
          WHERE {grupo} AND {abertas} AND (?1 IS NULL OR origem = ?1)
          ORDER BY chegou_em DESC, content_hash LIMIT ?2"
    );
    let mut q = conn.prepare(&sql)?;
    q.query_map(
        params![filtro.origem.map(Origem::codigo), filtro.limite],
        de_linha,
    )?
    .collect()
}

/// Os contadores das duas listas (só o que está aberto).
///
/// # Errors
/// Falha do SQLite.
pub fn contagens(conn: &Connection) -> rusqlite::Result<Contagens> {
    garantir_tabela(conn)?;
    conn.query_row(
        "SELECT
           COALESCE(SUM(estado IN ('pronto', 'precisa_revisar', 'possivel_duplicidade')), 0),
           COALESCE(SUM(estado IN ('nao_reconhecido', 'protegido')), 0)
         FROM caixa_entrada WHERE situacao = 'aberta'",
        [],
        |l| {
            Ok(Contagens {
                reconhecidos: l.get(0)?,
                sem_padrao: l.get(1)?,
            })
        },
    )
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

#[cfg(test)]
mod testes {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().expect("memória");
        c.execute_batch(crate::esquema::MIGRACAO_1_BASE_BUN)
            .expect("base");
        c
    }

    fn registro(sha: &str, origem: Origem, estado: Estado) -> Registro {
        Registro {
            content_hash: sha.to_owned(),
            nome: format!("{sha}.pdf"),
            origem,
            tipo: Some("Boleto".to_owned()),
            resumo: Some("Boleto · Fornecedor Exemplo".to_owned()),
            estado,
        }
    }

    #[test]
    fn registra_lista_e_conta_por_grupo() {
        let c = conn();
        registrar(&c, &registro("a", Origem::Telegram, Estado::Pronto)).expect("a");
        registrar(&c, &registro("b", Origem::Gmail, Estado::PrecisaRevisar)).expect("b");
        registrar(&c, &registro("c", Origem::Pasta, Estado::NaoReconhecido)).expect("c");
        registrar(&c, &registro("d", Origem::Pasta, Estado::ProtegidoPorSenha)).expect("d");
        let caixa = listar(&c, &Filtro::caixa()).expect("caixa");
        assert_eq!(caixa.len(), 2);
        let sem = listar(&c, &Filtro::sem_padrao()).expect("sem padrão");
        assert_eq!(sem.len(), 2);
        assert_eq!(
            contagens(&c).expect("contagens"),
            Contagens {
                reconhecidos: 2,
                sem_padrao: 2
            }
        );
        let so_gmail = listar(
            &c,
            &Filtro {
                origem: Some(Origem::Gmail),
                ..Filtro::caixa()
            },
        )
        .expect("gmail");
        assert_eq!(so_gmail.len(), 1);
        assert_eq!(so_gmail[0].content_hash, "b");
    }

    #[test]
    fn idempotente_guarda_a_primeira_origem_e_conta_as_vezes() {
        let c = conn();
        registrar(&c, &registro("a", Origem::Telegram, Estado::NaoReconhecido)).expect("1");
        let mut r = registro("a", Origem::Gmail, Estado::Pronto);
        r.resumo = Some("novo".to_owned());
        registrar(&c, &r).expect("2");
        let e = obter(&c, "a").expect("obter").expect("existe");
        assert_eq!(e.origem, Origem::Telegram, "a origem é a da chegada");
        assert_eq!(e.estado, Estado::Pronto, "o estado vira o novo");
        assert_eq!(e.vezes, 2);
        assert_eq!(listar(&c, &Filtro::caixa()).expect("lista").len(), 1);
        // O estado mudou: saiu da lista de sem padrão, entrou na da caixa.
        assert!(listar(&c, &Filtro::sem_padrao()).expect("lista").is_empty());
    }

    #[test]
    fn resolver_tira_da_lista_e_so_a_acao_do_usuario_reabre() {
        let c = conn();
        registrar(&c, &registro("a", Origem::Telegram, Estado::Pronto)).expect("a");
        assert!(definir_situacao(&c, "a", Situacao::Resolvida).expect("resolver"));
        assert!(listar(&c, &Filtro::caixa()).expect("lista").is_empty());
        let todos = Filtro {
            so_abertas: false,
            ..Filtro::caixa()
        };
        assert_eq!(listar(&c, &todos).expect("todos").len(), 1);
        // O mesmo conteúdo chega de novo, sozinho: continua resolvido.
        registrar(
            &c,
            &registro("a", Origem::Gmail, Estado::PossivelDuplicidade),
        )
        .expect("auto");
        assert!(listar(&c, &Filtro::caixa()).expect("lista").is_empty());
        // Arrastar de novo é decisão do usuário: reabre.
        registrar(&c, &registro("a", Origem::Arrastado, Estado::Pronto)).expect("arrastar");
        assert_eq!(listar(&c, &Filtro::caixa()).expect("lista").len(), 1);
        assert!(!definir_situacao(&c, "inexistente", Situacao::Dispensada).expect("nada"));
    }

    #[test]
    fn processando_nao_apaga_o_que_ja_se_sabia() {
        let c = conn();
        registrar(&c, &registro("a", Origem::Cli, Estado::Pronto)).expect("a");
        marcar_processando(&c, "a", "a.pdf", Origem::Cli).expect("processando");
        let e = obter(&c, "a").expect("obter").expect("existe");
        assert_eq!(e.estado, Estado::Processando);
        assert_eq!(e.tipo.as_deref(), Some("Boleto"));
    }

    #[test]
    fn a_tabela_nasce_sozinha_num_banco_sem_a_migracao() {
        let c = conn();
        // Sem `garantir_tabela` explícito: `registrar` cria.
        registrar(&c, &registro("a", Origem::Pasta, Estado::Pronto)).expect("a");
        garantir_tabela(&c).expect("de novo");
        assert_eq!(contagens(&c).expect("contagens").reconhecidos, 1);
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
}
