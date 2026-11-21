//! `sbf reconhecer`: o reconhecimento de um arquivo solto, sem janela (spec 11).
//!
//! ```text
//! sbf reconhecer --inventario <cofre> --arquivo <caminho> [--detalhar] [--com-ia]
//! ```
//!
//! Roda as seis etapas do [`ReconhecedorDoCofre`] e imprime as etapas, o reconhecido e as
//! sugestões. **Não confirma nada**: o planejamento não muda. O arquivo é registrado no
//! inventário do cofre e copiado ao acervo (o original fica onde está) e o texto é gravado,
//! como a janela faz ao soltar.
//!
//! A saída padrão mostra só contagens e motivos genéricos; `--detalhar` mostra os rótulos
//! das sugestões, o resumo da IA e os avisos (no terminal, nunca em log).
//!
//! # A IA
//!
//! Por padrão **não há IA**: a etapa "Perguntar à IA" é pulada. Com `--com-ia` o provedor é
//! montado como na área Triagem (`triagem::montar_motor`: o `claude` em modo headless, a
//! reserva OpenRouter só com chave no chaveiro e ligada em Opções → IA, o nível de confiança
//! "completo"), e então o texto do arquivo **sai da máquina** se os leitores locais não o
//! reconhecerem. Só use `--com-ia` de propósito.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_reconhecimento::{
    Andamento, EstadoDoTexto, Reconhecedor, ReconhecedorDoCofre, Reconhecido, Resultado, Situacao,
    Sugestao,
};
use zeroize::Zeroizing;

const USO: &str =
    "uso: sbf reconhecer --inventario <cofre> --arquivo <caminho> [--detalhar] [--com-ia]";

#[derive(Debug, Default, PartialEq, Eq)]
struct Opcoes {
    inventario: Option<PathBuf>,
    arquivo: Option<PathBuf>,
    detalhar: bool,
    com_ia: bool,
}

fn ler_opcoes(args: &[String]) -> Result<Opcoes, String> {
    let mut o = Opcoes::default();
    let mut i = 0;
    while i < args.len() {
        let valor = |i: usize| {
            args.get(i + 1)
                .map(PathBuf::from)
                .ok_or_else(|| format!("{} pede um valor", args[i]))
        };
        match args[i].as_str() {
            "--inventario" => {
                o.inventario = Some(valor(i)?);
                i += 1;
            }
            "--arquivo" => {
                o.arquivo = Some(valor(i)?);
                i += 1;
            }
            "--detalhar" => o.detalhar = true,
            "--com-ia" => o.com_ia = true,
            outro => return Err(format!("argumento desconhecido: {outro}")),
        }
        i += 1;
    }
    Ok(o)
}

/// Sem chaveiro (não abriu): nenhuma estratégia de senha consegue segredo.
struct SemChaveiro;

impl FonteDeSegredos for SemChaveiro {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
}

pub fn rodar(args: &[String]) -> ExitCode {
    let o = match ler_opcoes(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sbf: {e}\n{USO}");
            return ExitCode::from(2);
        }
    };
    let (Some(cofre), Some(_)) = (&o.inventario, &o.arquivo) else {
        eprintln!("sbf: faltam --inventario e --arquivo.\n{USO}");
        return ExitCode::from(2);
    };
    let chaveiro = sbf_chaveiro::Chaveiro::abrir_na_pasta_padrao().ok();
    let reconhecedor = match montar(cofre, chaveiro, o.com_ia) {
        // `sbf reconhecer` reprocessa o arquivo e o põe na Caixa de entrada.
        Ok(r) => r.com_caixa_de_entrada(sbf_inventario::caixa_de_entrada::Canal::Cli, true),
        Err(e) => {
            eprintln!("sbf: não deu para montar a IA: {e}");
            return ExitCode::from(1);
        }
    };
    rodar_com(&o, &reconhecedor, &mut std::io::stdout())
}

type Segredos = sbf_reconhecimento::Segredos;

/// O reconhecedor do cofre, com o chaveiro (PDF protegido) e, com `com_ia`, a IA montada como
/// na área Triagem (`triagem::montar_motor`). Serve ao CLI e à janela.
///
/// # Errors
/// A IA pedida não montou.
pub fn montar(
    cofre: &std::path::Path,
    chaveiro: Option<sbf_chaveiro::Chaveiro>,
    com_ia: bool,
) -> Result<ReconhecedorDoCofre, String> {
    let segredos: Segredos = match &chaveiro {
        Some(c) => Arc::new(c.clone()),
        None => Arc::new(SemChaveiro),
    };
    // As regras de reconhecimento do cofre (spec 13); sem executor, só as declarativas.
    let mut reconhecedor = ReconhecedorDoCofre::novo(cofre, segredos, None)
        .com_regras(crate::script::executor_de_script().ok());
    if com_ia {
        let insumos = crate::triagem::Insumos::do_ambiente(chaveiro);
        let m = crate::triagem::montar_motor(&cofre.join("inventario.sqlite"), &insumos, false)?;
        reconhecedor = reconhecedor.com_motor_de_ia(sbf_reconhecimento::MotorDeIa {
            fila: m.fila,
            registro: m.registro,
            provedor: m.principal.nome(),
            rastro: Some(m.rastro),
        });
    }
    Ok(reconhecedor)
}

fn rodar_com(o: &Opcoes, r: &dyn Reconhecedor, saida: &mut dyn Write) -> ExitCode {
    let Some(arquivo) = &o.arquivo else {
        return ExitCode::from(2);
    };
    match reconhecer(o, r, arquivo, saida) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(1)
        }
    }
}

fn es(r: std::io::Result<()>) -> Result<(), String> {
    r.map_err(|e| e.to_string())
}

fn situacao_em_texto(s: &Situacao, detalhar: bool) -> String {
    match s {
        Situacao::Rodando => "rodando".to_owned(),
        Situacao::Feita => "feita".to_owned(),
        Situacao::Pulada(m) if detalhar => format!("pulada ({m})"),
        Situacao::Pulada(_) => "pulada".to_owned(),
        Situacao::Falhou(m) if detalhar => format!("falhou ({m})"),
        Situacao::Falhou(_) => "falhou".to_owned(),
    }
}

fn reconhecer(
    o: &Opcoes,
    r: &dyn Reconhecedor,
    arquivo: &Path,
    saida: &mut dyn Write,
) -> Result<(), String> {
    let mut linhas = Vec::new();
    let detalhar = o.detalhar;
    let resultado = r
        .reconhecer(
            arquivo,
            &mut |a: Andamento| {
                if a.situacao != Situacao::Rodando {
                    linhas.push(format!(
                        "etapa {}: {}",
                        a.etapa.rotulo(),
                        situacao_em_texto(&a.situacao, detalhar)
                    ));
                }
            },
            &AtomicBool::new(false),
        )
        .map_err(|e| e.to_string())?;
    for l in &linhas {
        es(writeln!(saida, "{l}"))?;
    }
    imprimir(o, &resultado, saida)?;
    es(writeln!(
        saida,
        "nada foi confirmado no planejamento (as sugestões só valem no clique da janela)"
    ))
}

fn imprimir(o: &Opcoes, r: &Resultado, saida: &mut dyn Write) -> Result<(), String> {
    es(writeln!(
        saida,
        "arquivo: sha256={} {}",
        &r.sha256[..r.sha256.len().min(12)],
        if r.ja_existia {
            "ja existia no inventario"
        } else {
            "novo no inventario"
        }
    ))?;
    let texto = match &r.texto {
        EstadoDoTexto::Extraido => "extraido agora".to_owned(),
        EstadoDoTexto::JaHavia => "o inventario ja tinha".to_owned(),
        EstadoDoTexto::SemTexto(m) if o.detalhar => format!("sem texto ({m})"),
        EstadoDoTexto::SemTexto(_) => "sem texto".to_owned(),
        EstadoDoTexto::Protegido(m) if o.detalhar => format!("protegido ({m})"),
        EstadoDoTexto::Protegido(_) => "protegido".to_owned(),
        EstadoDoTexto::Falhou(m) if o.detalhar => format!("a leitura falhou ({m})"),
        EstadoDoTexto::Falhou(_) => "a leitura falhou".to_owned(),
    };
    es(writeln!(saida, "texto: {texto}"))?;
    match &r.reconhecido {
        Reconhecido::Contracheque {
            contracheque,
            consignacoes,
        } => {
            let c = contracheque;
            let eventuais = c
                .eventuais
                .iter()
                .zip(&c.informativas)
                .filter(|(e, i)| **e && !**i)
                .count();
            es(writeln!(
                saida,
                "reconhecido: contracheque competencia={} rubricas={} eventuais={eventuais} informativas={} consignacoes={} parcelas={}{}",
                c.pagamento,
                c.rubricas.len(),
                c.informativas.iter().filter(|i| **i).count(),
                consignacoes.len(),
                c.parcelas_atuais.iter().flatten().count(),
                c.credito.map_or_else(String::new, |d| format!(
                    " credito={:02}/{:02}/{:04}",
                    d.dia, d.mes, d.ano
                ))
            ))?;
        }
        Reconhecido::PelaIa { resumo, .. } if o.detalhar => {
            es(writeln!(saida, "reconhecido: pela IA, {resumo}"))?;
        }
        Reconhecido::PelaIa { .. } => es(writeln!(saida, "reconhecido: pela IA"))?,
        Reconhecido::Nada { motivo } if o.detalhar => {
            es(writeln!(saida, "reconhecido: nada ({motivo})"))?;
        }
        Reconhecido::Nada { .. } => es(writeln!(saida, "reconhecido: nada"))?,
        Reconhecido::PelaRegra { reconhecimento: r } => {
            let campos: Vec<&str> = r.campos.keys().map(String::as_str).collect();
            es(writeln!(
                saida,
                "reconhecido: padrão {}@{} tipo={} campos={}{}",
                r.regra,
                r.versao,
                r.tipo_documental,
                campos.join(","),
                if r.revisar { " revisar" } else { "" }
            ))?;
            if o.detalhar {
                for m in &r.motivos {
                    es(writeln!(saida, "  motivo: {m}"))?;
                }
            }
        }
        Reconhecido::Ambiguo { candidatas, motivo } => {
            es(writeln!(
                saida,
                "reconhecido: ambiguo entre {} padrões ({motivo})",
                candidatas.len()
            ))?;
        }
    }
    es(writeln!(
        saida,
        "sugestoes={} avisos={}",
        r.sugestoes.len(),
        r.avisos.len()
    ))?;
    if o.detalhar {
        for s in &r.sugestoes {
            es(writeln!(saida, "sugestao: {}", s.rotulo()))?;
            match s {
                Sugestao::UsarRubricas { uso, .. } => {
                    let m = uso.eventuais_iniciais();
                    let c = &uso.contracheque;
                    let eventuais = c
                        .eventuais
                        .iter()
                        .zip(&c.informativas)
                        .filter(|(e, i)| **e && !**i)
                        .count();
                    let informativas = c.informativas.iter().filter(|i| **i).count();
                    es(writeln!(
                        saida,
                        "  composicoes={} eventuais={eventuais} informativas={informativas} fixas desde {:02}/{}",
                        uso.composicoes(&m).len(),
                        uso.fixas_desde(&m).mes,
                        uso.fixas_desde(&m).ano
                    ))?;
                    if let Some(d) = uso.divergencia(&m) {
                        es(writeln!(
                            saida,
                            "  o liquido das fixas diverge do digitado em {} centavos",
                            d.0
                        ))?;
                    }
                }
                Sugestao::Conciliar {
                    conciliacao,
                    provas,
                    ..
                } => {
                    es(writeln!(
                        saida,
                        "  credito em {:02}/{:02}/{:04} provas={provas}/{}",
                        conciliacao.data.dia,
                        conciliacao.data.mes,
                        conciliacao.data.ano,
                        sbf_reconhecimento::PROVAS_POSSIVEIS
                    ))?;
                }
                Sugestao::CriarDescontosEmFolha { descontos, .. } => {
                    let a_ligar = descontos.iter().filter(|d| d.ligar_a.is_some()).count();
                    es(writeln!(
                        saida,
                        "  compromissos={} a_ligar={a_ligar}",
                        descontos.len()
                    ))?;
                }
                Sugestao::DarBaixa { baixas, .. } => {
                    es(writeln!(saida, "  baixas={}", baixas.len()))?;
                }
                Sugestao::Vincular { .. } => {}
                Sugestao::ProvarRegistro {
                    regra,
                    provas,
                    automatica,
                    ..
                }
                | Sugestao::ConciliarOcorrencia {
                    regra,
                    provas,
                    automatica,
                    ..
                }
                | Sugestao::PagarFatura {
                    regra,
                    provas,
                    automatica,
                    ..
                } => {
                    es(writeln!(
                        saida,
                        "  regra={}@{} provas={provas}/{} automatica={automatica}",
                        regra.0,
                        regra.1,
                        sbf_reconhecimento::PROVAS_POSSIVEIS
                    ))?;
                }
            }
        }
        for a in &r.avisos {
            es(writeln!(saida, "aviso: {a}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::{Competencia, TipoDeDocumento};
    use sbf_reconhecimento::{EstadoDoTexto, ReconhecedorFicticio, ocorrencia_do_mes};

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    fn resultado() -> Resultado {
        Resultado {
            sha256: "ab".repeat(32),
            nome: "contracheque.pdf".to_owned(),
            ja_existia: false,
            texto: EstadoDoTexto::Extraido,
            reconhecido: Reconhecido::PelaIa {
                id_da_sugestao: 1,
                resumo: "Boleto · Fornecedor Ficticio".to_owned(),
            },
            sugestoes: vec![Sugestao::Vincular {
                alvo: ocorrencia_do_mes("salario", Competencia { ano: 2026, mes: 10 }),
                rotulo: "Vincular ao Salário · 10/2026".to_owned(),
                tipo: TipoDeDocumento::Contracheque,
                data: None,
                regra: None,
                chave: None,
            }],
            avisos: vec!["aviso ficticio".to_owned()],
            entrada: None,
            leitura: None,
        }
    }

    fn rodar_texto(args: &[&str]) -> String {
        let o = ler_opcoes(&v(args)).expect("opcoes");
        let falso = ReconhecedorFicticio::novo(Ok(resultado()));
        let mut saida = Vec::new();
        let codigo = rodar_com(&o, &falso, &mut saida);
        assert_eq!(codigo, ExitCode::SUCCESS);
        String::from_utf8(saida).expect("utf8")
    }

    #[test]
    fn opcoes_sao_lidas_e_as_erradas_recusadas() {
        let o = ler_opcoes(&v(&[
            "--inventario",
            "/c",
            "--arquivo",
            "/a.pdf",
            "--detalhar",
        ]))
        .expect("opcoes");
        assert_eq!(o.inventario, Some(PathBuf::from("/c")));
        assert!(o.detalhar && !o.com_ia);
        assert!(ler_opcoes(&v(&["--nada"])).is_err());
        assert!(ler_opcoes(&v(&["--arquivo"])).is_err());
    }

    #[test]
    fn a_saida_padrao_mostra_so_contagens_e_nao_confirma_nada() {
        let t = rodar_texto(&["--inventario", "/c", "--arquivo", "/a.pdf"]);
        assert!(t.contains("sugestoes=1 avisos=1"), "{t}");
        assert!(t.contains("etapa Conferir no inventário: feita"), "{t}");
        assert!(t.contains("etapa Perguntar à IA: pulada"), "{t}");
        assert!(t.contains("nada foi confirmado"), "{t}");
        assert!(!t.contains("Fornecedor") && !t.contains("Salário"), "{t}");
    }

    #[test]
    fn detalhar_mostra_os_rotulos_o_resumo_e_os_avisos() {
        let t = rodar_texto(&["--inventario", "/c", "--arquivo", "/a.pdf", "--detalhar"]);
        assert!(t.contains("pela IA, Boleto · Fornecedor Ficticio"), "{t}");
        assert!(t.contains("sugestao: Vincular ao Salário · 10/2026"), "{t}");
        assert!(t.contains("aviso: aviso ficticio"), "{t}");
        assert!(
            t.contains("pulada (os leitores locais reconheceram)"),
            "{t}"
        );
    }
}
