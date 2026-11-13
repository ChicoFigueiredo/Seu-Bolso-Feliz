//! Apoio dos testes: cofre fictício, banco com o esquema real e rclone com registro.
#![allow(dead_code)]

use rusqlite::{Connection, params};
use sbf_estante::Rclone;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const SQL_BASE: &str = include_str!("../../../sbf-inventario/sql/0001-base-bun-v2.sql");

/// Um documento fictício do cofre de teste.
pub struct Doc<'a> {
    pub hash: &'a str,
    pub estado: &'a str,
    pub tipo: Option<&'a str>,
    pub data: Option<&'a str>,
    pub ciclo: Option<&'a str>,
    pub fornecedor: Option<&'a str>,
    pub valor: Option<i64>,
    pub ext: &'a str,
    pub conteudo: Option<&'a [u8]>,
    pub nome_original: &'a str,
}

pub fn novo_banco(caminho: &Path) -> Connection {
    let c = Connection::open(caminho).expect("abrir banco");
    c.pragma_update(None, "journal_mode", "WAL").expect("wal");
    c.execute_batch(SQL_BASE).expect("esquema");
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('f', 'disco', 'teste')",
        [],
    )
    .expect("fonte");
    for (id, ini, fim) in [
        ("2026-10", "2026-10-01", "2026-10-31"),
        ("2026-11", "2026-11-01", "2026-11-30"),
    ] {
        c.execute(
            "INSERT INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
            params![id, ini, fim],
        )
        .expect("ciclo");
    }
    c.execute("INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel) VALUES (1, 'f', 'E1', 'x', 'teste')", [])
        .expect("varredura");
    c
}

pub fn inserir(c: &Connection, cofre: &Path, d: &Doc<'_>) {
    let caminho = d.conteudo.map(|bytes| {
        let rel = format!("acervo/{}/{}.{}", &d.hash[..2], d.hash, d.ext);
        let abs = cofre.join(&rel);
        fs::create_dir_all(abs.parent().expect("pai")).expect("pasta");
        fs::write(abs, bytes).expect("arquivo");
        rel
    });
    c.execute(
        "INSERT OR REPLACE INTO achado (content_hash, file_size_bytes, extensao, camada, caminho_acervo,
           estado, document_type, supplier_name_raw, data_chave, ciclo_id, valor_centavos)
         VALUES (?1, 10, ?2, 'A', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![d.hash, d.ext, caminho, d.estado, d.tipo, d.fornecedor, d.data, d.ciclo, d.valor],
    )
    .expect("achado");
    c.execute(
        "INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, filename)
         VALUES (?1, 'f', 1, 'local_file', ?2, ?3)",
        params![d.hash, format!("k:{}", d.hash), d.nome_original],
    )
    .expect("origem");
}

/// rclone embrulhado: registra o `cmdline` de cada chamada e as variáveis `RCLONE_CONFIG_*`
/// do ambiente, e então executa o rclone de verdade.
pub struct Embrulho {
    pub rclone: Rclone,
    pub cmdlines: PathBuf,
    pub ambientes: PathBuf,
}

pub fn rclone_com_registro(pasta: &Path) -> Embrulho {
    let cmdlines = pasta.join("cmdlines.log");
    let ambientes = pasta.join("ambientes.log");
    let script = pasta.join("rclone-registrado.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             tr '\\0' ' ' < /proc/$$/cmdline >> '{c}'; echo >> '{c}'\n\
             tr '\\0' '\\n' < /proc/$$/environ | grep '^RCLONE_CONFIG_' >> '{a}'\n\
             exec /usr/bin/rclone \"$@\"\n",
            c = cmdlines.display(),
            a = ambientes.display()
        ),
    )
    .expect("script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    Embrulho {
        rclone: Rclone::com_binario(script),
        cmdlines,
        ambientes,
    }
}

/// Lista (caminho relativo, bytes) de todos os arquivos sob `raiz`, ordenada.
pub fn arvore(raiz: &Path) -> Vec<(String, Vec<u8>)> {
    let mut saida = Vec::new();
    let mut pilha = vec![raiz.to_path_buf()];
    while let Some(p) = pilha.pop() {
        let Ok(entradas) = fs::read_dir(&p) else {
            continue;
        };
        for e in entradas.flatten() {
            let caminho = e.path();
            if caminho.is_dir() {
                pilha.push(caminho);
            } else {
                let rel = caminho
                    .strip_prefix(raiz)
                    .expect("prefixo")
                    .to_string_lossy()
                    .into_owned();
                saida.push((rel, fs::read(&caminho).expect("ler")));
            }
        }
    }
    saida.sort();
    saida
}

pub fn nomes(raiz: &Path) -> Vec<String> {
    arvore(raiz).into_iter().map(|(n, _)| n).collect()
}
