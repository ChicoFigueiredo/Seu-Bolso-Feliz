//! Teste de ouro dos números de Dinheiro (EST-09, EST-10, EST-11 no que é regra): o
//! `sbf-livro` sobre `ouro/livro-ficticio.json` contra as Server Actions do web rodando
//! sobre o mesmo livro gravado num Postgres de teste (`ouro/paineis.ts`, `gerar.sh --banco`),
//! às 10h de 13/11/2026 em São Paulo, corte 20:
//!
//! - `carregarPainelHoje` (`apps/web/src/app/actions/painel-hoje.ts`);
//! - `carregarPainelCentros` (`apps/web/src/app/actions/painel-centros.ts`);
//! - `carregarPainelRecorrentes` (`apps/web/src/app/actions/painel-recorrentes.ts`);
//! - a consulta de referência do gasto pela data da compra (grill Q18).
//!
//! O TS soma reais em ponto flutuante; a comparação é no centavo (`round(x * 100)`).
//! Divergências intencionais, cada uma conferida no seu teste: o ajuste fora do centro e do
//! não alocado (grill Q15) e o pagamento de fatura fora de "Não alocadas" em Para resolver.

use serde_json::Value;

use sbf_livro::centros::painel_centros;
use sbf_livro::hoje::{MotivoParaResolver, OrigemDoCompromisso, painel_hoje};
use sbf_livro::recorrencia::total_mensal_comprometido;
use sbf_livro::{Centavos, Ciclo, Data, LivroCaixa};

fn ler(nome: &str) -> Value {
    let caminho = format!("{}/ouro/{nome}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(caminho).expect("arquivo de ouro")).expect("json")
}

fn livro() -> LivroCaixa {
    serde_json::from_value(ler("livro-ficticio.json")["donos"]["a"]["livro"].clone())
        .expect("livro fictício")
}

fn hoje() -> Data {
    Data::de_iso("2026-11-13").expect("data")
}

/// Reais do TS (ponto flutuante) em centavos.
#[allow(clippy::cast_possible_truncation)]
fn c(v: &Value) -> Centavos {
    let reais = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|t| t.parse().ok()))
        .unwrap_or_else(|| panic!("valor: {v}"));
    Centavos((reais * 100.0).round() as i64)
}

fn c_opt(v: &Value) -> Option<Centavos> {
    (!v.is_null()).then(|| c(v))
}

fn texto(v: &Value) -> String {
    v.as_str()
        .unwrap_or_else(|| panic!("texto: {v}"))
        .to_owned()
}

fn texto_opt(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn data_opt(v: &Value) -> Option<Data> {
    v.as_str().map(|t| Data::de_iso(t).expect("data"))
}

#[test]
fn hoje_bate_com_carregar_painel_hoje() {
    let ts = &ler("fixtures/paineis.json")["hoje"];
    let p = painel_hoje(&livro(), hoje());

    assert_eq!(p.ciclo.inicio.to_string(), texto(&ts["periodo"]["inicio"]));
    assert_eq!(p.ciclo.fim.to_string(), texto(&ts["periodo"]["fim"]));
    assert_eq!(p.ciclo.rotulo(), texto(&ts["periodo"]["rotulo"]));

    assert_eq!(p.disponivel_agora, c(&ts["disponivelAgora"]));
    assert_eq!(p.livre_apos_compromissos, c(&ts["livreAposCompromissos"]));
    assert_eq!(
        p.estimativa,
        ts["estimativaIncompleta"].as_bool().expect("bool")
    );
    assert_eq!(p.divida_total, c(&ts["dividaTotal"]));
    assert_eq!(
        p.compromisso_mensal_de_dividas,
        c(&ts["compromissoMensalDividas"])
    );
    assert_eq!(p.falta_ate_o_fim_do_ciclo, c(&ts["faltaAteFimDoPeriodo"]));

    let mes = &ts["mes"];
    assert_eq!(p.resultado.entrou, c(&mes["entrou"]));
    assert_eq!(p.resultado.saiu, c(&mes["saiu"]));
    assert_eq!(p.resultado.no_cartao, c(&mes["noCartao"]));
    assert_eq!(p.resultado.ainda_vai_sair, c(&mes["aindaVaiSair"]));
    assert_eq!(p.resultado.livre_projetado, c(&mes["livreProjetado"]));
    // EST-06 dentro do resultado: 0,10 + 0,20 entram sem erro de arredondamento.
    assert_eq!(p.resultado.saiu, Centavos(250_915));

    let contas: Vec<_> = p
        .contas
        .iter()
        .map(|x| (x.id.clone(), x.nome.clone(), x.instituicao.clone(), x.saldo))
        .collect();
    let contas_ts: Vec<_> = ts["contas"]
        .as_array()
        .expect("contas")
        .iter()
        .map(|x| {
            (
                texto(&x["id"]),
                texto(&x["nome"]),
                texto_opt(&x["instituicao"]),
                c(&x["saldo"]),
            )
        })
        .collect();
    assert_eq!(contas, contas_ts);

    let cartoes: Vec<_> = p
        .cartoes
        .iter()
        .map(|x| {
            (
                x.conta_id.clone(),
                x.nome.clone(),
                x.fatura_atual,
                x.limite,
                x.limite_livre,
                x.fechamento,
                x.vencimento,
            )
        })
        .collect();
    let cartoes_ts: Vec<_> = ts["cartoes"]
        .as_array()
        .expect("cartões")
        .iter()
        .map(|x| {
            (
                texto(&x["id"]),
                texto(&x["nome"]),
                c(&x["faturaAtual"]),
                c_opt(&x["limiteTotal"]),
                c_opt(&x["limiteLivre"]),
                data_opt(&x["fechamento"]),
                data_opt(&x["vencimento"]),
            )
        })
        .collect();
    assert_eq!(cartoes, cartoes_ts);

    // O TS devolve os 8 primeiros; o Rust devolve todos, na mesma ordem.
    let proximos_ts = ts["proximos"].as_array().expect("próximos");
    assert!(p.compromissos.len() >= proximos_ts.len());
    for (r, t) in p.compromissos.iter().zip(proximos_ts) {
        let origem = match r.origem {
            OrigemDoCompromisso::Movimentacao => "movimentacao",
            OrigemDoCompromisso::Recorrente => "recorrente",
            OrigemDoCompromisso::Fatura => "fatura",
        };
        assert_eq!(
            (
                r.id.clone(),
                r.descricao.clone(),
                r.valor,
                r.data.to_string(),
                origem,
                r.conta.clone(),
                r.vencido,
                r.valor_estimado
            ),
            (
                texto(&t["id"]),
                texto(&t["descricao"]),
                c(&t["valor"]),
                texto(&t["data"]),
                t["origem"].as_str().expect("origem"),
                texto_opt(&t["produto"]),
                t["vencido"].as_bool().expect("bool"),
                t["valorEstimado"].as_bool().expect("bool")
            )
        );
    }

    // Os centros da Hoje: o TS mostra os 6 primeiros ativos por nome. O saldo diverge só
    // no ajuste (grill Q15): o centro Beta tem um rateio de 33,00 num ajuste.
    let centros_ts = ts["centros"].as_array().expect("centros");
    assert_eq!(p.centros.len(), centros_ts.len());
    for (r, t) in p.centros.iter().zip(centros_ts) {
        assert_eq!(r.id, texto(&t["id"]));
        assert_eq!(r.nome, texto(&t["nome"]));
        assert_eq!(r.gasto_no_ciclo, c(&t["gastoNoMes"]), "{}", r.nome);
        let ajuste = if r.nome == "Beta Pessoa" {
            Centavos(3300)
        } else {
            Centavos::ZERO
        };
        assert_eq!(r.saldo, c(&t["saldo"]) + ajuste, "{}", r.nome);
    }
}

#[test]
fn para_resolver_bate_com_as_contagens_do_web() {
    let ts = &ler("fixtures/paineis.json")["hoje"]["pendencias"];
    let p = painel_hoje(&livro(), hoje());
    let rotulos: Vec<_> = p
        .para_resolver
        .iter()
        .map(|i| (i.motivo.rotulo().to_owned(), i.quantidade))
        .collect();
    let rotulos_ts: Vec<_> = ts
        .as_array()
        .expect("pendências")
        .iter()
        .map(|i| {
            let n = u32::try_from(i["quantidade"].as_u64().expect("n")).expect("u32");
            let rotulo = texto(&i["rotulo"]);
            // O web conta o pagamento de fatura (1 no livro) em "Não alocadas", mas ele não
            // aceita rateio: nunca sairia da lista. O Rust não conta.
            let n = if rotulo == "Não alocadas" { n - 1 } else { n };
            (rotulo, n)
        })
        .collect();
    assert_eq!(rotulos, rotulos_ts);
    // Cada contagem abre as movimentações que a somam.
    for item in &p.para_resolver {
        if item.motivo != MotivoParaResolver::DocumentosARevisar {
            assert_eq!(item.movimentacoes.len(), item.quantidade as usize);
        }
    }
}

#[test]
fn centros_e_nao_alocado_batem_com_carregar_painel_centros() {
    let ts = &ler("fixtures/paineis.json")["centros"];
    let livro = livro();
    let ciclo = Ciclo::da_data(hoje(), livro.corte);
    let p = painel_centros(&livro, &ciclo);

    // Não alocado: o ajuste de 33,00 (todo rateado) sai da base no Rust (grill Q15).
    let na = &ts["naoAlocado"];
    assert_eq!(p.nao_alocado.total, c(&na["total"]));
    assert_eq!(
        p.nao_alocado.saidas_do_ciclo + Centavos(3300),
        c(&na["saidasDoPeriodo"])
    );
    assert_eq!(
        u64::from(p.nao_alocado.movimentacoes),
        na["movimentacoes"].as_u64().expect("n")
    );
    let mut pendentes: Vec<_> = p
        .nao_alocado
        .pendentes
        .iter()
        .map(|m| (m.id.clone(), m.valor, m.alocado, m.nao_alocado))
        .collect();
    let mut pendentes_ts: Vec<_> = ts["pendentes"]
        .as_array()
        .expect("pendentes")
        .iter()
        .map(|m| {
            (
                texto(&m["id"]),
                c(&m["valor"]),
                c(&m["alocado"]),
                c(&m["naoAlocado"]),
            )
        })
        .collect();
    pendentes.truncate(12);
    pendentes.sort();
    pendentes_ts.sort();
    assert_eq!(pendentes, pendentes_ts);

    let todos_ts: Vec<&Value> = ts["centrosAtivos"]
        .as_array()
        .expect("ativos")
        .iter()
        .chain(ts["centrosArquivados"].as_array().expect("arquivados"))
        .collect();
    let todos: Vec<_> = p.ativos.iter().chain(&p.arquivados).collect();
    assert_eq!(todos.len(), todos_ts.len());
    for (r, t) in todos.iter().zip(todos_ts) {
        assert_eq!(r.centro.id, texto(&t["id"]));
        let ajuste = if r.centro.nome == "Beta Pessoa" {
            Centavos(3300)
        } else {
            Centavos::ZERO
        };
        assert_eq!(r.saldo.saldo, c(&t["saldo"]) + ajuste, "{}", r.centro.nome);
        assert_eq!(r.saldo.entrou_no_ciclo, c(&t["entrouNoPeriodo"]));
        assert_eq!(
            r.saldo.saiu_no_ciclo + ajuste,
            c(&t["saiuNoPeriodo"]),
            "{}",
            r.centro.nome
        );
        assert_eq!(
            r.saldo.resultado_no_ciclo,
            c(&t["resultadoNoPeriodo"]) + ajuste
        );
        assert_eq!(
            u64::from(r.saldo.rateios),
            t["rateios"].as_u64().expect("n")
        );
    }
}

#[test]
fn total_mensal_dos_recorrentes_bate_com_carregar_painel_recorrentes() {
    let ts = &ler("fixtures/paineis.json")["recorrentes"];
    let total = total_mensal_comprometido(&livro().recorrentes);
    assert_eq!(total.total, c(&ts["totalMensalComprometido"]));
    assert_eq!(
        total.estimativa,
        ts["totalMensalEstimado"].as_bool().expect("bool")
    );
}

#[test]
fn gasto_pela_data_da_compra_bate_com_a_consulta_de_referencia() {
    use std::collections::BTreeMap;

    use sbf_livro::gastos::gastos_pela_data_da_compra;

    let ts = &ler("fixtures/paineis.json")["gastosPorDia"];
    let mut por_dia: BTreeMap<String, (Centavos, u64)> = BTreeMap::new();
    for g in gastos_pela_data_da_compra(&livro()) {
        let e = por_dia.entry(g.data.to_string()).or_default();
        e.0 += g.valor;
        e.1 += 1;
    }
    let por_dia_ts: BTreeMap<String, (Centavos, u64)> = ts
        .as_array()
        .expect("gastos")
        .iter()
        .map(|g| {
            (
                texto(&g["data"]),
                (c(&g["valor"]), g["vezes"].as_u64().expect("n")),
            )
        })
        .collect();
    assert_eq!(por_dia, por_dia_ts);
}

#[cfg(feature = "relatorios")]
#[test]
fn livro_caixa_e_uma_fonte_de_gastos_dos_relatorios() {
    use sbf_inventario::relatorios::FonteDeGastos;
    use sbf_livro::gastos::GastosDoLivro;

    let livro = livro();
    let fonte = GastosDoLivro::novo(&livro);
    let gastos = fonte.gastos("2026-10", "2026-10", &[]).expect("gastos");
    // Ciclo 2026-10 (20/out a 19/nov): todas as saídas confirmadas e compras do seed
    // nesse intervalo, sem pagamento de fatura, transferência, ajuste e receita.
    let total: i64 = gastos.iter().map(|g| g.centavos).sum();
    assert_eq!(
        total,
        12_040 + 8_990 + 30_000 + 12_345 + 4_321 + 733 + 45_000 + 1_500 + 30
    );
    assert!(gastos.iter().all(|g| g.ciclo == "2026-10"));
    let padaria: i64 = gastos
        .iter()
        .filter(|g| g.fornecedor.as_deref() == Some("Padaria Exemplo"))
        .map(|g| g.centavos)
        .sum();
    assert_eq!(padaria, 12_040 + 8_990 + 4_321 + 10);
    // Filtrar por conta (pelo UUID ou pelo nome): só o cartão.
    let so_cartao = fonte
        .gastos("2026-10", "2026-10", &["Cartao Gama".to_owned()])
        .expect("gastos");
    let total_cartao: i64 = so_cartao.iter().map(|g| g.centavos).sum();
    assert_eq!(total_cartao, 8_990 + 733 + 4_321);
    // Ciclos fora da janela não aparecem.
    assert!(
        fonte
            .gastos("2027-01", "2027-02", &[])
            .expect("gastos")
            .is_empty()
    );
}
