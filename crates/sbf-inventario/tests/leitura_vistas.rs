//! INV-03: cada uma das 12 vistas devolve as mesmas linhas que a consulta do Datasette
//! correspondente, sobre o banco de teste. O teste roda a SQL original do
//! `workers/inventario/datasette/metadata.json` e compara.

mod comum;

use std::collections::BTreeMap;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, ToSql};
use sbf_inventario::consulta::{Celula, Filtro, Fonte, PedidoDeTabela};
use sbf_inventario::vistas::{self, Contexto};

fn sql_do_datasette(consulta: &str) -> String {
    let caminho = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../workers/inventario/datasette/metadata.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(caminho).expect("metadata.json"))
            .expect("json");
    json["databases"]["inventario"]["queries"][consulta]["sql"]
        .as_str()
        .unwrap_or_else(|| panic!("consulta {consulta} não existe"))
        .to_owned()
}

/// Roda a SQL original com parâmetros nomeados e devolve cada linha como texto.
fn linhas_originais(
    conn: &Connection,
    consulta: &str,
    params: &[(&str, &str)],
) -> Vec<Vec<String>> {
    let sql = sql_do_datasette(consulta);
    let mut instrucao = conn.prepare(&sql).expect("preparar SQL original");
    let nomes: Vec<String> = (0..instrucao.parameter_count())
        .filter_map(|i| instrucao.parameter_name(i + 1).map(str::to_owned))
        .collect();
    let valores: Vec<(String, String)> = nomes
        .iter()
        .map(|n| {
            let valor = params
                .iter()
                .find(|(k, _)| format!(":{k}") == *n)
                .map_or("", |(_, v)| *v);
            (n.clone(), valor.to_owned())
        })
        .collect();
    let refs: Vec<(&str, &dyn ToSql)> = valores
        .iter()
        .map(|(n, v)| (n.as_str(), v as &dyn ToSql))
        .collect();
    let n_colunas = instrucao.column_count();
    let mut linhas = instrucao
        .query(refs.as_slice())
        .expect("rodar SQL original");
    let mut saida = Vec::new();
    while let Some(l) = linhas.next().expect("linha") {
        saida.push(
            (0..n_colunas)
                .map(|i| match l.get_ref(i).expect("coluna") {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(f) => format!("{f:?}"),
                    ValueRef::Text(t) | ValueRef::Blob(t) => {
                        String::from_utf8_lossy(t).into_owned()
                    }
                })
                .collect(),
        );
    }
    saida
}

/// Centavos como o `printf('%.2f', centavos / 100.0)` do SQLite.
fn reais(centavos: i64) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    format!(
        "{sinal}{}.{:02}",
        centavos.abs() / 100,
        centavos.abs() % 100
    )
}

fn reais_opt(c: Option<i64>) -> String {
    c.map_or(String::new(), reais)
}

fn t(o: &Option<String>) -> String {
    o.clone().unwrap_or_default()
}

fn ordenadas(mut linhas: Vec<Vec<String>>) -> Vec<Vec<String>> {
    linhas.sort();
    linhas
}

fn confere(consulta: &str, esperado: Vec<Vec<String>>, obtido: Vec<Vec<String>>) {
    assert!(
        !esperado.is_empty(),
        "a fixture precisa ter linhas para {consulta}"
    );
    assert_eq!(
        ordenadas(obtido),
        ordenadas(esperado),
        "a vista {consulta} difere da SQL do Datasette"
    );
}

fn abrir() -> (comum::BancoDeTeste, Connection) {
    let banco = comum::banco_de_teste();
    let conn = Connection::open(&banco.caminho).expect("abrir");
    (banco, conn)
}

#[test]
fn por_ciclo_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for desde in ["", "2026-09"] {
        let esperado = linhas_originais(&conn, "por_ciclo", &[("desde", desde)]);
        let obtido = vistas::por_ciclo_datasette(&ctx, desde)
            .expect("vista")
            .into_iter()
            .map(|l| {
                vec![
                    l.ciclo,
                    reais(l.entrou),
                    reais(l.saiu),
                    reais(l.movido_entre_contas),
                    l.registros.to_string(),
                ]
            })
            .collect();
        confere("por_ciclo", esperado, obtido);
    }
}

#[test]
fn fornecedores_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for (de, ate) in [("", ""), ("2026-09", "2026-10")] {
        let esperado: Vec<Vec<String>> =
            linhas_originais(&conn, "fornecedores", &[("de", de), ("ate", ate)])
                .into_iter()
                .map(|mut l| {
                    // group_concat(DISTINCT) não garante ordem: compara os ciclos ordenados.
                    let mut ciclos: Vec<&str> = l[6].split(',').collect();
                    ciclos.sort_unstable();
                    l[6] = ciclos.join(",");
                    l
                })
                .collect();
        let obtido = vistas::fornecedores_datasette(&ctx, de, ate)
            .expect("vista")
            .into_iter()
            .map(|l| {
                let mut ciclos = l.ciclos.clone();
                ciclos.sort();
                vec![
                    t(&l.fornecedor),
                    l.tipo,
                    l.vezes.to_string(),
                    reais(l.saiu),
                    l.primeira,
                    l.ultima,
                    ciclos.join(","),
                ]
            })
            .collect();
        confere("fornecedores", esperado, obtido);
    }
}

#[test]
fn movimento_entre_contas_nao_aparece_em_fornecedores() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let linhas = vistas::fornecedores_datasette(&ctx, "", "").expect("vista");
    assert!(!linhas.is_empty());
    for l in &linhas {
        assert!(
            !sbf_inventario::movimento::e_movimento_entre_contas(&l.tipo),
            "{} é movimento entre contas e não pode contar como gasto",
            l.tipo
        );
    }
}

#[test]
fn registros_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for (busca, ciclo) in [
        ("", ""),
        ("Energia", ""),
        ("", "2026-10"),
        ("mercado", "2026-09"),
    ] {
        let esperado = linhas_originais(&conn, "registros", &[("busca", busca), ("ciclo", ciclo)]);
        if esperado.is_empty() {
            assert!(
                vistas::registros_datasette(&ctx, busca, ciclo)
                    .expect("vista")
                    .is_empty()
            );
            continue;
        }
        let obtido = vistas::registros_datasette(&ctx, busca, ciclo)
            .expect("vista")
            .into_iter()
            .map(|l| {
                vec![
                    l.data,
                    t(&l.ciclo),
                    reais(l.valor),
                    l.tipo,
                    t(&l.fornecedor),
                    t(&l.contraparte),
                    t(&l.data_compra),
                    t(&l.descricao),
                    t(&l.categoria),
                    t(&l.conta),
                    l.estado,
                    l.fonte,
                ]
            })
            .collect();
        confere("registros", esperado, obtido);
    }
}

fn colunas_de_documento(l: vistas::LinhaDeAchado) -> Vec<String> {
    vec![
        t(&l.tipo),
        t(&l.data_chave),
        t(&l.data_chave_tipo),
        t(&l.ciclo),
        reais_opt(l.valor),
        t(&l.fornecedor),
        t(&l.nome),
        t(&l.onde),
        l.origens.to_string(),
        t(&l.no_cofre),
        l.confianca.map_or(String::new(), |c| format!("{c:?}")),
    ]
}

#[test]
fn documentos_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for (tipo, ciclo) in [("", ""), ("fatura", ""), ("", "2026-10")] {
        let esperado = linhas_originais(&conn, "documentos", &[("tipo", tipo), ("ciclo", ciclo)]);
        let obtido = vistas::documentos_datasette(&ctx, tipo, ciclo)
            .expect("vista")
            .into_iter()
            .map(colunas_de_documento)
            .collect();
        confere("documentos", esperado, obtido);
    }
}

#[test]
fn duvidas_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado = linhas_originais(&conn, "duvidas", &[]);
    let obtido = vistas::duvidas_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                t(&l.extensao),
                t(&l.notas),
                t(&l.nome),
                t(&l.onde),
                t(&l.no_cofre),
            ]
        })
        .collect();
    confere("duvidas", esperado, obtido);
}

#[test]
fn emails_por_remetente_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado = linhas_originais(&conn, "emails_por_remetente", &[]);
    let obtido = vistas::emails_por_remetente_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                t(&l.dominio),
                l.emails.to_string(),
                t(&l.ultimo),
                t(&l.ultimo_assunto),
            ]
        })
        .collect();
    confere("emails_por_remetente", esperado, obtido);
}

#[test]
fn emails_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for busca in ["", "energia", "Promo"] {
        let esperado = linhas_originais(&conn, "emails", &[("busca", busca)]);
        let obtido = vistas::emails_datasette(&ctx, busca)
            .expect("vista")
            .into_iter()
            .map(|l| {
                vec![
                    t(&l.data),
                    t(&l.de),
                    t(&l.assunto),
                    l.anexos.to_string(),
                    l.estado,
                    t(&l.eml_no_cofre),
                ]
            })
            .collect();
        confere("emails", esperado, obtido);
    }
}

#[test]
fn arquivos_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    for busca in ["", "energia", "e2/misc"] {
        let esperado = linhas_originais(&conn, "arquivos", &[("busca", busca)]);
        let obtido = vistas::arquivos_datasette(&ctx, busca)
            .expect("vista")
            .into_iter()
            .map(|l| {
                vec![
                    l.nome,
                    t(&l.caminho),
                    l.bytes.map_or(String::new(), |b| b.to_string()),
                    t(&l.data_arquivo),
                    t(&l.camada),
                    t(&l.estado),
                    t(&l.tipo),
                    t(&l.no_cofre),
                ]
            })
            .collect();
        confere("arquivos", esperado, obtido);
    }
}

#[test]
fn pastas_pessoais_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado = linhas_originais(&conn, "pastas_pessoais", &[]);
    let obtido = vistas::pastas_pessoais_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                t(&l.pasta),
                l.achados.to_string(),
                l.documentos.to_string(),
                l.descartados.to_string(),
            ]
        })
        .collect();
    confere("pastas_pessoais", esperado, obtido);
}

#[test]
fn varreduras_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado = linhas_originais(&conn, "varreduras", &[]);
    let obtido: Vec<Vec<String>> = vistas::varreduras_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                l.id.to_string(),
                l.fonte_id,
                l.etapa,
                l.status,
                l.iniciada_em,
                t(&l.concluida_em),
                l.vistos.to_string(),
                l.novos.to_string(),
                l.repetidos.to_string(),
                l.excluidos.to_string(),
                l.erros.to_string(),
                l.escopo,
            ]
        })
        .collect();
    // `varreduras` tem ordem definida (id decrescente): compara a ordem também.
    assert_eq!(obtido, esperado);
}

#[test]
fn faturas_conferencia_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado = linhas_originais(&conn, "faturas_conferencia", &[]);
    let obtido = vistas::faturas_conferencia_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                t(&l.conta),
                l.vencimento,
                reais(l.total),
                l.transacoes.to_string(),
                reais(l.soma_transacoes),
                reais(l.diferenca),
                l.situacao,
            ]
        })
        .collect::<Vec<_>>();
    confere("faturas_conferencia", esperado.clone(), obtido.clone());
    let situacoes: Vec<&str> = obtido.iter().map(|l| l[6].as_str()).collect();
    assert!(situacoes.contains(&"confere") && situacoes.contains(&"não confere"));
}

#[test]
fn contas_buracos_igual_ao_datasette() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let esperado: Vec<Vec<String>> = linhas_originais(&conn, "contas_buracos", &[])
        .into_iter()
        .collect();
    let obtido = vistas::contas_buracos_datasette(&ctx)
        .expect("vista")
        .into_iter()
        .map(|l| {
            vec![
                t(&l.conta),
                l.transacoes.to_string(),
                l.de,
                l.ate,
                l.meses_sem_nada.join(" "),
            ]
        })
        .collect();
    confere("contas_buracos", esperado, obtido);
}

#[test]
fn doze_consultas_do_datasette_estao_cobertas() {
    let caminho = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../workers/inventario/datasette/metadata.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(caminho).expect("metadata.json"))
            .expect("json");
    let nomes: Vec<&str> = json["databases"]["inventario"]["queries"]
        .as_object()
        .expect("queries")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(nomes.len(), 12, "{nomes:?}");
    // Cada nome tem um teste `<nome>_igual_ao_datasette` neste arquivo.
    let este = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/leitura_vistas.rs"
    ))
    .expect("este arquivo");
    for n in nomes {
        assert!(
            este.contains(&format!("fn {n}_igual_ao_datasette")),
            "falta o teste de {n}"
        );
    }
}

#[test]
fn toda_celula_de_nome_e_link_e_todo_total_tem_itens() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    let tabela = vistas::tabela_por_ciclo(
        &vistas::por_ciclo(&ctx, &vistas::Selecao::nova()).expect("vista"),
    );
    assert!(!tabela.linhas.is_empty());
    for linha in &tabela.linhas {
        assert!(matches!(linha.celulas[0], Celula::Link { .. }));
        for c in &linha.celulas[1..4] {
            let Celula::Valor { fonte, itens, .. } = c else {
                panic!("esperava Valor")
            };
            assert_eq!(*fonte, Fonte::Inventario);
            assert!(matches!(
                itens.as_deref(),
                Some(PedidoDeTabela::ItensDoTotal { .. })
            ));
        }
    }
    // As áreas devolvem tabela com todas as linhas e sem filtro.
    for area in [
        sbf_inventario::consulta::Area::Registros,
        sbf_inventario::consulta::Area::Documentos,
        sbf_inventario::consulta::Area::EmDuvida,
        sbf_inventario::consulta::Area::ATriar,
        sbf_inventario::consulta::Area::Fornecedores,
        sbf_inventario::consulta::Area::Faturas,
        sbf_inventario::consulta::Area::ContasLacunas,
        sbf_inventario::consulta::Area::Emails,
        sbf_inventario::consulta::Area::ArquivosEPastas,
        sbf_inventario::consulta::Area::Coletas,
    ] {
        let tabela = vistas::tabela_da_area(&ctx, area, &Filtro::default()).expect("área");
        assert!(!tabela.linhas.is_empty(), "{area:?} vazia");
        for l in &tabela.linhas {
            assert_eq!(l.celulas.len(), tabela.colunas.len(), "{area:?}");
        }
    }
}

#[test]
fn filtros_da_aba_inicio_valem_nas_areas() {
    let (_b, conn) = abrir();
    let ctx = Contexto::novo(&conn).expect("contexto");
    use sbf_inventario::consulta::{Area, Quem};
    let quantas = |area, filtro: Filtro| -> usize {
        vistas::tabela_da_area(&ctx, area, &filtro)
            .expect("área")
            .linhas
            .len()
    };
    let todos = quantas(Area::Registros, Filtro::default());
    assert!(
        quantas(
            Area::Registros,
            Filtro {
                texto: Some("energia".into()),
                ..Filtro::default()
            }
        ) < todos
    );
    assert!(
        quantas(
            Area::Registros,
            Filtro {
                ciclos: Some(("2026-10".into(), "2026-10".into())),
                ..Filtro::default()
            }
        ) < todos
    );
    assert!(
        quantas(
            Area::Registros,
            Filtro {
                tipos: vec!["compra".into()],
                ..Filtro::default()
            }
        ) < todos
    );
    assert!(
        quantas(
            Area::Registros,
            Filtro {
                fontes: vec!["gmail".into()],
                ..Filtro::default()
            }
        ) == 1
    );
    assert!(
        quantas(
            Area::Registros,
            Filtro {
                quem_decidiu: Some(Quem::Usuario),
                ..Filtro::default()
            }
        ) >= 2
    );
    // Achados por estado, tipo e quem decidiu.
    assert_eq!(quantas(Area::ATriar, Filtro::default()), 2);
    assert_eq!(quantas(Area::EmDuvida, Filtro::default()), 2);
    assert_eq!(
        quantas(
            Area::EmDuvida,
            Filtro {
                quem_decidiu: Some(Quem::Usuario),
                ..Filtro::default()
            }
        ),
        1
    );
    assert_eq!(
        quantas(
            Area::EmDuvida,
            Filtro {
                quem_decidiu: Some(Quem::Automatico("regras@2".into())),
                ..Filtro::default()
            }
        ),
        1
    );
    assert_eq!(
        quantas(
            Area::Documentos,
            Filtro {
                tipos: vec!["boleto".into()],
                ..Filtro::default()
            }
        ),
        1
    );
    assert_eq!(
        quantas(
            Area::Documentos,
            Filtro {
                fontes: vec!["onedrive".into()],
                ..Filtro::default()
            }
        ),
        1
    );
    assert_eq!(
        quantas(
            Area::Documentos,
            Filtro {
                texto: Some("energia-set".into()),
                ..Filtro::default()
            }
        ),
        1
    );
    let _: BTreeMap<u8, u8> = BTreeMap::new();
}
