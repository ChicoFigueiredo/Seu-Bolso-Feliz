//! Ajuda dos testes da triagem: banco fictício migrado, inserções e um `Provedor` falso.
//! Reaproveita a fixture do `sbf-inventario` (`tests/comum`): `aplicar_schema_do_bun`.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{Connection, params};
use sbf_ia::{
    Consumo, ErroDeIa, NivelDeConfianca, Pedido, Provedor, Registro, Resposta, ResultadoDeIa, Saida,
};
use sbf_inventario::banco::Inventario;
use serde_json::{Value, json};
use tempfile::TempDir;

#[path = "../../../sbf-inventario/tests/comum/mod.rs"]
pub mod comum;

pub use comum::{Reg, hash, inserir_registro};

pub struct Banco {
    _dir: TempDir,
    pub caminho: PathBuf,
}

impl Banco {
    pub fn conexao(&self) -> Connection {
        let c = Connection::open(&self.caminho).expect("abrir");
        c.busy_timeout(std::time::Duration::from_secs(10))
            .expect("espera");
        c
    }

    pub fn inventario(&self) -> Inventario {
        Inventario::abrir(&self.caminho).expect("abrir inventário")
    }
}

/// Um banco do Bun "2" vazio, migrado pelo mecanismo do app (versão 3), com as fontes que
/// os testes usam. Sem a fixture grande.
pub fn banco_limpo() -> Banco {
    let dir = tempfile::tempdir().expect("diretório");
    let caminho = dir.path().join("inventario.sqlite");
    let c = Connection::open(&caminho).expect("abrir");
    comum::aplicar_schema_do_bun(&c);
    drop(c);
    let mut inv = Inventario::abrir(&caminho).expect("abrir");
    inv.migrar(dir.path().join("copia.sqlite")).expect("migrar");
    drop(inv);
    let c = Connection::open(&caminho).expect("reabrir");
    for (id, tipo) in [
        ("conexao:pluggy", "conexao"),
        ("gmail", "email"),
        ("disco:e2", "disco"),
    ] {
        c.execute(
            "INSERT INTO fonte (id, tipo, descricao) VALUES (?1, ?2, ?1)",
            params![id, tipo],
        )
        .expect("fonte");
    }
    for mes in 1..=12 {
        let (ano_fim, mes_fim) = if mes == 12 {
            (2027, 1)
        } else {
            (2026, mes + 1)
        };
        c.execute(
            "INSERT INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
            params![
                format!("2026-{mes:02}"),
                format!("2026-{mes:02}-20"),
                format!("{ano_fim}-{mes_fim:02}-19")
            ],
        )
        .expect("ciclo");
    }
    c.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
         VALUES (1, 'disco:e2', 'E1', 'tudo', 'teste')",
        [],
    )
    .expect("varredura");
    Banco { _dir: dir, caminho }
}

/// Um achado fictício, com texto e uma origem.
pub fn inserir_achado(
    c: &Connection,
    n: u8,
    estado: &str,
    texto: Option<&str>,
    arquivo: &str,
) -> String {
    let h = hash(n);
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, classificado_por, decidido_por)
         VALUES (?1, 1000, 'A', ?2, 'regras@2', 'regras@2')",
        params![h, estado],
    )
    .expect("achado");
    if let Some(t) = texto {
        c.execute(
            "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
            params![h, t],
        )
        .expect("texto");
    }
    c.execute(
        "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, filename)
         VALUES (?1, 'disco:e2', 1, 'local_file', ?2, ?3)",
        params![h, format!("local:{arquivo}:{n}"), arquivo],
    )
    .expect("origem");
    h
}

/// Um registro de compra fictício sem classificação.
pub fn compra<'a>(
    data: &'a str,
    valor: i64,
    descricao: &'a str,
    fornecedor: Option<&'a str>,
) -> Reg<'a> {
    Reg {
        fonte: "conexao:pluggy",
        tipo: "compra",
        data,
        valor,
        descricao,
        fornecedor,
        ..Reg::default()
    }
}

pub fn valor_texto(c: &Connection, sql: &str) -> String {
    c.query_row(sql, [], |l| l.get::<_, String>(0))
        .unwrap_or_default()
}

pub fn contar(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |l| l.get(0)).expect("contar")
}

// ---------------------------------------------------------------------------------------
// Provedor falso

type Roteiro = Box<dyn FnMut(&Pedido) -> ResultadoDeIa<Value> + Send>;

/// Responde com a função do teste e guarda cada pedido. Nenhum teste chama provedor real.
pub struct Falso {
    roteiro: Mutex<Roteiro>,
    pub pedidos: Mutex<Vec<Pedido>>,
    pub filas: Mutex<VecDeque<ResultadoDeIa<Value>>>,
}

impl Falso {
    pub fn com(roteiro: impl FnMut(&Pedido) -> ResultadoDeIa<Value> + Send + 'static) -> Self {
        Self {
            roteiro: Mutex::new(Box::new(roteiro)),
            pedidos: Mutex::new(Vec::new()),
            filas: Mutex::new(VecDeque::new()),
        }
    }

    /// Respostas em ordem; esgotadas, devolve `Outro`.
    pub fn em_fila(respostas: Vec<ResultadoDeIa<Value>>) -> Self {
        let falso = Self::com(|_| Err(ErroDeIa::Outro("sem resposta no roteiro".into())));
        *falso.filas.lock().expect("fila") = respostas.into();
        falso
    }

    pub fn chamadas(&self) -> usize {
        self.pedidos.lock().expect("pedidos").len()
    }

    pub fn pedido(&self, i: usize) -> Pedido {
        self.pedidos.lock().expect("pedidos")[i].clone()
    }
}

impl Provedor for Falso {
    fn nome(&self) -> &'static str {
        "falso"
    }

    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        self.pedidos.lock().expect("pedidos").push(pedido.clone());
        let proxima = self.filas.lock().expect("fila").pop_front();
        let json = match proxima {
            Some(r) => r?,
            None => (self.roteiro.lock().expect("roteiro"))(pedido)?,
        };
        Ok(Resposta {
            json,
            consumo: Consumo {
                tokens_entrada: 1000 + pedido.entrada.len() as u64 / 4,
                tokens_saida: 200,
                custo_dezmilesimos_usd: 150,
                nominal: true,
            },
            modelo: match pedido.degrau {
                sbf_ia::Degrau::Rapido => "falso-rapido".into(),
                sbf_ia::Degrau::Forte => "falso-forte".into(),
            },
        })
    }
}

/// Faz o papel do motor: registra a saída de cada chamada (sem o texto) em `saida_ia`.
pub struct ComRegistro<'a> {
    pub dentro: &'a dyn Provedor,
    pub registro: &'a dyn Registro,
    pub rodada_agora: i64,
}

impl Provedor for ComRegistro<'_> {
    fn nome(&self) -> &'static str {
        self.dentro.nome()
    }

    fn pedir(&self, pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        let r = self.dentro.pedir(pedido)?;
        self.registro.registrar_saida(&Saida {
            destinatario: "triagem".into(),
            provedor: self.dentro.nome().into(),
            modelo: r.modelo.clone(),
            nivel: NivelDeConfianca::Completo,
            bytes_enviados: pedido.entrada.len() as u64,
            consumo: r.consumo,
            quando: "2026-11-14T10:00:00Z".into(),
        })?;
        Ok(r)
    }
}

/// Os `n` e os ids de registro de uma entrada de lote: `[(n, Some(id registro) ou None)]`.
pub fn itens_da_entrada(entrada: &str) -> Vec<(u64, Option<i64>)> {
    entrada
        .lines()
        .filter_map(|l| l.strip_prefix("=== ITEM "))
        .filter_map(|l| {
            let (n, resto) = l.split_once(' ')?;
            let n = n.parse().ok()?;
            let id = resto
                .strip_prefix("(registro ")
                .and_then(|r| r.split(')').next())
                .and_then(|r| r.parse().ok());
            Some((n, id))
        })
        .collect()
}

/// Uma sugestão no formato da resposta.
pub fn s(campo: &str, valor: Value, confianca: f64) -> Value {
    json!({"campo": campo, "valor": valor, "confianca": confianca, "justificativa": "por causa do teste"})
}

/// Uma resposta com a categoria dada para cada item do lote.
pub fn categoria_para_todos(pedido: &Pedido, categoria: &str, confianca: f64) -> Value {
    let itens: Vec<Value> = itens_da_entrada(&pedido.entrada)
        .into_iter()
        .map(|(n, _)| json!({"n": n, "sugestoes": [s("categoria", json!(categoria), confianca)]}))
        .collect();
    json!({"itens": itens})
}
