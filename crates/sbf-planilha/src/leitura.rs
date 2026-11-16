//! Leitura da aba mensal (`AAAA.MM`). Só valores em cache: nunca executa macro nem
//! recalcula fórmula. Célula com erro (`#NAME?`, `#VALUE!`...) vira valor ausente mais a
//! marca do erro, nunca zero.

use std::io::{Read, Seek};
use std::path::Path;

use calamine::{Data as Cel, Range, Reader, open_workbook_auto};
use sbf_planejamento::{Centavos, Data};

use crate::texto::{centavos_de_f64, centavos_de_texto};
use crate::{ErroPlanilha, Resultado};

// Colunas (0-based).
const COL_GRUPO: u32 = 1; // B
const COL_DESCRICAO: u32 = 2; // C
const COL_QTD: u32 = 3; // D
const COL_UNITARIO: u32 = 4; // E
const COL_MINIMO: u32 = 5; // F
const COL_VENCIMENTO: u32 = 6; // G
const COL_MEIO: u32 = 7; // H
const COL_PREVISTO: u32 = 8; // I
const COL_PAGO: u32 = 9; // J
const COL_OBS: u32 = 11; // L
const COL_LEGENDA_CODIGO: u32 = 17; // R
const COL_LEGENDA_TEXTO: u32 = 18; // S
const COL_PREVISTO_SALARIO: u32 = 19; // T
const COL_PAGO_SALARIO: u32 = 20; // U
const COL_RUBRICA: u32 = 24; // Y
const COL_RUBRICA_NOME: u32 = 27; // AB
const COLS_RUBRICA_VALORES: [u32; 3] = [28, 29, 30]; // AC, AD, AE
const COLS_FLAGS: std::ops::RangeInclusive<u32> = 32..=37; // AG..AL

/// Primeira linha de dados (1-based).
const PRIMEIRA_LINHA: u32 = 7;
/// Até onde procurar o bloco das consignações e o fim dos dados (1-based).
const LIMITE_PAINEL: u32 = 80;

#[derive(Debug, Clone, PartialEq)]
pub struct ErroDeCelula {
    /// A coluna em letra (`I`, `J`...).
    pub coluna: String,
    /// `#NAME?`, `#VALUE!`...
    pub erro: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinhaDaPlanilha {
    pub aba: String,
    /// Número da linha na planilha (1-based).
    pub linha: u32,
    pub grupo: String,
    pub descricao: String,
    pub qtd: Option<f64>,
    pub unitario: Option<Centavos>,
    pub minimo: Option<Centavos>,
    pub vencimento: Option<Data>,
    pub meio: Option<String>,
    pub previsto: Option<Centavos>,
    pub pago: Option<Centavos>,
    pub obs: Option<String>,
    pub erros: Vec<ErroDeCelula>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtotal {
    pub linha: u32,
    pub grupo: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Salario {
    pub previsto: Option<Centavos>,
    pub pago: Option<Centavos>,
    pub erros: Vec<ErroDeCelula>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rubrica {
    pub linha: u32,
    pub codigo: String,
    pub competencia: Option<String>,
    /// O nome dentro do texto da coluna Y.
    pub nome: String,
    /// O valor do texto da coluna Y.
    pub valor: Option<Centavos>,
    /// O nome da coluna AB, quando houver.
    pub nome_painel: Option<String>,
    /// Colunas AC, AD e AE.
    pub valores: [Option<Centavos>; 3],
    /// Flags 0/1 de AG a AL, com o cabeçalho da linha 3 (Consig, Saude, INSS, Prev, Sal, IR).
    pub flags: Vec<(String, bool)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbaLida {
    pub nome: String,
    pub linhas: Vec<LinhaDaPlanilha>,
    pub salario: Option<Salario>,
    pub rubricas: Vec<Rubrica>,
    pub consignacoes: Vec<Rubrica>,
    /// Código do meio para a descrição (`Salário` para `Desconto Contracheque`).
    pub legenda: Vec<(String, String)>,
    pub subtotais: Vec<Subtotal>,
}

impl AbaLida {
    /// A descrição do meio na legenda, sem diferenciar maiúsculas.
    pub fn descricao_do_meio(&self, meio: &str) -> Option<&str> {
        let m = crate::texto::normalizar(meio);
        self.legenda
            .iter()
            .find(|(c, _)| crate::texto::normalizar(c) == m)
            .map(|(_, d)| d.as_str())
    }

    /// O meio é desconto em contracheque (consignação, não sai do caixa)?
    pub fn meio_e_desconto_em_folha(&self, meio: &str) -> bool {
        match self.descricao_do_meio(meio) {
            Some(d) => crate::texto::normalizar(d).contains("contracheque"),
            None => crate::texto::normalizar(meio) == "salario",
        }
    }
}

/// Os nomes das abas, na ordem do arquivo.
pub fn nomes_das_abas(arquivo: &Path) -> Resultado<Vec<String>> {
    let wb = open_workbook_auto(arquivo).map_err(|e| ErroPlanilha::Abrir(e.to_string()))?;
    Ok(wb.sheet_names())
}

/// Lê uma aba do arquivo.
pub fn ler_aba(arquivo: &Path, aba: &str) -> Resultado<AbaLida> {
    let mut wb = open_workbook_auto(arquivo).map_err(|e| ErroPlanilha::Abrir(e.to_string()))?;
    ler_do_livro(&mut wb, aba)
}

fn ler_do_livro<R: Reader<RS>, RS: Read + Seek>(wb: &mut R, aba: &str) -> Resultado<AbaLida> {
    if !wb.sheet_names().iter().any(|n| n == aba) {
        return Err(ErroPlanilha::AbaInexistente(aba.to_owned()));
    }
    let faixa = wb
        .worksheet_range(aba)
        .map_err(|_| ErroPlanilha::Abrir("aba ilegível".to_owned()))?;
    Ok(ler_faixa(aba, &faixa))
}

// ---------------------------------------------------------------------------------------
// Células
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Valor {
    Vazio,
    Texto(String),
    Numero(f64),
    /// Número de série de data do Excel.
    DataSerie(f64),
    Erro(String),
}

fn celula(faixa: &Range<Cel>, linha: u32, coluna: u32) -> Valor {
    // `linha` é 1-based; a faixa usa posições absolutas 0-based.
    let Some(c) = faixa.get_value((linha - 1, coluna)) else {
        return Valor::Vazio;
    };
    match c {
        Cel::Empty => Valor::Vazio,
        Cel::String(s) | Cel::DateTimeIso(s) | Cel::DurationIso(s) => {
            let t = s.trim();
            if t.is_empty() {
                Valor::Vazio
            } else if t.starts_with('#') && (t.ends_with('!') || t.ends_with('?') || t == "#N/A")
            {
                Valor::Erro(t.to_owned())
            } else {
                Valor::Texto(t.to_owned())
            }
        }
        Cel::Float(f) => Valor::Numero(*f),
        Cel::Int(i) => Valor::Numero(*i as f64),
        Cel::Bool(b) => Valor::Numero(f64::from(u8::from(*b))),
        Cel::DateTime(d) => Valor::DataSerie(d.as_f64()),
        Cel::Error(e) => Valor::Erro(e.to_string()),
    }
}

fn letra(coluna: u32) -> String {
    let mut n = coluna + 1;
    let mut s = String::new();
    while n > 0 {
        let r = ((n - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        n = (n - 1) / 26;
    }
    s
}

struct Leitor<'a> {
    faixa: &'a Range<Cel>,
}

impl Leitor<'_> {
    fn v(&self, linha: u32, coluna: u32) -> Valor {
        celula(self.faixa, linha, coluna)
    }

    fn texto(&self, linha: u32, coluna: u32) -> Option<String> {
        match self.v(linha, coluna) {
            Valor::Texto(t) => Some(t),
            Valor::Numero(n) => Some(format!("{n}")),
            _ => None,
        }
    }

    fn dinheiro(
        &self,
        linha: u32,
        coluna: u32,
        erros: &mut Vec<ErroDeCelula>,
    ) -> Option<Centavos> {
        match self.v(linha, coluna) {
            Valor::Numero(n) => centavos_de_f64(n),
            Valor::Texto(t) => centavos_de_texto(&t),
            Valor::Erro(e) => {
                erros.push(ErroDeCelula { coluna: letra(coluna), erro: e });
                None
            }
            Valor::Vazio | Valor::DataSerie(_) => None,
        }
    }

    fn data(&self, linha: u32, coluna: u32, erros: &mut Vec<ErroDeCelula>) -> Option<Data> {
        match self.v(linha, coluna) {
            Valor::DataSerie(s) => data_de_serie(s),
            // Número solto numa coluna de data: só aceito se for uma data plausível.
            Valor::Numero(n) if (20_000.0..80_000.0).contains(&n) => data_de_serie(n),
            Valor::Texto(t) => data_de_texto(&t),
            Valor::Erro(e) => {
                erros.push(ErroDeCelula { coluna: letra(coluna), erro: e });
                None
            }
            _ => None,
        }
    }
}

/// Dias desde 30/12/1899 (a época do Excel, já com o ano 1900 "bissexto") para a data.
fn data_de_serie(serie: f64) -> Option<Data> {
    if !serie.is_finite() || serie < 1.0 {
        return None;
    }
    let dias = serie.floor() as i64 - 25_569;
    Some(Data::de_dias_desde_1970(dias))
}

fn data_de_texto(t: &str) -> Option<Data> {
    let t = t.trim();
    if let Some(d) = Data::de_iso(t.get(..10).unwrap_or(t)) {
        return Some(d);
    }
    let mut partes = t.split('/');
    let dia = partes.next()?.trim().parse().ok()?;
    let mes = partes.next()?.trim().parse().ok()?;
    let ano: i32 = partes.next()?.trim().parse().ok()?;
    Data::nova(if ano < 100 { 2000 + ano } else { ano }, mes, dia)
}

// ---------------------------------------------------------------------------------------
// A aba
// ---------------------------------------------------------------------------------------

fn ler_faixa(aba: &str, faixa: &Range<Cel>) -> AbaLida {
    let l = Leitor { faixa };
    let ultima = faixa.end().map_or(0, |(r, _)| r + 1);
    let mut linhas = Vec::new();
    let mut subtotais = Vec::new();
    let mut grupo = String::new();

    for n in PRIMEIRA_LINHA..=ultima {
        let b = l.texto(n, COL_GRUPO);
        let c = l.texto(n, COL_DESCRICAO);
        if let Some(b) = &b {
            grupo = b.clone();
        }
        let Some(descricao) = c else {
            if let Some(b) = b {
                // Grupo preenchido e descrição vazia: cabeçalho ou subtotal de grupo.
                subtotais.push(Subtotal { linha: n, grupo: b });
            }
            continue;
        };
        let mut erros = Vec::new();
        linhas.push(LinhaDaPlanilha {
            aba: aba.to_owned(),
            linha: n,
            grupo: grupo.clone(),
            descricao,
            qtd: match l.v(n, COL_QTD) {
                Valor::Numero(q) => Some(q),
                _ => None,
            },
            unitario: l.dinheiro(n, COL_UNITARIO, &mut erros),
            minimo: l.dinheiro(n, COL_MINIMO, &mut erros),
            vencimento: l.data(n, COL_VENCIMENTO, &mut erros),
            meio: l.texto(n, COL_MEIO),
            previsto: l.dinheiro(n, COL_PREVISTO, &mut erros),
            pago: l.dinheiro(n, COL_PAGO, &mut erros),
            obs: l.texto(n, COL_OBS),
            erros,
        });
    }

    let salario = (l.texto(3, COL_LEGENDA_CODIGO).map(|t| crate::texto::normalizar(&t))
        == Some("salario".to_owned()))
    .then(|| {
        let mut erros = Vec::new();
        let previsto = l.dinheiro(3, COL_PREVISTO_SALARIO, &mut erros);
        let pago = l.dinheiro(3, COL_PAGO_SALARIO, &mut erros);
        Salario { previsto, pago, erros }
    });

    let cabecalhos: Vec<String> = COLS_FLAGS
        .map(|c| l.texto(3, c).unwrap_or_default())
        .collect();
    let rubricas = (4..=13)
        .filter_map(|n| rubrica(&l, n, &cabecalhos))
        .collect();

    // O bloco das consignações abre na linha com "NÃO DEBITADO" (Y16) e vai até a 1ª
    // linha de Y vazia.
    let marca = (14..=LIMITE_PAINEL).find(|&n| {
        l.texto(n, COL_RUBRICA)
            .is_some_and(|t| crate::texto::normalizar(&t).contains("nao debitado"))
    });
    let mut consignacoes = Vec::new();
    if let Some(m) = marca {
        for n in (m + 1)..=LIMITE_PAINEL {
            if l.texto(n, COL_RUBRICA).is_none() {
                break;
            }
            if let Some(r) = rubrica(&l, n, &cabecalhos) {
                consignacoes.push(r);
            }
        }
    }

    let legenda = (9..=13)
        .filter_map(|n| Some((l.texto(n, COL_LEGENDA_CODIGO)?, l.texto(n, COL_LEGENDA_TEXTO)?)))
        .collect();

    AbaLida {
        nome: aba.to_owned(),
        linhas,
        salario,
        rubricas,
        consignacoes,
        legenda,
        subtotais,
    }
}

/// `CÓDIGO NOME AAAA/MM VALOR`: o código é a 1ª palavra, o valor a última e o período
/// a palavra `AAAA/MM`; o resto é o nome.
fn rubrica(l: &Leitor<'_>, linha: u32, cabecalhos: &[String]) -> Option<Rubrica> {
    let texto = l.texto(linha, COL_RUBRICA)?;
    let palavras: Vec<&str> = texto.split_whitespace().collect();
    let codigo = (*palavras.first()?).to_owned();
    let valor = (palavras.len() > 1)
        .then(|| palavras.last().and_then(|p| centavos_de_texto(p)))
        .flatten();
    let periodo = palavras.iter().position(|p| {
        p.len() == 7 && p.as_bytes()[4] == b'/' && p[..4].chars().all(|c| c.is_ascii_digit())
    });
    let fim_do_nome = periodo.unwrap_or(if valor.is_some() {
        palavras.len() - 1
    } else {
        palavras.len()
    });
    let nome = palavras.get(1..fim_do_nome).unwrap_or(&[]).join(" ");
    let mut ignorados = Vec::new();
    let valores = COLS_RUBRICA_VALORES.map(|c| l.dinheiro(linha, c, &mut ignorados));
    let flags = COLS_FLAGS
        .zip(cabecalhos.iter())
        .map(|(c, nome)| {
            let ligado = matches!(l.v(linha, c), Valor::Numero(n) if n != 0.0);
            (nome.clone(), ligado)
        })
        .collect();
    Some(Rubrica {
        linha,
        codigo,
        competencia: periodo.map(|i| palavras[i].to_owned()),
        nome,
        valor,
        nome_painel: l.texto(linha, COL_RUBRICA_NOME),
        valores,
        flags,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fixtura::{ABA, planilha_ficticia};

    fn lida() -> AbaLida {
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("p.xlsx");
        planilha_ficticia(&arq);
        assert_eq!(nomes_das_abas(&arq).expect("abas"), ["Resumo", ABA]);
        ler_aba(&arq, ABA).expect("aba")
    }

    #[test]
    fn grupo_propaga_e_subtotal_nao_e_linha() {
        let a = lida();
        assert_eq!(a.linhas.len(), 8);
        assert_eq!(a.subtotais.len(), 1);
        assert_eq!(a.subtotais[0].linha, 9);
        let internet = &a.linhas[1];
        assert_eq!(internet.grupo, "Essencial");
        assert_eq!(internet.linha, 8);
        let luz = a.linhas.iter().find(|l| l.descricao == "Luz Exemplo").expect("luz");
        assert_eq!(luz.grupo, "Lar");
        let plano = a.linhas.iter().find(|l| l.descricao.starts_with("Plano")).expect("plano");
        assert_eq!(plano.grupo, "Lar");
        assert_eq!(plano.meio.as_deref(), Some("Salário"));
    }

    #[test]
    fn erro_de_formula_vira_ausente_com_marca_e_nunca_zero() {
        let a = lida();
        let internet = &a.linhas[1];
        assert_eq!(internet.previsto, None);
        assert_eq!(internet.erros.len(), 1);
        assert_eq!(internet.erros[0].coluna, "I");
        assert_eq!(internet.erros[0].erro, "#NAME?");
        assert_eq!(internet.obs.as_deref(), Some("obs da internet"));
        // Vazio sem erro também é ausente, mas sem marca.
        let sem = a.linhas.iter().find(|l| l.descricao == "Sem Previsto").expect("linha");
        assert_eq!(sem.previsto, None);
        assert!(sem.erros.is_empty());
    }

    #[test]
    fn valores_em_centavos_e_datas() {
        let a = lida();
        let aluguel = &a.linhas[0];
        assert_eq!(aluguel.previsto, Some(Centavos(150_000)));
        assert_eq!(aluguel.pago, Some(Centavos(150_000)));
        assert_eq!(aluguel.vencimento, Data::nova(2026, 11, 5));
        let soma = a.linhas.iter().find(|l| l.descricao == "Taxa Soma").expect("linha");
        assert_eq!(soma.previsto, Some(Centavos(30)));
        let luz = a.linhas.iter().find(|l| l.descricao == "Luz Exemplo").expect("luz");
        assert_eq!(luz.previsto, Some(Centavos(18_050)));
        assert_eq!(luz.pago, None);
    }

    #[test]
    fn painel_salario_rubricas_consignacoes_e_legenda() {
        let a = lida();
        let s = a.salario.as_ref().expect("salario");
        assert_eq!(s.previsto, Some(Centavos(500_000)));
        assert_eq!(s.pago, Some(Centavos(500_000)));
        assert_eq!(a.rubricas.len(), 2);
        let base = &a.rubricas[0];
        assert_eq!(base.codigo, "001");
        assert_eq!(base.nome, "SALARIO BASE");
        assert_eq!(base.competencia.as_deref(), Some("2026/11"));
        assert_eq!(base.valor, Some(Centavos(500_000)));
        assert_eq!(base.flags.len(), 6);
        assert_eq!(base.flags[4], ("Sal".to_owned(), true));
        assert_eq!(a.rubricas[1].flags[2], ("INSS".to_owned(), true));
        assert_eq!(a.consignacoes.len(), 2);
        assert_eq!(a.consignacoes[1].valor, Some(Centavos(40_000)));
        assert_eq!(a.consignacoes[0].flags[0], ("Consig".to_owned(), true));
        assert_eq!(a.legenda.len(), 3);
        assert!(a.meio_e_desconto_em_folha("Salário"));
        assert!(!a.meio_e_desconto_em_folha("Nubank"));
        assert_eq!(a.descricao_do_meio("salario"), Some("Desconto Contracheque"));
    }

    #[test]
    fn aba_inexistente_e_arquivo_ruim() {
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("p.xlsx");
        planilha_ficticia(&arq);
        assert!(matches!(ler_aba(&arq, "2099.01"), Err(ErroPlanilha::AbaInexistente(_))));
        let ruim = pasta.path().join("x.xlsx");
        std::fs::write(&ruim, b"nao e planilha").expect("escrever");
        assert!(matches!(ler_aba(&ruim, ABA), Err(ErroPlanilha::Abrir(_))));
    }
}
