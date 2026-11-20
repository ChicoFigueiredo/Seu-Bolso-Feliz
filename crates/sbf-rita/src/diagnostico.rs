//! O diagnóstico do pré-teste que o modelo consegue usar (spec 15, D15): em vez de "não se
//! cumpre", diz **o que** não aparece no texto e **onde** olhar. Modelos pequenos repetem o
//! erro quando o retorno é vago; com a linha candidata na mão, acertam.
//!
//! O que sai daqui vai ao modelo (que já tem o texto inteiro, D13) e à tela do usuário, nunca a
//! log. Cada entrada é curta.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use sbf_regras::Entrada;
use sbf_regras::definicao::{
    Condicao, Definicao, FonteDaExtracao, Formato, normalizar, normalizar_ocr,
};

use crate::texto::cortar;

/// Quantas linhas do texto uma sugestão traz.
const LINHAS_SUGERIDAS: usize = 3;
/// Tamanho máximo de cada linha sugerida.
const TAMANHO_DA_LINHA: usize = 70;
/// Quantas entradas o diagnóstico tem, no máximo.
const MAXIMO_DE_ENTRADAS: usize = 6;
/// Tamanho máximo de cada entrada.
const TAMANHO_DA_ENTRADA: usize = 420;

/// Palavras do pedido que não dizem nada sobre o documento.
const SEM_VALOR: [&str; 14] = [
    "isto", "padrao", "determ", "campo", "numero", "cria", "criar", "compro", "esse", "essa",
    "sobre", "deste", "desta", "fazer",
];

/// Nomes de campo que guardam um identificador (número do contrato, da conta...).
const NOMES_DE_IDENTIFICADOR: [&str; 15] = [
    "contrato",
    "conta",
    "numero",
    "codigo",
    "protocolo",
    "matricula",
    "cartao",
    "agencia",
    "fatura",
    "nota",
    "proposta",
    "pedido",
    "documento",
    "inscricao",
    "apolice",
];

fn regex_fixo(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|_| unreachable!("regex fixo inválido"))
}

static IDENTIFICADOR: LazyLock<Regex> = LazyLock::new(|| regex_fixo(r"\d[\d.\-/]{4,}\d"));
static DINHEIRO: LazyLock<Regex> = LazyLock::new(|| regex_fixo(r"R\$|\d,\d{2}\b"));
static DATA_NUMERICA: LazyLock<Regex> =
    LazyLock::new(|| regex_fixo(r"\b\d{1,2}/\d{1,2}/\d{2,4}\b|\b\d{1,2}/\d{4}\b"));
static MES_POR_EXTENSO: LazyLock<Regex> = LazyLock::new(|| {
    regex_fixo(
        r"\b(janeiro|fevereiro|marco|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro)\b",
    )
});

/// O que o pré-teste achou de errado no item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostico {
    /// Entradas curtas para o modelo (e a tela).
    pub linhas: Vec<String>,
    /// Quantas condições de `detectar` não se cumprem (o item casa quando é zero).
    pub condicoes_falhando: usize,
    /// Os campos declarados (`extrair` e `obrigatorios`) que não saíram do item.
    pub faltam: Vec<String>,
}

fn palavras(s: &str) -> Vec<String> {
    normalizar(s).split(' ').map(str::to_owned).collect()
}

fn radical(p: &str) -> String {
    p.chars().take(6).collect()
}

/// Os radicais (seis primeiras letras) das palavras de quatro letras ou mais.
fn radicais(s: &str) -> BTreeSet<String> {
    palavras(s)
        .iter()
        .filter(|p| p.chars().count() >= 4)
        .map(|p| radical(p))
        .collect()
}

fn linhas_do(texto: &str) -> Vec<&str> {
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// Até `n` linhas do texto que mais se parecem com `alvo` (por radicais de palavras em
/// comum). Vazio se nenhuma tem palavra em comum.
pub fn linhas_parecidas(texto: &str, alvo: &str, n: usize) -> Vec<String> {
    let procurados = radicais(alvo);
    if procurados.is_empty() {
        return Vec::new();
    }
    let mut pontuadas: Vec<(usize, &str)> = linhas_do(texto)
        .into_iter()
        .map(|l| (radicais(l).intersection(&procurados).count(), l))
        .filter(|(p, _)| *p > 0)
        .collect();
    pontuadas.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
    let mut vistas = BTreeSet::new();
    pontuadas
        .into_iter()
        .filter(|(_, l)| vistas.insert(*l))
        .take(n)
        .map(|(_, l)| cortar(l, TAMANHO_DA_LINHA))
        .collect()
}

fn entre_aspas(linhas: &[String]) -> String {
    linhas
        .iter()
        .map(|l| format!("«{l}»"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Até três linhas do texto onde o valor do campo `nome` provavelmente está: as que trazem
/// uma palavra do nome do campo, o formato esperado (dígitos de um identificador, `R$`, mês por
/// extenso) ou palavras do pedido.
fn candidatas_do_campo(nome: &str, formato: Formato, texto: &str, pedido: &str) -> Vec<String> {
    let pedido: BTreeSet<String> = radicais(pedido)
        .into_iter()
        .filter(|r| !SEM_VALOR.contains(&r.as_str()))
        .collect();
    let pedacos: Vec<&str> = nome.split('_').filter(|p| p.len() >= 3).collect();
    let identificador = pedacos.iter().any(|p| NOMES_DE_IDENTIFICADOR.contains(p));
    let mut pontuadas: Vec<(usize, usize, &str)> = Vec::new();
    for (ordem, linha) in linhas_do(texto).into_iter().enumerate() {
        let norm = normalizar(linha);
        let ps: Vec<&str> = norm.split(' ').collect();
        let mut p = 0;
        if pedacos
            .iter()
            .any(|n| *n != "centavos" && ps.iter().any(|w| w.starts_with(n)))
        {
            p += 3;
        }
        let tem_identificador = IDENTIFICADOR.is_match(linha) && !DATA_NUMERICA.is_match(linha);
        match formato {
            Formato::Centavos if DINHEIRO.is_match(linha) => p += 4,
            Formato::Data if DATA_NUMERICA.is_match(linha) || MES_POR_EXTENSO.is_match(&norm) => {
                p += 3;
            }
            Formato::Competencia
                if MES_POR_EXTENSO.is_match(&norm) || DATA_NUMERICA.is_match(linha) =>
            {
                p += 3;
            }
            Formato::Inteiro if linha.chars().any(|c| c.is_ascii_digit()) => p += 2,
            _ => {}
        }
        if identificador && tem_identificador {
            p += 5;
        }
        p += radicais(linha).intersection(&pedido).count().min(2);
        if p >= 3 {
            pontuadas.push((p, ordem, linha));
        }
    }
    pontuadas.sort_by_key(|(p, ordem, _)| (std::cmp::Reverse(*p), *ordem));
    pontuadas
        .into_iter()
        .take(LINHAS_SUGERIDAS)
        .map(|(_, _, l)| cortar(l, TAMANHO_DA_LINHA))
        .collect()
}

/// O token inteiro (dígitos, letras e `-./` em volta) quando o trecho de `ini` a `fim` é só
/// parte de um número maior; `None` se o trecho já é o token ou não é um número.
fn numero_cortado(texto: &str, ini: usize, fim: usize) -> Option<String> {
    let parte = texto.get(ini..fim)?.trim();
    if parte.chars().filter(char::is_ascii_digit).count() < 4
        || !parte
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '.' | '/'))
    {
        return None;
    }
    let do_token = |c: char| c.is_alphanumeric() || matches!(c, '-' | '.' | '/');
    let mut comeco = ini;
    for (i, c) in texto[..ini].char_indices().rev() {
        if !do_token(c) {
            break;
        }
        comeco = i;
    }
    let mut termino = fim;
    for (i, c) in texto[fim..].char_indices() {
        if !do_token(c) {
            break;
        }
        termino = fim + i + c.len_utf8();
    }
    let token = texto[comeco..termino].trim_matches(|c: char| matches!(c, '-' | '.' | '/'));
    (token.len() > parte.len()).then(|| token.to_owned())
}

/// O pedido tem uma frase em que `campo` (ou uma palavra do nome dele) "determina" o padrão.
fn determina(pedido: &str, campo: &str) -> bool {
    pedido
        .split(['.', ';', '\n', '!', '?'])
        .map(normalizar)
        .any(|frase| {
            frase.contains("determin")
                && campo
                    .split('_')
                    .filter(|p| p.len() >= 4)
                    .any(|p| frase.split(' ').any(|w| w.starts_with(p)))
        })
}

/// Alguma condição `contem` de `detectar` traz o valor extraído.
fn alguma_condicao_usa(def: &Definicao, valor: &str) -> bool {
    let v = normalizar(valor);
    def.detectar
        .todos
        .iter()
        .chain(&def.detectar.algum)
        .any(|c| matches!(c, Condicao::Contem(t) if normalizar(t).contains(&v)))
}

fn texto_da_fonte(item: &Entrada, fonte: FonteDaExtracao, texto: &str) -> Option<String> {
    match fonte {
        FonteDaExtracao::Texto => Some(texto.to_owned()),
        FonteDaExtracao::Descricao => item.dicas.descricao.clone(),
        FonteDaExtracao::Contraparte => item.dicas.contraparte.clone(),
        FonteDaExtracao::ContraparteDoc => item.dicas.contraparte_doc.clone(),
    }
}

/// Confere, no item, cada condição de `detectar` e cada campo declarado. `pedido` é o texto do
/// usuário (as palavras dele ajudam a achar as linhas candidatas).
pub fn diagnosticar(def: &Definicao, item: &Entrada, pedido: &str) -> Diagnostico {
    let texto_bruto = item.texto.as_deref().unwrap_or("");
    let texto = if def.texto_de_ocr {
        normalizar_ocr(texto_bruto)
    } else {
        texto_bruto.to_owned()
    };
    let normalizado = normalizar(&texto);
    let mut d = Diagnostico::default();

    let cumpre = |c: &Condicao| match c {
        Condicao::Contem(t) => Some(normalizado.contains(&normalizar(t))),
        Condicao::Regex(p) => Regex::new(p).ok().map(|r| r.is_match(&texto)),
        _ => None,
    };
    for (i, c) in def.detectar.todos.iter().enumerate() {
        if cumpre(c) != Some(false) {
            continue;
        }
        d.condicoes_falhando += 1;
        d.linhas.push(match c {
            Condicao::Contem(t) => {
                let sugestao = linhas_parecidas(&texto, t, LINHAS_SUGERIDAS);
                let fim = if sugestao.is_empty() {
                    "Nenhuma linha se parece com ele: remova esta condição.".to_owned()
                } else {
                    format!(
                        "Linhas parecidas: {}. Use um trecho que está no texto ou remova esta condição.",
                        entre_aspas(&sugestao)
                    )
                };
                format!(
                    "a condição todos[{i}] não se cumpre: `{}` não aparece no texto do item. {fim}",
                    cortar(t, 60)
                )
            }
            Condicao::Regex(p) => format!(
                "a condição todos[{i}] não se cumpre: o regex `{}` não casa com nada no texto do item. Teste-o com `testar_regex` ou troque por `contem`.",
                cortar(p, 60)
            ),
            _ => format!("a condição todos[{i}] não se cumpre neste item."),
        });
    }
    if !def.detectar.algum.is_empty()
        && !def.detectar.algum.iter().any(|c| cumpre(c) != Some(false))
    {
        d.condicoes_falhando += 1;
        d.linhas
            .push("nenhuma condição de `algum` se cumpre neste texto.".to_owned());
    }
    for (i, c) in def.detectar.nenhum.iter().enumerate() {
        if cumpre(c) == Some(true) {
            d.condicoes_falhando += 1;
            d.linhas.push(format!(
                "a condição nenhum[{i}] aparece no texto do item: remova-a."
            ));
        }
    }

    let mut nomes: Vec<&String> = def.obrigatorios.iter().collect();
    nomes.extend(def.extrair.keys().filter(|k| !def.obrigatorios.contains(k)));
    for nome in nomes {
        let rotulo = if def.obrigatorios.contains(nome) {
            "o campo obrigatório"
        } else {
            "o campo"
        };
        let Some(e) = def.extrair.get(nome) else {
            d.faltam.push(nome.clone());
            d.linhas.push(format!(
                "{rotulo} `{nome}` não está em `extrair`: declare o regex dele ou tire-o de `obrigatorios`."
            ));
            continue;
        };
        let Some(fonte) = texto_da_fonte(item, e.fonte, &texto) else {
            d.faltam.push(nome.clone());
            d.linhas.push(format!(
                "{rotulo} `{nome}`: o item não tem a fonte `{:?}`.",
                e.fonte
            ));
            continue;
        };
        let Ok(r) = Regex::new(&e.regex) else {
            continue;
        };
        let casamento = r.captures(&fonte).and_then(|c| c.get(e.grupo));
        let achado = casamento
            .map(|m| m.as_str().trim().to_owned())
            .filter(|m| !m.is_empty());
        match achado {
            None => {
                d.faltam.push(nome.clone());
                let candidatas = if e.fonte == FonteDaExtracao::Texto {
                    candidatas_do_campo(nome, e.formato, &fonte, pedido)
                } else {
                    Vec::new()
                };
                let fim = if candidatas.is_empty() {
                    "Não achei no texto uma linha que pareça ter este valor: se ele não está no texto, remova o campo.".to_owned()
                } else {
                    format!(
                        "Linhas candidatas (o regex não casa com nenhuma): {}. Teste a correção com `testar_regex`.",
                        entre_aspas(&candidatas)
                    )
                };
                d.linhas.push(format!(
                    "{rotulo} `{nome}`: o regex `{}` não acha nada no texto. {fim}",
                    cortar(&e.regex, 60)
                ));
            }
            Some(trecho) => {
                if e.formato
                    .aplicar_em(&trecho, item.momento.as_deref())
                    .is_none()
                {
                    d.faltam.push(nome.clone());
                    d.linhas.push(e.formato.motivo_da_falha(
                        nome,
                        &trecho,
                        item.momento.as_deref(),
                    ));
                } else if let Some(inteiro) = casamento
                    .filter(|_| {
                        e.fonte == FonteDaExtracao::Texto
                            && matches!(
                                e.formato,
                                Formato::Texto | Formato::Inteiro | Formato::Minusculas
                            )
                    })
                    .and_then(|m| numero_cortado(&fonte, m.start(), m.end()))
                {
                    d.faltam.push(nome.clone());
                    d.linhas.push(format!(
                        "{rotulo} `{nome}`: o regex pegou só parte do número: «{}» está dentro de «{}». Capture o número inteiro (ex.: `\\d{{12}}-\\d`, ou ancore com `\\b`).",
                        cortar(&trecho, 40),
                        cortar(&inteiro, 40)
                    ));
                } else if determina(pedido, nome)
                    && NOMES_DE_IDENTIFICADOR
                        .iter()
                        .any(|n| nome.split('_').any(|p| p == *n))
                    && !alguma_condicao_usa(def, &trecho)
                {
                    d.faltam.push(nome.clone());
                    d.linhas.push(format!(
                        "o pedido diz que `{nome}` determina o padrão, mas nenhuma condição de `detectar` usa o valor dele: acrescente `{{\"contem\": \"{}\"}}` em `todos`.",
                        cortar(&trecho, 40)
                    ));
                }
            }
        }
    }
    d.linhas.truncate(MAXIMO_DE_ENTRADAS);
    for l in &mut d.linhas {
        *l = cortar(l, TAMANHO_DA_ENTRADA);
    }
    d
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contexto::entrada_de_documento;

    const TEXTO: &str = "Prestação atual\nR$ 1.234,56\nVencimento em 25 de Novembro\nSobre o contrato\n123456789012-3\nPrazo contratado 360 meses";

    fn item() -> Entrada {
        entrada_de_documento("aa", Some("a.txt"), None, Some(TEXTO))
    }

    #[test]
    fn linhas_parecidas_usa_palavras_em_comum() {
        let l = linhas_parecidas(TEXTO, "prestacao vigente", 3);
        assert_eq!(l, vec!["Prestação atual"]);
        assert!(linhas_parecidas(TEXTO, "caixa", 3).is_empty());
    }

    #[test]
    fn condicao_ausente_diz_o_trecho_e_o_que_fazer() {
        let mut d = Definicao::simples("x", "prestacao atual");
        d.detectar.todos.push(Condicao::Contem("caixa".into()));
        let g = diagnosticar(&d, &item(), "");
        assert_eq!(g.condicoes_falhando, 1);
        assert!(g.linhas[0].contains("todos[1]"), "{:?}", g.linhas);
        assert!(g.linhas[0].contains("`caixa` não aparece no texto do item"));
        assert!(g.linhas[0].contains("remova esta condição"));
    }

    #[test]
    fn numero_cortado_acha_o_token_inteiro() {
        let t = "Contrato 123456789012-3 vigente";
        let ini = t.find("23456789012").expect("pos");
        assert_eq!(
            numero_cortado(t, ini, ini + 11 + 2).as_deref(),
            Some("123456789012-3")
        );
        let todo = t.find("123456789012-3").expect("pos");
        assert_eq!(numero_cortado(t, todo, todo + 14), None);
        assert_eq!(numero_cortado(t, 0, 8), None, "palavra não é número");
    }

    #[test]
    fn identificador_que_o_pedido_diz_que_determina_precisa_de_condicao() {
        use sbf_regras::definicao::Extracao;
        let mut d = Definicao::simples("x", "prestacao atual");
        d.extrair.insert(
            "contrato".into(),
            Extracao {
                regex: r"(\d{12}-\d)".into(),
                grupo: 1,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Texto,
            },
        );
        let pedido = "O que determina é o título e o número do contrato. A data muda.";
        let g = diagnosticar(&d, &item(), pedido);
        assert_eq!(g.faltam, vec!["contrato"], "{:?}", g.linhas);
        assert!(
            g.linhas[0].contains("\"contem\": \"123456789012-3\""),
            "{:?}",
            g.linhas
        );
        d.detectar
            .todos
            .push(Condicao::Contem("123456789012-3".into()));
        assert!(diagnosticar(&d, &item(), pedido).faltam.is_empty());
        // Sem a palavra "determina" para o contrato, não exige nada.
        d.detectar.todos.pop();
        assert!(
            diagnosticar(&d, &item(), "o contrato é um campo")
                .faltam
                .is_empty()
        );
    }

    #[test]
    fn campo_sem_achado_mostra_as_linhas_candidatas() {
        use sbf_regras::definicao::Extracao;
        let mut d = Definicao::simples("x", "prestacao atual");
        d.extrair.insert(
            "contrato".into(),
            Extracao {
                regex: r"(\d{6}\s*\d{6}\s*\d{2})".into(),
                grupo: 1,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Texto,
            },
        );
        d.obrigatorios = vec!["contrato".into()];
        let g = diagnosticar(&d, &item(), "o número do contrato determina");
        assert_eq!(g.faltam, vec!["contrato"]);
        let l = &g.linhas[0];
        assert!(l.contains("`contrato`"), "{l}");
        assert!(l.contains("«123456789012-3»"), "{l}");
        assert!(l.contains("testar_regex"), "{l}");
        assert!(!l.contains("contratado"), "{l}");
    }
}
