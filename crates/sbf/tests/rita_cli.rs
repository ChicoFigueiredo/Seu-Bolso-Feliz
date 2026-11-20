//! `sbf rita` rodando o binário de verdade contra um servidor HTTP falso no formato do OpenAI
//! (RITA-07), com cofre temporário e texto fictício. Nenhum provedor real, nenhum dado real.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::{Command, Stdio};

use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
use sbf_regras::Estado;
use sbf_regras::catalogo::Catalogo;
use serde_json::json;

const REGRA: &str = "comprovante-pix-banco-exemplo";
const PEDIDO: &str = "o título determina o padrão; a data determina a competência; cria";
const COMPROVANTE: &str = "Comprovante Pix\nBanco Exemplo\nValor: R$ 150,00\nData: 10/11/2026\nAutenticação: ABC123XYZ SEGREDO-FICTICIO-DO-DOCUMENTO\n";

struct Ambiente {
    pasta: tempfile::TempDir,
}

impl Ambiente {
    fn novo() -> Self {
        let pasta = tempfile::tempdir().unwrap();
        std::fs::create_dir(pasta.path().join("cofre")).unwrap();
        Catalogo::abrir(&pasta.path().join("cofre").join("regras.sqlite")).unwrap();
        // O inventário com `saida_ia`: sem ele a CLI não envia nada (ADR-015).
        sbf_rita::registro::inventario_minimo_para_testes(&pasta.path().join("cofre"));
        Self { pasta }
    }

    fn cofre(&self) -> PathBuf {
        self.pasta.path().join("cofre")
    }

    fn documento(&self, texto: &str) -> PathBuf {
        let p = self.pasta.path().join("comprovante.txt");
        std::fs::write(&p, texto).unwrap();
        p
    }

    fn sbf(
        &self,
        servidor: &ServidorFalso,
        doc: &std::path::Path,
        pedido: &str,
    ) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_sbf"))
            .arg("rita")
            .arg("--inventario")
            .arg(self.cofre())
            .arg("--documento")
            .arg(doc)
            .args(["--provedor", "local", "--modelo", "modelo-ficticio"])
            .arg("--endereco")
            .arg(format!("{}/v1", servidor.base()))
            .arg(pedido)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn resposta_com_chamada() -> String {
    let definicao = json!({
        "tipo_documental": REGRA,
        "detectar": {"todos": [{"contem": "comprovante pix"}, {"contem": "banco exemplo"}]},
        "extrair": {
            "valor_centavos": {"regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"},
            "competencia": {"regex": "Data:\\s*\\d{2}/(\\d{2}/\\d{4})", "formato": "competencia"}
        },
        "obrigatorios": ["valor_centavos"]
    });
    let args =
        json!({"regra": REGRA, "nome": "Comprovante Pix Banco Exemplo", "definicao": definicao});
    json!({
        "model": "modelo-ficticio",
        "choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [
            {"id": "call_1", "type": "function",
             "function": {"name": "propor_padrao", "arguments": args.to_string()}}
        ]}}],
        "usage": {"prompt_tokens": 1234, "completion_tokens": 56}
    })
    .to_string()
}

#[test]
fn rita07_o_formato_openai_cria_o_rascunho_e_a_cli_mostra_tudo() {
    let a = Ambiente::novo();
    let doc = a.documento(COMPROVANTE);
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::json(&resposta_com_chamada()),
    );
    let o = a.sbf(&s, &doc, PEDIDO);
    let saida = String::from_utf8_lossy(&o.stdout).into_owned();
    let erro = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{saida}\n{erro}");
    assert!(saida.contains("propor_padrao"), "{saida}");
    assert!(saida.contains(&format!("Rascunho: {REGRA} v1")), "{saida}");
    assert!(saida.contains("passou"), "{saida}");
    assert!(saida.contains("tokens 1234 entrada / 56 saída"), "{saida}");
    assert!(saida.contains("local, sem custo"), "{saida}");
    assert!(saida.contains("valor_centavos = R$ 150,00"), "{saida}");
    // O texto do documento nunca é impresso.
    assert!(!saida.contains("SEGREDO-FICTICIO"), "{saida}");
    assert!(!erro.contains("SEGREDO-FICTICIO"), "{erro}");

    let c = Catalogo::abrir(&a.cofre().join("regras.sqlite")).unwrap();
    let v = c.versoes(REGRA).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].estado, Estado::Rascunho);
    assert_eq!(v[0].motivo, PEDIDO);
    assert_eq!(
        c.historico_de_estado(REGRA, 1).unwrap()[0].2,
        "assistente (modelo-ficticio)"
    );
    assert!(c.vigentes().unwrap().is_empty());
    // O pedido ao servidor levou o texto no bloco de dados e as sete ferramentas.
    let recebidos = s.pedidos();
    assert_eq!(recebidos.len(), 1, "para no primeiro que passa");
    let corpo: serde_json::Value = serde_json::from_slice(&recebidos[0].corpo).unwrap();
    assert_eq!(corpo["tools"].as_array().unwrap().len(), 7);
    assert!(
        corpo["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("<<<DADOS-")
    );
    // O backup foi feito antes da escrita.
    assert!(a.cofre().join("backups").join("mcp").is_dir());
    // O que saiu ficou registrado, sem conteúdo (ADR-015), mesmo sendo o provedor local.
    let banco = rusqlite::Connection::open(a.cofre().join("inventario.sqlite")).unwrap();
    let linha: String = banco
        .query_row(
            "SELECT destinatario||'|'||nivel||'|'||provedor||'|'||modelo||'|'||tokens_entrada||'|'||tokens_saida FROM saida_ia",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(linha, "rita|completo|local|modelo-ficticio|1234|56");
}

#[test]
fn sem_o_registro_do_que_sai_a_cli_nao_envia_nada_e_diz_o_porque() {
    let a = Ambiente::novo();
    std::fs::remove_file(a.cofre().join("inventario.sqlite")).unwrap();
    let doc = a.documento(COMPROVANTE);
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::json(&resposta_com_chamada()),
    );
    let o = a.sbf(&s, &doc, PEDIDO);
    assert_eq!(o.status.code(), Some(1));
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("nada foi enviado"), "{e}");
    assert!(s.pedidos().is_empty(), "o servidor não recebeu nada");
}

#[test]
fn documento_sem_texto_ou_inexistente_da_erro_claro() {
    let a = Ambiente::novo();
    let s = ServidorFalso::iniciar();
    let o = a.sbf(&s, std::path::Path::new("abcdef123456"), PEDIDO);
    assert_eq!(o.status.code(), Some(1));
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("nenhum achado"), "{e}");
    assert!(s.pedidos().is_empty());

    let pdf = a.pasta.path().join("x.pdf");
    std::fs::write(&pdf, b"%PDF-1.4 ficticio").unwrap();
    let o = a.sbf(&s, &pdf, PEDIDO);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("sem texto guardado")
            || String::from_utf8_lossy(&o.stderr).contains("não tem texto guardado")
    );
}

#[test]
fn sem_argumentos_mostra_a_ajuda() {
    let o = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .arg("rita")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("sbf rita"));
}
