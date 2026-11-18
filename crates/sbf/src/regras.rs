//! `sbf regras`: as regras de reconhecimento pelo terminal (spec 13, ADR-021).
//!
//! ```text
//! sbf regras --inventario <cofre> <subcomando> ...
//! ```
//!
//! O catálogo é `<cofre>/regras.sqlite`. Ver [`AJUDA`] para os subcomandos.
//!
//! # Quem pode ativar (D14)
//!
//! `ativar`, `consolidar`, `desativar`, `reverter` e `parametro` mudam o que o sbf
//! reconhece e calcula. Só rodam num **terminal interativo** (stdin é um terminal) e depois
//! de a pessoa digitar exatamente o que o comando pede (`regra@versao`, ou só `regra`). Não
//! há opção nem variável de ambiente que pule isso: o MCP e a IA, que não têm terminal,
//! nunca ativam. A ação só chama o catálogo com `Quem::Usuario` depois da confirmação.
//!
//! Nunca imprime o texto de um documento: só nomes de campos, valores extraídos e motivos.

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use sbf_regras::catalogo::{Catalogo, Esperado, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::Definicao;
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::simulacao::{ResumoDaDecisao, simular};
use sbf_regras::{
    Autor, Canal, Dicas, Entrada, ErroDasRegras, Estado, Origem, Quem, TipoDeEntrada,
    VERSAO_DO_ESQUEMA,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const AJUDA: &str = "\
uso: sbf regras --inventario <cofre> <subcomando>

  listar                                 as regras, o que está em vigor e as sugestões de consolidação
  mostrar <regra>                        versões, exemplos e histórico de estado
  importar <arquivo.json> [--script <arquivo.luau>] [--motivo <texto>]
                                         cria um RASCUNHO (nunca ativa)
  exemplo <regra> <arquivo.txt> (--nao-se-aplica | --esperado <arquivo.json>) [--nome <nome>]
                                         grava um exemplo de teste
  testar <regra> <versao>                roda os exemplos; sai com código 1 se algum falha
  simular <regra> <versao> <arquivo.txt>...
                                         o que muda se esta versão entrasse em vigor (só leitura)
  ativar <regra> <versao>                só num terminal interativo, com confirmação digitada
  consolidar <regra> <versao>            idem
  desativar <regra>                      idem
  reverter <regra>                       idem (volta à versão anterior)
  parametro <nome> <AAAA-MM> <arquivo.json> --fonte <url> [--ate AAAA-MM>]
                                         grava um parâmetro versionado; idem, com confirmação

Ativar, consolidar, desativar, reverter e parametro não rodam sem terminal: o MCP e a IA
não ativam.";

const LIMITE_DO_ARQUIVO: u64 = 1024 * 1024;

/// O que dá errado, já em português para o terminal.
#[derive(Debug, PartialEq, Eq)]
enum Falha {
    /// Uso errado da linha de comando (código 2).
    Uso(String),
    /// Falha da operação (código 1).
    Operacao(String),
}

impl From<ErroDasRegras> for Falha {
    fn from(e: ErroDasRegras) -> Self {
        Self::Operacao(e.to_string())
    }
}

fn uso(m: impl Into<String>) -> Falha {
    Falha::Uso(m.into())
}

fn op(m: impl Into<String>) -> Falha {
    Falha::Operacao(m.into())
}

/// A linha de comando separada: o cofre, o subcomando e o resto.
#[derive(Debug, Default, PartialEq, Eq)]
struct Linha {
    cofre: Option<PathBuf>,
    ajuda: bool,
    sub: Option<String>,
    resto: Vec<String>,
}

fn separar(args: &[String]) -> Result<Linha, Falha> {
    let mut l = Linha::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ajuda" | "--help" | "-h" if l.sub.is_none() => l.ajuda = true,
            "--inventario" if l.sub.is_none() => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| uso("--inventario pede a pasta do cofre"))?;
                l.cofre = Some(PathBuf::from(v));
                i += 1;
            }
            outro if l.sub.is_none() => {
                if outro.starts_with("--") {
                    return Err(uso(format!("argumento desconhecido: {outro}")));
                }
                l.sub = Some(outro.to_owned());
            }
            outro => l.resto.push(outro.to_owned()),
        }
        i += 1;
    }
    Ok(l)
}

/// Opções `--nome valor` e `--sinal` de um subcomando, mais os argumentos soltos.
#[derive(Debug, Default)]
struct Opcoes {
    soltos: Vec<String>,
    valores: BTreeMap<String, String>,
    sinais: Vec<String>,
}

fn opcoes(resto: &[String], com_valor: &[&str], sinais: &[&str]) -> Result<Opcoes, Falha> {
    let mut o = Opcoes::default();
    let mut i = 0;
    while i < resto.len() {
        let a = resto[i].as_str();
        if com_valor.contains(&a) {
            let v = resto
                .get(i + 1)
                .ok_or_else(|| uso(format!("{a} pede um valor")))?;
            if o.valores.insert(a.to_owned(), v.clone()).is_some() {
                return Err(uso(format!("{a} repetido")));
            }
            i += 1;
        } else if sinais.contains(&a) {
            o.sinais.push(a.to_owned());
        } else if a.starts_with("--") {
            return Err(uso(format!("argumento desconhecido: {a}")));
        } else {
            o.soltos.push(a.to_owned());
        }
        i += 1;
    }
    Ok(o)
}

fn versao_de(texto: &str) -> Result<u32, Falha> {
    texto.parse::<u32>().ok().filter(|v| *v > 0).ok_or_else(|| {
        uso(format!(
            "versão inválida `{texto}` (um número a partir de 1)"
        ))
    })
}

fn competencia_valida(c: &str) -> bool {
    let b = c.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..].iter().all(u8::is_ascii_digit)
        && matches!(
            &c[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

// ---------------------------------------------------------------------------------------
// Confirmação (D14)
// ---------------------------------------------------------------------------------------

/// A confirmação que muda o que o sbf reconhece. Recusa sem terminal; com terminal, a
/// pessoa precisa digitar exatamente `esperado`. Não existe atalho.
fn confirmar(
    terminal: bool,
    verbo: &str,
    resumo: &str,
    esperado: &str,
    entrada: &mut dyn BufRead,
    saida: &mut dyn Write,
) -> Result<(), Falha> {
    if !terminal {
        return Err(op(format!(
            "{verbo} só na janela ou num terminal interativo; o MCP e a IA não ativam. Nada foi alterado."
        )));
    }
    let _ = writeln!(saida, "{resumo}");
    let _ = write!(saida, "Para confirmar, digite exatamente `{esperado}`: ");
    let _ = saida.flush();
    let mut linha = String::new();
    let lidos = entrada
        .read_line(&mut linha)
        .map_err(|e| op(format!("não deu para ler a confirmação: {e}")))?;
    if lidos == 0 || linha.trim_end_matches(['\n', '\r']) != esperado {
        return Err(op("confirmação diferente do pedido; nada foi alterado."));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Arquivos e entradas
// ---------------------------------------------------------------------------------------

fn ler_texto(caminho: &Path) -> Result<String, Falha> {
    let meta = std::fs::metadata(caminho)
        .map_err(|e| op(format!("não deu para ler {}: {e}", caminho.display())))?;
    if !meta.is_file() {
        return Err(op(format!("{} não é um arquivo", caminho.display())));
    }
    if meta.len() > LIMITE_DO_ARQUIVO {
        return Err(op(format!(
            "{} passa de 1 MiB; o motor não lê arquivos desse tamanho",
            caminho.display()
        )));
    }
    std::fs::read_to_string(caminho).map_err(|e| {
        op(format!(
            "não deu para ler {} como texto (UTF-8): {e}",
            caminho.display()
        ))
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A [`Entrada`] de um arquivo `.txt`. PDF fica de fora: o extrator de PDF do
/// `sbf-reconhecimento` não é público; extraia o texto antes e passe o `.txt`.
fn entrada_do_arquivo(caminho: &Path) -> Result<Entrada, Falha> {
    let ext = caminho
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if ext.as_deref() != Some("txt") {
        return Err(op(format!(
            "{}: por enquanto só arquivos .txt (texto já extraído); PDF não é lido por este comando",
            caminho.display()
        )));
    }
    let texto = ler_texto(caminho)?;
    let nome = caminho
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    Ok(Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: hex_sha256(texto.as_bytes()),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Manual,
            id_externo: None,
        },
        sha256: Some(hex_sha256(texto.as_bytes())),
        mime: Some("text/plain".to_owned()),
        nome,
        texto: Some(texto),
        dicas: Dicas::default(),
        momento: None,
    })
}

// ---------------------------------------------------------------------------------------
// Ambiente: catálogo, executor, terminal
// ---------------------------------------------------------------------------------------

struct Ambiente<'a> {
    cofre: &'a Path,
    terminal: bool,
    entrada: &'a mut dyn BufRead,
    saida: &'a mut dyn Write,
    /// O executor dos scripts (um processo filho do próprio `sbf`).
    executor: &'a dyn Fn() -> Result<Arc<dyn ExecutorDeScript>, Falha>,
}

fn abrir_catalogo(cofre: &Path) -> Result<Catalogo, Falha> {
    if !cofre.is_dir() {
        return Err(op(format!("{} não é uma pasta de cofre", cofre.display())));
    }
    let mut c = Catalogo::abrir(&cofre.join("regras.sqlite"))?;
    c.semear_fabrica()?;
    Ok(c)
}

fn estado_texto(e: Estado) -> &'static str {
    match e {
        Estado::Rascunho => "rascunho",
        Estado::Ativa => "ativa",
        Estado::Consolidada => "consolidada",
        Estado::Desativada => "desativada",
    }
}

fn autor_texto(a: Autor) -> &'static str {
    match a {
        Autor::Usuario => "usuário",
        Autor::Ia => "IA",
        Autor::Fabrica => "fábrica",
        Autor::Correcao => "correção",
    }
}

fn curto(checksum: &str) -> &str {
    checksum.get(..12).unwrap_or(checksum)
}

fn w(saida: &mut dyn Write, linha: &str) {
    let _ = writeln!(saida, "{linha}");
}

// ---------------------------------------------------------------------------------------
// Subcomandos
// ---------------------------------------------------------------------------------------

fn listar(a: &mut Ambiente) -> Result<(), Falha> {
    let c = abrir_catalogo(a.cofre)?;
    let regras = c.regras()?;
    if regras.is_empty() {
        w(a.saida, "Nenhuma regra no catálogo.");
        return Ok(());
    }
    w(
        a.saida,
        &format!(
            "{:<34} {:<30} {:>6} {:<12} {:>7} {:>9} {:>7}",
            "regra", "tipo", "vigor", "estado", "versões", "execuções", "acerto"
        ),
    );
    for r in &regras {
        let vigor = r
            .versao_em_vigor
            .map_or_else(|| "-".to_owned(), |v| format!("v{v}"));
        let estado = r.estado.map_or("-", estado_texto);
        let acerto = r
            .taxa_de_acerto()
            .map_or_else(|| "-".to_owned(), |t| format!("{:.0}%", t * 100.0));
        w(
            a.saida,
            &format!(
                "{:<34} {:<30} {:>6} {:<12} {:>7} {:>9} {:>7}",
                r.id, r.tipo_documental, vigor, estado, r.versoes, r.execucoes, acerto
            ),
        );
    }
    let sugestoes = c.sugestoes_de_consolidacao()?;
    if !sugestoes.is_empty() {
        w(a.saida, "");
        w(
            a.saida,
            "Sugestões de consolidação (propostas aceitas seguidas):",
        );
        for (regra, aceitas) in sugestoes {
            w(
                a.saida,
                &format!(
                    "  {regra}: {aceitas} aceitas seguidas; veja `sbf regras ... consolidar {regra} <versao>`"
                ),
            );
        }
    }
    Ok(())
}

fn mostrar(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let [regra] = resto else {
        return Err(uso("uso: mostrar <regra>"));
    };
    let c = abrir_catalogo(a.cofre)?;
    let versoes = c.versoes(regra)?;
    if versoes.is_empty() {
        return Err(op(format!("regra `{regra}` não encontrada")));
    }
    w(a.saida, &format!("Regra {regra}"));
    for v in &versoes {
        w(
            a.saida,
            &format!(
                "  v{} {} · autor {} · checksum {} · criada {}{}{}{}",
                v.versao,
                estado_texto(v.estado),
                autor_texto(v.autor),
                curto(&v.checksum),
                v.criada_em,
                v.ativada_em
                    .as_deref()
                    .map_or(String::new(), |d| format!(" · ativada {d}")),
                v.desativada_em
                    .as_deref()
                    .map_or(String::new(), |d| format!(" · desativada {d}")),
                if v.script.is_some() {
                    " · com script"
                } else {
                    ""
                },
            ),
        );
        w(a.saida, &format!("      motivo: {}", v.motivo));
        for (de, para, quem, motivo, em) in c.historico_de_estado(regra, v.versao)? {
            w(
                a.saida,
                &format!(
                    "      {em}: {} -> {para} por {quem} ({motivo})",
                    de.as_deref().unwrap_or("-")
                ),
            );
        }
    }
    let exemplos = c.exemplos(regra)?;
    w(a.saida, &format!("Exemplos: {}", exemplos.len()));
    for e in exemplos {
        let esperado = match e.esperado {
            Esperado::NaoSeAplica => "não se aplica".to_owned(),
            Esperado::Reconhece {
                tipo_documental,
                campos,
            } => format!(
                "reconhece `{tipo_documental}` com {} campo(s)",
                campos.len()
            ),
        };
        w(a.saida, &format!("  {}: {esperado}", e.nome));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoDeRegra {
    regra: String,
    nome: String,
    definicao: Definicao,
}

fn importar(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let o = opcoes(resto, &["--script", "--motivo"], &[])?;
    let [arquivo] = o.soltos.as_slice() else {
        return Err(uso(
            "uso: importar <arquivo.json> [--script <arquivo.luau>] [--motivo <texto>]",
        ));
    };
    let json = ler_texto(Path::new(arquivo))?;
    let def: ArquivoDeRegra = serde_json::from_str(&json)
        .map_err(|e| op(format!("o JSON da regra não é válido: {e}")))?;
    let script = match o.valores.get("--script") {
        Some(p) => Some(ler_texto(Path::new(p))?),
        None => None,
    };
    if let Some(fonte) = &script {
        // O script confere antes de gravar: sintaxe e `transformar`.
        let ex = (a.executor)()?;
        ex.verificar(fonte)
            .map_err(|e| op(format!("o script não passou na verificação: {e}")))?;
    }
    let mut c = abrir_catalogo(a.cofre)?;
    let antes = c.versoes(&def.regra)?.len();
    let v = c.salvar_rascunho(NovaVersao {
        regra: def.regra,
        nome: def.nome,
        definicao: def.definicao,
        script,
        autor: Autor::Usuario,
        motivo: o
            .valores
            .get("--motivo")
            .cloned()
            .unwrap_or_else(|| "importada pelo terminal".to_owned()),
    })?;
    let depois = c.versoes(&v.regra)?.len();
    if depois == antes {
        w(
            a.saida,
            &format!(
                "{}: igual à versão {} (checksum {}); nada novo foi criado.",
                v.regra,
                v.versao,
                curto(&v.checksum)
            ),
        );
    } else {
        w(
            a.saida,
            &format!(
                "{}: criada a versão {} como rascunho (checksum {}). Teste e ative antes de valer.",
                v.regra,
                v.versao,
                curto(&v.checksum)
            ),
        );
    }
    Ok(())
}

fn exemplo(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let o = opcoes(resto, &["--esperado", "--nome"], &["--nao-se-aplica"])?;
    let [regra, arquivo] = o.soltos.as_slice() else {
        return Err(uso(
            "uso: exemplo <regra> <arquivo.txt> (--nao-se-aplica | --esperado <arquivo.json>) [--nome <nome>]",
        ));
    };
    let nao_se_aplica = !o.sinais.is_empty();
    let esperado = match (nao_se_aplica, o.valores.get("--esperado")) {
        (true, None) => Esperado::NaoSeAplica,
        (false, Some(p)) => serde_json::from_str::<Esperado>(&ler_texto(Path::new(p))?)
            .map_err(|e| op(format!("o JSON do esperado não é válido: {e}")))?,
        _ => return Err(uso("informe exatamente um: --nao-se-aplica ou --esperado")),
    };
    let entrada = entrada_do_arquivo(Path::new(arquivo))?;
    let nome = o
        .valores
        .get("--nome")
        .cloned()
        .or_else(|| entrada.nome.clone())
        .unwrap_or_else(|| "exemplo".to_owned());
    let mut c = abrir_catalogo(a.cofre)?;
    c.salvar_exemplo(regra, &nome, &entrada, esperado)?;
    w(a.saida, &format!("{regra}: exemplo `{nome}` gravado."));
    Ok(())
}

fn testar(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let [regra, versao] = resto else {
        return Err(uso("uso: testar <regra> <versao>"));
    };
    let versao = versao_de(versao)?;
    let c = abrir_catalogo(a.cofre)?;
    let ex = (a.executor)()?;
    let r = c.testar(regra, versao, Some(&*ex))?;
    if r.resultados.is_empty() {
        w(
            a.saida,
            &format!("{regra} v{versao}: sem exemplos; nada foi conferido."),
        );
    }
    for x in &r.resultados {
        if x.passou {
            w(a.saida, &format!("  passou: {}", x.nome));
        } else {
            w(a.saida, &format!("  FALHOU: {} ({})", x.nome, x.detalhe));
        }
    }
    if r.aprovado() {
        w(
            a.saida,
            &format!(
                "{regra} v{versao}: {} exemplo(s) passaram.",
                r.resultados.len()
            ),
        );
        Ok(())
    } else {
        Err(op(format!(
            "{regra} v{versao}: {} exemplo(s) falharam.",
            r.falhas().len()
        )))
    }
}

fn resumo_texto(r: &ResumoDaDecisao) -> String {
    match r {
        ResumoDaDecisao::Nenhuma => "nenhuma regra".to_owned(),
        ResumoDaDecisao::Ambiguo { regras } => format!("ambíguo entre {}", regras.join(", ")),
        ResumoDaDecisao::Reconhecido {
            regra,
            tipo_documental,
            campos,
            revisar,
            propostas,
        } => {
            let campos: Vec<String> = campos
                .iter()
                .map(|(n, v)| format!("{n}={}", valor_texto(v)))
                .collect();
            format!(
                "{regra} ({tipo_documental}){}; {} proposta(s); {}",
                if *revisar { ", revisar" } else { "" },
                propostas,
                campos.join(" ")
            )
        }
    }
}

fn valor_texto(v: &sbf_regras::Valor) -> String {
    use sbf_regras::Valor::{Booleano, Centavos, Competencia, Data, Inteiro, Texto};
    match v {
        Texto(s) | Data(s) | Competencia(s) => s.clone(),
        Centavos(c) => format!("{c}c"),
        Inteiro(n) => n.to_string(),
        Booleano(b) => b.to_string(),
    }
}

fn simular_versao(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let [regra, versao, arquivos @ ..] = resto else {
        return Err(uso("uso: simular <regra> <versao> <arquivo.txt>..."));
    };
    if arquivos.is_empty() {
        return Err(uso("simular pede ao menos um arquivo"));
    }
    let versao = versao_de(versao)?;
    let c = abrir_catalogo(a.cofre)?;
    let candidata = c.versao(regra, versao)?;
    let ex = (a.executor)()?;
    let mut entradas = Vec::new();
    for f in arquivos {
        entradas.push(entrada_do_arquivo(Path::new(f))?);
    }
    let vigentes = c.vigentes()?;
    let atual = Motor::novo(vigentes.clone(), c.parametros()?, Some(ex.clone()));
    let mut com: Vec<VersaoDeRegra> = vigentes.into_iter().filter(|v| v.regra != *regra).collect();
    com.push(candidata);
    let novo = Motor::novo(com, c.parametros()?, Some(ex));
    let d = simular(&atual, &novo, &entradas);
    let nomes: BTreeMap<&str, &str> = entradas
        .iter()
        .map(|e| (e.id.as_str(), e.nome.as_deref().unwrap_or("?")))
        .collect();
    w(
        a.saida,
        &format!(
            "Simulação de {regra} v{versao} sobre {} arquivo(s): {} igual(is), {} mudaria(m). Nada foi gravado.",
            entradas.len(),
            d.iguais,
            d.mudaram.len()
        ),
    );
    for m in &d.mudaram {
        w(
            a.saida,
            &format!(
                "  {}:",
                nomes.get(m.entrada.as_str()).copied().unwrap_or("?")
            ),
        );
        w(a.saida, &format!("    antes:  {}", resumo_texto(&m.antes)));
        w(a.saida, &format!("    depois: {}", resumo_texto(&m.depois)));
    }
    Ok(())
}

/// Ativar, consolidar, desativar e reverter: a confirmação vem antes de qualquer escrita.
fn mudar_estado(a: &mut Ambiente, acao: &str, resto: &[String]) -> Result<(), Falha> {
    let precisa_versao = matches!(acao, "ativar" | "consolidar");
    let (regra, versao) = match (precisa_versao, resto) {
        (true, [r, v]) => (r.clone(), Some(versao_de(v)?)),
        (false, [r]) => (r.clone(), None),
        (true, _) => return Err(uso(format!("uso: {acao} <regra> <versao>"))),
        (false, _) => return Err(uso(format!("uso: {acao} <regra>"))),
    };
    // O terminal vem primeiro: sem ele nem o catálogo é aberto.
    if !a.terminal {
        return Err(recusa_sem_terminal(acao));
    }
    let mut c = abrir_catalogo(a.cofre)?;
    let esperado = versao.map_or_else(|| regra.clone(), |v| format!("{regra}@{v}"));
    let resumo = match versao {
        Some(v) => {
            let alvo = c.versao(&regra, v)?;
            format!(
                "{acao} `{regra}` versão {v} (estado {}, autor {}, checksum {}): {}",
                estado_texto(alvo.estado),
                autor_texto(alvo.autor),
                curto(&alvo.checksum),
                alvo.motivo
            )
        }
        None => {
            let versoes = c.versoes(&regra)?;
            if versoes.is_empty() {
                return Err(op(format!("regra `{regra}` não encontrada")));
            }
            format!("{acao} `{regra}`")
        }
    };
    confirmar(a.terminal, acao, &resumo, &esperado, a.entrada, a.saida)?;
    match (acao, versao) {
        ("ativar", Some(v)) => {
            let ex = (a.executor)()?;
            c.ativar(&regra, v, &Quem::Usuario, Some(&*ex))?;
            w(a.saida, &format!("{regra} v{v}: ativada."));
        }
        ("consolidar", Some(v)) => {
            c.consolidar(&regra, v, &Quem::Usuario)?;
            w(a.saida, &format!("{regra} v{v}: consolidada."));
        }
        ("desativar", None) => {
            c.desativar(&regra, &Quem::Usuario)?;
            w(a.saida, &format!("{regra}: desativada."));
        }
        ("reverter", None) => {
            c.reverter(&regra, &Quem::Usuario)?;
            w(
                a.saida,
                &format!("{regra}: revertida para a versão anterior."),
            );
        }
        _ => return Err(uso("subcomando inesperado")),
    }
    Ok(())
}

fn recusa_sem_terminal(verbo: &str) -> Falha {
    op(format!(
        "{verbo} só na janela ou num terminal interativo; o MCP e a IA não ativam. Nada foi alterado."
    ))
}

fn parametro(a: &mut Ambiente, resto: &[String]) -> Result<(), Falha> {
    let o = opcoes(resto, &["--fonte", "--ate"], &[])?;
    let [nome, inicio, arquivo] = o.soltos.as_slice() else {
        return Err(uso(
            "uso: parametro <nome> <AAAA-MM> <arquivo.json> --fonte <url> [--ate AAAA-MM]",
        ));
    };
    let fonte = o
        .valores
        .get("--fonte")
        .ok_or_else(|| uso("parametro pede --fonte <url>"))?;
    let ate = o.valores.get("--ate").map(String::as_str);
    if !competencia_valida(inicio) || ate.is_some_and(|c| !competencia_valida(c)) {
        return Err(uso("a vigência é AAAA-MM"));
    }
    if !a.terminal {
        return Err(recusa_sem_terminal("parametro"));
    }
    let valor: serde_json::Value = serde_json::from_str(&ler_texto(Path::new(arquivo))?)
        .map_err(|e| op(format!("o JSON do parâmetro não é válido: {e}")))?;
    let esperado = format!("{nome}@{inicio}");
    let resumo = format!(
        "parametro `{nome}` vigente de {inicio}{} (fonte {fonte}); muda cálculos das regras que o usam",
        ate.map_or(String::new(), |f| format!(" até {f}"))
    );
    confirmar(
        a.terminal,
        "parametro",
        &resumo,
        &esperado,
        a.entrada,
        a.saida,
    )?;
    let mut c = abrir_catalogo(a.cofre)?;
    let p = c.salvar_parametro(nome, inicio, ate, valor, fonte)?;
    w(
        a.saida,
        &format!(
            "{}: gravada a versão {} (checksum {}).",
            p.nome,
            p.versao,
            curto(&p.checksum)
        ),
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Entrada do subcomando
// ---------------------------------------------------------------------------------------

fn despachar(l: &Linha, a: &mut Ambiente) -> Result<(), Falha> {
    let Some(sub) = l.sub.as_deref() else {
        return Err(uso("falta o subcomando"));
    };
    match sub {
        "listar" if l.resto.is_empty() => listar(a),
        "listar" => Err(uso("uso: listar")),
        "mostrar" => mostrar(a, &l.resto),
        "importar" => importar(a, &l.resto),
        "exemplo" => exemplo(a, &l.resto),
        "testar" => testar(a, &l.resto),
        "simular" => simular_versao(a, &l.resto),
        "ativar" | "consolidar" | "desativar" | "reverter" => mudar_estado(a, sub, &l.resto),
        "parametro" => parametro(a, &l.resto),
        outro => Err(uso(format!("subcomando desconhecido: {outro}"))),
    }
}

pub fn rodar(args: &[String]) -> ExitCode {
    let linha = match separar(args) {
        Ok(l) => l,
        Err(Falha::Uso(m) | Falha::Operacao(m)) => {
            eprintln!("sbf: {m}\n{AJUDA}");
            return ExitCode::from(2);
        }
    };
    if linha.ajuda || (linha.sub.is_none() && linha.cofre.is_none()) {
        println!("{AJUDA}");
        return ExitCode::SUCCESS;
    }
    let Some(cofre) = linha.cofre.clone() else {
        eprintln!("sbf: falta --inventario <cofre>.\n{AJUDA}");
        return ExitCode::from(2);
    };
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    let mut entrada = stdin.lock();
    let mut saida = std::io::stdout().lock();
    let executor = || -> Result<Arc<dyn ExecutorDeScript>, Falha> {
        crate::script::executor_de_script()
            .map_err(|e| op(format!("não deu para montar o executor de scripts: {e}")))
    };
    let mut ambiente = Ambiente {
        cofre: &cofre,
        terminal,
        entrada: &mut entrada,
        saida: &mut saida,
        executor: &executor,
    };
    match despachar(&linha, &mut ambiente) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Falha::Uso(m)) => {
            eprintln!("sbf: {m}");
            ExitCode::from(2)
        }
        Err(Falha::Operacao(m)) => {
            eprintln!("sbf: {m}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn separa_cofre_subcomando_e_resto() {
        let l = separar(&s(&["--inventario", "/c", "testar", "r", "2"])).unwrap();
        assert_eq!(l.cofre, Some(PathBuf::from("/c")));
        assert_eq!(l.sub.as_deref(), Some("testar"));
        assert_eq!(l.resto, s(&["r", "2"]));
        assert!(separar(&s(&["--inventario"])).is_err());
        assert!(separar(&s(&["--zzz"])).is_err());
    }

    #[test]
    fn confirmacao_recusa_sem_terminal_e_com_texto_diferente() {
        let mut saida = Vec::new();
        let e = confirmar(
            false,
            "ativar",
            "x",
            "r@1",
            &mut "r@1\n".as_bytes(),
            &mut saida,
        );
        assert!(matches!(e, Err(Falha::Operacao(m)) if m.contains("terminal interativo")));
        for digitado in ["r@2\n", "r\n", "\n", "", "R@1\n", " r@1\n"] {
            let e = confirmar(
                true,
                "ativar",
                "x",
                "r@1",
                &mut digitado.as_bytes(),
                &mut saida,
            );
            assert!(e.is_err(), "{digitado:?}");
        }
        assert!(
            confirmar(
                true,
                "ativar",
                "x",
                "r@1",
                &mut "r@1\n".as_bytes(),
                &mut saida
            )
            .is_ok()
        );
        assert!(
            confirmar(
                true,
                "ativar",
                "x",
                "r@1",
                &mut "r@1\r\n".as_bytes(),
                &mut saida
            )
            .is_ok()
        );
    }

    #[test]
    fn competencia_e_versao() {
        assert!(competencia_valida("2026-01"));
        assert!(!competencia_valida("2026-13"));
        assert!(!competencia_valida("26-01"));
        assert!(versao_de("0").is_err());
        assert_eq!(versao_de("3").unwrap(), 3);
    }

    #[test]
    fn so_aceita_txt() {
        let p = tempfile::tempdir().unwrap();
        let pdf = p.path().join("a.pdf");
        std::fs::write(&pdf, "x").unwrap();
        assert!(entrada_do_arquivo(&pdf).is_err());
        let txt = p.path().join("a.txt");
        std::fs::write(&txt, "Banco Exemplo").unwrap();
        let e = entrada_do_arquivo(&txt).unwrap();
        assert_eq!(e.id.len(), 64);
        assert_eq!(e.nome.as_deref(), Some("a.txt"));
    }
}
