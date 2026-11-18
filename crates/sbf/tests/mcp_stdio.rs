//! O protocolo por stdio com o binário de verdade: initialize, tools/list e tools/call,
//! sobre um inventário fictício criado pelo teste.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use serde_json::{Value, json};

#[path = "../../sbf-inventario/tests/comum/mod.rs"]
mod comum;

fn pasta_com_inventario() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("inventario.sqlite");
    let c = Connection::open(&caminho).unwrap();
    comum::aplicar_schema_do_bun(&c);
    drop(c);
    let mut inv = Inventario::abrir(&caminho).unwrap();
    inv.migrar(dir.path().join("copia.sqlite")).unwrap();
    drop(inv);
    let c = Connection::open(&caminho).unwrap();
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('conexao:pluggy', 'conexao', 'x')",
        [],
    )
    .unwrap();
    c.execute_batch(
        "INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-10', '2026-10-20', '2026-11-19');
         INSERT INTO registro (fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos,
            descricao_original, extraido_por, decidido_por)
         VALUES ('conexao:pluggy', 'compra', '2026-10-21', 'compra', '2026-10', -1500,
            'PADARIA FICTICIA', 'pluggy@1', 'pluggy@1');",
    )
    .unwrap();
    dir
}

struct Cliente {
    filho: std::process::Child,
    entrada: std::process::ChildStdin,
    saida: BufReader<std::process::ChildStdout>,
    proximo: u64,
}

impl Cliente {
    fn iniciar(pasta: &std::path::Path) -> Self {
        let mut filho = Command::new(env!("CARGO_BIN_EXE_sbf"))
            .args(["mcp", "--inventario"])
            .arg(pasta)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let entrada = filho.stdin.take().unwrap();
        let saida = BufReader::new(filho.stdout.take().unwrap());
        Self {
            filho,
            entrada,
            saida,
            proximo: 1,
        }
    }

    fn enviar(&mut self, v: &Value) {
        writeln!(self.entrada, "{v}").unwrap();
        self.entrada.flush().unwrap();
    }

    fn pedir(&mut self, metodo: &str, params: Value) -> Value {
        let id = self.proximo;
        self.proximo += 1;
        self.enviar(&json!({"jsonrpc": "2.0", "id": id, "method": metodo, "params": params}));
        loop {
            let mut linha = String::new();
            assert!(
                self.saida.read_line(&mut linha).unwrap() > 0,
                "o servidor fechou"
            );
            let v: Value = serde_json::from_str(&linha).unwrap();
            if v["id"] == json!(id) {
                return v;
            }
        }
    }
}

impl Drop for Cliente {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

#[test]
fn initialize_listar_e_chamar_ferramentas() {
    let dir = pasta_com_inventario();
    let mut c = Cliente::iniciar(dir.path());
    let inicio = c.pedir(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "teste", "version": "0"}}),
    );
    assert_eq!(inicio["result"]["serverInfo"]["name"], "sbf");
    c.enviar(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let lista = c.pedir("tools/list", json!({}));
    let nomes: Vec<&str> = lista["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for esperado in [
        "inventario_resumo",
        "buscar_registros",
        "ver_registro",
        "buscar_achados",
        "ver_texto_do_achado",
        "fornecedores",
        "decisoes_recentes",
        "livro_movimentacoes",
        "sugerir",
        "sugestoes_abertas",
        "telegram_conversas",
        "telegram_seguir",
        "telegram_coletar",
    ] {
        assert!(
            nomes.contains(&esperado),
            "falta a ferramenta {esperado}: {nomes:?}"
        );
    }

    let resumo = c.pedir(
        "tools/call",
        json!({"name": "inventario_resumo", "arguments": {}}),
    );
    let texto = resumo["result"]["content"][0]["text"].as_str().unwrap();
    let v: Value = serde_json::from_str(texto).unwrap();
    assert_eq!(v["registros"]["total"], 1);

    let sug = c.pedir(
        "tools/call",
        json!({"name": "sugerir", "arguments": {"sugestoes": [
            {"alvo_tipo": "registro", "alvo_id": "1", "campo": "categoria",
             "valor": "Padaria", "confianca": 0.8, "justificativa": "descricao"}]}}),
    );
    let v: Value =
        serde_json::from_str(sug["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["gravadas"], 1);

    let erro = c.pedir(
        "tools/call",
        json!({"name": "ver_registro", "arguments": {"id": 999}}),
    );
    assert_eq!(erro["result"]["isError"], true);
}

#[test]
fn sem_o_inventario_o_modo_falha_sem_criar_nada() {
    let dir = tempfile::tempdir().unwrap();
    let saida = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .args(["mcp", "--inventario"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!saida.status.success());
    assert!(!dir.path().join("inventario.sqlite").exists());
}
