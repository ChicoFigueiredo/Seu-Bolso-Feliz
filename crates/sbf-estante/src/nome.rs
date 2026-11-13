//! Nome do arquivo na estante: porte fiel de `workers/inventario/src/vista-nome.ts`
//! (`trechoDeNome`, `nomeNaVista`) e de `PASTA_DO_TIPO` de `vista-ciclos.ts`.
//!
//! O teste `tests/nomes_ts.rs` compara com saídas geradas pelo próprio TypeScript (Bun).

use unicode_normalization::UnicodeNormalization;

/// Marca combinante que o `\p{Diacritic}` do JavaScript tira depois do NFD.
fn e_diacritico(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}')
}

/// Trecho seguro para nome de arquivo: sem acento, sem barra, minúsculo, com no máximo
/// `max` caracteres e sem hífen no fim. Igual a `trechoDeNome` do TypeScript.
#[must_use]
pub fn trecho_de_nome(texto: Option<&str>, max: usize) -> String {
    let Some(texto) = texto else {
        return String::new();
    };
    let sem_acento: String = texto.nfd().filter(|c| !e_diacritico(*c)).collect();
    let minusculo = sem_acento.to_lowercase();
    // `[^a-z0-9]+` vira um hífen só.
    let mut trecho = String::with_capacity(minusculo.len());
    for c in minusculo.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            trecho.push(c);
        } else if !trecho.ends_with('-') {
            trecho.push('-');
        }
    }
    // `^-+|-+$`, o corte em `max` (tudo já é ASCII) e de novo o hífen do fim.
    let aparado = trecho.trim_matches('-');
    let cortado: String = aparado.chars().take(max).collect();
    cortado.trim_end_matches('-').to_owned()
}

/// Trecho com o limite padrão de 40 caracteres.
#[must_use]
pub fn trecho_padrao(texto: Option<&str>) -> String {
    trecho_de_nome(texto, 40)
}

/// O que o nome do arquivo usa de um achado.
#[derive(Debug, Clone, Copy)]
pub struct DadosDoNome<'a> {
    /// Data-chave `AAAA-MM-DD`.
    pub data_chave: Option<&'a str>,
    /// Fornecedor como apareceu.
    pub fornecedor: Option<&'a str>,
    /// Valor em centavos (o sinal é ignorado).
    pub valor_centavos: Option<i64>,
    /// Hash do conteúdo (os 8 primeiros caracteres entram no nome).
    pub content_hash: &'a str,
    /// Extensão sem ponto.
    pub extensao: Option<&'a str>,
    /// Nome do primeiro arquivo de origem, usado se não há fornecedor.
    pub nome_original: Option<&'a str>,
}

/// `\.[^.]+$`: tira a última extensão do nome.
fn sem_extensao(nome: &str) -> &str {
    match nome.rfind('.') {
        Some(posicao) if posicao + 1 < nome.len() => &nome[..posicao],
        _ => nome,
    }
}

/// Os primeiros 8 caracteres do hash (os `slice(0, 8)` do TypeScript contam UTF-16, mas
/// o hash é hexadecimal).
fn oito(hash: &str) -> String {
    hash.chars().take(8).collect()
}

/// `data_fornecedor_valor_hash8.ext`. Igual a `nomeNaVista` do TypeScript.
#[must_use]
pub fn nome_na_vista(d: &DadosDoNome<'_>) -> String {
    let fornecedor = {
        let direto = trecho_padrao(d.fornecedor);
        if direto.is_empty() {
            trecho_padrao(d.nome_original.map(sem_extensao))
        } else {
            direto
        }
    };
    let valor = d.valor_centavos.map(|centavos| {
        let absoluto = centavos.unsigned_abs();
        format!("{},{:02}", absoluto / 100, absoluto % 100)
    });
    let partes: Vec<String> = [
        d.data_chave.map(str::to_owned),
        Some(fornecedor),
        valor,
        Some(oito(d.content_hash)),
    ]
    .into_iter()
    .flatten()
    .filter(|p| !p.is_empty())
    .collect();
    let extensao = d
        .extensao
        .filter(|e| !e.is_empty())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    format!("{}{extensao}", partes.join("_"))
}

/// Pasta do tipo de documento (`PASTA_DO_TIPO` do `vista-ciclos.ts`); o resto é `outros`.
#[must_use]
pub fn pasta_do_tipo(tipo: Option<&str>) -> &'static str {
    match tipo.unwrap_or("") {
        "fatura" => "faturas",
        "boleto" => "boletos",
        "comprovante" => "comprovantes",
        "extrato" => "extratos",
        "nota_fiscal" | "nfe" => "notas-fiscais",
        "contracheque" => "contracheques",
        "informe_rendimentos" => "informes-de-rendimentos",
        "irpf" => "irpf",
        "contrato" => "contratos",
        "recibo" => "recibos",
        "pedido" => "pedidos",
        "cobranca" => "cobrancas",
        _ => "outros",
    }
}

/// Pasta da estante onde o documento mora: `<ciclo>/<tipo>` ou `a-classificar/<tipo>`.
#[must_use]
pub fn pasta_na_estante(ciclo: Option<&str>, tipo: Option<&str>) -> String {
    let raiz = ciclo.filter(|c| !c.is_empty()).unwrap_or("a-classificar");
    format!("{raiz}/{}", pasta_do_tipo(tipo))
}
