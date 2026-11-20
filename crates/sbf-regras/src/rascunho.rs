//! Rascunho de regra a partir de uma correção (D7), sem IA e determinístico.
//!
//! O usuário corrige um documento na caixa de entrada (tipo e campos). Daí sai uma versão
//! rascunho:
//!
//! - **detectar**: de 1 a 3 âncoras em `todos`, linhas curtas e distintivas do texto
//!   normalizado, de preferência do topo, que não contêm os valores corrigidos nem
//!   sequências de 4 ou mais dígitos (datas, contas, códigos mudam de documento para
//!   documento);
//! - **extrair**: para cada campo corrigido, acha o texto do valor no documento (formatos
//!   brasileiros de centavos, data e competência), pega o rótulo antes dele na mesma linha
//!   (ou a linha de cima, se o valor está sozinho) e gera
//!   `rótulo\s*[:\-]?\s*(padrão do formato)`. O resultado é conferido contra o próprio
//!   documento: o campo que não volta igual ao corrigido fica de fora e o `motivo` diz por
//!   quê;
//! - o [`Esperado`] devolvido (tipo e os campos que ficaram) vira o primeiro exemplo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalogo::{Esperado, NovaVersao};
use crate::definicao::{Condicao, Definicao, Extracao, FonteDaExtracao, Formato, normalizar};
use crate::{Autor, Entrada, ErroDasRegras, IdDaRegra, Valor};

/// O que o usuário corrigiu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correcao {
    /// A regra a que a correção pertence; sem ela, o id sai do `nome`.
    pub regra: Option<IdDaRegra>,
    pub nome: String,
    pub tipo_documental: String,
    pub campos: BTreeMap<String, Valor>,
}

/// Como um valor pode aparecer no texto e o padrão que o captura de volta.
struct Forma {
    texto: String,
    padrao: &'static str,
    formato: Formato,
    numerica: bool,
}

fn milhar(reais: u64) -> String {
    let digitos = reais.to_string();
    let mut saida = String::new();
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i).is_multiple_of(3) {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

fn formas(valor: &Valor) -> Vec<Forma> {
    match valor {
        Valor::Centavos(c) => {
            let abs = c.unsigned_abs();
            let (reais, cent) = (abs / 100, abs % 100);
            vec![
                Forma {
                    texto: format!("{},{cent:02}", milhar(reais)),
                    padrao: r"-?\d[\d.]*,\d{2}",
                    formato: Formato::Centavos,
                    numerica: true,
                },
                Forma {
                    texto: format!("{reais},{cent:02}"),
                    padrao: r"-?\d[\d.]*,\d{2}",
                    formato: Formato::Centavos,
                    numerica: true,
                },
                Forma {
                    texto: format!("{reais}.{cent:02}"),
                    padrao: r"-?\d+\.\d{2}",
                    formato: Formato::Centavos,
                    numerica: true,
                },
            ]
        }
        Valor::Data(d) => {
            let p: Vec<&str> = d.split('-').collect();
            let mut v = Vec::new();
            if let [a, m, dia] = p.as_slice() {
                v.push(Forma {
                    texto: format!("{dia}/{m}/{a}"),
                    padrao: r"\d{2}/\d{2}/\d{4}",
                    formato: Formato::Data,
                    numerica: true,
                });
            }
            v.push(Forma {
                texto: d.clone(),
                padrao: r"\d{4}-\d{2}-\d{2}",
                formato: Formato::Data,
                numerica: true,
            });
            v
        }
        Valor::Competencia(c) => {
            let mut v = Vec::new();
            if let Some((a, m)) = c.split_once('-') {
                v.push(Forma {
                    texto: format!("{m}/{a}"),
                    padrao: r"\d{2}/\d{4}",
                    formato: Formato::Competencia,
                    numerica: true,
                });
            }
            v.push(Forma {
                texto: c.clone(),
                padrao: r"\d{4}-\d{2}",
                formato: Formato::Competencia,
                numerica: true,
            });
            v
        }
        Valor::Inteiro(n) => vec![Forma {
            texto: n.to_string(),
            padrao: r"-?\d+",
            formato: Formato::Inteiro,
            numerica: true,
        }],
        Valor::Texto(t) if !t.trim().is_empty() => vec![Forma {
            texto: t.trim().to_owned(),
            padrao: r"[^\r\n]+",
            formato: Formato::Texto,
            numerica: false,
        }],
        _ => Vec::new(),
    }
}

/// A primeira posição de `forma` no texto, com fronteira de número quando numérica.
fn achar(texto: &str, forma: &Forma) -> Option<usize> {
    texto
        .match_indices(&forma.texto)
        .map(|(i, _)| i)
        .find(|&i| {
            if !forma.numerica {
                return true;
            }
            let antes = texto[..i].chars().next_back();
            let depois_ini = i + forma.texto.len();
            let mut depois = texto[depois_ini..].chars();
            let d1 = depois.next();
            let d2 = depois.next();
            let antes_ok = antes.is_none_or(|c| !c.is_ascii_digit() && c != ',' && c != '.');
            let depois_ok = d1.is_none_or(|c| {
                !c.is_ascii_digit()
                    && !((c == ',' || c == '.') && d2.is_some_and(|x| x.is_ascii_digit()))
            });
            antes_ok && depois_ok
        })
}

/// O rótulo que antecede a posição: o trecho da mesma linha, ou a linha de cima se o valor
/// está sozinho. No máximo 40 caracteres, começando numa palavra inteira.
pub fn rotulo_antes(texto: &str, pos: usize) -> Option<String> {
    let inicio_da_linha = texto[..pos].rfind('\n').map_or(0, |i| i + 1);
    let mut cru = texto[inicio_da_linha..pos].trim_end().to_owned();
    if let Some(r) = cru.strip_suffix("R$") {
        cru = r.trim_end().to_owned();
    }
    let mut rotulo = cru
        .trim_end_matches([':', '-', ' ', '\t'])
        .trim()
        .to_owned();
    if rotulo.is_empty() && inicio_da_linha > 0 {
        let acima = &texto[..inicio_da_linha - 1];
        let linha = acima.rsplit('\n').next().unwrap_or("");
        rotulo = linha
            .trim()
            .trim_end_matches([':', '-', ' ', '\t'])
            .trim()
            .to_owned();
    }
    if rotulo.chars().count() > 40 {
        let corte = rotulo.char_indices().rev().nth(39).map_or(0, |(i, _)| i);
        let cauda = &rotulo[corte..];
        rotulo = cauda
            .split_once(' ')
            .map_or(cauda, |(_, resto)| resto)
            .trim()
            .to_owned();
    }
    rotulo.chars().any(char::is_alphabetic).then_some(rotulo)
}

pub fn rotulo_para_regex(rotulo: &str) -> String {
    rotulo
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+")
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

fn ancoras(texto: &str, valores_achados: &[String]) -> Vec<String> {
    let candidatas: Vec<String> = texto
        .lines()
        .filter(|linha| !valores_achados.iter().any(|v| linha.contains(v.as_str())))
        .map(normalizar)
        .filter(|n| {
            (6..=60).contains(&n.chars().count())
                && n.chars().any(char::is_alphabetic)
                && !tem_digitos_longos(n)
        })
        .collect();
    // Linhas sem nenhum dígito são as mais estáveis entre documentos do mesmo tipo; as
    // com dígitos curtos só entram se não sobrar nada melhor.
    let mut escolhidas: Vec<String> = Vec::new();
    for exigir_sem_digito in [true, false] {
        for n in &candidatas {
            if escolhidas.len() == 3 {
                break;
            }
            if (exigir_sem_digito && n.chars().any(|c| c.is_ascii_digit()))
                || escolhidas.contains(n)
            {
                continue;
            }
            escolhidas.push(n.clone());
        }
        if !escolhidas.is_empty() {
            break;
        }
    }
    escolhidas
}

fn slug(nome: &str) -> String {
    let s = normalizar(nome).replace(' ', "-");
    if s.is_empty() {
        "regra-corrigida".to_owned()
    } else {
        s
    }
}

/// Monta o rascunho e o primeiro exemplo.
///
/// # Errors
/// [`ErroDasRegras::Definicao`] se a entrada não tem texto, não há âncora para detectar ou
/// a definição sai inválida.
pub fn rascunho_da_correcao(
    entrada: &Entrada,
    correcao: &Correcao,
) -> Result<(NovaVersao, Esperado), ErroDasRegras> {
    let erro = |m: &str| ErroDasRegras::Definicao(m.to_owned());
    let texto = entrada
        .texto
        .as_deref()
        .ok_or_else(|| erro("a entrada não tem texto para aprender"))?;

    let mut candidatas: BTreeMap<String, Extracao> = BTreeMap::new();
    let mut achados: Vec<String> = Vec::new();
    let mut avisos: Vec<String> = Vec::new();
    for (nome, valor) in &correcao.campos {
        let pos = formas(valor)
            .into_iter()
            .find_map(|f| achar(texto, &f).map(|p| (f, p)));
        let Some((forma, pos)) = pos else {
            avisos.push(format!("`{nome}`: não achei o valor no documento"));
            continue;
        };
        achados.push(forma.texto.clone());
        let Some(rotulo) = rotulo_antes(texto, pos) else {
            avisos.push(format!("`{nome}`: o valor não tem rótulo antes dele"));
            continue;
        };
        let prefixo = if forma.formato == Formato::Centavos {
            r"(?:R\$\s*)?"
        } else {
            ""
        };
        candidatas.insert(
            nome.clone(),
            Extracao {
                regex: format!(
                    r"(?i){}\s*[:\-]?\s*{prefixo}({})",
                    rotulo_para_regex(&rotulo),
                    forma.padrao
                ),
                grupo: 1,
                formato: forma.formato,
                fonte: FonteDaExtracao::Texto,
            },
        );
    }

    let ancoras = ancoras(texto, &achados);
    if ancoras.is_empty() {
        return Err(erro(
            "não achei linhas distintivas para detectar este documento",
        ));
    }
    let mut definicao = Definicao::simples(&correcao.tipo_documental, "x");
    definicao.aplica_a = vec![entrada.tipo];
    definicao.detectar.todos = ancoras.into_iter().map(Condicao::Contem).collect();
    definicao.extrair = candidatas;
    definicao.validar()?;

    // Confere contra o próprio documento: só fica o campo que volta igual ao corrigido.
    let compilada = definicao.compilar()?;
    let (extraidos, _) = compilada.extrair(entrada);
    let mut mantidos = BTreeMap::new();
    for nome in definicao.extrair.keys().cloned().collect::<Vec<_>>() {
        match (extraidos.get(&nome), correcao.campos.get(&nome)) {
            (Some(c), Some(esperado)) if c.valor == *esperado => {
                mantidos.insert(nome, esperado.clone());
            }
            _ => {
                definicao.extrair.remove(&nome);
                avisos.push(format!(
                    "`{nome}`: a expressão gerada não devolveu o valor corrigido"
                ));
            }
        }
    }
    definicao.obrigatorios = mantidos.keys().cloned().collect();
    definicao.validar()?;

    let mut motivo = format!(
        "montada a partir de uma correção do documento {}; {} campo(s) com expressão",
        entrada.id,
        mantidos.len()
    );
    if !avisos.is_empty() {
        motivo.push_str("; fora: ");
        motivo.push_str(&avisos.join("; "));
    }
    let nova = NovaVersao {
        regra: correcao
            .regra
            .clone()
            .unwrap_or_else(|| slug(&correcao.nome)),
        nome: correcao.nome.clone(),
        definicao,
        script: None,
        autor: Autor::Correcao,
        motivo,
    };
    let esperado = Esperado::Reconhece {
        tipo_documental: correcao.tipo_documental.clone(),
        campos: mantidos,
    };
    Ok((nova, esperado))
}
