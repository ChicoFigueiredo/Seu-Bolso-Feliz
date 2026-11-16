//! Coleta de nuvem ponta a ponta com um remoto `alias` sobre pasta temporária (nunca uma
//! nuvem real, nunca a configuração do usuário).

#![cfg(unix)]

use rusqlite::Connection;
use sbf_coleta::nuvem::{ColetaDeNuvem, Fase, Nuvem, PastasDoCofre};
use sbf_estante::{Conexoes, Rclone};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

struct Cenario {
    _pasta: tempfile::TempDir,
    nuvem: std::path::PathBuf,
    cofre: std::path::PathBuf,
    espelho: std::path::PathBuf,
    rclone: Rclone,
    banco: Connection,
}

fn cenario() -> Option<Cenario> {
    if Rclone::novo().binario().as_os_str().is_empty() {
        return None;
    }
    let pasta = tempfile::tempdir().expect("pasta");
    let nuvem = pasta.path().join("nuvem");
    let cofre = pasta.path().join("cofre");
    let espelho = pasta.path().join("espelho");
    for d in [&nuvem, &cofre, &espelho] {
        fs::create_dir_all(d).expect("dir");
    }
    let rclone = Rclone::novo().com_configuracao(pasta.path().join("rclone.conf"));
    let c = Conexoes::nova(rclone.clone()).expect("conexoes");
    c.criar_remoto("falso", "alias", &[&format!("remote={}", nuvem.display())])
        .expect("alias");
    let banco = Connection::open(cofre.join("inventario.sqlite")).expect("banco");
    banco.execute_batch(SQL_BASE).expect("esquema");
    Some(Cenario {
        _pasta: pasta,
        nuvem,
        cofre,
        espelho,
        rclone,
        banco,
    })
}

fn escrever(raiz: &Path, rel: &str, conteudo: &str) {
    let alvo = raiz.join(rel);
    fs::create_dir_all(alvo.parent().expect("pai")).expect("pai");
    fs::write(alvo, conteudo).expect("escrever");
}

fn coleta<'a>(c: &'a Cenario, so_listar: bool) -> ColetaDeNuvem<'a> {
    let nuvem = Nuvem::onedrive().com_espelhos(vec![c.espelho.clone()]);
    let k = ColetaDeNuvem::nova(&c.rclone, nuvem, "falso:", PastasDoCofre::de(&c.cofre));
    if so_listar { k.so_listando() } else { k }
}

fn contar(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |l| l.get(0)).expect("contar")
}

#[test]
fn lista_ignora_sbf_baixa_so_candidatos_e_e_incremental() {
    let Some(c) = cenario() else { return };
    escrever(&c.nuvem, "Contas/fatura-luz.pdf", "pdf fictício 1");
    escrever(&c.nuvem, "Contas/extrato.ofx", "ofx");
    escrever(&c.nuvem, "Fotos/ferias.jpg", "jpg");
    escrever(&c.nuvem, "Musica/faixa.mp3", "mp3");
    escrever(&c.nuvem, "SBF/Estante/2026-10/fatura/x.pdf", "do app");
    escrever(&c.nuvem, "SBF/Copia-de-seguranca/y.pdf", "cifrada");

    let fases = std::sync::Mutex::new(Vec::new());
    let r = coleta(&c, false)
        .coletar(&c.banco, &AtomicBool::new(false), &|a| {
            fases.lock().expect("fases").push(a.fase);
        })
        .expect("coletar");
    assert_eq!(r.listados, 4, "SBF/ nem entra: {r:?}");
    // Espelho fora das regras do dono: camada B; pdf e ofx são documentos; jpg é foto contada.
    assert_eq!((r.candidatos, r.fora_de_escopo), (2, 2), "{r:?}");
    assert_eq!((r.a_baixar, r.baixados), (2, 2), "{r:?}");
    let entrada = c.cofre.join("entrada/onedrive");
    assert!(entrada.join("Contas/fatura-luz.pdf").is_file());
    assert!(entrada.join("Contas/extrato.ofx").is_file());
    assert!(!entrada.join("Fotos/ferias.jpg").exists());
    assert!(!entrada.join("Musica").exists());
    assert!(!entrada.join("SBF").exists());
    assert_eq!(contar(&c.banco, "SELECT count(*) FROM origem"), 2);
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM origem WHERE mensagem_ref LIKE '%SBF%'"
        ),
        0
    );
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM varredura WHERE status = 'concluida'"
        ),
        1
    );
    let fases = fases.lock().expect("fases");
    assert!(fases.contains(&Fase::Listando) && fases.contains(&Fase::Baixando));
    // A listagem guardada é a do rclone, com os hashes pedidos.
    let listagem = fs::read_dir(c.cofre.join("entrada"))
        .expect("entrada")
        .filter_map(Result::ok)
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("onedrive-lista-")
        })
        .expect("listagem");
    assert!(
        fs::read_to_string(listagem.path())
            .expect("ler")
            .contains("Hashes")
    );

    // O scanner (outra etapa) indexa o baixado; depois a origem da nuvem herda o conteúdo.
    c.banco
        .execute_batch(&format!(
            "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES ('h1', 14, 'B');
             INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath, filename)
               VALUES ('h1', 'nuvem:onedrive', 1, 'local_file', 'local:a', '{}/Contas/fatura-luz.pdf', 'fatura-luz.pdf');",
            entrada.display()
        ))
        .expect("scan");
    assert_eq!(coleta(&c, false).ligar_hashes(&c.banco).expect("ligar"), 1);
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM origem WHERE origin_type = 'onedrive' AND content_hash = 'h1'"
        ),
        1
    );

    // Segunda rodada sem mudança: nada novo a baixar; o ligado é reconhecido.
    let r2 = coleta(&c, false)
        .coletar(&c.banco, &AtomicBool::new(false), &|_| {})
        .expect("de novo");
    assert_eq!(r2.baixados, 0, "{r2:?}");
    assert_eq!(r2.ja_locais, 1, "{r2:?}");
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM origem WHERE origin_type = 'onedrive'"
        ),
        2
    );

    // Arquivo novo na nuvem: só ele é baixado.
    escrever(&c.nuvem, "Contas/boleto.pdf", "novo");
    let r3 = coleta(&c, false)
        .coletar(&c.banco, &AtomicBool::new(false), &|_| {})
        .expect("terceira");
    assert_eq!(r3.baixados, 1, "{r3:?}");
    assert!(entrada.join("Contas/boleto.pdf").is_file());
}

#[test]
fn candidato_ja_local_so_liga_a_origem_sem_baixar() {
    let Some(c) = cenario() else { return };
    escrever(&c.nuvem, "Contas/fatura.pdf", "abcde");
    escrever(&c.espelho, "Contas/fatura.pdf", "vwxyz"); // mesmo caminho e tamanho
    let local = c.espelho.join("Contas/fatura.pdf");
    c.banco
        .execute_batch(&format!(
            "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:x', 'disco', 'x');
             INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES ('disco:x', 'E1', '/', 't');
             INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES ('hlocal', 5, 'B');
             INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath, filename)
               VALUES ('hlocal', 'disco:x', 1, 'local_file', 'local:b', '{}', 'fatura.pdf');",
            local.display()
        ))
        .expect("scan");
    let r = coleta(&c, false)
        .coletar(&c.banco, &AtomicBool::new(false), &|_| {})
        .expect("coletar");
    assert_eq!((r.ja_locais, r.a_baixar, r.baixados), (1, 0, 0), "{r:?}");
    assert!(!c.cofre.join("entrada/onedrive/Contas").exists());
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM origem WHERE origin_type = 'onedrive' AND content_hash = 'hlocal'"
        ),
        1
    );
}

#[test]
fn so_listar_nao_baixa_e_cancelar_interrompe_a_varredura() {
    let Some(c) = cenario() else { return };
    escrever(&c.nuvem, "a/um.pdf", "1");
    let r = coleta(&c, true)
        .coletar(&c.banco, &AtomicBool::new(false), &|_| {})
        .expect("listar");
    assert_eq!((r.a_baixar, r.baixados), (1, 0));
    assert!(!c.cofre.join("entrada/onedrive/a").exists());
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM varredura WHERE status = 'interrompida'"
        ),
        1
    );

    let cancelar = AtomicBool::new(false);
    let r = coleta(&c, false)
        .coletar(&c.banco, &cancelar, &|a| {
            if a.fase == Fase::Listando {
                cancelar.store(true, Ordering::Relaxed);
            }
        })
        .expect("cancelada");
    assert!(r.interrompida && r.baixados == 0, "{r:?}");
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM varredura WHERE status = 'interrompida'"
        ),
        2
    );
}

#[test]
fn falha_do_remoto_nao_deixa_varredura_rodando() {
    let Some(c) = cenario() else { return };
    let nuvem = Nuvem::onedrive().com_espelhos(vec![c.espelho.clone()]);
    let k = ColetaDeNuvem::nova(
        &c.rclone,
        nuvem,
        "inexistente:",
        PastasDoCofre::de(&c.cofre),
    );
    assert!(
        k.coletar(&c.banco, &AtomicBool::new(false), &|_| {})
            .is_err()
    );
    assert_eq!(
        contar(
            &c.banco,
            "SELECT count(*) FROM varredura WHERE status = 'rodando'"
        ),
        0
    );
}
