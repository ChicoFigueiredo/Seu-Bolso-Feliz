//! Conexões no arquivo de configuração próprio do SBF, com remotos `local` fictícios.
//! Nunca toca a configuração do usuário nem uma nuvem real.

#![cfg(unix)]

mod comum;

use sbf_estante::{Conexoes, Erro, Rclone};
use std::os::unix::fs::PermissionsExt as _;

fn conexoes(pasta: &std::path::Path) -> (Conexoes, std::path::PathBuf) {
    let arquivo = pasta.join("config-sbf/rclone.conf");
    let rclone = Rclone::novo().com_configuracao(&arquivo);
    (Conexoes::nova(rclone).expect("conexoes"), arquivo)
}

#[test]
fn arquivo_proprio_com_permissao_restrita_e_ciclo_de_vida() {
    if Rclone::novo().binario().as_os_str().is_empty() {
        return;
    }
    let pasta = tempfile::tempdir().expect("pasta");
    let (c, arquivo) = conexoes(pasta.path());
    let modo =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    assert_eq!(modo(&arquivo), 0o600);
    assert_eq!(modo(arquivo.parent().expect("pai")), 0o700);

    c.criar_remoto("nuvem1", "local", &[]).expect("criar");
    assert_eq!(modo(&arquivo), 0o600, "o rclone não afrouxa a permissão");
    let texto = std::fs::read_to_string(&arquivo).expect("ler");
    assert!(texto.contains("[nuvem1]"));
    assert!(matches!(
        c.criar_remoto("nuvem1", "local", &[]),
        Err(Erro::ConexaoRepetida(_))
    ));
    assert!(matches!(
        c.criar_remoto("-x", "local", &[]),
        Err(Erro::NomeDeConexaoInvalido(_))
    ));
    // `local` não é Drive nem OneDrive: não aparece na lista de nuvens.
    assert!(c.listar().expect("listar").is_empty());

    c.desconectar("nuvem1").expect("desconectar");
    assert!(
        !std::fs::read_to_string(&arquivo)
            .expect("ler")
            .contains("nuvem1")
    );
    assert!(matches!(
        c.desconectar("nuvem1"),
        Err(Erro::ConexaoInexistente(_))
    ));
}

#[test]
fn sem_arquivo_proprio_nao_conecta() {
    assert!(matches!(
        Conexoes::nova(Rclone::novo()),
        Err(Erro::SemConfiguracaoPropria)
    ));
}

#[test]
fn estante_e_copia_usam_o_remoto_do_arquivo_proprio() {
    use comum::{Doc, inserir, novo_banco};
    use sbf_estante::{Destino, ListaDeDestinos, Segredo, TipoDeDestino, atualizar_destinos};
    if Rclone::novo().binario().as_os_str().is_empty() {
        return;
    }
    let pasta = tempfile::tempdir().expect("pasta");
    let (c, _) = conexoes(pasta.path());
    let raiz = pasta.path().join("nuvem-falsa");
    std::fs::create_dir_all(&raiz).expect("raiz");
    // `alias` aponta o "Drive" para uma pasta temporária; o remoto só existe no arquivo do SBF.
    c.criar_remoto("gfalso", "alias", &[&format!("remote={}", raiz.display())])
        .expect("alias");
    let cofre = tempfile::tempdir().expect("cofre");
    let banco = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(
        &banco,
        cofre.path(),
        &Doc {
            hash: "aaaaaaaa11111111",
            estado: "documento",
            tipo: Some("fatura"),
            data: Some("2026-10-25"),
            ciclo: Some("2026-10"),
            fornecedor: Some("Luz"),
            valor: Some(-1000),
            ext: "pdf",
            conteudo: Some(b"um"),
            nome_original: "f.pdf",
        },
    );
    let mut lista = ListaDeDestinos::nova();
    lista
        .adicionar(
            Destino::nuvem("drive falso", TipoDeDestino::Drive, "gfalso:SBF/Estante").expect("d"),
        )
        .expect("add");
    let rel = atualizar_destinos(c.rclone(), &lista, &banco, cofre.path(), &|_, _| {});
    assert_eq!(
        rel[0].resultado.as_ref().expect("ok").enviados,
        1,
        "{rel:?}"
    );
    assert!(raiz.join("SBF/Estante/2026-10").is_dir());

    // A cópia cifrada também resolve o remoto pelo arquivo do SBF.
    let copia =
        sbf_estante::CopiaDeSeguranca::nova(c.rclone().clone(), "gfalso:SBF/Copia-de-seguranca")
            .expect("copia");
    let senha = Segredo::novo("senha-ficticia-bem-longa-123".to_owned());
    copia.fazer(&banco, cofre.path(), &senha).expect("fazer");
    assert!(
        raiz.join("SBF/Copia-de-seguranca/sal-da-copia.txt")
            .is_file()
    );
}
