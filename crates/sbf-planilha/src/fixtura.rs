//! Planilha fictícia no layout da aba mensal, gerada nos testes. Nada aqui é dado real.

use std::path::Path;

use rust_xlsxwriter::{ExcelDateTime, Format, Formula, Workbook};

pub const ABA: &str = "2026.11";

/// Gera um `.xlsx` com a aba `2026.11` e uma aba de outro nome.
pub fn planilha_ficticia(caminho: &Path) {
    let mut wb = Workbook::new();
    let data = Format::new().set_num_format("dd/mm/yyyy");

    let outra = wb.add_worksheet();
    outra.set_name("Resumo").expect("nome");
    outra.write_string(0, 0, "ignorar").expect("celula");

    let ws = wb.add_worksheet();
    ws.set_name(ABA).expect("nome");
    let cab = [
        "Grupo",
        "Descrição",
        "Qtd",
        "Unitário",
        "Mínimo",
        "Vencimento",
        "Meio",
        "Valor Previsto",
        "Valor Pago",
        "",
        "Obs",
    ];
    for (i, c) in cab.iter().enumerate() {
        ws.write_string(5, 1 + i as u16, *c).expect("celula");
    }

    // (grupo, descrição, vencimento, meio, previsto, pago)
    type Linha<'a> = (
        &'a str,
        &'a str,
        Option<(u16, u8, u8)>,
        &'a str,
        Option<f64>,
        Option<f64>,
    );
    let linhas: [Linha; 9] = [
        (
            "Essencial",
            "Aluguel Fictício",
            Some((2026, 11, 5)),
            "CX-0001",
            Some(1500.0),
            Some(1500.0),
        ),
        (
            "",
            "Internet Exemplo",
            Some((2026, 11, 10)),
            "Nubank",
            None,
            None,
        ), // fórmula #NAME?
        ("Essencial Total", "", None, "", Some(1500.0), None), // subtotal
        (
            "Lar",
            "Luz Exemplo",
            Some((2026, 11, 12)),
            "CX-0001",
            Some(180.5),
            None,
        ),
        (
            "",
            "Plano de Saúde Exemplo",
            Some((2026, 11, 15)),
            "Salário",
            Some(250.0),
            None,
        ),
        (
            "",
            "Empréstimo Consignado",
            Some((2026, 11, 15)),
            "Salário",
            Some(400.0),
            None,
        ),
        (
            "Parcial",
            "Fatura Velha",
            Some((2026, 3, 10)),
            "Nubank",
            Some(90.0),
            None,
        ), // resto antigo
        (
            "",
            "Taxa Soma",
            Some((2026, 11, 20)),
            "Nubank",
            Some(0.1 + 0.2),
            None,
        ),
        (
            "",
            "Sem Previsto",
            Some((2026, 11, 25)),
            "Nubank",
            None,
            None,
        ),
    ];
    for (i, (g, d, v, m, p, pago)) in linhas.iter().enumerate() {
        let r = 6 + i as u32;
        if !g.is_empty() {
            ws.write_string(r, 1, *g).expect("celula");
        }
        if !d.is_empty() {
            ws.write_string(r, 2, *d).expect("celula");
        }
        if let Some((a, me, di)) = v {
            let dt = ExcelDateTime::from_ymd(*a, *me, *di).expect("data");
            ws.write_datetime_with_format(r, 6, &dt, &data)
                .expect("celula");
        }
        if !m.is_empty() {
            ws.write_string(r, 7, *m).expect("celula");
        }
        if let Some(p) = p {
            ws.write_number(r, 8, *p).expect("celula");
        }
        if let Some(p) = pago {
            ws.write_number(r, 9, *p).expect("celula");
        }
    }
    // Fórmula com erro em cache na linha 8 (Internet Exemplo), coluna I.
    ws.write_formula(
        7,
        8,
        Formula::new("=NOMEINEXISTENTE()").set_result("#NAME?"),
    )
    .expect("celula");
    ws.write_string(7, 11, "obs da internet").expect("celula");

    // Painel lateral.
    ws.write_string(1, 19, "Previsto").expect("celula");
    ws.write_string(1, 20, "Pago").expect("celula");
    ws.write_string(2, 17, "Salário").expect("celula");
    ws.write_number(2, 19, 5000.0).expect("celula");
    ws.write_number(2, 20, 5000.0).expect("celula");
    for (i, h) in ["Consig", "Saude", "INSS", "Prev", "Sal", "IR"]
        .iter()
        .enumerate()
    {
        ws.write_string(2, 32 + i as u16, *h).expect("celula");
    }
    ws.write_string(3, 24, "001 SALARIO BASE 2026/11 5.400,00")
        .expect("celula");
    ws.write_string(3, 27, "Salario base").expect("celula");
    ws.write_number(3, 28, 5400.0).expect("celula");
    ws.write_number(3, 32, 0.0).expect("celula");
    ws.write_number(3, 36, 1.0).expect("celula");
    ws.write_string(4, 24, "210 INSS 2026/11 400,00")
        .expect("celula");
    ws.write_number(4, 34, 1.0).expect("celula");
    ws.write_string(15, 24, "== NÃO DEBITADO ==")
        .expect("celula");
    ws.write_string(16, 24, "301 PLANO SAUDE 2026/11 250,00")
        .expect("celula");
    ws.write_number(16, 32, 1.0).expect("celula");
    ws.write_string(17, 24, "302 EMPRESTIMO CONSIGNADO 2026/11 400,00")
        .expect("celula");
    ws.write_number(17, 32, 1.0).expect("celula");

    // Legenda dos meios.
    for (i, (c, t)) in [
        ("CX-0001", "Conta corrente 0630"),
        ("Nubank", "Cartão Nubank"),
        ("Salário", "Desconto Contracheque"),
    ]
    .iter()
    .enumerate()
    {
        ws.write_string(8 + i as u32, 17, *c).expect("celula");
        ws.write_string(8 + i as u32, 18, *t).expect("celula");
    }

    wb.save(caminho).expect("salvar");
}

/// Inventário fictício em memória: só as colunas de `registro` que este crate lê.
pub fn inventario_ficticio() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().expect("memoria");
    c.execute_batch(
        "CREATE TABLE registro (
           id INTEGER PRIMARY KEY, external_ref TEXT UNIQUE, tipo TEXT NOT NULL,
           data TEXT NOT NULL, ciclo_id TEXT, valor_centavos INTEGER NOT NULL,
           descricao_original TEXT, supplier_name_raw TEXT, fornecedor_normalizado TEXT,
           conta_provavel TEXT);",
    )
    .expect("esquema");
    c
}

/// Insere uma transação. `ciclo` é derivado da data (corte no dia 20).
pub fn registro(
    c: &rusqlite::Connection,
    ref_: &str,
    tipo: &str,
    data: &str,
    valor: i64,
    forn: &str,
) {
    let (a, m, d): (i32, i32, i32) = (
        data[..4].parse().expect("ano"),
        data[5..7].parse().expect("mes"),
        data[8..10].parse().expect("dia"),
    );
    let total = a * 12 + m - 1 - i32::from(d < 20);
    let ciclo = format!(
        "{:04}-{:02}",
        total.div_euclid(12),
        total.rem_euclid(12) + 1
    );
    c.execute(
        "INSERT INTO registro (external_ref, tipo, data, ciclo_id, valor_centavos, fornecedor_normalizado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![ref_, tipo, data, ciclo, valor, forn],
    )
    .expect("insert");
}

/// Insere um movimento do extrato com conta e descrição (o fornecedor fica igual à descrição).
pub fn registro_completo(
    c: &rusqlite::Connection,
    ref_: &str,
    tipo: &str,
    data: &str,
    valor: i64,
    conta: Option<&str>,
    descricao: &str,
) {
    c.execute(
        "INSERT INTO registro (external_ref, tipo, data, valor_centavos, conta_provavel,
                               descricao_original, fornecedor_normalizado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![ref_, tipo, data, valor, conta, descricao],
    )
    .expect("insert");
}

/// Casos de borda de grupo (B numérico) e previsto zero, aba `2026.11`.
pub fn planilha_de_casos(caminho: &Path) {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(ABA).expect("nome");
    let data = Format::new().set_num_format("dd/mm/yyyy");
    let dt = ExcelDateTime::from_ymd(2026, 11, 10).expect("data");
    ws.write_string(6, 1, "Essencial").expect("c");
    ws.write_string(6, 2, "A").expect("c");
    ws.write_number(6, 8, 100.0).expect("c");
    ws.write_number(7, 1, 1234.5).expect("c"); // total do grupo, com linha
    ws.write_string(7, 2, "B").expect("c");
    ws.write_number(7, 8, 50.0).expect("c");
    ws.write_string(8, 1, "300").expect("c"); // total como texto, sem linha
    ws.write_formula(
        9,
        1,
        Formula::new("=NOMEINEXISTENTE()").set_result("#NAME?"),
    )
    .expect("c");
    ws.write_string(9, 2, "C").expect("c");
    ws.write_number(9, 8, 10.0).expect("c");
    ws.write_string(10, 1, "Lar").expect("c");
    ws.write_string(10, 2, "Zero Com Obs").expect("c");
    ws.write_number(10, 4, 80.0).expect("c");
    ws.write_number(10, 8, 0.0).expect("c");
    ws.write_string(10, 11, "atrasado").expect("c");
    ws.write_string(11, 2, "Zero Sem Unitario").expect("c");
    ws.write_number(11, 8, 0.0).expect("c");
    ws.write_string(11, 11, "obs").expect("c");
    ws.write_string(12, 2, "Zero Sem Obs").expect("c");
    ws.write_number(12, 4, 50.0).expect("c");
    ws.write_number(12, 8, 0.0).expect("c");
    for r in [6, 7, 9, 10, 11, 12] {
        ws.write_datetime_with_format(r, 6, &dt, &data).expect("c");
    }
    wb.save(caminho).expect("salvar");
}
