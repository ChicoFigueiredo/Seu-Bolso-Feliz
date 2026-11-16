//! Leitor do texto de um "DEMONSTRATIVO DE PAGAMENTO" (o que `pdftotext -layout` gera).
//!
//! Só texto: nada aqui abre arquivo. Nunca loga nome, matrícula, conta nem valores; os
//! erros não carregam o conteúdo do documento.

use sbf_planejamento::{Centavos, Competencia, Composicao, Rubrica, TipoDeRubrica};

use crate::texto::{centavos_de_texto, normalizar};
use crate::{ErroPlanilha, Resultado};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contracheque {
    /// O mês/ano de pagamento.
    pub pagamento: Competencia,
    pub rubricas: Vec<Rubrica>,
    /// Rubricas cujo código não é de provento nem de desconto (ficam fora de `rubricas`).
    pub nao_classificadas: u32,
    pub bruto: Option<Centavos>,
    pub descontos: Option<Centavos>,
    pub liquido: Option<Centavos>,
    /// Proventos menos descontos das rubricas batem com o líquido declarado.
    pub conferido: bool,
}

/// Uma consignação com prazo: o contrato começou em `inicio` e dura `prazo` parcelas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsignacaoComPrazo {
    pub nome: String,
    pub inicio: Competencia,
    pub prazo: u32,
    /// A competência da última parcela (`inicio` + `prazo` − 1 meses).
    pub termina: Competencia,
}

fn erro(o_que: &str) -> ErroPlanilha {
    ErroPlanilha::Contracheque(o_que.to_owned())
}

fn mes_do_nome(nome: &str) -> Option<u8> {
    const MESES: [&str; 12] = [
        "janeiro",
        "fevereiro",
        "marco",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ];
    let n = normalizar(nome);
    MESES.iter().position(|m| *m == n).map(|i| i as u8 + 1)
}

/// `MM/AAAA`.
fn competencia_de(texto: &str) -> Option<Competencia> {
    let (m, a) = texto.split_once('/')?;
    if m.len() != 2 || a.len() != 4 {
        return None;
    }
    Competencia::nova(a.parse().ok()?, m.parse().ok()?)
}

fn tipo_do_codigo(codigo: &str) -> Option<TipoDeRubrica> {
    match codigo.chars().next()? {
        '2' => Some(TipoDeRubrica::Provento),
        '3' | '4' => Some(TipoDeRubrica::Desconto),
        _ => None,
    }
}

/// Todos os `R$ valor` de uma linha, na ordem.
fn valores_da_linha(linha: &str) -> Vec<Centavos> {
    let palavras: Vec<&str> = linha.split_whitespace().collect();
    palavras
        .windows(2)
        .filter(|w| w[0] == "R$")
        .filter_map(|w| centavos_de_texto(w[1]))
        .collect()
}

/// Código, nome, competência, prazo e valor.
type LinhaDeRubrica = (String, String, Option<Competencia>, Option<u32>, Centavos);

/// `CÓDIGO NOME... [MM/AAAA] [PRAZO] R$ VALOR`; `None` quando a linha não é uma rubrica.
fn rubrica_da_linha(linha: &str) -> Option<LinhaDeRubrica> {
    let palavras: Vec<&str> = linha.split_whitespace().collect();
    let codigo = *palavras.first()?;
    if !(4..=5).contains(&codigo.len()) || !codigo.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n = palavras.len();
    if n < 4 || palavras[n - 2] != "R$" {
        return None;
    }
    let valor = centavos_de_texto(palavras[n - 1])?;
    let meio = &palavras[1..n - 2];
    let pos = meio.iter().position(|p| competencia_de(p).is_some());
    let (nome, competencia, prazo) = match pos {
        Some(i) => {
            let prazo = meio.get(i + 1).and_then(|p| p.parse::<u32>().ok());
            (meio[..i].join(" "), competencia_de(meio[i]), prazo)
        }
        None => (meio.join(" "), None, None),
    };
    Some((codigo.to_owned(), nome, competencia, prazo, valor))
}

/// Lê o texto do demonstrativo.
pub fn ler(texto: &str) -> Resultado<Contracheque> {
    let linhas: Vec<&str> = texto.lines().collect();
    if !normalizar(texto).contains("demonstrativo de pagamento") {
        return Err(erro("não é um demonstrativo de pagamento"));
    }

    // Mês / ano de pagamento: a 1ª linha com conteúdo depois do cabeçalho.
    let cab = linhas
        .iter()
        .position(|l| normalizar(l).contains("mes ano de pagamento"))
        .ok_or_else(|| erro("falta o mês e o ano de pagamento"))?;
    let pagamento = linhas[cab + 1..]
        .iter()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            let i = p.iter().position(|x| *x == "/")?;
            let mes = mes_do_nome(p.get(i.checked_sub(1)?)?)?;
            Competencia::nova(p.get(i + 1)?.parse().ok()?, mes)
        })
        .ok_or_else(|| erro("o mês e o ano de pagamento não leem"))?;

    let mut rubricas = Vec::new();
    let mut nao_classificadas = 0;
    let inicio = linhas
        .iter()
        .position(|l| normalizar(l).contains("discriminacao da rubrica"))
        .ok_or_else(|| erro("falta a tabela de rubricas"))?;
    let fim = linhas
        .iter()
        .position(|l| normalizar(l).contains("salario bruto"))
        .ok_or_else(|| erro("faltam os totais"))?;
    if fim <= inicio {
        return Err(erro("a tabela de rubricas vem depois dos totais"));
    }
    for l in &linhas[inicio + 1..fim] {
        let Some((codigo, nome, competencia, prazo, valor)) = rubrica_da_linha(l) else {
            continue;
        };
        match tipo_do_codigo(&codigo) {
            Some(tipo) => rubricas.push(Rubrica {
                codigo: Some(codigo),
                nome,
                tipo,
                valor,
                competencia,
                prazo,
                recorrente: None,
            }),
            None => nao_classificadas += 1,
        }
    }

    // Totais: a linha com os valores logo depois do cabeçalho "Salário Bruto".
    let totais = linhas[fim + 1..]
        .iter()
        .map(|l| valores_da_linha(l))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    let (bruto, descontos, liquido) = (
        totais.first().copied(),
        totais.get(1).copied(),
        totais.get(2).copied(),
    );

    let soma = |t: TipoDeRubrica| -> i64 {
        rubricas
            .iter()
            .filter(|r| r.tipo == t)
            .map(|r| r.valor.0)
            .sum()
    };
    let conferido = nao_classificadas == 0
        && !rubricas.is_empty()
        && liquido
            .is_some_and(|l| soma(TipoDeRubrica::Provento) - soma(TipoDeRubrica::Desconto) == l.0);
    Ok(Contracheque {
        pagamento,
        rubricas,
        nao_classificadas,
        bruto,
        descontos,
        liquido,
        conferido,
    })
}

impl Contracheque {
    /// As rubricas como composição do recorrente do salário, valendo desde o pagamento.
    pub fn composicao(&self, recorrente: &str, sha256: &str) -> Composicao {
        Composicao {
            recorrente: recorrente.to_owned(),
            desde: self.pagamento,
            rubricas: self.rubricas.clone(),
            documento: Some(sha256.to_owned()),
            origem: format!(
                "contracheque {:02}/{:04}",
                self.pagamento.mes, self.pagamento.ano
            ),
        }
    }

    /// As consignações com prazo: rubricas de competência anterior ao pagamento (o início
    /// do contrato) e com prazo. Só informação; nada é aplicado.
    pub fn consignacoes_com_prazo(&self) -> Vec<ConsignacaoComPrazo> {
        self.rubricas
            .iter()
            .filter(|r| r.tipo == TipoDeRubrica::Desconto)
            .filter_map(|r| {
                let (inicio, prazo) = (r.competencia?, r.prazo?);
                (inicio < self.pagamento && prazo > 0).then(|| ConsignacaoComPrazo {
                    nome: r.nome.clone(),
                    inicio,
                    prazo,
                    termina: inicio.somar_meses(prazo as i32 - 1),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const TEXTO: &str = include_str!("../tests/contracheque-ficticio.txt");

    #[test]
    fn le_o_demonstrativo_ficticio() {
        let c = ler(TEXTO).expect("ler");
        assert_eq!(c.pagamento, Competencia { ano: 2026, mes: 10 });
        assert_eq!(c.rubricas.len(), 9);
        assert_eq!(c.bruto, Some(Centavos(1_000_000)));
        assert_eq!(c.descontos, Some(Centavos(430_050)));
        assert_eq!(c.liquido, Some(Centavos(569_950)));
        assert!(c.conferido);
        let sal = &c.rubricas[0];
        assert_eq!(sal.codigo.as_deref(), Some("2002"));
        assert_eq!(sal.nome, "SALARIO PADRAO");
        assert_eq!(sal.tipo, TipoDeRubrica::Provento);
        assert_eq!(sal.valor, Centavos(500_000));
        assert_eq!(sal.prazo, None);
        let inss = &c.rubricas[3];
        assert_eq!(inss.tipo, TipoDeRubrica::Desconto);
        assert_eq!(inss.prazo, Some(1));
        assert_eq!(inss.valor, Centavos(90_000));
        let consig = &c.rubricas[5];
        assert_eq!(consig.codigo.as_deref(), Some("43345"));
        assert_eq!(consig.tipo, TipoDeRubrica::Desconto);
        assert_eq!(consig.competencia, Some(Competencia { ano: 2024, mes: 10 }));
        assert_eq!(consig.prazo, Some(96));
        assert_eq!(c.rubricas[6].valor, Centavos(45_050));
    }

    #[test]
    fn consignacoes_com_prazo_mostram_inicio_e_termino() {
        let c = ler(TEXTO).expect("ler");
        let l = c.consignacoes_com_prazo();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].inicio, Competencia { ano: 2024, mes: 10 });
        assert_eq!(l[0].prazo, 96);
        assert_eq!(l[0].termina, Competencia { ano: 2032, mes: 9 });
        assert_eq!(l[1].termina, Competencia { ano: 2035, mes: 4 });
    }

    #[test]
    fn soma_que_nao_bate_marca_nao_conferido() {
        let t = TEXTO.replace("R$ 5.699,50", "R$ 5.600,00");
        assert!(!ler(&t).expect("ler").conferido);
        let t = TEXTO.replace("2005        COMPL", "7005        COMPL");
        let c = ler(&t).expect("ler");
        assert_eq!(c.nao_classificadas, 1);
        assert!(!c.conferido);
    }

    #[test]
    fn texto_que_nao_e_contracheque_e_erro_sem_conteudo() {
        let e = ler("qualquer coisa com R$ 1,00")
            .expect_err("erro")
            .to_string();
        assert!(!e.contains("1,00"));
        assert!(ler("DEMONSTRATIVO DE PAGAMENTO\nsem mais nada").is_err());
    }
}
