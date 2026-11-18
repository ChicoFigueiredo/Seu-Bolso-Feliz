//! O executor de processo filho: prazo do pai, reinício, ambiente limpo e isolamento de SO.

mod comum;

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use comum::{corpo, pedido, pedido_com};
use sbf_regras::script::{ErroDoScript, ExecutorDeScript, Limites};
use sbf_script::{Estado, ExecutorFilho, servir};
use serde_json::json;

fn filho() -> ExecutorFilho {
    ExecutorFilho::novo(
        PathBuf::from(env!("CARGO_BIN_EXE_sbf-script-filho")),
        vec![],
    )
}

#[test]
fn executa_e_mantem_o_filho_vivo_entre_pedidos() {
    let ex = filho();
    let p = pedido(
        "function transformar(e) return {corresponde = true, campos = {n = e.campos.a}} end",
    );
    let t = Instant::now();
    let primeira = ex.executar(&p).expect("1");
    let subida = t.elapsed();
    let t = Instant::now();
    for _ in 0..100 {
        assert_eq!(ex.executar(&p).expect("n"), primeira);
    }
    let por_pedido = t.elapsed() / 100;
    eprintln!("primeiro pedido (com subida): {subida:?}; por pedido: {por_pedido:?}");
    assert_eq!(primeira.campos["n"], json!(1));
    assert!(por_pedido < Duration::from_millis(100));
}

#[test]
fn filho_travado_e_morto_pelo_prazo_do_pai_e_o_proximo_pedido_funciona() {
    // O primeiro filho trava de verdade (nem a VM nem o SO o salvam: ele nem chega a Luau);
    // o segundo, subido depois do prazo, é o binário real.
    let pasta = tempfile::tempdir().expect("pasta");
    let marca = pasta.path().join("marca");
    let roteiro = format!(
        "if [ -e '{m}' ]; then exec '{b}'; fi; touch '{m}'; read x; sleep 60",
        m = marca.display(),
        b = env!("CARGO_BIN_EXE_sbf-script-filho"),
    );
    let ex = ExecutorFilho::novo(PathBuf::from("/bin/sh"), vec!["-c".into(), roteiro]);
    let limites = Limites {
        tempo_ms: 200,
        ..Limites::default()
    };
    let t = Instant::now();
    let e = ex
        .executar(&pedido_com(&corpo(""), json!({}), json!({}), limites))
        .expect_err("preso");
    assert_eq!(e, ErroDoScript::Tempo);
    let gasto = t.elapsed();
    eprintln!("prazo do pai estourado em {gasto:?}");
    assert!(gasto < Duration::from_secs(6), "{gasto:?}");
    assert!(ex.executar(&pedido(&corpo(""))).is_ok());
}

#[test]
fn laco_comum_vira_tempo_dentro_do_filho_e_o_filho_reinicia() {
    let ex = filho();
    let limites = Limites {
        tempo_ms: 100,
        instrucoes: u64::MAX,
        ..Limites::default()
    };
    let e = ex
        .executar(&pedido_com(
            &corpo("while true do end"),
            json!({}),
            json!({}),
            limites,
        ))
        .expect_err("laço");
    assert_eq!(e, ErroDoScript::Tempo);
    assert!(ex.executar(&pedido(&corpo(""))).is_ok());
}

#[test]
fn limite_de_memoria_pelo_filho() {
    let ex = filho();
    let limites = Limites {
        memoria_bytes: 8 * 1024 * 1024,
        ..Limites::default()
    };
    let p = pedido_com(
        &corpo("local s = string.rep('x', 900000000)"),
        json!({}),
        json!({}),
        limites,
    );
    assert_eq!(ex.executar(&p), Err(ErroDoScript::Memoria));
    assert!(ex.executar(&pedido(&corpo(""))).is_ok());
}

#[test]
fn filho_nao_enxerga_o_ambiente_do_pai() {
    // Um "filho" de mentira que fala o protocolo e conta o que enxerga: variáveis do pai
    // (o teste roda sob o cargo, que exporta dezenas de CARGO_*), e arquivos na pasta de
    // trabalho. Os dois têm de ser zero.
    assert!(std::env::var("CARGO_MANIFEST_DIR").is_ok());
    let roteiro = r#"read x
vars=$(env | grep -c '^CARGO')
home=$(env | grep -c '^HOME=')
arquivos=$(ls -A | wc -l)
echo "{\"erro\":{\"erro\":\"execucao\",\"detalhe\":\"cargo=$vars home=$home arquivos=$arquivos\"}}""#;
    let ex = ExecutorFilho::novo(PathBuf::from("/bin/sh"), vec!["-c".into(), roteiro.into()]);
    let e = ex
        .executar(&pedido(&corpo("")))
        .expect_err("resposta de mentira");
    assert_eq!(
        e,
        ErroDoScript::Execucao("cargo=0 home=0 arquivos=0".into())
    );
}

#[test]
fn filho_que_morre_vira_erro_de_processo_e_reinicia() {
    // `true` sai na hora: o pedido falha como erro de processo, sem pânico.
    let ex = ExecutorFilho::novo(PathBuf::from("/bin/true"), vec![]);
    assert!(matches!(
        ex.executar(&pedido(&corpo(""))),
        Err(ErroDoScript::Processo(_))
    ));
    assert!(matches!(
        ex.executar(&pedido(&corpo(""))),
        Err(ErroDoScript::Processo(_))
    ));
    // Programa inexistente.
    let ex = ExecutorFilho::novo(PathBuf::from("/nao/existe/sbf"), vec![]);
    assert!(matches!(
        ex.executar(&pedido(&corpo(""))),
        Err(ErroDoScript::Processo(_))
    ));
    // Fora do protocolo: `cat` devolve o próprio pedido.
    let ex = ExecutorFilho::novo(PathBuf::from("/bin/cat"), vec![]);
    assert!(matches!(
        ex.executar(&pedido(&corpo(""))),
        Err(ErroDoScript::Processo(_))
    ));
}

#[test]
fn pedido_grande_demais_nem_sobe_o_filho() {
    let ex = ExecutorFilho::novo(PathBuf::from("/nao/existe/sbf"), vec![]);
    let p = pedido_com(
        &corpo(""),
        json!({"t": "x".repeat(5000)}),
        json!({}),
        Limites {
            entrada_bytes: 100,
            ..Limites::default()
        },
    );
    assert!(matches!(ex.executar(&p), Err(ErroDoScript::Grande(_))));
}

#[test]
fn verificar_pelo_filho() {
    let ex = filho();
    assert_eq!(ex.verificar(&corpo("")), Ok(()));
    assert!(matches!(
        ex.verificar("function transformar( end"),
        Err(ErroDoScript::Sintaxe(_))
    ));
    assert_eq!(ex.verificar("x = 1"), Err(ErroDoScript::SemTransformar));
}

#[test]
fn servir_fala_o_protocolo_de_linhas() {
    let p = pedido(&corpo(""));
    let entrada = format!(
        "{}\n\nlixo\n{}\n{}\n",
        json!({"executar": p}),
        json!({"verificar": "function transformar() end"}),
        json!({"verificar": "x = 1"}),
    );
    let mut saida = Vec::new();
    servir(Cursor::new(entrada), &mut saida).expect("servir");
    let linhas: Vec<serde_json::Value> = String::from_utf8(saida)
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("json"))
        .collect();
    assert_eq!(linhas.len(), 4);
    assert_eq!(linhas[0]["ok"]["corresponde"], json!(true));
    assert_eq!(linhas[1]["erro"]["erro"], json!("processo"));
    assert_eq!(linhas[2], json!({"ok": null}));
    assert_eq!(linhas[3], json!({"erro": {"erro": "sem_transformar"}}));
}

#[cfg(target_os = "linux")]
#[test]
fn endurecimento_de_so_do_filho() {
    let ex = filho();
    let r = ex.endurecimento().expect("relatório");
    eprintln!("endurecimento: {r:?}");
    assert_ne!(r.rlimits, Estado::Nenhum);

    // O autoteste tenta o que o isolamento deve impedir, com o ambiente vazio.
    let saida = Command::new(env!("CARGO_BIN_EXE_sbf-script-filho"))
        .arg("--autoteste")
        .env_clear()
        .current_dir(tempfile::tempdir().expect("pasta").path())
        .output()
        .expect("autoteste");
    let texto = String::from_utf8(saida.stdout).expect("utf8");
    eprintln!("{texto}");
    // Sempre vale, com ou sem Landlock: não grava arquivo, não cria processo.
    assert!(texto.contains("criar_arquivo negado"), "{texto}");
    assert!(texto.contains("novo_processo negado"), "{texto}");
    if r.landlock != Estado::Nenhum {
        assert!(texto.contains("abrir_arquivo negado"), "{texto}");
        assert!(texto.contains("listar_diretorio negado"), "{texto}");
    }
}
