//! Achados A6 e A7 da revisão de segurança da fatia 1, no rclone: caminho absoluto nas
//! pastas do sistema, nunca o `PATH`; nada em pasta gravável por outros; e o filho não
//! herda o ambiente do app (nem um `RCLONE_*` solto no shell). Dados fictícios.

#![cfg(unix)]

mod comum;

use comum::{Doc, escrever_executavel, inserir, novo_banco};
use sbf_estante::{CopiaDeSeguranca, Erro, Estante, Rclone, Segredo};
use sbf_processo::{PASTAS_DO_SISTEMA, confiavel};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SENHA: &str = "senha-ficticia-do-teste-Zq7!";
const CANARIO: &str = "valor-ficticio-que-nao-pode-vazar";

/// Script que grava o ambiente que recebeu (um arquivo por chamada) e chama o rclone real.
fn embrulho(pasta: &Path) -> (PathBuf, PathBuf) {
    let registros = pasta.join("ambientes");
    std::fs::create_dir(&registros).expect("pasta");
    let script = pasta.join("rclone-embrulho");
    escrever_executavel(
        &script,
        &format!(
            "#!/bin/sh\n/usr/bin/env > \"{}/$$.txt\"\nexec /usr/bin/rclone \"$@\"\n",
            registros.display()
        ),
    );
    (script, registros)
}

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
fn padrao_e_caminho_absoluto_numa_pasta_do_sistema() {
    let rclone = Rclone::novo();
    assert!(rclone.binario().is_absolute(), "{:?}", rclone.binario());
    assert!(
        PASTAS_DO_SISTEMA
            .iter()
            .any(|p| rclone.binario().starts_with(p))
    );
}

#[test]
fn rclone_inseguro_ou_relativo_nao_roda() {
    let pasta = tempfile::tempdir().expect("pasta");
    let aberta = pasta.path().join("aberta");
    std::fs::create_dir(&aberta).expect("aberta");
    let (script, registros) = embrulho(&aberta);
    std::fs::set_permissions(&aberta, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(!confiavel(&script));
    for rclone in [Rclone::com_binario(&script), Rclone::com_binario("rclone")] {
        let estante = Estante::nova(rclone, pasta.path().to_string_lossy()).expect("estante");
        assert!(matches!(estante.listar(), Err(Erro::RcloneAusente)));
    }
    assert_eq!(
        std::fs::read_dir(&registros).expect("ler").count(),
        0,
        "o executável inseguro rodou"
    );
}

/// Roda o teste interno com um `rclone` falso na frente do `PATH` e variáveis do shell que
/// não podem chegar ao filho.
#[test]
fn path_e_ambiente_do_usuario_nao_chegam_ao_rclone() {
    let pasta = tempfile::tempdir().expect("pasta");
    let falsa = pasta.path().join("bin-falso");
    std::fs::create_dir(&falsa).expect("pasta");
    let marca = pasta.path().join("falso-rodou");
    std::fs::write(
        falsa.join("rclone"),
        format!("#!/bin/sh\ntouch '{}'\n", marca.display()),
    )
    .expect("falso");
    std::fs::set_permissions(falsa.join("rclone"), std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    let path = format!(
        "{}:{}",
        falsa.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let status = std::process::Command::new(std::env::current_exe().expect("exe"))
        .args(["--exact", "interno_com_ambiente_hostil", "--ignored", "-q"])
        .env("PATH", path)
        .env("SBF_TESTE_PASTA", pasta.path())
        .env("RCLONE_DRY_RUN", "true")
        .env("RCLONE_CONFIG_SBFCOPIA_PASSWORD2", CANARIO)
        .env("TOKEN_FICTICIO_DE_API", CANARIO)
        .status()
        .expect("teste interno");
    assert!(status.success(), "o teste interno falhou");
    assert!(!marca.exists(), "o rclone falso do PATH rodou");
}

#[test]
#[ignore = "chamado por path_e_ambiente_do_usuario_nao_chegam_ao_rclone"]
fn interno_com_ambiente_hostil() {
    let Some(pasta) = std::env::var_os("SBF_TESTE_PASTA").map(PathBuf::from) else {
        return;
    };
    // O rclone padrão não é o falso do PATH, e a cópia funciona de verdade (sem dry-run).
    let padrao = Rclone::novo();
    assert!(
        !padrao.binario().starts_with(&pasta),
        "{:?}",
        padrao.binario()
    );
    let (cofre, c) = cofre_ficticio();
    let remoto = pasta.join("remoto");
    let copia = CopiaDeSeguranca::nova(padrao, remoto.to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);
    let feita = copia.fazer(&c, cofre.path(), &senha).expect("fazer");
    assert_eq!(
        copia.instantaneos(&senha).expect("listar"),
        vec![feita.banco]
    );

    // O ambiente de cada chamada, pelo embrulho.
    let (script, registros) = embrulho(&pasta);
    let copia = CopiaDeSeguranca::nova(Rclone::com_binario(script), remoto.to_string_lossy())
        .expect("copia");
    copia.instantaneos(&senha).expect("listar");
    let mut chamadas = 0;
    for registro in std::fs::read_dir(&registros).expect("ler").flatten() {
        chamadas += 1;
        let ambiente = std::fs::read_to_string(registro.path()).expect("ambiente");
        assert!(!ambiente.contains(CANARIO), "variável do shell vazou");
        assert!(
            !ambiente.contains("RCLONE_DRY_RUN"),
            "RCLONE_* do shell vazou"
        );
        assert!(ambiente.contains("PATH=/usr/bin:/bin\n"));
        for linha in ambiente.lines() {
            let nome = linha.split_once('=').map_or(linha, |(n, _)| n);
            let permitido = matches!(
                nome,
                "PATH"
                    | "LANG"
                    | "LC_ALL"
                    | "LC_CTYPE"
                    | "HOME"
                    | "XDG_CONFIG_HOME"
                    | "XDG_CACHE_HOME"
                    | "TMPDIR"
                    | "RCLONE_CONFIG"
                    | "RCLONE_CONFIG_PASS"
                    | "HTTP_PROXY"
                    | "HTTPS_PROXY"
                    | "NO_PROXY"
                    | "http_proxy"
                    | "https_proxy"
                    | "no_proxy"
                    | "SSL_CERT_FILE"
                    | "SSL_CERT_DIR"
                    | "PWD"
                    | "SHLVL"
                    | "_"
                    | "OLDPWD"
            ) || nome.starts_with("RCLONE_CONFIG_SBFCOPIA_");
            assert!(permitido, "variável a mais no filho: {nome}");
        }
    }
    assert!(chamadas >= 3, "obscure, cat do sal e lsf: {chamadas}");
}
