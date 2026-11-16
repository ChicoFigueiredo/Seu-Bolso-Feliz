//! `sbf planejamento`: importação da planilha, itens ocultos e conciliação (spec 11).
//!
//! ```text
//! sbf planejamento importar --inventario <cofre> --planilha <arq> --aba AAAA.MM [--mapa <arq>] [--confirmar] [--detalhar]
//! sbf planejamento ocultos  --inventario <cofre> [--mapa <arq>] [--confirmar] [--detalhar]
//! sbf planejamento conciliar --inventario <cofre> [--ciclo AAAA-MM] [--mapa <arq>] [--detalhar]
//! sbf planejamento contracheque --inventario <cofre> (--arquivo <pdf|txt> | --achado <sha256>) [--recorrente <id>] [--confirmar] [--detalhar]
//! ```
//!
//! A saída padrão mostra só contagens. `--detalhar` mostra a prévia linha a linha no
//! terminal (nunca em log). Sem `--confirmar`, nada é gravado. O banco do planejamento é
//! `<cofre>/planejamento.sqlite`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Autor, Centavos, Competencia, Data, Periodo, Planejamento,
    TipoDeDocumento,
};
use sbf_planilha::conciliacao::{self, ConfigConciliacao};
use sbf_planilha::contracheque;
use sbf_planilha::importacao;
use sbf_planilha::leitura;
use sbf_planilha::mapa::Mapa;
use sbf_planilha::ocultos::{self, ConfigOcultos};
use sbf_reconhecimento::{UsoDeRubricas, salario};

const USO: &str = "uso: sbf planejamento importar --inventario <cofre> --planilha <arq> --aba AAAA.MM [--mapa <arq>] [--confirmar] [--detalhar] | sbf planejamento ocultos --inventario <cofre> [--mapa <arq>] [--confirmar] [--detalhar] | sbf planejamento conciliar --inventario <cofre> [--ciclo AAAA-MM] [--mapa <arq>] [--detalhar] | sbf planejamento contracheque --inventario <cofre> (--arquivo <pdf|txt> | --achado <sha256>) [--recorrente <id>] [--confirmar] [--detalhar] | sbf planejamento acompanhar --inventario <cofre> --ciclo AAAA-MM";

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
    if args.first().map(String::as_str) == Some("acompanhar") {
        return acompanhar(&args[1..]);
    }
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
    arquivo: Option<PathBuf>,
    achado: Option<String>,
    recorrente: Option<String>,
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
            "--arquivo" => o.arquivo = Some(valor("--arquivo")?.into()),
            "--achado" => o.achado = Some(valor("--achado")?),
            "--recorrente" => o.recorrente = Some(valor("--recorrente")?),
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
        "contracheque" => contracheque(&opcoes, abrir, saida),
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

/// O texto do contracheque: de um PDF (`pdftotext -layout`, sem shell), de um `.txt` ou do
/// achado do inventário (só leitura). Devolve o texto, o sha256 e o nome para mostrar.
fn texto_do_contracheque(o: &Opcoes) -> Result<(String, String, String), String> {
    match (&o.arquivo, &o.achado) {
        (Some(_), Some(_)) | (None, None) => {
            Err("use --arquivo ou --achado (um dos dois)".to_owned())
        }
        (Some(arquivo), None) => {
            let sha = leitura::sha256_do_arquivo(arquivo).map_err(|e| e.to_string())?;
            let nome = arquivo.file_name().map_or_else(
                || "contracheque".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            let e_pdf = arquivo
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            let texto = if e_pdf {
                let saida = std::process::Command::new("pdftotext")
                    .arg("-layout")
                    .arg(arquivo)
                    .arg("-")
                    .output()
                    .map_err(|e| format!("não deu para rodar o pdftotext: {e}"))?;
                if !saida.status.success() {
                    return Err("o pdftotext não leu o PDF".to_owned());
                }
                String::from_utf8_lossy(&saida.stdout).into_owned()
            } else {
                std::fs::read_to_string(arquivo)
                    .map_err(|e| format!("não deu para ler o arquivo: {e}"))?
            };
            Ok((texto, sha, nome))
        }
        (None, Some(achado)) => {
            let cofre = exige(&o.inventario, "--inventario")?;
            let conn = abrir_inventario(cofre)?;
            let texto: String = conn
                .query_row(
                    "SELECT conteudo FROM texto WHERE content_hash = ?1",
                    [achado],
                    |l| l.get(0),
                )
                .map_err(|_| "o achado não tem texto extraído no inventário".to_owned())?;
            Ok((
                texto,
                achado.clone(),
                format!("contracheque {}", &achado[..achado.len().min(8)]),
            ))
        }
    }
}

fn contracheque(o: &Opcoes, abrir: Abrir<'_>, saida: &mut dyn Write) -> Result<(), String> {
    let cofre = exige(&o.inventario, "--inventario")?;
    let (texto, sha, nome) = texto_do_contracheque(o)?;
    let mut plan = abrir(cofre)?;
    contracheque_com(o, &texto, &sha, &nome, &mut *plan, saida)
}

fn contracheque_com(
    o: &Opcoes,
    texto: &str,
    sha: &str,
    nome: &str,
    plan: &mut dyn Planejamento,
    saida: &mut dyn Write,
) -> Result<(), String> {
    let c = contracheque::ler(texto).map_err(|e| e.to_string())?;
    let recorrente = salario::recorrente_do_salario(&*plan, o.recorrente.as_deref())
        .map_err(|e| e.to_string())?;
    let uso = UsoDeRubricas {
        recorrente: recorrente.id.clone(),
        nome: recorrente.nome.clone(),
        documento: sha.to_owned(),
        digitado_no_mes: salario::valor_digitado(&recorrente, c.pagamento),
        digitado_depois: salario::valor_digitado(&recorrente, c.pagamento.somar_meses(1)),
        contracheque: c.clone(),
    };
    let marcacao = uso.eventuais_iniciais();
    let composicoes = uso.composicoes(&marcacao);
    let digitado = uso.digitado_das_fixas(&marcacao);
    let eventuais = c
        .eventuais
        .iter()
        .zip(&c.informativas)
        .filter(|(e, i)| **e && !**i)
        .count();
    let informativas = c.informativas.iter().filter(|i| **i).count();

    es(writeln!(
        saida,
        "contracheque: competencia={} rubricas={} bruto={} descontos={} liquido={} {}",
        c.pagamento,
        c.rubricas.len(),
        reais(c.bruto),
        reais(c.descontos),
        reais(c.liquido),
        if c.conferido {
            "conferido"
        } else {
            "NAO CONFERIDO (a soma das rubricas nao bate com o liquido)"
        }
    ))?;
    if let Some(d) = c.credito {
        es(writeln!(
            saida,
            "credito em {:02}/{:02}/{:04}",
            d.dia, d.mes, d.ano
        ))?;
    }
    if eventuais > 0 || informativas > 0 {
        es(writeln!(
            saida,
            "eventuais={eventuais} informativas={informativas} fixas={} liquido do mes={} liquido das fixas={}",
            c.rubricas.len() - eventuais - informativas,
            reais(Some(uso.liquido_do_mes())),
            reais(Some(uso.liquido_das_fixas(&marcacao)))
        ))?;
    }
    if o.detalhar {
        for a in &c.avisos {
            es(writeln!(saida, "aviso: {a}"))?;
        }
    }
    match uso.divergencia(&marcacao) {
        Some(dif) => es(writeln!(
            saida,
            "salario digitado={} diverge das rubricas em {}",
            reais(digitado),
            reais(Some(dif))
        ))?,
        None if digitado.is_some() => {
            es(writeln!(saida, "salario digitado confere com as rubricas"))?
        }
        None => es(writeln!(saida, "salario sem valor digitado"))?,
    }
    for k in c.consignacoes_com_prazo() {
        es(writeln!(
            saida,
            "consignacao {}: inicio {:02}/{:04}, prazo {}, termina {:02}/{:04}",
            k.nome, k.inicio.mes, k.inicio.ano, k.prazo, k.termina.mes, k.termina.ano
        ))?;
    }
    for (r, parcela) in c.rubricas.iter().zip(&c.parcelas_atuais) {
        if let Some(n) = parcela {
            es(writeln!(
                saida,
                "consignacao {}: parcela {n} (fim desconhecido)",
                r.nome
            ))?;
        }
    }
    if o.detalhar {
        for (i, r) in c.rubricas.iter().enumerate() {
            let grupo = if c.informativas[i] {
                "informativa"
            } else if c.eventuais[i] {
                "eventual"
            } else {
                "fixa"
            };
            es(writeln!(
                saida,
                "rubrica {} | {} | {:?} | {} | {grupo}",
                r.codigo.as_deref().unwrap_or("-"),
                r.nome,
                r.tipo,
                reais(Some(r.valor))
            ))?;
        }
    }
    if !o.confirmar {
        return es(writeln!(saida, "nada foi gravado (use --confirmar)"));
    }
    let mut igual = true;
    for composicao in composicoes {
        if salario::composicao_ja_vigente(&*plan, &composicao).map_err(|e| e.to_string())? {
            continue;
        }
        igual = false;
        // A versão lida logo antes de cada composição.
        let versao_base = plan
            .recorrente(&recorrente.id)
            .map_err(|e| e.to_string())?
            .map_or(recorrente.versao_do_registro, |r| r.versao_do_registro);
        plan.aplicar(
            Alteracao::DefinirComposicao {
                composicao,
                versao_base,
            },
            Autor::Importacao,
        )
        .map_err(|e| e.to_string())?;
    }
    let alvo = AlvoDoDocumento::Recorrente {
        id: recorrente.id.clone(),
    };
    let ja_vinculado = salario::ja_vinculado(&*plan, &alvo, sha).map_err(|e| e.to_string())?;
    if !ja_vinculado {
        plan.aplicar(
            Alteracao::VincularDocumento {
                alvo,
                sha256: sha.to_owned(),
                tipo_documento: TipoDeDocumento::Contracheque,
                nome: nome.to_owned(),
                data: Some(c.pagamento.dia(1)),
            },
            Autor::Importacao,
        )
        .map_err(|e| e.to_string())?;
    }
    es(writeln!(
        saida,
        "gravado: composicao={} documento={}",
        if igual { "ja existia" } else { "definida" },
        if ja_vinculado {
            "ja vinculado"
        } else {
            "vinculado"
        }
    ))
}

/// `sbf planejamento acompanhar --inventario <cofre> --ciclo AAAA-MM`: define o início do
/// acompanhamento. Antes dele, ocorrência sem pagamento informado não vira "em atraso" (a
/// planilha importada é histórico); só o que o usuário adiou continua devido.
fn acompanhar(args: &[String]) -> ExitCode {
    let opcoes = match ler_opcoes(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sbf: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    let feito = (|| -> Result<sbf_planejamento::Data, String> {
        let cofre = exige(&opcoes.inventario, "--inventario")?;
        let texto = exige(&opcoes.ciclo, "--ciclo")?;
        let competencia =
            Competencia::de_texto(texto).ok_or_else(|| format!("competência inválida: {texto}"))?;
        let banco = sbf_planejamento::Banco::abrir(cofre.join("planejamento.sqlite"))
            .map_err(|e| e.to_string())?;
        let mut plan = sbf_planejamento::PlanejamentoSqlite::novo(
            banco,
            Box::new(sbf_planejamento::SemSaldos),
        );
        // O acompanhamento começa no primeiro dia do ciclo (corte no dia 20).
        let corte = sbf_planejamento::DiaDeCorte::novo(20).expect("20 é um dia de corte válido");
        let inicio =
            sbf_planejamento::Ciclo::do_mes(competencia.ano, competencia.mes, corte).inicio;
        plan.definir_inicio_do_acompanhamento(inicio)
            .map_err(|e| e.to_string())?;
        Ok(inicio)
    })();
    match feito {
        Ok(d) => {
            println!("acompanhamento a partir de {d} (ciclo)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(1)
        }
    }
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

    const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");

    const CONTRACHEQUE_2026: &str =
        include_str!("../../sbf-planilha/tests/contracheque-2026-ficticio.txt");

    fn falso_com_salario(valor: i64) -> Falso {
        use sbf_planejamento::*;
        let mut f = Falso::default();
        f.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: "Salário".into(),
                    grupo: "Salário".into(),
                    subgrupo: None,
                    posicao: 0,
                    natureza: Natureza::Entrada,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: Competencia { ano: 2026, mes: 9 },
                        valor: Some(Centavos(valor)),
                        certeza: Certeza::Estimado,
                        fonte: Fonte::Usuario,
                        frequencia: Frequencia::Mensal { dia: 5 },
                        conta: None,
                        centro: None,
                        fornecedor: None,
                        categoria: None,
                        nota: None,
                    },
                },
                origem: None,
            },
            Autor::Usuario,
        )
        .expect("criar");
        f
    }

    fn opcoes_do_contracheque(confirmar: bool) -> Opcoes {
        Opcoes {
            inventario: Some(PathBuf::from("/c")),
            confirmar,
            ..Opcoes::default()
        }
    }

    fn rodar_contracheque(plan: &mut Falso, confirmar: bool) -> String {
        let mut saida = Vec::new();
        contracheque_com(
            &opcoes_do_contracheque(confirmar),
            CONTRACHEQUE,
            "abc123",
            "c.pdf",
            plan,
            &mut saida,
        )
        .expect("contracheque");
        String::from_utf8(saida).expect("utf8")
    }

    #[test]
    fn contracheque_previa_nao_grava_e_nao_mostra_dados_pessoais() {
        let mut plan = falso_com_salario(569_950);
        let t = rodar_contracheque(&mut plan, false);
        assert!(t.contains("rubricas=9"), "{t}");
        assert!(t.contains("conferido"), "{t}");
        assert!(t.contains("salario digitado confere"), "{t}");
        assert!(
            t.contains(
                "consignacao CAIXA CONSIGNACOES SINCR: inicio 10/2024, prazo 96, termina 09/2032"
            ),
            "{t}"
        );
        assert!(t.contains("nada foi gravado"));
        assert!(!t.contains("PESSOA") && !t.contains("c000000"));
        assert!(plan.composicoes.is_empty() && plan.documentos.is_empty());
    }

    #[test]
    fn contracheque_mostra_divergencia_com_o_salario_digitado() {
        let mut plan = falso_com_salario(550_000);
        let t = rodar_contracheque(&mut plan, false);
        assert!(t.contains("diverge das rubricas em R$ 199,50"), "{t}");
    }

    #[test]
    fn contracheque_confirmar_define_composicao_e_vincula_uma_vez() {
        let mut plan = falso_com_salario(569_950);
        rodar_contracheque(&mut plan, true);
        assert_eq!(plan.composicoes.len(), 1);
        let c = &plan.composicoes[0];
        assert_eq!(c.desde, Competencia { ano: 2026, mes: 10 });
        assert_eq!(c.origem, "contracheque 10/2026");
        assert_eq!(c.documento.as_deref(), Some("abc123"));
        assert_eq!(c.liquido(), Centavos(569_950));
        assert_eq!(plan.documentos.len(), 1);
        assert_eq!(
            plan.documentos[0].tipo,
            sbf_planejamento::TipoDeDocumento::Contracheque
        );
        // De novo: nada novo.
        let antes = plan.aplicadas.len();
        let t = rodar_contracheque(&mut plan, true);
        assert!(
            t.contains("ja existia") && t.contains("ja vinculado"),
            "{t}"
        );
        assert_eq!(plan.aplicadas.len(), antes);
    }

    fn rodar_contracheque_2026(plan: &mut Falso, confirmar: bool, detalhar: bool) -> String {
        let mut saida = Vec::new();
        let o = Opcoes {
            detalhar,
            ..opcoes_do_contracheque(confirmar)
        };
        contracheque_com(&o, CONTRACHEQUE_2026, "abc123", "c.pdf", plan, &mut saida)
            .expect("contracheque");
        String::from_utf8(saida).expect("utf8")
    }

    #[test]
    fn contracheque_de_2026_mostra_eventuais_informativa_parcela_e_credito() {
        let mut plan = falso_com_salario(954_885);
        let t = rodar_contracheque_2026(&mut plan, false, false);
        assert!(t.contains("rubricas=33"), "{t}");
        assert!(t.contains("credito em 13/10/2026"), "{t}");
        assert!(
            t.contains("eventuais=13 informativas=1 fixas=19 liquido do mes=R$ 24.605,22"),
            "{t}"
        );
        assert!(t.contains("salario digitado confere"), "{t}");
        assert!(
            t.contains("consignacao EMPRESTIMO E-CONSIGNADO: parcela 13 (fim desconhecido)"),
            "{t}"
        );
        assert!(!t.contains("termina"), "{t}");
        assert!(!t.contains("FULANO") && !t.contains("c000000"), "{t}");
        assert!(plan.composicoes.is_empty());
        let d = rodar_contracheque_2026(&mut plan, false, true);
        assert!(
            d.contains("ADIANTAMENTO DE SALARIO") && d.contains("| informativa"),
            "{d}"
        );
        assert!(d.contains("| eventual") && d.contains("| fixa"), "{d}");
    }

    #[test]
    fn contracheque_de_2026_confirmar_grava_a_do_mes_e_a_das_fixas() {
        let mut plan = falso_com_salario(954_885);
        rodar_contracheque_2026(&mut plan, true, false);
        assert_eq!(plan.composicoes.len(), 2);
        let desdes: Vec<_> = plan.composicoes.iter().map(|c| c.desde).collect();
        assert!(desdes.contains(&Competencia { ano: 2026, mes: 10 }));
        assert!(desdes.contains(&Competencia { ano: 2026, mes: 11 }));
        let do_mes = plan
            .composicoes
            .iter()
            .find(|c| c.desde.mes == 10)
            .expect("do mês");
        assert_eq!(do_mes.liquido(), Centavos(2_460_522));
        assert_eq!(do_mes.rubricas.len(), 32);
        let fixas = plan
            .composicoes
            .iter()
            .find(|c| c.desde.mes == 11)
            .expect("fixas");
        assert_eq!(fixas.rubricas.len(), 19);
        assert_eq!(fixas.liquido(), Centavos(954_885));
        let t = rodar_contracheque_2026(&mut plan, true, false);
        assert!(t.contains("composicao=ja existia"), "{t}");
        assert_eq!(plan.composicoes.len(), 2);
    }

    #[test]
    fn contracheque_sem_salario_ou_com_dois_pede_recorrente() {
        let mut plan = Falso::default();
        let mut saida = Vec::new();
        let e = contracheque_com(
            &opcoes_do_contracheque(false),
            CONTRACHEQUE,
            "a",
            "n",
            &mut plan,
            &mut saida,
        )
        .expect_err("sem salario");
        assert!(e.contains("--recorrente"));
    }
}
