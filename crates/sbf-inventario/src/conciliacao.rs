//! As conciliações óbvias que o desktop sugere (CONTEXT.md, Conciliação: dois registros
//! que descrevem o mesmo fato). O desktop só sugere; nada muda até o usuário confirmar.
//!
//! Um **par óbvio** reúne um registro de conexão ou extrato (`content_hash` nulo) e um
//! registro de documento, com o mesmo valor em centavos, a mesma conta provável (ou uma
//! delas vazia) e datas a até [`DIAS_DE_TOLERANCIA`] dias. Nenhum dos dois pode estar
//! conciliado (`relacao_registro`) nem rejeitado (`estado = 'divergente'`), e o par é
//! **único**: se um registro casa com dois candidatos, não é óbvio e fica fora.

use std::collections::HashMap;

use crate::consulta::Linha;
use crate::consulta::{Celula, Centavos, ResultadoDeConsulta, Tabela};
use crate::referencia::{Ref, Tipo};
use crate::vistas::{
    Contexto, celula_link, celula_valor, coluna, coluna_num, consultar, rotulo_do_registro,
};

/// A diferença máxima de datas, em dias, entre os dois registros de um par.
pub const DIAS_DE_TOLERANCIA: i64 = 3;

/// Um registro que pode entrar num par.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    pub id: i64,
    /// `AAAA-MM-DD`.
    pub data: String,
    pub valor: Centavos,
    pub conta: Option<String>,
    /// `true`: veio de conexão ou extrato (`content_hash` nulo). `false`: de documento.
    pub de_conexao: bool,
    pub descricao: Option<String>,
}

/// Uma sugestão: o registro de extrato e o de documento que descrevem o mesmo fato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Par {
    pub extrato: Candidato,
    pub documento: Candidato,
    /// Dias de diferença entre as datas (sempre positivo ou zero).
    pub dias: i64,
    /// "mesmo valor, 1 dia de diferença, mesma conta".
    pub motivo: String,
}

/// Dias desde 1970-01-01 de uma data `AAAA-MM-DD`; `None` se o texto não é uma data.
fn dia_numero(data: &str) -> Option<i64> {
    let d = data.get(..10)?;
    let mut partes = d.split('-');
    let ano: i64 = partes.next()?.parse().ok()?;
    let mes: i64 = partes.next()?.parse().ok()?;
    let dia: i64 = partes.next()?.parse().ok()?;
    if !(1..=12).contains(&mes) || !(1..=31).contains(&dia) {
        return None;
    }
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = a.div_euclid(400);
    let yoe = a - era * 400;
    let mp = (mes + 9) % 12;
    let doy = (153 * mp + 2) / 5 + dia - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn conta_vazia(c: &Option<String>) -> bool {
    c.as_deref().is_none_or(|s| s.trim().is_empty())
}

/// Os dois são compatíveis? Devolve a diferença em dias.
fn compativeis(extrato: &Candidato, documento: &Candidato) -> Option<i64> {
    if extrato.valor != documento.valor {
        return None;
    }
    if !conta_vazia(&extrato.conta)
        && !conta_vazia(&documento.conta)
        && extrato.conta != documento.conta
    {
        return None;
    }
    let dias = (dia_numero(&extrato.data)? - dia_numero(&documento.data)?).abs();
    (dias <= DIAS_DE_TOLERANCIA).then_some(dias)
}

fn motivo(extrato: &Candidato, documento: &Candidato, dias: i64) -> String {
    let diferenca = match dias {
        0 => "mesmo dia".to_owned(),
        1 => "1 dia de diferença".to_owned(),
        n => format!("{n} dias de diferença"),
    };
    let conta = if conta_vazia(&extrato.conta) || conta_vazia(&documento.conta) {
        "conta em branco num dos lados"
    } else {
        "mesma conta"
    };
    format!("mesmo valor, {diferenca}, {conta}")
}

/// A regra única de par óbvio. `candidatos` já não traz registros conciliados nem
/// rejeitados. A ordem do resultado é a do registro de extrato mais recente primeiro.
pub fn achar_pares(candidatos: &[Candidato]) -> Vec<Par> {
    let mut por_valor: HashMap<Centavos, (Vec<&Candidato>, Vec<&Candidato>)> = HashMap::new();
    for c in candidatos {
        let (extratos, documentos) = por_valor.entry(c.valor).or_default();
        if c.de_conexao {
            extratos.push(c);
        } else {
            documentos.push(c);
        }
    }
    let mut pares = Vec::new();
    for (extratos, documentos) in por_valor.values() {
        for e in extratos {
            let casam: Vec<(&&Candidato, i64)> = documentos
                .iter()
                .filter_map(|d| compativeis(e, d).map(|dias| (d, dias)))
                .collect();
            let [(d, dias)] = casam.as_slice() else {
                continue;
            };
            let unico = extratos
                .iter()
                .filter(|x| compativeis(x, d).is_some())
                .count()
                == 1;
            if unico {
                pares.push(Par {
                    extrato: (*e).clone(),
                    documento: (**d).clone(),
                    dias: *dias,
                    motivo: motivo(e, d, *dias),
                });
            }
        }
    }
    pares.sort_by(|a, b| {
        b.extrato
            .data
            .cmp(&a.extrato.data)
            .then(b.extrato.id.cmp(&a.extrato.id))
    });
    pares
}

/// Os registros do inventário que ainda podem entrar num par.
fn candidatos(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<Candidato>> {
    consultar(
        ctx.conn,
        "SELECT r.id, substr(r.data, 1, 10), r.valor_centavos, r.conta_provavel,
                r.content_hash IS NULL, r.descricao_original
           FROM registro r
          WHERE r.valor_centavos IS NOT NULL AND r.data IS NOT NULL
            AND r.estado <> 'divergente'
            AND r.id NOT IN (
                SELECT a FROM relacao_registro WHERE tipo = 'conciliacao'
                UNION SELECT b FROM relacao_registro WHERE tipo = 'conciliacao')
          ORDER BY r.id",
        &[],
        |l| {
            Ok(Candidato {
                id: l.get(0)?,
                data: l.get(1)?,
                valor: l.get(2)?,
                conta: l.get(3)?,
                de_conexao: l.get(4)?,
                descricao: l.get(5)?,
            })
        },
    )
}

/// Os pares óbvios do banco.
pub fn pares_obvios(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<Par>> {
    Ok(achar_pares(&candidatos(ctx)?))
}

/// A tabela da área Sugestões de conciliação. O alvo da linha é o registro de extrato
/// (único por linha); o de documento é o link da segunda coluna.
pub fn tabela(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Tabela> {
    Ok(tabela_de_pares(&pares_obvios(ctx)?))
}

pub fn tabela_de_pares(pares: &[Par]) -> Tabela {
    let link = |c: &Candidato| {
        celula_link(
            Tipo::Registro,
            &c.id.to_string(),
            &rotulo_do_registro(c.id, &c.descricao),
        )
    };
    Tabela {
        colunas: vec![
            coluna("Extrato ou conexão"),
            coluna("Documento"),
            coluna_num("Valor"),
            coluna("Data do extrato"),
            coluna("Data do documento"),
            coluna("Motivo"),
        ],
        linhas: pares
            .iter()
            .map(|p| Linha {
                alvo: Some(Ref::nova(Tipo::Registro, p.extrato.id.to_string())),
                celulas: vec![
                    link(&p.extrato),
                    link(&p.documento),
                    celula_valor(p.extrato.valor),
                    Celula::Data(p.extrato.data.clone()),
                    Celula::Data(p.documento.data.clone()),
                    Celula::Texto(p.motivo.clone()),
                ],
            })
            .collect(),
    }
}

/// Quantas sugestões há (a contagem da navegação).
pub fn contar(ctx: &Contexto<'_>) -> ResultadoDeConsulta<i64> {
    Ok(pares_obvios(ctx)?.len() as i64)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn c(id: i64, data: &str, valor: i64, conta: Option<&str>, conexao: bool) -> Candidato {
        Candidato {
            id,
            data: data.into(),
            valor,
            conta: conta.map(str::to_owned),
            de_conexao: conexao,
            descricao: None,
        }
    }

    #[test]
    fn par_obvio_com_motivo() {
        let v = [
            c(1, "2026-10-25", -18750, Some("Banco Alfa"), true),
            c(2, "2026-10-24", -18750, Some("Banco Alfa"), false),
        ];
        let p = achar_pares(&v);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].extrato.id, p[0].documento.id), (1, 2));
        assert_eq!(p[0].motivo, "mesmo valor, 1 dia de diferença, mesma conta");
    }

    #[test]
    fn conta_vazia_de_um_lado_casa() {
        let v = [
            c(1, "2026-10-25", -500, None, true),
            c(2, "2026-10-28", -500, Some("Banco Alfa"), false),
        ];
        assert_eq!(achar_pares(&v).len(), 1);
    }

    #[test]
    fn valor_conta_ou_data_diferente_nao_casa() {
        let base = c(1, "2026-10-25", -500, Some("A"), true);
        for outro in [
            c(2, "2026-10-25", -501, Some("A"), false),
            c(2, "2026-10-25", -500, Some("B"), false),
            c(2, "2026-10-29", -500, Some("A"), false),
        ] {
            assert!(achar_pares(&[base.clone(), outro]).is_empty());
        }
    }

    #[test]
    fn mesma_origem_nao_casa() {
        let v = [
            c(1, "2026-10-25", -500, None, true),
            c(2, "2026-10-25", -500, None, true),
        ];
        assert!(achar_pares(&v).is_empty());
    }

    #[test]
    fn ambiguo_fica_fora_dos_dois_lados() {
        // Um extrato, dois documentos.
        let v = [
            c(1, "2026-10-25", -500, None, true),
            c(2, "2026-10-25", -500, None, false),
            c(3, "2026-10-26", -500, None, false),
        ];
        assert!(achar_pares(&v).is_empty());
        // Dois extratos, um documento.
        let v = [
            c(1, "2026-10-25", -500, None, true),
            c(2, "2026-10-26", -500, None, true),
            c(3, "2026-10-25", -500, None, false),
        ];
        assert!(achar_pares(&v).is_empty());
    }

    #[test]
    fn datas_atravessam_o_mes() {
        let v = [
            c(1, "2026-10-31", -500, None, true),
            c(2, "2026-11-03", -500, None, false),
        ];
        let p = achar_pares(&v);
        assert_eq!(p[0].dias, 3);
        assert_eq!(
            p[0].motivo,
            "mesmo valor, 3 dias de diferença, conta em branco num dos lados"
        );
    }
}
