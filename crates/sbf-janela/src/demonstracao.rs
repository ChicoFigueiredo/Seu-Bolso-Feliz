//! O modo demonstração: `Consulta` e `Comandos` em memória, com dados **fictícios** e
//! coerentes no espírito do protótipo (INV-17: nenhum dado real). As decisões mudam o
//! estado, o desfazer restaura o antes e os lotes funcionam.
//!
//! As duas metades ([`ConsultaDemonstracao`] e [`ComandosDemonstracao`]) dividem o mesmo
//! estado, como o `LeitorSqlite` e o `Inventario` dividem o mesmo banco.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde_json::{Value, json};

use sbf_inventario::comando::{
    Acao, ErroDeComando, IdDecisao, PedidoDeDecisao, ResultadoDeComando,
};
use sbf_inventario::consulta::{
    Campo, Celula, Coluna, Contagens, Elo, ErroDeConsulta, Filtro, Fonte, Item, Linha, Painel,
    PedidoDeTabela, Pergunta, Quem, Rastro, ResultadoDeConsulta, Secao, Tabela, TotalDoCiclo,
};
use sbf_inventario::{Comandos, Consulta, Ref, Tipo};

/// Os ciclos da demonstração, do mais antigo ao atual.
pub const CICLOS: [&str; 6] = [
    "2026-05", "2026-06", "2026-07", "2026-08", "2026-09", "2026-10",
];
const CICLO_ATUAL: &str = "2026-10";

#[derive(Debug, Clone)]
struct Conexao {
    id: &'static str,
    nome: &'static str,
    tipo: &'static str,
}

#[derive(Debug, Clone)]
struct Coleta {
    id: i64,
    conexao: &'static str,
    quando: String,
    responsavel: &'static str,
}

#[derive(Debug, Clone)]
struct Achado {
    hash: String,
    nome: String,
    /// `novo`, `documento`, `descartado`, `duvida`.
    estado: String,
    decidido_por: Option<String>,
    tipo_de_documento: String,
    fornecedor: Option<String>,
    data: Option<String>,
    valor: Option<i64>,
    texto: Option<String>,
    protegido: bool,
    duplicidades: BTreeSet<String>,
    /// Remetente e assunto, quando veio do e-mail.
    email: Option<(String, String)>,
}

#[derive(Debug, Clone)]
struct Origem {
    id: i64,
    hash: String,
    conexao: &'static str,
    caminho: String,
    coleta: i64,
    visto_em: String,
}

#[derive(Debug, Clone)]
struct Registro {
    id: i64,
    hash: String,
    descricao: String,
    fornecedor: String,
    conta: String,
    categoria: Option<String>,
    tipo: String,
    data: String,
    valor: i64,
    ciclo: String,
    /// `extraido`, `conferido`, `aprovado`, `divergente`.
    estado: String,
    decidido_por: String,
    fatura: Option<String>,
    conciliados: BTreeSet<i64>,
}

#[derive(Debug, Clone)]
struct Decisao {
    id: i64,
    alvo: Ref,
    campo: String,
    antes: Value,
    depois: Value,
    autor: String,
    motivo: Option<String>,
    em: String,
    desfaz: Option<i64>,
}

#[derive(Debug, Clone)]
struct Lote {
    id: i64,
    nome: String,
    existe: bool,
    criado_em: String,
    aprovado_em: Option<String>,
    itens: Vec<Ref>,
}

#[derive(Debug, Clone)]
struct Pendencia {
    id: i64,
    alvo: Ref,
    existe: bool,
    pergunta: String,
    resposta: Option<String>,
    criada_em: String,
    resolvida_em: Option<String>,
}

#[derive(Debug, Clone)]
struct RegraDeSenha {
    id: i64,
    fornecedor: String,
    tipo_de_documento: String,
    estrategia: String,
    criada_em: String,
}

#[derive(Debug, Clone)]
struct Fatura {
    chave: String,
    conta: String,
    ciclo: String,
    declarado: i64,
    hash: String,
}

/// O estado inteiro da demonstração.
#[derive(Debug, Clone)]
pub struct Estado {
    conexoes: Vec<Conexao>,
    coletas: Vec<Coleta>,
    achados: Vec<Achado>,
    origens: Vec<Origem>,
    registros: Vec<Registro>,
    decisoes: Vec<Decisao>,
    lotes: Vec<Lote>,
    pendencias: Vec<Pendencia>,
    regras: Vec<RegraDeSenha>,
    faturas: Vec<Fatura>,
    recorrentes: BTreeSet<String>,
    apelidos: BTreeMap<String, Vec<String>>,
    relogio: i64,
}

/// A metade de leitura.
#[derive(Debug, Clone)]
pub struct ConsultaDemonstracao(Rc<RefCell<Estado>>);

/// A metade de escrita.
#[derive(Debug, Clone)]
pub struct ComandosDemonstracao(Rc<RefCell<Estado>>);

/// Cria as duas metades sobre os mesmos dados fictícios.
pub fn nova() -> (ConsultaDemonstracao, ComandosDemonstracao) {
    let estado = Rc::new(RefCell::new(Estado::ficticio()));
    (
        ConsultaDemonstracao(Rc::clone(&estado)),
        ComandosDemonstracao(estado),
    )
}

/// Movimento entre contas não conta como gasto (a função única da spec).
pub fn e_movimento_entre_contas(tipo: &str) -> bool {
    matches!(
        tipo,
        "transferencia_interna" | "investimento" | "pagamento_fatura" | "fatura_cartao"
    )
}

fn hash_de(semente: &str) -> String {
    // FNV-1a de 64 bits, duas vezes, para um "hash" fictício estável de 32 dígitos.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut g: u64 = 0x8422_2325_cbf2_9ce4;
    for b in semente.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
        g ^= u64::from(b).rotate_left(3);
        g = g.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}{g:016x}")
}

fn mes_curto(ciclo: &str) -> &'static str {
    match ciclo.get(5..7) {
        Some("01") => "jan",
        Some("02") => "fev",
        Some("03") => "mar",
        Some("04") => "abr",
        Some("05") => "mai",
        Some("06") => "jun",
        Some("07") => "jul",
        Some("08") => "ago",
        Some("09") => "set",
        Some("10") => "out",
        Some("11") => "nov",
        _ => "dez",
    }
}

fn proximo_mes(ciclo: &str) -> String {
    let a: i32 = ciclo.get(..4).and_then(|x| x.parse().ok()).unwrap_or(2026);
    let m: i32 = ciclo.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(1);
    if m == 12 {
        format!("{:04}-01", a + 1)
    } else {
        format!("{a:04}-{:02}", m + 1)
    }
}

/// Data dentro do ciclo: dia 20 em diante cai no mês do ciclo; antes, no mês seguinte.
fn data_no_ciclo(ciclo: &str, dia: u32) -> String {
    if dia >= 20 {
        format!("{ciclo}-{dia:02}")
    } else {
        format!("{}-{dia:02}", proximo_mes(ciclo))
    }
}

fn ciclo_da_data(data: &str) -> String {
    let dia: u32 = data.get(8..10).and_then(|x| x.parse().ok()).unwrap_or(1);
    let mes = data.get(..7).unwrap_or("2026-01").to_owned();
    if dia >= 20 {
        mes
    } else {
        let a: i32 = mes.get(..4).and_then(|x| x.parse().ok()).unwrap_or(2026);
        let m: i32 = mes.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(1);
        if m == 1 {
            format!("{:04}-12", a - 1)
        } else {
            format!("{a:04}-{:02}", m - 1)
        }
    }
}

fn periodo_do_ciclo(ciclo: &str) -> String {
    format!(
        "20/{} a 19/{}",
        mes_curto(ciclo),
        mes_curto(&proximo_mes(ciclo))
    )
}

fn rotulo_estado_achado(e: &str) -> &'static str {
    match e {
        "documento" => "Documento",
        "descartado" => "Descartado",
        "duvida" => "Em dúvida",
        _ => "A triar",
    }
}

fn rotulo_estado_registro(e: &str) -> &'static str {
    match e {
        "aprovado" => "Aprovado",
        "conferido" => "Conferido",
        "divergente" => "Divergente",
        _ => "Extraído",
    }
}

fn rotulo_tipo_doc(t: &str) -> String {
    match t {
        "boleto" => "Boleto".into(),
        "fatura_cartao" => "Fatura de cartão".into(),
        "extrato" => "Extrato".into(),
        "comprovante" => "Comprovante".into(),
        "nota_fiscal" => "Nota fiscal".into(),
        "foto" => "Foto".into(),
        "planilha" => "Planilha".into(),
        "contrato" => "Contrato".into(),
        "outro" => "Outro".into(),
        outro => outro.to_owned(),
    }
}

fn rotulo_tipo_registro(t: &str) -> String {
    match t {
        "conta" => "Conta de consumo".into(),
        "compra" => "Compra".into(),
        "assinatura" => "Assinatura".into(),
        "receita" => "Receita".into(),
        "tarifa" => "Tarifa".into(),
        "pagamento_fatura" => "Pagamento de fatura".into(),
        "transferencia_interna" => "Transferência entre contas".into(),
        "mensalidade" => "Mensalidade".into(),
        outro => outro.to_owned(),
    }
}

fn quem(autor: &str) -> Quem {
    Quem::de_texto(autor)
}

fn txt(s: impl Into<String>) -> Celula {
    Celula::Texto(s.into())
}

fn link(alvo: Ref, texto: impl Into<String>) -> Celula {
    Celula::Link {
        alvo,
        texto: texto.into(),
    }
}

fn valor(centavos: i64, itens: Option<PedidoDeTabela>) -> Celula {
    Celula::Valor {
        centavos,
        fonte: Fonte::Inventario,
        itens: itens.map(Box::new),
    }
}

fn contagem(n: usize, itens: Option<PedidoDeTabela>) -> Celula {
    Celula::Contagem {
        n: n as i64,
        itens: itens.map(Box::new),
    }
}

fn ligados(de: &Ref, tipo: Tipo) -> Option<PedidoDeTabela> {
    Some(PedidoDeTabela::Ligados {
        de: de.clone(),
        tipo,
    })
}

fn do_total(de: &Ref, ciclo: Option<&str>, recorte: Option<&str>) -> Option<PedidoDeTabela> {
    Some(PedidoDeTabela::ItensDoTotal {
        de: de.clone(),
        fonte: Fonte::Inventario,
        ciclo: ciclo.map(str::to_owned),
        recorte: recorte.map(str::to_owned),
    })
}

fn col(rotulo: &str) -> Coluna {
    Coluna {
        rotulo: rotulo.to_owned(),
        numerica: false,
    }
}

fn coln(rotulo: &str) -> Coluna {
    Coluna {
        rotulo: rotulo.to_owned(),
        numerica: true,
    }
}

fn campo(rotulo: &str, valor: Celula) -> Campo {
    Campo {
        rotulo: rotulo.to_owned(),
        valor,
    }
}

fn elo(
    alvo: Ref,
    rotulo: impl Into<String>,
    papel: &str,
    autor: Option<&str>,
    quando: Option<&str>,
) -> Elo {
    Elo {
        alvo,
        rotulo: rotulo.into(),
        papel: papel.to_owned(),
        quem: autor.map(quem),
        quando: quando.map(str::to_owned),
    }
}

impl Estado {
    // ------------------------------------------------------------ dados fictícios

    fn ficticio() -> Estado {
        let mut e = Estado {
            conexoes: vec![
                Conexao {
                    id: "gmail:pessoal",
                    nome: "Gmail",
                    tipo: "E-mail (histórico)",
                },
                Conexao {
                    id: "disco:d",
                    nome: "Disco D",
                    tipo: "Disco local",
                },
                Conexao {
                    id: "drive:principal",
                    nome: "Google Drive",
                    tipo: "Nuvem",
                },
                Conexao {
                    id: "whatsapp:celular",
                    nome: "WhatsApp",
                    tipo: "Conversas exportadas",
                },
                Conexao {
                    id: "onedrive:pessoal",
                    nome: "OneDrive",
                    tipo: "Nuvem",
                },
                Conexao {
                    id: "icloud:fotos",
                    nome: "iCloud Fotos",
                    tipo: "Nuvem",
                },
            ],
            coletas: vec![
                Coleta {
                    id: 1,
                    conexao: "disco:d",
                    quando: "2026-10-02T21:00:00".into(),
                    responsavel: "coletar-local@4",
                },
                Coleta {
                    id: 2,
                    conexao: "drive:principal",
                    quando: "2026-10-03T08:10:00".into(),
                    responsavel: "coletar-nuvem@2",
                },
                Coleta {
                    id: 3,
                    conexao: "gmail:pessoal",
                    quando: "2026-11-10T07:00:00".into(),
                    responsavel: "coletar-gmail@3",
                },
                Coleta {
                    id: 4,
                    conexao: "whatsapp:celular",
                    quando: "2026-11-12T19:30:00".into(),
                    responsavel: "coletar-whatsapp@1",
                },
                Coleta {
                    id: 5,
                    conexao: "onedrive:pessoal",
                    quando: "2026-11-11T22:15:00".into(),
                    responsavel: "coletar-nuvem@2",
                },
            ],
            achados: Vec::new(),
            origens: Vec::new(),
            registros: Vec::new(),
            decisoes: Vec::new(),
            lotes: Vec::new(),
            pendencias: Vec::new(),
            regras: Vec::new(),
            faturas: Vec::new(),
            recorrentes: BTreeSet::new(),
            apelidos: BTreeMap::new(),
            relogio: 0,
        };
        e.apelidos.insert(
            "Energia Exemplo".into(),
            vec![
                "PAG CONTA ENERGIA EXEMPLO".into(),
                "ENERGIA EXEMPLO S/A".into(),
            ],
        );
        e.apelidos
            .insert("Streaming Zeta".into(), vec!["ZETA*STREAMING".into()]);
        e.apelidos.insert(
            "Mercado Delta".into(),
            vec!["MERCADO DELTA LJ 12".into(), "DELTA SUPERMERC".into()],
        );
        e.recorrentes.insert("Condomínio Kappa".into());

        // Boletos e contas de consumo: um documento e um registro por ciclo.
        struct Rec {
            fornecedor: &'static str,
            arquivo: &'static str,
            categoria: &'static str,
            tipo: &'static str,
            dia: u32,
            valores: [i64; 6],
            remetente: Option<&'static str>,
        }
        let recorrentes = [
            Rec {
                fornecedor: "Energia Exemplo",
                arquivo: "conta-energia",
                categoria: "Energia",
                tipo: "conta",
                dia: 28,
                valores: [22870, 23310, 24950, 26480, 28710, 31244],
                remetente: None,
            },
            Rec {
                fornecedor: "Água Sigma",
                arquivo: "conta-agua",
                categoria: "Água",
                tipo: "conta",
                dia: 5,
                valores: [14120, 13870, 15230, 14410, 14980, 15012],
                remetente: Some("faturas@agua-sigma.example"),
            },
            Rec {
                fornecedor: "Condomínio Kappa",
                arquivo: "boleto-condominio",
                categoria: "Moradia",
                tipo: "conta",
                dia: 15,
                valores: [98000, 98000, 98000, 98000, 98000, 98000],
                remetente: Some("administracao@kappa.example"),
            },
            Rec {
                fornecedor: "Escola Gama",
                arquivo: "mensalidade-escola",
                categoria: "Educação",
                tipo: "mensalidade",
                dia: 13,
                valores: [189000, 189000, 189000, 189000, 189000, 189000],
                remetente: Some("financeiro@escola-gama.example"),
            },
        ];
        for (ci, ciclo) in CICLOS.iter().enumerate() {
            let mes = mes_curto(ciclo);
            for r in &recorrentes {
                let data = data_no_ciclo(ciclo, r.dia);
                let nome = format!("{}-{mes}.pdf", r.arquivo);
                let hash = e.novo_documento(
                    &nome,
                    "boleto",
                    Some(r.fornecedor),
                    Some(&data),
                    Some(-r.valores[ci]),
                    r.remetente,
                );
                // Energia: o mesmo conteúdo no Disco D e no Google Drive (duas origens).
                if r.remetente.is_none() {
                    e.nova_origem(
                        &hash,
                        "disco:d",
                        &format!("D:/Imposto de renda/2026/{nome}"),
                        1,
                    );
                    e.nova_origem(
                        &hash,
                        "drive:principal",
                        &format!("Meu Drive/Digitalizados/{nome}"),
                        2,
                    );
                }
                let estado = if ci <= 3 {
                    "aprovado"
                } else if ci == 4 {
                    "conferido"
                } else {
                    "extraido"
                };
                let desc = format!("{} · {}/{}", r.fornecedor, &data[8..10], &data[5..7]);
                e.novo_registro(
                    &hash,
                    &desc,
                    r.fornecedor,
                    "Banco Alfa · corrente",
                    Some(r.categoria),
                    r.tipo,
                    &data,
                    -r.valores[ci],
                    estado,
                    None,
                );
            }

            // Fatura do Cartão Beta: várias compras num documento.
            let chave_beta = format!("Cartão Beta · {ciclo}");
            let compras_beta: Vec<(&str, Option<&str>, &str, u32, i64)> = vec![
                (
                    "Streaming Zeta",
                    Some("Assinaturas"),
                    "assinatura",
                    28,
                    3990,
                ),
                (
                    "Mercado Delta",
                    Some("Alimentação"),
                    "compra",
                    22,
                    41230 + 1370 * ci as i64,
                ),
                (
                    "Mercado Delta",
                    Some("Alimentação"),
                    "compra",
                    6,
                    28760 - 510 * ci as i64,
                ),
                (
                    "Livraria Ômicron",
                    if ci % 2 == 0 { Some("Lazer") } else { None },
                    "compra",
                    11,
                    8990,
                ),
            ];
            let total_beta: i64 = compras_beta.iter().map(|c| c.4).sum();
            let nome = format!("fatura-beta-{mes}.pdf");
            let hash = e.novo_documento(
                &nome,
                "fatura_cartao",
                Some("Banco Beta"),
                Some(&data_no_ciclo(ciclo, 25)),
                Some(-total_beta),
                Some("faturas@banco-beta.example"),
            );
            if ci == 5 {
                e.achado_mut(&hash).protegido = true;
            }
            for (forn, cat, tipo, dia, v) in &compras_beta {
                let data = data_no_ciclo(ciclo, *dia);
                let desc = format!("{forn} · {}/{}", &data[8..10], &data[5..7]);
                let estado = if ci <= 3 {
                    "aprovado"
                } else if ci == 4 && *forn == "Mercado Delta" && *dia == 6 {
                    "divergente"
                } else if ci == 4 {
                    "conferido"
                } else {
                    "extraido"
                };
                e.novo_registro(
                    &hash,
                    &desc,
                    forn,
                    "Cartão Beta",
                    *cat,
                    tipo,
                    &data,
                    -v,
                    estado,
                    Some(&chave_beta),
                );
            }
            e.faturas.push(Fatura {
                chave: chave_beta,
                conta: "Cartão Beta".into(),
                ciclo: (*ciclo).into(),
                declarado: -total_beta,
                hash: hash.clone(),
            });

            // Fatura do Cartão Ômega (sem fatura em 2026-07: a lacuna da conta).
            if *ciclo != "2026-07" {
                let chave = format!("Cartão Ômega · {ciclo}");
                let compras: Vec<(&str, Option<&str>, &str, u32, i64)> = vec![
                    ("Internet Teta", Some("Internet"), "assinatura", 10, 11990),
                    (
                        "Farmácia Épsilon",
                        Some("Saúde"),
                        "compra",
                        3,
                        6420 + 830 * ci as i64,
                    ),
                ];
                let soma: i64 = compras.iter().map(|c| c.4).sum();
                // Em 2026-09 a fatura declara uma compra que não virou registro: não fecha.
                let declarado = if *ciclo == "2026-09" {
                    soma + 1990
                } else {
                    soma
                };
                let nome = format!("fatura-omega-{mes}.pdf");
                let hash = e.novo_documento(
                    &nome,
                    "fatura_cartao",
                    Some("Banco Ômega"),
                    Some(&data_no_ciclo(ciclo, 2)),
                    Some(-declarado),
                    Some("fatura@banco-omega.example"),
                );
                for (forn, cat, tipo, dia, v) in &compras {
                    let data = data_no_ciclo(ciclo, *dia);
                    let desc = format!("{forn} · {}/{}", &data[8..10], &data[5..7]);
                    let estado = if ci <= 3 {
                        "aprovado"
                    } else if ci == 4 {
                        "conferido"
                    } else {
                        "extraido"
                    };
                    e.novo_registro(
                        &hash,
                        &desc,
                        forn,
                        "Cartão Ômega",
                        *cat,
                        tipo,
                        &data,
                        -v,
                        estado,
                        Some(&chave),
                    );
                }
                e.faturas.push(Fatura {
                    chave,
                    conta: "Cartão Ômega".into(),
                    ciclo: (*ciclo).into(),
                    declarado: -declarado,
                    hash,
                });
            }

            // Extrato do Banco Alfa: salário, tarifa, pagamento de fatura e transferência.
            let nome = format!("extrato-alfa-{mes}.pdf");
            let hash = e.novo_documento(
                &nome,
                "extrato",
                Some("Banco Alfa"),
                Some(&data_no_ciclo(ciclo, 19)),
                None,
                None,
            );
            e.nova_origem(
                &hash,
                "onedrive:pessoal",
                &format!("OneDrive/Bancos/Alfa/{nome}"),
                5,
            );
            let estado = if ci <= 3 {
                "aprovado"
            } else if ci == 4 {
                "conferido"
            } else {
                "extraido"
            };
            let linhas: Vec<(&str, Option<&str>, &str, u32, i64)> = vec![
                (
                    "Empresa Exemplo Ltda",
                    Some("Salário"),
                    "receita",
                    30,
                    850000,
                ),
                ("Banco Alfa", None, "tarifa", 1, -3990),
                ("Banco Beta", None, "pagamento_fatura", 2, -total_beta),
                ("Banco Alfa", None, "transferencia_interna", 4, -100000),
            ];
            for (forn, cat, tipo, dia, v) in linhas {
                let data = data_no_ciclo(ciclo, dia);
                let desc = match tipo {
                    "receita" => "Salário · Empresa Exemplo".to_owned(),
                    "tarifa" => "Tarifa de pacote".to_owned(),
                    "pagamento_fatura" => "Pagamento da fatura do Cartão Beta".to_owned(),
                    _ => "Transferência para a poupança".to_owned(),
                };
                e.novo_registro(
                    &hash,
                    &desc,
                    forn,
                    "Banco Alfa · corrente",
                    cat,
                    tipo,
                    &data,
                    v,
                    estado,
                    None,
                );
            }
        }

        // A triar: chegaram e nenhuma regra passou ainda.
        let a_triar: [(&str, &str, &'static str, i64, bool); 8] = [
            (
                "fatura-2026-11-recebida.pdf",
                "fatura_cartao",
                "gmail:pessoal",
                3,
                true,
            ),
            (
                "WhatsApp Image 2026-11-12.jpg",
                "foto",
                "whatsapp:celular",
                4,
                false,
            ),
            ("boleto (1).pdf", "boleto", "disco:d", 1, false),
            (
                "comprovante_transferencia.pdf",
                "comprovante",
                "gmail:pessoal",
                3,
                false,
            ),
            ("nfe-35261111.xml", "nota_fiscal", "gmail:pessoal", 3, false),
            (
                "Digitalizado_20261111.pdf",
                "boleto",
                "drive:principal",
                2,
                false,
            ),
            ("IMG_4471.heic", "foto", "onedrive:pessoal", 5, false),
            (
                "extrato-novembro-parcial.pdf",
                "extrato",
                "gmail:pessoal",
                3,
                true,
            ),
        ];
        for (nome, tipo, conexao, coleta, protegido) in a_triar {
            let hash = hash_de(nome);
            e.achados.push(Achado {
                hash: hash.clone(),
                nome: nome.into(),
                estado: "novo".into(),
                decidido_por: None,
                tipo_de_documento: tipo.into(),
                fornecedor: None,
                data: Some("2026-11-12".into()),
                valor: None,
                texto: (!protegido && tipo != "foto")
                    .then(|| format!("{nome}\n(texto extraído de exemplo)")),
                protegido,
                duplicidades: BTreeSet::new(),
                email: (conexao == "gmail:pessoal").then(|| {
                    (
                        "avisos@exemplo.example".to_owned(),
                        format!("Documento: {nome}"),
                    )
                }),
            });
            e.nova_origem(&hash, conexao, &format!("Recebidos/{nome}"), coleta);
        }
        // O digitalizado parece a conta de energia de outubro (possível duplicidade).
        let dig = hash_de("Digitalizado_20261111.pdf");
        {
            let a = e.achado_mut(&dig);
            a.fornecedor = Some("Energia Exemplo".into());
            a.data = Some("2026-10-28".into());
            a.valor = Some(-31244);
            a.texto = Some("ENERGIA EXEMPLO S/A\nConta de energia · referência out/2026\nTotal a pagar R$ 312,44\nVencimento 28/10/2026".into());
        }

        // Em dúvida e descartados, com quem decidiu.
        for (nome, tipo, autor) in [
            ("recibo-sem-data.jpg", "foto", "regras@2"),
            ("planilha-gastos-2025.xlsx", "planilha", "regras@2"),
            ("print-pix.png", "foto", "usuario"),
            ("contrato-aluguel.pdf", "contrato", "regras@2"),
        ] {
            let h = e.novo_achado_decidido(nome, tipo, "duvida", autor);
            e.nova_origem(
                &h,
                "drive:principal",
                &format!("Meu Drive/Diversos/{nome}"),
                2,
            );
        }
        for nome in [
            "foto-praia.jpg",
            "manual-geladeira.pdf",
            "convite-casamento.pdf",
            "cardapio.pdf",
            "meme.png",
            "curriculo.pdf",
        ] {
            let h = e.novo_achado_decidido(nome, "outro", "descartado", "regras@2");
            e.nova_origem(&h, "disco:d", &format!("D:/Downloads/{nome}"), 1);
        }

        // Um lote em montagem com os boletos de outubro.
        let mut itens = Vec::new();
        for a in &e.achados {
            if a.tipo_de_documento == "boleto"
                && a.estado == "documento"
                && a.data
                    .as_deref()
                    .is_some_and(|d| ciclo_da_data(d) == CICLO_ATUAL)
            {
                itens.push(Ref::nova(Tipo::Documento, a.hash.clone()));
            }
        }
        let regs: Vec<Ref> = e
            .registros
            .iter()
            .filter(|r| r.ciclo == CICLO_ATUAL && itens.iter().any(|i| i.chave == r.hash))
            .map(|r| Ref::nova(Tipo::Registro, r.id.to_string()))
            .collect();
        itens.extend(regs);
        e.lotes.push(Lote {
            id: 1,
            nome: "Boletos de outubro".into(),
            existe: true,
            criado_em: "2026-11-12T21:40:00".into(),
            aprovado_em: None,
            itens,
        });
        e.registrar(
            Ref::nova(Tipo::Lote, "1"),
            "existe",
            json!(false),
            json!(true),
            "usuario",
            None,
        );

        // Pendências.
        let fatura_nov = hash_de("fatura-2026-11-recebida.pdf");
        e.pendencias.push(Pendencia {
            id: 1,
            alvo: Ref::nova(Tipo::Achado, fatura_nov),
            existe: true,
            pergunta: "PDF protegido sem regra: qual senha abre as faturas do Banco Beta?".into(),
            resposta: None,
            criada_em: "2026-11-10T07:05:00".into(),
            resolvida_em: None,
        });
        let divergente = e
            .registros
            .iter()
            .find(|r| r.estado == "divergente")
            .map(|r| r.id)
            .unwrap_or(1);
        e.pendencias.push(Pendencia {
            id: 2,
            alvo: Ref::nova(Tipo::Registro, divergente.to_string()),
            existe: true,
            pergunta: "O valor desta compra não bate com o comprovante. Qual vale?".into(),
            resposta: None,
            criada_em: "2026-10-30T18:20:00".into(),
            resolvida_em: None,
        });
        e.pendencias.push(Pendencia {
            id: 3,
            alvo: Ref::nova(Tipo::Lote, "1"),
            existe: true,
            pergunta: "Incluir a mensalidade da escola neste lote?".into(),
            resposta: Some("Sim, entra junto.".into()),
            criada_em: "2026-11-12T21:41:00".into(),
            resolvida_em: Some("2026-11-12T21:45:00".into()),
        });

        // Uma regra de senha (só a estratégia: o valor fica no chaveiro).
        e.regras.push(RegraDeSenha {
            id: 1,
            fornecedor: "Banco Ômega".into(),
            tipo_de_documento: "fatura_cartao".into(),
            estrategia: "Os 5 primeiros dígitos do CPF".into(),
            criada_em: "2026-10-05T10:00:00".into(),
        });

        // Decisões antigas do usuário: aprovou os registros até 2026-08.
        let aprovados: Vec<i64> = e
            .registros
            .iter()
            .filter(|r| r.estado == "aprovado")
            .map(|r| r.id)
            .collect();
        for id in aprovados {
            let r = e.registros.iter_mut().find(|r| r.id == id);
            if let Some(r) = r {
                r.decidido_por = "usuario".into();
            }
            e.registrar(
                Ref::nova(Tipo::Registro, id.to_string()),
                "estado",
                json!("conferido"),
                json!("aprovado"),
                "usuario",
                None,
            );
        }
        let div = e
            .registros
            .iter()
            .find(|r| r.estado == "divergente")
            .map(|r| r.id);
        if let Some(id) = div {
            if let Some(r) = e.registros.iter_mut().find(|r| r.id == id) {
                r.decidido_por = "usuario".into();
            }
            e.registrar(
                Ref::nova(Tipo::Registro, id.to_string()),
                "estado",
                json!("conferido"),
                json!("divergente"),
                "usuario",
                Some("Valor diferente do comprovante".into()),
            );
        }
        e.datar_decisoes_iniciais();
        e
    }

    /// As decisões da carga fictícia ganham datas plausíveis (a coleta, ou o dia em que
    /// você aprovou); as decisões novas da sessão começam em 13/11/2026 09:01.
    fn datar_decisoes_iniciais(&mut self) {
        let mut datas = Vec::new();
        for d in &self.decisoes {
            let em = match d.alvo.tipo {
                Tipo::Achado | Tipo::Documento => self
                    .origens
                    .iter()
                    .find(|o| o.hash == d.alvo.chave)
                    .map(|o| {
                        o.visto_em
                            .replace(":00:00", ":01:00")
                            .replace(":10:00", ":11:00")
                    })
                    .unwrap_or_else(|| "2026-10-02T21:01:00".to_owned()),
                Tipo::Registro => self
                    .registro(&d.alvo.chave)
                    .map(|r| format!("{}-28T21:16:00", proximo_mes(&r.ciclo)))
                    .unwrap_or_else(|| "2026-10-01T21:16:00".to_owned()),
                _ => "2026-11-12T21:40:00".to_owned(),
            };
            datas.push(em);
        }
        for (d, em) in self.decisoes.iter_mut().zip(datas) {
            d.em = em;
        }
        self.relogio = 0;
    }

    fn agora(&mut self) -> String {
        self.relogio += 1;
        let minutos = 9 * 60 + self.relogio;
        format!(
            "2026-11-13T{:02}:{:02}:00",
            (minutos / 60) % 24,
            minutos % 60
        )
    }

    fn achado_mut(&mut self, hash: &str) -> &mut Achado {
        let i = self
            .achados
            .iter()
            .position(|a| a.hash == hash)
            .unwrap_or(0);
        &mut self.achados[i]
    }

    fn novo_documento(
        &mut self,
        nome: &str,
        tipo: &str,
        fornecedor: Option<&str>,
        data: Option<&str>,
        valor: Option<i64>,
        remetente: Option<&str>,
    ) -> String {
        let hash = hash_de(nome);
        let texto = format!(
            "{}\n{}\n{}{}",
            fornecedor.unwrap_or("").to_uppercase(),
            rotulo_tipo_doc(tipo),
            data.map(|d| format!("Data {}\n", crate::formato::data(d)))
                .unwrap_or_default(),
            valor
                .map(|v| format!("Total {}", crate::formato::dinheiro(v.abs())))
                .unwrap_or_default()
        );
        self.achados.push(Achado {
            hash: hash.clone(),
            nome: nome.into(),
            estado: "documento".into(),
            decidido_por: Some("regras@2".into()),
            tipo_de_documento: tipo.into(),
            fornecedor: fornecedor.map(str::to_owned),
            data: data.map(str::to_owned),
            valor,
            texto: Some(texto),
            protegido: false,
            duplicidades: BTreeSet::new(),
            email: remetente.map(|r| {
                (
                    r.to_owned(),
                    format!("{} · {}", fornecedor.unwrap_or("Documento"), nome),
                )
            }),
        });
        if let Some(r) = remetente {
            self.nova_origem(&hash, "gmail:pessoal", &format!("{r}/anexo {nome}"), 3);
        }
        let alvo = Ref::nova(Tipo::Achado, hash.clone());
        self.registrar(
            alvo,
            "estado",
            json!("novo"),
            json!("documento"),
            "regras@2",
            None,
        );
        hash
    }

    fn novo_achado_decidido(
        &mut self,
        nome: &str,
        tipo: &str,
        estado: &str,
        autor: &str,
    ) -> String {
        let hash = hash_de(nome);
        self.achados.push(Achado {
            hash: hash.clone(),
            nome: nome.into(),
            estado: estado.into(),
            decidido_por: Some(autor.into()),
            tipo_de_documento: tipo.into(),
            fornecedor: None,
            data: Some("2026-09-15".into()),
            valor: None,
            texto: Some(format!("{nome}\n(sem campos financeiros reconhecidos)")),
            protegido: false,
            duplicidades: BTreeSet::new(),
            email: None,
        });
        self.registrar(
            Ref::nova(Tipo::Achado, hash.clone()),
            "estado",
            json!("novo"),
            json!(estado),
            autor,
            None,
        );
        hash
    }

    fn nova_origem(&mut self, hash: &str, conexao: &'static str, caminho: &str, coleta: i64) {
        let id = self.origens.len() as i64 + 1;
        let visto_em = self
            .coletas
            .iter()
            .find(|c| c.id == coleta)
            .map(|c| c.quando.clone())
            .unwrap_or_default();
        self.origens.push(Origem {
            id,
            hash: hash.into(),
            conexao,
            caminho: caminho.into(),
            coleta,
            visto_em,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn novo_registro(
        &mut self,
        hash: &str,
        descricao: &str,
        fornecedor: &str,
        conta: &str,
        categoria: Option<&str>,
        tipo: &str,
        data: &str,
        valor: i64,
        estado: &str,
        fatura: Option<&str>,
    ) {
        let id = self.registros.len() as i64 + 1;
        self.registros.push(Registro {
            id,
            hash: hash.into(),
            descricao: descricao.into(),
            fornecedor: fornecedor.into(),
            conta: conta.into(),
            categoria: categoria.map(str::to_owned),
            tipo: tipo.into(),
            data: data.into(),
            valor,
            ciclo: ciclo_da_data(data),
            estado: estado.into(),
            decidido_por: "regras@2".into(),
            fatura: fatura.map(str::to_owned),
            conciliados: BTreeSet::new(),
        });
    }

    fn registrar(
        &mut self,
        alvo: Ref,
        campo: &str,
        antes: Value,
        depois: Value,
        autor: &str,
        motivo: Option<String>,
    ) -> i64 {
        let id = self.decisoes.len() as i64 + 1;
        let em = self.agora();
        self.decisoes.push(Decisao {
            id,
            alvo,
            campo: campo.into(),
            antes,
            depois,
            autor: autor.into(),
            motivo,
            em,
            desfaz: None,
        });
        id
    }

    // ------------------------------------------------------------ busca

    fn achado(&self, hash: &str) -> Option<&Achado> {
        self.achados.iter().find(|a| a.hash == hash)
    }

    fn registro(&self, id: &str) -> Option<&Registro> {
        let id: i64 = id.parse().ok()?;
        self.registros.iter().find(|r| r.id == id)
    }

    fn ref_do_achado(a: &Achado) -> Ref {
        if a.estado == "documento" {
            Ref::nova(Tipo::Documento, a.hash.clone())
        } else {
            Ref::nova(Tipo::Achado, a.hash.clone())
        }
    }

    fn ref_do_registro(r: &Registro) -> Ref {
        Ref::nova(Tipo::Registro, r.id.to_string())
    }

    fn rotulo(&self, r: &Ref) -> String {
        match r.tipo {
            Tipo::Achado | Tipo::Documento => self
                .achado(&r.chave)
                .map_or_else(|| r.chave.clone(), |a| a.nome.clone()),
            Tipo::Registro => self
                .registro(&r.chave)
                .map_or_else(|| r.chave.clone(), |x| x.descricao.clone()),
            Tipo::Origem => self
                .origens
                .iter()
                .find(|o| o.id.to_string() == r.chave)
                .map_or_else(
                    || r.chave.clone(),
                    |o| {
                        format!(
                            "{} · {}",
                            self.nome_conexao(o.conexao),
                            o.caminho.rsplit('/').next().unwrap_or(&o.caminho)
                        )
                    },
                ),
            Tipo::Decisao => self
                .decisao(&r.chave)
                .map_or_else(|| r.chave.clone(), |d| self.rotulo_decisao(d)),
            Tipo::Lote => self
                .lote(&r.chave)
                .map_or_else(|| r.chave.clone(), |l| l.nome.clone()),
            Tipo::Pendencia => self
                .pendencia(&r.chave)
                .map_or_else(|| r.chave.clone(), |p| p.pergunta.clone()),
            Tipo::RegraDeSenha => self
                .regras
                .iter()
                .find(|x| x.id.to_string() == r.chave)
                .map_or_else(
                    || r.chave.clone(),
                    |x| {
                        format!(
                            "{} · {}",
                            x.fornecedor,
                            rotulo_tipo_doc(&x.tipo_de_documento)
                        )
                    },
                ),
            Tipo::Conexao => self.nome_conexao(&r.chave).to_owned(),
            Tipo::Coleta => self
                .coletas
                .iter()
                .find(|c| c.id.to_string() == r.chave)
                .map_or_else(
                    || r.chave.clone(),
                    |c| {
                        format!(
                            "{} · {}",
                            self.nome_conexao(c.conexao),
                            crate::formato::data(&c.quando)
                        )
                    },
                ),
            Tipo::Ciclo => format!("Ciclo {}", r.chave),
            _ => r.chave.clone(),
        }
    }

    fn nome_conexao<'a>(&'a self, id: &'a str) -> &'a str {
        self.conexoes
            .iter()
            .find(|c| c.id == id)
            .map_or(id, |c| c.nome)
    }

    fn decisao(&self, id: &str) -> Option<&Decisao> {
        let id: i64 = id.parse().ok()?;
        self.decisoes.iter().find(|d| d.id == id)
    }

    fn lote(&self, id: &str) -> Option<&Lote> {
        self.lotes
            .iter()
            .find(|l| l.id.to_string() == id && l.existe)
    }

    fn pendencia(&self, id: &str) -> Option<&Pendencia> {
        self.pendencias
            .iter()
            .find(|p| p.id.to_string() == id && p.existe)
    }

    fn rotulo_decisao(&self, d: &Decisao) -> String {
        let curto = |v: &Value| match v {
            Value::String(s) => s.clone(),
            Value::Null => "—".to_owned(),
            Value::Bool(true) => "sim".to_owned(),
            Value::Bool(false) => "não".to_owned(),
            outro => outro.to_string(),
        };
        let campo = match d.campo.as_str() {
            "estado" => "Estado",
            "fornecedor" => "Fornecedor",
            "categoria" => "Categoria",
            "document_type" | "tipo" => "Tipo",
            "data_e_valor" => "Data e valor",
            "duplicidade" => "Duplicidade",
            "conciliacao" => "Conciliação",
            "recorrente" => "Recorrência",
            "existe" => "Criação",
            "resposta" => "Resposta",
            "itens" => "Itens do lote",
            "aprovado" => "Aprovação do lote",
            "regra_senha" => "Regra de senha",
            outro => outro,
        };
        let mut s = match d.campo.as_str() {
            "itens" | "aprovado" => campo.to_owned(),
            _ => format!("{campo}: {} → {}", curto(&d.antes), curto(&d.depois)),
        };
        if d.desfaz.is_some() {
            s = format!("Desfaz · {s}");
        }
        s
    }

    fn desfeita_por(&self, id: i64) -> Option<&Decisao> {
        self.decisoes.iter().find(|d| d.desfaz == Some(id))
    }

    fn fornecedores(&self) -> Vec<String> {
        let mut v: BTreeSet<String> = self
            .registros
            .iter()
            .map(|r| r.fornecedor.clone())
            .collect();
        v.extend(self.achados.iter().filter_map(|a| a.fornecedor.clone()));
        v.into_iter().collect()
    }

    fn contas(&self) -> Vec<String> {
        let v: BTreeSet<String> = self.registros.iter().map(|r| r.conta.clone()).collect();
        v.into_iter().collect()
    }

    fn origens_de(&self, hash: &str) -> Vec<&Origem> {
        self.origens.iter().filter(|o| o.hash == hash).collect()
    }

    fn lotes_com(&self, alvo: &Ref) -> Vec<&Lote> {
        self.lotes
            .iter()
            .filter(|l| {
                l.existe
                    && l.itens.iter().any(|i| {
                        i.chave == alvo.chave
                            && (i.tipo == alvo.tipo
                                || (matches!(i.tipo, Tipo::Achado | Tipo::Documento)
                                    && matches!(alvo.tipo, Tipo::Achado | Tipo::Documento)))
                    })
            })
            .collect()
    }

    /// Os registros de um lote: os diretos e os dos documentos dele.
    fn registros_do_lote(&self, l: &Lote) -> Vec<&Registro> {
        self.registros
            .iter()
            .filter(|r| {
                l.itens.iter().any(|i| {
                    (i.tipo == Tipo::Registro && i.chave == r.id.to_string())
                        || (matches!(i.tipo, Tipo::Documento | Tipo::Achado) && i.chave == r.hash)
                })
            })
            .collect()
    }

    /// Os registros que somam um item (fornecedor, conta, ciclo, fatura, lote, documento).
    fn registros_de(&self, de: &Ref) -> Vec<&Registro> {
        match de.tipo {
            Tipo::Fornecedor => self
                .registros
                .iter()
                .filter(|r| r.fornecedor == de.chave)
                .collect(),
            Tipo::Conta => self
                .registros
                .iter()
                .filter(|r| r.conta == de.chave)
                .collect(),
            Tipo::Ciclo => self
                .registros
                .iter()
                .filter(|r| r.ciclo == de.chave)
                .collect(),
            Tipo::Fatura => self
                .registros
                .iter()
                .filter(|r| r.fatura.as_deref() == Some(de.chave.as_str()))
                .collect(),
            Tipo::Achado | Tipo::Documento => self
                .registros
                .iter()
                .filter(|r| r.hash == de.chave)
                .collect(),
            Tipo::Lote => self
                .lote(&de.chave)
                .map(|l| self.registros_do_lote(l))
                .unwrap_or_default(),
            Tipo::Registro => self.registro(&de.chave).into_iter().collect(),
            Tipo::Coleta | Tipo::Conexao | Tipo::Origem => {
                let hashes: BTreeSet<String> = self
                    .vizinhos(de)
                    .into_iter()
                    .filter(|r| matches!(r.tipo, Tipo::Achado | Tipo::Documento))
                    .map(|r| r.chave)
                    .collect();
                self.registros
                    .iter()
                    .filter(|r| hashes.contains(&r.hash))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Todos os itens ligados a `de`, em qualquer direção.
    fn vizinhos(&self, de: &Ref) -> Vec<Ref> {
        let mut v: Vec<Ref> = Vec::new();
        let decisoes_sobre = |alvo_tipo: &[Tipo], chave: &str| -> Vec<Ref> {
            self.decisoes
                .iter()
                .filter(|d| alvo_tipo.contains(&d.alvo.tipo) && d.alvo.chave == chave)
                .map(|d| Ref::nova(Tipo::Decisao, d.id.to_string()))
                .collect()
        };
        match de.tipo {
            Tipo::Achado | Tipo::Documento => {
                let Some(a) = self.achado(&de.chave) else {
                    return v;
                };
                for o in self.origens_de(&a.hash) {
                    v.push(Ref::nova(Tipo::Origem, o.id.to_string()));
                    v.push(Ref::nova(Tipo::Coleta, o.coleta.to_string()));
                    v.push(Ref::nova(Tipo::Conexao, o.conexao));
                }
                v.extend(
                    self.registros
                        .iter()
                        .filter(|r| r.hash == a.hash)
                        .map(Self::ref_do_registro),
                );
                v.extend(decisoes_sobre(&[Tipo::Achado, Tipo::Documento], &a.hash));
                v.extend(
                    self.pendencias
                        .iter()
                        .filter(|p| p.existe && p.alvo.chave == a.hash)
                        .map(|p| Ref::nova(Tipo::Pendencia, p.id.to_string())),
                );
                v.extend(
                    self.lotes_com(de)
                        .into_iter()
                        .map(|l| Ref::nova(Tipo::Lote, l.id.to_string())),
                );
                for d in &a.duplicidades {
                    if let Some(x) = self.achado(d) {
                        v.push(Self::ref_do_achado(x));
                    }
                }
                for x in &self.achados {
                    if x.duplicidades.contains(&a.hash) {
                        v.push(Self::ref_do_achado(x));
                    }
                }
                if let Some(f) = &a.fornecedor {
                    v.push(Ref::nova(Tipo::Fornecedor, f.clone()));
                    v.extend(
                        self.regras
                            .iter()
                            .filter(|r| {
                                &r.fornecedor == f && r.tipo_de_documento == a.tipo_de_documento
                            })
                            .map(|r| Ref::nova(Tipo::RegraDeSenha, r.id.to_string())),
                    );
                }
                if let Some(d) = &a.data {
                    v.push(Ref::nova(Tipo::Ciclo, ciclo_da_data(d)));
                }
            }
            Tipo::Origem => {
                if let Some(o) = self.origens.iter().find(|o| o.id.to_string() == de.chave) {
                    if let Some(a) = self.achado(&o.hash) {
                        v.push(Self::ref_do_achado(a));
                    }
                    v.push(Ref::nova(Tipo::Coleta, o.coleta.to_string()));
                    v.push(Ref::nova(Tipo::Conexao, o.conexao));
                    v.extend(
                        self.registros
                            .iter()
                            .filter(|r| r.hash == o.hash)
                            .map(Self::ref_do_registro),
                    );
                }
            }
            Tipo::Registro => {
                let Some(r) = self.registro(&de.chave) else {
                    return v;
                };
                if let Some(a) = self.achado(&r.hash) {
                    v.push(Self::ref_do_achado(a));
                    for o in self.origens_de(&a.hash) {
                        v.push(Ref::nova(Tipo::Origem, o.id.to_string()));
                    }
                }
                v.push(Ref::nova(Tipo::Fornecedor, r.fornecedor.clone()));
                v.push(Ref::nova(Tipo::Conta, r.conta.clone()));
                v.push(Ref::nova(Tipo::Ciclo, r.ciclo.clone()));
                if let Some(f) = &r.fatura {
                    v.push(Ref::nova(Tipo::Fatura, f.clone()));
                }
                if let Some(c) = &r.categoria {
                    v.push(Ref::nova(Tipo::Categoria, c.clone()));
                }
                v.extend(decisoes_sobre(&[Tipo::Registro], &de.chave));
                v.extend(
                    self.lotes_com(de)
                        .into_iter()
                        .map(|l| Ref::nova(Tipo::Lote, l.id.to_string())),
                );
                // Um registro também está no lote do documento dele.
                if let Some(a) = self.achado(&r.hash) {
                    v.extend(
                        self.lotes_com(&Self::ref_do_achado(a))
                            .into_iter()
                            .map(|l| Ref::nova(Tipo::Lote, l.id.to_string())),
                    );
                }
                v.extend(
                    self.pendencias
                        .iter()
                        .filter(|p| {
                            p.existe && p.alvo.tipo == Tipo::Registro && p.alvo.chave == de.chave
                        })
                        .map(|p| Ref::nova(Tipo::Pendencia, p.id.to_string())),
                );
                for c in &r.conciliados {
                    v.push(Ref::nova(Tipo::Registro, c.to_string()));
                }
                for x in &self.registros {
                    if x.conciliados.contains(&r.id) {
                        v.push(Self::ref_do_registro(x));
                    }
                }
            }
            Tipo::Decisao => {
                let Some(d) = self.decisao(&de.chave) else {
                    return v;
                };
                let alvo = match d.alvo.tipo {
                    Tipo::Achado | Tipo::Documento => self
                        .achado(&d.alvo.chave)
                        .map(Self::ref_do_achado)
                        .unwrap_or_else(|| d.alvo.clone()),
                    _ => d.alvo.clone(),
                };
                v.push(alvo);
                if let Some(x) = d.desfaz {
                    v.push(Ref::nova(Tipo::Decisao, x.to_string()));
                }
                if let Some(x) = self.desfeita_por(d.id) {
                    v.push(Ref::nova(Tipo::Decisao, x.id.to_string()));
                }
            }
            Tipo::Lote => {
                let Some(l) = self.lote(&de.chave) else {
                    return v;
                };
                for i in &l.itens {
                    match i.tipo {
                        Tipo::Achado | Tipo::Documento => {
                            if let Some(a) = self.achado(&i.chave) {
                                v.push(Self::ref_do_achado(a));
                            }
                        }
                        _ => v.push(i.clone()),
                    }
                }
                v.extend(decisoes_sobre(&[Tipo::Lote], &de.chave));
                v.extend(
                    self.pendencias
                        .iter()
                        .filter(|p| {
                            p.existe && p.alvo.tipo == Tipo::Lote && p.alvo.chave == de.chave
                        })
                        .map(|p| Ref::nova(Tipo::Pendencia, p.id.to_string())),
                );
            }
            Tipo::Pendencia => {
                let Some(p) = self.pendencia(&de.chave) else {
                    return v;
                };
                let alvo = match p.alvo.tipo {
                    Tipo::Achado | Tipo::Documento => self
                        .achado(&p.alvo.chave)
                        .map(Self::ref_do_achado)
                        .unwrap_or_else(|| p.alvo.clone()),
                    _ => p.alvo.clone(),
                };
                v.push(alvo);
                v.extend(decisoes_sobre(&[Tipo::Pendencia], &de.chave));
            }
            Tipo::RegraDeSenha => {
                if let Some(r) = self.regras.iter().find(|r| r.id.to_string() == de.chave) {
                    v.push(Ref::nova(Tipo::Fornecedor, r.fornecedor.clone()));
                    v.extend(
                        self.achados
                            .iter()
                            .filter(|a| {
                                a.fornecedor.as_deref() == Some(r.fornecedor.as_str())
                                    && a.tipo_de_documento == r.tipo_de_documento
                            })
                            .map(Self::ref_do_achado),
                    );
                    v.extend(
                        decisoes_sobre(&[Tipo::Fornecedor], &r.fornecedor)
                            .into_iter()
                            .filter(|d| {
                                self.decisao(&d.chave)
                                    .is_some_and(|x| x.campo == "regra_senha")
                            }),
                    );
                }
            }
            Tipo::Conexao => {
                v.extend(
                    self.coletas
                        .iter()
                        .filter(|c| c.conexao == de.chave)
                        .map(|c| Ref::nova(Tipo::Coleta, c.id.to_string())),
                );
                for o in self.origens.iter().filter(|o| o.conexao == de.chave) {
                    v.push(Ref::nova(Tipo::Origem, o.id.to_string()));
                    if let Some(a) = self.achado(&o.hash) {
                        v.push(Self::ref_do_achado(a));
                    }
                }
            }
            Tipo::Coleta => {
                if let Some(c) = self.coletas.iter().find(|c| c.id.to_string() == de.chave) {
                    v.push(Ref::nova(Tipo::Conexao, c.conexao));
                    for o in self.origens.iter().filter(|o| o.coleta == c.id) {
                        v.push(Ref::nova(Tipo::Origem, o.id.to_string()));
                        if let Some(a) = self.achado(&o.hash) {
                            v.push(Self::ref_do_achado(a));
                        }
                    }
                }
            }
            Tipo::Ciclo => {
                for r in self.registros.iter().filter(|r| r.ciclo == de.chave) {
                    v.push(Self::ref_do_registro(r));
                    v.push(Ref::nova(Tipo::Fornecedor, r.fornecedor.clone()));
                }
                v.extend(
                    self.achados
                        .iter()
                        .filter(|a| {
                            a.data
                                .as_deref()
                                .is_some_and(|d| ciclo_da_data(d) == de.chave)
                        })
                        .map(Self::ref_do_achado),
                );
                v.extend(
                    self.faturas
                        .iter()
                        .filter(|f| f.ciclo == de.chave)
                        .map(|f| Ref::nova(Tipo::Fatura, f.chave.clone())),
                );
            }
            Tipo::Fornecedor => {
                for r in self.registros.iter().filter(|r| r.fornecedor == de.chave) {
                    v.push(Self::ref_do_registro(r));
                    if let Some(a) = self.achado(&r.hash) {
                        v.push(Self::ref_do_achado(a));
                    }
                }
                v.extend(
                    self.achados
                        .iter()
                        .filter(|a| a.fornecedor.as_deref() == Some(de.chave.as_str()))
                        .map(Self::ref_do_achado),
                );
                v.extend(
                    self.regras
                        .iter()
                        .filter(|r| r.fornecedor == de.chave)
                        .map(|r| Ref::nova(Tipo::RegraDeSenha, r.id.to_string())),
                );
                v.extend(decisoes_sobre(&[Tipo::Fornecedor], &de.chave));
            }
            Tipo::Conta => {
                for r in self.registros.iter().filter(|r| r.conta == de.chave) {
                    v.push(Self::ref_do_registro(r));
                    if let Some(a) = self.achado(&r.hash) {
                        v.push(Self::ref_do_achado(a));
                    }
                }
                v.extend(
                    self.faturas
                        .iter()
                        .filter(|f| f.conta == de.chave)
                        .map(|f| Ref::nova(Tipo::Fatura, f.chave.clone())),
                );
            }
            Tipo::Fatura => {
                if let Some(f) = self.faturas.iter().find(|f| f.chave == de.chave) {
                    v.push(Ref::nova(Tipo::Conta, f.conta.clone()));
                    v.push(Ref::nova(Tipo::Ciclo, f.ciclo.clone()));
                    if let Some(a) = self.achado(&f.hash) {
                        v.push(Self::ref_do_achado(a));
                    }
                    v.extend(
                        self.registros
                            .iter()
                            .filter(|r| r.fatura.as_deref() == Some(de.chave.as_str()))
                            .map(Self::ref_do_registro),
                    );
                }
            }
            _ => {}
        }
        let mut vistos = BTreeSet::new();
        v.retain(|r| r != de && vistos.insert(r.clone()));
        v
    }

    fn ligados(&self, de: &Ref, tipo: Tipo) -> Vec<Ref> {
        self.vizinhos(de)
            .into_iter()
            .filter(|r| match tipo {
                Tipo::Achado => matches!(r.tipo, Tipo::Achado | Tipo::Documento),
                outro => r.tipo == outro,
            })
            .collect()
    }

    // ------------------------------------------------------------ tabelas

    fn linha_do_achado(&self, a: &Achado) -> Linha {
        let alvo = Self::ref_do_achado(a);
        let origens = self.origens_de(&a.hash);
        let mut conexoes: Vec<(Ref, String)> = Vec::new();
        for o in &origens {
            let r = Ref::nova(Tipo::Conexao, o.conexao);
            if !conexoes.iter().any(|(x, _)| *x == r) {
                conexoes.push((r, self.nome_conexao(o.conexao).to_owned()));
            }
        }
        Linha {
            alvo: Some(alvo.clone()),
            celulas: vec![
                link(alvo.clone(), a.nome.clone()),
                Celula::Estado {
                    rotulo: rotulo_estado_achado(&a.estado).into(),
                    quem: a.decidido_por.as_deref().map(quem),
                },
                txt(format!(
                    "{}{}",
                    rotulo_tipo_doc(&a.tipo_de_documento),
                    if a.protegido { " · protegido" } else { "" }
                )),
                a.fornecedor.as_ref().map_or(Celula::Vazia, |f| {
                    link(Ref::nova(Tipo::Fornecedor, f.clone()), f.clone())
                }),
                a.data
                    .as_ref()
                    .map_or(Celula::Vazia, |d| Celula::Data(d.clone())),
                a.valor.map_or(Celula::Vazia, |v| {
                    valor(
                        v,
                        if self.registros.iter().any(|r| r.hash == a.hash) {
                            do_total(&alvo, None, None)
                        } else {
                            None
                        },
                    )
                }),
                contagem(origens.len(), ligados(&alvo, Tipo::Origem)),
                Celula::Links(conexoes),
            ],
        }
    }

    fn tabela_de_achados<'a>(&self, achados: impl Iterator<Item = &'a Achado>) -> Tabela {
        Tabela {
            colunas: vec![
                col("Nome"),
                col("Estado"),
                col("Tipo"),
                col("Fornecedor"),
                col("Data-chave"),
                coln("Valor"),
                coln("Origens"),
                col("Conexão"),
            ],
            linhas: achados.map(|a| self.linha_do_achado(a)).collect(),
        }
    }

    fn tabela_de_registros<'a>(&self, registros: impl Iterator<Item = &'a Registro>) -> Tabela {
        Tabela {
            colunas: vec![
                col("Data"),
                col("Registro"),
                col("Fornecedor"),
                col("Conta"),
                col("Categoria"),
                col("Tipo"),
                coln("Valor"),
                col("Ciclo"),
                col("Estado"),
                col("Documento"),
            ],
            linhas: registros
                .map(|r| {
                    let alvo = Self::ref_do_registro(r);
                    Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            Celula::Data(r.data.clone()),
                            link(alvo, r.descricao.clone()),
                            link(
                                Ref::nova(Tipo::Fornecedor, r.fornecedor.clone()),
                                r.fornecedor.clone(),
                            ),
                            link(Ref::nova(Tipo::Conta, r.conta.clone()), r.conta.clone()),
                            r.categoria.as_ref().map_or(Celula::Vazia, |c| {
                                link(Ref::nova(Tipo::Categoria, c.clone()), c.clone())
                            }),
                            txt(rotulo_tipo_registro(&r.tipo)),
                            valor(r.valor, None),
                            link(Ref::nova(Tipo::Ciclo, r.ciclo.clone()), r.ciclo.clone()),
                            Celula::Estado {
                                rotulo: rotulo_estado_registro(&r.estado).into(),
                                quem: Some(quem(&r.decidido_por)),
                            },
                            self.achado(&r.hash).map_or(Celula::Vazia, |a| {
                                link(Self::ref_do_achado(a), a.nome.clone())
                            }),
                        ],
                    }
                })
                .collect(),
        }
    }

    fn tabela_de_refs(&self, tipo: Tipo, refs: &[Ref]) -> Tabela {
        match tipo {
            Tipo::Achado | Tipo::Documento => {
                let hashes: Vec<&str> = refs.iter().map(|r| r.chave.as_str()).collect();
                self.tabela_de_achados(
                    self.achados
                        .iter()
                        .filter(|a| hashes.contains(&a.hash.as_str())),
                )
            }
            Tipo::Registro => {
                let ids: Vec<&str> = refs.iter().map(|r| r.chave.as_str()).collect();
                self.tabela_de_registros(
                    self.registros
                        .iter()
                        .filter(|r| ids.contains(&r.id.to_string().as_str())),
                )
            }
            Tipo::Origem => Tabela {
                colunas: vec![
                    col("Origem"),
                    col("Achado"),
                    col("Conexão"),
                    col("Caminho · anexo · conversa"),
                    col("Coleta"),
                    col("Vista em"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.origens.iter().find(|o| o.id.to_string() == r.chave))
                    .map(|o| {
                        let alvo = Ref::nova(Tipo::Origem, o.id.to_string());
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), self.rotulo(&alvo)),
                                self.achado(&o.hash).map_or(Celula::Vazia, |a| {
                                    link(Self::ref_do_achado(a), a.nome.clone())
                                }),
                                link(
                                    Ref::nova(Tipo::Conexao, o.conexao),
                                    self.nome_conexao(o.conexao),
                                ),
                                txt(o.caminho.clone()),
                                link(
                                    Ref::nova(Tipo::Coleta, o.coleta.to_string()),
                                    format!("Coleta {}", o.coleta),
                                ),
                                Celula::Data(o.visto_em.clone()),
                            ],
                        }
                    })
                    .collect(),
            },
            Tipo::Decisao => Tabela {
                colunas: vec![
                    col("Quando"),
                    col("Decisão"),
                    col("Sobre"),
                    col("Quem"),
                    col("Motivo"),
                    col("Desfeita"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.decisao(&r.chave))
                    .map(|d| {
                        let alvo = Ref::nova(Tipo::Decisao, d.id.to_string());
                        let sobre = match d.alvo.tipo {
                            Tipo::Achado | Tipo::Documento => self
                                .achado(&d.alvo.chave)
                                .map(Self::ref_do_achado)
                                .unwrap_or_else(|| d.alvo.clone()),
                            _ => d.alvo.clone(),
                        };
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                Celula::Data(d.em.clone()),
                                link(alvo, self.rotulo_decisao(d)),
                                link(sobre.clone(), self.rotulo(&sobre)),
                                txt(quem(&d.autor).rotulo().to_owned()),
                                d.motivo.clone().map_or(Celula::Vazia, txt),
                                self.desfeita_por(d.id).map_or(Celula::Vazia, |x| {
                                    link(
                                        Ref::nova(Tipo::Decisao, x.id.to_string()),
                                        format!("por {}", x.id),
                                    )
                                }),
                            ],
                        }
                    })
                    .collect(),
            },
            Tipo::Lote => Tabela {
                colunas: vec![
                    col("Lote"),
                    col("Estado"),
                    coln("Itens"),
                    coln("Total"),
                    col("Criado em"),
                    col("Aprovado em"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.lote(&r.chave))
                    .map(|l| self.linha_do_lote(l))
                    .collect(),
            },
            Tipo::Pendencia => Tabela {
                colunas: vec![
                    col("Pergunta"),
                    col("Sobre"),
                    col("Estado"),
                    col("Criada em"),
                    col("Resposta"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.pendencia(&r.chave))
                    .map(|p| self.linha_da_pendencia(p))
                    .collect(),
            },
            Tipo::Coleta => Tabela {
                colunas: vec![
                    col("Coleta"),
                    col("Conexão"),
                    col("Quando"),
                    col("Quem coletou"),
                    coln("Achados"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.coletas.iter().find(|c| c.id.to_string() == r.chave))
                    .map(|c| self.linha_da_coleta(c))
                    .collect(),
            },
            Tipo::Conexao => Tabela {
                colunas: vec![
                    col("Conexão"),
                    col("Tipo"),
                    coln("Coletas"),
                    coln("Achados"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.conexoes.iter().find(|c| c.id == r.chave))
                    .map(|c| {
                        let alvo = Ref::nova(Tipo::Conexao, c.id);
                        let achados: BTreeSet<&str> = self
                            .origens
                            .iter()
                            .filter(|o| o.conexao == c.id)
                            .map(|o| o.hash.as_str())
                            .collect();
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), c.nome),
                                txt(c.tipo),
                                contagem(
                                    self.coletas.iter().filter(|x| x.conexao == c.id).count(),
                                    ligados(&alvo, Tipo::Coleta),
                                ),
                                contagem(achados.len(), ligados(&alvo, Tipo::Achado)),
                            ],
                        }
                    })
                    .collect(),
            },
            Tipo::Fornecedor => {
                let nomes: Vec<String> = refs.iter().map(|r| r.chave.clone()).collect();
                self.tabela_de_fornecedores(&nomes)
            }
            Tipo::Fatura => {
                let chaves: Vec<&str> = refs.iter().map(|r| r.chave.as_str()).collect();
                self.tabela_de_faturas(
                    self.faturas
                        .iter()
                        .filter(|f| chaves.contains(&f.chave.as_str())),
                )
            }
            Tipo::RegraDeSenha => Tabela {
                colunas: vec![
                    col("Regra"),
                    col("Fornecedor"),
                    col("Tipo de documento"),
                    col("Estratégia"),
                    col("Criada em"),
                ],
                linhas: refs
                    .iter()
                    .filter_map(|r| self.regras.iter().find(|x| x.id.to_string() == r.chave))
                    .map(|x| {
                        let alvo = Ref::nova(Tipo::RegraDeSenha, x.id.to_string());
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), self.rotulo(&alvo)),
                                link(
                                    Ref::nova(Tipo::Fornecedor, x.fornecedor.clone()),
                                    x.fornecedor.clone(),
                                ),
                                txt(rotulo_tipo_doc(&x.tipo_de_documento)),
                                txt(x.estrategia.clone()),
                                Celula::Data(x.criada_em.clone()),
                            ],
                        }
                    })
                    .collect(),
            },
            Tipo::Ciclo | Tipo::Conta => Tabela {
                colunas: vec![col(tipo.singular()), coln("Registros")],
                linhas: refs
                    .iter()
                    .map(|r| Linha {
                        alvo: Some(r.clone()),
                        celulas: vec![
                            link(r.clone(), self.rotulo(r)),
                            contagem(self.registros_de(r).len(), do_total(r, None, None)),
                        ],
                    })
                    .collect(),
            },
            _ => Tabela {
                colunas: vec![col(tipo.singular())],
                linhas: refs
                    .iter()
                    .map(|r| Linha {
                        alvo: Some(r.clone()),
                        celulas: vec![link(r.clone(), self.rotulo(r))],
                    })
                    .collect(),
            },
        }
    }

    fn linha_do_lote(&self, l: &Lote) -> Linha {
        let alvo = Ref::nova(Tipo::Lote, l.id.to_string());
        let total: i64 = self.registros_do_lote(l).iter().map(|r| r.valor).sum();
        Linha {
            alvo: Some(alvo.clone()),
            celulas: vec![
                link(alvo.clone(), l.nome.clone()),
                Celula::Estado {
                    rotulo: if l.aprovado_em.is_some() {
                        "Aprovado".into()
                    } else {
                        "Em montagem".into()
                    },
                    quem: Some(Quem::Usuario),
                },
                contagem(l.itens.len(), ligados(&alvo, Tipo::Documento)),
                valor(total, do_total(&alvo, None, None)),
                Celula::Data(l.criado_em.clone()),
                l.aprovado_em.clone().map_or(Celula::Vazia, Celula::Data),
            ],
        }
    }

    fn linha_da_pendencia(&self, p: &Pendencia) -> Linha {
        let alvo = Ref::nova(Tipo::Pendencia, p.id.to_string());
        let sobre = match p.alvo.tipo {
            Tipo::Achado | Tipo::Documento => self
                .achado(&p.alvo.chave)
                .map(Self::ref_do_achado)
                .unwrap_or_else(|| p.alvo.clone()),
            _ => p.alvo.clone(),
        };
        Linha {
            alvo: Some(alvo.clone()),
            celulas: vec![
                link(alvo, p.pergunta.clone()),
                link(sobre.clone(), self.rotulo(&sobre)),
                Celula::Estado {
                    rotulo: if p.resposta.is_some() {
                        "Respondida".into()
                    } else {
                        "Aberta".into()
                    },
                    quem: None,
                },
                Celula::Data(p.criada_em.clone()),
                p.resposta.clone().map_or(Celula::Vazia, txt),
            ],
        }
    }

    fn linha_da_coleta(&self, c: &Coleta) -> Linha {
        let alvo = Ref::nova(Tipo::Coleta, c.id.to_string());
        let achados: BTreeSet<&str> = self
            .origens
            .iter()
            .filter(|o| o.coleta == c.id)
            .map(|o| o.hash.as_str())
            .collect();
        Linha {
            alvo: Some(alvo.clone()),
            celulas: vec![
                link(alvo.clone(), format!("Coleta {}", c.id)),
                link(
                    Ref::nova(Tipo::Conexao, c.conexao),
                    self.nome_conexao(c.conexao),
                ),
                Celula::Data(c.quando.clone()),
                txt(c.responsavel),
                contagem(achados.len(), ligados(&alvo, Tipo::Achado)),
            ],
        }
    }

    fn tabela_de_fornecedores(&self, nomes: &[String]) -> Tabela {
        Tabela {
            colunas: vec![
                col("Fornecedor"),
                col("Categoria"),
                coln("Registros"),
                coln("Ciclos"),
                coln("Total"),
                col("Último"),
                col("Recorrente"),
            ],
            linhas: nomes
                .iter()
                .map(|f| {
                    let alvo = Ref::nova(Tipo::Fornecedor, f.clone());
                    let regs: Vec<&Registro> = self
                        .registros
                        .iter()
                        .filter(|r| &r.fornecedor == f)
                        .collect();
                    let ciclos: BTreeSet<&str> = regs.iter().map(|r| r.ciclo.as_str()).collect();
                    let categoria = regs.iter().find_map(|r| r.categoria.clone());
                    let ultimo = regs.iter().map(|r| r.data.clone()).max();
                    Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            link(alvo.clone(), f.clone()),
                            categoria.map_or(Celula::Vazia, |c| {
                                link(Ref::nova(Tipo::Categoria, c.clone()), c)
                            }),
                            contagem(regs.len(), do_total(&alvo, None, None)),
                            Celula::Contagem {
                                n: ciclos.len() as i64,
                                itens: None,
                            },
                            valor(
                                regs.iter().map(|r| r.valor).sum(),
                                do_total(&alvo, None, None),
                            ),
                            ultimo.map_or(Celula::Vazia, Celula::Data),
                            txt(self.texto_recorrente(f)),
                        ],
                    }
                })
                .collect(),
        }
    }

    fn texto_recorrente(&self, f: &str) -> String {
        if self.recorrentes.contains(f) {
            "sim, confirmado".to_owned()
        } else {
            let ciclos: BTreeSet<&str> = self
                .registros
                .iter()
                .filter(|r| r.fornecedor == f)
                .map(|r| r.ciclo.as_str())
                .collect();
            if ciclos.len() >= 4 {
                "parece recorrente".to_owned()
            } else {
                "não".to_owned()
            }
        }
    }

    fn tabela_de_faturas<'a>(&self, faturas: impl Iterator<Item = &'a Fatura>) -> Tabela {
        Tabela {
            colunas: vec![
                col("Fatura"),
                col("Cartão"),
                col("Ciclo"),
                coln("Declarado"),
                coln("Soma dos registros"),
                coln("Diferença"),
                col("Confere"),
            ],
            linhas: faturas
                .map(|f| {
                    let alvo = Ref::nova(Tipo::Fatura, f.chave.clone());
                    let soma: i64 = self
                        .registros
                        .iter()
                        .filter(|r| r.fatura.as_deref() == Some(f.chave.as_str()))
                        .map(|r| r.valor)
                        .sum();
                    let dif = f.declarado - soma;
                    Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            link(alvo.clone(), f.chave.clone()),
                            link(Ref::nova(Tipo::Conta, f.conta.clone()), f.conta.clone()),
                            link(Ref::nova(Tipo::Ciclo, f.ciclo.clone()), f.ciclo.clone()),
                            valor(f.declarado, None),
                            valor(soma, do_total(&alvo, None, None)),
                            valor(dif, None),
                            Celula::Estado {
                                rotulo: if dif == 0 {
                                    "Aprovado · fecha".into()
                                } else {
                                    "Divergente · não fecha".into()
                                },
                                quem: None,
                            },
                        ],
                    }
                })
                .collect(),
        }
    }

    fn passa_no_filtro_texto(filtro: &Filtro, textos: &[&str]) -> bool {
        match &filtro.texto {
            None => true,
            Some(t) => {
                let t = t.to_lowercase();
                textos.iter().any(|x| x.to_lowercase().contains(&t))
            }
        }
    }

    fn quem_no_filtro(filtro: &Filtro, autor: Option<&str>) -> bool {
        match &filtro.quem_decidiu {
            None => true,
            Some(Quem::Usuario) => autor == Some("usuario"),
            Some(Quem::Automatico(_)) => autor.is_some_and(|a| a != "usuario"),
        }
    }

    fn ciclo_no_filtro(filtro: &Filtro, ciclo: &str) -> bool {
        match &filtro.ciclos {
            None => true,
            Some((de, ate)) => ciclo >= de.as_str() && ciclo <= ate.as_str(),
        }
    }

    fn area(&self, area: sbf_inventario::consulta::Area, filtro: &Filtro) -> Tabela {
        use sbf_inventario::consulta::Area as A;
        let achado_passa = |a: &&Achado| {
            Self::passa_no_filtro_texto(
                filtro,
                &[
                    &a.nome,
                    a.fornecedor.as_deref().unwrap_or(""),
                    &a.tipo_de_documento,
                ],
            ) && a
                .data
                .as_deref()
                .is_none_or(|d| Self::ciclo_no_filtro(filtro, &ciclo_da_data(d)))
                && Self::quem_no_filtro(filtro, a.decidido_por.as_deref())
                && (filtro.estados.is_empty() || filtro.estados.contains(&a.estado))
                && (filtro.tipos.is_empty() || filtro.tipos.contains(&a.tipo_de_documento))
                && (filtro.fontes.is_empty()
                    || self
                        .origens_de(&a.hash)
                        .iter()
                        .any(|o| filtro.fontes.iter().any(|f| f == o.conexao)))
        };
        match area {
            A::ATriar => self.tabela_de_achados(
                self.achados
                    .iter()
                    .filter(|a| a.estado == "novo")
                    .filter(achado_passa),
            ),
            A::EmDuvida => self.tabela_de_achados(
                self.achados
                    .iter()
                    .filter(|a| a.estado == "duvida")
                    .filter(achado_passa),
            ),
            A::Documentos => self.tabela_de_achados(
                self.achados
                    .iter()
                    .filter(|a| a.estado == "documento")
                    .filter(achado_passa),
            ),
            A::Registros => self.tabela_de_registros(self.registros.iter().filter(|r| {
                Self::passa_no_filtro_texto(
                    filtro,
                    &[
                        &r.descricao,
                        &r.fornecedor,
                        &r.conta,
                        r.categoria.as_deref().unwrap_or(""),
                    ],
                ) && Self::ciclo_no_filtro(filtro, &r.ciclo)
                    && Self::quem_no_filtro(filtro, Some(r.decidido_por.as_str()))
            })),
            A::Ciclos => Tabela {
                colunas: vec![
                    col("Ciclo"),
                    col("Período"),
                    coln("Registros"),
                    coln("Saiu"),
                    coln("Entrou"),
                    coln("Documentos"),
                ],
                linhas: CICLOS
                    .iter()
                    .rev()
                    .filter(|c| Self::ciclo_no_filtro(filtro, c))
                    .map(|c| {
                        let alvo = Ref::nova(Tipo::Ciclo, *c);
                        let regs: Vec<&Registro> =
                            self.registros.iter().filter(|r| r.ciclo == *c).collect();
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), *c),
                                txt(periodo_do_ciclo(c)),
                                contagem(regs.len(), do_total(&alvo, None, None)),
                                valor(Self::saiu(&regs), do_total(&alvo, None, Some("saidas"))),
                                valor(Self::entrou(&regs), do_total(&alvo, None, Some("entradas"))),
                                contagem(
                                    self.ligados(&alvo, Tipo::Documento).len(),
                                    ligados(&alvo, Tipo::Documento),
                                ),
                            ],
                        }
                    })
                    .collect(),
            },
            A::Fornecedores => {
                let nomes: Vec<String> = self
                    .fornecedores()
                    .into_iter()
                    .filter(|f| Self::passa_no_filtro_texto(filtro, &[f]))
                    .collect();
                self.tabela_de_fornecedores(&nomes)
            }
            A::Faturas => self.tabela_de_faturas(self.faturas.iter().filter(|f| {
                Self::ciclo_no_filtro(filtro, &f.ciclo)
                    && Self::passa_no_filtro_texto(filtro, &[&f.chave])
            })),
            A::ContasLacunas => Tabela {
                colunas: vec![
                    col("Conta"),
                    coln("Registros"),
                    coln("Ciclos com registro"),
                    col("Ciclos sem registro (lacunas)"),
                    col("Último registro"),
                ],
                linhas: self
                    .contas()
                    .into_iter()
                    .filter(|c| Self::passa_no_filtro_texto(filtro, &[c]))
                    .map(|c| {
                        let alvo = Ref::nova(Tipo::Conta, c.clone());
                        let regs: Vec<&Registro> =
                            self.registros.iter().filter(|r| r.conta == c).collect();
                        let ciclos: BTreeSet<&str> =
                            regs.iter().map(|r| r.ciclo.as_str()).collect();
                        let lacunas: Vec<(Ref, String)> = CICLOS
                            .iter()
                            .filter(|x| !ciclos.contains(*x))
                            .map(|x| (Ref::nova(Tipo::Ciclo, *x), (*x).to_owned()))
                            .collect();
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), c.clone()),
                                contagem(regs.len(), do_total(&alvo, None, None)),
                                Celula::Contagem {
                                    n: ciclos.len() as i64,
                                    itens: None,
                                },
                                if lacunas.is_empty() {
                                    txt("nenhuma")
                                } else {
                                    Celula::Links(lacunas)
                                },
                                regs.iter()
                                    .map(|r| r.data.clone())
                                    .max()
                                    .map_or(Celula::Vazia, Celula::Data),
                            ],
                        }
                    })
                    .collect(),
            },
            A::Emails => Tabela {
                colunas: vec![
                    col("Remetente"),
                    col("Assunto"),
                    col("Anexo"),
                    col("Estado"),
                    col("Recebido em"),
                ],
                linhas: self
                    .achados
                    .iter()
                    .filter_map(|a| a.email.as_ref().map(|e| (a, e)))
                    .filter(|(a, (rem, ass))| {
                        Self::passa_no_filtro_texto(filtro, &[rem, ass, &a.nome])
                    })
                    .map(|(a, (rem, ass))| {
                        let alvo = Self::ref_do_achado(a);
                        let quando = self
                            .origens_de(&a.hash)
                            .iter()
                            .find(|o| o.conexao == "gmail:pessoal")
                            .map(|o| o.visto_em.clone())
                            .unwrap_or_default();
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                txt(rem.clone()),
                                txt(ass.clone()),
                                link(alvo, a.nome.clone()),
                                Celula::Estado {
                                    rotulo: rotulo_estado_achado(&a.estado).into(),
                                    quem: a.decidido_por.as_deref().map(quem),
                                },
                                Celula::Data(quando),
                            ],
                        }
                    })
                    .collect(),
            },
            A::ArquivosEPastas => {
                let refs: Vec<Ref> = self
                    .origens
                    .iter()
                    .filter(|o| {
                        o.conexao != "gmail:pessoal"
                            && Self::passa_no_filtro_texto(filtro, &[&o.caminho])
                    })
                    .map(|o| Ref::nova(Tipo::Origem, o.id.to_string()))
                    .collect();
                self.tabela_de_refs(Tipo::Origem, &refs)
            }
            A::EmailsPorRemetente => {
                let mut por: BTreeMap<String, (usize, BTreeSet<String>, String)> = BTreeMap::new();
                for a in &self.achados {
                    if let Some((rem, _)) = &a.email {
                        if !Self::passa_no_filtro_texto(filtro, &[rem]) {
                            continue;
                        }
                        let quando = self
                            .origens_de(&a.hash)
                            .iter()
                            .find(|o| o.conexao == "gmail:pessoal")
                            .map(|o| o.visto_em.clone())
                            .unwrap_or_default();
                        let e = por.entry(rem.clone()).or_default();
                        e.0 += 1;
                        e.1.insert(a.hash.clone());
                        if quando > e.2 {
                            e.2 = quando;
                        }
                    }
                }
                Tabela {
                    colunas: vec![
                        col("Remetente"),
                        coln("E-mails"),
                        coln("Anexos achados"),
                        col("Último"),
                    ],
                    linhas: por
                        .into_iter()
                        .map(|(rem, (n, anexos, ultimo))| Linha {
                            alvo: None,
                            celulas: vec![
                                txt(rem),
                                Celula::Contagem {
                                    n: n as i64,
                                    itens: None,
                                },
                                Celula::Contagem {
                                    n: anexos.len() as i64,
                                    itens: None,
                                },
                                Celula::Data(ultimo),
                            ],
                        })
                        .collect(),
                }
            }
            A::PastasPessoais => {
                let mut por: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
                for o in self.origens.iter().filter(|o| o.conexao != "gmail:pessoal") {
                    let pasta = o
                        .caminho
                        .rsplit_once('/')
                        .map_or(o.caminho.as_str(), |(p, _)| p)
                        .to_owned();
                    if Self::passa_no_filtro_texto(filtro, &[&pasta]) {
                        *por.entry((pasta, o.conexao)).or_default() += 1;
                    }
                }
                Tabela {
                    colunas: vec![col("Pasta"), col("Conexão"), coln("Arquivos")],
                    linhas: por
                        .into_iter()
                        .map(|((pasta, conexao), n)| Linha {
                            alvo: None,
                            celulas: vec![
                                txt(pasta),
                                link(
                                    Ref::nova(Tipo::Conexao, conexao),
                                    self.nome_conexao(conexao),
                                ),
                                Celula::Contagem {
                                    n: n as i64,
                                    itens: None,
                                },
                            ],
                        })
                        .collect(),
                }
            }
            A::Coletas => Tabela {
                colunas: vec![
                    col("Coleta"),
                    col("Conexão"),
                    col("Quando"),
                    col("Quem coletou"),
                    coln("Achados"),
                ],
                linhas: self
                    .coletas
                    .iter()
                    .rev()
                    .map(|c| self.linha_da_coleta(c))
                    .collect(),
            },
            A::Pendencias => Tabela {
                colunas: vec![
                    col("Pergunta"),
                    col("Sobre"),
                    col("Estado"),
                    col("Criada em"),
                    col("Resposta"),
                ],
                linhas: self
                    .pendencias
                    .iter()
                    .filter(|p| p.existe && Self::passa_no_filtro_texto(filtro, &[&p.pergunta]))
                    .map(|p| self.linha_da_pendencia(p))
                    .collect(),
            },
            A::Lotes => Tabela {
                colunas: vec![
                    col("Lote"),
                    col("Estado"),
                    coln("Itens"),
                    coln("Total"),
                    col("Criado em"),
                    col("Aprovado em"),
                ],
                linhas: self
                    .lotes
                    .iter()
                    .filter(|l| l.existe)
                    .map(|l| self.linha_do_lote(l))
                    .collect(),
            },
            A::VisaoGeral | A::Relatorios => Tabela::default(),
        }
    }

    fn saiu(regs: &[&Registro]) -> i64 {
        regs.iter()
            .filter(|r| r.valor < 0 && !e_movimento_entre_contas(&r.tipo))
            .map(|r| r.valor)
            .sum()
    }

    fn entrou(regs: &[&Registro]) -> i64 {
        regs.iter()
            .filter(|r| r.valor > 0 && !e_movimento_entre_contas(&r.tipo))
            .map(|r| r.valor)
            .sum()
    }

    // ------------------------------------------------------------ páginas

    fn item(&self, alvo: &Ref) -> Option<Item> {
        let campos = match alvo.tipo {
            Tipo::Achado | Tipo::Documento => {
                let a = self.achado(&alvo.chave)?;
                let r = Self::ref_do_achado(a);
                let mut c = vec![
                    campo(
                        "Estado",
                        Celula::Estado {
                            rotulo: rotulo_estado_achado(&a.estado).into(),
                            quem: a.decidido_por.as_deref().map(quem),
                        },
                    ),
                    campo("Hash", txt(a.hash.get(..12).unwrap_or(&a.hash).to_owned())),
                    campo("Tipo", txt(rotulo_tipo_doc(&a.tipo_de_documento))),
                    campo(
                        "Fornecedor",
                        a.fornecedor.as_ref().map_or(Celula::Vazia, |f| {
                            link(Ref::nova(Tipo::Fornecedor, f.clone()), f.clone())
                        }),
                    ),
                    campo(
                        "Ciclo",
                        a.data.as_ref().map_or(Celula::Vazia, |d| {
                            let c = ciclo_da_data(d);
                            link(Ref::nova(Tipo::Ciclo, c.clone()), c)
                        }),
                    ),
                    campo(
                        "Valor",
                        a.valor
                            .map_or(Celula::Vazia, |v| valor(v, do_total(&r, None, None))),
                    ),
                    campo("Data", a.data.clone().map_or(Celula::Vazia, Celula::Data)),
                    campo(
                        "Origens",
                        contagem(self.origens_de(&a.hash).len(), ligados(&r, Tipo::Origem)),
                    ),
                    campo(
                        "Registros",
                        contagem(
                            self.registros.iter().filter(|x| x.hash == a.hash).count(),
                            ligados(&r, Tipo::Registro),
                        ),
                    ),
                ];
                let lotes: Vec<(Ref, String)> = self
                    .lotes_com(&r)
                    .iter()
                    .map(|l| (Ref::nova(Tipo::Lote, l.id.to_string()), l.nome.clone()))
                    .collect();
                c.push(campo(
                    "Lote",
                    if lotes.is_empty() {
                        Celula::Vazia
                    } else {
                        Celula::Links(lotes)
                    },
                ));
                if a.protegido {
                    let regra = self.regras.iter().find(|x| {
                        a.fornecedor.as_deref() == Some(x.fornecedor.as_str())
                            && x.tipo_de_documento == a.tipo_de_documento
                    });
                    c.push(campo(
                        "Protegido por senha",
                        match regra {
                            Some(x) => link(
                                Ref::nova(Tipo::RegraDeSenha, x.id.to_string()),
                                "sim, com regra",
                            ),
                            None => txt("sim, sem regra"),
                        },
                    ));
                }
                (a.nome.clone(), c)
            }
            Tipo::Origem => {
                let o = self
                    .origens
                    .iter()
                    .find(|o| o.id.to_string() == alvo.chave)?;
                let a = self.achado(&o.hash)?;
                (
                    self.rotulo(alvo),
                    vec![
                        campo("Achado", link(Self::ref_do_achado(a), a.nome.clone())),
                        campo(
                            "Conexão",
                            link(
                                Ref::nova(Tipo::Conexao, o.conexao),
                                self.nome_conexao(o.conexao),
                            ),
                        ),
                        campo("Caminho · anexo · conversa", txt(o.caminho.clone())),
                        campo(
                            "Coleta",
                            link(
                                Ref::nova(Tipo::Coleta, o.coleta.to_string()),
                                format!("Coleta {}", o.coleta),
                            ),
                        ),
                        campo("Vista em", Celula::Data(o.visto_em.clone())),
                    ],
                )
            }
            Tipo::Registro => {
                let r = self.registro(&alvo.chave)?;
                let a = self.achado(&r.hash);
                let conciliados: Vec<(Ref, String)> = self
                    .ligados(alvo, Tipo::Registro)
                    .into_iter()
                    .map(|x| {
                        let t = self.rotulo(&x);
                        (x, t)
                    })
                    .collect();
                (
                    r.descricao.clone(),
                    vec![
                        campo(
                            "Estado",
                            Celula::Estado {
                                rotulo: rotulo_estado_registro(&r.estado).into(),
                                quem: Some(quem(&r.decidido_por)),
                            },
                        ),
                        campo(
                            "Fornecedor",
                            link(
                                Ref::nova(Tipo::Fornecedor, r.fornecedor.clone()),
                                r.fornecedor.clone(),
                            ),
                        ),
                        campo(
                            "Conta",
                            link(Ref::nova(Tipo::Conta, r.conta.clone()), r.conta.clone()),
                        ),
                        campo(
                            "Categoria",
                            r.categoria.as_ref().map_or(txt("não classificado"), |c| {
                                link(Ref::nova(Tipo::Categoria, c.clone()), c.clone())
                            }),
                        ),
                        campo(
                            "Tipo",
                            txt(format!(
                                "{}{}",
                                rotulo_tipo_registro(&r.tipo),
                                if e_movimento_entre_contas(&r.tipo) {
                                    " · não conta como gasto"
                                } else {
                                    ""
                                }
                            )),
                        ),
                        campo("Data", Celula::Data(r.data.clone())),
                        campo("Valor", valor(r.valor, None)),
                        campo(
                            "Ciclo",
                            link(Ref::nova(Tipo::Ciclo, r.ciclo.clone()), r.ciclo.clone()),
                        ),
                        campo(
                            "Documento",
                            a.map_or(Celula::Vazia, |a| {
                                link(Self::ref_do_achado(a), a.nome.clone())
                            }),
                        ),
                        campo(
                            "Fatura",
                            r.fatura.as_ref().map_or(Celula::Vazia, |f| {
                                link(Ref::nova(Tipo::Fatura, f.clone()), f.clone())
                            }),
                        ),
                        campo(
                            "Conciliado com",
                            if conciliados.is_empty() {
                                Celula::Vazia
                            } else {
                                Celula::Links(conciliados)
                            },
                        ),
                        campo(
                            "Movimentação",
                            txt("ainda não: a promoção chega na fatia 3"),
                        ),
                    ],
                )
            }
            Tipo::Decisao => {
                let d = self.decisao(&alvo.chave)?;
                let sobre = match d.alvo.tipo {
                    Tipo::Achado | Tipo::Documento => self
                        .achado(&d.alvo.chave)
                        .map(Self::ref_do_achado)
                        .unwrap_or_else(|| d.alvo.clone()),
                    _ => d.alvo.clone(),
                };
                (
                    self.rotulo_decisao(d),
                    vec![
                        campo("Sobre", link(sobre.clone(), self.rotulo(&sobre))),
                        campo("Campo", txt(d.campo.clone())),
                        campo("Antes", txt(d.antes.to_string())),
                        campo("Depois", txt(d.depois.to_string())),
                        campo("Quem", txt(quem(&d.autor).rotulo().to_owned())),
                        campo("Quando", Celula::Data(d.em.clone())),
                        campo("Motivo", d.motivo.clone().map_or(Celula::Vazia, txt)),
                        campo(
                            "Desfaz",
                            d.desfaz.map_or(Celula::Vazia, |x| {
                                link(
                                    Ref::nova(Tipo::Decisao, x.to_string()),
                                    format!("decisão {x}"),
                                )
                            }),
                        ),
                        campo(
                            "Desfeita por",
                            self.desfeita_por(d.id).map_or(Celula::Vazia, |x| {
                                link(
                                    Ref::nova(Tipo::Decisao, x.id.to_string()),
                                    format!("decisão {}", x.id),
                                )
                            }),
                        ),
                    ],
                )
            }
            Tipo::Lote => {
                let l = self.lote(&alvo.chave)?;
                let regs = self.registros_do_lote(l);
                let docs = l
                    .itens
                    .iter()
                    .filter(|i| matches!(i.tipo, Tipo::Documento | Tipo::Achado))
                    .count();
                (
                    l.nome.clone(),
                    vec![
                        campo(
                            "Estado",
                            Celula::Estado {
                                rotulo: if l.aprovado_em.is_some() {
                                    "Aprovado".into()
                                } else {
                                    "Em montagem".into()
                                },
                                quem: Some(Quem::Usuario),
                            },
                        ),
                        campo("Documentos", contagem(docs, ligados(alvo, Tipo::Documento))),
                        campo(
                            "Registros",
                            contagem(regs.len(), do_total(alvo, None, None)),
                        ),
                        campo(
                            "Total",
                            valor(
                                regs.iter().map(|r| r.valor).sum(),
                                do_total(alvo, None, None),
                            ),
                        ),
                        campo("Criado em", Celula::Data(l.criado_em.clone())),
                        campo(
                            "Aprovado em",
                            l.aprovado_em.clone().map_or(Celula::Vazia, Celula::Data),
                        ),
                        campo(
                            "Promoção",
                            txt("Enviar ao servidor chega na fatia 3, com a caixa de entrada"),
                        ),
                    ],
                )
            }
            Tipo::Pendencia => {
                let p = self.pendencia(&alvo.chave)?;
                let sobre = self
                    .ligados(alvo, Tipo::Achado)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| p.alvo.clone());
                (
                    p.pergunta.clone(),
                    vec![
                        campo(
                            "Estado",
                            Celula::Estado {
                                rotulo: if p.resposta.is_some() {
                                    "Respondida".into()
                                } else {
                                    "Aberta".into()
                                },
                                quem: None,
                            },
                        ),
                        campo("Sobre", link(sobre.clone(), self.rotulo(&sobre))),
                        campo("Pergunta", txt(p.pergunta.clone())),
                        campo("Resposta", p.resposta.clone().map_or(Celula::Vazia, txt)),
                        campo("Criada em", Celula::Data(p.criada_em.clone())),
                        campo(
                            "Resolvida em",
                            p.resolvida_em.clone().map_or(Celula::Vazia, Celula::Data),
                        ),
                    ],
                )
            }
            Tipo::RegraDeSenha => {
                let x = self
                    .regras
                    .iter()
                    .find(|x| x.id.to_string() == alvo.chave)?;
                (
                    self.rotulo(alvo),
                    vec![
                        campo(
                            "Fornecedor",
                            link(
                                Ref::nova(Tipo::Fornecedor, x.fornecedor.clone()),
                                x.fornecedor.clone(),
                            ),
                        ),
                        campo(
                            "Tipo de documento",
                            txt(rotulo_tipo_doc(&x.tipo_de_documento)),
                        ),
                        campo("Estratégia", txt(x.estrategia.clone())),
                        campo("Senha", txt("fica no chaveiro, nunca aparece aqui")),
                        campo("Criada em", Celula::Data(x.criada_em.clone())),
                    ],
                )
            }
            Tipo::Conexao => {
                let c = self.conexoes.iter().find(|c| c.id == alvo.chave)?;
                let ultima = self
                    .coletas
                    .iter()
                    .filter(|x| x.conexao == c.id)
                    .map(|x| x.quando.clone())
                    .max();
                let achados: BTreeSet<&str> = self
                    .origens
                    .iter()
                    .filter(|o| o.conexao == c.id)
                    .map(|o| o.hash.as_str())
                    .collect();
                (
                    c.nome.to_owned(),
                    vec![
                        campo("Tipo", txt(c.tipo)),
                        campo(
                            "Agenda",
                            txt("sem agenda: varrida sob demanda (agenda na fatia 4)"),
                        ),
                        campo(
                            "Coletas",
                            contagem(
                                self.coletas.iter().filter(|x| x.conexao == c.id).count(),
                                ligados(alvo, Tipo::Coleta),
                            ),
                        ),
                        campo(
                            "Achados",
                            contagem(achados.len(), ligados(alvo, Tipo::Achado)),
                        ),
                        campo(
                            "Última coleta",
                            ultima.map_or(txt("nenhuma: fonte planejada"), Celula::Data),
                        ),
                    ],
                )
            }
            Tipo::Coleta => {
                let c = self
                    .coletas
                    .iter()
                    .find(|c| c.id.to_string() == alvo.chave)?;
                let origens = self.origens.iter().filter(|o| o.coleta == c.id).count();
                let achados: BTreeSet<&str> = self
                    .origens
                    .iter()
                    .filter(|o| o.coleta == c.id)
                    .map(|o| o.hash.as_str())
                    .collect();
                (
                    self.rotulo(alvo),
                    vec![
                        campo(
                            "Conexão",
                            link(
                                Ref::nova(Tipo::Conexao, c.conexao),
                                self.nome_conexao(c.conexao),
                            ),
                        ),
                        campo("Quando", Celula::Data(c.quando.clone())),
                        campo("Quem coletou", txt(c.responsavel)),
                        campo("Origens", contagem(origens, ligados(alvo, Tipo::Origem))),
                        campo(
                            "Achados",
                            contagem(achados.len(), ligados(alvo, Tipo::Achado)),
                        ),
                    ],
                )
            }
            Tipo::Ciclo => {
                if !CICLOS.contains(&alvo.chave.as_str())
                    && !self.registros.iter().any(|r| r.ciclo == alvo.chave)
                {
                    return None;
                }
                let regs: Vec<&Registro> = self
                    .registros
                    .iter()
                    .filter(|r| r.ciclo == alvo.chave)
                    .collect();
                (
                    format!("Ciclo {}", alvo.chave),
                    vec![
                        campo("Período", txt(periodo_do_ciclo(&alvo.chave))),
                        campo(
                            "Saiu",
                            valor(Self::saiu(&regs), do_total(alvo, None, Some("saidas"))),
                        ),
                        campo(
                            "Entrou",
                            valor(Self::entrou(&regs), do_total(alvo, None, Some("entradas"))),
                        ),
                        campo(
                            "Registros",
                            contagem(regs.len(), do_total(alvo, None, None)),
                        ),
                        campo(
                            "Documentos",
                            contagem(
                                self.ligados(alvo, Tipo::Documento).len(),
                                ligados(alvo, Tipo::Documento),
                            ),
                        ),
                        campo("Gasto", txt("movimento entre contas fica de fora")),
                    ],
                )
            }
            Tipo::Fornecedor => {
                let regs: Vec<&Registro> = self
                    .registros
                    .iter()
                    .filter(|r| r.fornecedor == alvo.chave)
                    .collect();
                if regs.is_empty()
                    && !self
                        .achados
                        .iter()
                        .any(|a| a.fornecedor.as_deref() == Some(alvo.chave.as_str()))
                {
                    return None;
                }
                let no_ciclo: Vec<&Registro> = regs
                    .iter()
                    .copied()
                    .filter(|r| r.ciclo == CICLO_ATUAL)
                    .collect();
                let apelidos = self
                    .apelidos
                    .get(&alvo.chave)
                    .map(|v| v.join(" · "))
                    .unwrap_or_default();
                let categoria = regs.iter().find_map(|r| r.categoria.clone());
                (
                    alvo.chave.clone(),
                    vec![
                        campo(
                            &format!("No ciclo {CICLO_ATUAL}"),
                            valor(
                                no_ciclo.iter().map(|r| r.valor).sum(),
                                do_total(alvo, Some(CICLO_ATUAL), None),
                            ),
                        ),
                        campo(
                            "Lido no inventário",
                            valor(
                                regs.iter().map(|r| r.valor).sum(),
                                do_total(alvo, None, None),
                            ),
                        ),
                        campo(
                            "Registros",
                            contagem(regs.len(), do_total(alvo, None, None)),
                        ),
                        campo("Recorrente", txt(self.texto_recorrente(&alvo.chave))),
                        campo(
                            "Categoria",
                            categoria.map_or(Celula::Vazia, |c| {
                                link(Ref::nova(Tipo::Categoria, c.clone()), c)
                            }),
                        ),
                        campo(
                            "Apelidos",
                            if apelidos.is_empty() {
                                Celula::Vazia
                            } else {
                                txt(apelidos)
                            },
                        ),
                        campo("Contratos", txt("chegam na fatia 2")),
                        campo("Total no livro-caixa", txt("chega na fatia 2")),
                    ],
                )
            }
            Tipo::Conta => {
                let regs: Vec<&Registro> = self
                    .registros
                    .iter()
                    .filter(|r| r.conta == alvo.chave)
                    .collect();
                if regs.is_empty() {
                    return None;
                }
                let ciclos: BTreeSet<&str> = regs.iter().map(|r| r.ciclo.as_str()).collect();
                let lacunas: Vec<(Ref, String)> = CICLOS
                    .iter()
                    .filter(|x| !ciclos.contains(*x))
                    .map(|x| (Ref::nova(Tipo::Ciclo, *x), (*x).to_owned()))
                    .collect();
                (
                    alvo.chave.clone(),
                    vec![
                        campo(
                            "Saiu",
                            valor(Self::saiu(&regs), do_total(alvo, None, Some("saidas"))),
                        ),
                        campo(
                            "Entrou",
                            valor(Self::entrou(&regs), do_total(alvo, None, Some("entradas"))),
                        ),
                        campo(
                            "Registros",
                            contagem(regs.len(), do_total(alvo, None, None)),
                        ),
                        campo(
                            "Ciclos com registro",
                            Celula::Contagem {
                                n: ciclos.len() as i64,
                                itens: None,
                            },
                        ),
                        campo(
                            "Lacunas",
                            if lacunas.is_empty() {
                                txt("nenhuma")
                            } else {
                                Celula::Links(lacunas)
                            },
                        ),
                        campo(
                            "Faturas",
                            contagem(
                                self.faturas
                                    .iter()
                                    .filter(|f| f.conta == alvo.chave)
                                    .count(),
                                ligados(alvo, Tipo::Fatura),
                            ),
                        ),
                    ],
                )
            }
            Tipo::Fatura => {
                let f = self.faturas.iter().find(|f| f.chave == alvo.chave)?;
                let soma: i64 = self
                    .registros
                    .iter()
                    .filter(|r| r.fatura.as_deref() == Some(f.chave.as_str()))
                    .map(|r| r.valor)
                    .sum();
                let a = self.achado(&f.hash);
                (
                    f.chave.clone(),
                    vec![
                        campo(
                            "Estado",
                            Celula::Estado {
                                rotulo: if soma == f.declarado {
                                    "Aprovado · fecha".into()
                                } else {
                                    "Divergente · não fecha".into()
                                },
                                quem: None,
                            },
                        ),
                        campo(
                            "Cartão",
                            link(Ref::nova(Tipo::Conta, f.conta.clone()), f.conta.clone()),
                        ),
                        campo(
                            "Ciclo",
                            link(Ref::nova(Tipo::Ciclo, f.ciclo.clone()), f.ciclo.clone()),
                        ),
                        campo("Declarado", valor(f.declarado, None)),
                        campo(
                            "Soma dos registros",
                            valor(soma, do_total(alvo, None, None)),
                        ),
                        campo("Diferença", valor(f.declarado - soma, None)),
                        campo(
                            "Documento",
                            a.map_or(Celula::Vazia, |a| {
                                link(Self::ref_do_achado(a), a.nome.clone())
                            }),
                        ),
                    ],
                )
            }
            _ => return None,
        };
        Some(Item {
            alvo: alvo.clone(),
            rotulo: campos.0,
            campos: campos.1,
        })
    }

    fn elo_de(&self, r: &Ref, papel: &str, autor: Option<&str>, quando: Option<&str>) -> Elo {
        elo(r.clone(), self.rotulo(r), papel, autor, quando)
    }

    /// O rastro de um achado/documento: decisões, origens, coletas e conexões.
    fn acima_do_achado(&self, a: &Achado) -> Vec<Elo> {
        let mut v = Vec::new();
        let ultima = self
            .decisoes
            .iter()
            .rev()
            .find(|d| d.alvo.chave == a.hash && d.campo == "estado");
        if let Some(d) = ultima {
            v.push(self.elo_de(
                &Ref::nova(Tipo::Decisao, d.id.to_string()),
                "triado por",
                Some(&d.autor),
                Some(&d.em),
            ));
        }
        for o in self.origens_de(&a.hash) {
            let origem = Ref::nova(Tipo::Origem, o.id.to_string());
            v.push(self.elo_de(&origem, "achado em", None, Some(&o.visto_em)));
            if let Some(c) = self.coletas.iter().find(|c| c.id == o.coleta) {
                v.push(self.elo_de(
                    &Ref::nova(Tipo::Coleta, c.id.to_string()),
                    "na coleta",
                    Some(c.responsavel),
                    Some(&c.quando),
                ));
            }
            v.push(self.elo_de(
                &Ref::nova(Tipo::Conexao, o.conexao),
                "pela conexão",
                None,
                None,
            ));
        }
        v
    }

    fn rastro(&self, alvo: &Ref) -> Option<Rastro> {
        let mut acima = Vec::new();
        let mut abaixo = Vec::new();
        match alvo.tipo {
            Tipo::Achado | Tipo::Documento => {
                let a = self.achado(&alvo.chave)?;
                acima = self.acima_do_achado(a);
                for r in self.registros.iter().filter(|r| r.hash == a.hash) {
                    abaixo.push(self.elo_de(
                        &Self::ref_do_registro(r),
                        "virou registro",
                        Some("regras@2"),
                        None,
                    ));
                }
                for l in self.lotes_com(&Self::ref_do_achado(a)) {
                    abaixo.push(self.elo_de(
                        &Ref::nova(Tipo::Lote, l.id.to_string()),
                        "no lote",
                        Some("usuario"),
                        Some(&l.criado_em),
                    ));
                }
            }
            Tipo::Registro => {
                let r = self.registro(&alvo.chave)?;
                let a = self.achado(&r.hash)?;
                let aprov = self
                    .decisoes
                    .iter()
                    .rev()
                    .find(|d| d.alvo.tipo == Tipo::Registro && d.alvo.chave == alvo.chave);
                if let Some(d) = aprov {
                    acima.push(self.elo_de(
                        &Ref::nova(Tipo::Decisao, d.id.to_string()),
                        "decidido por",
                        Some(&d.autor),
                        Some(&d.em),
                    ));
                }
                acima.push(self.elo_de(
                    &Self::ref_do_achado(a),
                    "extraído de",
                    Some("regras@2"),
                    None,
                ));
                acima.extend(self.acima_do_achado(a));
                for l in self
                    .lotes_com(alvo)
                    .into_iter()
                    .chain(self.lotes_com(&Self::ref_do_achado(a)))
                {
                    let papel = if l.aprovado_em.is_some() {
                        "aprovado no lote"
                    } else {
                        "no lote"
                    };
                    abaixo.push(self.elo_de(
                        &Ref::nova(Tipo::Lote, l.id.to_string()),
                        papel,
                        Some("usuario"),
                        l.aprovado_em.as_deref().or(Some(&l.criado_em)),
                    ));
                }
            }
            Tipo::Origem => {
                let o = self
                    .origens
                    .iter()
                    .find(|o| o.id.to_string() == alvo.chave)?;
                if let Some(c) = self.coletas.iter().find(|c| c.id == o.coleta) {
                    acima.push(self.elo_de(
                        &Ref::nova(Tipo::Coleta, c.id.to_string()),
                        "na coleta",
                        Some(c.responsavel),
                        Some(&c.quando),
                    ));
                }
                acima.push(self.elo_de(
                    &Ref::nova(Tipo::Conexao, o.conexao),
                    "pela conexão",
                    None,
                    None,
                ));
                if let Some(a) = self.achado(&o.hash) {
                    abaixo.push(self.elo_de(
                        &Self::ref_do_achado(a),
                        "achou",
                        None,
                        Some(&o.visto_em),
                    ));
                    for r in self.registros.iter().filter(|r| r.hash == a.hash) {
                        abaixo.push(self.elo_de(
                            &Self::ref_do_registro(r),
                            "virou registro",
                            Some("regras@2"),
                            None,
                        ));
                    }
                }
            }
            Tipo::Decisao => {
                let d = self.decisao(&alvo.chave)?;
                let sobre = self
                    .ligados(alvo, d.alvo.tipo)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| d.alvo.clone());
                acima.push(self.elo_de(&sobre, "sobre", None, None));
                if let Some(x) = d.desfaz {
                    acima.push(self.elo_de(
                        &Ref::nova(Tipo::Decisao, x.to_string()),
                        "desfaz",
                        Some(&d.autor),
                        Some(&d.em),
                    ));
                }
                if let Some(x) = self.desfeita_por(d.id) {
                    abaixo.push(self.elo_de(
                        &Ref::nova(Tipo::Decisao, x.id.to_string()),
                        "desfeita por",
                        Some(&x.autor),
                        Some(&x.em),
                    ));
                }
            }
            Tipo::Lote => {
                let l = self.lote(&alvo.chave)?;
                for i in self.ligados(alvo, Tipo::Achado).into_iter().take(12) {
                    acima.push(self.elo_de(&i, "reúne", Some("usuario"), Some(&l.criado_em)));
                }
            }
            Tipo::Pendencia => {
                let p = self.pendencia(&alvo.chave)?;
                let sobre = self
                    .ligados(alvo, Tipo::Achado)
                    .into_iter()
                    .chain(self.ligados(alvo, Tipo::Registro))
                    .chain(self.ligados(alvo, Tipo::Lote))
                    .next()
                    .unwrap_or_else(|| p.alvo.clone());
                acima.push(self.elo_de(&sobre, "trava", None, Some(&p.criada_em)));
                for d in self.ligados(alvo, Tipo::Decisao) {
                    abaixo.push(self.elo_de(&d, "decisão", Some("usuario"), None));
                }
            }
            Tipo::RegraDeSenha => {
                let x = self
                    .regras
                    .iter()
                    .find(|x| x.id.to_string() == alvo.chave)?;
                acima.push(self.elo_de(
                    &Ref::nova(Tipo::Fornecedor, x.fornecedor.clone()),
                    "do fornecedor",
                    Some("usuario"),
                    Some(&x.criada_em),
                ));
                for d in self.ligados(alvo, Tipo::Documento) {
                    abaixo.push(self.elo_de(&d, "abre", None, None));
                }
            }
            Tipo::Coleta => {
                let c = self
                    .coletas
                    .iter()
                    .find(|c| c.id.to_string() == alvo.chave)?;
                acima.push(self.elo_de(
                    &Ref::nova(Tipo::Conexao, c.conexao),
                    "pela conexão",
                    Some(c.responsavel),
                    Some(&c.quando),
                ));
                for o in self.ligados(alvo, Tipo::Origem).into_iter().take(12) {
                    abaixo.push(self.elo_de(&o, "trouxe", None, None));
                }
            }
            Tipo::Conexao => {
                self.conexoes.iter().find(|c| c.id == alvo.chave)?;
                for c in self.ligados(alvo, Tipo::Coleta) {
                    abaixo.push(self.elo_de(&c, "coleta", None, None));
                }
            }
            Tipo::Fornecedor | Tipo::Conta | Tipo::Ciclo | Tipo::Fatura => {
                self.item(alvo)?;
                for d in self
                    .ligados(alvo, Tipo::Documento)
                    .into_iter()
                    .rev()
                    .take(8)
                {
                    acima.push(self.elo_de(&d, "declarado em", Some("regras@2"), None));
                }
                for r in self.ligados(alvo, Tipo::Registro).into_iter().rev().take(8) {
                    abaixo.push(self.elo_de(&r, "registro", None, None));
                }
            }
            _ => return None,
        }
        Some(Rastro { acima, abaixo })
    }

    // ------------------------------------------------------------ painéis

    fn visao_geral(&self) -> Painel {
        let conta = |estado: &str| self.achados.iter().filter(|a| a.estado == estado).count();
        let por = |estado: &str, autor: &str| {
            self.achados
                .iter()
                .filter(|a| a.estado == estado && a.decidido_por.as_deref() == Some(autor))
                .count()
        };
        let area = |a: sbf_inventario::consulta::Area| {
            Some(PedidoDeTabela::Area {
                area: a,
                filtro: Filtro::default(),
            })
        };
        use sbf_inventario::consulta::Area as A;
        let resumo = Tabela {
            colunas: vec![col("Item"), coln("Quantos")],
            linhas: vec![
                Linha {
                    alvo: None,
                    celulas: vec![txt("Achados"), contagem(self.achados.len(), None)],
                },
                Linha {
                    alvo: None,
                    celulas: vec![
                        txt("Documentos"),
                        contagem(conta("documento"), area(A::Documentos)),
                    ],
                },
                Linha {
                    alvo: None,
                    celulas: vec![
                        txt("Origens"),
                        contagem(self.origens.len(), area(A::ArquivosEPastas)),
                    ],
                },
                Linha {
                    alvo: None,
                    celulas: vec![
                        txt("Registros"),
                        contagem(self.registros.len(), area(A::Registros)),
                    ],
                },
                Linha {
                    alvo: None,
                    celulas: vec![
                        txt("Pendências abertas"),
                        contagem(
                            self.pendencias
                                .iter()
                                .filter(|p| p.existe && p.resposta.is_none())
                                .count(),
                            area(A::Pendencias),
                        ),
                    ],
                },
                Linha {
                    alvo: None,
                    celulas: vec![
                        txt("Lotes"),
                        contagem(
                            self.lotes.iter().filter(|l| l.existe).count(),
                            area(A::Lotes),
                        ),
                    ],
                },
            ],
        };
        let etapas = Tabela {
            colunas: vec![
                col("Estado do achado"),
                coln("Total"),
                coln("Por regras@2"),
                coln("Por você"),
            ],
            linhas: [
                ("A triar", "novo", Some(A::ATriar)),
                ("Documento", "documento", Some(A::Documentos)),
                ("Em dúvida", "duvida", Some(A::EmDuvida)),
                ("Descartado", "descartado", None),
            ]
            .into_iter()
            .map(|(rotulo, estado, a)| Linha {
                alvo: None,
                celulas: vec![
                    Celula::Estado {
                        rotulo: rotulo.into(),
                        quem: None,
                    },
                    contagem(conta(estado), a.and_then(area)),
                    contagem(por(estado, "regras@2"), None),
                    contagem(por(estado, "usuario"), None),
                ],
            })
            .collect(),
        };
        let fontes = self.tabela_de_refs(
            Tipo::Conexao,
            &self
                .conexoes
                .iter()
                .map(|c| Ref::nova(Tipo::Conexao, c.id))
                .collect::<Vec<_>>(),
        );
        let ciclos = Tabela {
            colunas: vec![
                col("Ciclo"),
                coln("Documentos"),
                coln("Registros"),
                coln("Saiu"),
            ],
            linhas: CICLOS
                .iter()
                .rev()
                .map(|c| {
                    let alvo = Ref::nova(Tipo::Ciclo, *c);
                    let regs: Vec<&Registro> =
                        self.registros.iter().filter(|r| r.ciclo == *c).collect();
                    Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            link(alvo.clone(), *c),
                            contagem(
                                self.ligados(&alvo, Tipo::Documento).len(),
                                ligados(&alvo, Tipo::Documento),
                            ),
                            contagem(regs.len(), do_total(&alvo, None, None)),
                            valor(Self::saiu(&regs), do_total(&alvo, None, Some("saidas"))),
                        ],
                    }
                })
                .collect(),
        };
        let mut avisos = Vec::new();
        for c in &self.conexoes {
            if !self.coletas.iter().any(|x| x.conexao == c.id) {
                avisos.push(format!("{}: fonte planejada sem coleta.", c.nome));
            }
        }
        let protegidos = self.achados.iter().filter(|a| a.protegido).count();
        if protegidos > 0 {
            avisos.push(format!(
                "{protegidos} PDFs protegidos esperando regra de senha."
            ));
        }
        Painel {
            avisos,
            secoes: vec![
                Secao {
                    titulo: "Resumo".into(),
                    nota: None,
                    tabela: resumo,
                },
                Secao {
                    titulo: "Etapas da triagem".into(),
                    nota: Some("sempre com quem decidiu".into()),
                    tabela: etapas,
                },
                Secao {
                    titulo: "Fontes".into(),
                    nota: Some("conexões sem agenda nesta fatia".into()),
                    tabela: fontes,
                },
                Secao {
                    titulo: "Últimos ciclos".into(),
                    nota: Some(format!(
                        "registros do inventário · {}",
                        crate::formato::A_CONFERIR
                    )),
                    tabela: ciclos,
                },
            ],
        }
    }

    fn relatorio(&self, pergunta: Pergunta, filtro: &Filtro) -> Painel {
        let regs: Vec<&Registro> = self
            .registros
            .iter()
            .filter(|r| {
                Self::ciclo_no_filtro(filtro, &r.ciclo)
                    && Self::passa_no_filtro_texto(filtro, &[&r.fornecedor, &r.descricao])
            })
            .collect();
        let gastos: Vec<&Registro> = regs
            .iter()
            .copied()
            .filter(|r| r.valor < 0 && !e_movimento_entre_contas(&r.tipo))
            .collect();
        let nota = Some(format!(
            "registros do inventário · {} · movimento entre contas fica de fora",
            crate::formato::A_CONFERIR
        ));
        let por_fornecedor = |lista: &[&Registro]| -> BTreeMap<String, Vec<i64>> {
            let mut m: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for r in lista {
                m.entry(r.fornecedor.clone()).or_default().push(r.valor);
            }
            m
        };
        let forn = |f: &str| link(Ref::nova(Tipo::Fornecedor, f), f);
        match pergunta {
            Pergunta::QuantoSaiu => {
                let ciclos: BTreeSet<&str> = regs.iter().map(|r| r.ciclo.as_str()).collect();
                let linhas = ciclos
                    .iter()
                    .rev()
                    .map(|c| {
                        let alvo = Ref::nova(Tipo::Ciclo, *c);
                        let doc: Vec<&Registro> =
                            regs.iter().copied().filter(|r| r.ciclo == *c).collect();
                        Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                link(alvo.clone(), *c),
                                valor(Self::saiu(&doc), do_total(&alvo, None, Some("saidas"))),
                                valor(Self::entrou(&doc), do_total(&alvo, None, Some("entradas"))),
                                contagem(doc.len(), do_total(&alvo, None, None)),
                            ],
                        }
                    })
                    .collect();
                let atual: Vec<&Registro> = regs
                    .iter()
                    .copied()
                    .filter(|r| r.ciclo == CICLO_ATUAL)
                    .collect();
                let ciclo = Ref::nova(Tipo::Ciclo, CICLO_ATUAL);
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![
                        Secao {
                            titulo: format!("Saiu no ciclo {CICLO_ATUAL}"),
                            nota: nota.clone(),
                            tabela: Tabela {
                                colunas: vec![col("Ciclo"), coln("Saiu"), coln("Registros")],
                                linhas: vec![Linha {
                                    alvo: Some(ciclo.clone()),
                                    celulas: vec![
                                        link(ciclo.clone(), CICLO_ATUAL),
                                        valor(
                                            Self::saiu(&atual),
                                            do_total(&ciclo, None, Some("saidas")),
                                        ),
                                        contagem(atual.len(), do_total(&ciclo, None, None)),
                                    ],
                                }],
                            },
                        },
                        Secao {
                            titulo: "Nos últimos ciclos".into(),
                            nota,
                            tabela: Tabela {
                                colunas: vec![
                                    col("Ciclo"),
                                    coln("Saiu"),
                                    coln("Entrou"),
                                    coln("Registros"),
                                ],
                                linhas,
                            },
                        },
                    ],
                }
            }
            Pergunta::MaioresFornecedores | Pergunta::OndeSeConcentra => {
                let mut lista: Vec<(String, i64, usize)> = por_fornecedor(&gastos)
                    .into_iter()
                    .map(|(f, v)| (f, v.iter().sum(), v.len()))
                    .collect();
                lista.sort_by_key(|x| x.1);
                let total: i64 = lista.iter().map(|x| x.1).sum::<i64>().min(-1);
                let fornecedores = Tabela {
                    colunas: vec![
                        coln("#"),
                        col("Fornecedor"),
                        coln("Saiu"),
                        coln("Registros"),
                        coln("% do total"),
                    ],
                    linhas: lista
                        .iter()
                        .take(10)
                        .enumerate()
                        .map(|(i, (f, v, n))| {
                            let alvo = Ref::nova(Tipo::Fornecedor, f.clone());
                            Linha {
                                alvo: Some(alvo.clone()),
                                celulas: vec![
                                    Celula::Contagem {
                                        n: i as i64 + 1,
                                        itens: None,
                                    },
                                    forn(f),
                                    valor(*v, do_total(&alvo, None, None)),
                                    contagem(*n, do_total(&alvo, None, None)),
                                    txt(format!("{}%", v * 100 / total)),
                                ],
                            }
                        })
                        .collect(),
                };
                if pergunta == Pergunta::MaioresFornecedores {
                    return Painel {
                        avisos: Vec::new(),
                        secoes: vec![Secao {
                            titulo: "Maiores fornecedores".into(),
                            nota,
                            tabela: fornecedores,
                        }],
                    };
                }
                let mut cats: BTreeMap<String, i64> = BTreeMap::new();
                for r in &gastos {
                    *cats
                        .entry(
                            r.categoria
                                .clone()
                                .unwrap_or_else(|| "não classificado".into()),
                        )
                        .or_default() += r.valor;
                }
                let mut cats: Vec<(String, i64)> = cats.into_iter().collect();
                cats.sort_by_key(|x| x.1);
                let categorias = Tabela {
                    colunas: vec![col("Categoria"), coln("Saiu"), coln("% do total")],
                    linhas: cats
                        .iter()
                        .map(|(c, v)| Linha {
                            alvo: None,
                            celulas: vec![
                                if c == "não classificado" {
                                    txt(c.clone())
                                } else {
                                    link(Ref::nova(Tipo::Categoria, c.clone()), c.clone())
                                },
                                valor(*v, None),
                                txt(format!("{}%", v * 100 / total)),
                            ],
                        })
                        .collect(),
                };
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![
                        Secao {
                            titulo: "Por categoria".into(),
                            nota: nota.clone(),
                            tabela: categorias,
                        },
                        Secao {
                            titulo: "Por fornecedor".into(),
                            nota,
                            tabela: fornecedores,
                        },
                    ],
                }
            }
            Pergunta::OQueSeRepete => {
                let linhas = por_fornecedor(&gastos)
                    .into_keys()
                    .filter_map(|f| {
                        let doc: Vec<&Registro> = gastos
                            .iter()
                            .copied()
                            .filter(|r| r.fornecedor == f)
                            .collect();
                        let ciclos: BTreeSet<&str> = doc.iter().map(|r| r.ciclo.as_str()).collect();
                        if ciclos.len() < 3 {
                            return None;
                        }
                        let media = doc.iter().map(|r| r.valor).sum::<i64>() / ciclos.len() as i64;
                        let constante = doc.iter().all(|r| r.valor == doc[0].valor);
                        let alvo = Ref::nova(Tipo::Fornecedor, f.clone());
                        Some(Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                forn(&f),
                                Celula::Contagem {
                                    n: ciclos.len() as i64,
                                    itens: None,
                                },
                                valor(media, do_total(&alvo, None, None)),
                                txt(if constante {
                                    "assinatura (valor fixo)"
                                } else {
                                    "conta de consumo"
                                }),
                                txt(self.texto_recorrente(&f)),
                            ],
                        })
                    })
                    .collect();
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![Secao {
                        titulo: "O que se repete todo ciclo".into(),
                        nota,
                        tabela: Tabela {
                            colunas: vec![
                                col("Fornecedor"),
                                coln("Ciclos"),
                                coln("Média por ciclo"),
                                col("Jeito"),
                                col("Recorrência"),
                            ],
                            linhas,
                        },
                    }],
                }
            }
            Pergunta::OQueAumentou => {
                let linhas = por_fornecedor(&gastos)
                    .into_keys()
                    .filter_map(|f| {
                        let agora: i64 = gastos
                            .iter()
                            .filter(|r| r.fornecedor == f && r.ciclo == CICLO_ATUAL)
                            .map(|r| r.valor)
                            .sum();
                        let antes: Vec<i64> = CICLOS[..5]
                            .iter()
                            .map(|c| {
                                gastos
                                    .iter()
                                    .filter(|r| r.fornecedor == f && r.ciclo == *c)
                                    .map(|r| r.valor)
                                    .sum()
                            })
                            .filter(|v: &i64| *v != 0)
                            .collect();
                        if antes.is_empty() || agora == 0 {
                            return None;
                        }
                        let media = antes.iter().sum::<i64>() / antes.len() as i64;
                        // Valores negativos: aumentou quando saiu mais (mais negativo).
                        if agora >= media || (media - agora) * 10 < -media {
                            return None;
                        }
                        let alvo = Ref::nova(Tipo::Fornecedor, f.clone());
                        Some(Linha {
                            alvo: Some(alvo.clone()),
                            celulas: vec![
                                forn(&f),
                                valor(media, None),
                                valor(agora, do_total(&alvo, Some(CICLO_ATUAL), None)),
                                valor(agora - media, None),
                                txt(format!("{}%", (agora - media) * 100 / media)),
                            ],
                        })
                    })
                    .collect();
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![Secao {
                        titulo: format!("O que aumentou no ciclo {CICLO_ATUAL}"),
                        nota: Some(format!(
                            "contra a média dos ciclos anteriores · {}",
                            crate::formato::A_CONFERIR
                        )),
                        tabela: Tabela {
                            colunas: vec![
                                col("Fornecedor"),
                                coln("Média antes"),
                                coln("Agora"),
                                coln("Diferença"),
                                coln("Aumento"),
                            ],
                            linhas,
                        },
                    }],
                }
            }
            Pergunta::NaoClassificado => {
                let sem: Vec<&Registro> = regs
                    .iter()
                    .copied()
                    .filter(|r| r.categoria.is_none() && !e_movimento_entre_contas(&r.tipo))
                    .collect();
                let mut t = self.tabela_de_registros(sem.into_iter());
                t.colunas.truncate(10);
                use sbf_inventario::consulta::Area as A;
                let achados = Tabela {
                    colunas: vec![col("Achados sem decisão final"), coln("Quantos")],
                    linhas: vec![
                        Linha {
                            alvo: None,
                            celulas: vec![
                                txt("A triar"),
                                contagem(
                                    self.achados.iter().filter(|a| a.estado == "novo").count(),
                                    Some(PedidoDeTabela::Area {
                                        area: A::ATriar,
                                        filtro: Filtro::default(),
                                    }),
                                ),
                            ],
                        },
                        Linha {
                            alvo: None,
                            celulas: vec![
                                txt("Em dúvida"),
                                contagem(
                                    self.achados.iter().filter(|a| a.estado == "duvida").count(),
                                    Some(PedidoDeTabela::Area {
                                        area: A::EmDuvida,
                                        filtro: Filtro::default(),
                                    }),
                                ),
                            ],
                        },
                    ],
                };
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![
                        Secao {
                            titulo: "Registros sem categoria".into(),
                            nota,
                            tabela: t,
                        },
                        Secao {
                            titulo: "Achados ainda sem decisão".into(),
                            nota: None,
                            tabela: achados,
                        },
                    ],
                }
            }
            Pergunta::PossiveisDuplicidades => {
                let mut linhas = Vec::new();
                for (i, a) in self.achados.iter().enumerate() {
                    for b in self.achados.iter().skip(i + 1) {
                        if a.valor.is_some()
                            && a.valor == b.valor
                            && a.fornecedor.is_some()
                            && a.fornecedor == b.fornecedor
                            && a.data == b.data
                            && !a.duplicidades.contains(&b.hash)
                            && !b.duplicidades.contains(&a.hash)
                        {
                            linhas.push(Linha {
                                alvo: Some(Self::ref_do_achado(b)),
                                celulas: vec![
                                    link(Self::ref_do_achado(a), a.nome.clone()),
                                    link(Self::ref_do_achado(b), b.nome.clone()),
                                    a.fornecedor.as_deref().map_or(Celula::Vazia, forn),
                                    valor(a.valor.unwrap_or(0), None),
                                    a.data.clone().map_or(Celula::Vazia, Celula::Data),
                                    txt("mesmo fornecedor, valor e data; conteúdo diferente"),
                                ],
                            });
                        }
                    }
                }
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![Secao {
                        titulo: "Possíveis duplicidades".into(),
                        nota: Some(
                            "conteúdo idêntico não aparece aqui: é o mesmo achado com duas origens"
                                .into(),
                        ),
                        tabela: Tabela {
                            colunas: vec![
                                col("Achado"),
                                col("Parece com"),
                                col("Fornecedor"),
                                coln("Valor"),
                                col("Data"),
                                col("Por quê"),
                            ],
                            linhas,
                        },
                    }],
                }
            }
            Pergunta::OQueFaltaParaConfiar => {
                let mut contas = self.area(
                    sbf_inventario::consulta::Area::ContasLacunas,
                    &Filtro::default(),
                );
                contas.linhas.retain(
                    |l| !matches!(l.celulas.get(3), Some(Celula::Texto(t)) if t == "nenhuma"),
                );
                let mut faturas = self.tabela_de_faturas(self.faturas.iter());
                faturas.linhas.retain(|l| matches!(l.celulas.get(5), Some(Celula::Valor { centavos, .. }) if *centavos != 0));
                let fontes = Tabela {
                    colunas: vec![col("Fonte sem coleta"), col("Tipo")],
                    linhas: self
                        .conexoes
                        .iter()
                        .filter(|c| !self.coletas.iter().any(|x| x.conexao == c.id))
                        .map(|c| Linha {
                            alvo: Some(Ref::nova(Tipo::Conexao, c.id)),
                            celulas: vec![
                                link(Ref::nova(Tipo::Conexao, c.id), c.nome),
                                txt(c.tipo),
                            ],
                        })
                        .collect(),
                };
                Painel {
                    avisos: Vec::new(),
                    secoes: vec![
                        Secao {
                            titulo: "Lacunas por conta".into(),
                            nota: Some("ciclos sem nenhum registro".into()),
                            tabela: contas,
                        },
                        Secao {
                            titulo: "Faturas que não fecham".into(),
                            nota: Some("o declarado não bate com a soma dos registros".into()),
                            tabela: faturas,
                        },
                        Secao {
                            titulo: "Fontes sem coleta".into(),
                            nota: None,
                            tabela: fontes,
                        },
                    ],
                }
            }
        }
    }

    fn itens_do_total(&self, de: &Ref, ciclo: Option<&str>, recorte: Option<&str>) -> Tabela {
        let regs = self
            .registros_de(de)
            .into_iter()
            .filter(|r| ciclo.is_none_or(|c| r.ciclo == c))
            .filter(|r| match recorte {
                Some("saidas") => r.valor < 0 && !e_movimento_entre_contas(&r.tipo),
                Some("entradas") => r.valor > 0 && !e_movimento_entre_contas(&r.tipo),
                _ => true,
            });
        self.tabela_de_registros(regs)
    }

    // ------------------------------------------------------------ escrita

    /// Aplica um valor a um campo de um alvo. É a projeção: decidir e desfazer passam aqui.
    fn aplicar(&mut self, alvo: &Ref, campo: &str, v: &Value) -> Result<(), ErroDeComando> {
        let nao = || ErroDeComando::NaoEncontrado(alvo.clone());
        let texto = |v: &Value| v.as_str().map(str::to_owned);
        match (alvo.tipo, campo) {
            (Tipo::Achado | Tipo::Documento, _) => {
                let i = self
                    .achados
                    .iter()
                    .position(|a| a.hash == alvo.chave)
                    .ok_or_else(nao)?;
                let a = &mut self.achados[i];
                match campo {
                    "estado" => a.estado = texto(v).unwrap_or_else(|| "novo".into()),
                    "fornecedor" => a.fornecedor = texto(v),
                    "document_type" => a.tipo_de_documento = texto(v).unwrap_or_default(),
                    "data_e_valor" => {
                        a.data = v.get("data").and_then(Value::as_str).map(str::to_owned);
                        a.valor = v.get("valor_centavos").and_then(Value::as_i64);
                    }
                    "duplicidade" => {
                        if let Some(com) = v.get("com").and_then(Value::as_str) {
                            if v.get("ligada").and_then(Value::as_bool).unwrap_or(false) {
                                a.duplicidades.insert(com.to_owned());
                            } else {
                                a.duplicidades.remove(com);
                            }
                        }
                    }
                    "decidido_por" => a.decidido_por = texto(v),
                    _ => {}
                }
            }
            (Tipo::Registro, _) => {
                let id: i64 = alvo.chave.parse().map_err(|_| nao())?;
                let i = self
                    .registros
                    .iter()
                    .position(|r| r.id == id)
                    .ok_or_else(nao)?;
                let r = &mut self.registros[i];
                match campo {
                    "estado" => r.estado = texto(v).unwrap_or_else(|| "extraido".into()),
                    "fornecedor" => r.fornecedor = texto(v).unwrap_or_default(),
                    "categoria" => r.categoria = texto(v),
                    "tipo" => r.tipo = texto(v).unwrap_or_default(),
                    "decidido_por" => {
                        r.decidido_por = texto(v).unwrap_or_else(|| "regras@2".into())
                    }
                    "conciliacao" => {
                        if let Some(com) = v.get("com").and_then(Value::as_i64) {
                            if v.get("ligada").and_then(Value::as_bool).unwrap_or(false) {
                                r.conciliados.insert(com);
                            } else {
                                r.conciliados.remove(&com);
                            }
                        }
                    }
                    _ => {}
                }
            }
            (Tipo::Fornecedor, "recorrente") => {
                if v.as_bool().unwrap_or(false) {
                    self.recorrentes.insert(alvo.chave.clone());
                } else {
                    self.recorrentes.remove(&alvo.chave);
                }
            }
            (Tipo::Fornecedor, "regra_senha") => match v.get("id").and_then(Value::as_i64) {
                Some(id) => {
                    if !self.regras.iter().any(|r| r.id == id) {
                        let criada_em = self.agora();
                        self.regras.push(RegraDeSenha {
                            id,
                            fornecedor: alvo.chave.clone(),
                            tipo_de_documento: v
                                .get("document_type")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            estrategia: v
                                .get("estrategia")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            criada_em,
                        });
                    }
                }
                None => {
                    if let Some(id) = v.get("remover").and_then(Value::as_i64) {
                        self.regras.retain(|r| r.id != id);
                    }
                }
            },
            (Tipo::Lote, _) => {
                let id: i64 = alvo.chave.parse().map_err(|_| nao())?;
                let i = self.lotes.iter().position(|l| l.id == id).ok_or_else(nao)?;
                match campo {
                    "existe" => self.lotes[i].existe = v.as_bool().unwrap_or(false),
                    "itens" => {
                        self.lotes[i].itens = v
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str())
                                    .filter_map(|s| s.parse::<Ref>().ok())
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                    "aprovado" => {
                        self.lotes[i].aprovado_em = v
                            .get("aprovado_em")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        if let Some(m) = v.get("estados").and_then(Value::as_object) {
                            for (rid, est) in m {
                                if let (Ok(rid), Some(est)) = (rid.parse::<i64>(), est.as_str())
                                    && let Some(r) = self.registros.iter_mut().find(|r| r.id == rid)
                                {
                                    r.estado = est.to_owned();
                                    r.decidido_por = "usuario".into();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            (Tipo::Pendencia, _) => {
                let id: i64 = alvo.chave.parse().map_err(|_| nao())?;
                let i = self
                    .pendencias
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(nao)?;
                match campo {
                    "existe" => self.pendencias[i].existe = v.as_bool().unwrap_or(false),
                    "resposta" => {
                        let resposta = texto(v);
                        let quando = resposta.as_ref().map(|_| self.agora());
                        self.pendencias[i].resposta = resposta;
                        self.pendencias[i].resolvida_em = quando;
                    }
                    _ => {}
                }
            }
            _ => {
                return Err(ErroDeComando::NaoSeAplica {
                    acao: campo.to_owned(),
                    alvo: alvo.clone(),
                });
            }
        }
        Ok(())
    }

    fn valor_atual(&self, alvo: &Ref, campo: &str) -> Value {
        match alvo.tipo {
            Tipo::Achado | Tipo::Documento => match self.achado(&alvo.chave) {
                Some(a) => match campo {
                    "estado" => json!(a.estado),
                    "fornecedor" => json!(a.fornecedor),
                    "document_type" => json!(a.tipo_de_documento),
                    "data_e_valor" => json!({"data": a.data, "valor_centavos": a.valor}),
                    "decidido_por" => json!(a.decidido_por),
                    _ => Value::Null,
                },
                None => Value::Null,
            },
            Tipo::Registro => match self.registro(&alvo.chave) {
                Some(r) => match campo {
                    "estado" => json!(r.estado),
                    "fornecedor" => json!(r.fornecedor),
                    "categoria" => json!(r.categoria),
                    "tipo" => json!(r.tipo),
                    _ => Value::Null,
                },
                None => Value::Null,
            },
            Tipo::Fornecedor if campo == "recorrente" => {
                json!(self.recorrentes.contains(&alvo.chave))
            }
            _ => Value::Null,
        }
    }

    /// Grava a decisão e aplica a projeção, tudo ou nada (a "mesma transação").
    fn decidir(
        &mut self,
        alvo: Ref,
        campo: &str,
        depois: Value,
        motivo: Option<String>,
    ) -> Result<i64, ErroDeComando> {
        let antes = self.valor_atual(&alvo, campo);
        let copia = self.clone();
        let r = self.aplicar(&alvo, campo, &depois).and_then(|()| {
            // A decisão do usuário prevalece sobre as regras: marca quem decidiu.
            if campo == "estado"
                && matches!(alvo.tipo, Tipo::Achado | Tipo::Documento | Tipo::Registro)
            {
                self.aplicar(&alvo, "decidido_por", &json!("usuario"))?;
            }
            Ok(self.registrar(alvo.clone(), campo, antes, depois, "usuario", motivo))
        });
        if r.is_err() {
            *self = copia;
        }
        r
    }

    fn e_achado(alvo: &Ref) -> bool {
        matches!(alvo.tipo, Tipo::Achado | Tipo::Documento)
    }

    fn executar(&mut self, pedido: &PedidoDeDecisao) -> ResultadoDeComando<Vec<IdDecisao>> {
        let copia = self.clone();
        let r = self.executar_tudo(pedido);
        if r.is_err() {
            *self = copia;
        }
        r
    }

    fn executar_tudo(&mut self, pedido: &PedidoDeDecisao) -> ResultadoDeComando<Vec<IdDecisao>> {
        let nome_da_acao = format!("{:?}", pedido.acao);
        let nao_se_aplica = |alvo: &Ref| ErroDeComando::NaoSeAplica {
            acao: nome_da_acao.clone(),
            alvo: alvo.clone(),
        };
        let mut ids = Vec::new();
        for alvo in &pedido.alvos {
            let existe = match alvo.tipo {
                Tipo::Achado | Tipo::Documento => self.achado(&alvo.chave).is_some(),
                Tipo::Registro => self.registro(&alvo.chave).is_some(),
                Tipo::Lote => self.lote(&alvo.chave).is_some(),
                Tipo::Pendencia => self.pendencia(&alvo.chave).is_some(),
                Tipo::Fornecedor => self.fornecedores().contains(&alvo.chave),
                _ => false,
            };
            if !existe {
                return Err(ErroDeComando::NaoEncontrado(alvo.clone()));
            }
            let motivo = pedido.motivo.clone();
            let id = match &pedido.acao {
                Acao::MarcarDocumento | Acao::Descartar | Acao::EmDuvida => {
                    if !Self::e_achado(alvo) {
                        return Err(nao_se_aplica(alvo));
                    }
                    let estado = match pedido.acao {
                        Acao::MarcarDocumento => "documento",
                        Acao::Descartar => "descartado",
                        _ => "duvida",
                    };
                    self.decidir(
                        Ref::nova(Tipo::Achado, alvo.chave.clone()),
                        "estado",
                        json!(estado),
                        motivo,
                    )?
                }
                Acao::CorrigirFornecedor(f) => match alvo.tipo {
                    Tipo::Registro => self.decidir(alvo.clone(), "fornecedor", json!(f), motivo)?,
                    _ if Self::e_achado(alvo) => self.decidir(
                        Ref::nova(Tipo::Achado, alvo.chave.clone()),
                        "fornecedor",
                        json!(f),
                        motivo,
                    )?,
                    _ => return Err(nao_se_aplica(alvo)),
                },
                Acao::CorrigirCategoria(c) => {
                    if alvo.tipo != Tipo::Registro {
                        return Err(nao_se_aplica(alvo));
                    }
                    self.decidir(alvo.clone(), "categoria", json!(c), motivo)?
                }
                Acao::CorrigirTipo(t) => match alvo.tipo {
                    Tipo::Registro => self.decidir(alvo.clone(), "tipo", json!(t), motivo)?,
                    _ if Self::e_achado(alvo) => self.decidir(
                        Ref::nova(Tipo::Achado, alvo.chave.clone()),
                        "document_type",
                        json!(t),
                        motivo,
                    )?,
                    _ => return Err(nao_se_aplica(alvo)),
                },
                Acao::CorrigirDataEValor {
                    data,
                    valor_centavos,
                } => {
                    if !Self::e_achado(alvo) {
                        return Err(nao_se_aplica(alvo));
                    }
                    let a = self
                        .achado(&alvo.chave)
                        .ok_or_else(|| ErroDeComando::NaoEncontrado(alvo.clone()))?;
                    let depois = json!({"data": data.clone().or(a.data.clone()), "valor_centavos": valor_centavos.or(a.valor)});
                    self.decidir(
                        Ref::nova(Tipo::Achado, alvo.chave.clone()),
                        "data_e_valor",
                        depois,
                        motivo,
                    )?
                }
                Acao::MarcarDuplicidade { com } => {
                    if !Self::e_achado(alvo)
                        || !Self::e_achado(com)
                        || com.chave == alvo.chave
                        || self.achado(&com.chave).is_none()
                    {
                        return Err(nao_se_aplica(alvo));
                    }
                    let alvo_a = Ref::nova(Tipo::Achado, alvo.chave.clone());
                    let id = self.registrar(
                        alvo_a.clone(),
                        "duplicidade",
                        json!({"com": com.chave, "ligada": false}),
                        json!({"com": com.chave, "ligada": true}),
                        "usuario",
                        motivo,
                    );
                    self.aplicar(
                        &alvo_a,
                        "duplicidade",
                        &json!({"com": com.chave, "ligada": true}),
                    )?;
                    id
                }
                Acao::UnirConciliados { com } => {
                    let com_id: i64 = com.chave.parse().map_err(|_| nao_se_aplica(alvo))?;
                    if alvo.tipo != Tipo::Registro
                        || com.tipo != Tipo::Registro
                        || com.chave == alvo.chave
                        || self.registro(&com.chave).is_none()
                    {
                        return Err(nao_se_aplica(alvo));
                    }
                    let id = self.registrar(
                        alvo.clone(),
                        "conciliacao",
                        json!({"com": com_id, "ligada": false}),
                        json!({"com": com_id, "ligada": true}),
                        "usuario",
                        motivo,
                    );
                    self.aplicar(alvo, "conciliacao", &json!({"com": com_id, "ligada": true}))?;
                    id
                }
                Acao::ConfirmarRecorrencia => {
                    if alvo.tipo != Tipo::Fornecedor {
                        return Err(nao_se_aplica(alvo));
                    }
                    self.decidir(alvo.clone(), "recorrente", json!(true), motivo)?
                }
                Acao::Aprovar | Acao::Rejeitar => {
                    if alvo.tipo != Tipo::Registro {
                        return Err(nao_se_aplica(alvo));
                    }
                    if pedido.acao == Acao::Rejeitar
                        && motivo.as_deref().is_none_or(|m| m.trim().is_empty())
                    {
                        return Err(ErroDeComando::MotivoObrigatorio);
                    }
                    let estado = if pedido.acao == Acao::Aprovar {
                        "aprovado"
                    } else {
                        "divergente"
                    };
                    self.decidir(alvo.clone(), "estado", json!(estado), motivo)?
                }
                Acao::Sinalizar { pergunta } => {
                    if !matches!(
                        alvo.tipo,
                        Tipo::Achado | Tipo::Documento | Tipo::Registro | Tipo::Lote
                    ) {
                        return Err(nao_se_aplica(alvo));
                    }
                    let id = self.pendencias.len() as i64 + 1;
                    let criada_em = self.agora();
                    let alvo_p = if Self::e_achado(alvo) {
                        Ref::nova(Tipo::Achado, alvo.chave.clone())
                    } else {
                        alvo.clone()
                    };
                    self.pendencias.push(Pendencia {
                        id,
                        alvo: alvo_p,
                        existe: false,
                        pergunta: pergunta.clone(),
                        resposta: None,
                        criada_em,
                        resolvida_em: None,
                    });
                    self.decidir(
                        Ref::nova(Tipo::Pendencia, id.to_string()),
                        "existe",
                        json!(true),
                        motivo,
                    )?
                }
                Acao::Responder { resposta } => {
                    if alvo.tipo != Tipo::Pendencia {
                        return Err(nao_se_aplica(alvo));
                    }
                    let antes = self.pendencia(&alvo.chave).and_then(|p| p.resposta.clone());
                    let id = self.registrar(
                        alvo.clone(),
                        "resposta",
                        json!(antes),
                        json!(resposta),
                        "usuario",
                        motivo,
                    );
                    self.aplicar(alvo, "resposta", &json!(resposta))?;
                    id
                }
                Acao::GuardarRegraDeSenha {
                    tipo_de_documento,
                    estrategia,
                    ..
                } => {
                    if alvo.tipo != Tipo::Fornecedor {
                        return Err(nao_se_aplica(alvo));
                    }
                    let nova = self.regras.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                    let depois = json!({"id": nova, "document_type": tipo_de_documento, "estrategia": estrategia});
                    let id = self.registrar(
                        alvo.clone(),
                        "regra_senha",
                        json!({"remover": nova}),
                        depois.clone(),
                        "usuario",
                        motivo,
                    );
                    self.aplicar(alvo, "regra_senha", &depois)?;
                    id
                }
            };
            ids.push(id);
        }
        Ok(ids)
    }

    fn desfazer(&mut self, id: IdDecisao) -> ResultadoDeComando<IdDecisao> {
        let d = self
            .decisoes
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or(ErroDeComando::NadaADesfazer(id))?;
        if self.desfeita_por(id).is_some() {
            return Err(ErroDeComando::NadaADesfazer(id));
        }
        let copia = self.clone();
        let r = self.aplicar(&d.alvo, &d.campo, &d.antes).map(|()| {
            let nova = self.registrar(
                d.alvo.clone(),
                &d.campo,
                d.depois.clone(),
                d.antes.clone(),
                "usuario",
                None,
            );
            if let Some(x) = self.decisoes.iter_mut().find(|x| x.id == nova) {
                x.desfaz = Some(id);
            }
            nova
        });
        if r.is_err() {
            *self = copia;
        }
        r
    }

    fn desfazer_ultima(&mut self) -> ResultadoDeComando<Option<IdDecisao>> {
        let alvo = self
            .decisoes
            .iter()
            .rev()
            .find(|d| {
                d.autor == "usuario" && d.desfaz.is_none() && self.desfeita_por(d.id).is_none()
            })
            .map(|d| d.id);
        match alvo {
            Some(id) => self.desfazer(id).map(Some),
            None => Ok(None),
        }
    }

    fn lote_editavel(&self, lote: &Ref) -> ResultadoDeComando<&Lote> {
        let l = self
            .lote(&lote.chave)
            .ok_or_else(|| ErroDeComando::NaoEncontrado(lote.clone()))?;
        if l.aprovado_em.is_some() {
            return Err(ErroDeComando::LoteAprovado(lote.clone()));
        }
        Ok(l)
    }

    fn mudar_itens(&mut self, lote: &Ref, itens: Vec<Ref>) {
        let antes: Vec<String> = self
            .lote(&lote.chave)
            .map(|l| l.itens.iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        let depois: Vec<String> = itens.iter().map(ToString::to_string).collect();
        self.registrar(
            lote.clone(),
            "itens",
            json!(antes),
            json!(depois),
            "usuario",
            None,
        );
        if let Some(l) = self
            .lotes
            .iter_mut()
            .find(|l| l.id.to_string() == lote.chave)
        {
            l.itens = itens;
        }
    }
}

impl Consulta for ConsultaDemonstracao {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        let e = self.0.borrow();
        Ok(match pedido {
            PedidoDeTabela::Area { area, filtro } => e.area(*area, filtro),
            PedidoDeTabela::Ligados { de, tipo } => {
                if let Some(m) = tipo.motivo_da_fatia() {
                    return Err(ErroDeConsulta::NaoSuportado(m));
                }
                let refs = e.ligados(de, *tipo);
                e.tabela_de_refs(*tipo, &refs)
            }
            PedidoDeTabela::ItensDoTotal {
                de,
                fonte,
                ciclo,
                recorte,
            } => {
                if *fonte == Fonte::LivroCaixa {
                    return Err(ErroDeConsulta::NaoSuportado(
                        "Chega na fatia 2, com o livro-caixa".into(),
                    ));
                }
                e.itens_do_total(de, ciclo.as_deref(), recorte.as_deref())
            }
        })
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        Ok(self.0.borrow().item(alvo))
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        self.0
            .borrow()
            .rastro(alvo)
            .ok_or_else(|| ErroDeConsulta::NaoEncontrado(alvo.clone()))
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        if fonte == Fonte::LivroCaixa {
            return Err(ErroDeConsulta::NaoSuportado(
                "Chega na fatia 2, com o livro-caixa".into(),
            ));
        }
        let e = self.0.borrow();
        let inicio = CICLOS.len().saturating_sub(ciclos);
        Ok(CICLOS[inicio..]
            .iter()
            .map(|c| TotalDoCiclo {
                ciclo: (*c).to_owned(),
                centavos: e
                    .registros_de(alvo)
                    .iter()
                    .filter(|r| r.ciclo == *c)
                    .map(|r| r.valor)
                    .sum(),
                itens: do_total(alvo, Some(c), None).map(Box::new),
            })
            .collect())
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        Ok(self.0.borrow().visao_geral())
    }

    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        if fonte == Fonte::LivroCaixa {
            return Err(ErroDeConsulta::NaoSuportado(
                "Chega na fatia 2, com o livro-caixa".into(),
            ));
        }
        Ok(self.0.borrow().relatorio(pergunta, filtro))
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        let e = self.0.borrow();
        let n = |s: &str| e.achados.iter().filter(|a| a.estado == s).count() as i64;
        Ok(Contagens {
            achados: e.achados.len() as i64,
            documentos: n("documento"),
            em_duvida: n("duvida"),
            a_triar: n("novo"),
        })
    }

    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        Ok(self
            .0
            .borrow()
            .achado(&achado.chave)
            .and_then(|a| a.texto.clone()))
    }
}

impl Comandos for ComandosDemonstracao {
    fn executar(&mut self, pedido: &PedidoDeDecisao) -> ResultadoDeComando<Vec<IdDecisao>> {
        self.0.borrow_mut().executar(pedido)
    }

    fn desfazer(&mut self, decisao: IdDecisao) -> ResultadoDeComando<IdDecisao> {
        self.0.borrow_mut().desfazer(decisao)
    }

    fn desfazer_ultima(&mut self) -> ResultadoDeComando<Option<IdDecisao>> {
        self.0.borrow_mut().desfazer_ultima()
    }

    fn criar_lote(&mut self, nome: &str) -> ResultadoDeComando<Ref> {
        let mut e = self.0.borrow_mut();
        let id = e.lotes.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        let criado_em = e.agora();
        e.lotes.push(Lote {
            id,
            nome: nome.to_owned(),
            existe: false,
            criado_em,
            aprovado_em: None,
            itens: Vec::new(),
        });
        let alvo = Ref::nova(Tipo::Lote, id.to_string());
        e.decidir(alvo.clone(), "existe", json!(true), None)?;
        Ok(alvo)
    }

    fn adicionar_ao_lote(&mut self, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
        let mut e = self.0.borrow_mut();
        let mut lista = e.lote_editavel(lote)?.itens.clone();
        for i in itens {
            let normal = match i.tipo {
                Tipo::Achado | Tipo::Documento => match e.achado(&i.chave) {
                    Some(a) if a.estado == "documento" => {
                        Ref::nova(Tipo::Documento, a.hash.clone())
                    }
                    Some(_) => {
                        return Err(ErroDeComando::NaoSeAplica {
                            acao: "adicionar ao lote (só documentos)".into(),
                            alvo: i.clone(),
                        });
                    }
                    None => return Err(ErroDeComando::NaoEncontrado(i.clone())),
                },
                Tipo::Registro if e.registro(&i.chave).is_some() => i.clone(),
                _ => {
                    return Err(ErroDeComando::NaoSeAplica {
                        acao: "adicionar ao lote".into(),
                        alvo: i.clone(),
                    });
                }
            };
            if !lista.contains(&normal) {
                lista.push(normal);
            }
        }
        e.mudar_itens(lote, lista);
        Ok(())
    }

    fn remover_do_lote(&mut self, lote: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
        let mut e = self.0.borrow_mut();
        let mut lista = e.lote_editavel(lote)?.itens.clone();
        lista.retain(|x| !itens.iter().any(|i| i.chave == x.chave));
        e.mudar_itens(lote, lista);
        Ok(())
    }

    fn aprovar_lote(&mut self, lote: &Ref) -> ResultadoDeComando<()> {
        let mut e = self.0.borrow_mut();
        let l = e.lote_editavel(lote)?.clone();
        let regs: Vec<(i64, String)> = e
            .registros_do_lote(&l)
            .iter()
            .map(|r| (r.id, r.estado.clone()))
            .collect();
        let antes = json!({"aprovado_em": null, "estados": regs.iter().map(|(id, est)| (id.to_string(), json!(est))).collect::<serde_json::Map<_, _>>()});
        let quando = e.agora();
        let depois = json!({"aprovado_em": quando, "estados": regs.iter().map(|(id, _)| (id.to_string(), json!("aprovado"))).collect::<serde_json::Map<_, _>>()});
        e.registrar(
            lote.clone(),
            "aprovado",
            antes,
            depois.clone(),
            "usuario",
            None,
        );
        e.aplicar(lote, "aprovado", &depois)
    }

    fn desaprovar_lote(&mut self, lote: &Ref) -> ResultadoDeComando<()> {
        let mut e = self.0.borrow_mut();
        let ultima = e
            .decisoes
            .iter()
            .rev()
            .find(|d| d.alvo == *lote && d.campo == "aprovado" && d.desfaz.is_none())
            .map(|d| d.id)
            .ok_or_else(|| ErroDeComando::NaoSeAplica {
                acao: "desaprovar".into(),
                alvo: lote.clone(),
            })?;
        if e.lote(&lote.chave).is_some_and(|l| l.aprovado_em.is_none()) {
            return Err(ErroDeComando::NaoSeAplica {
                acao: "desaprovar (o lote não está aprovado)".into(),
                alvo: lote.clone(),
            });
        }
        e.desfazer(ultima).map(|_| ())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_inventario::consulta::Area;

    fn demo() -> (ConsultaDemonstracao, ComandosDemonstracao) {
        nova()
    }

    fn primeiro_a_triar(c: &ConsultaDemonstracao) -> Ref {
        let t = c
            .tabela(&PedidoDeTabela::Area {
                area: Area::ATriar,
                filtro: Filtro::default(),
            })
            .expect("tabela");
        t.linhas[0].alvo.clone().expect("alvo")
    }

    #[test]
    fn dados_coerentes_toda_ref_de_link_abre_uma_pagina() {
        let (c, _) = demo();
        let mut alvos = BTreeSet::new();
        for area in crate::areas::AREAS_DO_INVENTARIO {
            let t = c
                .tabela(&PedidoDeTabela::Area {
                    area,
                    filtro: Filtro::default(),
                })
                .expect("tabela");
            for l in &t.linhas {
                for cel in &l.celulas {
                    match cel {
                        Celula::Link { alvo, .. } => {
                            alvos.insert(alvo.clone());
                        }
                        Celula::Links(v) => alvos.extend(v.iter().map(|(r, _)| r.clone())),
                        _ => {}
                    }
                }
            }
        }
        assert!(alvos.len() > 50);
        for a in alvos {
            if a.tipo.fatia() == 1 {
                assert!(c.item(&a).expect("item").is_some(), "link sem página: {a}");
                assert!(c.rastro(&a).is_ok(), "sem rastro: {a}");
            }
        }
    }

    #[test]
    fn decidir_muda_o_estado_e_desfazer_restaura() {
        let (c, mut k) = demo();
        let alvo = primeiro_a_triar(&c);
        let antes = c.contagens().expect("contagens");
        let ids = k
            .executar(&PedidoDeDecisao {
                acao: Acao::MarcarDocumento,
                alvos: vec![alvo.clone()],
                motivo: None,
            })
            .expect("decidir");
        assert_eq!(ids.len(), 1);
        let depois = c.contagens().expect("contagens");
        assert_eq!(depois.a_triar, antes.a_triar - 1);
        assert_eq!(depois.documentos, antes.documentos + 1);
        let desfez = k.desfazer_ultima().expect("desfazer");
        assert!(desfez.is_some());
        assert_eq!(c.contagens().expect("contagens"), antes);
        // Desfazer de novo a mesma decisão não vale.
        assert!(k.desfazer(ids[0]).is_err());
    }

    #[test]
    fn rejeitar_exige_motivo_e_nada_muda_sem_ele() {
        let (c, mut k) = demo();
        let reg = Ref::nova(Tipo::Registro, "1");
        let antes = c.item(&reg).expect("item");
        assert!(matches!(
            k.executar(&PedidoDeDecisao {
                acao: Acao::Rejeitar,
                alvos: vec![reg.clone()],
                motivo: None
            }),
            Err(ErroDeComando::MotivoObrigatorio)
        ));
        assert_eq!(c.item(&reg).expect("item"), antes);
        k.executar(&PedidoDeDecisao {
            acao: Acao::Rejeitar,
            alvos: vec![reg.clone()],
            motivo: Some("valor errado".into()),
        })
        .expect("rejeitar");
        let item = c.item(&reg).expect("item").expect("existe");
        assert!(
            matches!(&item.campos[0].valor, Celula::Estado { rotulo, quem: Some(Quem::Usuario) } if rotulo == "Divergente")
        );
    }

    #[test]
    fn acao_que_nao_se_aplica_nao_grava_nada() {
        let (c, mut k) = demo();
        let alvo = primeiro_a_triar(&c);
        let reg = Ref::nova(Tipo::Registro, "1");
        // O segundo alvo falha: o primeiro não pode ficar decidido (tudo ou nada).
        let r = k.executar(&PedidoDeDecisao {
            acao: Acao::Descartar,
            alvos: vec![alvo.clone(), reg],
            motivo: None,
        });
        assert!(r.is_err());
        let item = c.item(&alvo).expect("item").expect("existe");
        assert!(
            matches!(&item.campos[0].valor, Celula::Estado { rotulo, .. } if rotulo == "A triar")
        );
    }

    #[test]
    fn lote_aprovado_marca_os_registros_e_desaprovar_volta() {
        let (c, mut k) = demo();
        let lote = k.criar_lote("Teste").expect("lote");
        let reg = Ref::nova(Tipo::Registro, "60");
        k.adicionar_ao_lote(&lote, std::slice::from_ref(&reg))
            .expect("adicionar");
        let estado = |c: &ConsultaDemonstracao| match &c
            .item(&reg)
            .expect("item")
            .expect("existe")
            .campos[0]
            .valor
        {
            Celula::Estado { rotulo, .. } => rotulo.clone(),
            _ => String::new(),
        };
        let antes = estado(&c);
        k.aprovar_lote(&lote).expect("aprovar");
        assert_eq!(estado(&c), "Aprovado");
        assert!(matches!(
            k.adicionar_ao_lote(&lote, &[Ref::nova(Tipo::Registro, "61")]),
            Err(ErroDeComando::LoteAprovado(_))
        ));
        k.desaprovar_lote(&lote).expect("desaprovar");
        assert_eq!(estado(&c), antes);
        // Achado que não é documento não entra no lote.
        let a_triar = primeiro_a_triar(&c);
        assert!(k.adicionar_ao_lote(&lote, &[a_triar]).is_err());
    }

    #[test]
    fn todo_total_do_fornecedor_abre_os_registros_que_o_somam() {
        let (c, _) = demo();
        let f = Ref::nova(Tipo::Fornecedor, "Energia Exemplo");
        let item = c.item(&f).expect("item").expect("existe");
        for campo in &item.campos {
            if let Celula::Valor {
                centavos,
                itens: Some(p),
                ..
            } = &campo.valor
            {
                let t = c.tabela(p).expect("tabela");
                let soma: i64 = t
                    .linhas
                    .iter()
                    .filter_map(|l| {
                        l.celulas.iter().find_map(|x| match x {
                            Celula::Valor { centavos, .. } => Some(*centavos),
                            _ => None,
                        })
                    })
                    .sum();
                assert_eq!(soma, *centavos, "campo {}", campo.rotulo);
            }
        }
        let barras = c.por_ciclo(&f, Fonte::Inventario, 6).expect("por ciclo");
        assert_eq!(barras.len(), 6);
        assert_eq!(barras[5].centavos, -31244);
    }

    #[test]
    fn rastro_do_registro_vai_ate_a_conexao_e_ao_lote() {
        let (c, _) = demo();
        let lote = c
            .tabela(&PedidoDeTabela::Ligados {
                de: Ref::nova(Tipo::Lote, "1"),
                tipo: Tipo::Registro,
            })
            .expect("tabela");
        let reg = lote.linhas[0].alvo.clone().expect("alvo");
        let r = c.rastro(&reg).expect("rastro");
        assert!(r.acima.iter().any(|e| e.alvo.tipo == Tipo::Documento));
        assert!(r.acima.iter().any(|e| e.alvo.tipo == Tipo::Origem));
        assert!(r.acima.iter().any(|e| e.alvo.tipo == Tipo::Conexao));
        assert!(r.abaixo.iter().any(|e| e.alvo.tipo == Tipo::Lote));
    }

    #[test]
    fn livro_caixa_nao_e_suportado_na_fatia_1() {
        let (c, _) = demo();
        assert!(
            c.relatorio(Pergunta::QuantoSaiu, Fonte::LivroCaixa, &Filtro::default())
                .is_err()
        );
        for p in crate::painel::PERGUNTAS {
            assert!(
                c.relatorio(p, Fonte::Inventario, &Filtro::default())
                    .is_ok()
            );
        }
    }

    #[test]
    fn movimento_entre_contas_nao_e_gasto() {
        let (c, _) = demo();
        let ciclo = Ref::nova(Tipo::Ciclo, CICLO_ATUAL);
        let t = c
            .tabela(&PedidoDeTabela::ItensDoTotal {
                de: ciclo,
                fonte: Fonte::Inventario,
                ciclo: None,
                recorte: Some("saidas".into()),
            })
            .expect("tabela");
        for l in &t.linhas {
            assert!(!l.celulas.iter().any(|x| matches!(x, Celula::Texto(t) if t == "Pagamento de fatura" || t == "Transferência entre contas")));
        }
    }
}
