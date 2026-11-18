//! O backup automático antes da primeira escrita do MCP (API de backup do SQLite, nunca
//! cópia de arquivo). Dados fictícios, em pasta temporária.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sbf_mcp::backup::{Backups, MAXIMO_DE_BACKUPS, Qual};

fn cofre_com(bancos: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for b in bancos {
        let c = Connection::open(dir.path().join(format!("{b}.sqlite"))).unwrap();
        // WAL de propósito: a cópia crua do arquivo perderia o que ainda está no -wal.
        c.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .unwrap();
        c.execute_batch(&format!(
            "CREATE TABLE t (n TEXT); INSERT INTO t VALUES ('linha de {b}');"
        ))
        .unwrap();
        // Deixa o `Connection` aberto de fora para o dado ficar no -wal.
        std::mem::forget(c);
    }
    dir
}

fn backups(dir: &Path) -> Backups {
    let mut n = 0_u32;
    // Relógio fixo e crescente: cada chamada avança um segundo.
    let relogio = std::sync::Mutex::new(move || {
        n += 1;
        format!("20261118-1200{n:02}")
    });
    Backups::novo(dir).com_relogio(move || (relogio.lock().unwrap())())
}

fn arquivos(dir: &Path) -> Vec<String> {
    let pasta = dir.join("backups").join("mcp");
    let Ok(rd) = fs::read_dir(pasta) else {
        return Vec::new();
    };
    let mut v: Vec<String> = rd
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn primeira_escrita_copia_o_banco_com_a_api_de_backup_incluindo_o_que_esta_no_wal() {
    let dir = cofre_com(&["planejamento"]);
    let b = backups(dir.path());
    b.registrar(Qual::Planejamento, dir.path().join("planejamento.sqlite"));
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    let nomes = arquivos(dir.path());
    assert_eq!(nomes, vec!["planejamento-20261118-120001.sqlite"]);
    let copia = Connection::open(
        dir.path()
            .join("backups/mcp")
            .join("planejamento-20261118-120001.sqlite"),
    )
    .unwrap();
    let texto: String = copia
        .query_row("SELECT n FROM t", [], |l| l.get(0))
        .unwrap();
    assert_eq!(texto, "linha de planejamento");
}

#[cfg(unix)]
#[test]
fn o_backup_e_privado() {
    use std::os::unix::fs::PermissionsExt;
    let dir = cofre_com(&["regras"]);
    let b = backups(dir.path());
    b.registrar(Qual::Regras, dir.path().join("regras.sqlite"));
    b.antes_de_escrever(Qual::Regras).unwrap();
    let arq = dir
        .path()
        .join("backups/mcp")
        .join(&arquivos(dir.path())[0]);
    assert_eq!(
        fs::metadata(&arq).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let pasta = fs::metadata(dir.path().join("backups/mcp")).unwrap();
    assert_eq!(pasta.permissions().mode() & 0o777, 0o700);
}

#[test]
fn so_a_primeira_escrita_de_cada_banco_na_sessao_faz_backup() {
    let dir = cofre_com(&["planejamento", "regras"]);
    let b = backups(dir.path());
    b.registrar(Qual::Planejamento, dir.path().join("planejamento.sqlite"));
    b.registrar(Qual::Regras, dir.path().join("regras.sqlite"));
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    b.antes_de_escrever(Qual::Regras).unwrap();
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    assert_eq!(arquivos(dir.path()).len(), 2);
    // Uma sessão nova (outro `Backups`) volta a copiar.
    let outra = backups(dir.path());
    outra.registrar(Qual::Regras, dir.path().join("regras.sqlite"));
    outra.antes_de_escrever(Qual::Regras).unwrap();
    assert_eq!(arquivos(dir.path()).len(), 3);
}

#[test]
fn banco_nao_configurado_nao_tem_o_que_copiar() {
    let dir = cofre_com(&[]);
    let b = backups(dir.path());
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    assert!(arquivos(dir.path()).is_empty());
}

#[test]
fn se_o_backup_falha_a_escrita_e_recusada_com_erro_claro_e_tenta_de_novo() {
    let dir = cofre_com(&[]);
    let b = backups(dir.path());
    b.registrar(Qual::Planejamento, dir.path().join("nao-existe.sqlite"));
    let erro = b.antes_de_escrever(Qual::Planejamento).unwrap_err();
    assert!(erro.contains("backup"), "{erro}");
    assert!(erro.contains("escrita foi recusada"), "{erro}");
    // Nada de arquivo parcial.
    assert!(arquivos(dir.path()).is_empty());
    // Quando o banco aparece, a próxima escrita faz o backup e segue.
    Connection::open(dir.path().join("nao-existe.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE t (n)")
        .unwrap();
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    assert_eq!(arquivos(dir.path()).len(), 1);
}

#[test]
fn guarda_so_os_vinte_mais_recentes_por_banco_e_nao_mexe_em_outros_arquivos() {
    let dir = cofre_com(&["planejamento", "regras"]);
    let pasta = dir.path().join("backups/mcp");
    fs::create_dir_all(&pasta).unwrap();
    for i in 0..25 {
        fs::write(
            pasta.join(format!("planejamento-20260101-0000{i:02}.sqlite")),
            "velho",
        )
        .unwrap();
    }
    fs::write(pasta.join("regras-20260101-000001.sqlite"), "outro banco").unwrap();
    fs::write(pasta.join("anotacao-do-dono.txt"), "meu").unwrap();
    fs::write(pasta.join("planejamento-manual.sqlite"), "meu também").unwrap();
    let b = backups(dir.path());
    b.registrar(Qual::Planejamento, dir.path().join("planejamento.sqlite"));
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    let nomes = arquivos(dir.path());
    let de_planejamento: Vec<_> = nomes
        .iter()
        .filter(|n| {
            n.starts_with("planejamento-2") // as datadas; "planejamento-manual" fica de fora
        })
        .collect();
    assert_eq!(de_planejamento.len(), MAXIMO_DE_BACKUPS);
    // Foram os mais antigos que saíram; o novo ficou.
    assert!(!nomes.contains(&"planejamento-20260101-000000.sqlite".to_owned()));
    assert!(!nomes.contains(&"planejamento-20260101-000005.sqlite".to_owned()));
    assert!(nomes.contains(&"planejamento-20260101-000006.sqlite".to_owned()));
    assert!(nomes.contains(&"planejamento-20261118-120001.sqlite".to_owned()));
    // O que não é backup deste banco não é tocado.
    assert!(nomes.contains(&"regras-20260101-000001.sqlite".to_owned()));
    assert!(nomes.contains(&"anotacao-do-dono.txt".to_owned()));
    assert!(nomes.contains(&"planejamento-manual.sqlite".to_owned()));
}

#[test]
fn fazer_backup_copia_todos_os_bancos_do_cofre_sob_demanda_mesmo_na_mesma_sessao() {
    let dir = cofre_com(&["inventario", "planejamento", "regras"]);
    let b = backups(dir.path());
    b.registrar(Qual::Inventario, dir.path().join("inventario.sqlite"));
    b.registrar(Qual::Planejamento, dir.path().join("planejamento.sqlite"));
    b.antes_de_escrever(Qual::Planejamento).unwrap();
    let r = b.fazer_todos().unwrap();
    let feitos = r["feitos"].as_array().unwrap();
    let bancos: Vec<&str> = feitos
        .iter()
        .map(|f| f["banco"].as_str().unwrap())
        .collect();
    assert_eq!(bancos, vec!["inventario", "planejamento", "regras"]);
    // 1 da primeira escrita + 3 sob demanda.
    assert_eq!(arquivos(dir.path()).len(), 4);
    assert_eq!(r["mantidos_por_banco"], MAXIMO_DE_BACKUPS);
    // Só caminhos relativos ao cofre, nunca o conteúdo.
    assert!(r["pasta"].as_str().unwrap().ends_with("backups/mcp"));
}

#[test]
fn dois_backups_no_mesmo_segundo_nao_se_sobrescrevem() {
    let dir = cofre_com(&["regras"]);
    let b = Backups::novo(dir.path()).com_relogio(|| "20261118-120000".to_owned());
    b.registrar(Qual::Regras, dir.path().join("regras.sqlite"));
    b.fazer(&dir.path().join("regras.sqlite")).unwrap();
    b.fazer(&dir.path().join("regras.sqlite")).unwrap();
    assert_eq!(
        arquivos(dir.path()),
        vec![
            "regras-20261118-120000-2.sqlite",
            "regras-20261118-120000.sqlite"
        ]
    );
}

#[allow(dead_code)]
fn _usa(_: PathBuf) {}
