//! Gera um `inventario.sqlite` fictício em escala, para medir e testar o núcleo (spec 06).
//!
//! Uso (o caminho é novo: o exemplo recusa sobrescrever e recusa qualquer caminho sob
//! `/mnt/d/Chico/financas`, onde ficam o cofre e o inventário real):
//!
//! ```text
//! cargo run -p sbf-inventario --example gerar_escala -- /caminho/novo/inventario.sqlite
//! ```
//!
//! Testes: `cargo test -p sbf-inventario --example gerar_escala` gera a escala reduzida
//! (200 registros), confere as contagens e confere que `reprojetar` não muda a projeção.
//!
//! O que gera (tudo inventado: fornecedores, valores e datas; sem CPF; semente fixa, então
//! o mesmo comando dá o mesmo banco):
//! - 12.000 registros (compra, pagamento, credito, tarifa, salario, estorno), datas nos
//!   últimos 24 meses e ~10% de duplicatas por `content_hash`;
//! - 3.000 achados com texto extraído: boletos, faturas, notas, recibos e comprovantes,
//!   mais lixo não financeiro (cardápio, convite, promoção) para a triagem distinguir;
//! - 80 fornecedores inventados, com grafias variantes (`Mercado Bom Preço`,
//!   `MERC BOM PRECO`, `MERCADO BOM PRECO LTDA ME`);
//! - ~800 decisões do usuário no formato JSON que o núcleo grava (as formas de
//!   `tests/comum/mod.rs`), com algumas desfeitas por uma decisão nova;
//! - ao fim, `Inventario::reprojetar`, que tem de passar.
//!
//! O banco nasce como o Bun o deixa (esquema da migração 1, colunas do `garantirColuna` e
//! `meta.schema_versao = '2'`) e é levado ao esquema do Rust por `Inventario::abrir` e
//! `migrar`, como o app faz.

use std::error::Error;
use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, Transaction, params};
use sbf_inventario::banco::Inventario;
use sbf_inventario::esquema::MIGRACAO_1_BASE_BUN;
use sbf_inventario::projecao::ResumoDaReprojecao;
use serde_json::{Value, json};

type Resultado<T> = Result<T, Box<dyn Error>>;

/// Semente fixa: o mesmo comando gera o mesmo banco.
const SEMENTE: u64 = 0x5BF0_2026_1114;
/// O cofre real. Nada gerado aqui pode cair dentro dele.
const COFRE: &str = "/mnt/d/Chico/financas";
/// "Hoje" da fixture, em (ano, mês, dia): as datas contam para trás a partir daqui.
const HOJE: (i64, i64, i64) = (2026, 11, 14);

/// `(id, tipo, descrição, raiz)` das fontes. A ordem é o índice usado no plano.
const FONTES: [(&str, &str, &str, Option<&str>); 4] = [
    (
        "disco:e2",
        "disco",
        "Disco E2 (HD externo)",
        Some("/dados/e2"),
    ),
    ("gmail", "email", "Caixa de e-mail principal", None),
    ("onedrive", "nuvem", "OneDrive pessoal", None),
    ("conexao:pluggy", "banco", "Open Finance (Pluggy)", None),
];
const FONTE_DISCO: usize = 0;
const FONTE_GMAIL: usize = 1;
const FONTE_ONEDRIVE: usize = 2;
const FONTE_PLUGGY: usize = 3;

const CONTAS: [&str; 3] = ["Conta Corrente Alfa", "Cartão Beta", "Conta Reserva Gama"];
const CATEGORIAS_DO_USUARIO: [&str; 5] = ["Alimentação", "Saúde", "Escritório", "Lazer", "Casa"];
const TIPOS_DOCUMENTO: [&str; 5] = ["boleto", "fatura", "nota", "recibo", "comprovante"];
/// Os primeiros fornecedores são os que o usuário categoriza nas decisões.
const FORNECEDORES_DECIDIDOS: usize = 12;
const EMPREGADOR: &str = "Empresa Empregadora Exemplo";
const TEXTOS_DE_LIXO: [&str; 3] = [
    "Cardápio do dia\nPrato executivo com arroz, feijão e salada\nSobremesa: pudim de leite\nCouvert artístico R$ 12,00",
    "Você está convidado para a festa de aniversário de Convidada Fictícia\nSábado, 20h, salão de festas",
    "Promoção de inverno: 30% de desconto em toda a loja até domingo",
];

/// Quantos itens gerar. Os outros números saem daqui (lixo, duplicatas, Open Finance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escala {
    pub registros: usize,
    pub achados: usize,
    pub decisoes: usize,
}

/// A escala do enunciado: 12.000 registros, 3.000 achados, ~800 decisões.
pub const ESCALA_COMPLETA: Escala = Escala {
    registros: 12_000,
    achados: 3_000,
    decisoes: 800,
};

/// A escala reduzida dos testes: ~200 itens.
#[cfg(test)]
const ESCALA_DE_TESTE: Escala = Escala {
    registros: 200,
    achados: 50,
    decisoes: 24,
};

/// O que foi gravado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contagens {
    pub registros: usize,
    pub achados: usize,
    pub textos: usize,
    pub origens: usize,
    pub decisoes: usize,
}

// ---------------------------------------------------------------------------------------
// Sorteio determinístico (SplitMix64): sem dependência nova.

struct Sorteio(u64);

impl Sorteio {
    fn proximo(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Um inteiro em `[a, b)`.
    fn entre(&mut self, a: usize, b: usize) -> usize {
        a + (self.proximo() % (b - a) as u64) as usize
    }

    /// Um valor em centavos em `[a, b)`.
    fn valor(&mut self, a: i64, b: i64) -> i64 {
        a + (self.proximo() % (b - a) as u64) as i64
    }

    /// Verdadeiro em `por_cento` de cada 100 sorteios.
    fn chance(&mut self, por_cento: u64) -> bool {
        self.proximo() % 100 < por_cento
    }

    fn escolha<'a, T>(&mut self, itens: &'a [T]) -> &'a T {
        &itens[self.entre(0, itens.len())]
    }

    /// Um hash fictício: 64 caracteres hexadecimais.
    fn hash(&mut self) -> String {
        (0..4).map(|_| format!("{:016x}", self.proximo())).collect()
    }
}

// ---------------------------------------------------------------------------------------
// Datas (calendário civil, sem dependência de tempo).

fn dias_de_civil(ano: i64, mes: i64, dia: i64) -> i64 {
    let ano = if mes <= 2 { ano - 1 } else { ano };
    let era = if ano >= 0 { ano } else { ano - 399 } / 400;
    let yoe = ano - era * 400;
    let mp = (mes + 9) % 12;
    let doy = (153 * mp + 2) / 5 + dia - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_de_dias(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    (ano, mes, dia)
}

/// Uma data ISO (`AAAA-MM-DD`) `dias_atras` dias antes de `HOJE`.
fn data_ha(dias_atras: i64) -> String {
    let (ano, mes, dia) = civil_de_dias(dias_de_civil(HOJE.0, HOJE.1, HOJE.2) - dias_atras);
    format!("{ano:04}-{mes:02}-{dia:02}")
}

/// `AAAA-MM-DD` para `DD/MM/AAAA`, como o boleto escreve.
fn dd_mm_aaaa(data: &str) -> String {
    format!("{}/{}/{}", &data[8..10], &data[5..7], &data[0..4])
}

/// O ciclo (`AAAA-MM` do início) de uma data ISO, com o dia de corte 20.
fn ciclo_de(data: &str) -> String {
    let ano: i64 = data[0..4].parse().unwrap_or(0);
    let mes: i64 = data[5..7].parse().unwrap_or(1);
    let dia: i64 = data[8..10].parse().unwrap_or(1);
    let (ano, mes) = match (dia < 20, mes) {
        (true, 1) => (ano - 1, 12),
        (true, _) => (ano, mes - 1),
        (false, _) => (ano, mes),
    };
    format!("{ano:04}-{mes:02}")
}

/// Centavos positivos como `R$` escreve: `1875` vira `18,75`.
fn reais(centavos: i64) -> String {
    let c = centavos.abs();
    format!("{},{:02}", c / 100, c % 100)
}

/// O instante de uma decisão, em ordem crescente com o índice (os ids também sobem).
fn em_da_decisao(i: usize, total: usize) -> String {
    let dias = 720 - (i * 700 / total.max(1)) as i64;
    format!("{}T{:02}:{:02}:00.000Z", data_ha(dias), 9 + i % 10, i % 60)
}

// ---------------------------------------------------------------------------------------
// Fornecedores inventados.

struct Fornecedor {
    canonico: String,
    /// Grafias como elas chegam: a caixa alta do extrato, a sigla, o "LTDA ME" do boleto.
    variantes: [String; 4],
    categoria: &'static str,
}

fn fornecedores() -> Vec<Fornecedor> {
    const MARCAS: [&str; 20] = [
        "Bom Preço",
        "Delta",
        "Estrela Azul",
        "Vale Verde",
        "Ômega Mini",
        "Zeta Plus",
        "Alfa Norte",
        "Sol Nascente",
        "Pico Alto",
        "Rio Claro",
        "Gama Fina",
        "Épsilon Lar",
        "Lua Cheia",
        "Nova Aurora",
        "Beta Sul",
        "Porto Fino",
        "Cedro Real",
        "Torre Leste",
        "Ipê Dourado",
        "Serra Mar",
    ];
    const TIPOS: [(&str, &str, &str); 4] = [
        ("Mercado", "MERC", "Alimentação"),
        ("Padaria", "PADA", "Alimentação"),
        ("Farmácia", "FARM", "Saúde"),
        ("Papelaria", "PAPEL", "Escritório"),
    ];
    let mut saida = Vec::with_capacity(MARCAS.len() * TIPOS.len());
    for (tipo, abreviado, categoria) in TIPOS {
        for marca in MARCAS {
            let caixa = marca.to_uppercase();
            saida.push(Fornecedor {
                canonico: format!("{tipo} {marca}"),
                variantes: [
                    format!("{tipo} {marca} LTDA"),
                    format!("{abreviado} {caixa}"),
                    format!("{} {caixa} LTDA ME", tipo.to_uppercase()),
                    format!("{tipo} {marca}"),
                ],
                categoria,
            });
        }
    }
    saida
}

// ---------------------------------------------------------------------------------------
// Plano: tudo decidido em memória antes de tocar no banco.

struct AchadoPlano {
    hash: String,
    fonte: usize,
    segunda_fonte: Option<usize>,
    extensao: &'static str,
    mime: &'static str,
    tamanho: i64,
    estado: &'static str,
    financeiro: bool,
    tipo_doc: &'static str,
    data_chave_tipo: Option<&'static str>,
    protegido: bool,
    fornecedor: Option<usize>,
    raw: Option<String>,
    data: Option<String>,
    valor: Option<i64>,
    metodo: &'static str,
    texto: String,
    /// Data ISO da primeira vez que o achado foi visto.
    visto: String,
    decidido: bool,
}

#[derive(Clone)]
struct RegistroPlano {
    hash: Option<String>,
    external_ref: Option<String>,
    fonte: usize,
    tipo: &'static str,
    data: String,
    data_tipo: &'static str,
    valor: i64,
    descricao: String,
    raw: Option<String>,
    fornecedor: Option<usize>,
    normalizado: Option<String>,
    categoria: Option<String>,
    conta: &'static str,
    confianca: f64,
    extraido_por: &'static str,
    estado: &'static str,
    movimentacao: Option<String>,
    decidido: bool,
}

struct OrigemPlano {
    hash: String,
    fonte: usize,
    chave: String,
    tipo: &'static str,
    caminho: Option<String>,
    mtime: Option<String>,
    nome: String,
    tamanho: i64,
    mensagem: Option<String>,
    de: Option<String>,
    assunto: Option<String>,
    data_msg: Option<String>,
    mensagem_ref: Option<&'static str>,
    visto: String,
}

/// `(tipo, valor, descrição, categoria, grafia, normalizado, índice do fornecedor)` de uma linha do Open Finance.
type LinhaPluggy = (
    &'static str,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<usize>,
);

struct DecisaoPlano {
    alvo_tipo: &'static str,
    alvo_id: String,
    campo: &'static str,
    antes: Value,
    depois: Value,
    desfaz: Option<i64>,
    motivo: Option<&'static str>,
}

struct Plano {
    achados: Vec<AchadoPlano>,
    origens: Vec<OrigemPlano>,
    registros: Vec<RegistroPlano>,
    decisoes: Vec<DecisaoPlano>,
}

fn faixa_de_valor(tipo: &str) -> (i64, i64) {
    match tipo {
        "boleto" => (5_000, 250_000),
        "fatura" => (10_000, 300_000),
        "nota" => (1_000, 80_000),
        "recibo" => (2_000, 50_000),
        _ => (5_000, 300_000),
    }
}

fn titulo(tipo: &str) -> &'static str {
    match tipo {
        "boleto" => "Boleto",
        "fatura" => "Fatura",
        "nota" => "Nota fiscal",
        "recibo" => "Recibo",
        _ => "Comprovante",
    }
}

fn tipo_do_registro(tipo_doc: &str) -> &'static str {
    if tipo_doc == "nota" {
        "compra"
    } else {
        "pagamento"
    }
}

fn data_tipo_do_documento(tipo_doc: &str) -> Option<&'static str> {
    match tipo_doc {
        "boleto" | "fatura" => Some("vencimento"),
        "nota" => Some("compra"),
        _ => Some("pagamento"),
    }
}

fn texto_do_documento(tipo: &str, nome: &str, data: &str, centavos: i64) -> String {
    let venc = dd_mm_aaaa(data);
    let valor = reais(centavos);
    match tipo {
        "boleto" => format!(
            "Boleto de cobrança\nBeneficiário: {nome}\nVencimento: {venc}\nValor do documento: R$ {valor}\nLinha digitável: 00000.00000 00000.000000 00000.000000 1 00000000000000"
        ),
        "fatura" => {
            format!("Fatura de serviço\n{nome}\nVencimento: {venc}\nTotal a pagar R$ {valor}")
        }
        "nota" => format!(
            "NOTA FISCAL DE CONSUMIDOR ELETRÔNICA\n{nome}\nData de emissão: {venc}\nValor total R$ {valor}"
        ),
        "recibo" => format!(
            "Recibo\nRecebemos de cliente fictício a quantia de R$ {valor}\nReferente a: {nome}\nData: {venc}"
        ),
        _ => format!(
            "Comprovante de transferência\nFavorecido: {nome}\nData: {venc}\nValor: R$ {valor}"
        ),
    }
}

fn planejar_achados(rng: &mut Sorteio, n: usize, forn: &[Fornecedor]) -> Vec<AchadoPlano> {
    // 70% financeiros; um em cada sete deles fica "novo" (ainda na triagem, sem registro).
    let n_fin = n * 7 / 10;
    let n_novo_fin = n_fin / 7;
    let mut saida = Vec::with_capacity(n);
    for i in 0..n {
        let hash = rng.hash();
        let visto = data_ha(rng.entre(0, 730) as i64);
        let fonte = if rng.chance(70) {
            FONTE_DISCO
        } else {
            FONTE_GMAIL
        };
        if i < n_fin {
            let tipo = *rng.escolha(&TIPOS_DOCUMENTO);
            let f = rng.entre(0, forn.len());
            let raw = forn[f].variantes[rng.entre(0, 4)].clone();
            let data = data_ha(rng.entre(0, 730) as i64);
            let (min, max) = faixa_de_valor(tipo);
            let valor = -rng.valor(min, max);
            let texto = texto_do_documento(tipo, &raw, &data, valor.abs());
            let segunda = rng.chance(25).then_some(if fonte == FONTE_DISCO {
                FONTE_GMAIL
            } else {
                FONTE_DISCO
            });
            saida.push(AchadoPlano {
                hash,
                fonte,
                segunda_fonte: segunda,
                extensao: "pdf",
                mime: "application/pdf",
                tamanho: rng.valor(20_000, 900_000),
                estado: if i < n_novo_fin { "novo" } else { "documento" },
                financeiro: true,
                tipo_doc: tipo,
                data_chave_tipo: data_tipo_do_documento(tipo),
                protegido: rng.chance(3),
                fornecedor: Some(f),
                raw: Some(raw),
                data: Some(data),
                valor: Some(valor),
                metodo: if rng.chance(20) {
                    "ocr-por"
                } else {
                    "pdftotext"
                },
                texto,
                visto,
                decidido: false,
            });
        } else {
            let k = i - n_fin;
            saida.push(AchadoPlano {
                hash,
                fonte,
                segunda_fonte: None,
                extensao: "jpg",
                mime: "image/jpeg",
                tamanho: rng.valor(20_000, 900_000),
                estado: if k.is_multiple_of(2) {
                    "novo"
                } else if k.is_multiple_of(3) {
                    "duvida"
                } else {
                    "descartado"
                },
                financeiro: false,
                tipo_doc: "outro",
                data_chave_tipo: None,
                protegido: false,
                fornecedor: None,
                raw: None,
                data: None,
                valor: None,
                metodo: "ocr-por",
                texto: (*rng.escolha(&TEXTOS_DE_LIXO)).to_owned(),
                visto,
                decidido: false,
            });
        }
    }
    saida
}

fn origem_de(n: usize, a: &AchadoPlano, fonte: usize) -> OrigemPlano {
    let ext = a.extensao;
    let visto = format!("{}T12:00:00.000Z", a.visto);
    let data = a.data.clone().unwrap_or_else(|| a.visto.clone());
    match fonte {
        FONTE_GMAIL => OrigemPlano {
            hash: a.hash.clone(),
            fonte,
            chave: format!("gmail:m{n:06}:h{}", &a.hash[..8]),
            tipo: "gmail",
            caminho: None,
            mtime: None,
            nome: format!("anexo-{n:06}.{ext}"),
            tamanho: a.tamanho,
            mensagem: Some(format!("m{n:06}")),
            de: Some(format!(
                "{} <nf@exemplo.invalid>",
                a.raw.as_deref().unwrap_or("Remetente Exemplo")
            )),
            assunto: Some("Seu comprovante".to_owned()),
            data_msg: Some(format!("{data}T19:00:00Z")),
            mensagem_ref: Some("anexo"),
            visto,
        },
        FONTE_ONEDRIVE => {
            let caminho = format!("onedrive:gerado/{n:06}.{ext}");
            OrigemPlano {
                hash: a.hash.clone(),
                fonte,
                chave: caminho.clone(),
                tipo: "onedrive",
                caminho: Some(caminho),
                mtime: None,
                nome: format!("{n:06}.{ext}"),
                tamanho: a.tamanho,
                mensagem: None,
                de: None,
                assunto: None,
                data_msg: None,
                mensagem_ref: None,
                visto,
            }
        }
        _ => {
            let mtime_ms = 1_700_000_000_000_i64 + n as i64 * 1000;
            let caminho = format!("/dados/e2/gerado/{n:06}.{ext}");
            OrigemPlano {
                hash: a.hash.clone(),
                fonte,
                chave: format!("local:{caminho}:{mtime_ms}"),
                tipo: "local_file",
                caminho: Some(caminho),
                mtime: Some(format!("{data}T10:00:00Z")),
                nome: format!("{n:06}.{ext}"),
                tamanho: a.tamanho,
                mensagem: None,
                de: None,
                assunto: None,
                data_msg: None,
                mensagem_ref: None,
                visto,
            }
        }
    }
}

fn planejar_origens(achados: &[AchadoPlano]) -> Vec<OrigemPlano> {
    let mut saida = Vec::with_capacity(achados.len() + achados.len() / 4);
    for a in achados {
        let n = saida.len() + 1;
        saida.push(origem_de(n, a, a.fonte));
        if let Some(segunda) = a.segunda_fonte {
            let n = saida.len() + 1;
            saida.push(origem_de(n, a, segunda));
        }
    }
    saida
}

fn estado_sorteado(rng: &mut Sorteio) -> &'static str {
    match rng.entre(0, 100) {
        0..35 => "extraido",
        35..60 => "conferido",
        60..75 => "aprovado",
        75..80 => "divergente",
        _ => "promovido",
    }
}

fn planejar_registros(
    rng: &mut Sorteio,
    escala: Escala,
    achados: &[AchadoPlano],
    forn: &[Fornecedor],
) -> Vec<RegistroPlano> {
    // Documentos: um registro por achado financeiro já documentado.
    let mut registros: Vec<RegistroPlano> = Vec::new();
    for a in achados
        .iter()
        .filter(|a| a.financeiro && a.estado == "documento")
    {
        let normalizado = a.fornecedor.map(|i| forn[i].canonico.clone());
        registros.push(RegistroPlano {
            hash: Some(a.hash.clone()),
            external_ref: None,
            fonte: a.fonte,
            tipo: tipo_do_registro(a.tipo_doc),
            data: a.data.clone().unwrap_or_default(),
            data_tipo: a.data_chave_tipo.unwrap_or("pagamento"),
            valor: a.valor.unwrap_or(0),
            descricao: format!("{} {}", titulo(a.tipo_doc), a.raw.as_deref().unwrap_or("")),
            raw: a.raw.clone(),
            fornecedor: a.fornecedor,
            categoria: a.fornecedor.map(|i| forn[i].categoria.to_owned()),
            normalizado,
            conta: CONTAS[rng.entre(0, CONTAS.len())],
            confianca: 0.9,
            extraido_por: "extrator@1",
            estado: estado_sorteado(rng),
            movimentacao: None,
            decidido: false,
        });
    }

    // Duplicatas: cópias de um documento, com o mesmo content_hash (~10% do total).
    let base = registros.len();
    for _ in 0..escala.registros / 10 {
        if base == 0 {
            break;
        }
        let origem = registros[rng.entre(0, base)].clone();
        registros.push(RegistroPlano {
            external_ref: None,
            fonte: FONTE_GMAIL,
            estado: "extraido",
            movimentacao: None,
            decidido: false,
            ..origem
        });
    }

    // Open Finance: o resto, com fornecedores do pool, salário e tarifa.
    let n_pluggy = escala.registros.saturating_sub(registros.len());
    for k in 0..n_pluggy {
        let f = rng.entre(0, forn.len());
        let pool = &forn[f];
        let variante = pool.variantes[rng.entre(0, 4)].clone();
        let data = data_ha(rng.entre(0, 730) as i64);
        let sorteio = rng.entre(0, 100);
        let (tipo, valor, descricao, categoria, raw, normalizado, fornecedor): LinhaPluggy =
            match sorteio {
                0..40 => (
                    "compra",
                    -rng.valor(1_000, 60_000),
                    format!("Compra {variante}"),
                    (!rng.chance(10)).then(|| pool.categoria.to_owned()),
                    Some(variante),
                    Some(pool.canonico.clone()),
                    Some(f),
                ),
                40..65 => (
                    "pagamento",
                    -rng.valor(3_000, 300_000),
                    format!("Pagamento {variante}"),
                    (!rng.chance(10)).then(|| pool.categoria.to_owned()),
                    Some(variante),
                    Some(pool.canonico.clone()),
                    Some(f),
                ),
                65..70 => (
                    "tarifa",
                    -rng.valor(990, 4_990),
                    format!("Tarifa mensal {variante}"),
                    Some("Tarifas".to_owned()),
                    Some(variante),
                    Some(pool.canonico.clone()),
                    Some(f),
                ),
                70..80 => (
                    "credito",
                    rng.valor(1_000, 200_000),
                    format!("Crédito {variante}"),
                    Some(pool.categoria.to_owned()),
                    Some(variante),
                    Some(pool.canonico.clone()),
                    Some(f),
                ),
                80..85 => (
                    "estorno",
                    rng.valor(500, 30_000),
                    format!("Estorno {variante}"),
                    Some(pool.categoria.to_owned()),
                    Some(variante),
                    Some(pool.canonico.clone()),
                    Some(f),
                ),
                _ => (
                    "salario",
                    rng.valor(300_000, 900_000),
                    "Salário".to_owned(),
                    Some("Salário".to_owned()),
                    Some(EMPREGADOR.to_owned()),
                    Some(EMPREGADOR.to_owned()),
                    None,
                ),
            };
        let conta = if tipo == "compra" {
            "Cartão Beta"
        } else if tipo == "salario" {
            "Conta Corrente Alfa"
        } else {
            *rng.escolha(&CONTAS)
        };
        registros.push(RegistroPlano {
            hash: None,
            external_ref: Some(format!("pluggy:txn_{:06}", k + 1)),
            fonte: FONTE_PLUGGY,
            tipo,
            data,
            data_tipo: if tipo == "compra" {
                "compra"
            } else {
                "pagamento"
            },
            valor,
            descricao,
            raw,
            fornecedor,
            normalizado,
            categoria,
            conta,
            confianca: 0.95,
            extraido_por: "pluggy@1",
            estado: estado_sorteado(rng),
            movimentacao: None,
            decidido: false,
        });
    }

    for (i, r) in registros.iter_mut().enumerate() {
        if r.estado == "promovido" && r.movimentacao.is_none() {
            r.movimentacao = Some(format!("mov-{:06}", i + 1));
        }
    }
    registros
}

/// O log de decisões: categoria de registros dos fornecedores decididos (às vezes com uma
/// desfeita), triagem de achados e recorrência de fornecedores. Cada alvo aparece uma vez
/// nas categorias, para que a projeção final seja a do próprio log.
fn planejar_decisoes(
    rng: &mut Sorteio,
    total: usize,
    registros: &mut [RegistroPlano],
    achados: &mut [AchadoPlano],
    forn: &[Fornecedor],
) -> Vec<DecisaoPlano> {
    let mut decisoes: Vec<DecisaoPlano> = Vec::with_capacity(total);
    let mut reg_candidatos: Vec<usize> = (0..registros.len())
        .filter(|&i| {
            registros[i]
                .fornecedor
                .is_some_and(|f| f < FORNECEDORES_DECIDIDOS)
        })
        .collect();
    let mut ach_candidatos: Vec<usize> = (0..achados.len())
        .filter(|&i| achados[i].estado == "novo")
        .collect();

    while decisoes.len() < total {
        let falta = total - decisoes.len();
        let sorteio = rng.entre(0, 100);
        if sorteio < 70 && !reg_candidatos.is_empty() {
            let pos = rng.entre(0, reg_candidatos.len());
            let idx = reg_candidatos.swap_remove(pos);
            let nova = *rng.escolha(&CATEGORIAS_DO_USUARIO);
            decisoes.push(DecisaoPlano {
                alvo_tipo: "registro",
                alvo_id: (idx + 1).to_string(),
                campo: "categoria",
                antes: json!({"categoria_provavel": registros[idx].categoria}),
                depois: json!({"categoria_provavel": nova}),
                desfaz: None,
                motivo: (sorteio.is_multiple_of(10)).then_some("ajuste da categoria pelo usuário"),
            });
            registros[idx].categoria = Some(nova.to_owned());
            registros[idx].decidido = true;
        } else if sorteio < 78 && falta >= 2 && !reg_candidatos.is_empty() {
            // Categoria trocada e depois desfeita: o registro volta ao valor de antes e
            // deixa de ser decidido pelo usuário (o log não tem decisão vigente sobre ele).
            let pos = rng.entre(0, reg_candidatos.len());
            let idx = reg_candidatos.swap_remove(pos);
            let original = registros[idx].categoria.clone();
            let nova = *rng.escolha(&CATEGORIAS_DO_USUARIO);
            let ida = decisoes.len() as i64 + 1;
            decisoes.push(DecisaoPlano {
                alvo_tipo: "registro",
                alvo_id: (idx + 1).to_string(),
                campo: "categoria",
                antes: json!({"categoria_provavel": original}),
                depois: json!({"categoria_provavel": nova}),
                desfaz: None,
                motivo: None,
            });
            decisoes.push(DecisaoPlano {
                alvo_tipo: "registro",
                alvo_id: (idx + 1).to_string(),
                campo: "categoria",
                antes: json!({"categoria_provavel": nova}),
                depois: json!({"categoria_provavel": original}),
                desfaz: Some(ida),
                motivo: Some("desfeita"),
            });
        } else if sorteio < 90 && !ach_candidatos.is_empty() {
            let pos = rng.entre(0, ach_candidatos.len());
            let idx = ach_candidatos.swap_remove(pos);
            let depois = if achados[idx].financeiro {
                "documento"
            } else {
                "descartado"
            };
            decisoes.push(DecisaoPlano {
                alvo_tipo: "achado",
                alvo_id: achados[idx].hash.clone(),
                campo: "estado",
                antes: json!({"estado": "novo"}),
                depois: json!({"estado": depois}),
                desfaz: None,
                motivo: (sorteio.is_multiple_of(3)).then_some("conferido na triagem"),
            });
            achados[idx].estado = depois;
            achados[idx].decidido = true;
        } else {
            let f = rng.entre(0, forn.len());
            decisoes.push(DecisaoPlano {
                alvo_tipo: "fornecedor",
                alvo_id: forn[f].canonico.clone(),
                campo: "recorrente",
                antes: json!({"recorrente": false}),
                depois: json!({"recorrente": true}),
                desfaz: None,
                motivo: None,
            });
        }
    }
    decisoes
}

fn planejar(escala: Escala) -> Plano {
    let mut rng = Sorteio(SEMENTE);
    let forn = fornecedores();
    let mut achados = planejar_achados(&mut rng, escala.achados, &forn);
    let origens = planejar_origens(&achados);
    let mut registros = planejar_registros(&mut rng, escala, &achados, &forn);
    let decisoes = planejar_decisoes(
        &mut rng,
        escala.decisoes,
        &mut registros,
        &mut achados,
        &forn,
    );
    Plano {
        achados,
        origens,
        registros,
        decisoes,
    }
}

// ---------------------------------------------------------------------------------------
// Gravação.

/// O que o Bun deixa num banco "2": a migração 1, as colunas do `garantirColuna` e a versão.
fn criar_banco_do_bun(destino: &Path) -> Resultado<()> {
    let conexao = Connection::open(destino)?;
    conexao.execute_batch(MIGRACAO_1_BASE_BUN)?;
    conexao.execute_batch(
        "ALTER TABLE registro ADD COLUMN tipo_original TEXT;
         ALTER TABLE registro ADD COLUMN contraparte TEXT;
         ALTER TABLE registro ADD COLUMN contraparte_doc TEXT;
         ALTER TABLE registro ADD COLUMN data_compra TEXT;
         ALTER TABLE registro ADD COLUMN merchant_cnpj TEXT;
         ALTER TABLE registro ADD COLUMN fatura_ref TEXT;
         INSERT OR REPLACE INTO meta (chave, valor) VALUES ('schema_versao', '2');",
    )?;
    Ok(())
}

fn inserir_fontes_e_varreduras(tx: &Transaction<'_>, plano: &Plano) -> Resultado<()> {
    for (id, tipo, descricao, raiz) in FONTES {
        tx.execute(
            "INSERT INTO fonte (id, tipo, descricao, raiz, escopo, motivo, criada_em)
             VALUES (?1, ?2, ?3, ?4, 'dentro', NULL, '2024-11-01T09:00:00.000Z')",
            params![id, tipo, descricao, raiz],
        )?;
    }
    for (i, (id, _, _, _)) in FONTES.iter().enumerate() {
        let vistos = plano.origens.iter().filter(|o| o.fonte == i).count() as i64;
        tx.execute(
            "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel, iniciada_em, concluida_em,
               status, vistos, novos, repetidos, excluidos, erros)
             VALUES (?1, ?2, 'E2', 'raiz', 'coletar-fonte@1', '2024-11-01T08:00:00.000Z',
               '2024-11-01T09:00:00.000Z', 'concluida', ?3, ?3, 0, 0, 0)",
            params![i as i64 + 1, id, vistos],
        )?;
    }
    Ok(())
}

fn inserir_ciclos(tx: &Transaction<'_>) -> Resultado<()> {
    for ano in 2024..=2026 {
        for mes in 1..=12 {
            let (ano_fim, mes_fim) = if mes == 12 {
                (ano + 1, 1)
            } else {
                (ano, mes + 1)
            };
            tx.execute(
                "INSERT INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
                params![
                    format!("{ano:04}-{mes:02}"),
                    format!("{ano:04}-{mes:02}-20"),
                    format!("{ano_fim:04}-{mes_fim:02}-19"),
                ],
            )?;
        }
    }
    Ok(())
}

fn inserir_achados(tx: &Transaction<'_>, plano: &Plano) -> Resultado<()> {
    for a in &plano.achados {
        let ciclo = a.data.as_deref().map(ciclo_de);
        tx.prepare_cached(
            "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada, caminho_acervo,
               estado, document_type, protegido_senha, canonical_fingerprint, supplier_name_raw,
               data_chave, data_chave_tipo, ciclo_id, valor_centavos, confianca, classificado_por,
               primeiro_visto_em, notas, decidido_por)
             VALUES (?1, ?2, ?3, ?4, 'B', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0.9,
               'regras@2', ?15, NULL, ?16)",
        )?
        .execute(params![
            a.hash,
            a.tamanho,
            a.mime,
            a.extensao,
            format!("acervo/{}/{}.{}", &a.hash[0..2], &a.hash[2..14], a.extensao),
            a.estado,
            a.tipo_doc,
            i64::from(a.protegido),
            format!("fp-{}", &a.hash[..16]),
            a.raw,
            a.data,
            a.data_chave_tipo,
            ciclo,
            a.valor,
            format!("{}T12:00:00.000Z", a.visto),
            if a.decidido { "usuario" } else { "regras@2" },
        ])?;
        tx.prepare_cached(
            "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![a.hash, a.metodo, a.texto])?;
    }
    Ok(())
}

fn inserir_origens(tx: &Transaction<'_>, plano: &Plano) -> Resultado<()> {
    for o in &plano.origens {
        tx.prepare_cached(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath,
               local_mtime, filename, file_size_bytes, gmail_message_id, gmail_from, gmail_subject,
               gmail_date, mensagem_ref, visto_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?
        .execute(params![
            o.hash,
            FONTES[o.fonte].0,
            o.fonte as i64 + 1,
            o.tipo,
            o.chave,
            o.caminho,
            o.mtime,
            o.nome,
            o.tamanho,
            o.mensagem,
            o.de,
            o.assunto,
            o.data_msg,
            o.mensagem_ref,
            o.visto,
        ])?;
    }
    Ok(())
}

fn inserir_registros(tx: &Transaction<'_>, plano: &Plano) -> Resultado<()> {
    for (i, r) in plano.registros.iter().enumerate() {
        let id = i as i64 + 1;
        let compra = r.tipo == "compra";
        tx.prepare_cached(
            "INSERT INTO registro (id, content_hash, external_ref, fonte_id, tipo, data, data_tipo, ciclo_id,
               valor_centavos, descricao_original, supplier_name_raw, fornecedor_normalizado,
               categoria_provavel, conta_provavel, confianca, extraido_por, estado, movimentacao_id,
               decidido_por, data_compra)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        )?
        .execute(params![
            id,
            r.hash,
            r.external_ref,
            FONTES[r.fonte].0,
            r.tipo,
            r.data,
            r.data_tipo,
            ciclo_de(&r.data),
            r.valor,
            r.descricao,
            r.raw,
            r.normalizado,
            r.categoria,
            r.conta,
            r.confianca,
            r.extraido_por,
            r.estado,
            r.movimentacao,
            if r.decidido { "usuario" } else { r.extraido_por },
            compra.then_some(r.data.as_str()),
        ])?;
    }
    Ok(())
}

fn inserir_decisoes(tx: &Transaction<'_>, plano: &Plano) -> Resultado<()> {
    let total = plano.decisoes.len();
    for (i, d) in plano.decisoes.iter().enumerate() {
        tx.prepare_cached(
            "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'usuario', ?7, ?8, ?9)",
        )?
        .execute(params![
            i as i64 + 1,
            d.alvo_tipo,
            d.alvo_id,
            d.campo,
            d.antes.to_string(),
            d.depois.to_string(),
            d.motivo,
            em_da_decisao(i, total),
            d.desfaz,
        ])?;
    }
    Ok(())
}

/// Escreve o banco inteiro (sem reprojetar). `destino` não pode existir.
pub fn escrever_banco(destino: &Path, escala: Escala) -> Resultado<Contagens> {
    if destino.exists() {
        return Err("o destino já existe".into());
    }
    let plano = planejar(escala);
    let contagens = Contagens {
        registros: plano.registros.len(),
        achados: plano.achados.len(),
        textos: plano.achados.len(),
        origens: plano.origens.len(),
        decisoes: plano.decisoes.len(),
    };

    criar_banco_do_bun(destino)?;
    let tmp = tempfile::tempdir()?;
    let mut inventario = Inventario::abrir(destino)?;
    inventario.migrar(tmp.path().join("copia-antes-da-migracao.sqlite"))?;
    drop(inventario);

    let mut conexao = Connection::open(destino)?;
    let tx = conexao.transaction()?;
    inserir_fontes_e_varreduras(&tx, &plano)?;
    inserir_ciclos(&tx)?;
    inserir_achados(&tx, &plano)?;
    inserir_origens(&tx, &plano)?;
    inserir_registros(&tx, &plano)?;
    inserir_decisoes(&tx, &plano)?;
    tx.commit()?;
    Ok(contagens)
}

/// Escreve o banco e roda `reprojetar` no fim, como o comando de manutenção.
pub fn gerar(destino: &Path, escala: Escala) -> Resultado<(Contagens, ResumoDaReprojecao)> {
    let contagens = escrever_banco(destino, escala)?;
    let mut inventario = Inventario::abrir(destino)?;
    let resumo = inventario
        .reprojetar()
        .map_err(|e| format!("reprojetar: {e}"))?;
    Ok((contagens, resumo))
}

// ---------------------------------------------------------------------------------------
// Caminho de destino.

/// Verdadeiro para um caminho dentro do cofre real.
fn eh_do_cofre(caminho: &Path) -> bool {
    caminho.starts_with(COFRE)
}

/// Confere o caminho de destino antes de criar qualquer coisa: nada sob o cofre, nada com
/// `..`, e nada que já exista.
fn destino_seguro(arg: &Path) -> Resultado<PathBuf> {
    let absoluto = if arg.is_absolute() {
        arg.to_path_buf()
    } else {
        std::env::current_dir()?.join(arg)
    };
    if eh_do_cofre(&absoluto) {
        return Err("recuso: o caminho fica dentro de /mnt/d/Chico/financas (cofre real)".into());
    }
    if absoluto.components().any(|c| c == Component::ParentDir) {
        return Err("recuso: use um caminho sem '..'".into());
    }
    let nome = absoluto
        .file_name()
        .ok_or("o caminho precisa terminar em um nome de arquivo")?
        .to_owned();
    let pai = absoluto.parent().ok_or("caminho sem pasta")?.to_path_buf();

    // Sobe até uma pasta que exista, resolve links e só então cria o resto.
    let mut existente = pai.as_path();
    while !existente.exists() {
        existente = existente.parent().ok_or("caminho sem pasta existente")?;
    }
    let resolvido = std::fs::canonicalize(existente)?.join(pai.strip_prefix(existente)?);
    if eh_do_cofre(&resolvido) {
        return Err("recuso: o caminho resolve para dentro do cofre real".into());
    }
    std::fs::create_dir_all(&resolvido)?;
    let destino = resolvido.join(nome);
    if destino.exists() {
        return Err("recuso: o arquivo já existe; apague-o antes".into());
    }
    Ok(destino)
}

fn main() -> Resultado<()> {
    let arg = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("uso: gerar_escala <caminho/novo/inventario.sqlite>")?;
    let destino = destino_seguro(&arg)?;
    let (contagens, reprojecao) = gerar(&destino, ESCALA_COMPLETA)?;
    println!("gerado: {}", destino.display());
    println!(
        "  registros {} | achados {} | textos {} | origens {} | decisoes {}",
        contagens.registros,
        contagens.achados,
        contagens.textos,
        contagens.origens,
        contagens.decisoes
    );
    println!(
        "  reprojetar: {} decisoes reaplicadas, {} alvos com decidido_por recalculado",
        reprojecao.decisoes, reprojecao.alvos
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;

    fn escalar(caminho: &Path, sql: &str) -> i64 {
        Connection::open(caminho)
            .unwrap()
            .query_row(sql, [], |l| l.get(0))
            .unwrap()
    }

    /// As linhas de uma consulta, cada uma num texto, para comparar projeções.
    fn linhas(caminho: &Path, sql: &str) -> Vec<String> {
        let conexao = Connection::open(caminho).unwrap();
        let mut consulta = conexao.prepare(sql).unwrap();
        let colunas = consulta.column_count();
        consulta
            .query_map([], |l| {
                let mut partes = Vec::with_capacity(colunas);
                for i in 0..colunas {
                    partes.push(format!("{:?}", l.get_ref(i)?));
                }
                Ok(partes.join("|"))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    /// A projeção: as colunas que as decisões mexem (e `decidido_por`). `criado_em` e
    /// `primeiro_visto_em` ficam de fora: o padrão é o relógio.
    fn projecao(caminho: &Path) -> (Vec<String>, Vec<String>) {
        (
            linhas(
                caminho,
                "SELECT id, content_hash, fonte_id, tipo, data, valor_centavos, estado,
                        categoria_provavel, fornecedor_normalizado, decidido_por, ciclo_id
                   FROM registro ORDER BY id",
            ),
            linhas(
                caminho,
                "SELECT content_hash, estado, document_type, supplier_name_raw, data_chave,
                        valor_centavos, decidido_por
                   FROM achado ORDER BY content_hash",
            ),
        )
    }

    #[test]
    fn escala_reduzida_confere_contagens_e_reprojetar_nao_muda_a_projecao() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("inventario.sqlite");

        let contagens = escrever_banco(&caminho, ESCALA_DE_TESTE).unwrap();
        assert_eq!(contagens.registros, 200);
        assert_eq!(contagens.achados, 50);
        assert_eq!(contagens.textos, 50);
        assert_eq!(contagens.decisoes, 24);

        assert_eq!(escalar(&caminho, "SELECT COUNT(*) FROM registro"), 200);
        assert_eq!(escalar(&caminho, "SELECT COUNT(*) FROM achado"), 50);
        assert_eq!(escalar(&caminho, "SELECT COUNT(*) FROM texto"), 50);
        assert_eq!(escalar(&caminho, "SELECT COUNT(*) FROM decisao"), 24);
        // Dez por cento de duplicatas por content_hash.
        assert_eq!(
            escalar(
                &caminho,
                "SELECT COUNT(*) - COUNT(DISTINCT content_hash) FROM registro WHERE content_hash IS NOT NULL",
            ),
            20
        );

        let antes = projecao(&caminho);
        let mut inventario = Inventario::abrir(&caminho).unwrap();
        let resumo = inventario.reprojetar().unwrap();
        drop(inventario);
        assert_eq!(resumo.decisoes, 24);
        assert_eq!(projecao(&caminho), antes);
    }

    #[test]
    fn a_mesma_semente_gera_o_mesmo_banco() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");
        escrever_banco(&a, ESCALA_DE_TESTE).unwrap();
        escrever_banco(&b, ESCALA_DE_TESTE).unwrap();
        assert_eq!(projecao(&a), projecao(&b));
        assert_eq!(
            linhas(&a, "SELECT * FROM decisao ORDER BY id"),
            linhas(&b, "SELECT * FROM decisao ORDER BY id")
        );
    }

    #[test]
    fn o_cofre_real_e_recusado_sem_tocar_no_disco() {
        assert!(eh_do_cofre(Path::new(
            "/mnt/d/Chico/financas/inventario.sqlite"
        )));
        assert!(!eh_do_cofre(Path::new("/tmp/escala/inventario.sqlite")));
    }

    #[test]
    fn datas_civis_voltam_e_vao_certo() {
        assert_eq!(data_ha(0), "2026-11-14");
        assert_eq!(data_ha(14), "2026-10-31");
        assert_eq!(ciclo_de("2026-11-14"), "2026-10");
        assert_eq!(ciclo_de("2026-11-19"), "2026-10");
        assert_eq!(ciclo_de("2026-11-20"), "2026-11");
        assert_eq!(ciclo_de("2026-01-05"), "2025-12");
    }
}
