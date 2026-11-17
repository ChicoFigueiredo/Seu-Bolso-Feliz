//! `sbf regras` e `sbf script --servir`, rodando o binário de verdade, com cofre temporário e
//! dados fictícios ("Banco Exemplo", "Fulano de Tal").
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use sbf_regras::catalogo::Catalogo;
use sbf_regras::motor::Motor;
use sbf_regras::script::{ExecutorDeScript, Limites, PedidoDeScript};
use sbf_regras::{Decisao, Estado, Quem, Valor};
use sbf_script::ExecutorFilho;
use serde_json::json;

const REGRA: &str = "comprovante-pix-banco-exemplo";

const COMPROVANTE: &str = "Banco Exemplo\nComprovante PIX\nFavorecido: Fulano de Tal\n\
Valor: R$ 1.234,56\nData: 06/10/2026\nAutenticacao: ABC123XYZ\n";

const OUTRO: &str = "Loja Exemplo\nRecibo de compra\nValor: R$ 10,00\n";

const SCRIPT_V1: &str = "function transformar(entrada, parametros, ajuda)\n  return {corresponde = true, campos = {favorecido = entrada.campos.favorecido}}\nend\n";
const SCRIPT_V2: &str = "function transformar(entrada, parametros, ajuda)\n  return {corresponde = true, campos = {favorecido = string.upper(entrada.campos.favorecido), tarifa_centavos = ajuda.mul_div_arred(entrada.campos.valor_centavos, 1, 100)}}\nend\n";

fn definicao_json(regra: &str) -> String {
    json!({
        "regra": regra,
        "nome": "Comprovante Pix · Banco Exemplo",
        "definicao": {
            "tipo_documental": "comprovante-pix",
            "detectar": {"todos": [{"contem": "banco exemplo"}, {"contem": "comprovante pix"}]},
            "extrair": {
                "valor_centavos": {"regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"},
                "data": {"regex": "Data:\\s*(\\d{2}/\\d{2}/\\d{4})", "formato": "data"},
                "favorecido": {"regex": "Favorecido:\\s*(.+)", "formato": "texto"}
            },
            "obrigatorios": ["valor_centavos"],
            "propor": [{"provar_registro": {"janela_dias": 2}}]
        }
    })
    .to_string()
}

fn esperado_json(favorecido: &str) -> String {
    json!({
        "esperado": "reconhece",
        "tipo_documental": "comprovante-pix",
        "campos": {
            "valor_centavos": {"tipo": "centavos", "valor": 123_456},
            "favorecido": {"tipo": "texto", "valor": favorecido}
        }
    })
    .to_string()
}

struct Ambiente {
    pasta: tempfile::TempDir,
}

impl Ambiente {
    fn novo() -> Self {
        let pasta = tempfile::tempdir().unwrap();
        std::fs::create_dir(pasta.path().join("cofre")).unwrap();
        Self { pasta }
    }

    fn cofre(&self) -> PathBuf {
        self.pasta.path().join("cofre")
    }

    fn arquivo(&self, nome: &str, conteudo: &str) -> PathBuf {
        let p = self.pasta.path().join(nome);
        std::fs::write(&p, conteudo).unwrap();
        p
    }

    /// Roda `sbf regras --inventario <cofre> args...` com a entrada vazia (sem terminal).
    fn sbf(&self, args: &[&str]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sbf"));
        c.arg("regras")
            .arg("--inventario")
            .arg(self.cofre())
            .args(args)
            .stdin(Stdio::null());
        c.output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let o = self.sbf(args);
        assert!(
            o.status.success(),
            "{args:?} falhou: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8(o.stdout).unwrap()
    }

    fn importar(&self, regra: &str, script: Option<&str>) -> String {
        let j = self.arquivo("regra.json", &definicao_json(regra));
        match script {
            Some(s) => {
                let l = self.arquivo("regra.luau", s);
                self.ok(&[
                    "importar",
                    j.to_str().unwrap(),
                    "--script",
                    l.to_str().unwrap(),
                ])
            }
            None => self.ok(&["importar", j.to_str().unwrap()]),
        }
    }

    fn catalogo(&self) -> Catalogo {
        Catalogo::abrir(&self.cofre().join("regras.sqlite")).unwrap()
    }
}

fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn executor() -> Arc<dyn ExecutorDeScript> {
    Arc::new(ExecutorFilho::novo(
        PathBuf::from(env!("CARGO_BIN_EXE_sbf")),
        vec!["script".into(), "--servir".into()],
    ))
}

#[test]
fn importar_listar_mostrar_e_testar_com_exemplo_que_passa() {
    let a = Ambiente::novo();
    let saida = a.importar(REGRA, Some(SCRIPT_V1));
    assert!(saida.contains("criada a versão 1"), "{saida}");
    // O mesmo conteúdo de novo não cria versão.
    let saida = a.importar(REGRA, Some(SCRIPT_V1));
    assert!(saida.contains("igual à versão 1"), "{saida}");

    let txt = a.arquivo("comprovante.txt", COMPROVANTE);
    let esp = a.arquivo("esperado.json", &esperado_json("Fulano de Tal"));
    let s = a.ok(&[
        "exemplo",
        REGRA,
        txt.to_str().unwrap(),
        "--esperado",
        esp.to_str().unwrap(),
    ]);
    assert!(s.contains("gravado"), "{s}");
    let outro = a.arquivo("recibo.txt", OUTRO);
    a.ok(&["exemplo", REGRA, outro.to_str().unwrap(), "--nao-se-aplica"]);

    let lista = a.ok(&["listar"]);
    assert!(lista.contains(REGRA), "{lista}");
    assert!(lista.contains("contracheque"), "{lista}");
    let mostrar = a.ok(&["mostrar", REGRA]);
    assert!(mostrar.contains("v1 rascunho"), "{mostrar}");
    assert!(mostrar.contains("Exemplos: 2"), "{mostrar}");
    assert!(!mostrar.contains("ABC123XYZ"), "nunca o texto do documento");

    let t = a.ok(&["testar", REGRA, "1"]);
    assert!(t.contains("2 exemplo(s) passaram"), "{t}");
}

#[test]
fn testar_sai_com_codigo_diferente_de_zero_quando_um_exemplo_falha() {
    let a = Ambiente::novo();
    a.importar(REGRA, Some(SCRIPT_V1));
    let txt = a.arquivo("comprovante.txt", COMPROVANTE);
    let esp = a.arquivo("esperado.json", &esperado_json("Beltrano"));
    a.ok(&[
        "exemplo",
        REGRA,
        txt.to_str().unwrap(),
        "--esperado",
        esp.to_str().unwrap(),
    ]);
    let o = a.sbf(&["testar", REGRA, "1"]);
    assert_eq!(o.status.code(), Some(1), "{}", texto(&o));
    assert!(texto(&o).contains("FALHOU"), "{}", texto(&o));
}

#[test]
fn importar_recusa_script_com_erro_de_sintaxe_e_nao_grava() {
    let a = Ambiente::novo();
    let j = a.arquivo("regra.json", &definicao_json(REGRA));
    let l = a.arquivo("regra.luau", "function transformar( return end");
    let o = a.sbf(&[
        "importar",
        j.to_str().unwrap(),
        "--script",
        l.to_str().unwrap(),
    ]);
    assert!(!o.status.success());
    assert!(texto(&o).contains("verificação"), "{}", texto(&o));
    assert!(a.catalogo().versoes(REGRA).unwrap().is_empty());
}

#[test]
fn ativar_sem_terminal_e_recusado_e_nada_muda() {
    let a = Ambiente::novo();
    a.importar(REGRA, Some(SCRIPT_V1));
    for args in [
        vec!["ativar", REGRA, "1"],
        vec!["consolidar", REGRA, "1"],
        vec!["desativar", REGRA],
        vec!["reverter", REGRA],
    ] {
        // Mesmo com a confirmação certa vinda por um pipe, não há terminal.
        let mut c = Command::new(env!("CARGO_BIN_EXE_sbf"));
        let mut filho = c
            .arg("regras")
            .arg("--inventario")
            .arg(a.cofre())
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = filho
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{REGRA}@1\n{REGRA}\n").as_bytes());
        let o = filho.wait_with_output().unwrap();
        assert!(!o.status.success(), "{args:?}");
        assert!(texto(&o).contains("terminal interativo"), "{}", texto(&o));
    }
    let v = a.catalogo().versao(REGRA, 1).unwrap();
    assert_eq!(v.estado, Estado::Rascunho);

    let p = a.arquivo("p.json", "{}");
    let o = a.sbf(&[
        "parametro",
        "irrf",
        "2026-01",
        p.to_str().unwrap(),
        "--fonte",
        "https://exemplo.invalid/tabela",
    ]);
    assert!(!o.status.success());
    assert!(a.catalogo().parametros().unwrap().is_empty());
}

#[test]
fn prova_sem_recompilar_regra_nova_e_depois_a_v2_com_o_luau_alterado() {
    // O MESMO binário já compilado, do começo ao fim. O teste faz o papel do usuário
    // (`Quem::Usuario`) na ativação, que o terminal só aceita interativamente.
    let a = Ambiente::novo();
    let txt = a.arquivo("comprovante.txt", COMPROVANTE);
    let esp1 = a.arquivo("esperado1.json", &esperado_json("Fulano de Tal"));

    // 1. Regra nova de um tipo que o binário não conhecia.
    a.importar(REGRA, Some(SCRIPT_V1));
    a.ok(&[
        "exemplo",
        REGRA,
        txt.to_str().unwrap(),
        "--esperado",
        esp1.to_str().unwrap(),
    ]);
    let antes = a.ok(&["simular", REGRA, "1", txt.to_str().unwrap()]);
    assert!(antes.contains("1 mudaria"), "{antes}");
    assert!(antes.contains("antes:  nenhuma regra"), "{antes}");
    assert!(antes.contains("favorecido=Fulano de Tal"), "{antes}");
    assert!(antes.contains("valor_centavos=123456c"), "{antes}");

    let ex = executor();
    a.catalogo()
        .ativar(REGRA, 1, &Quem::Usuario, Some(&*ex))
        .unwrap();
    // Reconhecido agora, sem rebuild.
    let motor = Motor::das_vigentes(&a.catalogo(), Some(ex.clone())).unwrap();
    let entrada = {
        let mut e = sbf_entrada(COMPROVANTE);
        e.nome = Some("comprovante.txt".into());
        e
    };
    let Decisao::Reconhecido(r) = motor.ler(&entrada).decisao else {
        panic!("deveria reconhecer");
    };
    assert_eq!(r.tipo_documental, "comprovante-pix");
    assert_eq!(
        r.campos["favorecido"].valor,
        Valor::Texto("Fulano de Tal".into())
    );
    assert!(!r.campos.contains_key("tarifa_centavos"));

    // 2. A v2 só muda o Luau.
    let saida = a.importar(REGRA, Some(SCRIPT_V2));
    assert!(saida.contains("criada a versão 2"), "{saida}");
    let diff = a.ok(&["simular", REGRA, "2", txt.to_str().unwrap()]);
    assert!(diff.contains("1 mudaria"), "{diff}");
    assert!(diff.contains("favorecido=FULANO DE TAL"), "{diff}");
    assert!(diff.contains("tarifa_centavos=1235c"), "{diff}");
    // A v1 em vigor falharia o exemplo da v2; o exemplo é atualizado, como a pessoa faria.
    let t = a.sbf(&["testar", REGRA, "2"]);
    assert_eq!(t.status.code(), Some(1), "{}", texto(&t));
    let esp2 = a.arquivo("esperado2.json", &esperado_json("FULANO DE TAL"));
    a.ok(&[
        "exemplo",
        REGRA,
        txt.to_str().unwrap(),
        "--esperado",
        esp2.to_str().unwrap(),
    ]);
    a.ok(&["testar", REGRA, "2"]);
    a.catalogo()
        .ativar(REGRA, 2, &Quem::Usuario, Some(&*ex))
        .unwrap();
    let motor = Motor::das_vigentes(&a.catalogo(), Some(ex)).unwrap();
    let Decisao::Reconhecido(r) = motor.ler(&entrada).decisao else {
        panic!("deveria reconhecer");
    };
    assert_eq!(r.versao, 2);
    assert_eq!(
        r.campos["favorecido"].valor,
        Valor::Texto("FULANO DE TAL".into())
    );
    assert!(r.campos.contains_key("tarifa_centavos"));
}

fn sbf_entrada(texto: &str) -> sbf_regras::Entrada {
    use sbf_regras::{Canal, Dicas, Origem, TipoDeEntrada};
    sbf_regras::Entrada {
        versao_do_esquema: 1,
        id: "teste".into(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Manual,
            id_externo: None,
        },
        sha256: None,
        mime: Some("text/plain".into()),
        nome: None,
        texto: Some(texto.into()),
        dicas: Dicas::default(),
        momento: None,
    }
}

fn pedir(
    filho_in: &mut impl Write,
    filho_out: &mut impl BufRead,
    mensagem: &serde_json::Value,
) -> serde_json::Value {
    writeln!(filho_in, "{mensagem}").unwrap();
    filho_in.flush().unwrap();
    let mut linha = String::new();
    filho_out.read_line(&mut linha).unwrap();
    serde_json::from_str(&linha).unwrap()
}

fn pedido(fonte: &str) -> serde_json::Value {
    let p = PedidoDeScript {
        fonte: fonte.to_owned(),
        checksum: "c".into(),
        entrada: json!({"tipo_documental": "x", "canal": "manual", "campos": {"a": 1}}),
        parametros: json!({}),
        limites: Limites::default(),
    };
    json!({"executar": p})
}

#[test]
fn script_servir_responde_e_recusa_io_open() {
    let mut filho = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .args(["script", "--servir"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut entrada = filho.stdin.take().unwrap();
    let mut saida = BufReader::new(filho.stdout.take().unwrap());

    let r = pedir(
        &mut entrada,
        &mut saida,
        &pedido(
            "function transformar(e) return {corresponde = true, campos = {n = e.campos.a}} end",
        ),
    );
    assert_eq!(r["ok"]["corresponde"], json!(true), "{r}");
    assert_eq!(r["ok"]["campos"]["n"], json!(1), "{r}");

    let r = pedir(
        &mut entrada,
        &mut saida,
        &pedido(
            "function transformar() local f = io.open('/etc/passwd') return {corresponde = f ~= nil} end",
        ),
    );
    assert!(r.get("ok").is_none(), "io.open não pode existir: {r}");
    assert!(r.get("erro").is_some(), "{r}");

    drop(entrada);
    assert!(filho.wait().unwrap().success());
}

#[test]
fn ajuda_lista_os_subcomandos_e_script_fica_fora_da_ajuda_principal() {
    let o = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .args(["regras", "--ajuda"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let t = String::from_utf8(o.stdout).unwrap();
    for sub in [
        "listar",
        "mostrar",
        "importar",
        "exemplo",
        "testar",
        "simular",
        "ativar",
        "consolidar",
        "desativar",
        "reverter",
        "parametro",
    ] {
        assert!(t.contains(sub), "{sub}");
    }
    let o = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .arg("--zzz")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&o.stderr).contains("script --servir"));
}
