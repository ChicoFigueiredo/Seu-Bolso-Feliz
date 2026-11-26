//! `sbf email` / `sbf gmail` sem rede e sem credenciais: só o que dá para conferir sem o
//! Gmail. O teste isola o ambiente (HOME e XDG_DATA_HOME numa pasta temporária, diretório
//! atual sem `.env`, variáveis do Gmail removidas) para nunca tocar no chaveiro nem nos dados
//! de verdade. As credenciais nunca vão em argumento.

use std::process::{Command, Output};

struct Ambiente {
    pasta: tempfile::TempDir,
}

impl Ambiente {
    fn novo() -> Self {
        Self {
            pasta: tempfile::tempdir().expect("pasta"),
        }
    }

    fn sbf(&self, args: &[&str]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sbf"));
        for v in [
            "GOOGLE_MAIL_CLIENT_ID",
            "GOOGLE_MAIL_CLIENT_SECRET",
            "GMAIL_REFRESH_TOKEN",
            "DISPLAY",
            "WAYLAND_DISPLAY",
        ] {
            c.env_remove(v);
        }
        c.env("HOME", self.pasta.path())
            .env("XDG_DATA_HOME", self.pasta.path().join("dados"))
            .current_dir(self.pasta.path())
            .args(args)
            .output()
            .expect("rodar o sbf")
    }
}

fn erro(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn saida(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn sem_conta_conectada_a_coleta_sai_com_2_e_nao_cria_nada_no_cofre() {
    let a = Ambiente::novo();
    let cofre = tempfile::tempdir().expect("cofre");
    let cofre_s = cofre.path().to_str().expect("utf8");
    for comando in ["gmail", "email"] {
        let o = a.sbf(&[comando, "coletar", "--inventario", cofre_s]);
        assert_eq!(o.status.code(), Some(2), "{}", erro(&o));
        assert!(erro(&o).contains("nenhuma conta conectada"), "{}", erro(&o));
    }
    assert_eq!(std::fs::read_dir(cofre.path()).expect("ler").count(), 0);
}

#[test]
fn argumentos_errados_saem_com_2_e_mostram_o_uso() {
    let a = Ambiente::novo();
    for args in [
        vec!["gmail"],
        vec!["email"],
        vec!["email", "coletar"],
        vec![
            "email",
            "coletar",
            "--inventario",
            "/x",
            "--limite",
            "muitos",
        ],
        vec!["email", "coletar", "--senha", "x"],
    ] {
        let o = a.sbf(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(erro(&o).contains("sbf email"), "{args:?}");
    }
}

#[test]
fn contas_adicionar_seguir_e_o_outlook_em_breve() {
    let a = Ambiente::novo();
    let o = a.sbf(&["email", "contas"]);
    assert!(o.status.success(), "{}", erro(&o));
    assert!(saida(&o).contains("nenhuma conta"));

    let o = a.sbf(&["email", "adicionar", "--tipo", "outlook"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(erro(&o).contains("em breve"), "{}", erro(&o));

    let o = a.sbf(&["email", "adicionar", "--tipo", "gmail", "--apelido", "Casa"]);
    assert!(o.status.success(), "{}", erro(&o));
    assert!(saida(&o).contains("conta gmail adicionada"));
    let o = a.sbf(&["email", "adicionar"]);
    assert!(saida(&o).contains("conta gmail-2 adicionada"));

    let o = a.sbf(&["email", "contas"]);
    let s = saida(&o);
    assert!(
        s.contains("gmail\tGmail") && s.contains("gmail-2\tGmail"),
        "{s}"
    );
    assert!(s.contains("não conectado"), "{s}");
    assert!(!s.contains("GOOGLE_MAIL"), "{s}");

    let o = a.sbf(&["email", "seguir", "gmail", "Viagens"]);
    assert!(o.status.success(), "{}", erro(&o));
    let o = a.sbf(&["email", "deixar", "gmail", "Viagens"]);
    assert!(saida(&o).contains("deixa de coletar"), "{}", saida(&o));

    // Entrar sem o Client ID e o Client secret do aplicativo diz o que fazer (sem abrir nada).
    let o = a.sbf(&["email", "entrar", "gmail-2"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(erro(&o).contains("sbf email app"), "{}", erro(&o));

    let o = a.sbf(&["email", "remover", "gmail-2"]);
    if erro(&o).contains("Não há keyring do sistema") {
        // O runner de CI não tem Secret Service nem keyring do kernel, e o `remover` abre o
        // chaveiro. Tudo o que vinha antes já foi conferido; este último passo fica para
        // ambientes com keyring.
        eprintln!("sem keyring neste ambiente: o `email remover` não foi exercitado");
        return;
    }
    assert!(o.status.success(), "{}", erro(&o));
    assert!(!saida(&a.sbf(&["email", "contas"])).contains("gmail-2"));
}
