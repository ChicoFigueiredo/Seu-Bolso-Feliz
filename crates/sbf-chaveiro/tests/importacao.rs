//! INV-08: o chaveiro importa um `cofre.enc` de teste (N=2^10); senha errada e arquivo
//! adulterado dão o mesmo erro; o valor de teste não aparece em nenhum arquivo tocado.
//!
//! Os dois fixtures em `tests/fixtures/` foram gerados pelo **próprio**
//! `workers/inventario/src/segredos.ts` (função `cifrar`, KDF `{N: 2**10, r: 8, p: 1}`), no
//! commit que ainda tinha o arquivo, com valores fictícios:
//!
//! ```text
//! cofre-ficticio.enc:            cpf 52998224725 (CPF de exemplo de documentação),
//!                                senhas {condominio-qnl: "senha-ficticia-QNL-71",
//!                                        fatura-loja-beta: "aspas\"e\\barra ção 🔑"}
//! cofre-ficticio-sem-senhas.enc: cpf 11144477735, senhas {}
//! senha-mestra dos dois:         senha-mestra-de-teste-ficticia
//! ```
//!
//! Por isso a leitura aqui prova compatibilidade byte a byte com o formato do Bun.

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo, Situacao};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro, SERVICO};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MESTRA: &str = "senha-mestra-de-teste-ficticia";
const CPF: &str = "52998224725";
const SENHA_QNL: &str = "senha-ficticia-QNL-71";
const SENHA_BETA: &str = "aspas\"e\\barra ção 🔑";
const VALORES: [&str; 4] = [CPF, SENHA_QNL, SENHA_BETA, MESTRA];

const FIXTURE: &[u8] = include_bytes!("fixtures/cofre-ficticio.enc");
const FIXTURE_SEM_SENHAS: &[u8] = include_bytes!("fixtures/cofre-ficticio-sem-senhas.enc");

struct Cenario {
    base: tempfile::TempDir,
    keyring: Arc<ChaveiroEmMemoria>,
}

impl Cenario {
    fn novo(keyring: ChaveiroEmMemoria, fixture: &[u8]) -> Self {
        let base = tempfile::tempdir().expect("pasta temporária");
        let segredos = base.path().join("cofre").join("segredos");
        std::fs::create_dir_all(&segredos).expect("pasta do cofre");
        std::fs::write(segredos.join("cofre.enc"), fixture).expect("fixture");
        Self {
            base,
            keyring: Arc::new(keyring),
        }
    }

    fn cofre_enc(&self) -> PathBuf {
        self.base
            .path()
            .join("cofre")
            .join("segredos")
            .join("cofre.enc")
    }

    fn pasta_do_chaveiro(&self) -> PathBuf {
        self.base
            .path()
            .join("dados")
            .join("seu-bolso-feliz")
            .join("chaveiro")
    }

    fn chaveiro_em(&self, pasta: &Path) -> Chaveiro {
        Chaveiro::abrir_com_chaveiro(
            pasta,
            self.keyring.clone(),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .expect("chaveiro")
    }

    fn chaveiro(&self) -> Chaveiro {
        self.chaveiro_em(&self.pasta_do_chaveiro())
    }

    /// Todo arquivo debaixo da pasta do cenário.
    fn arquivos(&self) -> Vec<PathBuf> {
        fn andar(pasta: &Path, saida: &mut Vec<PathBuf>) {
            for entrada in std::fs::read_dir(pasta).expect("listar") {
                let caminho = entrada.expect("entrada").path();
                if caminho.is_dir() {
                    andar(&caminho, saida);
                } else {
                    saida.push(caminho);
                }
            }
        }
        let mut saida = Vec::new();
        andar(self.base.path(), &mut saida);
        saida.sort();
        saida
    }
}

fn mestra() -> Segredo {
    Segredo::novo(MESTRA)
}

fn contem(palheiro: &[u8], agulha: &str) -> bool {
    let agulha = agulha.as_bytes();
    palheiro
        .windows(agulha.len())
        .any(|janela| janela == agulha)
}

/// Nenhum valor fictício, em UTF-8 nem em base64, em nenhum arquivo do cenário.
fn conferir_que_nenhum_arquivo_tem_valor(cenario: &Cenario) {
    for arquivo in cenario.arquivos() {
        let bytes = std::fs::read(&arquivo).expect("ler");
        for valor in VALORES {
            assert!(
                !contem(&bytes, valor),
                "valor fictício em {}",
                arquivo.display()
            );
            assert!(
                !contem(&bytes, &STANDARD.encode(valor)),
                "valor fictício em base64 em {}",
                arquivo.display()
            );
        }
    }
}

fn conferir_que_o_texto_nao_tem_valor(texto: &str) {
    for valor in VALORES {
        assert!(!texto.contains(valor), "valor fictício em {texto:?}");
    }
}

#[test]
fn importa_para_o_keyring_do_sistema() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    assert_eq!(chaveiro.situacao(), Situacao::ChaveiroDoSistema);

    let resumo = chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");
    assert!(resumo.cpf_valido);
    assert_eq!(resumo.rotulos, ["condominio-qnl", "fatura-loja-beta"]);
    assert_eq!(resumo.onde, Situacao::ChaveiroDoSistema);

    assert_eq!(
        cenario.keyring.entradas(),
        [
            (SERVICO.to_owned(), "cpf".to_owned()),
            (SERVICO.to_owned(), "senha:condominio-qnl".to_owned()),
            (SERVICO.to_owned(), "senha:fatura-loja-beta".to_owned()),
        ]
    );
    assert_eq!(chaveiro.cpf().expect("cpf").expor(), CPF);
    assert_eq!(
        chaveiro.senha("condominio-qnl").expect("qnl").expor(),
        SENHA_QNL
    );
    assert_eq!(
        chaveiro.senha("fatura-loja-beta").expect("beta").expor(),
        SENHA_BETA
    );
    assert!(chaveiro.tem_cpf());
    assert_eq!(
        chaveiro.rotulos_de_senha(),
        ["condominio-qnl", "fatura-loja-beta"]
    );

    conferir_que_nenhum_arquivo_tem_valor(&cenario);
    conferir_que_o_texto_nao_tem_valor(&format!("{chaveiro:?} {resumo:?}"));
    // O original nunca muda.
    assert_eq!(std::fs::read(cenario.cofre_enc()).expect("ler"), FIXTURE);
}

#[test]
fn sem_keyring_importa_para_o_arquivo_de_fallback_com_a_mesma_senha_mestra() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::indisponivel(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    assert_eq!(chaveiro.situacao(), Situacao::Indisponivel);

    let resumo = chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");
    assert_eq!(resumo.onde, Situacao::ArquivoAberto);

    // Os dois arquivos da bancada, só eles, e na pasta de dados (fora do cofre).
    let pasta = cenario.pasta_do_chaveiro();
    let mut nomes: Vec<String> = std::fs::read_dir(&pasta)
        .expect("listar")
        .map(|e| {
            e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    nomes.sort();
    assert_eq!(nomes, ["rotulos.json", "segredos.cifrado"]);
    assert_eq!(cenario.arquivos().len(), 3, "sem temporário sobrando");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = std::fs::metadata(pasta.join("segredos.cifrado"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600);
    }
    conferir_que_nenhum_arquivo_tem_valor(&cenario);

    // Numa nova sessão o arquivo abre trancado e destranca com a mesma senha-mestra.
    let outra_sessao = cenario.chaveiro();
    assert_eq!(outra_sessao.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(
        outra_sessao.rotulos_de_senha(),
        ["condominio-qnl", "fatura-loja-beta"]
    );
    outra_sessao.destrancar(&mestra()).expect("destrancar");
    assert_eq!(outra_sessao.cpf().expect("cpf").expor(), CPF);
    assert_eq!(
        outra_sessao
            .senha("fatura-loja-beta")
            .expect("beta")
            .expor(),
        SENHA_BETA
    );
}

/// Cada adulteração muda um campo do envelope do `cofre.enc`.
fn adulteracoes() -> Vec<(&'static str, Vec<u8>)> {
    let original: serde_json::Value = serde_json::from_slice(FIXTURE).expect("json");
    let trocar = |caminho: &[&str], novo: serde_json::Value| {
        let mut v = original.clone();
        let mut alvo = &mut v;
        for parte in caminho {
            alvo = &mut alvo[*parte];
        }
        *alvo = novo;
        serde_json::to_vec(&v).expect("json")
    };
    let virar_um_bit = |caminho: &[&str]| {
        let mut alvo = &original;
        for parte in caminho {
            alvo = &alvo[*parte];
        }
        let mut bytes = STANDARD.decode(alvo.as_str().expect("texto")).expect("b64");
        bytes[0] ^= 1;
        trocar(caminho, STANDARD.encode(bytes).into())
    };
    let mut cortado = FIXTURE.to_vec();
    cortado.truncate(FIXTURE.len() / 2);
    vec![
        ("dados", virar_um_bit(&["dados"])),
        ("etiqueta", virar_um_bit(&["tag"])),
        ("iv", virar_um_bit(&["iv"])),
        ("sal", virar_um_bit(&["kdf", "sal"])),
        ("N", trocar(&["kdf", "N"], 2048.into())),
        ("r", trocar(&["kdf", "r"], 4.into())),
        ("p", trocar(&["kdf", "p"], 2.into())),
        ("N absurdo", trocar(&["kdf", "N"], (1u64 << 40).into())),
        ("alg", trocar(&["kdf", "alg"], "pbkdf2".into())),
        ("versão", trocar(&["versao"], 2.into())),
        ("dados vazios", trocar(&["dados"], "".into())),
        ("etiqueta curta", trocar(&["tag"], "AAAA".into())),
        (
            "base64 inválido",
            trocar(&["dados"], "não é base64!".into()),
        ),
        ("cortado", cortado),
        ("lixo", b"lixo que nem JSON e".to_vec()),
        ("vazio", Vec::new()),
    ]
}

#[test]
fn senha_errada_e_arquivo_adulterado_dao_o_mesmo_erro() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    let errada = chaveiro
        .importar(&cenario.cofre_enc(), &Segredo::novo("outra-senha-qualquer"))
        .expect_err("senha errada");
    assert_eq!(errada, ErroDoChaveiro::SenhaOuArquivoInvalido);
    let mensagem = errada.to_string();

    for (nome, bytes) in adulteracoes() {
        std::fs::write(cenario.cofre_enc(), &bytes).expect("gravar adulterado");
        let erro = chaveiro
            .importar(&cenario.cofre_enc(), &mestra())
            .expect_err(nome);
        assert_eq!(erro, ErroDoChaveiro::SenhaOuArquivoInvalido, "{nome}");
        assert_eq!(erro.to_string(), mensagem, "{nome}");
        assert_eq!(format!("{erro:?}"), format!("{errada:?}"), "{nome}");
    }
    // Nada foi gravado no chaveiro.
    assert!(cenario.keyring.entradas().is_empty());
    assert!(chaveiro.rotulos_de_senha().is_empty());
    conferir_que_o_texto_nao_tem_valor(&mensagem);
}

#[test]
fn sem_keyring_a_senha_errada_nao_cria_o_arquivo_de_fallback() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::indisponivel(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    let erro = chaveiro
        .importar(&cenario.cofre_enc(), &Segredo::novo("outra-senha-qualquer"))
        .expect_err("senha errada");
    assert_eq!(erro, ErroDoChaveiro::SenhaOuArquivoInvalido);
    assert_eq!(chaveiro.situacao(), Situacao::Indisponivel);
    assert!(
        !cenario
            .pasta_do_chaveiro()
            .join("segredos.cifrado")
            .exists()
    );
}

#[test]
fn confere_e_so_oferece_apagar_o_cofre_enc() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");

    let oferta = chaveiro
        .conferir_importacao(&cenario.cofre_enc(), &mestra())
        .expect("confere");
    assert_eq!(oferta.caminho(), cenario.cofre_enc());
    conferir_que_o_texto_nao_tem_valor(&format!("{oferta:?}"));
    // Recusar é descartar a oferta: o arquivo continua lá.
    drop(oferta);
    assert!(cenario.cofre_enc().exists());

    // Se o chaveiro diverge, não há oferta.
    chaveiro
        .guardar_senha("condominio-qnl", Segredo::novo("trocada-depois"))
        .expect("trocar");
    assert_eq!(
        chaveiro
            .conferir_importacao(&cenario.cofre_enc(), &mestra())
            .expect_err("diverge"),
        ErroDoChaveiro::NaoConfere
    );
    assert!(cenario.cofre_enc().exists());

    // Reimportar é idempotente e volta a conferir; aceitar apaga.
    chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("reimportar");
    chaveiro
        .conferir_importacao(&cenario.cofre_enc(), &mestra())
        .expect("confere")
        .apagar()
        .expect("apagar");
    assert!(!cenario.cofre_enc().exists());
    // Os segredos continuam no chaveiro.
    assert_eq!(chaveiro.cpf().expect("cpf").expor(), CPF);
}

#[test]
fn conferir_com_senha_errada_nao_oferece_nada() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");
    assert_eq!(
        chaveiro
            .conferir_importacao(&cenario.cofre_enc(), &Segredo::novo("outra-senha-qualquer"))
            .expect_err("senha errada"),
        ErroDoChaveiro::SenhaOuArquivoInvalido
    );
}

#[test]
fn recusa_chaveiro_dentro_do_cofre() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::indisponivel(), FIXTURE);
    for dentro in [
        cenario.base.path().join("cofre").join("chaveiro"),
        cenario
            .base
            .path()
            .join("cofre")
            .join("segredos")
            .join("chaveiro"),
    ] {
        let chaveiro = cenario.chaveiro_em(&dentro);
        assert_eq!(
            chaveiro
                .importar(&cenario.cofre_enc(), &mestra())
                .expect_err("dentro do cofre"),
            ErroDoChaveiro::PastaDentroDoCofre
        );
        assert!(!dentro.join("segredos.cifrado").exists());
    }
}

#[test]
fn arquivo_de_fallback_com_outra_senha_mestra_pede_para_destrancar() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::indisponivel(), FIXTURE);
    cenario
        .chaveiro()
        .criar_arquivo(&Segredo::novo("outra-mestra-do-chaveiro"))
        .expect("criar");
    let chaveiro = cenario.chaveiro();
    assert_eq!(chaveiro.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(
        chaveiro
            .importar(&cenario.cofre_enc(), &mestra())
            .expect_err("trancado"),
        ErroDoChaveiro::ChaveiroTrancado
    );
    // Destrancado com a senha dele, a importação entra.
    chaveiro
        .destrancar(&Segredo::novo("outra-mestra-do-chaveiro"))
        .expect("destrancar");
    chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");
    assert_eq!(
        chaveiro.senha("condominio-qnl").expect("qnl").expor(),
        SENHA_QNL
    );
}

#[test]
fn cofre_enc_sem_senhas_proprias() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE_SEM_SENHAS);
    let chaveiro = cenario.chaveiro();
    let resumo = chaveiro
        .importar(&cenario.cofre_enc(), &mestra())
        .expect("importar");
    assert!(resumo.cpf_valido);
    assert!(resumo.rotulos.is_empty());
    assert_eq!(chaveiro.cpf().expect("cpf").expor(), "11144477735");
}

#[test]
fn cofre_enc_ausente() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    assert_eq!(
        chaveiro
            .importar(
                &cenario.cofre_enc().with_file_name("nao-existe.enc"),
                &mestra()
            )
            .expect_err("ausente"),
        ErroDoChaveiro::ArquivoAntigoAusente
    );
}

#[test]
fn rotulo_invalido_e_recusado() {
    let cenario = Cenario::novo(ChaveiroEmMemoria::novo(), FIXTURE);
    let chaveiro = cenario.chaveiro();
    assert_eq!(
        chaveiro
            .guardar_senha("", Segredo::novo("x"))
            .expect_err("vazio"),
        ErroDoChaveiro::RotuloInvalido
    );
    assert_eq!(
        chaveiro
            .guardar_senha("com\nquebra", Segredo::novo("x"))
            .expect_err("controle"),
        ErroDoChaveiro::RotuloInvalido
    );
}

/// O crate não loga nem imprime nada: nenhum caminho para um valor sair por aí.
#[test]
fn o_crate_nao_loga() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entrada in std::fs::read_dir(src).expect("src") {
        let caminho = entrada.expect("entrada").path();
        let texto = std::fs::read_to_string(&caminho).expect("ler");
        for proibido in [
            "println!",
            "eprintln!",
            "print!(",
            "eprint!(",
            "dbg!",
            "log::",
            "tracing::",
        ] {
            assert!(
                !texto.contains(proibido),
                "{proibido} em {}",
                caminho.display()
            );
        }
    }
}
