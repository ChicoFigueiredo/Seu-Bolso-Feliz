//! Leitor do texto de um "DEMONSTRATIVO DE PAGAMENTO" (o que `pdftotext -layout` gera).
//!
//! Só texto: nada aqui abre arquivo. Nunca loga nome, matrícula, conta nem valores; os
//! erros não carregam o conteúdo do documento.

use sbf_planejamento::{Centavos, Competencia, Composicao, Data, Rubrica, TipoDeRubrica};

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
    /// Proventos menos descontos das rubricas (sem as informativas) batem com o líquido
    /// declarado.
    pub conferido: bool,
    /// "Líquido creditado em DD/MM/AAAA" (o layout de 2026 traz; o antigo não).
    pub credito: Option<Data>,
    /// Alinhado a `rubricas`: a rubrica é eventual (retroativo, PLR, prêmio...), não fixa.
    /// Veja [`e_eventual`]. É só a marcação inicial; quem confirma pode trocá-la.
    pub eventuais: Vec<bool>,
    /// Alinhado a `rubricas`: a rubrica aparece entre os descontos mas não entra em "Valor
    /// Descontos" (o adiantamento de salário). Fica fora da composição e do líquido.
    pub informativas: Vec<bool>,
    /// Alinhado a `rubricas`: no layout de 2026, a coluna Prazo das consignações é o número
    /// da parcela atual (o fim do contrato é desconhecido). `None` nas demais rubricas e em
    /// todo o layout antigo.
    pub parcelas_atuais: Vec<Option<u32>>,
    /// `true` no layout de 2026 (mês/ano como `MM/AAAA`, coluna Prazo = parcela atual).
    pub prazo_e_parcela_atual: bool,
    /// Observações da leitura, em português e sem valores ("X não entra no total de
    /// descontos do contracheque").
    pub avisos: Vec<String>,
    /// "Base Cálculo Imposto Renda" do rodapé, quando o layout a traz (o de 2026 traz).
    pub base_irrf: Option<Centavos>,
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
        '1' | '2' => Some(TipoDeRubrica::Provento),
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

/// Os totais de um bloco "Salário Bruto / Valor Descontos / Salário Liquido": a linha de
/// valores logo depois do cabeçalho (bruto, descontos, líquido).
fn totais_do_bloco(linhas: &[&str], cabecalho: usize) -> Option<[Option<Centavos>; 3]> {
    let v = linhas[cabecalho + 1..]
        .iter()
        .take(3)
        .map(|l| valores_da_linha(l))
        .find(|v| !v.is_empty())?;
    Some([v.first().copied(), v.get(1).copied(), v.get(2).copied()])
}

/// `Liquido creditado em DD/MM/AAAA`.
fn credito_da_linha(linha: &str) -> Option<Data> {
    let p: Vec<&str> = linha.split_whitespace().collect();
    let i = p
        .windows(2)
        .position(|w| normalizar(w[0]) == "creditado" && normalizar(w[1]) == "em")?;
    let mut partes = p.get(i + 2)?.split('/');
    let (d, m, a) = (partes.next()?, partes.next()?, partes.next()?);
    if partes.next().is_some() || d.len() != 2 || m.len() != 2 || a.len() != 4 {
        return None;
    }
    Data::de_iso(&format!("{a}-{m}-{d}"))
}

/// O valor de um rótulo do rodapé: os rótulos ficam numa linha (separados por 2 espaços ou
/// mais) e os `R$ valor`, na primeira linha com valores logo abaixo, na mesma ordem. `None`
/// quando o rótulo não existe, a contagem não bate ou o valor não lê.
fn valor_do_rodape(linhas: &[&str], rotulo: &str) -> Option<Centavos> {
    for (i, l) in linhas.iter().enumerate() {
        let rotulos: Vec<&str> = l
            .split("  ")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let Some(pos) = rotulos.iter().position(|r| normalizar(r) == rotulo) else {
            continue;
        };
        let valores = linhas[i + 1..]
            .iter()
            .take(2)
            .map(|x| valores_da_linha(x))
            .find(|v| !v.is_empty())?;
        return (valores.len() == rotulos.len())
            .then(|| valores.get(pos).copied())
            .flatten();
    }
    None
}

/// Rubrica eventual: não se repete todo mês. Duas regras, só estas:
///
/// 1. retroativo: a competência da rubrica não é o mês do pagamento (só no layout de 2026;
///    no antigo a competência de uma consignação é o início do contrato);
/// 2. o nome, sem acento, traz uma destas palavras ou expressões: PLR, PREMIO, ADIANTAMENTO,
///    ABONO, FERIAS, CONTRIBUICAO NEGOCIAL, 13 SALARIO, GRATIFICACAO NATALINA.
///
/// É uma marcação inicial: a pessoa troca rubrica por rubrica antes de confirmar.
#[must_use]
pub fn e_eventual(rubrica: &Rubrica, pagamento: Competencia, layout_de_2026: bool) -> bool {
    const PALAVRAS: [&str; 5] = ["plr", "premio", "adiantamento", "abono", "ferias"];
    const EXPRESSOES: [&str; 3] = [
        "contribuicao negocial",
        "13 salario",
        "gratificacao natalina",
    ];
    if layout_de_2026 && rubrica.competencia.is_some_and(|c| c != pagamento) {
        return true;
    }
    let nome = format!(" {} ", normalizar(&rubrica.nome));
    PALAVRAS
        .iter()
        .chain(EXPRESSOES.iter())
        .any(|p| nome.contains(&format!(" {p} ")))
}

/// Consignação ou empréstimo em folha (a coluna Prazo é a parcela atual no layout de 2026).
fn e_consignacao(nome: &str) -> bool {
    let n = normalizar(nome);
    n.contains("consign") || n.contains("emprestimo")
}

/// Lê o texto do demonstrativo. Aceita o layout antigo (mês por extenso, uma página) e o de
/// 2026 (`MM/AAAA`, várias páginas que repetem cabeçalho e rodapé).
pub fn ler(texto: &str) -> Resultado<Contracheque> {
    let linhas: Vec<&str> = texto.lines().collect();
    if !normalizar(texto).contains("demonstrativo de pagamento") {
        return Err(erro("não é um demonstrativo de pagamento"));
    }

    // Mês / ano de pagamento: o valor está na 1ª linha com conteúdo depois do rótulo.
    let cab = linhas
        .iter()
        .position(|l| normalizar(l).contains("mes ano de pagamento"))
        .ok_or_else(|| erro("falta o mês e o ano de pagamento"))?;
    let valor = linhas[cab + 1..]
        .iter()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| erro("o mês e o ano de pagamento não leem"))?;
    let palavras: Vec<&str> = valor.split_whitespace().collect();
    let numerico = palavras.first().and_then(|p| competencia_de(p));
    let prazo_e_parcela_atual = numerico.is_some();
    let pagamento = numerico
        .or_else(|| {
            // Layout antigo: `OUTUBRO / 2026`.
            let i = palavras.iter().position(|x| *x == "/")?;
            let mes = mes_do_nome(palavras.get(i.checked_sub(1)?)?)?;
            Competencia::nova(palavras.get(i + 1)?.parse().ok()?, mes)
        })
        .ok_or_else(|| erro("o mês e o ano de pagamento não leem"))?;

    // As rubricas de todas as páginas; os totais de cada bloco.
    let mut rubricas = Vec::new();
    let mut nao_classificadas = 0;
    let mut viu_tabela = false;
    let mut na_tabela = false;
    let mut blocos: Vec<[Option<Centavos>; 3]> = Vec::new();
    let mut credito = None;
    for (i, l) in linhas.iter().enumerate() {
        let n = normalizar(l);
        if n.contains("discriminacao da rubrica") {
            na_tabela = true;
            viu_tabela = true;
        } else if n.contains("salario bruto") {
            na_tabela = false;
            if let Some(b) = totais_do_bloco(&linhas, i) {
                blocos.push(b);
            }
        } else if n.contains("liquido creditado em") {
            credito = credito.or_else(|| credito_da_linha(l));
        } else if na_tabela
            && let Some((codigo, nome, competencia, prazo, valor)) = rubrica_da_linha(l)
        {
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
    }
    if !viu_tabela {
        return Err(erro("falta a tabela de rubricas"));
    }
    let Some(totais) = blocos.first().copied() else {
        return Err(erro("faltam os totais"));
    };
    let mut avisos = Vec::new();
    if blocos.iter().any(|b| *b != totais) {
        avisos.push("as páginas do contracheque trazem totais diferentes".to_owned());
    }
    let [bruto, descontos, liquido] = totais;

    // No layout de 2026 a coluna Prazo das consignações é a parcela atual; nas demais
    // rubricas ela não diz nada que sirva.
    let mut parcelas_atuais = vec![None; rubricas.len()];
    if prazo_e_parcela_atual {
        for (r, p) in rubricas.iter_mut().zip(parcelas_atuais.iter_mut()) {
            if e_consignacao(&r.nome) {
                *p = r.prazo;
            }
            r.prazo = None;
        }
    }

    let soma = |t: TipoDeRubrica, informativas: &[bool]| -> i64 {
        rubricas
            .iter()
            .zip(informativas)
            .filter(|(r, fora)| r.tipo == t && !**fora)
            .map(|(r, _)| r.valor.0)
            .sum()
    };
    let mut informativas = vec![false; rubricas.len()];
    // Uma rubrica de desconto que não entra em "Valor Descontos": a diferença da soma é
    // exatamente o valor de uma só. Se houver mais de uma com esse valor, vale a que se
    // chama adiantamento; senão não adivinha.
    if let Some(declarado) = descontos {
        let diferenca = soma(TipoDeRubrica::Desconto, &informativas) - declarado.0;
        if diferenca > 0 {
            let candidatas: Vec<usize> = rubricas
                .iter()
                .enumerate()
                .filter(|(_, r)| r.tipo == TipoDeRubrica::Desconto && r.valor.0 == diferenca)
                .map(|(i, _)| i)
                .collect();
            let escolhida = match candidatas.as_slice() {
                [unica] => Some(*unica),
                varias => {
                    let adiantamentos: Vec<usize> = varias
                        .iter()
                        .copied()
                        .filter(|i| normalizar(&rubricas[*i].nome).contains("adiantamento"))
                        .collect();
                    match adiantamentos.as_slice() {
                        [unica] => Some(*unica),
                        _ => None,
                    }
                }
            };
            if let Some(i) = escolhida {
                informativas[i] = true;
                avisos.push(format!(
                    "{} não entra no total de descontos do contracheque",
                    rubricas[i].nome
                ));
            }
        }
        if soma(TipoDeRubrica::Desconto, &informativas) != declarado.0 {
            avisos.push("a soma dos descontos não bate com o total do contracheque".to_owned());
        }
    }

    let conferido = nao_classificadas == 0
        && !rubricas.is_empty()
        && liquido.is_some_and(|l| {
            soma(TipoDeRubrica::Provento, &informativas)
                - soma(TipoDeRubrica::Desconto, &informativas)
                == l.0
        });
    let eventuais = rubricas
        .iter()
        .map(|r| e_eventual(r, pagamento, prazo_e_parcela_atual))
        .collect();
    Ok(Contracheque {
        pagamento,
        rubricas,
        nao_classificadas,
        bruto,
        descontos,
        liquido,
        conferido,
        credito,
        eventuais,
        informativas,
        parcelas_atuais,
        prazo_e_parcela_atual,
        avisos,
        base_irrf: valor_do_rodape(&linhas, "base calculo imposto renda"),
    })
}

impl Contracheque {
    /// Quantas rubricas entram na composição (todas menos as informativas).
    #[must_use]
    pub fn rubricas_da_composicao(&self) -> usize {
        self.informativas.iter().filter(|i| !**i).count()
    }

    /// A marcação de eventuais completa: `eventuais` do chamador, ou a inicial se faltar.
    fn eventual(&self, marcacao: &[bool], i: usize) -> bool {
        marcacao
            .get(i)
            .copied()
            .unwrap_or_else(|| self.eventuais.get(i).copied().unwrap_or(false))
    }

    fn informativa(&self, i: usize) -> bool {
        self.informativas.get(i).copied().unwrap_or(false)
    }

    /// Há alguma rubrica eventual (fora as informativas) na marcação?
    #[must_use]
    pub fn tem_eventuais(&self, marcacao: &[bool]) -> bool {
        (0..self.rubricas.len()).any(|i| !self.informativa(i) && self.eventual(marcacao, i))
    }

    fn liquido_onde(&self, entra: impl Fn(usize) -> bool) -> Centavos {
        Centavos(
            self.rubricas
                .iter()
                .enumerate()
                .filter(|(i, _)| !self.informativa(*i) && entra(*i))
                .map(|(_, r)| match r.tipo {
                    TipoDeRubrica::Provento => r.valor.0,
                    TipoDeRubrica::Desconto => -r.valor.0,
                })
                .sum(),
        )
    }

    /// O líquido do mês: todas as rubricas, menos as informativas.
    #[must_use]
    pub fn liquido_do_mes(&self) -> Centavos {
        self.liquido_onde(|_| true)
    }

    /// O líquido das fixas: as rubricas que não são eventuais na marcação dada.
    #[must_use]
    pub fn liquido_das_fixas(&self, marcacao: &[bool]) -> Centavos {
        self.liquido_onde(|i| !self.eventual(marcacao, i))
    }

    /// A competência em que as fixas passam a valer: o mês seguinte quando há eventuais
    /// (o mês do pagamento tem a composição própria), senão o do pagamento.
    #[must_use]
    pub fn fixas_desde(&self, marcacao: &[bool]) -> Competencia {
        if self.tem_eventuais(marcacao) {
            self.pagamento.somar_meses(1)
        } else {
            self.pagamento
        }
    }

    /// As composições do recorrente do salário para a marcação de eventuais dada
    /// (`marcacao[i]` = a rubrica `i` é eventual; o que faltar vale a marcação inicial).
    ///
    /// - sem eventuais: uma só, com todas as rubricas, valendo desde o pagamento;
    /// - com eventuais: a do mês (todas, desde o pagamento) e a das fixas (desde o mês
    ///   seguinte), nesta ordem. Sem nenhuma fixa, só a do mês.
    ///
    /// As rubricas informativas não entram em nenhuma.
    #[must_use]
    pub fn composicoes(
        &self,
        recorrente: &str,
        sha256: &str,
        marcacao: &[bool],
    ) -> Vec<Composicao> {
        let origem = format!(
            "contracheque {:02}/{:04}",
            self.pagamento.mes, self.pagamento.ano
        );
        let monta =
            |desde: Competencia, origem: String, entra: &dyn Fn(usize) -> bool| Composicao {
                recorrente: recorrente.to_owned(),
                desde,
                rubricas: self
                    .rubricas
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !self.informativa(*i) && entra(*i))
                    .map(|(_, r)| r.clone())
                    .collect(),
                documento: Some(sha256.to_owned()),
                origem,
            };
        let do_mes = monta(self.pagamento, origem.clone(), &|_| true);
        if !self.tem_eventuais(marcacao) {
            return vec![do_mes];
        }
        let fixas = monta(
            self.pagamento.somar_meses(1),
            format!("{origem} (fixas)"),
            &|i| !self.eventual(marcacao, i),
        );
        if fixas.rubricas.is_empty() {
            vec![do_mes]
        } else {
            vec![do_mes, fixas]
        }
    }

    /// As consignações com prazo do layout antigo: rubricas de competência anterior ao
    /// pagamento (o início do contrato) e com prazo. Só informação; nada é aplicado. No
    /// layout de 2026 o fim é desconhecido (a coluna Prazo é a parcela atual): vazio; veja
    /// [`Contracheque::parcelas_atuais`].
    pub fn consignacoes_com_prazo(&self) -> Vec<ConsignacaoComPrazo> {
        if self.prazo_e_parcela_atual {
            return Vec::new();
        }
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

    const TEXTO_2026: &str = include_str!("../tests/contracheque-2026-ficticio.txt");

    fn indice(c: &Contracheque, codigo: &str, valor: i64) -> usize {
        c.rubricas
            .iter()
            .position(|r| r.codigo.as_deref() == Some(codigo) && r.valor.0 == valor)
            .expect("rubrica")
    }

    #[test]
    fn le_o_layout_de_2026_com_mes_credito_e_as_duas_paginas() {
        let c = ler(TEXTO_2026).expect("ler");
        assert_eq!(c.pagamento, Competencia { ano: 2026, mes: 10 });
        assert_eq!(c.credito, Data::de_iso("2026-10-13"));
        assert_eq!(c.rubricas.len(), 33);
        assert_eq!(c.nao_classificadas, 0);
        assert_eq!(c.bruto, Some(Centavos(4_323_340)));
        assert_eq!(c.descontos, Some(Centavos(1_862_818)));
        assert_eq!(c.liquido, Some(Centavos(2_460_522)));
        assert!(c.prazo_e_parcela_atual);
        // Os dois grupos de consignação (uma em cada página) estão lá.
        let consig = c
            .rubricas
            .iter()
            .filter(|r| r.codigo.as_deref() == Some("43355"))
            .count();
        assert_eq!(consig, 10);
        let retro = &c.rubricas[0];
        assert_eq!(retro.codigo.as_deref(), Some("1002"));
        assert_eq!(retro.tipo, TipoDeRubrica::Provento);
        assert_eq!(retro.competencia, Some(Competencia { ano: 2026, mes: 9 }));
        assert_eq!(c.rubricas[1].valor, Centavos(618_155));
        assert_eq!(c.rubricas[1].codigo.as_deref(), Some("2002"));
        let premio = &c.rubricas[indice(&c, "11264", 250_000)];
        assert_eq!(premio.tipo, TipoDeRubrica::Provento);
        assert_eq!(
            c.rubricas[indice(&c, "4313", 87_344)].tipo,
            TipoDeRubrica::Desconto
        );
    }

    #[test]
    fn le_a_base_de_calculo_do_imposto_de_renda_do_rodape() {
        let c = ler(TEXTO_2026).expect("ler");
        assert_eq!(c.base_irrf, Some(Centavos(1_800_000)));
        // Sem o rótulo, ou com os valores em número diferente dos rótulos, não adivinha.
        let sem = TEXTO_2026.replace("Base Cálculo Imposto Renda", "Outra Coisa Qualquer");
        assert_eq!(ler(&sem).expect("ler").base_irrf, None);
        let quebrado = TEXTO_2026.replace("R$ 18.000,00", "");
        assert_eq!(ler(&quebrado).expect("ler").base_irrf, None);
    }

    #[test]
    fn a_rubrica_fora_do_total_vira_informativa_e_o_liquido_confere() {
        let c = ler(TEXTO_2026).expect("ler");
        let adiantamento = indice(&c, "43100", 1_200_000);
        let informativas: Vec<usize> = (0..c.rubricas.len())
            .filter(|i| c.informativas[*i])
            .collect();
        assert_eq!(informativas, vec![adiantamento]);
        assert!(c.conferido);
        assert_eq!(
            c.avisos,
            vec!["ADIANTAMENTO DE SALARIO não entra no total de descontos do contracheque"]
        );
        assert_eq!(c.liquido_do_mes(), Centavos(2_460_522));
        assert_eq!(c.rubricas_da_composicao(), 32);
    }

    #[test]
    fn sem_uma_rubrica_que_explique_a_diferenca_nao_confere_e_avisa() {
        let t = TEXTO_2026.replace("R$ 18.628,18", "R$ 18.628,19");
        let c = ler(&t).expect("ler");
        assert!(c.informativas.iter().all(|i| !i));
        assert!(!c.conferido);
        assert!(c.avisos.iter().any(|a| a.contains("não bate")));
    }

    #[test]
    fn paginas_com_totais_diferentes_avisam() {
        let ultimo = TEXTO_2026.rfind("R$ 43.233,40").expect("total");
        let t = format!(
            "{}R$ 43.233,41{}",
            &TEXTO_2026[..ultimo],
            &TEXTO_2026[ultimo + "R$ 43.233,40".len()..]
        );
        let c = ler(&t).expect("ler");
        // O total vale o da primeira página.
        assert_eq!(c.bruto, Some(Centavos(4_323_340)));
        assert!(c.avisos.iter().any(|a| a.contains("páginas")));
    }

    #[test]
    fn eventuais_sao_os_retroativos_plr_premio_negociais_e_adiantamento() {
        let c = ler(TEXTO_2026).expect("ler");
        let nomes_eventuais: Vec<(&str, i64)> = c
            .rubricas
            .iter()
            .zip(&c.eventuais)
            .filter(|(_, e)| **e)
            .map(|(r, _)| (r.codigo.as_deref().expect("código"), r.valor.0))
            .collect();
        assert_eq!(nomes_eventuais.len(), 14, "{nomes_eventuais:?}");
        for codigo in ["1002", "1005", "1275", "21230", "11264", "43100", "4396"] {
            assert!(
                nomes_eventuais.iter().any(|(c, _)| *c == codigo),
                "{codigo}"
            );
        }
        // O retroativo de 09/2026 do plano de saúde é eventual; o do mês não.
        assert!(c.eventuais[indice(&c, "4460", 4_210)]);
        assert!(!c.eventuais[indice(&c, "4460", 98_199)]);
        // Fixas.
        for (codigo, valor) in [
            ("2002", 618_155),
            ("2275", 1_051_690),
            ("21041", 9_500),
            ("4313", 87_344),
        ] {
            assert!(!c.eventuais[indice(&c, codigo, valor)], "{codigo}");
        }
        // As seis contribuições negociais.
        let negociais = c
            .rubricas
            .iter()
            .zip(&c.eventuais)
            .filter(|(r, e)| r.nome.starts_with("CONTRIBUICAO NEGOCIAL") && **e)
            .count();
        assert_eq!(negociais, 6);
    }

    #[test]
    fn a_coluna_prazo_das_consignacoes_e_a_parcela_atual_e_nao_ha_termino() {
        let c = ler(TEXTO_2026).expect("ler");
        assert!(c.consignacoes_com_prazo().is_empty());
        let parcelas: Vec<u32> = c.parcelas_atuais.iter().flatten().copied().collect();
        assert_eq!(parcelas.len(), 10);
        assert_eq!(parcelas.iter().filter(|p| **p == 13).count(), 8);
        assert_eq!(parcelas.iter().filter(|p| **p == 14).count(), 2);
        // Nada de "N meses": o prazo cru não vira prazo da rubrica.
        assert!(c.rubricas.iter().all(|r| r.prazo.is_none()));
        // O layout antigo segue como era.
        assert!(
            ler(TEXTO)
                .expect("ler")
                .parcelas_atuais
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn sem_eventuais_e_uma_composicao_so_e_o_layout_antigo_nao_muda() {
        let c = ler(TEXTO).expect("ler");
        assert!(!c.prazo_e_parcela_atual);
        assert_eq!(c.credito, None);
        assert!(c.eventuais.iter().all(|e| !e));
        assert!(c.informativas.iter().all(|e| !e));
        assert!(c.avisos.is_empty());
        let comps = c.composicoes("rec_salario", "abc", &c.eventuais);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].desde, Competencia { ano: 2026, mes: 10 });
        assert_eq!(comps[0].rubricas.len(), 9);
        assert_eq!(comps[0].liquido(), Centavos(569_950));
        assert_eq!(comps[0].origem, "contracheque 10/2026");
    }

    #[test]
    fn com_eventuais_sao_duas_composicoes_com_desde_certos() {
        let c = ler(TEXTO_2026).expect("ler");
        let comps = c.composicoes("rec_salario", "abc", &c.eventuais);
        assert_eq!(comps.len(), 2);
        let (mes, fixas) = (&comps[0], &comps[1]);
        assert_eq!(mes.desde, Competencia { ano: 2026, mes: 10 });
        assert_eq!(fixas.desde, Competencia { ano: 2026, mes: 11 });
        // A do mês tem tudo menos a informativa; o líquido é o declarado.
        assert_eq!(mes.rubricas.len(), 32);
        assert_eq!(mes.liquido(), Centavos(2_460_522));
        assert_eq!(mes.liquido(), c.liquido_do_mes());
        // A das fixas: as 33 menos as 14 eventuais (a informativa é uma delas).
        assert_eq!(fixas.rubricas.len(), 19);
        assert_eq!(fixas.liquido(), c.liquido_das_fixas(&c.eventuais));
        assert_eq!(fixas.documento.as_deref(), Some("abc"));
        assert_eq!(
            c.fixas_desde(&c.eventuais),
            Competencia { ano: 2026, mes: 11 }
        );
        assert!(fixas.origem.contains("fixas"));
    }

    #[test]
    fn trocar_a_marcacao_muda_as_composicoes() {
        let c = ler(TEXTO_2026).expect("ler");
        // Tudo fixo: uma composição só, desde o mês.
        let tudo_fixo = vec![false; c.rubricas.len()];
        let uma = c.composicoes("r", "s", &tudo_fixo);
        assert_eq!(uma.len(), 1);
        assert_eq!(uma[0].desde, Competencia { ano: 2026, mes: 10 });
        assert_eq!(
            c.fixas_desde(&tudo_fixo),
            Competencia { ano: 2026, mes: 10 }
        );
        // O prêmio vira fixo: a composição das fixas ganha uma rubrica.
        let mut marcacao = c.eventuais.clone();
        let premio = indice(&c, "11264", 250_000);
        let antes = c.composicoes("r", "s", &marcacao)[1].rubricas.len();
        marcacao[premio] = false;
        let depois = c.composicoes("r", "s", &marcacao);
        assert_eq!(depois[1].rubricas.len(), antes + 1);
        assert_eq!(depois[0].rubricas.len(), 32);
        // Uma fixa vira eventual: a das fixas perde uma.
        marcacao[indice(&c, "21041", 9_500)] = true;
        assert_eq!(c.composicoes("r", "s", &marcacao)[1].rubricas.len(), antes);
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
