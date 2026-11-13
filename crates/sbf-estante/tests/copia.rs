//! INV-12 (ida e volta pela cópia cifrada) e a senha fora da linha de comando.

#![cfg(unix)]

mod comum;

use comum::{Doc, arvore, inserir, nomes, novo_banco, rclone_com_registro};
use sbf_estante::{CopiaDeSeguranca, Erro, Rclone, Segredo};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

const SENHA: &str = "senha-ficticia-do-teste-Zq7!";

fn cofre_ficticio() -> (tempfile::TempDir, rusqlite::Connection) {
    let cofre = tempfile::tempdir().expect("cofre");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    for (hash, conteudo) in [
        ("aaaaaaaa11111111", b"%PDF-1.4 fatura ficticia".as_slice()),
        (
            "bbbbbbbb22222222",
            b"%PDF-1.7 /Encrypt protegido ficticio".as_slice(),
        ),
    ] {
        inserir(
            &c,
            cofre.path(),
            &Doc {
                hash,
                estado: "documento",
                tipo: Some("fatura"),
                data: Some("2026-10-25"),
                ciclo: Some("2026-10"),
                fornecedor: Some("Loja Ficticia"),
                valor: Some(1000),
                ext: "pdf",
                conteudo: Some(conteudo),
                nome_original: "arquivo-original-legivel.pdf",
            },
        );
    }
    (cofre, c)
}

fn banco_logico(c: &rusqlite::Connection) -> Vec<(String, String)> {
    let mut s = c
        .prepare("SELECT content_hash, estado FROM achado ORDER BY content_hash")
        .expect("preparar");
    s.query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .expect("consulta")
        .map(|l| l.expect("linha"))
        .collect()
}

#[test]
fn ida_e_volta_reproduz_acervo_e_banco_com_o_mesmo_hash() {
    let (cofre, c) = cofre_ficticio();
    // Um escritor ainda com dados só no WAL: a cópia precisa incluí-los.
    c.execute(
        "UPDATE achado SET notas = 'so no wal' WHERE content_hash = 'aaaaaaaa11111111'",
        [],
    )
    .expect("update");
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);

    let rel = copia.fazer(&c, cofre.path(), &senha).expect("fazer");
    assert_eq!(rel.arquivos_do_acervo, 2);
    assert_eq!(rel.banco_sha256.len(), 64);

    // O destino por baixo da cripto não tem nome nem conteúdo legível.
    for (nome, bytes) in arvore(remoto.path()) {
        for proibido in ["acervo", "banco", "inventario", "aaaaaaaa", "pdf", "sqlite"] {
            assert!(!nome.contains(proibido), "nome legível no destino: {nome}");
        }
        assert!(!String::from_utf8_lossy(&bytes).contains("fatura ficticia"));
        assert!(!bytes.starts_with(b"SQLite format 3"));
    }
    // Nenhum WAL foi copiado: só o acervo e um banco cifrados (nomes opacos).
    assert_eq!(instantaneos_da(&copia, &senha), vec![rel.banco.clone()]);

    // Restaura numa pasta nova.
    let volta = tempfile::tempdir().expect("volta");
    let alvo = volta.path().join("cofre-restaurado");
    let r = copia.restaurar(&senha, &alvo).expect("restaurar");
    assert_eq!(r.banco, rel.banco);
    assert_eq!(r.banco_sha256, rel.banco_sha256, "banco com o mesmo hash");
    assert_eq!(r.arquivos_do_acervo, 2);
    assert_eq!(
        arvore(&cofre.path().join("acervo")),
        arvore(&alvo.join("acervo")),
        "acervo igual"
    );

    // O banco restaurado tem o conteúdo, inclusive o que estava só no WAL.
    let restaurado = rusqlite::Connection::open(alvo.join("inventario.sqlite")).expect("abrir");
    assert_eq!(banco_logico(&restaurado), banco_logico(&c));
    let nota: String = restaurado
        .query_row(
            "SELECT notas FROM achado WHERE content_hash = 'aaaaaaaa11111111'",
            [],
            |l| l.get(0),
        )
        .expect("nota");
    assert_eq!(nota, "so no wal");
    assert!(!alvo.join("inventario.sqlite-wal").exists());

    // Não sobrescreve.
    assert!(matches!(
        copia.restaurar(&senha, &alvo),
        Err(Erro::JaExiste(_))
    ));
}

fn instantaneos_da(copia: &CopiaDeSeguranca, senha: &Segredo) -> Vec<String> {
    copia.instantaneos(senha).expect("instantâneos")
}

#[test]
fn senha_errada_nao_restaura_nada() {
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    copia
        .fazer(&c, cofre.path(), &Segredo::novo(SENHA))
        .expect("fazer");

    let volta = tempfile::tempdir().expect("volta");
    let errada = Segredo::novo("outra-senha-qualquer");
    assert!(matches!(
        copia.restaurar(&errada, &volta.path().join("x")),
        Err(Erro::SemInstantaneo)
    ));
    assert!(matches!(
        copia.fazer(&c, cofre.path(), &Segredo::novo("")),
        Err(Erro::SenhaVazia)
    ));
    assert!(nomes(volta.path()).is_empty());
}

#[test]
fn copias_seguintes_acrescentam_e_a_restauracao_usa_a_mais_nova() {
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);
    copia.fazer(&c, cofre.path(), &senha).expect("primeira");
    // O nome do instantâneo carrega segundos; garante um nome diferente.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inserir(
        &c,
        cofre.path(),
        &Doc {
            hash: "cccccccc33333333",
            estado: "novo",
            tipo: None,
            data: None,
            ciclo: None,
            fornecedor: None,
            valor: None,
            ext: "pdf",
            conteudo: Some(b"terceiro"),
            nome_original: "t.pdf",
        },
    );
    let segunda = copia.fazer(&c, cofre.path(), &senha).expect("segunda");
    assert_eq!(instantaneos_da(&copia, &senha).len(), 2);

    let volta = tempfile::tempdir().expect("volta");
    let r = copia
        .restaurar(&senha, &volta.path().join("c"))
        .expect("restaurar");
    assert_eq!(r.banco, segunda.banco);
    assert_eq!(r.arquivos_do_acervo, 3);
}

/// A senha nunca aparece na linha de comando de nenhum processo rclone, nem no que o
/// processo ao lado enxerga em `/proc/<pid>/cmdline`.
#[test]
fn senha_da_cripto_nao_aparece_no_cmdline_do_rclone() {
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    let registro = tempfile::tempdir().expect("registro");
    let embrulho = rclone_com_registro(registro.path());
    let copia = CopiaDeSeguranca::nova(embrulho.rclone.clone(), remoto.path().to_string_lossy())
        .expect("copia");
    let senha = Segredo::novo(SENHA);

    // Em paralelo, varre /proc procurando a senha em qualquer cmdline.
    let parar = AtomicBool::new(false);
    let vazou = AtomicBool::new(false);
    let vistos = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|escopo| {
        escopo.spawn(|| {
            while !parar.load(Ordering::Relaxed) {
                if let Ok(entradas) = fs::read_dir("/proc") {
                    for e in entradas.flatten() {
                        let Ok(cmd) = fs::read(e.path().join("cmdline")) else {
                            continue;
                        };
                        let texto = String::from_utf8_lossy(&cmd);
                        // Só processos rclone (ou o embrulho dele): o shell que roda o teste
                        // pode ter o texto do teste na própria linha de comando.
                        let e_rclone = texto
                            .split('\0')
                            .take(2)
                            .any(|a| a.ends_with("rclone") || a.ends_with("rclone-registrado.sh"));
                        if !e_rclone {
                            continue;
                        }
                        vistos.fetch_add(1, Ordering::Relaxed);
                        if texto.contains(SENHA) {
                            eprintln!("VAZOU em {:?}: {}", e.path(), texto.replace('\0', " "));
                            vazou.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }
        });
        copia.fazer(&c, cofre.path(), &senha).expect("fazer");
        let volta = tempfile::tempdir().expect("volta");
        copia
            .restaurar(&senha, &volta.path().join("c"))
            .expect("restaurar");
        parar.store(true, Ordering::Relaxed);
    });
    assert!(
        !vazou.load(Ordering::Relaxed),
        "a senha apareceu no cmdline de algum processo"
    );

    // O registro determinístico: cada chamada ao rclone deixou o cmdline dela.
    let cmdlines = fs::read_to_string(&embrulho.cmdlines).expect("cmdlines");
    assert!(cmdlines.lines().count() >= 6, "{cmdlines}");
    assert!(
        cmdlines.contains("obscure -"),
        "a senha entra pela entrada padrão: {cmdlines}"
    );
    assert!(!cmdlines.contains(SENHA), "{cmdlines}");
    // A senha também não está ofuscada em argumento nenhum.
    let ambientes = fs::read_to_string(&embrulho.ambientes).expect("ambientes");
    let ofuscada = ambientes
        .lines()
        .find_map(|l| l.strip_prefix("RCLONE_CONFIG_SBFCOPIA_PASSWORD="))
        .expect("a senha vai no ambiente do filho");
    assert!(!ofuscada.is_empty() && ofuscada != SENHA);
    assert!(
        !cmdlines.contains(ofuscada),
        "a senha ofuscada vazou para o cmdline"
    );
    // A chave do ambiente existe e o resto da configuração também.
    assert!(ambientes.contains("RCLONE_CONFIG_SBFCOPIA_TYPE=crypt"));
}
