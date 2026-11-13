//! O `LeitorSqlite`: tabelas das áreas, páginas de item, `Ligados`, `ItensDoTotal`, rastro,
//! total por ciclo e a conexão só de leitura (INV-19 e INV-20, parte do inventário).

mod comum;

use comum::{
    BancoDeTeste, H_A_TRIAR_1, H_DUVIDA_REGRAS, H_ENERGIA_OUT, H_ENERGIA_SET, H_FATURA_CARTAO,
    H_MERCADO, H_STREAMING_A, H_STREAMING_B, Reg, hash, inserir_registro,
};
use rusqlite::Connection;
use sbf_inventario::consulta::{
    Area, Celula, Consulta, ErroDeConsulta, Filtro, Fonte, PedidoDeTabela, Quem, Tabela,
};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::referencia::{Ref, Tipo};

const HOJE: &str = "2026-11-13";

fn abrir() -> (BancoDeTeste, LeitorSqlite) {
    let banco = comum::banco_de_teste();
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje(HOJE);
    (banco, leitor)
}

fn r(texto: &str) -> Ref {
    texto.parse().expect("ref válida")
}

fn ligados(leitor: &LeitorSqlite, de: &str, tipo: Tipo) -> Tabela {
    leitor
        .tabela(&PedidoDeTabela::Ligados { de: r(de), tipo })
        .unwrap_or_else(|e| panic!("ligados {de} → {tipo}: {e}"))
}

fn sql_n(banco: &BancoDeTeste, sql: &str) -> usize {
    let conn = Connection::open(&banco.caminho).expect("conexão");
    let n: i64 = conn.query_row(sql, [], |l| l.get(0)).expect("consulta");
    usize::try_from(n).expect("contagem")
}

fn area(leitor: &LeitorSqlite, area: Area) -> Tabela {
    leitor
        .tabela(&PedidoDeTabela::Area {
            area,
            filtro: Filtro::default(),
        })
        .expect("área")
}

#[test]
fn contagens_da_barra_de_status() {
    let (_b, leitor) = abrir();
    let c = leitor.contagens().expect("contagens");
    assert_eq!(c.achados, 13);
    assert_eq!(c.documentos, 6);
    assert_eq!(c.em_duvida, 2);
    assert_eq!(c.a_triar, 2);
}

#[test]
fn texto_extraido_de_um_achado() {
    let (_b, leitor) = abrir();
    let t = leitor
        .texto(&Ref::nova(Tipo::Achado, hash(H_ENERGIA_OUT)))
        .expect("texto");
    assert!(t.expect("tem texto").contains("Fatura de energia"));
    assert_eq!(
        leitor
            .texto(&Ref::nova(Tipo::Achado, hash(H_MERCADO)))
            .expect("texto"),
        None
    );
    assert_eq!(
        leitor
            .texto(&r("fornecedor:Energia Exemplo"))
            .expect("texto"),
        None
    );
}

#[test]
fn fornecedor_liga_registros_documentos_e_decisoes() {
    let (b, leitor) = abrir();
    let energia = "fornecedor:Energia Exemplo";
    assert_eq!(ligados(&leitor, energia, Tipo::Registro).linhas.len(), 5);
    assert_eq!(ligados(&leitor, energia, Tipo::Documento).linhas.len(), 2);
    assert_eq!(ligados(&leitor, energia, Tipo::Achado).linhas.len(), 2);
    let streaming = "fornecedor:Streaming Zeta";
    assert_eq!(ligados(&leitor, streaming, Tipo::Registro).linhas.len(), 3);
    assert_eq!(ligados(&leitor, streaming, Tipo::Documento).linhas.len(), 2);
    assert_eq!(ligados(&leitor, streaming, Tipo::Decisao).linhas.len(), 1);
    // Os registros ligados são os que a SQL diz.
    assert_eq!(
        sql_n(
            &b,
            "SELECT count(*) FROM registro WHERE fornecedor_normalizado = 'Energia Exemplo'"
        ),
        5
    );
    // O fornecedor sem documentos nem decisões devolve tabela vazia, não erro.
    assert!(
        ligados(&leitor, "fornecedor:Mercado Delta", Tipo::Decisao)
            .linhas
            .is_empty()
    );
}

#[test]
fn ciclo_liga_registros_e_documentos() {
    let (b, leitor) = abrir();
    let n = sql_n(
        &b,
        "SELECT count(*) FROM registro WHERE ciclo_id = '2026-10'",
    );
    assert_eq!(
        ligados(&leitor, "ciclo:2026-10", Tipo::Registro)
            .linhas
            .len(),
        n
    );
    assert_eq!(
        ligados(&leitor, "ciclo:2026-10", Tipo::Documento)
            .linhas
            .len(),
        1
    );
}

#[test]
fn conta_e_fatura_ligam_registros() {
    let (b, leitor) = abrir();
    let n = sql_n(
        &b,
        "SELECT count(*) FROM registro WHERE conta_provavel = 'Cartão Beta'",
    );
    assert_eq!(
        ligados(&leitor, "conta:Cartão Beta", Tipo::Registro)
            .linhas
            .len(),
        n
    );
    assert_eq!(
        ligados(&leitor, "conta:Cartão Beta", Tipo::Fatura)
            .linhas
            .len(),
        2
    );
    // A fatura de outubro: as duas compras ligadas mais a própria fatura.
    assert_eq!(
        ligados(&leitor, "fatura:b-out", Tipo::Registro)
            .linhas
            .len(),
        3
    );
}

#[test]
fn achado_liga_origens_registros_decisoes_lotes_e_relacoes() {
    let (_b, leitor) = abrir();
    let h1 = format!("achado:{}", hash(H_ENERGIA_OUT));
    assert_eq!(ligados(&leitor, &h1, Tipo::Origem).linhas.len(), 2);
    assert_eq!(ligados(&leitor, &h1, Tipo::Registro).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &h1, Tipo::Decisao).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &h1, Tipo::Lote).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &h1, Tipo::Achado).linhas.len(), 0);
    assert_eq!(ligados(&leitor, &h1, Tipo::Fornecedor).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &h1, Tipo::Ciclo).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &h1, Tipo::Conexao).linhas.len(), 2);
    assert_eq!(ligados(&leitor, &h1, Tipo::Coleta).linhas.len(), 2);
    // Achado → documento não faz sentido.
    assert!(ligados(&leitor, &h1, Tipo::Documento).linhas.is_empty());
    // A mesma página vale para `documento:<hash>`.
    let d1 = format!("documento:{}", hash(H_ENERGIA_OUT));
    assert_eq!(ligados(&leitor, &d1, Tipo::Origem).linhas.len(), 2);

    // Possível duplicidade: a relação aparece dos dois lados.
    let a = format!("achado:{}", hash(H_STREAMING_A));
    let b = format!("achado:{}", hash(H_STREAMING_B));
    assert_eq!(ligados(&leitor, &a, Tipo::Achado).linhas.len(), 1);
    assert_eq!(ligados(&leitor, &b, Tipo::Achado).linhas.len(), 1);

    let h2 = format!("achado:{}", hash(H_ENERGIA_SET));
    assert_eq!(ligados(&leitor, &h2, Tipo::Decisao).linhas.len(), 2);
    // A pendência que trava o achado.
    let h4 = format!("achado:{}", hash(H_FATURA_CARTAO));
    assert_eq!(ligados(&leitor, &h4, Tipo::Pendencia).linhas.len(), 1);
}

#[test]
fn origem_conexao_e_coleta_se_ligam() {
    let (b, leitor) = abrir();
    assert_eq!(ligados(&leitor, "origem:1", Tipo::Achado).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "origem:1", Tipo::Conexao).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "origem:1", Tipo::Coleta).linhas.len(), 1);
    let gmail = sql_n(&b, "SELECT count(*) FROM origem WHERE fonte_id = 'gmail'");
    assert_eq!(
        ligados(&leitor, "conexao:gmail", Tipo::Origem).linhas.len(),
        gmail
    );
    assert_eq!(
        ligados(&leitor, "conexao:gmail", Tipo::Coleta).linhas.len(),
        2
    );
    let da_coleta = sql_n(&b, "SELECT count(*) FROM origem WHERE varredura_id = 1");
    assert_eq!(
        ligados(&leitor, "coleta:1", Tipo::Origem).linhas.len(),
        da_coleta
    );
    // A chave da conexão pode ter `:`.
    assert_eq!(
        ligados(&leitor, "conexao:disco:e2", Tipo::Coleta)
            .linhas
            .len(),
        2
    );
}

#[test]
fn registro_lote_pendencia_regra_e_decisao_se_ligam() {
    let (_b, leitor) = abrir();
    // Registro 2 é o promovido (documento da conta de setembro).
    assert_eq!(ligados(&leitor, "registro:2", Tipo::Achado).linhas.len(), 1);
    assert_eq!(
        ligados(&leitor, "registro:2", Tipo::Decisao).linhas.len(),
        1
    );
    assert_eq!(ligados(&leitor, "registro:2", Tipo::Lote).linhas.len(), 1);
    assert_eq!(
        ligados(&leitor, "registro:2", Tipo::Conexao).linhas.len(),
        1
    );
    assert_eq!(
        ligados(&leitor, "registro:2", Tipo::Fornecedor)
            .linhas
            .len(),
        1
    );
    assert_eq!(ligados(&leitor, "registro:2", Tipo::Conta).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "registro:2", Tipo::Ciclo).linhas.len(), 1);

    // Unir conciliados: o registro do documento e o da conexão.
    assert_eq!(
        ligados(&leitor, "registro:1", Tipo::Registro).linhas.len(),
        1
    );
    assert!(
        ligados(&leitor, "registro:2", Tipo::Registro)
            .linhas
            .is_empty()
    );

    // Lote → itens.
    assert_eq!(ligados(&leitor, "lote:1", Tipo::Registro).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "lote:1", Tipo::Documento).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "lote:2", Tipo::Achado).linhas.len(), 2);
    assert_eq!(ligados(&leitor, "lote:2", Tipo::Registro).linhas.len(), 2);

    // Pendência → o alvo que ela trava.
    assert_eq!(
        ligados(&leitor, "pendencia:1", Tipo::Achado).linhas.len(),
        1
    );
    assert!(
        ligados(&leitor, "pendencia:1", Tipo::Registro)
            .linhas
            .is_empty()
    );
    assert_eq!(
        ligados(&leitor, "pendencia:2", Tipo::Registro).linhas.len(),
        1
    );
    assert_eq!(
        ligados(&leitor, "pendencia:3", Tipo::Achado).linhas.len(),
        1
    );

    // Regra de senha → tentativas e fornecedor.
    assert_eq!(
        ligados(&leitor, "regra-senha:1", Tipo::Achado).linhas.len(),
        2
    );
    assert_eq!(
        ligados(&leitor, "regra-senha:1", Tipo::Fornecedor)
            .linhas
            .len(),
        1
    );
    assert!(
        ligados(&leitor, "regra-senha:2", Tipo::Achado)
            .linhas
            .is_empty()
    );

    // Decisão → alvo, e a cadeia desfaz / desfeita por.
    assert_eq!(ligados(&leitor, "decisao:3", Tipo::Achado).linhas.len(), 1);
    assert_eq!(
        ligados(&leitor, "decisao:7", Tipo::Registro).linhas.len(),
        1
    );
    assert_eq!(
        ligados(&leitor, "decisao:8", Tipo::Fornecedor).linhas.len(),
        1
    );
    assert_eq!(ligados(&leitor, "decisao:9", Tipo::Lote).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "decisao:6", Tipo::Decisao).linhas.len(), 1);
    assert_eq!(ligados(&leitor, "decisao:5", Tipo::Decisao).linhas.len(), 1);
}

#[test]
fn par_sem_sentido_e_tabela_vazia_e_fatia_futura_e_nao_suportada() {
    let (_b, leitor) = abrir();
    assert!(
        ligados(&leitor, "conta:Cartão Beta", Tipo::Lote)
            .linhas
            .is_empty()
    );
    assert!(
        ligados(&leitor, "ciclo:2026-10", Tipo::Pendencia)
            .linhas
            .is_empty()
    );
    for (de, tipo, fatia) in [
        ("fornecedor:Energia Exemplo", Tipo::Movimentacao, "fatia 2"),
        ("fornecedor:Energia Exemplo", Tipo::Compromisso, "fatia 2"),
        ("fornecedor:Energia Exemplo", Tipo::Alerta, "fatia 4"),
        ("fornecedor:Energia Exemplo", Tipo::Roteiro, "fatia 5"),
        ("fornecedor:Energia Exemplo", Tipo::Ponto, "fatia 6"),
        ("mov:12", Tipo::Registro, "fatia 2"),
    ] {
        let erro = leitor
            .tabela(&PedidoDeTabela::Ligados { de: r(de), tipo })
            .expect_err("deve recusar");
        let ErroDeConsulta::NaoSuportado(motivo) = erro else {
            panic!("esperava NaoSuportado, veio {erro}")
        };
        assert!(motivo.contains(fatia), "{motivo}");
        assert_eq!(
            Some(motivo),
            if tipo.fatia() == 1 {
                r(de).tipo.motivo_da_fatia()
            } else {
                tipo.motivo_da_fatia()
            }
            .or(r(de).tipo.motivo_da_fatia())
        );
    }
}

#[test]
fn livro_caixa_nao_e_suportado_nos_totais() {
    let (_b, leitor) = abrir();
    let erro = leitor
        .tabela(&PedidoDeTabela::ItensDoTotal {
            de: r("fornecedor:Energia Exemplo"),
            fonte: Fonte::LivroCaixa,
            ciclo: None,
            recorte: None,
        })
        .expect_err("recusa");
    assert!(matches!(erro, ErroDeConsulta::NaoSuportado(_)));
    assert!(matches!(
        leitor.por_ciclo(&r("fornecedor:Energia Exemplo"), Fonte::LivroCaixa, 3),
        Err(ErroDeConsulta::NaoSuportado(_))
    ));
}

fn campo_do_item(leitor: &LeitorSqlite, alvo: &str, rotulo: &str) -> Celula {
    let item = leitor.item(&r(alvo)).expect("item").expect("existe");
    item.campos
        .into_iter()
        .find(|c| c.rotulo == rotulo)
        .unwrap_or_else(|| panic!("{alvo} sem o campo {rotulo}"))
        .valor
}

#[test]
fn cabecalho_do_fornecedor_tem_apelidos_e_contratos() {
    let (banco, leitor) = abrir();
    // Na fixture, toda grafia de Energia Exemplo é o próprio nome: nenhum apelido.
    assert_eq!(
        campo_do_item(&leitor, "fornecedor:Energia Exemplo", "Apelidos"),
        Celula::Vazia
    );
    assert_eq!(
        campo_do_item(&leitor, "fornecedor:Energia Exemplo", "Contratos"),
        Celula::Texto("chegam na fatia 2".into())
    );

    let conn = Connection::open(&banco.caminho).expect("conexão");
    let base = Reg {
        fonte: "disco:e2",
        tipo: "pagamento",
        data: "2026-10-26",
        valor: -100,
        descricao: "Pagamento de conta",
        fornecedor: Some("Energia Exemplo"),
        estado: "extraido",
        extraido_por: "extrator@1",
        ..Reg::default()
    };
    for (bruto, contraparte) in [
        (Some("ENERGIA  EXEMPLO S.A."), None),
        (
            Some("energia exemplo s.a."),
            Some("Energia Exemplo Distribuidora"),
        ),
        (Some(" energia exemplo "), None),
    ] {
        inserir_registro(
            &conn,
            &Reg {
                supplier_raw: bruto,
                contraparte,
                ..base.clone()
            },
        );
    }
    let Celula::Texto(apelidos) = campo_do_item(&leitor, "fornecedor:Energia Exemplo", "Apelidos")
    else {
        panic!("apelidos em texto")
    };
    // A mais usada primeiro, caixa e espaços juntos, sem o próprio nome nem a descrição.
    assert_eq!(
        apelidos,
        "ENERGIA EXEMPLO S.A. · Energia Exemplo Distribuidora"
    );
}

#[test]
fn total_por_ciclo_do_fornecedor() {
    let (_b, leitor) = abrir();
    let totais = leitor
        .por_ciclo(&r("fornecedor:Energia Exemplo"), Fonte::Inventario, 3)
        .expect("por ciclo");
    let lidos: Vec<(&str, i64)> = totais
        .iter()
        .map(|t| (t.ciclo.as_str(), t.centavos))
        .collect();
    assert_eq!(
        lidos,
        vec![
            ("2026-08", 16_500),
            ("2026-09", 34_400),
            ("2026-10", 37_500)
        ]
    );
    for t in &totais {
        let Some(pedido) = &t.itens else {
            panic!("todo total tem itens")
        };
        let tabela = leitor.tabela(pedido).expect("itens");
        assert!(!tabela.linhas.is_empty());
    }
    // Ciclos sem gasto valem 0, e a ordem é do mais antigo ao mais novo.
    let seis = leitor
        .por_ciclo(&r("fornecedor:Energia Exemplo"), Fonte::Inventario, 6)
        .expect("por ciclo");
    assert_eq!(seis.len(), 6);
    assert_eq!(seis[0].ciclo, "2026-05");
    assert_eq!(seis[0].centavos, 0);
    assert_eq!(seis[5].ciclo, "2026-10");
    // O movimento entre contas não conta: a conta corrente moveu 100.000 mas gastou menos.
    let conta = leitor
        .por_ciclo(&r("conta:Conta Corrente Alfa"), Fonte::Inventario, 1)
        .expect("por ciclo");
    assert_eq!(conta[0].centavos, 18_750 + 18_750 + 2_990);
}

#[test]
fn rastro_do_achado_vai_da_origem_aos_registros_lote_e_decisoes() {
    let (_b, leitor) = abrir();
    let rastro = leitor
        .rastro(&Ref::nova(Tipo::Achado, hash(H_ENERGIA_OUT)))
        .expect("rastro");
    // Acima: duas origens, com a coleta (responsável e quando) e a conexão de cada uma.
    let origens: Vec<_> = rastro
        .acima
        .iter()
        .filter(|e| e.alvo.tipo == Tipo::Origem)
        .collect();
    assert_eq!(origens.len(), 2);
    let coletas: Vec<_> = rastro
        .acima
        .iter()
        .filter(|e| e.alvo.tipo == Tipo::Coleta)
        .collect();
    assert_eq!(coletas.len(), 2);
    for c in &coletas {
        assert!(matches!(c.quem, Some(Quem::Automatico(_))));
        assert!(c.quando.is_some());
    }
    let conexoes: Vec<_> = rastro
        .acima
        .iter()
        .filter(|e| e.alvo.tipo == Tipo::Conexao)
        .collect();
    assert_eq!(conexoes.len(), 2);
    assert!(
        coletas
            .iter()
            .any(|c| c.quem == Some(Quem::Automatico("scan-discos@3".into())))
    );
    // Abaixo: o registro extraído, o lote e a decisão.
    let tipos_abaixo: Vec<Tipo> = rastro.abaixo.iter().map(|e| e.alvo.tipo).collect();
    assert!(tipos_abaixo.contains(&Tipo::Registro));
    assert!(tipos_abaixo.contains(&Tipo::Lote));
    assert!(tipos_abaixo.contains(&Tipo::Decisao));
    let reg = rastro
        .abaixo
        .iter()
        .find(|e| e.alvo.tipo == Tipo::Registro)
        .expect("registro");
    assert_eq!(reg.quem, Some(Quem::Automatico("extrator@1".into())));
    assert!(reg.quando.is_some());
    let dec = rastro
        .abaixo
        .iter()
        .find(|e| e.alvo.tipo == Tipo::Decisao)
        .expect("decisão");
    assert_eq!(dec.quem, Some(Quem::Automatico("regras@2".into())));
    assert_eq!(dec.papel, "decidido por");
    // A decisão do usuário aparece como "você".
    let h2 = leitor
        .rastro(&Ref::nova(Tipo::Achado, hash(H_ENERGIA_SET)))
        .expect("rastro");
    assert!(
        h2.abaixo
            .iter()
            .any(|e| e.quem == Some(Quem::Usuario) && e.alvo.tipo == Tipo::Decisao)
    );
    // O elo do lote aprovado diz quando foi aprovado.
    let lote = h2
        .abaixo
        .iter()
        .find(|e| e.alvo.tipo == Tipo::Lote)
        .expect("lote");
    assert_eq!(lote.papel, "aprovado no lote");
    assert_eq!(lote.quando.as_deref(), Some("2026-10-21T10:00:00.000Z"));
}

#[test]
fn rastro_do_registro_vai_ate_a_movimentacao() {
    let (_b, leitor) = abrir();
    let promovido = leitor.rastro(&r("registro:2")).expect("rastro");
    // Acima: o documento, as origens, as coletas e as conexões.
    assert!(
        promovido
            .acima
            .iter()
            .any(|e| e.alvo.tipo == Tipo::Documento && e.papel == "extraído de")
    );
    assert_eq!(
        promovido
            .acima
            .iter()
            .filter(|e| e.alvo.tipo == Tipo::Origem)
            .count(),
        2
    );
    assert!(promovido.acima.iter().any(|e| e.alvo.tipo == Tipo::Conexao));
    // Abaixo: a decisão, o lote e a movimentação em que virou.
    let mov = promovido
        .abaixo
        .iter()
        .find(|e| e.alvo.tipo == Tipo::Movimentacao)
        .expect("movimentação");
    assert_eq!(mov.alvo.chave, "mov-101");
    assert_eq!(mov.papel, "promovido para");
    assert_eq!(mov.quando.as_deref(), Some("2026-10-22T10:00:00.000Z"));
    assert!(promovido.abaixo.iter().any(|e| e.alvo.tipo == Tipo::Lote));
    assert!(
        promovido
            .abaixo
            .iter()
            .any(|e| e.alvo.tipo == Tipo::Decisao)
    );

    // Um registro da conexão (sem documento) para na conexão e não tem movimentação.
    let do_banco = leitor.rastro(&r("registro:4")).expect("rastro");
    assert_eq!(do_banco.acima.len(), 1);
    assert_eq!(do_banco.acima[0].alvo, r("conexao:conexao:pluggy"));
    assert!(
        do_banco
            .abaixo
            .iter()
            .all(|e| e.alvo.tipo != Tipo::Movimentacao)
    );
}

#[test]
fn rastro_de_cada_tipo_da_fatia_1() {
    let (_b, leitor) = abrir();
    for alvo in [
        "origem:1",
        "coleta:1",
        "conexao:gmail",
        "decisao:6",
        "decisao:5",
        "lote:2",
        "pendencia:1",
        "regra-senha:1",
        "fornecedor:Streaming Zeta",
        "ciclo:2026-10",
        "conta:Cartão Beta",
        "fatura:b-out",
    ] {
        let rastro = leitor
            .rastro(&r(alvo))
            .unwrap_or_else(|e| panic!("{alvo}: {e}"));
        let _ = rastro;
    }
    let origem = leitor.rastro(&r("origem:1")).expect("rastro");
    assert_eq!(origem.acima.len(), 2);
    assert!(origem.abaixo.iter().any(|e| e.alvo.tipo == Tipo::Achado));
    let decisao = leitor.rastro(&r("decisao:6")).expect("rastro");
    assert!(decisao.acima.iter().any(|e| e.alvo.tipo == Tipo::Achado));
    assert!(decisao.acima.iter().any(|e| e.alvo.tipo == Tipo::Decisao));
    let pendencia = leitor.rastro(&r("pendencia:1")).expect("rastro");
    assert_eq!(pendencia.acima.len(), 1);
    let lote = leitor.rastro(&r("lote:2")).expect("rastro");
    assert_eq!(lote.acima.len(), 4);
    let regra = leitor.rastro(&r("regra-senha:1")).expect("rastro");
    assert_eq!(regra.abaixo.len(), 2);
}

#[test]
fn todo_item_da_fatia_1_tem_cabecalho_e_item_ausente_e_none() {
    let (_b, leitor) = abrir();
    let mut refs = vec![
        r("fornecedor:Energia Exemplo"),
        r("fornecedor:Streaming Zeta"),
        r("ciclo:2026-10"),
        r("conta:Cartão Beta"),
        r("conta:Conta Corrente Alfa"),
        r("fatura:b-out"),
        r("fatura:b-set"),
        r("conexao:disco:e2"),
        r("conexao:gmail"),
        r("conexao:celular"),
    ];
    for n in 1..=13 {
        refs.push(Ref::nova(Tipo::Achado, hash(n)));
    }
    refs.push(Ref::nova(Tipo::Documento, hash(H_ENERGIA_OUT)));
    for n in 1..=17 {
        refs.push(Ref::nova(Tipo::Origem, n.to_string()));
    }
    for n in 1..=130 {
        refs.push(Ref::nova(Tipo::Registro, n.to_string()));
    }
    for n in 1..=10 {
        refs.push(Ref::nova(Tipo::Decisao, n.to_string()));
    }
    for n in 1..=2 {
        refs.push(Ref::nova(Tipo::Lote, n.to_string()));
        refs.push(Ref::nova(Tipo::RegraDeSenha, n.to_string()));
    }
    for n in 1..=3 {
        refs.push(Ref::nova(Tipo::Pendencia, n.to_string()));
    }
    for n in 1..=6 {
        refs.push(Ref::nova(Tipo::Coleta, n.to_string()));
    }
    let mut achados = 0;
    for alvo in &refs {
        let item = leitor.item(alvo).unwrap_or_else(|e| panic!("{alvo}: {e}"));
        if let Some(item) = item {
            achados += 1;
            assert_eq!(&item.alvo, alvo);
            assert!(!item.rotulo.is_empty(), "{alvo}");
            assert!(!item.campos.is_empty(), "{alvo}");
        }
    }
    // Todos os que a fixture tem: 10 + 13 + 1 + 17 + (registros) + 10 + 4 + 3 + 6.
    let registros = sql_registros(&_b);
    assert_eq!(achados, 10 + 13 + 1 + 17 + registros + 10 + 4 + 3 + 6);
    assert!(
        leitor
            .item(&r("fornecedor:Não Existe"))
            .expect("item")
            .is_none()
    );
    assert!(leitor.item(&r("registro:99999")).expect("item").is_none());
    assert!(leitor.item(&r("origem:99999")).expect("item").is_none());
    assert!(matches!(
        leitor.item(&r("mov:1")),
        Err(ErroDeConsulta::NaoSuportado(_))
    ));
}

fn sql_registros(banco: &BancoDeTeste) -> usize {
    sql_n(banco, "SELECT count(*) FROM registro")
}

/// Quantas linhas uma tabela de itens tem e quanto somam os valores dela.
fn confere_total(leitor: &LeitorSqlite, onde: &str, celula: &Celula) -> usize {
    match celula {
        Celula::Contagem {
            n,
            itens: Some(pedido),
        } => {
            let tabela = leitor
                .tabela(pedido)
                .unwrap_or_else(|e| panic!("{onde}: {pedido:?}: {e}"));
            assert_eq!(
                i64::try_from(tabela.linhas.len()).expect("n"),
                *n,
                "{onde}: a contagem {n} deveria abrir {n} itens ({pedido:?})"
            );
            1
        }
        Celula::Valor {
            centavos,
            fonte,
            itens: Some(pedido),
        } => {
            assert_eq!(*fonte, Fonte::Inventario, "{onde}");
            let tabela = leitor
                .tabela(pedido)
                .unwrap_or_else(|e| panic!("{onde}: {pedido:?}: {e}"));
            let Some(idx) = tabela.colunas.iter().position(|c| c.rotulo == "Valor") else {
                panic!("{onde}: o total {centavos} abre uma tabela sem coluna Valor ({pedido:?})")
            };
            let soma: i64 = tabela
                .linhas
                .iter()
                .map(|l| match &l.celulas[idx] {
                    Celula::Valor { centavos, .. } => *centavos,
                    outra => panic!("{onde}: esperava Valor, veio {outra:?}"),
                })
                .sum();
            let PedidoDeTabela::ItensDoTotal { recorte, .. } = pedido.as_ref() else {
                panic!("{onde}: o total abre {pedido:?}")
            };
            let base = recorte
                .as_deref()
                .and_then(|t| t.split(['|', ':']).next())
                .unwrap_or("registros");
            let esperado = if matches!(base, "saidas" | "movido" | "transacoes" | "fatura") {
                -soma
            } else {
                soma
            };
            assert_eq!(
                *centavos, esperado,
                "{onde}: o total não bate com os itens que ele abre ({pedido:?})"
            );
            1
        }
        _ => 0,
    }
}

fn percorrer(leitor: &LeitorSqlite, onde: &str, tabela: &Tabela) -> usize {
    let mut n = 0;
    for l in &tabela.linhas {
        for c in &l.celulas {
            n += confere_total(leitor, onde, c);
        }
    }
    n
}

#[test]
fn todo_total_abre_os_itens_que_o_somam() {
    let (_b, leitor) = abrir();
    let mut conferidos = 0;
    for a in [
        Area::Ciclos,
        Area::Fornecedores,
        Area::Registros,
        Area::Documentos,
        Area::EmDuvida,
        Area::ATriar,
        Area::Faturas,
        Area::ContasLacunas,
        Area::Coletas,
        Area::Pendencias,
        Area::Lotes,
    ] {
        conferidos += percorrer(&leitor, &format!("{a:?}"), &area(&leitor, a));
    }
    // Áreas com filtro: o número da tabela filtrada abre os itens do mesmo filtro.
    let filtros = [
        Filtro {
            ciclos: Some(("2026-09".into(), "2026-10".into())),
            ..Filtro::default()
        },
        Filtro {
            fontes: vec!["conexao:pluggy".into()],
            ..Filtro::default()
        },
        Filtro {
            texto: Some("energia".into()),
            ..Filtro::default()
        },
        Filtro {
            tipos: vec!["compra".into()],
            ..Filtro::default()
        },
    ];
    for f in &filtros {
        for a in [Area::Fornecedores, Area::Ciclos] {
            let t = leitor
                .tabela(&PedidoDeTabela::Area {
                    area: a,
                    filtro: f.clone(),
                })
                .expect("área filtrada");
            conferidos += percorrer(&leitor, &format!("{a:?} {f:?}"), &t);
        }
    }
    for alvo in [
        "fornecedor:Energia Exemplo",
        "fornecedor:Streaming Zeta",
        "fornecedor:Mercado Delta",
        "ciclo:2026-10",
        "ciclo:2026-09",
        "conta:Cartão Beta",
        "conta:Conta Corrente Alfa",
        "fatura:b-out",
        "fatura:b-set",
        "conexao:disco:e2",
        "conexao:conexao:pluggy",
        "coleta:1",
        "lote:1",
        "lote:2",
        "regra-senha:1",
        "pendencia:1",
        "origem:1",
        "registro:2",
        "decisao:3",
    ] {
        let item = leitor
            .item(&r(alvo))
            .expect("item")
            .unwrap_or_else(|| panic!("{alvo} existe"));
        for c in &item.campos {
            conferidos += confere_total(&leitor, alvo, &c.valor);
        }
    }
    for n in 1..=13 {
        let item = leitor
            .item(&Ref::nova(Tipo::Achado, hash(n)))
            .expect("item")
            .expect("achado");
        for c in &item.campos {
            conferidos += confere_total(&leitor, &format!("achado {n}"), &c.valor);
        }
    }
    let painel = leitor.visao_geral().expect("visão geral");
    for s in &painel.secoes {
        conferidos += percorrer(&leitor, &s.titulo, &s.tabela);
    }
    for p in [
        sbf_inventario::consulta::Pergunta::QuantoSaiu,
        sbf_inventario::consulta::Pergunta::MaioresFornecedores,
        sbf_inventario::consulta::Pergunta::OQueSeRepete,
        sbf_inventario::consulta::Pergunta::OQueAumentou,
        sbf_inventario::consulta::Pergunta::OndeSeConcentra,
        sbf_inventario::consulta::Pergunta::NaoClassificado,
        sbf_inventario::consulta::Pergunta::PossiveisDuplicidades,
        sbf_inventario::consulta::Pergunta::OQueFaltaParaConfiar,
    ] {
        let painel = leitor
            .relatorio(p, Fonte::Inventario, &Filtro::default())
            .expect("relatório");
        for s in &painel.secoes {
            conferidos += percorrer(&leitor, &format!("{p:?} / {}", s.titulo), &s.tabela);
        }
    }
    assert!(conferidos > 200, "só {conferidos} totais conferidos");
}

#[test]
fn nome_de_item_e_link_nas_tabelas() {
    let (_b, leitor) = abrir();
    let registros = area(&leitor, Area::Registros);
    let col = |nome: &str| {
        registros
            .colunas
            .iter()
            .position(|c| c.rotulo == nome)
            .expect("coluna")
    };
    let forn = col("Fornecedor");
    let conta = col("Conta");
    let ciclo = col("Ciclo");
    let fonte = col("Fonte");
    let mut com_fornecedor = 0;
    for l in &registros.linhas {
        match &l.celulas[forn] {
            Celula::Link { alvo, texto } => {
                assert_eq!(alvo.tipo, Tipo::Fornecedor);
                assert_eq!(&alvo.chave, texto);
                com_fornecedor += 1;
            }
            Celula::Vazia => {}
            outra => panic!("fornecedor deveria ser link: {outra:?}"),
        }
        assert!(matches!(
            l.celulas[conta],
            Celula::Link { .. } | Celula::Vazia
        ));
        assert!(matches!(
            l.celulas[ciclo],
            Celula::Link { .. } | Celula::Vazia
        ));
        assert!(matches!(l.celulas[fonte], Celula::Link { .. }));
    }
    assert!(com_fornecedor > 20);
}

#[test]
fn leitor_e_so_de_leitura_e_convive_com_um_escritor_em_wal() {
    let banco = comum::banco_de_teste();
    // Um escritor em WAL mantém a conexão aberta e grava enquanto o leitor lê.
    let escritor = Connection::open(&banco.caminho).expect("escritor");
    escritor
        .pragma_update(None, "journal_mode", "WAL")
        .expect("wal");
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje(HOJE);
    let antes = leitor.contagens().expect("contagens").achados;
    escritor
        .execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 1, 'B')",
            [hash(200)],
        )
        .expect("gravar");
    assert_eq!(leitor.contagens().expect("contagens").achados, antes + 1);
    // O leitor não deixa escrita passar: a conexão é `query_only`.
    let so_leitura =
        Connection::open_with_flags(&banco.caminho, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("ro");
    assert!(so_leitura.execute("DELETE FROM achado", []).is_err());
}

#[test]
fn abrir_banco_que_nao_existe_falha_sem_criar_arquivo() {
    let dir = tempfile::tempdir().expect("dir");
    let caminho = dir.path().join("nao-existe.sqlite");
    assert!(matches!(
        LeitorSqlite::abrir(&caminho),
        Err(ErroDeConsulta::Banco(_))
    ));
    assert!(!caminho.exists());
}

#[test]
fn banco_sem_as_colunas_do_bun_ainda_abre() {
    // Um banco na migração 1 pura (sem contraparte, data_compra e fatura_ref) lê NULL no lugar.
    let dir = tempfile::tempdir().expect("dir");
    let caminho = dir.path().join("inventario.sqlite");
    let conn = Connection::open(&caminho).expect("banco");
    conn.execute_batch(sbf_inventario::esquema::MIGRACAO_1_BASE_BUN)
        .expect("m1");
    conn.execute_batch(sbf_inventario::esquema::MIGRACAO_2_DECISOES_E_LOTES)
        .expect("m2");
    conn.execute_batch(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('conexao:pluggy', 'banco', 'Pluggy');
         INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-10', '2026-10-20', '2026-11-19');
         INSERT INTO registro (fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos, extraido_por, fornecedor_normalizado)
           VALUES ('conexao:pluggy', 'compra', '2026-10-21', 'compra', '2026-10', -1000, 'x', 'Loja Teste');",
    )
    .expect("dados");
    drop(conn);
    let leitor = LeitorSqlite::abrir(&caminho).expect("abrir").com_hoje(HOJE);
    for a in [
        Area::Registros,
        Area::Fornecedores,
        Area::Faturas,
        Area::Ciclos,
    ] {
        let t = leitor
            .tabela(&PedidoDeTabela::Area {
                area: a,
                filtro: Filtro::default(),
            })
            .unwrap_or_else(|e| panic!("{a:?}: {e}"));
        let _ = t;
    }
    assert_eq!(area(&leitor, Area::Registros).linhas.len(), 1);
    assert!(leitor.visao_geral().is_ok());
}

#[test]
fn areas_pendencias_e_lotes_com_filtro() {
    let (_b, leitor) = abrir();
    let todas = area(&leitor, Area::Pendencias);
    assert_eq!(todas.linhas.len(), 3);
    // As abertas vêm antes.
    let situacao = |t: &Tabela, i: usize| match &t.linhas[i].celulas[1] {
        Celula::Estado { rotulo, .. } => rotulo.clone(),
        outra => panic!("{outra:?}"),
    };
    assert_eq!(situacao(&todas, 0), "Aberta");
    assert_eq!(situacao(&todas, 2), "Respondida");
    let abertas = leitor
        .tabela(&PedidoDeTabela::Area {
            area: Area::Pendencias,
            filtro: Filtro {
                estados: vec!["aberta".into()],
                ..Filtro::default()
            },
        })
        .expect("abertas");
    assert_eq!(abertas.linhas.len(), 2);
    let lotes = area(&leitor, Area::Lotes);
    assert_eq!(lotes.linhas.len(), 2);
    let aprovados = leitor
        .tabela(&PedidoDeTabela::Area {
            area: Area::Lotes,
            filtro: Filtro {
                estados: vec!["aprovado".into(), "promovido".into()],
                ..Filtro::default()
            },
        })
        .expect("lotes");
    assert_eq!(aprovados.linhas.len(), 1);
}

#[test]
fn visao_geral_e_relatorios_nao_sao_tabelas() {
    let (_b, leitor) = abrir();
    for a in [Area::VisaoGeral, Area::Relatorios] {
        assert!(matches!(
            leitor.tabela(&PedidoDeTabela::Area {
                area: a,
                filtro: Filtro::default()
            }),
            Err(ErroDeConsulta::NaoSuportado(_))
        ));
    }
}

#[test]
fn a_triar_e_em_duvida_mostram_quem_decidiu() {
    let (_b, leitor) = abrir();
    let duvidas = area(&leitor, Area::EmDuvida);
    assert_eq!(duvidas.linhas.len(), 2);
    let quem: Vec<Option<Quem>> = duvidas
        .linhas
        .iter()
        .map(|l| match &l.celulas[1] {
            Celula::Estado { rotulo, quem } => {
                assert_eq!(rotulo, "Em dúvida");
                quem.clone()
            }
            outra => panic!("{outra:?}"),
        })
        .collect();
    assert!(quem.contains(&Some(Quem::Usuario)));
    assert!(quem.contains(&Some(Quem::Automatico("regras@2".into()))));
    let a_triar = area(&leitor, Area::ATriar);
    for l in &a_triar.linhas {
        match &l.celulas[1] {
            Celula::Estado { rotulo, quem } => {
                assert_eq!(rotulo, "A triar");
                assert_eq!(*quem, None);
            }
            outra => panic!("{outra:?}"),
        }
    }
    let alvos: Vec<&Ref> = a_triar
        .linhas
        .iter()
        .filter_map(|l| l.alvo.as_ref())
        .collect();
    assert!(alvos.contains(&&Ref::nova(Tipo::Achado, hash(H_A_TRIAR_1))));
    assert!(!alvos.contains(&&Ref::nova(Tipo::Achado, hash(H_DUVIDA_REGRAS))));
}
