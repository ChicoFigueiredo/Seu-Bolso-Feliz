//! `sbf planejamento`: importação da planilha, itens ocultos e conciliação (spec 11).
//!
//! ```text
//! sbf planejamento importar --inventario <cofre> --planilha <arq> --aba AAAA.MM [--mapa <arq>] [--confirmar] [--detalhar]
//! sbf planejamento ocultos  --inventario <cofre> [--mapa <arq>] [--confirmar] [--detalhar]
//! sbf planejamento conciliar --inventario <cofre> [--ciclo AAAA-MM] [--mapa <arq>] [--detalhar]
//! ```
//!
//! A saída padrão mostra só contagens. `--detalhar` mostra a prévia linha a linha no
//! terminal (nunca em log). Sem `--confirmar`, nada é gravado. O banco do planejamento é
//! `<cofre>/planejamento.sqlite`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sbf_planejamento::{Centavos, Competencia, Data, Periodo, Planejamento};
use sbf_planilha::conciliacao::{self, ConfigConciliacao};
use sbf_planilha::importacao;
use sbf_planilha::leitura;
use sbf_planilha::mapa::Mapa;
use sbf_planilha::ocultos::{self, ConfigOcultos};

const USO: &str = "uso: sbf planejamento importar --inventario <cofre> --planilha <arq> --aba AAAA.MM [--mapa <arq>] [--confirmar] [--detalhar] | sbf planejamento ocultos --inventario <cofre> [--mapa <arq>] [--confirmar] [--detalhar] | sbf planejamento conciliar --inventario <cofre> [--ciclo AAAA-MM] [--mapa <arq>] [--detalhar]";

type Abrir<'a> = &'a dyn Fn(&Path) -> Result<Box<dyn Planejamento>, String>;

/// O serviço concreto do planejamento sobre `<cofre>/planejamento.sqlite`.
///
/// O planejamento do cofre (`<cofre>/planejamento.sqlite`, criado 0600 se não existir), com os
/// saldos da Pluggy guardados no cofre quando houver.
fn abrir_planejamento(cofre: &Path) -> Result<Box<dyn Planejamento>, String> {
    let banco = sbf_planejamento::Banco::abrir(cofre.join("planejamento.sqlite"))
        .map_err(|e| e.to_string())?;
    Ok(Box::new(sbf_planejamento::PlanejamentoSqlite::novo(
        banco,
        crate::mcp::saldos_do_cofre(cofre),
    )))
}

pub fn rodar(args: &[String]) -> ExitCode {
    let mut saida = std::io::stdout();
    rodar_com(args, &abrir_planejamento, &mut saida)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Opcoes {
    inventario: Option<PathBuf>,
    planilha: Option<PathBuf>,
    aba: Option<String>,
    mapa: Option<PathBuf>,
    ciclo: Option<String>,
    confirmar: bool,
    detalhar: bool,
}

fn ler_opcoes(args: &[String]) -> Result<Opcoes, String> {
    let mut o = Opcoes::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = |nome: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("falta o valor de {nome}"))
        };
        match a.as_str() {
            "--inventario" => o.inventario = Some(valor("--inventario")?.into()),
            "--planilha" => o.planilha = Some(valor("--planilha")?.into()),
            "--aba" => o.aba = Some(valor("--aba")?),
            "--mapa" => o.mapa = Some(valor("--mapa")?.into()),
            "--ciclo" => o.ciclo = Some(valor("--ciclo")?),
            "--confirmar" => o.confirmar = true,
            "--detalhar" => o.detalhar = true,
            outro => return Err(format!("opção desconhecida: {outro}")),
        }
    }
    Ok(o)
}

fn rodar_com(args: &[String], abrir: Abrir<'_>, saida: &mut dyn Write) -> ExitCode {
    let Some(sub) = args.first() else {
        eprintln!("{USO}");
        return ExitCode::from(2);
    };
    let opcoes = match ler_opcoes(&args[1..]) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sbf: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    let resultado = match sub.as_str() {
        "importar" => importar(&opcoes, abrir, saida),
        "ocultos" => achar_ocultos(&opcoes, abrir, saida),
        "conciliar" => conciliar(&opcoes, abrir, saida),
        _ => Err(format!("subcomando desconhecido\n{USO}")),
    };
    match resultado {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(1)
        }
    }
}

fn exige<'a, T>(valor: &'a Option<T>, nome: &str) -> Result<&'a T, String> {
    valor.as_ref().ok_or_else(|| format!("falta {nome}"))
}

fn mapa_de(o: &Opcoes) -> Result<Mapa, String> {
    match &o.mapa {
        Some(p) => Mapa::ler(p).map_err(|e| e.to_string()),
        None => Ok(Mapa::vazio()),
    }
}

fn abrir_inventario(cofre: &Path) -> Result<rusqlite::Connection, String> {
    sbf_inventario::banco::abrir_leitura(cofre.join("inventario.sqlite"))
        .map_err(|e| format!("não deu para abrir o inventário: {e}"))
}

/// R$ 1.234,56, só para o terminal.
fn reais(c: Option<Centavos>) -> String {
    let Some(Centavos(v)) = c else {
        return "sem valor".to_owned();
    };
    let (sinal, v) = if v < 0 { ("-", -v) } else { ("", v) };
    let inteiro = (v / 100).to_string();
    let mut milhares = String::new();
    for (i, ch) in inteiro.chars().enumerate() {
        if i > 0 && (inteiro.len() - i) % 3 == 0 {
            milhares.push('.');
        }
        milhares.push(ch);
    }
    format!("{sinal}R$ {milhares},{:02}", v % 100)
}

fn es(r: std::io::Result<()>) -> Result<(), String> {
    r.map_err(|e| e.to_string())
}

fn importar(o: &Opcoes, abrir: Abrir<'_>, saida: &mut dyn Write) -> Result<(), String> {
    let cofre = exige(&o.inventario, "--inventario")?;
    let arquivo = exige(&o.planilha, "--planilha")?;
    let nome_da_aba = exige(&o.aba, "--aba")?;
    let competencia = leitura::competencia_da_aba(nome_da_aba)
        .ok_or("a aba tem de se chamar AAAA.MM (por exemplo 2026.11)")?;
    let mapa = mapa_de(o)?;
    let sha = leitura::sha256_do_arquivo(arquivo).map_err(|e| e.to_string())?;
    let aba = leitura::ler_aba(arquivo, nome_da_aba).map_err(|e| e.to_string())?;
    let mut plan = abrir(cofre)?;
    let previa = importacao::previa_com_mapa(&aba, &sha, &*plan, competencia, &mapa)
        .map_err(|e| e.to_string())?;

    es(writeln!(
        saida,
        "linhas lidas={} subtotais={}",
        aba.linhas.len(),
        aba.subtotais.len()
    ))?;
    es(writeln!(saida, "previa: {}", previa.contagens()))?;
    if o.detalhar {
        for n in &previa.novos {
            es(writeln!(
                saida,
                "novo   l{:<3} {} | {} | {}",
                n.linha,
                n.recorrente.grupo,
                n.recorrente.nome,
                reais(n.recorrente.primeira_versao.valor)
            ))?;
        }
        for e in &previa.existentes {
            es(writeln!(saida, "existe l{:<3} {}", e.linha, e.nome))?;
        }
        for a in &previa.atualizaveis {
            es(writeln!(
                saida,
                "atualiza l{:<3} {} | {} -> {}",
                a.linha,
                a.nome,
                reais(a.valor_atual),
                reais(Some(a.valor_novo))
            ))?;
        }
        for c in &previa.conflitos {
            es(writeln!(
                saida,
                "conflito l{:<3} {} | {}",
                c.linha, c.nome, c.motivo
            ))?;
        }
        for i in &previa.ignorados {
            es(writeln!(saida, "ignorado l{:<3} {}", i.linha, i.motivo))?;
        }
        for f in &previa.faltantes {
            es(writeln!(
                saida,
                "falta  l{:<3} {} | {}",
                f.linha, f.nome, f.o_que
            ))?;
        }
    }
    if !o.confirmar {
        return es(writeln!(saida, "nada foi gravado (use --confirmar)"));
    }
    let c = importacao::confirmar(&previa, &mut *plan).map_err(|e| e.to_string())?;
    es(writeln!(
        saida,
        "gravado: grupos={} criados={} atualizados={} pagos_informados={}",
        c.grupos, c.criados, c.atualizados, c.pagos_informados
    ))
}

fn achar_ocultos(o: &Opcoes, abrir: Abrir<'_>, saida: &mut dyn Write) -> Result<(), String> {
    let cofre = exige(&o.inventario, "--inventario")?;
    let mapa = mapa_de(o)?;
    let conn = abrir_inventario(cofre)?;
    let mut plan = abrir(cofre)?;
    // A competência do início do ciclo atual (corte no dia 20): a divergência vale a partir
    // da seguinte, que é a do ciclo do próximo salário.
    let corte = sbf_planejamento::DiaDeCorte::novo(20).expect("20 é um dia de corte válido");
    let ciclo = sbf_planejamento::Ciclo::da_data(Data::hoje_em_sao_paulo(), corte);
    let competencia = Competencia::de_data(ciclo.inicio);
    let previa =
        ocultos::previa_de_ocultos(&conn, &*plan, &mapa, &ConfigOcultos::default(), competencia)
            .map_err(|e| e.to_string())?;
    es(writeln!(saida, "previa: {}", previa.contagens()))?;
    if o.detalhar {
        for a in &previa.novos {
            es(writeln!(
                saida,
                "novo   {} | {} | dia {} | {} ciclos",
                a.fornecedor,
                reais(Some(a.mediana)),
                a.dia,
                a.ciclos_presentes
            ))?;
        }
        for d in &previa.divergentes {
            es(writeln!(
                saida,
                "diverge {} | {} -> {} a partir de {}",
                d.nome,
                reais(Some(d.valor_atual)),
                reais(Some(d.valor_pluggy)),
                d.desde
            ))?;
        }
    }
    if !o.confirmar {
        return es(writeln!(saida, "nada foi gravado (use --confirmar)"));
    }
    let c = ocultos::confirmar_ocultos(&previa, &mut *plan).map_err(|e| e.to_string())?;
    es(writeln!(
        saida,
        "gravado: criados={} editados={}",
        c.criados, c.editados
    ))
}

fn conciliar(o: &Opcoes, abrir: Abrir<'_>, saida: &mut dyn Write) -> Result<(), String> {
    let cofre = exige(&o.inventario, "--inventario")?;
    let mapa = mapa_de(o)?;
    let corte = sbf_planejamento::DiaDeCorte::PADRAO;
    let ciclo = match &o.ciclo {
        Some(c) => {
            sbf_planejamento::Ciclo::do_nome(c, corte).ok_or("o ciclo tem de ser AAAA-MM")?
        }
        None => sbf_planejamento::Ciclo::atual(corte),
    };
    let conn = abrir_inventario(cofre)?;
    let mut plan = abrir(cofre)?;
    let hoje = Data::hoje_em_sao_paulo();
    let periodo = Periodo::Ciclo(ciclo);
    let cfg = ConfigConciliacao::default();
    if o.detalhar {
        let sugestoes = conciliacao::sugerir(&conn, &*plan, &periodo, &mapa, &cfg, hoje)
            .map_err(|e| e.to_string())?;
        for s in &sugestoes {
            es(writeln!(
                saida,
                "sugere {} -> {} | {} em {}",
                s.ocorrencia,
                s.movimento,
                reais(Some(s.valor)),
                s.data.dia
            ))?;
        }
    }
    let n = conciliacao::conciliar(&conn, &mut *plan, &periodo, &mapa, &cfg, hoje)
        .map_err(|e| e.to_string())?;
    es(writeln!(
        saida,
        "sugestoes gravadas={n} (nenhuma confirmada)"
    ))
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planilha::falso::Falso;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn opcoes_sao_lidas_e_as_erradas_recusadas() {
        let o = ler_opcoes(&v(&[
            "--inventario",
            "/c",
            "--aba",
            "2026.11",
            "--confirmar",
        ]))
        .expect("opcoes");
        assert_eq!(o.inventario, Some(PathBuf::from("/c")));
        assert!(o.confirmar && !o.detalhar);
        assert!(ler_opcoes(&v(&["--nada"])).is_err());
        assert!(ler_opcoes(&v(&["--aba"])).is_err());
    }

    #[test]
    fn reais_formata_milhar_e_sinal() {
        assert_eq!(reais(Some(Centavos(123_456))), "R$ 1.234,56");
        assert_eq!(reais(Some(Centavos(-5))), "-R$ 0,05");
        assert_eq!(reais(None), "sem valor");
    }

    #[test]
    fn sem_o_banco_ligado_o_comando_falha_sem_gravar() {
        let mut saida = Vec::new();
        let args = v(&["conciliar", "--inventario", "/nao/existe"]);
        let codigo = rodar_com(&args, &abrir_planejamento, &mut saida);
        assert_eq!(codigo, ExitCode::from(1));
        assert!(saida.is_empty());
    }

    #[test]
    fn ocultos_mostra_so_contagens_e_so_grava_com_confirmar() {
        let cofre = tempfile::tempdir().expect("cofre");
        let c = rusqlite::Connection::open(cofre.path().join("inventario.sqlite")).expect("banco");
        c.execute_batch(
            "CREATE TABLE registro (id INTEGER PRIMARY KEY, external_ref TEXT, tipo TEXT, data TEXT,
               ciclo_id TEXT, valor_centavos INTEGER, fornecedor_normalizado TEXT);",
        )
        .expect("esquema");
        for mes in 1..=6 {
            c.execute(
                "INSERT INTO registro (external_ref, tipo, data, ciclo_id, valor_centavos, fornecedor_normalizado)
                 VALUES (?1, 'compra', ?2, ?3, -3990, 'STREAMING FICTICIO')",
                rusqlite::params![format!("pluggy:txn_{mes}"), format!("2026-{mes:02}-25"), format!("2026-{mes:02}")],
            )
            .expect("insert");
        }
        drop(c);
        let abrir =
            |_: &Path| -> Result<Box<dyn Planejamento>, String> { Ok(Box::new(Falso::default())) };
        let cofre_s = cofre.path().to_string_lossy().into_owned();

        let mut saida = Vec::new();
        let codigo = rodar_com(
            &v(&["ocultos", "--inventario", &cofre_s]),
            &abrir,
            &mut saida,
        );
        assert_eq!(codigo, ExitCode::SUCCESS);
        let texto = String::from_utf8(saida).expect("utf8");
        assert!(texto.contains("novos=1"), "{texto}");
        assert!(
            !texto.contains("STREAMING"),
            "a saída padrão não mostra nomes"
        );
        assert!(texto.contains("nada foi gravado"));

        let mut saida = Vec::new();
        rodar_com(
            &v(&["ocultos", "--inventario", &cofre_s, "--detalhar"]),
            &abrir,
            &mut saida,
        );
        assert!(
            String::from_utf8(saida)
                .expect("utf8")
                .contains("STREAMING FICTICIO")
        );
    }
}
