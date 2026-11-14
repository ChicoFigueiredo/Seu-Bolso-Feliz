//! O provedor `ClaudeCli` contra um `claude` falso (script de shell gerado pelo teste).
//! IA-05: o prompt vai pela entrada padrão e nunca aparece no argv do processo filho.

#![cfg(unix)]

mod apoio;

use sbf_ia::{ClaudeCli, Degrau, ErroDeIa, Pedido, Provedor};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const ENTRADA: &str = "ENTRADA-FICTICIA-XYZ boleto 123";
const INSTRUCAO: &str = "INSTRUCAO-FICTICIA-ABC";

/// Escreve o script por um processo filho: se o próprio teste abrisse o arquivo para escrita,
/// o `fork` de um teste paralelo poderia herdar o descritor antes do `exec` e o script daria
/// `ETXTBSY` de vez em quando (mesma técnica do `sbf-processo`).
fn script(pasta: &Path, nome: &str, corpo: &str) -> PathBuf {
    let caminho = pasta.join(nome);
    let estado = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("printf '%s\\n' \"$2\" > \"$1\" && chmod 755 \"$1\"")
        .arg("sh")
        .arg(&caminho)
        .arg(format!("#!/bin/sh\n{corpo}"))
        .status()
        .expect("escrever o script");
    assert!(estado.success());
    caminho
}

struct Cenario {
    _raiz: tempfile::TempDir,
    registros: PathBuf,
    base: PathBuf,
    provedor: ClaudeCli,
}

/// Um `claude` falso que registra o que recebeu e responde o que o teste pôs em `saida.json`.
fn cenario(saida: &str, codigo: i32) -> Cenario {
    let raiz = tempfile::tempdir().expect("pasta");
    let registros = raiz.path().join("registros");
    let base = raiz.path().join("base");
    let bin = raiz.path().join("bin");
    for p in [&registros, &base, &bin] {
        std::fs::create_dir(p).expect("pasta");
    }
    std::fs::write(registros.join("saida.json"), saida).expect("saida");
    std::fs::write(registros.join("codigo.txt"), codigo.to_string()).expect("codigo");
    let d = registros.display();
    let claude = script(
        &bin,
        "claude",
        &format!(
            "D='{d}'\n\
             cat > \"$D/stdin.txt\"\n\
             printf '%s\\n' \"$@\" > \"$D/argv.txt\"\n\
             env > \"$D/env.txt\"\n\
             pwd > \"$D/pwd.txt\"\n\
             ls -ld . | cut -c1-10 > \"$D/modo.txt\"\n\
             ls -A | wc -l > \"$D/conteudo.txt\"\n\
             cat \"$D/saida.json\"\n\
             exit \"$(cat \"$D/codigo.txt\")\"\n"
        ),
    );
    let provedor = ClaudeCli::novo(claude).com_pasta_base(&base);
    Cenario {
        _raiz: raiz,
        registros,
        base,
        provedor,
    }
}

fn pedido(degrau: Degrau) -> Pedido {
    Pedido {
        prompt_versao: "v1".into(),
        instrucao: INSTRUCAO.into(),
        entrada: ENTRADA.into(),
        esquema: json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
        degrau,
        max_tokens_saida: 321,
    }
}

fn saida_boa() -> String {
    json!({
        "type":"result","subtype":"success","is_error":false,
        "result":"```json\n{\"ok\":true}\n```",
        "total_cost_usd":0.0042,
        "usage":{"input_tokens":50,"output_tokens":7,"cache_read_input_tokens":3},
        "modelUsage":{"claude-haiku-ficticio":{}}
    })
    .to_string()
}

fn ler(c: &Cenario, nome: &str) -> String {
    std::fs::read_to_string(c.registros.join(nome)).expect(nome)
}

#[test]
fn le_resposta_uso_e_custo_nominal() {
    let c = cenario(&saida_boa(), 0);
    let r = c.provedor.pedir(&pedido(Degrau::Rapido)).expect("resposta");
    assert_eq!(r.json, json!({"ok": true}));
    assert_eq!(r.modelo, "claude-haiku-ficticio");
    assert_eq!(r.consumo.tokens_entrada, 53);
    assert_eq!(r.consumo.tokens_saida, 7);
    assert_eq!(r.consumo.custo_dezmilesimos_usd, 42);
    assert!(r.consumo.nominal);
    assert_eq!(c.provedor.nome(), "claude-cli");
}

#[test]
fn modelo_por_degrau() {
    let c = cenario(&saida_boa(), 0);
    c.provedor.pedir(&pedido(Degrau::Rapido)).expect("rápido");
    assert!(ler(&c, "argv.txt").lines().any(|l| l == "haiku"));
    c.provedor.pedir(&pedido(Degrau::Forte)).expect("forte");
    let argv = ler(&c, "argv.txt");
    assert!(argv.lines().any(|l| l == "sonnet"), "{argv}");
    assert!(!argv.lines().any(|l| l == "haiku"));
}

#[test]
fn ia05_o_prompt_vai_pela_entrada_padrao_e_nunca_pelo_argv() {
    let c = cenario(&saida_boa(), 0);
    c.provedor.pedir(&pedido(Degrau::Rapido)).expect("resposta");
    let argv = ler(&c, "argv.txt");
    let stdin = ler(&c, "stdin.txt");
    for marca in [
        "ENTRADA-FICTICIA",
        "INSTRUCAO-FICTICIA",
        "boleto",
        "required",
    ] {
        assert!(!argv.contains(marca), "{marca} no argv:\n{argv}");
        assert!(stdin.contains(marca), "{marca} faltou na entrada padrão");
    }
    // Só opções fixas: modo de impressão, formato, modelo, sem ferramentas, sem sessão.
    let linhas: Vec<&str> = argv.lines().collect();
    assert_eq!(linhas[0], "-p");
    let pos = |o: &str| linhas.iter().position(|l| *l == o).expect(o);
    assert_eq!(linhas[pos("--output-format") + 1], "json");
    assert_eq!(linhas[pos("--tools") + 1], "", "sem ferramentas");
    for o in [
        "--no-session-persistence",
        "--strict-mcp-config",
        "--disable-slash-commands",
    ] {
        pos(o);
    }
}

#[test]
fn diretorio_de_trabalho_vazio_0700_sob_a_base_e_apagado_no_fim() {
    let c = cenario(&saida_boa(), 0);
    c.provedor.pedir(&pedido(Degrau::Rapido)).expect("resposta");
    let pwd = PathBuf::from(ler(&c, "pwd.txt").trim());
    let base = std::fs::canonicalize(&c.base).expect("base");
    assert!(pwd.starts_with(&base), "{pwd:?} fora de {base:?}");
    assert_eq!(ler(&c, "modo.txt").trim(), "drwx------");
    assert_eq!(ler(&c, "conteudo.txt").trim(), "0");
    assert!(!pwd.exists(), "o diretório de trabalho foi apagado");
    assert_eq!(std::fs::read_dir(&c.base).expect("base").count(), 0);
}

#[test]
fn ambiente_minimo_sem_chaves_do_shell() {
    let c = cenario(&saida_boa(), 0);
    c.provedor.pedir(&pedido(Degrau::Rapido)).expect("resposta");
    let permitidos: BTreeSet<&str> = [
        "PATH",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
        // Que o próprio sh acrescenta:
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
    ]
    .into_iter()
    .collect();
    let env = ler(&c, "env.txt");
    let nomes: BTreeSet<String> = env
        .lines()
        .filter_map(|l| l.split_once('=').map(|(n, _)| n.to_owned()))
        .collect();
    let extras: Vec<_> = nomes
        .iter()
        .filter(|n| !permitidos.contains(n.as_str()))
        .collect();
    assert!(extras.is_empty(), "variáveis herdadas a mais: {extras:?}");
    assert!(env.contains("CLAUDE_CODE_MAX_OUTPUT_TOKENS=321"));
}

#[test]
fn erro_de_limite_vira_limite_com_a_espera_do_epoch() {
    let futuro = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("relógio")
        .as_secs()
        + 900;
    let saida = json!({"type":"result","is_error":true,
        "result": format!("Claude AI usage limit reached|{futuro}")})
    .to_string();
    let c = cenario(&saida, 1);
    match c.provedor.pedir(&pedido(Degrau::Rapido)) {
        Err(ErroDeIa::Limite { espera_segundos }) => {
            assert!((880..=900).contains(&espera_segundos), "{espera_segundos}");
        }
        outro => panic!("{outro:?}"),
    }
}

#[test]
fn limite_sem_data_usa_a_espera_padrao_configurada() {
    let saida = json!({"is_error":true,"result":"API Error: 429 rate limit exceeded"}).to_string();
    let c = cenario(&saida, 1);
    let provedor = c.provedor.clone().com_espera_do_limite(42);
    assert!(matches!(
        provedor.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Limite {
            espera_segundos: 42
        })
    ));
}

#[test]
fn resposta_sem_json_ou_fora_do_esquema_e_invalida() {
    for texto in ["não consegui", "{\"ok\":\"sim\"}"] {
        let saida = json!({"is_error":false,"result":texto,"usage":{}}).to_string();
        let c = cenario(&saida, 0);
        assert!(matches!(
            c.provedor.pedir(&pedido(Degrau::Rapido)),
            Err(ErroDeIa::RespostaInvalida(_))
        ));
    }
}

#[test]
fn programa_ausente_e_indisponivel() {
    let c = cenario(&saida_boa(), 0);
    let provedor = ClaudeCli::novo(c.base.join("nao-existe")).com_pasta_base(&c.base);
    assert!(matches!(
        provedor.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Indisponivel(_))
    ));
}

#[test]
fn passou_do_prazo_e_morto() {
    let raiz = tempfile::tempdir().expect("pasta");
    let claude = script(raiz.path(), "claude", "cat > /dev/null\nsleep 10\n");
    let provedor = ClaudeCli::novo(claude)
        .com_pasta_base(raiz.path())
        .com_prazo(Duration::from_millis(300));
    let inicio = std::time::Instant::now();
    assert!(matches!(
        provedor.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Outro(_))
    ));
    assert!(inicio.elapsed() < Duration::from_secs(5));
}
