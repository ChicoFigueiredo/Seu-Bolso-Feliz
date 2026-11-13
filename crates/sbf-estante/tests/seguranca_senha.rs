//! Achado A2 da revisão de segurança da fatia 1: a senha da cópia de segurança.
//!
//! - o app gera uma senha forte ([`gerar_senha_da_copia`]) para o chaveiro guardar;
//! - uma senha escolhida abaixo de [`MINIMO_DA_SENHA_ESCOLHIDA`] é recusada;
//! - cada cópia tem um sal (`PASSWORD2` do rclone crypt) aleatório, nunca o padrão do rclone,
//!   guardado em claro ao lado da cópia, para a restauração precisar só da senha.
//!
//! Cofre, banco e remoto fictícios, gerados aqui.

#![cfg(unix)]

mod comum;

use comum::{Doc, inserir, nomes, novo_banco, rclone_com_registro};
use sbf_estante::copia::{MINIMO_DA_SENHA_ESCOLHIDA, NOME_DO_SAL, gerar_senha_da_copia};
use sbf_estante::{CopiaDeSeguranca, Erro, Rclone, Segredo};
use std::collections::HashSet;
use std::fs;

const SENHA: &str = "senha-ficticia-do-teste-Zq7!";

fn cofre_ficticio() -> (tempfile::TempDir, rusqlite::Connection) {
    let cofre = tempfile::tempdir().expect("cofre");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(
        &c,
        cofre.path(),
        &Doc {
            hash: "aaaaaaaa11111111",
            estado: "documento",
            tipo: Some("fatura"),
            data: Some("2026-10-25"),
            ciclo: Some("2026-10"),
            fornecedor: Some("Loja Ficticia"),
            valor: Some(1000),
            ext: "pdf",
            conteudo: Some(b"%PDF-1.4 fatura ficticia"),
            nome_original: "a.pdf",
        },
    );
    (cofre, c)
}

#[test]
fn senha_gerada_e_forte_e_nunca_se_repete() {
    let mut vistas = HashSet::new();
    for _ in 0..50 {
        let senha = gerar_senha_da_copia().expect("gerar");
        let texto = senha.expor();
        let simbolos: Vec<char> = texto.chars().filter(|c| *c != '-').collect();
        // 30 símbolos de um alfabeto de 32: 150 bits.
        assert_eq!(simbolos.len(), 30, "{}", texto.len());
        assert!(
            simbolos
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "só minúsculas e dígitos, fácil de copiar num papel"
        );
        assert!(
            !simbolos.iter().any(|c| matches!(c, 'l' | 'o' | '0' | '1')),
            "sem caracteres que se confundem"
        );
        assert!(texto.chars().count() >= MINIMO_DA_SENHA_ESCOLHIDA);
        assert!(vistas.insert(texto.to_owned()), "senha repetida");
    }
    // Em 50 senhas, todos os 32 símbolos aparecem (a chance de faltar um é desprezível).
    let usados: HashSet<char> = vistas.iter().flat_map(|s| s.chars()).collect();
    assert_eq!(usados.len(), 33, "32 símbolos e o hífen");
}

#[test]
fn senha_escolhida_curta_e_recusada_antes_do_rclone() {
    assert_eq!(MINIMO_DA_SENHA_ESCOLHIDA, 16);
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    let registro = tempfile::tempdir().expect("registro");
    let embrulho = rclone_com_registro(registro.path());
    let copia = CopiaDeSeguranca::nova(embrulho.rclone.clone(), remoto.path().to_string_lossy())
        .expect("copia");
    let curta = Segredo::novo("quinze-chars-ab"); // 15
    assert_eq!(curta.expor().chars().count(), 15);
    assert!(matches!(
        copia.fazer(&c, cofre.path(), &curta),
        Err(Erro::SenhaCurta { minimo: 16 })
    ));
    assert!(matches!(
        copia.restaurar(&curta, &remoto.path().join("x")),
        Err(Erro::SenhaCurta { .. })
    ));
    assert!(matches!(
        copia.fazer(&c, cofre.path(), &Segredo::novo("")),
        Err(Erro::SenhaVazia)
    ));
    // Contam caracteres, não bytes: 15 letras acentuadas são curtas.
    assert!(matches!(
        copia.fazer(&c, cofre.path(), &Segredo::novo("ççççççççççççççç")),
        Err(Erro::SenhaCurta { .. })
    ));
    assert!(!embrulho.cmdlines.exists(), "o rclone nem foi chamado");
    assert!(nomes(remoto.path()).is_empty());
}

/// O `PASSWORD2` existe, é diferente em cada destino, e a cópia funciona com a senha gerada.
#[test]
fn cada_copia_tem_sal_proprio_e_a_volta_usa_so_a_senha() {
    let (cofre, c) = cofre_ficticio();
    let registro = tempfile::tempdir().expect("registro");
    let embrulho = rclone_com_registro(registro.path());
    let senha = gerar_senha_da_copia().expect("gerar");

    let mut sais = Vec::new();
    for _ in 0..2 {
        let remoto = tempfile::tempdir().expect("remoto");
        let copia =
            CopiaDeSeguranca::nova(embrulho.rclone.clone(), remoto.path().to_string_lossy())
                .expect("copia");
        let feita = copia.fazer(&c, cofre.path(), &senha).expect("fazer");
        let sal = fs::read_to_string(remoto.path().join(NOME_DO_SAL)).expect("sal ao lado");
        assert_eq!(sal.trim().len(), 64);
        assert!(sal.trim().bytes().all(|b| b.is_ascii_hexdigit()));
        sais.push(sal);

        // Uma segunda cópia no mesmo destino usa o mesmo sal (senão a primeira ficaria
        // ilegível), e a restauração precisa só da senha.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        copia.fazer(&c, cofre.path(), &senha).expect("segunda");
        assert_eq!(
            fs::read_to_string(remoto.path().join(NOME_DO_SAL)).expect("sal"),
            sais[sais.len() - 1]
        );
        let volta = tempfile::tempdir().expect("volta");
        let r = copia
            .restaurar(&senha, &volta.path().join("c"))
            .expect("restaurar");
        assert_ne!(r.banco, feita.banco, "a mais nova");
    }
    assert_ne!(sais[0], sais[1], "sal aleatório por cópia");

    let ambientes = fs::read_to_string(&embrulho.ambientes).expect("ambientes");
    // O `obscure` do rclone usa IV aleatório: o valor muda a cada chamada. O `reveal` (só no
    // teste, com sal fictício) mostra que cada destino usou o próprio sal.
    let revelados: HashSet<String> = ambientes
        .lines()
        .filter_map(|l| l.strip_prefix("RCLONE_CONFIG_SBFCOPIA_PASSWORD2="))
        .map(|ofuscado| {
            assert!(!ofuscado.is_empty(), "PASSWORD2 vazio");
            let saida = std::process::Command::new("/usr/bin/rclone")
                .args(["reveal", ofuscado])
                .output()
                .expect("reveal");
            String::from_utf8(saida.stdout)
                .expect("utf8")
                .trim()
                .to_owned()
        })
        .collect();
    let esperados: HashSet<String> = sais.iter().map(|s| s.trim().to_owned()).collect();
    assert_eq!(revelados, esperados, "cada destino com o próprio sal");
    // O sal em claro não vai no ambiente (só ofuscado), nem no argv.
    let cmdlines = fs::read_to_string(&embrulho.cmdlines).expect("cmdlines");
    for sal in &sais {
        assert!(!ambientes.contains(sal.trim()));
        assert!(!cmdlines.contains(sal.trim()));
    }
}

/// Um destino com arquivos e sem o sal não é de uma cópia do app (ou é de uma versão antiga,
/// com o sal padrão do rclone): a cópia recusa em vez de misturar.
#[test]
fn destino_ocupado_sem_sal_e_recusado() {
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    fs::write(remoto.path().join("alheio.txt"), b"de outra pessoa").expect("alheio");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);
    assert!(matches!(
        copia.fazer(&c, cofre.path(), &senha),
        Err(Erro::DestinoSemSal)
    ));
    assert_eq!(nomes(remoto.path()), vec!["alheio.txt".to_owned()]);
    // E a restauração de um destino sem sal não acha cópia.
    let volta = tempfile::tempdir().expect("volta");
    assert!(matches!(
        copia.restaurar(&senha, &volta.path().join("c")),
        Err(Erro::SemInstantaneo)
    ));
}

/// Um sal adulterado no destino (não hexadecimal, ou enorme) não é usado.
#[test]
fn sal_adulterado_e_recusado() {
    let (cofre, c) = cofre_ficticio();
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);
    copia.fazer(&c, cofre.path(), &senha).expect("fazer");
    for ruim in ["", "zz", &"a".repeat(10_000)] {
        fs::write(remoto.path().join(NOME_DO_SAL), ruim).expect("adulterar");
        assert!(matches!(
            copia.fazer(&c, cofre.path(), &senha),
            Err(Erro::SalInvalido)
        ));
    }
}
