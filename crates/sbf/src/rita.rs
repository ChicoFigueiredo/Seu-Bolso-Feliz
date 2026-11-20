//! `sbf rita --documento <sha|caminho> "pedido"`: Rita pelo terminal, sem janela (spec 15).
//! Serve para calibrar o prompt e as descrições das ferramentas com um modelo de verdade.
//!
//! ```text
//! sbf rita [--inventario <cofre>] --documento <sha|caminho> [--provedor <id>] \
//!          [--endereco <url>] [--modelo <nome>] "pedido"
//! ```
//!
//! Imprime cada chamada de ferramenta, o rascunho, o pré-teste, os tokens e o tempo. Só lê o
//! cofre; a única escrita é um rascunho em `regras.sqlite`, depois do backup de sempre
//! (`<cofre>/backups/mcp/`). Os provedores `local` e `claude` não abrem o Chaveiro; os outros
//! ids leem a configuração e a chave do Chaveiro. Nunca imprime o texto do documento nem chave.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_ia::{
    ChatCompletions, ConfiguracaoChatCompletions, ENDERECO_DO_OLLAMA, ErroDeIa,
    MODELO_LOCAL_PADRAO, NaturezaDoCusto,
};
use sbf_rita::cofre::{achar_pelo_prefixo, nome_e_mime, texto_guardado};
use sbf_rita::{
    ClaudeDeRita, Contexto, DadosDaTela, Evento, GuardaDeProvedores, ProvedorDaRita, Relatorio,
    Rita, entrada_de_documento,
};
use sha2::{Digest, Sha256};

pub const AJUDA: &str = "\
uso: sbf rita [--inventario <cofre>] --documento <sha|caminho> [opções] \"pedido\"

  --documento <sha|caminho>  o item: o sha256 (ou o começo dele) de um achado do cofre, ou um
                             arquivo (o texto vem do inventário pelo hash; .txt vale sem ele)
  --inventario <cofre>       a pasta do cofre (padrão: FINANCAS_DIR)
  --provedor <id>            local (padrão), claude, ou o id de um provedor do Chaveiro
  --endereco <url>           endereço do provedor local ou compatível (https, ou http em localhost)
  --modelo <nome>            o modelo (padrão do local: qwen3-coder:30b)
  --area <nome>              a área que a tela mostraria (padrão: Documentos)

Só lê o cofre. A única escrita é um RASCUNHO em regras.sqlite, depois de um backup, e uma
linha por envio em saida_ia (ADR-015): sem o inventário do cofre, nada é enviado. Rita nunca
ativa nada.";

const LIMITE_DO_ARQUIVO: u64 = 8 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
struct Linha {
    cofre: Option<PathBuf>,
    documento: Option<String>,
    provedor: Option<String>,
    endereco: Option<String>,
    modelo: Option<String>,
    area: Option<String>,
    pedido: Option<String>,
    ajuda: bool,
}

fn separar(args: &[String]) -> Result<Linha, String> {
    let mut l = Linha::default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let valor = |i: usize| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{a} pede um valor"))
        };
        match a {
            "--ajuda" | "--help" | "-h" => l.ajuda = true,
            "--inventario" => {
                l.cofre = Some(PathBuf::from(valor(i)?));
                i += 1;
            }
            "--documento" => {
                l.documento = Some(valor(i)?);
                i += 1;
            }
            "--provedor" => {
                l.provedor = Some(valor(i)?);
                i += 1;
            }
            "--endereco" => {
                l.endereco = Some(valor(i)?);
                i += 1;
            }
            "--modelo" => {
                l.modelo = Some(valor(i)?);
                i += 1;
            }
            "--area" => {
                l.area = Some(valor(i)?);
                i += 1;
            }
            outro if outro.starts_with("--") => {
                return Err(format!("argumento desconhecido: {outro}"));
            }
            outro => {
                if l.pedido.replace(outro.to_owned()).is_some() {
                    return Err("o pedido vem entre aspas, uma vez só".to_owned());
                }
            }
        }
        i += 1;
    }
    Ok(l)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn mime_do_conteudo(inicio: &[u8], nome: &str) -> &'static str {
    if inicio.starts_with(b"%PDF") {
        "application/pdf"
    } else if nome.to_ascii_lowercase().ends_with(".txt") {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}

/// A entrada do `--documento`: um sha (ou prefixo) do cofre, ou um arquivo.
fn entrada_do_documento(cofre: &Path, doc: &str) -> Result<sbf_regras::Entrada, String> {
    let caminho = Path::new(doc);
    if caminho.is_file() {
        let meta = std::fs::metadata(caminho).map_err(|e| format!("{doc}: {}", e.kind()))?;
        if meta.len() > LIMITE_DO_ARQUIVO {
            return Err(format!("{doc}: arquivo grande demais para esta ferramenta"));
        }
        let bytes = std::fs::read(caminho).map_err(|e| format!("{doc}: {}", e.kind()))?;
        let sha = hex(&Sha256::digest(&bytes));
        let nome = caminho
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned);
        let mime = mime_do_conteudo(&bytes[..bytes.len().min(16)], nome.as_deref().unwrap_or(""));
        // O inventário manda; sem ele, só texto simples.
        let texto = texto_guardado(cofre, &sha).or_else(|| {
            (mime == "text/plain")
                .then(|| String::from_utf8(bytes.clone()).ok())
                .flatten()
        });
        let Some(texto) = texto else {
            return Err("este arquivo não tem texto guardado no inventário (rode `sbf reconhecer` antes) e não é .txt".to_owned());
        };
        let (nome_inv, mime_inv) = nome_e_mime(cofre, &sha).unwrap_or((None, None));
        return Ok(entrada_de_documento(
            &sha,
            nome.as_deref().or(nome_inv.as_deref()),
            mime_inv.as_deref().or(Some(mime)),
            Some(&texto),
        ));
    }
    let prefixo = doc.to_ascii_lowercase();
    if prefixo.len() < 6 || !prefixo.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "{doc}: não é um arquivo nem um sha256 (ou começo dele)"
        ));
    }
    let achados = achar_pelo_prefixo(cofre, &prefixo);
    let [sha] = achados.as_slice() else {
        return Err(if achados.is_empty() {
            format!("{doc}: nenhum achado do cofre começa assim")
        } else {
            format!("{doc}: mais de um achado começa assim; use mais caracteres")
        });
    };
    let Some(texto) = texto_guardado(cofre, sha) else {
        return Err("este achado não tem texto guardado (rode `sbf reconhecer` antes)".to_owned());
    };
    let (nome, mime) = nome_e_mime(cofre, sha).unwrap_or((None, None));
    Ok(entrada_de_documento(
        sha,
        nome.as_deref(),
        mime.as_deref(),
        Some(&texto),
    ))
}

fn escolher_provedor(l: &Linha) -> Result<ProvedorDaRita, String> {
    let id = l.provedor.as_deref().unwrap_or("local");
    let programa =
        std::env::current_exe().map_err(|_| "não achei o próprio programa".to_owned())?;
    match id {
        "local" => {
            let c = ConfiguracaoChatCompletions::local(
                l.endereco
                    .clone()
                    .unwrap_or_else(|| ENDERECO_DO_OLLAMA.to_owned()),
                l.modelo
                    .clone()
                    .unwrap_or_else(|| MODELO_LOCAL_PADRAO.to_owned()),
            );
            let p = ChatCompletions::novo(c, None).map_err(|e| e.to_string())?;
            Ok(ProvedorDaRita::Conversa(Arc::new(p)))
        }
        "claude" | "claude-cli" => {
            let cli = sbf_ia::ClaudeCli::do_ambiente()
                .ok_or_else(|| "o `claude` não foi encontrado".to_owned())?;
            let modelo = l.modelo.clone().unwrap_or_else(|| "sonnet".to_owned());
            let cli = cli
                .com_modelos(modelo.clone(), modelo.clone())
                .com_prazo(std::time::Duration::from_secs(600));
            Ok(ProvedorDaRita::Claude(ClaudeDeRita::novo(
                cli, programa, modelo,
            )))
        }
        outro => {
            let chaveiro = sbf_chaveiro::Chaveiro::abrir_na_pasta_padrao()
                .map_err(|e| format!("chaveiro: {e}"))?;
            let guarda: Arc<dyn GuardaDeProvedores> = Arc::new(chaveiro);
            let config = guarda.ler();
            let mut p = config.por_id(outro).cloned().ok_or_else(|| {
                format!("não há provedor `{outro}` configurado (aba IA, em Conexões)")
            })?;
            if let Some(e) = &l.endereco {
                p.endereco = Some(e.clone());
            }
            if let Some(m) = &l.modelo {
                p.modelo.clone_from(m);
            }
            sbf_rita::construir(&p, &guarda, &programa).map_err(|e| e.to_string())
        }
    }
}

fn imprimir_chamada(c: &sbf_rita::ChamadaFeita) {
    println!(
        "  [{}] {}({}){}\n      -> {}",
        c.numero,
        c.nome,
        c.argumentos,
        if c.recusada { "  RECUSADA" } else { "" },
        c.resultado
    );
}

fn custo_texto(r: &Relatorio) -> String {
    let usd = r.consumo.custo_dezmilesimos_usd;
    match r.natureza {
        NaturezaDoCusto::Zero => "local, sem custo".to_owned(),
        NaturezaDoCusto::Nominal => format!(
            "nominal US$ {}.{:04} (assinatura)",
            usd / 10_000,
            usd % 10_000
        ),
        NaturezaDoCusto::Real => format!("US$ {}.{:04}", usd / 10_000, usd % 10_000),
        NaturezaDoCusto::NaoInformado => "custo não informado pelo provedor".to_owned(),
    }
}

fn imprimir(r: &Relatorio) {
    println!("\n== Resposta ({:?}) ==\n{}", r.parada, r.resposta);
    if let Some(m) = r.melhor() {
        println!(
            "\n== Rascunho: {} v{} ({}) ==\nPré-teste ({}):",
            m.regra,
            m.versao,
            m.nome,
            if m.pre_teste.passou {
                "passou"
            } else {
                "NÃO passou"
            }
        );
        for l in m.pre_teste.linhas() {
            println!("  - {l}");
        }
        if m.pre_teste.pede_confirmacao() {
            println!("  (ativar pediria confirmação)");
        }
    }
    println!(
        "\n{} chamadas de ferramenta · tokens {} entrada / {} saída · {} · {} · {:.1}s",
        r.chamadas.iter().filter(|c| !c.recusada).count(),
        r.consumo.tokens_entrada,
        r.consumo.tokens_saida,
        custo_texto(r),
        r.modelo,
        r.duracao.as_secs_f64()
    );
}

fn executar(l: &Linha) -> Result<(), String> {
    let cofre = l
        .cofre
        .clone()
        .or_else(|| std::env::var_os("FINANCAS_DIR").map(PathBuf::from))
        .ok_or("falta --inventario <cofre>")?;
    if !cofre.is_dir() {
        return Err(format!("{} não é uma pasta de cofre", cofre.display()));
    }
    let doc = l
        .documento
        .as_deref()
        .ok_or("falta --documento <sha|caminho>")?;
    let pedido = l.pedido.as_deref().ok_or("falta o pedido, entre aspas")?;
    let item = entrada_do_documento(&cofre, doc)?;
    // ADR-015: sem o registro do que sai (inventário do cofre), nada é enviado a provedor algum.
    let registro = sbf_rita::abrir_registro(&cofre).map_err(|e| e.to_string())?;
    let provedor = escolher_provedor(l)?;
    let mut tela = DadosDaTela::de_documento(&cofre, item);
    if let Some(a) = &l.area {
        tela.area.clone_from(a);
    }
    println!("Rita · pedido: {pedido}");
    let parar = AtomicBool::new(false);
    let executor = crate::script::executor_de_script()
        .map_err(|e| format!("não deu para montar o executor de scripts: {e}"))?;
    let relatorio = match provedor {
        ProvedorDaRita::Claude(c) => c.pedir(&tela, pedido, &parar, &registro, executor),
        ProvedorDaRita::Conversa(p) => {
            let mut rita = Rita::nova(Contexto::novo(tela, executor));
            rita.pedir(p.as_ref(), pedido, &parar, &registro, &mut |e| match e {
                Evento::Chamada(c) => imprimir_chamada(c),
                Evento::Fala(t) => println!("  Rita: {t}"),
                Evento::Consumo(_) => {}
            })
        }
    }
    .map_err(|e| match e {
        ErroDeIa::Limite { espera_segundos } => {
            format!("limite do provedor; tente em {espera_segundos}s")
        }
        outro => outro.to_string(),
    })?;
    if !relatorio.chamadas.is_empty() && matches!(relatorio.natureza, NaturezaDoCusto::Nominal) {
        // O `claude` fez as chamadas em outro processo: mostra-as agora.
        println!("Chamadas feitas pelo claude:");
        relatorio.chamadas.iter().for_each(imprimir_chamada);
    }
    imprimir(&relatorio);
    Ok(())
}

pub fn rodar(args: &[String]) -> ExitCode {
    let l = match separar(args) {
        Ok(l) => l,
        Err(m) => {
            eprintln!("sbf: {m}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    if l.ajuda || args.is_empty() {
        println!("{AJUDA}");
        return ExitCode::SUCCESS;
    }
    match executar(&l) {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            eprintln!("sbf: {m}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn separa_as_opcoes_e_o_pedido() {
        let l = separar(&s(&[
            "--documento",
            "abcdef",
            "--provedor",
            "local",
            "--modelo",
            "m",
            "cria o padrão",
        ]))
        .unwrap();
        assert_eq!(l.documento.as_deref(), Some("abcdef"));
        assert_eq!(l.pedido.as_deref(), Some("cria o padrão"));
        assert_eq!(l.modelo.as_deref(), Some("m"));
        assert!(separar(&s(&["--bobagem"])).is_err());
        assert!(separar(&s(&["um", "dois"])).is_err());
        assert!(separar(&s(&["--documento"])).is_err());
    }

    #[test]
    fn sha_curto_demais_ou_nao_hex_e_recusado() {
        let d = tempfile::tempdir().unwrap();
        assert!(entrada_do_documento(d.path(), "abc").is_err());
        assert!(entrada_do_documento(d.path(), "zzzzzzzz").is_err());
        assert!(entrada_do_documento(d.path(), "abcdef12").is_err());
    }
}
