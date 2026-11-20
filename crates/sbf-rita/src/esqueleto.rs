//! O esqueleto de definição (spec 15, "menos exploração"): antes de falar com o modelo, o harness
//! monta, **sem IA**, a partir do pedido e do texto do item, uma definição já válida e a pré-testa.
//! O primeiro prompt traz o esqueleto e o resultado do pré-teste dele; o modelo só ajusta e chama
//! `propor_padrao` (ou propõe direto, se o esqueleto já passou).
//!
//! Só entra no esqueleto o que o pedido ou o texto sustentam:
//!
//! - **detectar**: as linhas do texto que o pedido cita (entre aspas ou parênteses, ou a linha
//!   inteira escrita no pedido) e, se o pedido diz que o número do contrato (da conta...)
//!   determina, o identificador longo do texto;
//! - **extrair**: o identificador (`contrato`), o primeiro `R$` depois do título
//!   (`valor_centavos`) e a data (`competencia`, ou `vencimento`) com o rótulo que vem antes dela
//!   no texto. Cada campo é conferido contra o próprio texto, com o `momento` do item; o que não
//!   sai fica de fora e o motivo vai junto.
//!
//! O que sai daqui vai ao modelo dentro de um bloco de dados, nunca a log.

use std::collections::BTreeSet;

use regex::Regex;
use sbf_regras::catalogo::{NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::{
    Condicao, Definicao, Extracao, FonteDaExtracao, Formato, normalizar, normalizar_ocr,
};
use sbf_regras::{Autor, Entrada, Estado};

use crate::contexto::Contexto;
use crate::diagnostico::{identificador_em, regex_da_data, regex_do_numero, regex_do_valor};
use crate::ferramentas::catalogo_do;
use crate::pre_teste::pre_testar_versao;
use crate::texto::cortar;

/// Identificadores que, só de aparecerem no pedido, são o campo (`contrato`, `protocolo`...).
const IDENTIFICADORES_DIRETOS: [&str; 6] = [
    "contrato",
    "protocolo",
    "matricula",
    "apolice",
    "inscricao",
    "proposta",
];
/// Identificadores que só valem depois de "número"/"código" (`número da conta`).
const IDENTIFICADORES_COM_NUMERO: [&str; 6] =
    ["conta", "cartao", "agencia", "fatura", "nota", "pedido"];

/// O que o harness monta sem o modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct Esqueleto {
    pub regra: String,
    pub nome: String,
    pub definicao: Definicao,
    /// Campos que o pedido sugere mas que não saíram do texto, com o porquê.
    pub fora: Vec<String>,
}

fn regex_fixo(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|_| unreachable!("regex fixo inválido"))
}

fn tem_digitos_longos(s: &str) -> bool {
    let mut corrida = 0;
    for c in s.chars() {
        if c.is_ascii_digit() {
            corrida += 1;
            if corrida >= 4 {
                return true;
            }
        } else {
            corrida = 0;
        }
    }
    false
}

/// Os trechos entre aspas, parênteses ou «» do pedido.
fn citados(pedido: &str) -> Vec<String> {
    let mut saida = Vec::new();
    let pares = [('(', ')'), ('"', '"'), ('«', '»'), ('“', '”'), ('\'', '\'')];
    for (abre, fecha) in pares {
        let mut resto = pedido;
        while let Some(i) = resto.find(abre) {
            let depois = &resto[i + abre.len_utf8()..];
            let Some(f) = depois.find(fecha) else { break };
            saida.push(depois[..f].to_owned());
            resto = &depois[f + fecha.len_utf8()..];
        }
    }
    saida
}

fn contem_palavras(grande: &str, trecho: &str) -> bool {
    format!(" {grande} ").contains(&format!(" {trecho} "))
}

/// Um trecho que serve de condição: letras, sem dígitos longos e de tamanho razoável.
fn serve_de_ancora(norm: &str, minimo: usize) -> bool {
    (minimo..=60).contains(&norm.chars().count())
        && norm.chars().any(char::is_alphabetic)
        && !tem_digitos_longos(norm)
}

/// As condições `contem` de título: o que o pedido cita e está no texto; senão, a primeira linha
/// de cima, curta, sem dígitos. Devolve também o índice da linha (para achar o valor depois dela).
fn titulos(linhas: &[&str], pedido: &str) -> Vec<(usize, String)> {
    let ped = normalizar(pedido);
    let normalizadas: Vec<String> = linhas.iter().map(|l| normalizar(l)).collect();
    let mut achados: Vec<(usize, String)> = Vec::new();
    let mut vistos = BTreeSet::new();
    let mut guardar = |i: usize, t: String, achados: &mut Vec<(usize, String)>| {
        if achados.len() < 2 && vistos.insert(t.clone()) {
            achados.push((i, t));
        }
    };
    for c in citados(pedido) {
        let n = normalizar(&c);
        if !serve_de_ancora(&n, 5) {
            continue;
        }
        if let Some(i) = normalizadas.iter().position(|l| contem_palavras(l, &n)) {
            guardar(i, n, &mut achados);
        }
    }
    for (i, n) in normalizadas.iter().enumerate() {
        if serve_de_ancora(n, 5) && n.contains(' ') && contem_palavras(&ped, n) {
            guardar(i, n.clone(), &mut achados);
        }
    }
    if achados.is_empty() {
        // O pedido não cita nada que esteja no texto: a primeira linha de cima que é só texto.
        let escolhida = normalizadas.iter().enumerate().find(|(_, n)| {
            serve_de_ancora(n, 8)
                && n.contains(' ')
                && !n.chars().any(|c| c.is_ascii_digit())
                && n.chars().filter(|c| c.is_alphabetic()).count() * 10 >= n.chars().count() * 8
        });
        if let Some((i, n)) = escolhida {
            guardar(i, n.clone(), &mut achados);
        }
    }
    achados
}

/// O nome do identificador que o pedido quer (`contrato`), se houver.
fn nome_do_identificador(pedido: &str) -> Option<&'static str> {
    let ped = normalizar(pedido);
    let palavras: Vec<&str> = ped.split(' ').collect();
    for (i, p) in palavras.iter().enumerate() {
        if let Some(n) = IDENTIFICADORES_DIRETOS.iter().find(|n| p.starts_with(**n)) {
            return Some(n);
        }
        if let Some(n) = IDENTIFICADORES_COM_NUMERO
            .iter()
            .find(|n| p.starts_with(**n))
        {
            let antes = palavras[i.saturating_sub(3)..i]
                .iter()
                .any(|w| matches!(*w, "numero" | "num" | "codigo" | "cod" | "n"));
            if antes {
                return Some(n);
            }
        }
    }
    None
}

/// O pedido diz, numa frase com o identificador, que ele determina, identifica ou reconhece o
/// documento (então o número entra em `detectar`).
fn pede_como_condicao(pedido: &str, nome: &str) -> bool {
    pedido
        .split(['.', ';', '\n', '!', '?'])
        .map(normalizar)
        .any(|frase| {
            let f = format!(" {frase} ");
            f.contains(&format!(" {nome}"))
                && [
                    "determin",
                    "reconhec",
                    "identific",
                    "distingu",
                    "defin",
                    " pelo ",
                    " pela ",
                ]
                .iter()
                .any(|g| f.contains(g))
        })
}

/// O nome do padrão: "é um comprovante de ..." ou "padrão para o comprovante de ..." no pedido;
/// senão o título achado no texto.
fn nome_do_pedido(pedido: &str, titulo: Option<&str>) -> String {
    static E_UM: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        regex_fixo(r"(?i)(?:\b[ée]|trata-se de)\s+(?:um|uma)\s+([^.;:\n]{4,90})")
    });
    static PADRAO_PARA: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        regex_fixo(
            r"(?i)\b(?:padr[aã]o|regra)\s+(?:para|d[eoa])\s+(?:(?:o|a|os|as|um|uma)\s+)?([^.;:\n]{4,90})",
        )
    });
    static CORTE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        regex_fixo(
            r"(?i)(?:,|\(|\s+que\s+|\s+cujo|\s+cuja|\s+onde\s+|\s+e\s+o\s+|\s+e\s+a\s+|\s+com\s+o\s+)",
        )
    });
    let achado = E_UM
        .captures(pedido)
        .or_else(|| PADRAO_PARA.captures(pedido))
        .and_then(|c| c.get(1))
        .map(|m| {
            let m = m.as_str();
            let fim = CORTE.find(m).map_or(m.len(), |c| c.start());
            m[..fim].trim().to_owned()
        })
        .filter(|n| n.chars().filter(|c| c.is_alphabetic()).count() >= 6);
    let base = achado.unwrap_or_else(|| titulo.unwrap_or("padrão do documento").to_owned());
    let mut cs = base.chars();
    let nome = cs
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + cs.as_str())
        .unwrap_or_default();
    cortar(&nome, 80)
}

fn slug(nome: &str) -> String {
    let s = normalizar(nome).replace(' ', "-");
    let s: String = s.chars().take(60).collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "padrao".to_owned() } else { s }
}

/// A extração conferida contra o texto: o campo só entra se o regex casa e o formato aceita.
fn conferir(
    regex: &str,
    formato: Formato,
    texto: &str,
    momento: Option<&str>,
    campo: &str,
) -> Result<Extracao, String> {
    let r = Regex::new(regex).map_err(|_| format!("`{campo}`: o regex montado não compila"))?;
    let bruto = r
        .captures(texto)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_owned())
        .ok_or_else(|| format!("`{campo}`: não achei o valor no texto"))?;
    formato
        .aplicar_em(&bruto, momento)
        .ok_or_else(|| formato.motivo_da_falha(campo, &bruto, momento))?;
    Ok(Extracao {
        regex: regex.to_owned(),
        grupo: 1,
        formato,
        fonte: FonteDaExtracao::Texto,
    })
}

/// O campo de data que o pedido quer: `competencia` (competência, ciclo) ou `vencimento`.
fn campo_de_data(pedido: &str) -> Option<(&'static str, Formato)> {
    let ped = normalizar(pedido);
    let tem = |p: &str| ped.split(' ').any(|w| w.starts_with(p));
    if tem("competenc") || tem("ciclo") {
        Some(("competencia", Formato::Competencia))
    } else if tem("vencimento") {
        Some(("vencimento", Formato::Data))
    } else {
        None
    }
}

/// A linha de data que o campo quer: a que traz o rótulo que o pedido cita (vencimento...); senão
/// a primeira com data. Devolve o regex pronto.
fn regex_da_data_do_campo(linhas: &[&str], pedido: &str, formato: Formato) -> Option<String> {
    let ped = normalizar(pedido);
    let dicas: Vec<&str> = ["vencimento", "emissao", "pagamento", "referen", "competenc"]
        .into_iter()
        .filter(|d| ped.split(' ').any(|w| w.starts_with(d)))
        .collect();
    let mut melhor: Option<(usize, String)> = None;
    for (i, l) in linhas.iter().enumerate() {
        let anterior = i.checked_sub(1).map(|j| linhas[j]);
        let Some(rx) = regex_da_data(anterior, l, formato) else {
            continue;
        };
        let norm = normalizar(&format!("{} {l}", anterior.unwrap_or("")));
        let pontos = usize::from(dicas.iter().any(|d| norm.contains(d)));
        if melhor.as_ref().is_none_or(|(p, _)| pontos > *p) {
            melhor = Some((pontos, rx));
        }
    }
    melhor.map(|(_, r)| r)
}

/// Monta o esqueleto, ou `None` se o item não tem texto ou nada do pedido se acha nele.
pub fn montar(item: &Entrada, pedido: &str) -> Option<Esqueleto> {
    let bruto = item.texto.as_deref().filter(|t| !t.trim().is_empty())?;
    let de_ocr = item
        .mime
        .as_deref()
        .is_some_and(|m| m.starts_with("image/"));
    let texto = if de_ocr {
        normalizar_ocr(bruto)
    } else {
        bruto.to_owned()
    };
    let linhas: Vec<&str> = texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let momento = item.momento.as_deref();
    let titulos = titulos(&linhas, pedido);
    let mut todos: Vec<Condicao> = titulos
        .iter()
        .map(|(_, t)| Condicao::Contem(t.clone()))
        .collect();
    let mut extrair = std::collections::BTreeMap::new();
    let mut fora = Vec::new();

    // O identificador (número do contrato...).
    if let Some(nome) = nome_do_identificador(pedido) {
        let candidatos: Vec<&str> = linhas
            .iter()
            .filter_map(|l| identificador_em(l).map(|_| *l))
            .collect();
        let preferida = candidatos
            .iter()
            .find(|l| normalizar(l).split(' ').any(|w| w.starts_with(nome)))
            .or_else(|| candidatos.first())
            .and_then(|l| identificador_em(l));
        match preferida {
            Some(token) => {
                if pede_como_condicao(pedido, nome) {
                    todos.push(Condicao::Contem(token.to_owned()));
                }
                match conferir(
                    &regex_do_numero(token),
                    Formato::Texto,
                    &texto,
                    momento,
                    nome,
                ) {
                    Ok(e) => {
                        extrair.insert(nome.to_owned(), e);
                    }
                    Err(m) => fora.push(m),
                }
            }
            None => fora.push(format!(
                "`{nome}`: não achei no texto um número longo para ele"
            )),
        }
    }

    // O valor: o primeiro `R$` depois do título (ou o primeiro do texto).
    let a_partir_de = titulos.iter().map(|(i, _)| *i).min().unwrap_or(0);
    let indice_do_valor = (a_partir_de..linhas.len())
        .chain(0..a_partir_de)
        .find(|i| regex_do_valor(None, linhas[*i]).is_some());
    if let Some(i) = indice_do_valor {
        let anterior = i.checked_sub(1).map(|j| linhas[j]);
        let candidatos = [
            regex_do_valor(anterior, linhas[i]),
            regex_do_valor(None, linhas[i]),
        ];
        let mut erro = None;
        for rx in candidatos.into_iter().flatten() {
            match conferir(&rx, Formato::Centavos, &texto, momento, "valor_centavos") {
                Ok(e) => {
                    extrair.insert("valor_centavos".to_owned(), e);
                    erro = None;
                    break;
                }
                Err(m) => erro = Some(m),
            }
        }
        fora.extend(erro);
    }

    // A data (competência ou vencimento).
    if let Some((nome, formato)) = campo_de_data(pedido) {
        match regex_da_data_do_campo(&linhas, pedido, formato) {
            Some(rx) => match conferir(&rx, formato, &texto, momento, nome) {
                Ok(e) => {
                    extrair.insert(nome.to_owned(), e);
                }
                Err(m) => fora.push(m),
            },
            None => fora.push(format!("`{nome}`: não achei uma data no texto")),
        }
    }

    if todos.is_empty() {
        return None;
    }
    let nome = nome_do_pedido(pedido, titulos.first().map(|(_, t)| t.as_str()));
    let tipo = slug(&nome);
    let mut definicao = Definicao::simples(&tipo, "x");
    definicao.aplica_a = vec![item.tipo];
    definicao.detectar.todos = todos;
    definicao.obrigatorios = extrair
        .keys()
        .filter(|k| k.as_str() != "valor_centavos")
        .cloned()
        .collect();
    definicao.extrair = extrair;
    definicao.texto_de_ocr = de_ocr;
    definicao.validar().ok()?;
    Some(Esqueleto {
        regra: tipo,
        nome,
        definicao,
        fora,
    })
}

/// O esqueleto do item na tela, já pré-testado, como texto para o primeiro prompt (vai dentro de
/// um bloco de dados). `None` se não há esqueleto ou o catálogo não abre.
pub fn esqueleto_resumido(ctx: &Contexto, pedido: &str) -> Option<String> {
    let e = montar(&ctx.tela.item, pedido)?;
    let c = catalogo_do(ctx).ok()?;
    let existe = c.regras().ok()?.iter().any(|r| r.id == e.regra);
    let regra = if existe {
        format!("{}-novo", e.regra)
    } else {
        e.regra.clone()
    };
    let avulsa = VersaoDeRegra::avulsa(
        NovaVersao {
            regra: regra.clone(),
            nome: e.nome.clone(),
            definicao: e.definicao.clone(),
            script: None,
            autor: Autor::Ia,
            motivo: cortar(pedido.trim(), 200),
        },
        1,
        Estado::Rascunho,
    )
    .ok()?;
    let pre = pre_testar_versao(ctx, &c, &avulsa, pedido).ok()?;
    let mut linhas = vec![
        format!("regra: {regra}"),
        format!("nome: {}", e.nome),
        format!(
            "definicao: {}",
            serde_json::to_string(&e.definicao).unwrap_or_default()
        ),
    ];
    linhas.push(if pre.passou {
        let campos = pre
            .item
            .campos
            .iter()
            .map(|(n, v)| format!("{n} = {v}"))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "pré-teste do esqueleto: PASSOU. Casa neste item ({campos}); no cofre casam {} de {} parecidos.",
            pre.amostra.casam, pre.amostra.examinados
        )
    } else {
        let diag = if pre.item.diagnostico.is_empty() {
            pre.motivo.clone()
        } else {
            pre.item.diagnostico.join(" ")
        };
        format!("pré-teste do esqueleto: NÃO PASSOU. {}", cortar(&diag, 700))
    });
    if !e.fora.is_empty() {
        linhas.push(format!(
            "ficou de fora do esqueleto: {}",
            cortar(&e.fora.join("; "), 400)
        ));
    }
    Some(linhas.join("\n"))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contexto::entrada_de_documento;

    const PRINT: &str = include_str!("../tests/fixtures/print-prestacao-habitacao-ficticio.txt");
    const PEDIDO: &str = "Isto é um comprovante de prestação da habitação CAIXA. O que determina é o título (Prestação atual) e o número do contrato. A data de vencimento determina a competência. Cria o padrão.";

    fn item(texto: &str, momento: Option<&str>) -> Entrada {
        let mut i = entrada_de_documento("aa", Some("p.png"), Some("image/png"), Some(texto));
        i.momento = momento.map(str::to_owned);
        i
    }

    fn contem(e: &Esqueleto) -> Vec<String> {
        e.definicao
            .detectar
            .todos
            .iter()
            .filter_map(|c| match c {
                Condicao::Contem(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn o_fixture_vira_titulo_contrato_valor_e_competencia() {
        let e = montar(&item(PRINT, Some("2026-11-20")), PEDIDO).expect("esqueleto");
        assert_eq!(contem(&e), vec!["prestacao atual", "123456789012-3"]);
        let nomes: Vec<&String> = e.definicao.extrair.keys().collect();
        assert_eq!(nomes, ["competencia", "contrato", "valor_centavos"]);
        assert_eq!(e.definicao.obrigatorios, ["competencia", "contrato"]);
        assert_eq!(e.nome, "Comprovante de prestação da habitação CAIXA");
        assert_eq!(e.regra, "comprovante-de-prestacao-da-habitacao-caixa");
        assert!(e.definicao.texto_de_ocr, "print é imagem: texto de OCR");
        assert!(e.fora.is_empty(), "{:?}", e.fora);
    }

    #[test]
    fn sem_momento_a_competencia_fica_de_fora_com_o_motivo() {
        let e = montar(&item(PRINT, None), PEDIDO).expect("esqueleto");
        assert!(!e.definicao.extrair.contains_key("competencia"));
        assert!(e.definicao.extrair.contains_key("valor_centavos"));
        assert!(e.fora.iter().any(|f| f.contains("sem ano")), "{:?}", e.fora);
    }

    #[test]
    fn pedido_escrito_de_outro_jeito() {
        let p = "Crie um padrão para a prestação do financiamento da casa. Reconheça pelo \"Prestação atual\" e pelo número do contrato; o ciclo vem do vencimento.";
        let e = montar(&item(PRINT, Some("2025-11-05")), p).expect("esqueleto");
        assert_eq!(contem(&e), vec!["prestacao atual", "123456789012-3"]);
        assert!(e.definicao.extrair.contains_key("competencia"));
        assert!(e.definicao.extrair.contains_key("contrato"));
        assert_eq!(e.nome, "Prestação do financiamento da casa");
    }

    #[test]
    fn so_cria_sem_citar_nada_usa_a_primeira_linha_de_texto() {
        let e = montar(&item(PRINT, Some("2026-11-20")), "cria").expect("esqueleto");
        assert_eq!(contem(&e), vec!["prestacao atual"]);
        assert!(!e.definicao.extrair.contains_key("contrato"));
        assert!(!e.definicao.extrair.contains_key("competencia"));
        assert!(e.definicao.extrair.contains_key("valor_centavos"));
    }

    #[test]
    fn ocr_sujo_o_motor_conserta_os_numeros_e_o_valor() {
        let sujo = PRINT
            .replace("123456789012-3", "1234567890l2-3")
            .replace("R$ 1.234,56", "RS 1.234,5O");
        let e = montar(&item(&sujo, Some("2026-11-20")), PEDIDO).expect("esqueleto");
        assert!(
            e.definicao.extrair.contains_key("valor_centavos"),
            "{:?}",
            e.fora
        );
        assert_eq!(contem(&e)[1], "123456789012-3");
    }

    #[test]
    fn data_numerica_vira_competencia_pelo_mes_e_ano() {
        let texto =
            "Fatura Exemplo\nContrato 4455-7788\nVencimento: 15/11/2026\nValor total R$ 129,90\n";
        let e = montar(
            &item(texto, None),
            "O título determina o padrão; a data de vencimento é a competência",
        )
        .expect("esqueleto");
        let rx = &e.definicao.extrair["competencia"].regex;
        assert!(rx.contains(r"\d{2}/(\d{2}/\d{4})"), "{rx}");
        assert!(e.fora.is_empty(), "{:?}", e.fora);
    }

    #[test]
    fn sem_texto_ou_sem_ancora_nao_ha_esqueleto() {
        assert!(montar(&item("", None), PEDIDO).is_none());
        assert!(montar(&item("12345\n67890", None), "cria").is_none());
    }
}
