//! Achados A6, A7 e A10 da revisão de segurança da fatia 1, na regra única: o programa é
//! resolvido por caminho absoluto nas pastas do sistema, nunca pelo `PATH`; um executável em
//! pasta gravável por outros (ou ele mesmo gravável) é recusado; o filho recebe só o ambiente
//! mínimo; e a saída tem teto e prazo. Tudo com scripts fictícios, gerados aqui.

#![cfg(unix)]

use sbf_processo::{
    Chamada, FalhaDeProcesso, PASTAS_DO_SISTEMA, VARIAVEIS_BASICAS, confiavel, localizar,
};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

fn script(pasta: &Path, nome: &str, corpo: &str) -> PathBuf {
    let caminho = pasta.join(nome);
    std::fs::write(&caminho, format!("#!/bin/sh\n{corpo}\n")).expect("script");
    std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    caminho
}

fn nomes_do_ambiente(texto: &str) -> BTreeSet<String> {
    texto
        .lines()
        .filter_map(|l| l.split_once('=').map(|(n, _)| n.to_owned()))
        .collect()
}

#[test]
fn padrao_e_caminho_absoluto_numa_pasta_do_sistema() {
    let sh = localizar("sh").expect("sh");
    assert!(sh.is_absolute(), "{sh:?}");
    assert!(
        PASTAS_DO_SISTEMA.iter().any(|p| sh.starts_with(p)),
        "{sh:?}"
    );
    assert_eq!(localizar("nao-existe-sbf-ficticio"), None);
}

/// Roda o teste interno com um `PATH` que tem programas falsos na frente.
#[test]
fn path_do_usuario_nao_e_consultado() {
    let pasta = tempfile::tempdir().expect("pasta");
    let falsa = pasta.path().join("bin-falso");
    std::fs::create_dir(&falsa).expect("pasta");
    let marca = pasta.path().join("falso-rodou");
    for nome in ["sh", "sbf-ficticio-so-no-path"] {
        script(&falsa, nome, &format!("touch '{}'", marca.display()));
    }
    let path = format!(
        "{}:{}",
        falsa.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let status = std::process::Command::new(std::env::current_exe().expect("exe"))
        .args(["--exact", "interno_com_path_falso", "--ignored", "-q"])
        .env("PATH", path)
        .env("SBF_TESTE_PASTA_FALSA", &falsa)
        .status()
        .expect("teste interno");
    assert!(status.success(), "o teste interno falhou");
    assert!(!marca.exists(), "o executável falso do PATH rodou");
}

#[test]
#[ignore = "chamado por path_do_usuario_nao_e_consultado"]
fn interno_com_path_falso() {
    let Some(falsa) = std::env::var_os("SBF_TESTE_PASTA_FALSA").map(PathBuf::from) else {
        return;
    };
    assert_eq!(localizar("sbf-ficticio-so-no-path"), None);
    let sh = localizar("sh").expect("sh");
    assert!(!sh.starts_with(&falsa), "{sh:?}");
    let saida = Chamada::nova(sh)
        .argumentos(["-c", "echo ok"])
        .rodar()
        .expect("rodar");
    assert_eq!(&saida.stdout[..], b"ok\n");
}

#[test]
fn programa_em_pasta_gravavel_por_outros_e_recusado() {
    let pasta = tempfile::tempdir().expect("pasta");
    let aberta = pasta.path().join("aberta");
    std::fs::create_dir(&aberta).expect("pasta");
    let marca = pasta.path().join("rodou");
    let inseguro = script(&aberta, "inseguro", &format!("touch '{}'", marca.display()));
    assert!(confiavel(&inseguro), "antes de abrir a pasta");
    std::fs::set_permissions(&aberta, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(!confiavel(&inseguro));
    assert_eq!(
        Chamada::nova(&inseguro).rodar().map(|_| ()),
        Err(FalhaDeProcesso::NaoEncontrado)
    );
    assert!(!marca.exists(), "o executável inseguro rodou");

    // O próprio arquivo gravável por outros também não vale.
    std::fs::set_permissions(&aberta, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    std::fs::set_permissions(&inseguro, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(!confiavel(&inseguro));
}

#[test]
fn caminho_relativo_ou_ausente_e_recusado() {
    assert!(!confiavel(Path::new("sh")));
    assert!(!confiavel(Path::new("/nao/existe/sbf")));
    assert_eq!(
        Chamada::nova("sh").rodar().map(|_| ()),
        Err(FalhaDeProcesso::NaoEncontrado)
    );
}

#[test]
fn filho_recebe_so_o_ambiente_minimo_e_o_pedido() {
    let pasta = tempfile::tempdir().expect("pasta");
    let env = script(pasta.path(), "ambiente", "/usr/bin/env");
    let saida = Chamada::nova(&env).rodar().expect("rodar");
    let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
    // O que o `sh` acrescenta sozinho (PWD, SHLVL, _) e o que o app passa.
    let mut permitidos: BTreeSet<String> = ["PATH", "PWD", "SHLVL", "_", "OLDPWD"]
        .into_iter()
        .chain(VARIAVEIS_BASICAS.iter().copied())
        .map(str::to_owned)
        .collect();
    let nomes = nomes_do_ambiente(&texto);
    assert!(
        nomes.is_subset(&permitidos),
        "variáveis a mais: {:?}",
        nomes.difference(&permitidos).collect::<Vec<_>>()
    );
    assert!(texto.contains("PATH=/usr/bin:/bin\n"), "{texto}");

    // Herdar `HOME` e pôr uma variável só no filho.
    let saida = Chamada::nova(&env)
        .herdando(&["HOME"])
        .com_variavel("SBF_FICTICIA", Zeroizing::new("valor-ficticio".to_owned()))
        .rodar()
        .expect("rodar");
    let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
    assert!(texto.contains("SBF_FICTICIA=valor-ficticio\n"), "{texto}");
    permitidos.extend(["HOME".to_owned(), "SBF_FICTICIA".to_owned()]);
    assert!(nomes_do_ambiente(&texto).is_subset(&permitidos), "{texto}");
}

#[test]
fn entrada_vai_pela_entrada_padrao_e_nao_pelo_argv() {
    let pasta = tempfile::tempdir().expect("pasta");
    let eco = script(
        pasta.path(),
        "eco",
        "printf 'ARGS:%s\\n' \"$*\"\n/usr/bin/cat",
    );
    let saida = Chamada::nova(&eco)
        .com_entrada(Zeroizing::new(b"segredo-ficticio".to_vec()))
        .rodar()
        .expect("rodar");
    assert!(saida.status.success());
    assert_eq!(&saida.stdout[..], b"ARGS:\nsegredo-ficticio");
    assert!(!saida.stdout_cortado);
}

#[test]
fn saida_enorme_e_cortada_sem_travar_e_o_erro_guarda_o_fim() {
    let pasta = tempfile::tempdir().expect("pasta");
    let falante = script(
        pasta.path(),
        "falante",
        "head -c 40000000 /dev/zero | tr '\\000' a\n\
         head -c 300000 /dev/zero | tr '\\000' e >&2\necho ULTIMA >&2",
    );
    let inicio = Instant::now();
    let saida = Chamada::nova(&falante)
        .com_saida_maxima(1024 * 1024)
        .com_erro_maximo(1000)
        .com_prazo(Duration::from_secs(20))
        .rodar()
        .expect("rodar");
    assert!(inicio.elapsed() < Duration::from_secs(15), "travou");
    assert_eq!(saida.stdout.len(), 1024 * 1024);
    assert!(saida.stdout_cortado);
    assert_eq!(saida.stderr.len(), 1000);
    assert!(saida.stderr.ends_with(b"ULTIMA\n"));
}

#[test]
fn passou_do_prazo_e_morto() {
    let pasta = tempfile::tempdir().expect("pasta");
    let lento = script(pasta.path(), "lento", "exec /usr/bin/sleep 30");
    let inicio = Instant::now();
    let r = Chamada::nova(&lento)
        .com_prazo(Duration::from_millis(200))
        .rodar();
    assert_eq!(r.map(|_| ()), Err(FalhaDeProcesso::TempoEsgotado));
    assert!(inicio.elapsed() < Duration::from_secs(5));

    // Um neto que herdou os pipes não segura o prazo.
    let com_neto = script(
        pasta.path(),
        "com-neto",
        "/usr/bin/sleep 30
true",
    );
    let inicio = Instant::now();
    let r = Chamada::nova(&com_neto)
        .com_prazo(Duration::from_millis(200))
        .rodar();
    assert_eq!(r.map(|_| ()), Err(FalhaDeProcesso::TempoEsgotado));
    assert!(
        inicio.elapsed() < Duration::from_secs(5),
        "{:?}",
        inicio.elapsed()
    );
}

#[test]
fn debug_da_chamada_nao_mostra_valores() {
    let c = Chamada::nova("/usr/bin/true")
        .argumentos(["argumento-ficticio"])
        .com_variavel("SBF_X", Zeroizing::new("valor-ficticio".to_owned()))
        .com_entrada(Zeroizing::new(b"entrada-ficticia".to_vec()));
    let texto = format!("{c:?}");
    for valor in ["argumento-ficticio", "valor-ficticio", "entrada-ficticia"] {
        assert!(!texto.contains(valor), "{texto}");
    }
}
