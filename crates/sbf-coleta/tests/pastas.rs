//! Varredura e vigia de pastas sobre árvores temporárias fictícias (nunca o cofre real).

#![cfg(unix)]

use rusqlite::Connection;
use sbf_coleta::pastas::{
    Contadores, Destino, Opcoes, Varredor, Vigia, abrir_varredura_do_vigia, indexar_arquivo,
    iso_de, mtime_ms,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

struct Cenario {
    _pasta: tempfile::TempDir,
    raiz: PathBuf,
    cofre: PathBuf,
    conn: Connection,
}

fn cenario() -> Cenario {
    let pasta = tempfile::tempdir().expect("pasta");
    let raiz = pasta.path().join("disco");
    let cofre = pasta.path().join("cofre");
    fs::create_dir_all(&raiz).expect("raiz");
    fs::create_dir_all(&cofre).expect("cofre");
    let conn = Connection::open(cofre.join("inventario.sqlite")).expect("banco");
    conn.execute_batch(SQL_BASE).expect("esquema");
    Cenario {
        _pasta: pasta,
        raiz,
        cofre,
        conn,
    }
}

fn escrever(raiz: &Path, rel: &str, conteudo: &str) {
    let alvo = raiz.join(rel);
    fs::create_dir_all(alvo.parent().expect("pai")).expect("pai");
    fs::write(alvo, conteudo).expect("escrever");
}

fn opcoes(c: &Cenario, entrada: bool) -> Opcoes {
    Opcoes {
        cofre: c.cofre.clone(),
        entrada,
        ..Opcoes::default()
    }
}

fn varrer(c: &Cenario, entrada: bool) -> Contadores {
    let mut v = Varredor::novo(&c.conn, "disco:teste", &c.raiz, opcoes(c, entrada)).expect("v");
    assert!(v.varrer(&AtomicBool::new(false)).expect("varrer"));
    v.finalizar("concluida").expect("fim")
}

fn contar(c: &Cenario, sql: &str) -> i64 {
    c.conn.query_row(sql, [], |l| l.get(0)).expect("contar")
}

#[test]
fn entrada_vira_area_a_com_copia_ao_cofre_e_reexecutar_nao_repete() {
    let c = cenario();
    escrever(&c.raiz, "fatura-luz.pdf", "conteudo ficticio um");
    escrever(&c.raiz, "sub/extrato.csv", "conteudo ficticio dois");
    escrever(&c.raiz, "som.mp3", "nao e candidato");
    escrever(&c.raiz, "vazio.pdf", "");
    let r1 = varrer(&c, true);
    assert_eq!(
        (r1.novos, r1.copiados, r1.conteudos_novos),
        (2, 2, 2),
        "{r1:?}"
    );
    assert_eq!(r1.vazios, 1);
    assert_eq!(
        contar(&c, "SELECT COUNT(*) FROM achado WHERE camada = 'A'"),
        2
    );
    let copia: String = c
        .conn
        .query_row(
            "SELECT caminho_acervo FROM achado WHERE extensao = 'pdf'",
            [],
            |l| l.get(0),
        )
        .expect("caminho");
    assert!(
        copia.starts_with("acervo/") && copia.ends_with(".pdf"),
        "{copia}"
    );
    assert_eq!(
        fs::read_to_string(c.cofre.join(&copia)).expect("copia"),
        "conteudo ficticio um"
    );
    let r2 = varrer(&c, true);
    assert_eq!((r2.novos, r2.repetidos), (0, 2), "{r2:?}");
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM origem"), 2);
    // A segunda varredura nasceu e a primeira terminou.
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM varredura WHERE status = 'concluida'"
        ),
        2
    );
}

#[test]
fn disco_comum_e_b_hash_sem_copia_foto_contada_e_pastas_de_sistema_fora() {
    let c = cenario();
    escrever(&c.raiz, "contas/boleto.pdf", "boleto ficticio");
    escrever(&c.raiz, "fotos/praia.jpg", "foto qualquer");
    escrever(&c.raiz, "fotos/comprovante-pix.jpg", "foto de comprovante");
    escrever(
        &c.raiz,
        "proj/node_modules/pacote/nota-fiscal.pdf",
        "nome financeiro resgatado",
    );
    escrever(&c.raiz, "proj/node_modules/pacote/lixo.js", "codigo");
    escrever(&c.raiz, "Daiane/ir/relatorio.pdf", "de outra pessoa");
    let r = varrer(&c, false);
    // B: o boleto e o comprovante em foto (nome financeiro) entram; a foto comum é contada.
    assert_eq!(r.fotos_contadas, 1, "{r:?}");
    assert_eq!(r.copiados, 0, "área B não copia: {r:?}");
    let hashes: Vec<String> = c
        .conn
        .prepare("SELECT filename FROM origem ORDER BY filename")
        .and_then(|mut q| q.query_map([], |l| l.get(0))?.collect())
        .expect("nomes");
    assert!(hashes.contains(&"boleto.pdf".to_owned()));
    assert!(hashes.contains(&"comprovante-pix.jpg".to_owned()));
    assert!(
        hashes.contains(&"nota-fiscal.pdf".to_owned()),
        "resgate em C: {hashes:?}"
    );
    assert!(!hashes.contains(&"praia.jpg".to_owned()));
    // C: contado por motivo, o arquivo de outra pessoa não vira origem.
    let motivos: Vec<String> = c
        .conn
        .prepare("SELECT motivo FROM area_excluida ORDER BY motivo")
        .and_then(|mut q| q.query_map([], |l| l.get(0))?.collect())
        .expect("motivos");
    assert!(motivos.contains(&"outra-pessoa".to_owned()), "{motivos:?}");
    assert!(motivos.contains(&"sistema".to_owned()), "{motivos:?}");
    assert!(
        motivos.contains(&"fotos-fora-de-area-pessoal".to_owned()),
        "{motivos:?}"
    );
    assert!(
        !hashes.contains(&"relatorio.pdf".to_owned()),
        "outra pessoa não entra"
    );
}

#[test]
fn o_cofre_nunca_e_varrido_e_links_nao_sao_seguidos() {
    let c = cenario();
    // O cofre dentro da raiz varrida.
    let raiz = c.raiz.clone();
    let cofre_dentro = raiz.join("meu-cofre");
    fs::create_dir_all(&cofre_dentro).expect("cofre dentro");
    escrever(&cofre_dentro, "acervo/ab/x.pdf", "segredo do cofre");
    escrever(&raiz, "doc.pdf", "documento");
    std::os::unix::fs::symlink(&cofre_dentro, raiz.join("atalho")).expect("link");
    let mut v = Varredor::novo(
        &c.conn,
        "disco:teste",
        &raiz,
        Opcoes {
            cofre: cofre_dentro,
            ..Opcoes::default()
        },
    )
    .expect("v");
    v.varrer(&AtomicBool::new(false)).expect("varrer");
    let r = v.finalizar("concluida").expect("fim");
    assert_eq!(r.novos, 1, "{r:?}");
    assert_eq!(r.symlinks, 1);
    assert_eq!(
        contar(&c, "SELECT COUNT(*) FROM origem WHERE filename = 'x.pdf'"),
        0
    );
}

#[test]
fn varredura_cancelada_para_e_a_proxima_continua() {
    let c = cenario();
    for i in 0..5 {
        escrever(&c.raiz, &format!("d{i}/f.pdf"), &format!("arquivo {i}"));
    }
    let mut v = Varredor::novo(&c.conn, "disco:teste", &c.raiz, opcoes(&c, true)).expect("v");
    assert!(!v.varrer(&AtomicBool::new(true)).expect("cancelada"));
    v.finalizar("interrompida").expect("fim");
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM varredura WHERE status = 'interrompida'"
        ),
        1
    );
    let r = varrer(&c, true);
    assert_eq!(r.novos, 5);
}

#[derive(Deserialize)]
struct OuroMtime {
    segundos: f64,
    #[serde(rename = "mtimeMs")]
    mtime_ms: String,
    iso: String,
}

#[test]
fn chave_e_iso_do_mtime_batem_com_o_typescript() {
    let ouro: Vec<OuroMtime> =
        serde_json::from_str(include_str!("fixtures/pastas-ts.json")).expect("ouro");
    assert!(ouro.len() >= 8);
    let pasta = tempfile::tempdir().expect("pasta");
    for (i, o) in ouro.iter().enumerate() {
        let f = pasta.path().join(format!("m{i}.txt"));
        fs::write(&f, "x").expect("f");
        let quando = UNIX_EPOCH + Duration::from_secs_f64(o.segundos);
        fs::File::options()
            .write(true)
            .open(&f)
            .expect("abrir")
            .set_modified(quando)
            .expect("mtime");
        let meta = fs::metadata(&f).expect("meta");
        assert_eq!(
            format!("{}", mtime_ms(&meta)),
            o.mtime_ms,
            "mtimeMs de {}",
            o.segundos
        );
        assert_eq!(iso_de(meta.modified().expect("m")), o.iso);
    }
}

#[test]
fn vigia_avisa_e_indexa_um_arquivo_novo_com_as_mesmas_regras() {
    let c = cenario();
    let vigia = Vigia::iniciar(std::slice::from_ref(&c.raiz)).expect("vigia");
    assert_eq!(vigia.vigiadas.len(), 1);
    let varredura = abrir_varredura_do_vigia(&c.conn, "disco:teste", &c.raiz).expect("varredura");
    // Reaproveita a mesma varredura aberta.
    assert_eq!(
        abrir_varredura_do_vigia(&c.conn, "disco:teste", &c.raiz).expect("outra"),
        varredura
    );
    escrever(&c.raiz, "novo/boleto-agua.pdf", "boleto novo ficticio");
    escrever(&c.raiz, "novo/musica.mp3", "som");
    let mut avisados = Vec::new();
    for _ in 0..30 {
        avisados.extend(vigia.mudancas(Duration::from_millis(200)));
        if avisados.iter().any(|p| p.ends_with("boleto-agua.pdf")) {
            break;
        }
    }
    assert!(
        avisados.iter().any(|p| p.ends_with("boleto-agua.pdf")),
        "o notify não avisou: {avisados:?}"
    );
    let mut cont = Contadores::default();
    for p in &avisados {
        indexar_arquivo(&c.conn, &c.cofre, "disco:teste", varredura, p, &mut cont)
            .expect("indexar");
    }
    assert_eq!(cont.novos, 1, "{cont:?}");
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM origem WHERE filename = 'boleto-agua.pdf'"
        ),
        1
    );
    // Avisar duas vezes não duplica.
    let p = c.raiz.join("novo/boleto-agua.pdf");
    let d = indexar_arquivo(&c.conn, &c.cofre, "disco:teste", varredura, &p, &mut cont)
        .expect("de novo");
    assert_eq!(d, Destino::Repetido);
    // Arquivo que sumiu entre o aviso e a leitura é ignorado, não é erro.
    let d = indexar_arquivo(
        &c.conn,
        &c.cofre,
        "disco:teste",
        varredura,
        &c.raiz.join("sumiu.pdf"),
        &mut cont,
    )
    .expect("sumiu");
    assert_eq!(d, Destino::Ignorado);
}

#[test]
fn origem_gravada_pelo_bun_com_mtime_divergente_nao_ganha_segunda_origem() {
    // Fracoes problematicas: .5, .9999 e valores grandes. A chave do Bun pode diferir
    // na ultima casa do mtimeMs; o caminho, o tamanho e o mtime em ms ainda casam.
    for segundos in [
        1_700_000_000.5_f64,
        1_700_000_000.9999,
        4_102_444_799.123,
    ] {
        let c = cenario();
        escrever(&c.raiz, "fatura-luz.pdf", "conteudo ficticio um");
        let alvo = c.raiz.join("fatura-luz.pdf");
        fs::File::options()
            .write(true)
            .open(&alvo)
            .expect("abrir")
            .set_modified(UNIX_EPOCH + Duration::from_secs_f64(segundos))
            .expect("mtime");
        let r1 = varrer(&c, true);
        assert_eq!(r1.novos, 1, "{r1:?}");
        // Simula a linha gravada pelo Bun: mesma origem, ultima casa do mtimeMs diferente.
        let n = c
            .conn
            .execute("UPDATE origem SET origin_key = origin_key || '1'", [])
            .expect("bun");
        assert_eq!(n, 1);
        let r2 = varrer(&c, true);
        assert_eq!((r2.novos, r2.repetidos), (0, 1), "{segundos}: {r2:?}");
        assert_eq!(contar(&c, "SELECT COUNT(*) FROM origem"), 1, "{segundos}");
    }
}
