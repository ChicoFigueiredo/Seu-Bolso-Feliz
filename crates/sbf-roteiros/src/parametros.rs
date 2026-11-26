//! Os **parâmetros do roteiro** (spec 18): o que o manifesto declara, o que a agenda, o
//! terminal e a janela entregam, e a resolução para o que o Luau enxerga na tabela `parametros`.
//!
//! Só existem dois tipos, porque nunca é segredo: `mes` e `booleano`.
//!
//! - `mes` aceita o absoluto `"AAAA-MM"` ou o relativo ao mês corrente: `"0"` (este mês),
//!   `"-1"` (o anterior), `"-12"`... (inteiro menor ou igual a zero, em texto). O relativo é
//!   resolvido aqui, no fuso do usuário (UTC−3), e o roteiro só vê o `"AAAA-MM"` já resolvido.
//! - `booleano` é `true` ou `false`.
//!
//! Quando o roteiro declara `de` e `ate` (ambos `mes`), vale `de` ≤ `ate` e no máximo 60 meses
//! no intervalo (o mesmo teto de `util.meses`).

use crate::erro::{ErroDeRoteiro, Resultado};
use crate::escrito::{FUSO_EM_SEGUNDOS, data_dos_dias};
use bancada_navegador::luau::{Parametros, ValorDeParametro};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Quantos parâmetros um manifesto declara, no máximo.
pub const MAXIMO_DE_PARAMETROS: usize = 8;
/// O maior intervalo (em meses, contando as duas pontas) entre `de` e `ate`.
pub const MAXIMO_DE_MESES: i64 = 60;
/// O mês relativo mais antigo aceito (100 anos para trás: mais que isso é erro de digitação).
const RELATIVO_MAXIMO: i64 = 1_200;

/// O tipo de um parâmetro declarado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeParametro {
    /// Um mês: `"AAAA-MM"` ou relativo (`"0"`, `"-1"`).
    Mes,
    /// Sim ou não.
    Booleano,
}

/// O valor como o JSON o traz (antes de resolver o mês relativo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValorBruto {
    /// Texto (um mês).
    Texto(String),
    /// Sim ou não.
    Booleano(bool),
}

/// Um parâmetro declarado no manifesto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametroDeclarado {
    /// `[a-z][a-z0-9_]{0,31}`.
    pub nome: String,
    pub tipo: TipoDeParametro,
    /// O valor quando ninguém entrega nenhum: `"0"` para `mes`, `false` para `booleano`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padrao: Option<ValorBruto>,
}

impl ParametroDeclarado {
    fn padrao_efetivo(&self) -> ValorBruto {
        self.padrao.clone().unwrap_or(match self.tipo {
            TipoDeParametro::Mes => ValorBruto::Texto("0".to_owned()),
            TipoDeParametro::Booleano => ValorBruto::Booleano(false),
        })
    }
}

/// Um mês como foi escrito: absoluto ou relativo ao corrente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EspecDeMes {
    /// `"AAAA-MM"`.
    Absoluto { ano: i64, mes: i64 },
    /// `"0"`, `"-1"`: quantos meses antes do corrente (0 ou negativo).
    Relativo(i64),
}

/// Um mês como número (`ano * 12 + mes - 1`), para comparar e contar.
type Ordinal = i64;

fn ordinal(ano: i64, mes: i64) -> Ordinal {
    ano * 12 + (mes - 1)
}

fn texto_do_ordinal(o: Ordinal) -> String {
    format!("{:04}-{:02}", o.div_euclid(12), o.rem_euclid(12) + 1)
}

impl EspecDeMes {
    /// Lê `"AAAA-MM"` (ano de 1 a 9999) ou um inteiro menor ou igual a zero.
    #[must_use]
    pub fn ler(texto: &str) -> Option<Self> {
        let b = texto.as_bytes();
        if b.len() == 7 && b[4] == b'-' {
            let digitos = b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit);
            if !digitos {
                return None;
            }
            let ano: i64 = texto[..4].parse().ok()?;
            let mes: i64 = texto[5..].parse().ok()?;
            return ((1..=9999).contains(&ano) && (1..=12).contains(&mes))
                .then_some(Self::Absoluto { ano, mes });
        }
        // Relativo: "0" ou "-N" (N de 1 a 1200), sem "+", sem zero à esquerda.
        if texto == "0" {
            return Some(Self::Relativo(0));
        }
        let n = texto.strip_prefix('-')?;
        let ok = !n.is_empty()
            && n.len() <= 4
            && n.bytes().all(|c| c.is_ascii_digit())
            && !n.starts_with('0');
        let n: i64 = n.parse().ok().filter(|_| ok)?;
        (n <= RELATIVO_MAXIMO).then_some(Self::Relativo(-n))
    }

    /// O mês resolvido contra o `corrente` (`ano`, `mes`); `None` se cair fora do ano 1 a 9999.
    #[must_use]
    pub fn resolver(self, corrente: (i64, i64)) -> Option<(i64, i64)> {
        let o = self.ordinal(corrente);
        let ano = o.div_euclid(12);
        ((1..=9999).contains(&ano)).then_some((ano, o.rem_euclid(12) + 1))
    }

    fn ordinal(self, corrente: (i64, i64)) -> Ordinal {
        match self {
            Self::Absoluto { ano, mes } => ordinal(ano, mes),
            Self::Relativo(n) => ordinal(corrente.0, corrente.1) + n,
        }
    }

    fn e_relativo(self) -> bool {
        matches!(self, Self::Relativo(_))
    }
}

fn invalido(m: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::Invalido(format!("parâmetros: {}", m.into()))
}

/// O mês (ano, mês) de `agora_unix` no fuso do usuário (UTC−3): às 21h do dia 30 ainda é o
/// mês do dia 30, mesmo que o UTC já esteja no mês seguinte.
#[must_use]
pub fn mes_corrente(agora_unix: i64) -> (i64, i64) {
    let (a, m, _) = data_dos_dias(
        agora_unix
            .saturating_add(FUSO_EM_SEGUNDOS)
            .div_euclid(86_400),
    );
    (a, m)
}

fn nome_valido(nome: &str) -> bool {
    let mut cs = nome.bytes();
    nome.len() <= 32
        && cs.next().is_some_and(|c| c.is_ascii_lowercase())
        && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

/// O valor combina com o tipo? (Sem ecoar o valor na mensagem.)
fn conferir_valor(p: &ParametroDeclarado, v: &ValorBruto, onde: &str) -> Resultado<()> {
    match (p.tipo, v) {
        (TipoDeParametro::Mes, ValorBruto::Texto(t)) if EspecDeMes::ler(t).is_some() => Ok(()),
        (TipoDeParametro::Mes, _) => Err(invalido(format!(
            "{onde}: `{}` é um mês e tem de ser \"AAAA-MM\" ou relativo (\"0\", \"-1\"...)",
            p.nome
        ))),
        (TipoDeParametro::Booleano, ValorBruto::Booleano(_)) => Ok(()),
        (TipoDeParametro::Booleano, _) => Err(invalido(format!(
            "{onde}: `{}` é sim ou não e tem de ser true ou false",
            p.nome
        ))),
    }
}

/// A declaração do manifesto: até 8, nomes únicos e válidos, padrões do tipo certo.
///
/// # Errors
/// [`ErroDeRoteiro::Invalido`] com o motivo.
pub fn validar_declaracao(declarados: &[ParametroDeclarado]) -> Resultado<()> {
    if declarados.len() > MAXIMO_DE_PARAMETROS {
        return Err(invalido(format!(
            "no máximo {MAXIMO_DE_PARAMETROS} parâmetros declarados"
        )));
    }
    for (i, p) in declarados.iter().enumerate() {
        if !nome_valido(&p.nome) {
            return Err(invalido(
                "o nome de um parâmetro tem de ser `[a-z][a-z0-9_]{0,31}` (minúsculas, dígitos e _)",
            ));
        }
        if declarados[..i].iter().any(|o| o.nome == p.nome) {
            return Err(invalido(format!(
                "o parâmetro `{}` está declarado duas vezes",
                p.nome
            )));
        }
        if let Some(v) = &p.padrao {
            conferir_valor(p, v, "padrao")?;
        }
    }
    Ok(())
}

/// Confere os valores entregues (por uma entrada da agenda, pelo terminal ou pela janela)
/// contra a declaração: o nome tem de ter sido declarado e o valor, ser do tipo.
///
/// # Errors
/// [`ErroDeRoteiro::Invalido`] com o motivo.
pub fn validar_entregues(
    declarados: &[ParametroDeclarado],
    entregues: &BTreeMap<String, ValorBruto>,
    onde: &str,
) -> Resultado<()> {
    for (nome, v) in entregues {
        let Some(p) = declarados.iter().find(|p| &p.nome == nome) else {
            return Err(invalido(format!(
                "{onde}: o roteiro não declara o parâmetro `{nome}`"
            )));
        };
        conferir_valor(p, v, onde)?;
    }
    Ok(())
}

/// `de` ≤ `ate` e no máximo [`MAXIMO_DE_MESES`] meses, contando as duas pontas.
fn conferir_intervalo(de: Ordinal, ate: Ordinal) -> Resultado<()> {
    if de > ate {
        return Err(invalido("`de` não pode ser depois de `ate`"));
    }
    if ate - de + 1 > MAXIMO_DE_MESES {
        return Err(invalido(format!(
            "o intervalo de `de` a `ate` passa de {MAXIMO_DE_MESES} meses"
        )));
    }
    Ok(())
}

/// A checagem estática de uma entrega (no manifesto, sem relógio): se `de` e `ate` são ambos
/// absolutos, ou ambos relativos, o intervalo já se confere. Misto só se confere ao rodar.
///
/// # Errors
/// [`ErroDeRoteiro::Invalido`] com o motivo.
pub fn conferir_intervalo_estatico(
    declarados: &[ParametroDeclarado],
    entregues: &BTreeMap<String, ValorBruto>,
) -> Resultado<()> {
    let Some((de, ate)) = par_de_meses(declarados, |nome| {
        entregues.get(nome).cloned().or_else(|| {
            declarados
                .iter()
                .find(|p| p.nome == nome)
                .map(ParametroDeclarado::padrao_efetivo)
        })
    }) else {
        return Ok(());
    };
    if de.e_relativo() == ate.e_relativo() {
        // Mesma natureza: o corrente se cancela; qualquer um serve de referência.
        conferir_intervalo(de.ordinal((2000, 1)), ate.ordinal((2000, 1)))?;
    }
    Ok(())
}

/// Os `de` e `ate` quando os dois são declarados como `mes` e têm valor legível.
fn par_de_meses(
    declarados: &[ParametroDeclarado],
    valor: impl Fn(&str) -> Option<ValorBruto>,
) -> Option<(EspecDeMes, EspecDeMes)> {
    let e_mes = |n: &str| {
        declarados
            .iter()
            .any(|p| p.nome == n && p.tipo == TipoDeParametro::Mes)
    };
    if !(e_mes("de") && e_mes("ate")) {
        return None;
    }
    let ler = |n: &str| match valor(n)? {
        ValorBruto::Texto(t) => EspecDeMes::ler(&t),
        ValorBruto::Booleano(_) => None,
    };
    Some((ler("de")?, ler("ate")?))
}

/// Resolve os parâmetros de uma execução: o que foi entregue, senão o padrão da declaração;
/// o mês relativo vira `"AAAA-MM"` contra o `corrente`. É o que o Luau recebe.
///
/// # Errors
/// Nome fora da declaração, valor do tipo errado, mês fora do calendário, `de` depois de
/// `ate` ou intervalo de mais de 60 meses.
pub fn resolver(
    declarados: &[ParametroDeclarado],
    entregues: &BTreeMap<String, ValorBruto>,
    corrente: (i64, i64),
) -> Resultado<Parametros> {
    validar_entregues(declarados, entregues, "execução")?;
    let mut saida = Parametros::novo();
    let mut meses: BTreeMap<&str, Ordinal> = BTreeMap::new();
    for p in declarados {
        let valor = entregues
            .get(&p.nome)
            .cloned()
            .unwrap_or_else(|| p.padrao_efetivo());
        match (p.tipo, valor) {
            (TipoDeParametro::Mes, ValorBruto::Texto(t)) => {
                let (ano, mes) = EspecDeMes::ler(&t)
                    .and_then(|e| e.resolver(corrente))
                    .ok_or_else(|| {
                        invalido(format!(
                            "`{}` cai fora do calendário (anos 1 a 9999)",
                            p.nome
                        ))
                    })?;
                meses.insert(p.nome.as_str(), ordinal(ano, mes));
                saida.inserir_texto(p.nome.clone(), texto_do_ordinal(ordinal(ano, mes)));
            }
            (TipoDeParametro::Booleano, ValorBruto::Booleano(b)) => {
                saida.inserir_booleano(p.nome.clone(), b);
            }
            _ => {
                return Err(invalido(format!(
                    "`{}` não combina com o tipo declarado",
                    p.nome
                )));
            }
        }
    }
    if let (Some(&de), Some(&ate)) = (meses.get("de"), meses.get("ate")) {
        conferir_intervalo(de, ate)?;
    }
    Ok(saida)
}

/// Os parâmetros resolvidos como JSON (`{"ate":"2026-11","de":"2026-10"}`), para o histórico.
/// Só mês e sim/não: nunca há segredo aqui.
#[must_use]
pub fn para_json(p: &Parametros) -> String {
    let mapa: serde_json::Map<String, serde_json::Value> = p
        .iter()
        .map(|(n, v)| {
            let valor = match v {
                ValorDeParametro::Texto(t) => serde_json::Value::String(t.clone()),
                ValorDeParametro::Booleano(b) => serde_json::Value::Bool(*b),
            };
            (n.to_owned(), valor)
        })
        .collect();
    serde_json::Value::Object(mapa).to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    fn decl(nome: &str, tipo: TipoDeParametro, padrao: Option<ValorBruto>) -> ParametroDeclarado {
        ParametroDeclarado {
            nome: nome.into(),
            tipo,
            padrao,
        }
    }

    fn de_ate() -> Vec<ParametroDeclarado> {
        vec![
            decl(
                "de",
                TipoDeParametro::Mes,
                Some(ValorBruto::Texto("0".into())),
            ),
            decl(
                "ate",
                TipoDeParametro::Mes,
                Some(ValorBruto::Texto("0".into())),
            ),
            decl(
                "forcar",
                TipoDeParametro::Booleano,
                Some(ValorBruto::Booleano(false)),
            ),
        ]
    }

    fn entrega(pares: &[(&str, &str)]) -> BTreeMap<String, ValorBruto> {
        pares
            .iter()
            .map(|(n, v)| ((*n).to_owned(), ValorBruto::Texto((*v).to_owned())))
            .collect()
    }

    fn texto(p: &Parametros, nome: &str) -> String {
        match p.obter(nome) {
            Some(ValorDeParametro::Texto(t)) => t.clone(),
            outro => panic!("{nome}: {outro:?}"),
        }
    }

    #[test]
    fn le_mes_absoluto_e_relativo() {
        assert_eq!(
            EspecDeMes::ler("2026-11"),
            Some(EspecDeMes::Absoluto { ano: 2026, mes: 11 })
        );
        assert_eq!(EspecDeMes::ler("0"), Some(EspecDeMes::Relativo(0)));
        assert_eq!(EspecDeMes::ler("-1"), Some(EspecDeMes::Relativo(-1)));
        assert_eq!(EspecDeMes::ler("-12"), Some(EspecDeMes::Relativo(-12)));
        assert_eq!(EspecDeMes::ler("-1200"), Some(EspecDeMes::Relativo(-1200)));
        for ruim in [
            "",
            "1",
            "+1",
            "-0",
            "-01",
            "-1201",
            "--1",
            "2026-13",
            "2026-00",
            "0000-05",
            "2026-1",
            "26-11",
            "2026/11",
            "2026-11-01",
            "abc",
            " 0",
            "0 ",
            "２０２６-１１",
            "-",
        ] {
            assert_eq!(EspecDeMes::ler(ruim), None, "{ruim:?}");
        }
    }

    #[test]
    fn relativo_resolve_contra_o_mes_corrente_com_virada_de_ano() {
        let r = |t: &str, c| EspecDeMes::ler(t).unwrap().resolver(c);
        assert_eq!(r("0", (2026, 11)), Some((2026, 11)));
        assert_eq!(r("-1", (2026, 11)), Some((2026, 10)));
        assert_eq!(r("-1", (2026, 1)), Some((2025, 12)));
        assert_eq!(r("-12", (2026, 11)), Some((2025, 11)));
        assert_eq!(r("-13", (2026, 11)), Some((2025, 10)));
        assert_eq!(r("2024-02", (2026, 11)), Some((2024, 2)));
        // antes do ano 1
        assert_eq!(r("-1200", (50, 1)), None);
    }

    #[test]
    fn o_mes_corrente_usa_o_fuso_do_usuario() {
        // 2026-12-01T00:30:00Z = 21:30 de 30/11 em UTC−3: ainda é novembro.
        let unix = 1_796_085_000; // 2026-12-01T00:30:00Z
        assert_eq!(mes_corrente(unix), (2026, 11));
        // 2026-12-01T03:00:00Z = 00:00 de 01/12 em UTC−3.
        assert_eq!(mes_corrente(unix + 2 * 3600 + 30 * 60), (2026, 12));
    }

    #[test]
    fn declaracao_valida_e_recusa_o_resto() {
        assert!(validar_declaracao(&de_ate()).is_ok());
        assert!(validar_declaracao(&[]).is_ok());
        // nome
        for ruim in ["", "De", "1a", "a-b", "a b", "ç", &"a".repeat(33)] {
            let d = vec![decl(ruim, TipoDeParametro::Booleano, None)];
            assert!(validar_declaracao(&d).is_err(), "{ruim:?}");
        }
        assert!(
            validar_declaracao(&[decl(&"a".repeat(32), TipoDeParametro::Booleano, None)]).is_ok()
        );
        assert!(validar_declaracao(&[decl("a_1", TipoDeParametro::Booleano, None)]).is_ok());
        // repetido
        let repetido = vec![
            decl("de", TipoDeParametro::Mes, None),
            decl("de", TipoDeParametro::Mes, None),
        ];
        assert!(validar_declaracao(&repetido).is_err());
        // máximo de 8
        let oito: Vec<_> = (0..8)
            .map(|i| decl(&format!("p{i}"), TipoDeParametro::Booleano, None))
            .collect();
        assert!(validar_declaracao(&oito).is_ok());
        let nove: Vec<_> = (0..9)
            .map(|i| decl(&format!("p{i}"), TipoDeParametro::Booleano, None))
            .collect();
        assert!(validar_declaracao(&nove).is_err());
        // padrão do tipo errado
        assert!(
            validar_declaracao(&[decl(
                "de",
                TipoDeParametro::Mes,
                Some(ValorBruto::Booleano(true))
            )])
            .is_err()
        );
        assert!(
            validar_declaracao(&[decl(
                "de",
                TipoDeParametro::Mes,
                Some(ValorBruto::Texto("ontem".into()))
            )])
            .is_err()
        );
        assert!(
            validar_declaracao(&[decl(
                "f",
                TipoDeParametro::Booleano,
                Some(ValorBruto::Texto("true".into()))
            )])
            .is_err()
        );
    }

    #[test]
    fn resolve_os_padroes_e_o_que_foi_entregue() {
        let p = resolver(&de_ate(), &BTreeMap::new(), (2026, 11)).unwrap();
        assert_eq!(texto(&p, "de"), "2026-11");
        assert_eq!(texto(&p, "ate"), "2026-11");
        assert_eq!(p.obter("forcar"), Some(&ValorDeParametro::Booleano(false)));

        let p = resolver(&de_ate(), &entrega(&[("de", "-1")]), (2026, 11)).unwrap();
        assert_eq!(
            (texto(&p, "de"), texto(&p, "ate")),
            ("2026-10".into(), "2026-11".into())
        );

        let p = resolver(
            &de_ate(),
            &entrega(&[("de", "2025-01"), ("ate", "2025-03")]),
            (2026, 11),
        )
        .unwrap();
        assert_eq!(
            (texto(&p, "de"), texto(&p, "ate")),
            ("2025-01".into(), "2025-03".into())
        );
    }

    #[test]
    fn nome_fora_da_declaracao_e_valor_do_tipo_errado_sao_erro() {
        let e = resolver(&de_ate(), &entrega(&[("mes", "0")]), (2026, 11));
        assert!(e.is_err());
        let e = resolver(&de_ate(), &entrega(&[("forcar", "true")]), (2026, 11));
        assert!(e.is_err(), "booleano não aceita texto");
        let mut b = BTreeMap::new();
        b.insert("de".to_owned(), ValorBruto::Booleano(true));
        assert!(resolver(&de_ate(), &b, (2026, 11)).is_err());
        assert!(resolver(&de_ate(), &entrega(&[("de", "2026-13")]), (2026, 11)).is_err());
        // sem declaração, qualquer entrega é erro
        assert!(resolver(&[], &entrega(&[("de", "0")]), (2026, 11)).is_err());
        assert!(resolver(&[], &BTreeMap::new(), (2026, 11)).unwrap().vazio());
    }

    #[test]
    fn de_depois_de_ate_e_intervalo_de_mais_de_60_meses_sao_erro() {
        // de > ate
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "0"), ("ate", "-1")]),
                (2026, 11)
            )
            .is_err()
        );
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "2026-12"), ("ate", "2026-11")]),
                (2026, 1)
            )
            .is_err()
        );
        // de == ate vale
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "-3"), ("ate", "-3")]),
                (2026, 11)
            )
            .is_ok()
        );
        // 60 meses (contando as pontas) vale, 61 não
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "2022-01"), ("ate", "2026-12")]),
                (2026, 11)
            )
            .is_ok()
        );
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "2022-01"), ("ate", "2027-01")]),
                (2026, 11)
            )
            .is_err()
        );
        // misto: relativo contra absoluto, resolvido contra o corrente
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "2026-10"), ("ate", "0")]),
                (2026, 11)
            )
            .is_ok()
        );
        assert!(
            resolver(
                &de_ate(),
                &entrega(&[("de", "2026-10"), ("ate", "0")]),
                (2026, 9)
            )
            .is_err(),
            "com o corrente em setembro, ate < de"
        );
    }

    #[test]
    fn a_checagem_estatica_pega_o_obvio_e_deixa_o_misto_para_a_execucao() {
        let d = de_ate();
        assert!(conferir_intervalo_estatico(&d, &entrega(&[("de", "-1"), ("ate", "0")])).is_ok());
        assert!(conferir_intervalo_estatico(&d, &entrega(&[("de", "0"), ("ate", "-1")])).is_err());
        assert!(
            conferir_intervalo_estatico(&d, &entrega(&[("de", "2026-12"), ("ate", "2026-11")]))
                .is_err()
        );
        assert!(conferir_intervalo_estatico(&d, &entrega(&[("de", "-61"), ("ate", "0")])).is_err());
        // misto: passa aqui (depende do dia em que roda)
        assert!(
            conferir_intervalo_estatico(&d, &entrega(&[("de", "2026-12"), ("ate", "-1")])).is_ok()
        );
        // sem de/ate: nada a conferir
        assert!(
            conferir_intervalo_estatico(
                &[decl("forcar", TipoDeParametro::Booleano, None)],
                &BTreeMap::new()
            )
            .is_ok()
        );
    }

    #[test]
    fn json_do_historico_so_tem_mes_e_sim_ou_nao() {
        let p = resolver(&de_ate(), &entrega(&[("de", "-1")]), (2026, 11)).unwrap();
        assert_eq!(
            para_json(&p),
            r#"{"ate":"2026-11","de":"2026-10","forcar":false}"#
        );
        assert_eq!(para_json(&Parametros::novo()), "{}");
    }
}
