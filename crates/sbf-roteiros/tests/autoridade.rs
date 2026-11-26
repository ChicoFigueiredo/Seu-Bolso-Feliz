//! Teste de ARQUITETURA: quem pode ativar um roteiro escrito.
//!
//! A [`sbf_roteiros::banco::Confirmacao`] é "autoridade por convenção": os construtores são
//! `pub` porque o terminal (`sbf`) e a janela (`sbf-janela`) precisam deles. O que mantém o MCP,
//! o worker e a IA longe de ativar, desativar ou marcar como testado não é o sistema de tipos:
//! é este teste, que lê o código-fonte desses crates e **falha** se algum deles citar o que só
//! o usuário (terminal ou janela) pode tocar.
//!
//! Os comentários (`//`, `///`, `//!`, `/* */`) não contam: documentar a regra é permitido.

use std::path::{Path, PathBuf};

/// Os crates que nunca ativam roteiro escrito.
const CRATES_PROIBIDOS: [&str; 3] = ["sbf-mcp", "sbf-worker", "sbf-ia"];

/// Citar isto, em qualquer lugar do código, é ter o que só o usuário tem.
const PROIBIDOS: [&str; 4] = ["Confirmacao", "da_janela", "do_terminal", "marcar_testado"];

/// Chamar isto num arquivo que toca em roteiros escritos é ativar ou desativar.
const ACOES: [&str; 2] = [".ativar(", ".desativar("];

/// Mostra o código sem comentários (as strings ficam; o que parece comentário dentro de uma
/// string, como `https://`, não é cortado).
fn sem_comentarios(fonte: &str) -> String {
    let cs: Vec<char> = fonte.chars().collect();
    let mut saida = String::with_capacity(fonte.len());
    let mut i = 0;
    let mut em_string = false;
    while i < cs.len() {
        let c = cs[i];
        if em_string {
            saida.push(c);
            if c == '\\' && i + 1 < cs.len() {
                saida.push(cs[i + 1]);
                i += 1;
            } else if c == '"' {
                em_string = false;
            }
            i += 1;
        } else if c == '"' {
            em_string = true;
            saida.push(c);
            i += 1;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && cs.get(i + 1) == Some(&'*') {
            i += 2;
            while i < cs.len() && !(cs[i] == '*' && cs.get(i + 1) == Some(&'/')) {
                if cs[i] == '\n' {
                    saida.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else {
            saida.push(c);
            i += 1;
        }
    }
    saida
}

/// O que há de proibido neste arquivo (já sem comentários), um item por achado.
fn achados(codigo: &str) -> Vec<String> {
    let mut v: Vec<String> = PROIBIDOS
        .iter()
        .filter(|p| codigo.contains(**p))
        .map(|p| format!("cita `{p}`"))
        .collect();
    // `.ativar(` também existe nas regras de reconhecimento: só conta onde o arquivo toca em
    // roteiros escritos (o banco ou o crate dos roteiros).
    if codigo.contains("BancoDeRoteiros") || codigo.contains("sbf_roteiros") {
        v.extend(
            ACOES
                .iter()
                .filter(|a| codigo.contains(**a))
                .map(|a| format!("chama `{a}` num arquivo que usa os roteiros")),
        );
    }
    v
}

fn arquivos_rs(pasta: &Path, saida: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for e in entradas.flatten() {
        let caminho = e.path();
        if caminho.is_dir() {
            arquivos_rs(&caminho, saida);
        } else if caminho.extension().is_some_and(|x| x == "rs") {
            saida.push(caminho);
        }
    }
}

#[test]
fn o_detector_acha_o_proibido_e_ignora_comentarios() {
    assert!(achados("let c = Confirmacao::da_janela(a, b);").len() >= 2);
    assert!(!achados("let b = BancoDeRoteiros::abrir(p); b.marcar_testado(x);").is_empty());
    assert!(!achados("use sbf_roteiros::x; b.ativar(n, 1, &c);").is_empty());
    assert!(!achados("use sbf_roteiros::x; b.desativar(n, &c);").is_empty());
    // as regras de reconhecimento também têm `.ativar(`: só conta junto dos roteiros
    assert!(achados("c.ativar(&regra, 1, &Quem::Mcp)").is_empty());
    let comentado = sem_comentarios(
        "// Confirmacao e marcar_testado\n/// da_janela\n//! do_terminal\n/* Confirmacao\n .ativar( */\nfn ok() {}\n",
    );
    assert!(achados(&comentado).is_empty(), "{comentado}");
    // `//` dentro de string não esconde o que vem depois
    let esperto = sem_comentarios("let u = \"https://x\"; Confirmacao::da_janela(a, b);\n");
    assert!(!achados(&esperto).is_empty());
}

#[test]
fn mcp_worker_e_ia_nao_ativam_nem_testam_roteiro_escrito() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut problemas = Vec::new();
    let mut lidos = 0;
    for c in CRATES_PROIBIDOS {
        let src = raiz.join(c).join("src");
        assert!(src.is_dir(), "não achei {}", src.display());
        let mut arquivos = Vec::new();
        arquivos_rs(&src, &mut arquivos);
        assert!(!arquivos.is_empty(), "{c} não tem fontes?");
        for a in arquivos {
            let fonte = std::fs::read_to_string(&a).unwrap_or_default();
            lidos += 1;
            for achado in achados(&sem_comentarios(&fonte)) {
                problemas.push(format!("{}: {achado}", a.display()));
            }
        }
    }
    assert!(lidos > 10, "li poucos arquivos ({lidos}): o caminho mudou?");
    assert!(
        problemas.is_empty(),
        "só o terminal e a janela do usuário ativam, desativam e marcam como testado um roteiro escrito:\n{}",
        problemas.join("\n")
    );
}
