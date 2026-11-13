//! O livro-caixa **fictício** do modo demonstração (spec 07): contas, cartões com faturas,
//! dívidas com parcelas, recorrentes e as ocorrências, centros com rateios e movimentações
//! em seis ciclos, mais uma estação com alterações (uma aceita, uma recusada) e um estado
//! de sincronização. Tudo inventado; nenhum dado real.
//!
//! Implementa as três costuras que a estação real (`sbf-estacao`) implementa sobre o
//! cache: [`Consulta`] para `Fonte::LivroCaixa`, [`Alteracoes`] e [`Sincronizacao`]. Os
//! números seguem as fórmulas da spec 07 ("Os números e as fórmulas"), com as decisões do
//! grill: ciclo com corte 20 (Q14), `adjustment` fora de tudo (Q15), "Para resolver"
//! (Q16) e gasto pela data da compra (Q18).
//!
//! Dinheiro em centavos `i64`; nenhuma conta passa por ponto flutuante.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use sbf_inventario::alteracao::{
    Alteracao, Alteracoes, CampoLadoALado, Credencial, ErroDeAlteracao, Estacao, EstadoDaAlteracao,
    EstadoDaRede, EstadoDaSincronizacao, OpcaoDeCategoria, PedidoDeAlteracao, Recusa,
    ResultadoDeAlteracao, Sincronizacao, Troca,
};
use sbf_inventario::consulta::{
    Area, Celula, Centavos, Coluna, Contagens, ErroDeConsulta, Filtro, Fonte, Indicador, Item,
    Linha, Painel, PainelDeArea, PedidoDeTabela, Pergunta, Rastro, ResultadoDeConsulta, Secao,
    Tabela, TotalDoCiclo,
};
use sbf_inventario::vistas::{fim_do_ciclo, inicio_do_ciclo, mes_anterior};
use sbf_inventario::{Consulta, Ref, Tipo};

use crate::formato;
use crate::janela::ciclo_de;

/// "Hoje" do livro fictício: os números da demonstração são calculados para este dia.
pub const HOJE: &str = "2026-11-13";
/// O ciclo de [`HOJE`].
pub const CICLO_ATUAL: &str = "2026-10";
/// Os ciclos com movimentações (os mesmos do inventário fictício).
pub const CICLOS: [&str; 6] = [
    "2026-05", "2026-06", "2026-07", "2026-08", "2026-09", "2026-10",
];

// ---------------------------------------------------------------------------
// O modelo (o que o servidor manda, já em centavos)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TipoMov {
    Receita,
    Estorno,
    Despesa,
    Tarifa,
    PagamentoDeFatura,
    PagamentoDeDivida,
    Ajuste,
}

impl TipoMov {
    fn rotulo(self) -> &'static str {
        match self {
            TipoMov::Receita => "Receita",
            TipoMov::Estorno => "Estorno",
            TipoMov::Despesa => "Despesa",
            TipoMov::Tarifa => "Tarifa",
            TipoMov::PagamentoDeFatura => "Pagamento de fatura",
            TipoMov::PagamentoDeDivida => "Pagamento de dívida",
            TipoMov::Ajuste => "Ajuste de saldo",
        }
    }

    /// Entra no "Entrou" do resultado do mês (`income` e `refund`).
    fn entra(self) -> bool {
        matches!(self, TipoMov::Receita | TipoMov::Estorno)
    }

    /// Pode ser rateada a centros (tudo menos `income`, `refund`, `statement_payment` e,
    /// pelo grill Q15, `adjustment`).
    fn rateavel_como_saida(self) -> bool {
        matches!(
            self,
            TipoMov::Despesa | TipoMov::Tarifa | TipoMov::PagamentoDeDivida
        )
    }
}

#[derive(Debug, Clone)]
struct Mov {
    id: String,
    data: String,
    descricao: String,
    /// Positivo entrou, negativo saiu.
    valor: Centavos,
    tipo: TipoMov,
    conta: Option<String>,
    cartao: Option<String>,
    fatura: Option<String>,
    categoria: Option<String>,
    fornecedor: Option<String>,
    confirmada: bool,
    parcela: Option<String>,
    tags: Vec<String>,
    /// `transactions.updated_at`: a versão de base das alterações (grill Q6/Q7).
    versao: String,
}

#[derive(Debug, Clone)]
struct Rateio {
    mov: String,
    centro: String,
    /// Sempre positivo (o sinal vem do tipo da movimentação).
    valor: Centavos,
}

#[derive(Debug, Clone)]
struct Conta {
    id: String,
    nome: String,
    tipo: &'static str,
    instituicao: String,
    saldo: Centavos,
    atualizado: String,
    origem: &'static str,
}

#[derive(Debug, Clone)]
struct Cartao {
    id: String,
    nome: String,
    instituicao: String,
    conta_de_pagamento: String,
    limite: Centavos,
    fecha_dia: u32,
    vence_dia: u32,
}

#[derive(Debug, Clone)]
struct Fatura {
    id: String,
    cartao: String,
    /// `AAAA-MM` do mês do fechamento.
    referencia: String,
    fecha: String,
    vence: String,
    total: Centavos,
    pago: Centavos,
}

#[derive(Debug, Clone)]
struct Nome {
    id: String,
    nome: String,
    /// Grupo (categoria), tipo (produto), apelidos (fornecedor)…
    extra: String,
}

#[derive(Debug, Clone)]
struct Fornecedor {
    id: String,
    nome: String,
    categoria: Option<String>,
    /// Os nomes do inventário que viram apelido (grill Q17).
    apelidos: Vec<String>,
}

#[derive(Debug, Clone)]
struct Divida {
    id: String,
    nome: String,
    credor: String,
    sistema: &'static str,
    /// Taxa ao mês em centésimos de ponto percentual (149 = 1,49% a.m.).
    taxa_bp: i64,
    principal: Centavos,
    contrato: Option<String>,
    cartao: Option<String>,
}

#[derive(Debug, Clone)]
struct Parcela {
    id: String,
    divida: String,
    numero: u32,
    de: u32,
    vence: String,
    valor: Centavos,
    juros: Centavos,
    amortizacao: Centavos,
    /// Saldo devedor depois desta parcela.
    saldo_depois: Centavos,
    paga: bool,
    mov: Option<String>,
}

#[derive(Debug, Clone)]
struct Recorrente {
    id: String,
    nome: String,
    fornecedor: Option<String>,
    categoria: Option<String>,
    frequencia: &'static str,
    /// Valor esperado; `None` quando não se sabe.
    valor: Option<Centavos>,
    variavel: bool,
    ativa: bool,
    conta: Option<String>,
    contrato: Option<String>,
    centro: Option<String>,
}

#[derive(Debug, Clone)]
struct Compromisso {
    id: String,
    recorrente: String,
    descricao: String,
    vence: String,
    valor: Option<Centavos>,
    conta: Option<String>,
    mov: Option<String>,
}

#[derive(Debug, Clone)]
struct Transferencia {
    id: String,
    data: String,
    de: String,
    para: String,
    valor: Centavos,
}

#[derive(Debug, Clone)]
struct Documento {
    id: String,
    nome: String,
    tipo: &'static str,
    data: String,
    estado: &'static str,
    ligado_a: Vec<Ref>,
}

#[derive(Debug, Clone)]
struct Contrato {
    id: String,
    nome: String,
    fornecedor: Option<String>,
    instituicao: Option<String>,
    inicio: String,
    fim: Option<String>,
}

/// O livro-caixa fictício inteiro, mais a estação.
#[derive(Debug, Clone)]
struct Livro {
    instituicoes: Vec<Nome>,
    contas: Vec<Conta>,
    produtos: Vec<Conta>,
    cartoes: Vec<Cartao>,
    faturas: Vec<Fatura>,
    categorias: Vec<Nome>,
    tags: Vec<Nome>,
    centros: Vec<Nome>,
    fornecedores: Vec<Fornecedor>,
    contratos: Vec<Contrato>,
    dividas: Vec<Divida>,
    parcelas: Vec<Parcela>,
    recorrentes: Vec<Recorrente>,
    compromissos: Vec<Compromisso>,
    movs: Vec<Mov>,
    rateios: Vec<Rateio>,
    transferencias: Vec<Transferencia>,
    documentos: Vec<Documento>,
    /// Ligações diretas entre itens, nos dois sentidos.
    arestas: BTreeMap<Ref, BTreeSet<Ref>>,
    // Estação
    estacoes: Vec<Estacao>,
    alteracoes: Vec<Alteracao>,
    trocas: Vec<Troca>,
    rede: EstadoDaRede,
    ultima_troca: Option<String>,
    copia_de: Option<String>,
    /// Movimentações que mudaram no servidor depois da cópia: a próxima alteração sobre
    /// elas é recusada, com a versão do servidor (valor, versão).
    mudadas_no_servidor: BTreeMap<String, (Centavos, String)>,
    proxima_alteracao: u32,
    proxima_troca_minuto: u32,
}

/// Um id com cara de UUID (o livro-caixa usa UUID; o inventário, nomes e hashes).
fn u(n: u32) -> String {
    format!("5bf00000-0000-4000-8000-{n:012}")
}

fn data(ano: i32, mes: i32, dia: u32) -> String {
    let (ano, mes) = if mes > 12 {
        (ano + 1, mes - 12)
    } else if mes < 1 {
        (ano - 1, mes + 12)
    } else {
        (ano, mes)
    };
    format!("{ano:04}-{mes:02}-{dia:02}")
}

fn ano_mes(ciclo: &str) -> (i32, i32) {
    (
        ciclo.get(0..4).and_then(|a| a.parse().ok()).unwrap_or(2026),
        ciclo.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1),
    )
}

/// A data do dia `dia` dentro do ciclo: dia 20 em diante cai no mês do ciclo; antes, no
/// mês seguinte.
fn dia_do_ciclo(ciclo: &str, dia: u32) -> String {
    let (a, m) = ano_mes(ciclo);
    if dia >= 20 {
        data(a, m, dia)
    } else {
        data(a, m + 1, dia)
    }
}

/// `2026-11-08` → `08/11`.
fn dia_mes(d: &str) -> String {
    match (d.get(8..10), d.get(5..7)) {
        (Some(dd), Some(mm)) => format!("{dd}/{mm}"),
        _ => d.to_owned(),
    }
}

/// Juros de um mês, arredondado ao centavo, só com inteiros.
fn juros(saldo: Centavos, taxa_bp: i64) -> Centavos {
    (saldo * taxa_bp + 5_000) / 10_000
}

/// O cronograma de uma dívida: (valor, juros, amortização, saldo depois) por parcela.
/// Price acha a prestação fixa por busca binária sobre centavos inteiros.
fn cronograma(
    sistema: &str,
    principal: Centavos,
    taxa_bp: i64,
    n: u32,
) -> Vec<(Centavos, Centavos, Centavos, Centavos)> {
    let simular = |pmt: Centavos| -> Centavos {
        let mut saldo = principal;
        for _ in 0..n {
            saldo = saldo + juros(saldo, taxa_bp) - pmt;
        }
        saldo
    };
    let mut v = Vec::new();
    let mut saldo = principal;
    match sistema {
        "SAC" => {
            let amort = principal / i64::from(n.max(1));
            for k in 0..n {
                let j = juros(saldo, taxa_bp);
                let a = if k + 1 == n { saldo } else { amort };
                saldo -= a;
                v.push((a + j, j, a, saldo));
            }
        }
        _ => {
            let (mut baixo, mut alto) = (0i64, principal * 2);
            while baixo < alto {
                let meio = (baixo + alto) / 2;
                if simular(meio) > 0 {
                    baixo = meio + 1;
                } else {
                    alto = meio;
                }
            }
            let pmt = baixo;
            for k in 0..n {
                let j = juros(saldo, taxa_bp);
                let a = if k + 1 == n { saldo } else { pmt - j };
                saldo -= a;
                v.push((a + j, j, a, saldo));
            }
        }
    }
    v
}

/// Dívida, parcelas, primeira (`AAAA-MM`), dia, conta e categoria.
type PlanoDeDivida = (u32, u32, &'static str, u32, Option<u32>, u32);

const MERCADO: [[Centavos; 3]; 6] = [
    [31_245, 19_870, 42_110],
    [28_990, 21_540, 38_760],
    [33_410, 17_620, 40_380],
    [30_125, 22_090, 36_640],
    [34_870, 18_930, 41_250],
    [32_045, 19_870, 39_915],
];
const ENERGIA: [Centavos; 6] = [19_842, 21_310, 23_475, 24_190, 22_860, 0];
const AGUA: [Centavos; 6] = [8_920, 9_340, 10_110, 9_870, 9_560, 0];

impl Livro {
    fn ficticio() -> Self {
        let mut l = Livro {
            instituicoes: Vec::new(),
            contas: Vec::new(),
            produtos: Vec::new(),
            cartoes: Vec::new(),
            faturas: Vec::new(),
            categorias: Vec::new(),
            tags: Vec::new(),
            centros: Vec::new(),
            fornecedores: Vec::new(),
            contratos: Vec::new(),
            dividas: Vec::new(),
            parcelas: Vec::new(),
            recorrentes: Vec::new(),
            compromissos: Vec::new(),
            movs: Vec::new(),
            rateios: Vec::new(),
            transferencias: Vec::new(),
            documentos: Vec::new(),
            arestas: BTreeMap::new(),
            estacoes: Vec::new(),
            alteracoes: Vec::new(),
            trocas: Vec::new(),
            rede: EstadoDaRede::Sincronizado,
            ultima_troca: Some("2026-11-13T08:12:00-03:00".to_owned()),
            copia_de: Some("2026-11-13T08:12:00-03:00".to_owned()),
            mudadas_no_servidor: BTreeMap::new(),
            proxima_alteracao: 3,
            proxima_troca_minuto: 0,
        };
        l.cadastros();
        l.movimentacoes();
        // As parcelas da compra no cartão entram nas faturas: faturas depois das dívidas.
        l.dividas_e_parcelas();
        l.documentos_ficticios();
        l.estacao_ficticia();
        l.indexar();
        l
    }

    fn cadastros(&mut self) {
        let nome = |n: u32, nome: &str, extra: &str| Nome {
            id: u(n),
            nome: nome.to_owned(),
            extra: extra.to_owned(),
        };
        self.instituicoes = vec![
            nome(101, "Banco Alfa", ""),
            nome(102, "Banco Beta", ""),
            nome(103, "Financeira Gama", ""),
        ];
        let conta = |n: u32, nome: &str, tipo, inst: u32, saldo, atualizado: &str, origem| Conta {
            id: u(n),
            nome: nome.to_owned(),
            tipo,
            instituicao: u(inst),
            saldo,
            atualizado: atualizado.to_owned(),
            origem,
        };
        self.contas = vec![
            conta(
                111,
                "Conta Alfa",
                "Conta corrente",
                101,
                481_237,
                "2026-11-13T08:12:00-03:00",
                "Open Finance",
            ),
            conta(
                112,
                "Poupança Alfa",
                "Poupança",
                101,
                1_230_000,
                "2026-11-13T08:12:00-03:00",
                "Open Finance",
            ),
            conta(
                113,
                "Conta Beta",
                "Conta corrente",
                102,
                120_590,
                "2026-11-12T19:40:00-03:00",
                "digitado",
            ),
        ];
        self.produtos = vec![
            conta(
                121,
                "CDB Alfa 2028",
                "Investimento",
                101,
                2_000_000,
                "2026-11-01T09:00:00-03:00",
                "digitado",
            ),
            conta(
                122,
                "Financiamento Gama",
                "Financiamento",
                103,
                0,
                "2026-11-01T09:00:00-03:00",
                "digitado",
            ),
        ];
        self.cartoes = vec![
            Cartao {
                id: u(131),
                nome: "Cartão Alfa Visa".to_owned(),
                instituicao: u(101),
                conta_de_pagamento: u(111),
                limite: 800_000,
                fecha_dia: 3,
                vence_dia: 10,
            },
            Cartao {
                id: u(132),
                nome: "Cartão Beta Master".to_owned(),
                instituicao: u(102),
                conta_de_pagamento: u(113),
                limite: 500_000,
                fecha_dia: 25,
                vence_dia: 5,
            },
        ];
        self.categorias = [
            (201, "Mercado", "Despesa"),
            (202, "Casa", "Despesa"),
            (203, "Energia", "Despesa"),
            (204, "Água", "Despesa"),
            (205, "Internet", "Despesa"),
            (206, "Assinaturas", "Despesa"),
            (207, "Educação", "Despesa"),
            (208, "Restaurantes", "Despesa"),
            (209, "Lazer", "Despesa"),
            (210, "Tarifas", "Despesa"),
            (211, "Financiamento", "Despesa"),
            (212, "Seguros", "Despesa"),
            (213, "Salário", "Receita"),
            (214, "Serviços prestados", "Receita"),
            (215, "Compras parceladas", "Despesa"),
        ]
        .into_iter()
        .map(|(n, a, b)| nome(n, a, b))
        .collect();
        self.tags = vec![nome(221, "reembolsável", ""), nome(222, "viagem", "")];
        self.centros = vec![
            nome(231, "Família", ""),
            nome(232, "Projeto Ômega", ""),
            nome(233, "Reforma", ""),
        ];
        let forn = |n: u32, nome: &str, cat: Option<u32>, apelidos: &[&str]| Fornecedor {
            id: u(n),
            nome: nome.to_owned(),
            categoria: cat.map(u),
            apelidos: apelidos.iter().map(|a| (*a).to_owned()).collect(),
        };
        self.fornecedores = vec![
            forn(
                301,
                "Mercado Delta",
                Some(201),
                &["Mercado Delta", "MERCADO DELTA LTDA"],
            ),
            forn(302, "Energia Exemplo", Some(203), &["Energia Exemplo"]),
            forn(303, "Água Sigma", Some(204), &["Água Sigma"]),
            forn(304, "Condomínio Kappa", Some(202), &["Condomínio Kappa"]),
            forn(305, "Internet Teta", Some(205), &["Internet Teta"]),
            forn(306, "Streaming Zeta", Some(206), &["Streaming Zeta"]),
            forn(307, "Escola Gama", Some(207), &["Escola Gama"]),
            forn(308, "Restaurante Pi", Some(208), &[]),
            forn(
                309,
                "Empresa Exemplo Ltda",
                Some(213),
                &["Empresa Exemplo Ltda"],
            ),
            forn(310, "Loja Ômicron", Some(215), &[]),
            forn(311, "Seguradora Ômega", Some(212), &[]),
            forn(312, "Jornal Rho", Some(206), &[]),
            forn(313, "Academia Iota", Some(209), &[]),
        ];
        self.contratos = vec![
            Contrato {
                id: u(321),
                nome: "Contrato do Financiamento Gama".to_owned(),
                fornecedor: None,
                instituicao: Some(u(103)),
                inicio: "2024-11-15".to_owned(),
                fim: Some("2028-10-15".to_owned()),
            },
            Contrato {
                id: u(322),
                nome: "Matrícula Escola Gama 2026".to_owned(),
                fornecedor: Some(u(307)),
                instituicao: None,
                inicio: "2026-02-01".to_owned(),
                fim: Some("2026-12-31".to_owned()),
            },
        ];
        let rec = |n: u32,
                   nome: &str,
                   forn: Option<u32>,
                   cat: Option<u32>,
                   freq,
                   valor: Option<Centavos>,
                   variavel: bool,
                   ativa: bool,
                   conta: Option<u32>,
                   contrato: Option<u32>,
                   centro: Option<u32>| Recorrente {
            id: u(n),
            nome: nome.to_owned(),
            fornecedor: forn.map(u),
            categoria: cat.map(u),
            frequencia: freq,
            valor,
            variavel,
            ativa,
            conta: conta.map(u),
            contrato: contrato.map(u),
            centro: centro.map(u),
        };
        self.recorrentes = vec![
            rec(
                401,
                "Conta de energia",
                Some(302),
                Some(203),
                "mensal",
                Some(23_000),
                true,
                true,
                Some(111),
                None,
                Some(231),
            ),
            rec(
                402,
                "Conta de água",
                Some(303),
                Some(204),
                "mensal",
                None,
                true,
                true,
                Some(111),
                None,
                Some(231),
            ),
            rec(
                403,
                "Condomínio",
                Some(304),
                Some(202),
                "mensal",
                Some(95_000),
                false,
                true,
                Some(111),
                None,
                Some(231),
            ),
            rec(
                404,
                "Internet",
                Some(305),
                Some(205),
                "mensal",
                Some(11_990),
                false,
                true,
                None,
                None,
                None,
            ),
            rec(
                405,
                "Streaming Zeta",
                Some(306),
                Some(206),
                "mensal",
                Some(3_990),
                false,
                true,
                None,
                None,
                None,
            ),
            rec(
                406,
                "Mensalidade da escola",
                Some(307),
                Some(207),
                "mensal",
                Some(120_000),
                false,
                true,
                Some(113),
                Some(322),
                Some(232),
            ),
            rec(
                407,
                "Seguro anual",
                Some(311),
                Some(212),
                "anual",
                Some(186_000),
                false,
                true,
                Some(111),
                None,
                None,
            ),
            rec(
                408,
                "Jornal Rho",
                Some(312),
                Some(206),
                "semanal",
                Some(1_500),
                false,
                true,
                Some(113),
                None,
                Some(231),
            ),
            rec(
                409,
                "Academia Iota",
                Some(313),
                Some(209),
                "mensal",
                Some(9_900),
                false,
                false,
                Some(113),
                None,
                None,
            ),
        ];
        self.dividas = vec![
            Divida {
                id: u(501),
                nome: "Financiamento Gama".to_owned(),
                credor: u(103),
                sistema: "Price",
                taxa_bp: 149,
                principal: 4_000_000,
                contrato: Some(u(321)),
                cartao: None,
            },
            Divida {
                id: u(502),
                nome: "Empréstimo Beta".to_owned(),
                credor: u(102),
                sistema: "SAC",
                taxa_bp: 199,
                principal: 1_200_000,
                contrato: None,
                cartao: None,
            },
            Divida {
                id: u(503),
                nome: "TV em 10x na Loja Ômicron".to_owned(),
                credor: u(101),
                sistema: "Sem juros",
                taxa_bp: 0,
                principal: 280_000,
                contrato: None,
                cartao: Some(u(131)),
            },
        ];
    }

    fn mov_novo(&mut self, data: &str, descricao: &str, valor: Centavos, tipo: TipoMov) -> usize {
        let n = 10_000 + self.movs.len() as u32;
        self.movs.push(Mov {
            id: u(n),
            data: data.to_owned(),
            descricao: descricao.to_owned(),
            valor,
            tipo,
            conta: None,
            cartao: None,
            fatura: None,
            categoria: None,
            fornecedor: None,
            confirmada: data <= HOJE,
            parcela: None,
            tags: Vec::new(),
            versao: format!("{data}T10:00:00Z"),
        });
        self.movs.len() - 1
    }

    #[allow(clippy::too_many_arguments)]
    fn lancar(
        &mut self,
        data: &str,
        descricao: &str,
        valor: Centavos,
        tipo: TipoMov,
        conta: Option<u32>,
        cartao: Option<u32>,
        categoria: Option<u32>,
        fornecedor: Option<u32>,
        rateio: &[(u32, Centavos)],
    ) -> String {
        let i = self.mov_novo(data, descricao, valor, tipo);
        let m = &mut self.movs[i];
        m.conta = conta.map(u);
        m.cartao = cartao.map(u);
        m.categoria = categoria.map(u);
        m.fornecedor = fornecedor.map(u);
        let id = m.id.clone();
        for (centro, v) in rateio {
            self.rateios.push(Rateio {
                mov: id.clone(),
                centro: u(*centro),
                valor: *v,
            });
        }
        id
    }

    /// Uma ocorrência de recorrente, paga (com a movimentação) ou pendente.
    fn ocorrencia(&mut self, rec: u32, vence: &str, valor: Option<Centavos>, mov: Option<String>) {
        let n = 600 + self.compromissos.len() as u32;
        let r = self.recorrentes.iter().find(|r| r.id == u(rec)).cloned();
        let Some(r) = r else { return };
        self.compromissos.push(Compromisso {
            id: u(n),
            recorrente: r.id.clone(),
            descricao: format!("{} · {}", r.nome, dia_mes(vence)),
            vence: vence.to_owned(),
            valor,
            conta: r.conta.clone(),
            mov,
        });
    }

    fn movimentacoes(&mut self) {
        for (i, ciclo) in CICLOS.iter().enumerate() {
            let atual = *ciclo == CICLO_ATUAL;
            // Salário no dia 5.
            let d = dia_do_ciclo(ciclo, 5);
            self.lancar(
                &d,
                "Salário",
                850_000,
                TipoMov::Receita,
                Some(111),
                None,
                Some(213),
                Some(309),
                &[(231, 850_000)],
            );
            // Energia (dia 18), variável. No ciclo atual, ainda vai vencer.
            let d = dia_do_ciclo(ciclo, 18);
            if atual {
                self.ocorrencia(401, &d, Some(23_000), None);
            } else {
                let v = ENERGIA[i];
                let m = self.lancar(
                    &d,
                    "Energia Exemplo",
                    -v,
                    TipoMov::Despesa,
                    Some(111),
                    None,
                    Some(203),
                    Some(302),
                    &[(231, v)],
                );
                self.ocorrencia(401, &d, Some(v), Some(m));
            }
            // Água (dia 16), sem valor esperado.
            let d = dia_do_ciclo(ciclo, 16);
            if atual {
                self.ocorrencia(402, &d, None, None);
            } else {
                let v = AGUA[i];
                let m = self.lancar(
                    &d,
                    "Água Sigma",
                    -v,
                    TipoMov::Despesa,
                    Some(111),
                    None,
                    Some(204),
                    Some(303),
                    &[(231, v)],
                );
                self.ocorrencia(402, &d, Some(v), Some(m));
            }
            // Condomínio (dia 1).
            let d = dia_do_ciclo(ciclo, 1);
            let m = self.lancar(
                &d,
                "Condomínio Kappa",
                -95_000,
                TipoMov::Despesa,
                Some(111),
                None,
                Some(202),
                Some(304),
                &[(231, 95_000)],
            );
            self.ocorrencia(403, &d, Some(95_000), Some(m));
            // Escola (dia 10), metade para cada centro.
            let d = dia_do_ciclo(ciclo, 10);
            let m = self.lancar(
                &d,
                "Escola Gama",
                -120_000,
                TipoMov::Despesa,
                Some(113),
                None,
                Some(207),
                Some(307),
                &[(232, 60_000), (231, 60_000)],
            );
            self.ocorrencia(406, &d, Some(120_000), Some(m));
            // Internet no cartão Alfa (dia 8).
            let d = dia_do_ciclo(ciclo, 8);
            let m = self.lancar(
                &d,
                "Internet Teta",
                -11_990,
                TipoMov::Despesa,
                None,
                Some(131),
                Some(205),
                Some(305),
                &[(232, 5_995), (231, 5_995)],
            );
            self.ocorrencia(404, &d, Some(11_990), Some(m));
            // Streaming no cartão Beta (dia 4), não alocado.
            let d = dia_do_ciclo(ciclo, 4);
            let m = self.lancar(
                &d,
                "Streaming Zeta",
                -3_990,
                TipoMov::Despesa,
                None,
                Some(132),
                Some(206),
                Some(306),
                &[],
            );
            self.ocorrencia(405, &d, Some(3_990), Some(m));
            // Mercado: três compras no cartão Alfa; uma inteira, uma em parte, uma sem rateio.
            let [a, b, c] = MERCADO[i];
            let d = dia_do_ciclo(ciclo, 23);
            self.lancar(
                &d,
                "Mercado Delta",
                -a,
                TipoMov::Despesa,
                None,
                Some(131),
                Some(201),
                Some(301),
                &[(231, a)],
            );
            let d = dia_do_ciclo(ciclo, 8);
            self.lancar(
                &d,
                "Mercado Delta",
                -b,
                TipoMov::Despesa,
                None,
                Some(131),
                Some(201),
                Some(301),
                &[(231, 10_000)],
            );
            let d = dia_do_ciclo(ciclo, 12);
            self.lancar(
                &d,
                "Mercado Delta",
                -c,
                TipoMov::Despesa,
                None,
                Some(131),
                Some(201),
                Some(301),
                &[],
            );
            // Restaurante no cartão Beta (dia 7), reembolsável pelo Projeto Ômega.
            let d = dia_do_ciclo(ciclo, 7);
            let v = 8_650 + 310 * i as i64;
            let m = self.lancar(
                &d,
                "Restaurante Pi",
                -v,
                TipoMov::Despesa,
                None,
                Some(132),
                Some(208),
                Some(308),
                &[(232, v)],
            );
            if let Some(x) = self.movs.iter_mut().find(|x| x.id == m) {
                x.tags.push(u(221));
            }
            // Tarifa do pacote (dia 25), não alocada.
            let d = dia_do_ciclo(ciclo, 25);
            self.lancar(
                &d,
                "Tarifa do pacote",
                -1_490,
                TipoMov::Tarifa,
                Some(113),
                None,
                Some(210),
                None,
                &[],
            );
            // Transferência para a poupança (dia 6): fora das movimentações.
            let d = dia_do_ciclo(ciclo, 6);
            self.transferencias.push(Transferencia {
                id: u(700 + i as u32),
                data: d,
                de: u(111),
                para: u(112),
                valor: 50_000,
            });
        }
        // Avulsos.
        self.lancar(
            "2026-09-28",
            "Projeto Ômega · nota 12",
            320_000,
            TipoMov::Receita,
            Some(113),
            None,
            Some(214),
            None,
            &[(232, 320_000)],
        );
        self.lancar(
            "2026-09-30",
            "Ajuste de saldo",
            1_234,
            TipoMov::Ajuste,
            Some(113),
            None,
            None,
            None,
            &[],
        );
        self.lancar(
            "2026-11-09",
            "Estorno Mercado Delta",
            3_150,
            TipoMov::Estorno,
            None,
            Some(131),
            Some(201),
            Some(301),
            &[(231, 3_150)],
        );
        self.lancar(
            "2026-11-11",
            "PIX enviado",
            -5_700,
            TipoMov::Despesa,
            Some(113),
            None,
            None,
            None,
            &[],
        );
        let seguro = self.lancar(
            "2026-11-17",
            "Seguro do carro · 2ª parcela",
            -31_000,
            TipoMov::Despesa,
            Some(111),
            None,
            Some(212),
            Some(311),
            &[],
        );
        if let Some(x) = self.movs.iter_mut().find(|x| x.id == seguro) {
            x.confirmada = false;
        }
        let viagem = self.lancar(
            "2026-10-26",
            "Hotel em Ouro Preto",
            -64_000,
            TipoMov::Despesa,
            None,
            Some(132),
            Some(209),
            None,
            &[(231, 64_000)],
        );
        if let Some(x) = self.movs.iter_mut().find(|x| x.id == viagem) {
            x.tags.push(u(222));
        }
        // Jornal Rho, semanal, começou no ciclo atual.
        for (d, pago) in [
            ("2026-10-23", true),
            ("2026-10-30", true),
            ("2026-11-06", false),
            ("2026-11-13", false),
        ] {
            if pago {
                let m = self.lancar(
                    d,
                    "Jornal Rho",
                    -1_500,
                    TipoMov::Despesa,
                    Some(113),
                    None,
                    Some(206),
                    Some(312),
                    &[(231, 1_500)],
                );
                self.ocorrencia(408, d, Some(1_500), Some(m));
            } else {
                self.ocorrencia(408, d, Some(1_500), None);
            }
        }
        // A alteração aceita (Restaurante → Lazer) já está no servidor.
        if let Some(x) = self
            .movs
            .iter_mut()
            .find(|x| x.descricao == "Restaurante Pi" && x.data == "2026-11-07")
        {
            x.categoria = Some(u(209));
            x.versao = "2026-11-12T13:05:00Z".to_owned();
        }
    }

    fn fatura_de(&self, cartao: &Cartao, data_da_compra: &str) -> (String, String, String) {
        let (a, m) = ano_mes(data_da_compra);
        let dia: u32 = data_da_compra
            .get(8..10)
            .and_then(|d| d.parse().ok())
            .unwrap_or(1);
        let m = if dia <= cartao.fecha_dia { m } else { m + 1 };
        let fecha = data(a, m, cartao.fecha_dia);
        let vence = if cartao.vence_dia > cartao.fecha_dia {
            data(a, m, cartao.vence_dia)
        } else {
            data(a, m + 1, cartao.vence_dia)
        };
        (fecha.get(0..7).unwrap_or("").to_owned(), fecha, vence)
    }

    fn faturas_e_pagamentos(&mut self) {
        let cartoes = self.cartoes.clone();
        let mut por_fatura: BTreeMap<(String, String), (String, String, Centavos)> =
            BTreeMap::new();
        let mut ligacoes = Vec::new();
        for (i, m) in self.movs.iter().enumerate() {
            let Some(c) = m
                .cartao
                .as_ref()
                .and_then(|c| cartoes.iter().find(|x| &x.id == c))
            else {
                continue;
            };
            let (referencia, fecha, vence) = self.fatura_de(c, &m.data);
            let e = por_fatura
                .entry((c.id.clone(), referencia.clone()))
                .or_insert((fecha, vence, 0));
            e.2 -= m.valor;
            ligacoes.push((i, c.id.clone(), referencia));
        }
        let mut n = 800;
        for ((cartao, referencia), (fecha, vence, total)) in por_fatura {
            n += 1;
            let id = u(n);
            let paga = vence.as_str() <= HOJE;
            self.faturas.push(Fatura {
                id: id.clone(),
                cartao: cartao.clone(),
                referencia: referencia.clone(),
                fecha,
                vence: vence.clone(),
                total,
                pago: if paga { total } else { 0 },
            });
            for (i, c, r) in &ligacoes {
                if *c == cartao && *r == referencia {
                    self.movs[*i].fatura = Some(id.clone());
                }
            }
            if paga && vence.as_str() >= "2026-05-20" {
                let conta = cartoes
                    .iter()
                    .find(|c| c.id == cartao)
                    .map(|c| c.conta_de_pagamento.clone());
                let nome = cartoes
                    .iter()
                    .find(|c| c.id == cartao)
                    .map(|c| c.nome.clone())
                    .unwrap_or_default();
                let i = self.mov_novo(
                    &vence,
                    &format!("Pagamento da fatura {nome} {referencia}"),
                    -total,
                    TipoMov::PagamentoDeFatura,
                );
                self.movs[i].conta = conta;
                self.movs[i].fatura = Some(id.clone());
            }
        }
    }

    fn dividas_e_parcelas(&mut self) {
        let planos: [PlanoDeDivida; 3] = [
            // dívida, parcelas, primeira (AAAA-MM), dia, conta, categoria
            (501, 48, "2024-12", 15, Some(111), 211),
            (502, 24, "2025-09", 5, Some(113), 211),
            (503, 10, "2026-05", 8, None, 215),
        ];
        let mut n = 900;
        for (div, total, primeira, dia, conta, cat) in planos {
            let Some(d) = self.dividas.iter().find(|d| d.id == u(div)).cloned() else {
                continue;
            };
            let crono = cronograma(d.sistema, d.principal, d.taxa_bp, total);
            let (a, m) = ano_mes(primeira);
            for (k, (valor, j, amort, saldo)) in crono.into_iter().enumerate() {
                n += 1;
                let vence = data(a, m + k as i32, dia);
                let paga = vence.as_str() <= HOJE;
                let id = u(n);
                let mut mov = None;
                if paga && vence.as_str() >= "2026-05-20" {
                    let descricao = format!("{} · parcela {}/{}", d.nome, k + 1, total);
                    let centro = if div == 503 { 231 } else { 233 };
                    let mid = if d.cartao.is_some() {
                        self.lancar(
                            &vence,
                            &descricao,
                            -valor,
                            TipoMov::Despesa,
                            None,
                            Some(131),
                            Some(cat),
                            Some(310),
                            &[(centro, valor)],
                        )
                    } else {
                        self.lancar(
                            &vence,
                            &descricao,
                            -valor,
                            TipoMov::PagamentoDeDivida,
                            conta,
                            None,
                            Some(cat),
                            None,
                            &[(centro, valor)],
                        )
                    };
                    if let Some(x) = self.movs.iter_mut().find(|x| x.id == mid) {
                        x.parcela = Some(id.clone());
                    }
                    mov = Some(mid);
                }
                self.parcelas.push(Parcela {
                    id,
                    divida: d.id.clone(),
                    numero: k as u32 + 1,
                    de: total,
                    vence,
                    valor,
                    juros: j,
                    amortizacao: amort,
                    saldo_depois: saldo,
                    paga,
                    mov,
                });
            }
        }
        self.faturas_e_pagamentos();
    }

    fn documentos_ficticios(&mut self) {
        let mov = |l: &Livro, desc: &str, data: &str| {
            l.movs
                .iter()
                .find(|m| m.descricao == desc && m.data == data)
                .map(|m| Ref::nova(Tipo::Movimentacao, m.id.clone()))
        };
        let fatura_alfa = self
            .faturas
            .iter()
            .find(|f| f.cartao == u(131) && f.referencia == "2026-11")
            .map(|f| Ref::nova(Tipo::Fatura, f.id.clone()));
        let docs = [
            (
                951,
                "boleto-condominio-kappa-2026-11.pdf",
                "Boleto",
                "2026-10-25",
                "Pronto",
                vec![mov(self, "Condomínio Kappa", "2026-11-01")],
            ),
            (
                952,
                "fatura-cartao-alfa-2026-11.pdf",
                "Fatura de cartão",
                "2026-11-03",
                "Pronto",
                vec![fatura_alfa],
            ),
            (
                953,
                "contrato-financiamento-gama.pdf",
                "Contrato",
                "2024-11-10",
                "Pronto",
                vec![
                    Some(Ref::nova(Tipo::Contrato, u(321))),
                    Some(Ref::nova(Tipo::Divida, u(501))),
                ],
            ),
            (
                954,
                "comprovante-escola-gama-2026-11.pdf",
                "Comprovante",
                "2026-11-10",
                "Precisa revisar",
                vec![mov(self, "Escola Gama", "2026-11-10")],
            ),
        ];
        for (n, nome, tipo, data, estado, ligados) in docs {
            self.documentos.push(Documento {
                id: u(n),
                nome: nome.to_owned(),
                tipo,
                data: data.to_owned(),
                estado,
                ligado_a: ligados.into_iter().flatten().collect(),
            });
        }
    }

    fn estacao_ficticia(&mut self) {
        let est =
            |n: u32, nome: &str, sistema: &str, ultima: &str, ativa: bool, esta: bool| Estacao {
                alvo: Ref::nova(Tipo::Estacao, u(n)),
                nome: nome.to_owned(),
                sistema: sistema.to_owned(),
                ultima_sincronizacao: Some(ultima.to_owned()),
                credencial: if ativa {
                    Credencial::Ativa
                } else {
                    Credencial::Revogada
                },
                esta,
            };
        self.estacoes = vec![
            est(
                961,
                "Notebook",
                "Linux",
                "2026-11-13T08:12:00-03:00",
                true,
                true,
            ),
            est(
                962,
                "Desktop do escritório",
                "Windows",
                "2026-11-12T18:40:00-03:00",
                true,
                false,
            ),
            est(
                963,
                "Notebook antigo",
                "macOS",
                "2026-09-02T21:15:00-03:00",
                false,
                false,
            ),
        ];
        let esta = Ref::nova(Tipo::Estacao, u(961));
        let restaurante = self
            .movs
            .iter()
            .find(|m| m.descricao == "Restaurante Pi" && m.data == "2026-11-07")
            .map(|m| m.id.clone())
            .unwrap_or_default();
        let mercado = self
            .movs
            .iter()
            .find(|m| m.descricao == "Mercado Delta" && m.data == "2026-11-08")
            .cloned();
        self.alteracoes.push(Alteracao {
            id: "1".to_owned(),
            pedido: PedidoDeAlteracao::MudarCategoria {
                movimentacao: Ref::nova(Tipo::Movimentacao, restaurante.clone()),
                categoria: Some(Ref::nova(Tipo::Categoria, u(209))),
            },
            rotulo_do_alvo: "Restaurante Pi · 07/11".to_owned(),
            area: "Movimentações".to_owned(),
            antes: "Restaurantes".to_owned(),
            depois: "Lazer".to_owned(),
            feita_em: "2026-11-12T10:02:00-03:00".to_owned(),
            estacao: esta.clone(),
            estado: EstadoDaAlteracao::Aceita,
            versoes_de_base: vec![(
                Ref::nova(Tipo::Movimentacao, restaurante),
                "2026-11-07T10:00:00Z".to_owned(),
            )],
            recusa: None,
            refeita_em: None,
        });
        if let Some(m) = mercado {
            let alvo = Ref::nova(Tipo::Movimentacao, m.id.clone());
            self.alteracoes.push(Alteracao {
                id: "2".to_owned(),
                pedido: PedidoDeAlteracao::MudarCategoria {
                    movimentacao: alvo.clone(),
                    categoria: Some(Ref::nova(Tipo::Categoria, u(202))),
                },
                rotulo_do_alvo: "Mercado Delta · 08/11".to_owned(),
                area: "Movimentações".to_owned(),
                antes: "Mercado".to_owned(),
                depois: "Casa".to_owned(),
                feita_em: "2026-11-12T21:40:00-03:00".to_owned(),
                estacao: esta,
                estado: EstadoDaAlteracao::Recusada,
                versoes_de_base: vec![(alvo, m.versao.clone())],
                recusa: Some(Recusa {
                    motivo: "A movimentação mudou no servidor depois que você alterou: o valor foi corrigido pela fatura do cartão.".to_owned(),
                    quando: "2026-11-13T08:12:00-03:00".to_owned(),
                    campos: vec![
                        CampoLadoALado::novo("Categoria", "Casa", "Mercado"),
                        CampoLadoALado::novo(
                            "Valor",
                            formato::dinheiro(m.valor),
                            formato::dinheiro(-20_130),
                        ),
                        CampoLadoALado::novo("Data", "08/11/2026", "08/11/2026"),
                        CampoLadoALado::novo("Fornecedor", "Mercado Delta", "Mercado Delta"),
                        CampoLadoALado::novo("Conta ou cartão", "Cartão Alfa Visa", "Cartão Alfa Visa"),
                    ],
                }),
                refeita_em: None,
            });
            // A cópia local já tem a versão nova do servidor (veio na última troca).
            if let Some(x) = self.movs.iter_mut().find(|x| x.id == m.id) {
                x.valor = -20_130;
                x.versao = "2026-11-13T07:58:00Z".to_owned();
            }
        }
        self.trocas = vec![
            Troca {
                inicio: "2026-11-13T08:11:52-03:00".to_owned(),
                fim: Some("2026-11-13T08:12:00-03:00".to_owned()),
                colecoes: vec!["movimentacoes".to_owned(), "faturas".to_owned()],
                linhas: 214,
                enviadas: 1,
                aceitas: 0,
                recusadas: 1,
                erro: None,
            },
            Troca {
                inicio: "2026-11-12T13:05:10-03:00".to_owned(),
                fim: Some("2026-11-12T13:05:14-03:00".to_owned()),
                colecoes: vec!["movimentacoes".to_owned()],
                linhas: 212,
                enviadas: 1,
                aceitas: 1,
                recusadas: 0,
                erro: None,
            },
            Troca {
                inicio: "2026-11-12T07:30:00-03:00".to_owned(),
                fim: None,
                colecoes: Vec::new(),
                linhas: 0,
                enviadas: 0,
                aceitas: 0,
                recusadas: 0,
                erro: Some("Sem rede: o servidor não respondeu em 10 s.".to_owned()),
            },
        ];
    }

    /// Monta as ligações diretas entre os itens, nos dois sentidos.
    fn indexar(&mut self) {
        let mut a: BTreeMap<Ref, BTreeSet<Ref>> = BTreeMap::new();
        let mut ligar = |x: Ref, y: Ref| {
            a.entry(x.clone()).or_default().insert(y.clone());
            a.entry(y).or_default().insert(x);
        };
        let r = Ref::nova;
        for m in &self.movs {
            let me = r(Tipo::Movimentacao, m.id.clone());
            ligar(me.clone(), r(Tipo::Ciclo, ciclo_de(&m.data)));
            for (t, v) in [
                (Tipo::Conta, &m.conta),
                (Tipo::Cartao, &m.cartao),
                (Tipo::Fatura, &m.fatura),
                (Tipo::Categoria, &m.categoria),
                (Tipo::Fornecedor, &m.fornecedor),
                (Tipo::Parcela, &m.parcela),
            ] {
                if let Some(v) = v {
                    ligar(me.clone(), r(t, v.clone()));
                }
            }
            for t in &m.tags {
                ligar(me.clone(), r(Tipo::Tag, t.clone()));
            }
            if let (Some(f), Some(c)) = (&m.fornecedor, &m.categoria) {
                ligar(
                    r(Tipo::Fornecedor, f.clone()),
                    r(Tipo::Categoria, c.clone()),
                );
            }
        }
        for x in &self.rateios {
            ligar(
                r(Tipo::Movimentacao, x.mov.clone()),
                r(Tipo::Centro, x.centro.clone()),
            );
        }
        for c in &self.contas {
            ligar(
                r(Tipo::Conta, c.id.clone()),
                r(Tipo::Instituicao, c.instituicao.clone()),
            );
        }
        for c in &self.produtos {
            ligar(
                r(Tipo::Produto, c.id.clone()),
                r(Tipo::Instituicao, c.instituicao.clone()),
            );
        }
        for c in &self.cartoes {
            let me = r(Tipo::Cartao, c.id.clone());
            ligar(me.clone(), r(Tipo::Instituicao, c.instituicao.clone()));
            ligar(me, r(Tipo::Conta, c.conta_de_pagamento.clone()));
        }
        for f in &self.faturas {
            ligar(
                r(Tipo::Fatura, f.id.clone()),
                r(Tipo::Cartao, f.cartao.clone()),
            );
        }
        for d in &self.dividas {
            let me = r(Tipo::Divida, d.id.clone());
            ligar(me.clone(), r(Tipo::Instituicao, d.credor.clone()));
            if let Some(c) = &d.contrato {
                ligar(me.clone(), r(Tipo::Contrato, c.clone()));
            }
            if let Some(c) = &d.cartao {
                ligar(me.clone(), r(Tipo::Cartao, c.clone()));
            }
            if d.id == u(501) {
                ligar(me, r(Tipo::Produto, u(122)));
            }
        }
        for p in &self.parcelas {
            let me = r(Tipo::Parcela, p.id.clone());
            ligar(me.clone(), r(Tipo::Divida, p.divida.clone()));
            if let Some(c) = self
                .dividas
                .iter()
                .find(|d| d.id == p.divida)
                .and_then(|d| d.contrato.clone())
                && let Some(m) = &p.mov
            {
                ligar(r(Tipo::Movimentacao, m.clone()), r(Tipo::Contrato, c));
            }
            if p.mov.is_none() && p.vence.as_str() >= "2026-05-20" {
                ligar(me, r(Tipo::Ciclo, ciclo_de(&p.vence)));
            }
        }
        for x in &self.recorrentes {
            let me = r(Tipo::Recorrente, x.id.clone());
            for (t, v) in [
                (Tipo::Fornecedor, &x.fornecedor),
                (Tipo::Categoria, &x.categoria),
                (Tipo::Contrato, &x.contrato),
                (Tipo::Conta, &x.conta),
                (Tipo::Centro, &x.centro),
            ] {
                if let Some(v) = v {
                    ligar(me.clone(), r(t, v.clone()));
                }
            }
        }
        for c in &self.compromissos {
            let me = r(Tipo::Compromisso, c.id.clone());
            ligar(me.clone(), r(Tipo::Recorrente, c.recorrente.clone()));
            ligar(me.clone(), r(Tipo::Ciclo, ciclo_de(&c.vence)));
            if let Some(rec) = self.recorrentes.iter().find(|x| x.id == c.recorrente) {
                for (t, v) in [
                    (Tipo::Fornecedor, &rec.fornecedor),
                    (Tipo::Categoria, &rec.categoria),
                    (Tipo::Centro, &rec.centro),
                ] {
                    if let Some(v) = v {
                        ligar(me.clone(), r(t, v.clone()));
                    }
                }
                if let (Some(k), Some(m)) = (&rec.contrato, &c.mov) {
                    ligar(
                        r(Tipo::Movimentacao, m.clone()),
                        r(Tipo::Contrato, k.clone()),
                    );
                }
            }
            if let Some(v) = &c.conta {
                ligar(me.clone(), r(Tipo::Conta, v.clone()));
            }
            if let Some(m) = &c.mov {
                ligar(me, r(Tipo::Movimentacao, m.clone()));
            }
        }
        for c in &self.contratos {
            let me = r(Tipo::Contrato, c.id.clone());
            if let Some(f) = &c.fornecedor {
                ligar(me.clone(), r(Tipo::Fornecedor, f.clone()));
            }
            if let Some(i) = &c.instituicao {
                ligar(me, r(Tipo::Instituicao, i.clone()));
            }
        }
        for t in &self.transferencias {
            let me = r(Tipo::Transferencia, t.id.clone());
            ligar(me.clone(), r(Tipo::Conta, t.de.clone()));
            ligar(me.clone(), r(Tipo::Conta, t.para.clone()));
            ligar(me, r(Tipo::Ciclo, ciclo_de(&t.data)));
        }
        for d in &self.documentos {
            let me = r(Tipo::Documento, d.id.clone());
            for x in &d.ligado_a {
                ligar(me.clone(), x.clone());
                // O documento de uma fatura é também do cartão.
                if x.tipo == Tipo::Fatura
                    && let Some(f) = self.faturas.iter().find(|f| f.id == x.chave)
                {
                    ligar(me.clone(), r(Tipo::Cartao, f.cartao.clone()));
                }
            }
        }
        self.arestas = a;
    }

    // -----------------------------------------------------------------------
    // Busca
    // -----------------------------------------------------------------------

    fn mov(&self, id: &str) -> Option<&Mov> {
        self.movs.iter().find(|m| m.id == id)
    }

    fn nome_de(lista: &[Nome], id: &str) -> Option<String> {
        lista.iter().find(|x| x.id == id).map(|x| x.nome.clone())
    }

    /// O fornecedor pela chave: o UUID ou, como apelido, o nome do inventário (Q17).
    fn fornecedor(&self, chave: &str) -> Option<&Fornecedor> {
        self.fornecedores
            .iter()
            .find(|f| f.id == chave)
            .or_else(|| {
                self.fornecedores
                    .iter()
                    .find(|f| f.nome == chave || f.apelidos.iter().any(|a| a == chave))
            })
    }

    /// A chave do livro-caixa de um `Ref` que pode vir do inventário pelo nome.
    fn normalizar(&self, r: &Ref) -> Ref {
        match r.tipo {
            Tipo::Fornecedor => self
                .fornecedor(&r.chave)
                .map(|f| Ref::nova(Tipo::Fornecedor, f.id.clone()))
                .unwrap_or_else(|| r.clone()),
            Tipo::Conta => self
                .contas
                .iter()
                .find(|c| c.id == r.chave || c.nome == r.chave)
                .map(|c| Ref::nova(Tipo::Conta, c.id.clone()))
                .unwrap_or_else(|| r.clone()),
            _ => r.clone(),
        }
    }

    fn rotulo(&self, r: &Ref) -> Option<String> {
        let k = r.chave.as_str();
        Some(match r.tipo {
            Tipo::Movimentacao => {
                let m = self.mov(k)?;
                format!("{} · {}", m.descricao, dia_mes(&m.data))
            }
            Tipo::Transferencia => {
                let t = self.transferencias.iter().find(|t| t.id == k)?;
                format!(
                    "Para a {} · {}",
                    self.rotulo_conta(&t.para),
                    dia_mes(&t.data)
                )
            }
            Tipo::Conta => self.rotulo_conta_opt(k)?,
            Tipo::Produto => self.produtos.iter().find(|c| c.id == k)?.nome.clone(),
            Tipo::Instituicao => Self::nome_de(&self.instituicoes, k)?,
            Tipo::Cartao => self.cartoes.iter().find(|c| c.id == k)?.nome.clone(),
            Tipo::Fatura => {
                let f = self.faturas.iter().find(|f| f.id == k)?;
                let c = self.cartoes.iter().find(|c| c.id == f.cartao)?;
                format!("{} · {}", c.nome, f.referencia)
            }
            Tipo::Categoria => Self::nome_de(&self.categorias, k)?,
            Tipo::Tag => Self::nome_de(&self.tags, k)?,
            Tipo::Centro => Self::nome_de(&self.centros, k)?,
            Tipo::Fornecedor => self.fornecedor(k)?.nome.clone(),
            Tipo::Contrato => self.contratos.iter().find(|c| c.id == k)?.nome.clone(),
            Tipo::Divida => self.dividas.iter().find(|d| d.id == k)?.nome.clone(),
            Tipo::Parcela => {
                let p = self.parcelas.iter().find(|p| p.id == k)?;
                let d = self.dividas.iter().find(|d| d.id == p.divida)?;
                format!("{} · {}/{}", d.nome, p.numero, p.de)
            }
            Tipo::Recorrente => self.recorrentes.iter().find(|x| x.id == k)?.nome.clone(),
            Tipo::Compromisso => self
                .compromissos
                .iter()
                .find(|c| c.id == k)?
                .descricao
                .clone(),
            Tipo::Documento => self.documentos.iter().find(|d| d.id == k)?.nome.clone(),
            Tipo::Ciclo if k.len() == 7 => format!("Ciclo {k}"),
            _ => return None,
        })
    }

    fn rotulo_conta_opt(&self, id: &str) -> Option<String> {
        self.contas
            .iter()
            .chain(self.produtos.iter())
            .find(|c| c.id == id)
            .map(|c| c.nome.clone())
    }

    fn rotulo_conta(&self, id: &str) -> String {
        self.rotulo_conta_opt(id).unwrap_or_else(|| id.to_owned())
    }

    fn link(&self, tipo: Tipo, id: &str) -> Celula {
        let alvo = Ref::nova(tipo, id);
        match self.rotulo(&alvo) {
            Some(texto) => Celula::Link { alvo, texto },
            None => Celula::Vazia,
        }
    }

    fn link_opt(&self, tipo: Tipo, id: &Option<String>) -> Celula {
        match id {
            Some(id) => self.link(tipo, id),
            None => Celula::Vazia,
        }
    }

    fn rateado(&self, mov: &str) -> Centavos {
        self.rateios
            .iter()
            .filter(|r| r.mov == mov)
            .map(|r| r.valor)
            .sum()
    }

    fn nao_alocado(&self, m: &Mov) -> Centavos {
        if !m.tipo.rateavel_como_saida() {
            return 0;
        }
        (m.valor.abs() - self.rateado(&m.id)).max(0)
    }

    fn aguardando_para(&self, mov: &str) -> Option<&Alteracao> {
        self.alteracoes
            .iter()
            .find(|a| a.estado == EstadoDaAlteracao::Aguardando && a.pedido.alvo().chave == mov)
    }

    // -----------------------------------------------------------------------
    // Os números (spec 07, "Os números e as fórmulas")
    // -----------------------------------------------------------------------

    fn movs_do_ciclo<'a>(&'a self, ciclo: &'a str) -> impl Iterator<Item = &'a Mov> + 'a {
        self.movs.iter().filter(move |m| ciclo_de(&m.data) == ciclo)
    }

    fn disponivel_agora(&self) -> Centavos {
        self.contas.iter().map(|c| c.saldo).sum()
    }

    fn entrou(&self, ciclo: &str) -> Centavos {
        self.movs_do_ciclo(ciclo)
            .filter(|m| m.confirmada && m.tipo.entra() && m.cartao.is_none())
            .map(|m| m.valor)
            .sum()
    }

    fn e_saida_de_conta(m: &Mov) -> bool {
        m.confirmada
            && m.cartao.is_none()
            && matches!(
                m.tipo,
                TipoMov::Despesa
                    | TipoMov::Tarifa
                    | TipoMov::PagamentoDeFatura
                    | TipoMov::PagamentoDeDivida
            )
    }

    fn saiu(&self, ciclo: &str) -> Centavos {
        self.movs_do_ciclo(ciclo)
            .filter(|m| Self::e_saida_de_conta(m))
            .map(|m| m.valor)
            .sum()
    }

    fn no_cartao(&self, ciclo: &str) -> Centavos {
        self.movs_do_ciclo(ciclo)
            .filter(|m| m.confirmada && m.cartao.is_some())
            .map(|m| m.valor)
            .sum()
    }

    fn fim_do_ciclo_atual(&self) -> String {
        fim_do_ciclo(CICLO_ATUAL)
    }

    /// Os compromissos do ciclo atual: (alvo, vence, descrição, valor, sem valor).
    fn compromissos_do_ciclo(&self) -> Vec<(Ref, String, String, Centavos, bool)> {
        let fim = self.fim_do_ciclo_atual();
        let mut v = Vec::new();
        for m in self.movs_do_ciclo(CICLO_ATUAL) {
            if !m.confirmada && m.valor < 0 && m.fatura.is_none() && m.parcela.is_none() {
                v.push((
                    Ref::nova(Tipo::Movimentacao, m.id.clone()),
                    m.data.clone(),
                    m.descricao.clone(),
                    -m.valor,
                    false,
                ));
            }
        }
        for c in &self.compromissos {
            let ativa = self
                .recorrentes
                .iter()
                .any(|r| r.id == c.recorrente && r.ativa);
            if ativa && c.mov.is_none() && c.vence <= fim {
                v.push((
                    Ref::nova(Tipo::Compromisso, c.id.clone()),
                    c.vence.clone(),
                    c.descricao.clone(),
                    c.valor.unwrap_or(0),
                    c.valor.is_none(),
                ));
            }
        }
        for f in &self.faturas {
            let saldo = f.total - f.pago;
            if saldo > 0 {
                v.push((
                    Ref::nova(Tipo::Fatura, f.id.clone()),
                    f.vence.clone(),
                    format!(
                        "Fatura {}",
                        self.rotulo(&Ref::nova(Tipo::Fatura, f.id.clone()))
                            .unwrap_or_default()
                    ),
                    saldo,
                    false,
                ));
            }
        }
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    }

    fn saldo_do_centro(&self, centro: &str) -> (Centavos, Centavos) {
        let (mut entrou, mut saiu) = (0, 0);
        for r in self.rateios.iter().filter(|r| r.centro == centro) {
            let Some(m) = self.mov(&r.mov) else { continue };
            match m.tipo {
                TipoMov::Ajuste => {}
                t if t.entra() => entrou += r.valor,
                _ => saiu += r.valor,
            }
        }
        (entrou, saiu)
    }

    fn equivalente_mensal(r: &Recorrente) -> Option<Centavos> {
        let v = r.valor?;
        Some(match r.frequencia {
            "semanal" => v * 52 / 12,
            "quinzenal" => v * 26 / 12,
            "trimestral" => v / 3,
            "anual" => v / 12,
            _ => v,
        })
    }

    fn proxima_parcela(&self, divida: &str) -> Option<&Parcela> {
        self.parcelas.iter().find(|p| p.divida == divida && !p.paga)
    }

    fn saldo_da_divida(&self, divida: &str) -> Centavos {
        self.parcelas
            .iter()
            .rfind(|p| p.divida == divida && p.paga)
            .map(|p| p.saldo_depois)
            .or_else(|| {
                self.dividas
                    .iter()
                    .find(|d| d.id == divida)
                    .map(|d| d.principal)
            })
            .unwrap_or(0)
    }

    fn restantes(&self, divida: &str) -> i64 {
        self.parcelas
            .iter()
            .filter(|p| p.divida == divida && !p.paga)
            .count() as i64
    }
}

// ---------------------------------------------------------------------------
// Células e tabelas
// ---------------------------------------------------------------------------

fn col(r: &str) -> Coluna {
    Coluna {
        rotulo: r.to_owned(),
        numerica: false,
    }
}

fn coln(r: &str) -> Coluna {
    Coluna {
        rotulo: r.to_owned(),
        numerica: true,
    }
}

fn txt(t: impl Into<String>) -> Celula {
    Celula::Texto(t.into())
}

fn valor(c: Centavos, itens: Option<PedidoDeTabela>) -> Celula {
    Celula::Valor {
        centavos: c,
        fonte: Fonte::LivroCaixa,
        itens: itens.map(Box::new),
    }
}

fn contagem(n: usize, itens: Option<PedidoDeTabela>) -> Celula {
    Celula::Contagem {
        n: n as i64,
        itens: itens.map(Box::new),
    }
}

fn estado(rotulo: &str) -> Celula {
    Celula::Estado {
        rotulo: rotulo.to_owned(),
        quem: None,
    }
}

/// O pedido "o que soma este número" no livro-caixa.
pub fn total(de: Ref, ciclo: Option<&str>, recorte: Option<&str>) -> Option<PedidoDeTabela> {
    Some(PedidoDeTabela::ItensDoTotal {
        de,
        fonte: Fonte::LivroCaixa,
        ciclo: ciclo.map(str::to_owned),
        recorte: recorte.map(str::to_owned),
    })
}

fn ligados(de: Ref, tipo: Tipo) -> Option<PedidoDeTabela> {
    Some(PedidoDeTabela::Ligados { de, tipo })
}

fn ciclo_ref(c: &str) -> Ref {
    Ref::nova(Tipo::Ciclo, c)
}

fn percentual(bp: i64) -> String {
    format!("{},{:02}% a.m.", bp / 100, bp % 100)
}

fn passa_texto(filtro: &Filtro, campos: &[&str]) -> bool {
    match &filtro.texto {
        None => true,
        Some(t) => {
            let t = t.to_lowercase();
            campos.iter().any(|c| c.to_lowercase().contains(&t))
        }
    }
}

fn passa_ciclo(filtro: &Filtro, ciclo: &str) -> bool {
    match &filtro.ciclos {
        None => true,
        Some((de, ate)) => ciclo >= de.as_str() && ciclo <= ate.as_str(),
    }
}

impl Livro {
    fn colunas(tipo: Tipo) -> Vec<Coluna> {
        match tipo {
            Tipo::Movimentacao => vec![
                col("Data"),
                col("Movimentação"),
                coln("Valor"),
                col("Fornecedor"),
                col("Categoria"),
                col("Conta ou cartão"),
                col("Centros"),
                col("Situação"),
            ],
            Tipo::Conta | Tipo::Produto => vec![
                col("Conta"),
                col("Instituição"),
                col("Tipo"),
                coln("Saldo"),
                col("Atualizado"),
                col("Origem"),
            ],
            Tipo::Cartao => vec![
                col("Cartão"),
                col("Instituição"),
                col("Fatura atual"),
                coln("Valor"),
                col("Fecha"),
                col("Vence"),
                coln("Limite"),
                coln("Limite livre"),
            ],
            Tipo::Fatura => vec![
                col("Fatura"),
                col("Cartão"),
                col("Fecha"),
                col("Vence"),
                coln("Total"),
                coln("Pago"),
                col("Situação"),
            ],
            Tipo::Instituicao => vec![col("Instituição"), coln("Contas"), coln("Cartões")],
            Tipo::Categoria => vec![col("Categoria"), col("Grupo"), coln("No ciclo atual")],
            Tipo::Tag => vec![col("Tag"), coln("Movimentações")],
            Tipo::Centro => vec![
                col("Centro"),
                coln("Entrou"),
                coln("Saiu"),
                coln("Saldo"),
                coln("Rateios"),
            ],
            Tipo::Compromisso => vec![
                col("Vence"),
                col("Compromisso"),
                col("Recorrente"),
                col("Conta"),
                coln("Valor"),
                col("Situação"),
            ],
            Tipo::Recorrente => vec![
                col("Recorrente"),
                col("Fornecedor"),
                col("Categoria"),
                col("Frequência"),
                coln("Valor"),
                coln("Por mês"),
                col("Próximo"),
                col("Situação"),
            ],
            Tipo::Divida => vec![
                col("Dívida"),
                col("Credor"),
                col("Sistema"),
                coln("Saldo devedor"),
                col("Parcelas"),
                col("Taxa"),
                col("Próxima parcela"),
                coln("Valor"),
                col("Previsão de fim"),
            ],
            Tipo::Parcela => vec![
                col("Parcela"),
                col("Vence"),
                coln("Valor"),
                coln("Juros"),
                coln("Amortização"),
                coln("Saldo depois"),
                col("Situação"),
            ],
            Tipo::Fornecedor => vec![
                col("Fornecedor"),
                col("Categoria"),
                coln("Movimentações"),
                coln("Total"),
                col("Última"),
                col("Apelidos"),
            ],
            Tipo::Contrato => vec![col("Contrato"), col("Com"), col("Início"), col("Fim")],
            Tipo::Transferencia => vec![
                col("Data"),
                col("Transferência"),
                col("De"),
                col("Para"),
                coln("Valor"),
            ],
            Tipo::Documento => vec![col("Documento"), col("Tipo"), col("Data"), col("Situação")],
            Tipo::Ciclo => vec![
                col("Ciclo"),
                col("Período"),
                coln("Entrou"),
                coln("Saiu"),
                coln("No cartão"),
                coln("Resultado do mês"),
            ],
            _ => vec![col("Tipo"), col("Item")],
        }
    }

    fn linha(&self, r: &Ref) -> Option<Linha> {
        let k = r.chave.as_str();
        let alvo = Some(r.clone());
        let celulas = match r.tipo {
            Tipo::Movimentacao => {
                let m = self.mov(k)?;
                let centros: Vec<(Ref, String)> = self
                    .rateios
                    .iter()
                    .filter(|x| x.mov == m.id)
                    .filter_map(|x| {
                        let c = Ref::nova(Tipo::Centro, x.centro.clone());
                        self.rotulo(&c).map(|n| (c, n))
                    })
                    .collect();
                let situacao = if self.aguardando_para(&m.id).is_some() {
                    "Alteração aguardando"
                } else if !m.confirmada {
                    "Prevista"
                } else if self.nao_alocado(m) > 0 {
                    "Não alocada em parte"
                } else {
                    "Confirmada"
                };
                vec![
                    Celula::Data(m.data.clone()),
                    Celula::Link {
                        alvo: r.clone(),
                        texto: m.descricao.clone(),
                    },
                    valor(m.valor, None),
                    self.link_opt(Tipo::Fornecedor, &m.fornecedor),
                    match &m.categoria {
                        Some(c) => self.link(Tipo::Categoria, c),
                        None => txt("Sem categoria"),
                    },
                    match (&m.conta, &m.cartao) {
                        (_, Some(c)) => self.link(Tipo::Cartao, c),
                        (Some(c), None) => self.link(Tipo::Conta, c),
                        _ => Celula::Vazia,
                    },
                    if centros.is_empty() {
                        Celula::Vazia
                    } else {
                        Celula::Links(centros)
                    },
                    estado(situacao),
                ]
            }
            Tipo::Conta | Tipo::Produto => {
                let c = self
                    .contas
                    .iter()
                    .chain(self.produtos.iter())
                    .find(|c| c.id == k)?;
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: c.nome.clone(),
                    },
                    self.link(Tipo::Instituicao, &c.instituicao),
                    txt(c.tipo),
                    valor(c.saldo, ligados(r.clone(), Tipo::Movimentacao)),
                    Celula::Data(c.atualizado.clone()),
                    txt(c.origem),
                ]
            }
            Tipo::Cartao => {
                let c = self.cartoes.iter().find(|c| c.id == k)?;
                let aberta = self
                    .faturas
                    .iter()
                    .filter(|f| f.cartao == c.id && f.total > f.pago)
                    .min_by(|a, b| a.vence.cmp(&b.vence));
                let atual = aberta.map(|f| f.total - f.pago).unwrap_or(0);
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: c.nome.clone(),
                    },
                    self.link(Tipo::Instituicao, &c.instituicao),
                    aberta.map_or(Celula::Vazia, |f| self.link(Tipo::Fatura, &f.id)),
                    valor(
                        atual,
                        aberta
                            .and_then(|f| total(Ref::nova(Tipo::Fatura, f.id.clone()), None, None)),
                    ),
                    aberta.map_or(Celula::Vazia, |f| Celula::Data(f.fecha.clone())),
                    aberta.map_or(Celula::Vazia, |f| Celula::Data(f.vence.clone())),
                    valor(c.limite, None),
                    valor(c.limite - atual, None),
                ]
            }
            Tipo::Fatura => {
                let f = self.faturas.iter().find(|f| f.id == k)?;
                let situacao = if f.pago >= f.total {
                    "Paga"
                } else if f.fecha.as_str() > HOJE {
                    "Aberta"
                } else {
                    "Fechada"
                };
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    self.link(Tipo::Cartao, &f.cartao),
                    Celula::Data(f.fecha.clone()),
                    Celula::Data(f.vence.clone()),
                    valor(f.total, total(r.clone(), None, None)),
                    valor(f.pago, total(r.clone(), None, Some("pagamento"))),
                    estado(situacao),
                ]
            }
            Tipo::Instituicao => {
                let viz = self.arestas.get(r).cloned().unwrap_or_default();
                let n = |t: Tipo| viz.iter().filter(|x| x.tipo == t).count();
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    contagem(n(Tipo::Conta), ligados(r.clone(), Tipo::Conta)),
                    contagem(n(Tipo::Cartao), ligados(r.clone(), Tipo::Cartao)),
                ]
            }
            Tipo::Categoria => {
                let c = self.categorias.iter().find(|c| c.id == k)?;
                let soma: Centavos = self
                    .movs_do_ciclo(CICLO_ATUAL)
                    .filter(|m| m.categoria.as_deref() == Some(k) && m.tipo != TipoMov::Ajuste)
                    .map(|m| m.valor)
                    .sum();
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: c.nome.clone(),
                    },
                    txt(c.extra.clone()),
                    valor(soma, total(r.clone(), Some(CICLO_ATUAL), None)),
                ]
            }
            Tipo::Tag => {
                let n = self
                    .movs
                    .iter()
                    .filter(|m| m.tags.iter().any(|t| t == k))
                    .count();
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    contagem(n, ligados(r.clone(), Tipo::Movimentacao)),
                ]
            }
            Tipo::Centro => {
                let (e, s) = self.saldo_do_centro(k);
                let n = self.rateios.iter().filter(|x| x.centro == k).count();
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    valor(e, total(r.clone(), None, Some("entradas"))),
                    valor(-s, total(r.clone(), None, Some("saidas"))),
                    valor(e - s, total(r.clone(), None, Some("rateios"))),
                    contagem(n, total(r.clone(), None, Some("rateios"))),
                ]
            }
            Tipo::Compromisso => {
                let c = self.compromissos.iter().find(|c| c.id == k)?;
                let situacao = if c.mov.is_some() {
                    "Pago"
                } else if c.vence.as_str() < HOJE {
                    "Vencido"
                } else if c.valor.is_none() {
                    "Sem valor"
                } else {
                    "A vencer"
                };
                vec![
                    Celula::Data(c.vence.clone()),
                    Celula::Link {
                        alvo: r.clone(),
                        texto: c.descricao.clone(),
                    },
                    self.link(Tipo::Recorrente, &c.recorrente),
                    self.link_opt(Tipo::Conta, &c.conta),
                    match c.valor {
                        Some(v) => valor(-v, None),
                        None => txt("sem valor"),
                    },
                    estado(situacao),
                ]
            }
            Tipo::Recorrente => {
                let x = self.recorrentes.iter().find(|x| x.id == k)?;
                let proximo = self
                    .compromissos
                    .iter()
                    .filter(|c| c.recorrente == x.id && c.mov.is_none())
                    .min_by(|a, b| a.vence.cmp(&b.vence));
                let valor_txt = match (x.valor, x.variavel) {
                    (Some(v), false) => valor(-v, None),
                    (Some(v), true) => txt(format!("~{} (variável)", formato::dinheiro(-v))),
                    (None, _) => txt("sem valor"),
                };
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: x.nome.clone(),
                    },
                    self.link_opt(Tipo::Fornecedor, &x.fornecedor),
                    self.link_opt(Tipo::Categoria, &x.categoria),
                    txt(x.frequencia),
                    valor_txt,
                    match Self::equivalente_mensal(x) {
                        Some(v) => valor(-v, None),
                        None => txt("estimativa"),
                    },
                    proximo.map_or(Celula::Vazia, |c| self.link(Tipo::Compromisso, &c.id)),
                    estado(if x.ativa { "Ativa" } else { "Pausada" }),
                ]
            }
            Tipo::Divida => {
                let d = self.dividas.iter().find(|d| d.id == k)?;
                let pagas = self
                    .parcelas
                    .iter()
                    .filter(|p| p.divida == d.id && p.paga)
                    .count();
                let todas = self.parcelas.iter().filter(|p| p.divida == d.id).count();
                let prox = self.proxima_parcela(&d.id);
                let fim = self
                    .parcelas
                    .iter()
                    .filter(|p| p.divida == d.id)
                    .map(|p| p.vence.clone())
                    .max();
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: d.nome.clone(),
                    },
                    self.link(Tipo::Instituicao, &d.credor),
                    txt(d.sistema),
                    valor(
                        -self.saldo_da_divida(&d.id),
                        ligados(r.clone(), Tipo::Parcela),
                    ),
                    txt(format!("{pagas}/{todas}")),
                    txt(if d.taxa_bp > 0 {
                        percentual(d.taxa_bp)
                    } else {
                        "sem juros".to_owned()
                    }),
                    prox.map_or(Celula::Vazia, |p| self.link(Tipo::Parcela, &p.id)),
                    prox.map_or(Celula::Vazia, |p| valor(-p.valor, None)),
                    fim.map_or(Celula::Vazia, Celula::Data),
                ]
            }
            Tipo::Parcela => {
                let p = self.parcelas.iter().find(|p| p.id == k)?;
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    Celula::Data(p.vence.clone()),
                    valor(-p.valor, None),
                    valor(-p.juros, None),
                    valor(-p.amortizacao, None),
                    valor(-p.saldo_depois, None),
                    estado(if p.paga {
                        "Paga"
                    } else if p.vence.as_str() < HOJE {
                        "Vencida"
                    } else {
                        "A vencer"
                    }),
                ]
            }
            Tipo::Fornecedor => {
                let f = self.fornecedor(k)?;
                let movs: Vec<&Mov> = self
                    .movs
                    .iter()
                    .filter(|m| m.fornecedor.as_deref() == Some(f.id.as_str()))
                    .collect();
                let fr = Ref::nova(Tipo::Fornecedor, f.id.clone());
                vec![
                    Celula::Link {
                        alvo: fr.clone(),
                        texto: f.nome.clone(),
                    },
                    self.link_opt(Tipo::Categoria, &f.categoria),
                    contagem(movs.len(), ligados(fr.clone(), Tipo::Movimentacao)),
                    valor(
                        movs.iter()
                            .filter(|m| m.tipo != TipoMov::Ajuste)
                            .map(|m| m.valor)
                            .sum(),
                        total(fr, None, None),
                    ),
                    movs.iter()
                        .map(|m| m.data.clone())
                        .max()
                        .map_or(Celula::Vazia, Celula::Data),
                    if f.apelidos.is_empty() {
                        Celula::Vazia
                    } else {
                        txt(f.apelidos.join(" · "))
                    },
                ]
            }
            Tipo::Contrato => {
                let c = self.contratos.iter().find(|c| c.id == k)?;
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: c.nome.clone(),
                    },
                    match (&c.fornecedor, &c.instituicao) {
                        (Some(f), _) => self.link(Tipo::Fornecedor, f),
                        (None, Some(i)) => self.link(Tipo::Instituicao, i),
                        _ => Celula::Vazia,
                    },
                    Celula::Data(c.inicio.clone()),
                    c.fim.clone().map_or(Celula::Vazia, Celula::Data),
                ]
            }
            Tipo::Transferencia => {
                let t = self.transferencias.iter().find(|t| t.id == k)?;
                vec![
                    Celula::Data(t.data.clone()),
                    Celula::Link {
                        alvo: r.clone(),
                        texto: self.rotulo(r)?,
                    },
                    self.link(Tipo::Conta, &t.de),
                    self.link(Tipo::Conta, &t.para),
                    valor(t.valor, None),
                ]
            }
            Tipo::Documento => {
                let d = self.documentos.iter().find(|d| d.id == k)?;
                vec![
                    Celula::Link {
                        alvo: r.clone(),
                        texto: d.nome.clone(),
                    },
                    txt(d.tipo),
                    Celula::Data(d.data.clone()),
                    estado(d.estado),
                ]
            }
            Tipo::Ciclo => {
                let c = ciclo_ref(k);
                vec![
                    Celula::Link {
                        alvo: c.clone(),
                        texto: k.to_owned(),
                    },
                    txt(format!(
                        "{} a {}",
                        formato::data(&inicio_do_ciclo(k)),
                        formato::data(&fim_do_ciclo(k))
                    )),
                    valor(self.entrou(k), total(c.clone(), Some(k), Some("entrou"))),
                    valor(self.saiu(k), total(c.clone(), Some(k), Some("saiu"))),
                    valor(
                        self.no_cartao(k),
                        total(c.clone(), Some(k), Some("no-cartao")),
                    ),
                    valor(
                        self.entrou(k) + self.saiu(k),
                        total(c, Some(k), Some("resultado")),
                    ),
                ]
            }
            _ => return None,
        };
        Some(Linha { alvo, celulas })
    }

    /// Uma tabela de itens do mesmo tipo (ou mista, com Tipo e Item).
    fn tabela_de(&self, tipo: Option<Tipo>, refs: &[Ref]) -> Tabela {
        match tipo {
            Some(t) => Tabela {
                colunas: Self::colunas(t),
                linhas: refs.iter().filter_map(|r| self.linha(r)).collect(),
            },
            None => Tabela {
                colunas: vec![col("Tipo"), col("Item")],
                linhas: refs
                    .iter()
                    .filter_map(|r| {
                        let rotulo = self.rotulo(r)?;
                        Some(Linha {
                            alvo: Some(r.clone()),
                            celulas: vec![
                                txt(r.tipo.singular()),
                                Celula::Link {
                                    alvo: r.clone(),
                                    texto: rotulo,
                                },
                            ],
                        })
                    })
                    .collect(),
            },
        }
    }

    fn refs_de_movs<'a>(movs: impl Iterator<Item = &'a Mov>) -> Vec<Ref> {
        let mut v: Vec<&Mov> = movs.collect();
        v.sort_by(|a, b| b.data.cmp(&a.data).then(a.id.cmp(&b.id)));
        v.into_iter()
            .map(|m| Ref::nova(Tipo::Movimentacao, m.id.clone()))
            .collect()
    }

    fn ordenar(&self, tipo: Tipo, refs: &mut [Ref]) {
        let chave = |r: &Ref| -> String {
            match tipo {
                Tipo::Movimentacao => self
                    .mov(&r.chave)
                    .map(|m| m.data.clone())
                    .unwrap_or_default(),
                Tipo::Compromisso => self
                    .compromissos
                    .iter()
                    .find(|c| c.id == r.chave)
                    .map(|c| c.vence.clone())
                    .unwrap_or_default(),
                Tipo::Parcela => self
                    .parcelas
                    .iter()
                    .find(|p| p.id == r.chave)
                    .map(|p| p.vence.clone())
                    .unwrap_or_default(),
                Tipo::Fatura => self
                    .faturas
                    .iter()
                    .find(|f| f.id == r.chave)
                    .map(|f| f.vence.clone())
                    .unwrap_or_default(),
                Tipo::Transferencia => self
                    .transferencias
                    .iter()
                    .find(|t| t.id == r.chave)
                    .map(|t| t.data.clone())
                    .unwrap_or_default(),
                _ => self.rotulo(r).unwrap_or_default(),
            }
        };
        let decrescente = matches!(
            tipo,
            Tipo::Movimentacao | Tipo::Fatura | Tipo::Transferencia
        );
        refs.sort_by(|a, b| {
            let o = chave(a).cmp(&chave(b));
            if decrescente { o.reverse() } else { o }
        });
    }

    fn ligados_a(&self, de: &Ref, tipo: Tipo) -> Vec<Ref> {
        let de = self.normalizar(de);
        if de.tipo == Tipo::Ciclo && tipo == Tipo::Movimentacao {
            return Self::refs_de_movs(self.movs_do_ciclo(&de.chave));
        }
        let mut v: Vec<Ref> = self
            .arestas
            .get(&de)
            .map(|s| s.iter().filter(|r| r.tipo == tipo).cloned().collect())
            .unwrap_or_default();
        self.ordenar(tipo, &mut v);
        v
    }

    fn tabela_ligados(&self, de: &Ref, tipo: Tipo) -> Tabela {
        // `Ligados { de: x, tipo: x.tipo }` é a aba "Ligações": tudo o que se liga a x.
        if tipo == de.tipo && tipo != Tipo::Ciclo {
            let de = self.normalizar(de);
            let v: Vec<Ref> = self
                .arestas
                .get(&de)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            return self.tabela_de(None, &v);
        }
        if tipo == Tipo::Ciclo && de.tipo == Tipo::Ciclo {
            return self.tabela_de(Some(Tipo::Ciclo), std::slice::from_ref(de));
        }
        self.tabela_de(Some(tipo), &self.ligados_a(de, tipo))
    }

    // -----------------------------------------------------------------------
    // As áreas
    // -----------------------------------------------------------------------

    fn area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<Tabela> {
        Ok(match area {
            Area::Movimentacoes => {
                let refs = Self::refs_de_movs(self.movs.iter().filter(|m| {
                    passa_ciclo(filtro, &ciclo_de(&m.data))
                        && passa_texto(
                            filtro,
                            &[
                                &m.descricao,
                                &m.fornecedor
                                    .as_ref()
                                    .and_then(|f| self.fornecedor(f))
                                    .map(|f| f.nome.clone())
                                    .unwrap_or_default(),
                                &m.categoria
                                    .as_ref()
                                    .and_then(|c| Self::nome_de(&self.categorias, c))
                                    .unwrap_or_default(),
                            ],
                        )
                }));
                self.tabela_de(Some(Tipo::Movimentacao), &refs)
            }
            Area::ContasECartoes => {
                let refs: Vec<Ref> = self
                    .contas
                    .iter()
                    .chain(self.produtos.iter())
                    .filter(|c| passa_texto(filtro, &[&c.nome]))
                    .map(|c| {
                        let t = if self.contas.iter().any(|x| x.id == c.id) {
                            Tipo::Conta
                        } else {
                            Tipo::Produto
                        };
                        Ref::nova(t, c.id.clone())
                    })
                    .collect();
                self.tabela_de(Some(Tipo::Conta), &refs)
            }
            Area::Dividas => {
                let refs: Vec<Ref> = self
                    .dividas
                    .iter()
                    .filter(|d| passa_texto(filtro, &[&d.nome]))
                    .map(|d| Ref::nova(Tipo::Divida, d.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Divida), &refs)
            }
            Area::Recorrentes => {
                let refs: Vec<Ref> = self
                    .recorrentes
                    .iter()
                    .filter(|r| passa_texto(filtro, &[&r.nome]))
                    .map(|r| Ref::nova(Tipo::Recorrente, r.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Recorrente), &refs)
            }
            Area::Centros => {
                let refs: Vec<Ref> = self
                    .centros
                    .iter()
                    .filter(|c| passa_texto(filtro, &[&c.nome]))
                    .map(|c| Ref::nova(Tipo::Centro, c.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Centro), &refs)
            }
            Area::FornecedoresDoLivro => {
                let mut refs: Vec<Ref> = self
                    .fornecedores
                    .iter()
                    .filter(|f| passa_texto(filtro, &[&f.nome]))
                    .map(|f| Ref::nova(Tipo::Fornecedor, f.id.clone()))
                    .collect();
                self.ordenar(Tipo::Fornecedor, &mut refs);
                self.tabela_de(Some(Tipo::Fornecedor), &refs)
            }
            Area::Hoje => {
                return Err(ErroDeConsulta::NaoSuportado(
                    "a Hoje é um painel: use `painel_da_area()`".to_owned(),
                ));
            }
            outra => {
                return Err(ErroDeConsulta::NaoSuportado(format!(
                    "a área {outra:?} não é do livro-caixa"
                )));
            }
        })
    }

    fn ciclo_do_filtro(filtro: &Filtro) -> String {
        filtro
            .ciclos
            .as_ref()
            .map(|(_, ate)| ate.clone())
            .unwrap_or_else(|| CICLO_ATUAL.to_owned())
    }

    fn indicador(rotulo: &str, v: Celula, nota: Option<String>) -> Indicador {
        Indicador {
            rotulo: rotulo.to_owned(),
            valor: v,
            nota,
            estimativa: None,
            instantaneo: false,
        }
    }

    fn painel(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        let ciclo = Self::ciclo_do_filtro(filtro);
        let c = ciclo_ref(&ciclo);
        let periodo = format!(
            "{} a {}",
            formato::data(&inicio_do_ciclo(&ciclo)),
            formato::data(&fim_do_ciclo(&ciclo))
        );
        let resultado = |l: &Livro| {
            Self::indicador(
                "Resultado do mês",
                valor(
                    l.entrou(&ciclo) + l.saiu(&ciclo),
                    total(c.clone(), Some(&ciclo), Some("resultado")),
                ),
                Some(format!(
                    "ciclo {ciclo} · entrou {}, saiu {}",
                    formato::dinheiro(l.entrou(&ciclo)),
                    formato::dinheiro(l.saiu(&ciclo))
                )),
            )
        };
        let disponivel = |l: &Livro| Indicador {
            rotulo: "Disponível agora".to_owned(),
            valor: valor(
                l.disponivel_agora(),
                total(c.clone(), None, Some("disponivel")),
            ),
            nota: Some(format!(
                "soma de {}",
                l.contas
                    .iter()
                    .map(|c| c.nome.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            estimativa: None,
            instantaneo: true,
        };
        let mut p = PainelDeArea {
            ciclo: Some(ciclo.clone()),
            ..Default::default()
        };
        match area {
            Area::Hoje => {
                let comp = self.compromissos_do_ciclo();
                let soma: Centavos = comp.iter().map(|x| x.3).sum();
                let sem_valor = comp.iter().filter(|x| x.4).count();
                let ainda: Centavos = comp
                    .iter()
                    .filter(|x| x.1.as_str() >= HOJE)
                    .map(|x| x.3)
                    .sum();
                let disp = self.disponivel_agora();
                let dividas: Centavos = self
                    .dividas
                    .iter()
                    .map(|d| self.saldo_da_divida(&d.id))
                    .sum();
                let mensal: Centavos = self
                    .dividas
                    .iter()
                    .map(|d| {
                        let n = self.restantes(&d.id).max(1);
                        self.saldo_da_divida(&d.id) / n
                    })
                    .sum();
                p.indicadores = vec![
                    disponivel(self),
                    Indicador {
                        rotulo: "Livre após compromissos".to_owned(),
                        valor: valor(disp - soma, total(c.clone(), None, Some("compromissos"))),
                        nota: Some(format!(
                            "disponível agora menos {} compromissos até {}",
                            comp.len(),
                            formato::data(&fim_do_ciclo(&ciclo))
                        )),
                        estimativa: (sem_valor > 0).then(|| {
                            format!(
                                "{sem_valor} compromisso{} ainda sem valor confirmado: o número pode mudar",
                                if sem_valor == 1 { "" } else { "s" }
                            )
                        }),
                        instantaneo: false,
                    },
                    resultado(self),
                    Self::indicador(
                        "No cartão",
                        valor(
                            self.no_cartao(&ciclo),
                            total(c.clone(), Some(&ciclo), Some("no-cartao")),
                        ),
                        Some("compras do ciclo, pela data da compra".to_owned()),
                    ),
                    Self::indicador(
                        "Ainda vai sair",
                        valor(-ainda, total(c.clone(), None, Some("ainda-vai-sair"))),
                        Some(format!(
                            "falta até o fim: {}",
                            formato::dinheiro((ainda - disp).max(0))
                        )),
                    ),
                    Self::indicador(
                        "Dívida total",
                        valor(-dividas, total(c.clone(), None, Some("dividas"))),
                        Some(format!(
                            "{} por mês · informativo, fora do livre",
                            formato::dinheiro(mensal)
                        )),
                    ),
                ];
                p.secoes.push(self.para_resolver(&ciclo));
                let mut t = Tabela {
                    colunas: vec![
                        col("Vence"),
                        col("Compromisso"),
                        coln("Valor"),
                        col("Situação"),
                    ],
                    linhas: Vec::new(),
                };
                for (alvo, vence, desc, v, sem) in &comp {
                    t.linhas.push(Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            Celula::Data(vence.clone()),
                            Celula::Link {
                                alvo: alvo.clone(),
                                texto: desc.clone(),
                            },
                            if *sem {
                                txt("sem valor")
                            } else {
                                valor(-v, None)
                            },
                            estado(if vence.as_str() < HOJE {
                                "Vencido"
                            } else if *sem {
                                "Sem valor"
                            } else {
                                "A vencer"
                            }),
                        ],
                    });
                }
                p.secoes.push(Secao {
                    titulo: format!("Compromissos até {}", formato::data(&fim_do_ciclo(&ciclo))),
                    nota: Some(
                        "saídas previstas, recorrentes pendentes e faturas abertas".to_owned(),
                    ),
                    tabela: t,
                });
                let centros: Vec<Ref> = self
                    .centros
                    .iter()
                    .map(|x| Ref::nova(Tipo::Centro, x.id.clone()))
                    .collect();
                p.secoes.push(Secao {
                    titulo: "Centros".to_owned(),
                    nota: Some("saldo desde sempre; ajuste de saldo fica de fora".to_owned()),
                    tabela: self.tabela_de(Some(Tipo::Centro), &centros),
                });
            }
            Area::Movimentacoes => {
                p.indicadores = vec![
                    Self::indicador(
                        "Entrou",
                        valor(
                            self.entrou(&ciclo),
                            total(c.clone(), Some(&ciclo), Some("entrou")),
                        ),
                        Some(periodo.clone()),
                    ),
                    Self::indicador(
                        "Saiu",
                        valor(
                            self.saiu(&ciclo),
                            total(c.clone(), Some(&ciclo), Some("saiu")),
                        ),
                        Some("das contas, com o pagamento das faturas".to_owned()),
                    ),
                    Self::indicador(
                        "No cartão",
                        valor(
                            self.no_cartao(&ciclo),
                            total(c.clone(), Some(&ciclo), Some("no-cartao")),
                        ),
                        Some("à parte: entra no saiu quando a fatura é paga".to_owned()),
                    ),
                    resultado(self),
                ];
                p.avisos.push(
                    "Transferências entre contas e ajustes de saldo não entram em entrou nem em saiu."
                        .to_owned(),
                );
            }
            Area::ContasECartoes => {
                let faturas: Centavos =
                    self.faturas.iter().map(|f| (f.total - f.pago).max(0)).sum();
                let limite: Centavos = self.cartoes.iter().map(|c| c.limite).sum();
                p.indicadores = vec![
                    disponivel(self),
                    Self::indicador(
                        "Faturas abertas",
                        valor(-faturas, total(c.clone(), None, Some("faturas-abertas"))),
                        Some("a fatura é um compromisso, não um ciclo".to_owned()),
                    ),
                    Self::indicador(
                        "Limite livre",
                        valor(limite - faturas, None),
                        Some(format!("de {} de limite", formato::dinheiro(limite))),
                    ),
                ];
                let cartoes: Vec<Ref> = self
                    .cartoes
                    .iter()
                    .map(|x| Ref::nova(Tipo::Cartao, x.id.clone()))
                    .collect();
                p.secoes.push(Secao {
                    titulo: "Cartões".to_owned(),
                    nota: Some("fatura atual = total menos o já pago".to_owned()),
                    tabela: self.tabela_de(Some(Tipo::Cartao), &cartoes),
                });
                let ts: Vec<Ref> = self
                    .transferencias
                    .iter()
                    .filter(|t| ciclo_de(&t.data) == ciclo)
                    .map(|t| Ref::nova(Tipo::Transferencia, t.id.clone()))
                    .collect();
                p.secoes.push(Secao {
                    titulo: "Transferências do ciclo".to_owned(),
                    nota: Some("movimento entre contas: não é gasto".to_owned()),
                    tabela: self.tabela_de(Some(Tipo::Transferencia), &ts),
                });
            }
            Area::Dividas => {
                let saldo: Centavos = self
                    .dividas
                    .iter()
                    .map(|d| self.saldo_da_divida(&d.id))
                    .sum();
                let mensal: Centavos = self
                    .dividas
                    .iter()
                    .map(|d| self.saldo_da_divida(&d.id) / self.restantes(&d.id).max(1))
                    .sum();
                p.indicadores = vec![
                    Self::indicador(
                        "Saldo devedor",
                        valor(-saldo, total(c.clone(), None, Some("dividas"))),
                        Some(format!("em {} dívidas", self.dividas.len())),
                    ),
                    Self::indicador(
                        "Compromisso mensal",
                        valor(-mensal, None),
                        Some("saldo ÷ parcelas restantes · informativo".to_owned()),
                    ),
                ];
            }
            Area::Recorrentes => {
                let ativas: Vec<&Recorrente> =
                    self.recorrentes.iter().filter(|r| r.ativa).collect();
                let soma: Centavos = ativas
                    .iter()
                    .filter_map(|r| Self::equivalente_mensal(r))
                    .sum();
                let incertas = ativas
                    .iter()
                    .filter(|r| r.valor.is_none() || r.variavel)
                    .count();
                p.indicadores = vec![Indicador {
                    rotulo: "Total mensal comprometido".to_owned(),
                    valor: valor(-soma, total(c.clone(), None, Some("recorrentes"))),
                    nota: Some(format!("{} recorrentes ativos", ativas.len())),
                    estimativa: (incertas > 0)
                        .then(|| format!("{incertas} com valor variável ou sem valor esperado")),
                    instantaneo: false,
                }];
                let mut proximos: Vec<Ref> = self
                    .compromissos
                    .iter()
                    .filter(|x| x.mov.is_none())
                    .map(|x| Ref::nova(Tipo::Compromisso, x.id.clone()))
                    .collect();
                self.ordenar(Tipo::Compromisso, &mut proximos);
                p.secoes.push(Secao {
                    titulo: "Próximos compromissos".to_owned(),
                    nota: Some("ocorrências ainda sem movimentação".to_owned()),
                    tabela: self.tabela_de(Some(Tipo::Compromisso), &proximos),
                });
            }
            Area::Centros => {
                let movs: Vec<&Mov> = self
                    .movs_do_ciclo(&ciclo)
                    .filter(|m| self.nao_alocado(m) > 0)
                    .collect();
                let soma: Centavos = movs.iter().map(|m| self.nao_alocado(m)).sum();
                p.indicadores = vec![Self::indicador(
                    "Não alocado no ciclo",
                    valor(-soma, total(c.clone(), Some(&ciclo), Some("nao-alocadas"))),
                    Some(format!("{} movimentações com parte sem centro", movs.len())),
                )];
                p.secoes.push(Secao {
                    titulo: "Não alocado".to_owned(),
                    nota: Some(
                        "a parte de cada saída do ciclo que ainda não foi rateada: estado legítimo, não erro"
                            .to_owned(),
                    ),
                    tabela: self.tabela_nao_alocado(&movs),
                });
            }
            Area::FornecedoresDoLivro => {
                p.avisos.push(
                    "Os fornecedores dos registros do inventário (a conferir) ficam em Inventário › Fornecedores nos registros. As duas listas nunca se somam."
                        .to_owned(),
                );
            }
            outra => {
                return Err(ErroDeConsulta::NaoSuportado(format!(
                    "a área {outra:?} não tem painel"
                )));
            }
        }
        Ok(p)
    }

    fn tabela_nao_alocado(&self, movs: &[&Mov]) -> Tabela {
        Tabela {
            colunas: vec![
                col("Data"),
                col("Movimentação"),
                coln("Valor"),
                coln("Rateado"),
                coln("Não alocado"),
            ],
            linhas: movs
                .iter()
                .map(|m| {
                    let r = Ref::nova(Tipo::Movimentacao, m.id.clone());
                    Linha {
                        alvo: Some(r.clone()),
                        celulas: vec![
                            Celula::Data(m.data.clone()),
                            Celula::Link {
                                alvo: r,
                                texto: m.descricao.clone(),
                            },
                            valor(m.valor, None),
                            valor(self.rateado(&m.id), None),
                            valor(-self.nao_alocado(m), None),
                        ],
                    }
                })
                .collect(),
        }
    }

    fn para_resolver(&self, ciclo: &str) -> Secao {
        let c = ciclo_ref(ciclo);
        let vencidos = self
            .compromissos_do_ciclo()
            .into_iter()
            .filter(|x| x.1.as_str() < HOJE)
            .collect::<Vec<_>>();
        let sem_categoria: Vec<&Mov> = self
            .movs_do_ciclo(ciclo)
            .filter(|m| m.categoria.is_none() && m.tipo != TipoMov::PagamentoDeFatura)
            .collect();
        let nao_alocadas: Vec<&Mov> = self
            .movs_do_ciclo(ciclo)
            .filter(|m| self.nao_alocado(m) > 0)
            .collect();
        let nao_confirmadas: Vec<&Mov> = self
            .movs_do_ciclo(ciclo)
            .filter(|m| !m.confirmada)
            .collect();
        let docs = self
            .documentos
            .iter()
            .filter(|d| d.estado == "Precisa revisar")
            .count();
        let linha = |rotulo: &str, n: usize, v: Option<Centavos>, recorte: &str| Linha {
            alvo: None,
            celulas: vec![
                txt(rotulo),
                contagem(n, total(c.clone(), Some(ciclo), Some(recorte))),
                v.map_or(Celula::Vazia, |v| {
                    valor(v, total(c.clone(), Some(ciclo), Some(recorte)))
                }),
            ],
        };
        Secao {
            titulo: "Para resolver".to_owned(),
            nota: Some("o que pede atenção no livro-caixa".to_owned()),
            tabela: Tabela {
                colunas: vec![col("O quê"), coln("Quantos"), coln("Valor")],
                linhas: vec![
                    linha(
                        "Compromissos vencidos",
                        vencidos.len(),
                        Some(-vencidos.iter().map(|x| x.3).sum::<Centavos>()),
                        "vencidos",
                    ),
                    linha(
                        "Movimentações sem categoria",
                        sem_categoria.len(),
                        Some(sem_categoria.iter().map(|m| m.valor).sum()),
                        "sem-categoria",
                    ),
                    linha(
                        "Movimentações não alocadas",
                        nao_alocadas.len(),
                        Some(
                            -nao_alocadas
                                .iter()
                                .map(|m| self.nao_alocado(m))
                                .sum::<Centavos>(),
                        ),
                        "nao-alocadas",
                    ),
                    linha(
                        "Movimentações não confirmadas",
                        nao_confirmadas.len(),
                        Some(nao_confirmadas.iter().map(|m| m.valor).sum()),
                        "nao-confirmadas",
                    ),
                    linha("Documentos a revisar", docs, None, "documentos-a-revisar"),
                ],
            },
        }
    }

    // -----------------------------------------------------------------------
    // Os itens de um total
    // -----------------------------------------------------------------------

    fn itens_do_total(
        &self,
        de: &Ref,
        ciclo: Option<&str>,
        recorte: Option<&str>,
    ) -> ResultadoDeConsulta<Tabela> {
        let de = self.normalizar(de);
        let no_ciclo = |m: &&Mov| ciclo.is_none_or(|c| ciclo_de(&m.data) == c);
        let movs = |f: &dyn Fn(&Mov) -> bool| {
            let refs = Self::refs_de_movs(self.movs.iter().filter(|m| no_ciclo(m) && f(m)));
            self.tabela_de(Some(Tipo::Movimentacao), &refs)
        };
        Ok(match (de.tipo, recorte) {
            (Tipo::Ciclo, Some("entrou")) => {
                movs(&|m| m.confirmada && m.tipo.entra() && m.cartao.is_none())
            }
            (Tipo::Ciclo, Some("saiu")) => movs(&|m| Self::e_saida_de_conta(m)),
            (Tipo::Ciclo, Some("no-cartao")) => movs(&|m| m.confirmada && m.cartao.is_some()),
            (Tipo::Ciclo, Some("resultado")) => movs(&|m| {
                (m.confirmada && m.tipo.entra() && m.cartao.is_none()) || Self::e_saida_de_conta(m)
            }),
            (Tipo::Ciclo, Some("sem-categoria")) => {
                movs(&|m| m.categoria.is_none() && m.tipo != TipoMov::PagamentoDeFatura)
            }
            (Tipo::Ciclo, Some("nao-alocadas")) => {
                let v: Vec<&Mov> = self
                    .movs
                    .iter()
                    .filter(|m| no_ciclo(m) && self.nao_alocado(m) > 0)
                    .collect();
                self.tabela_nao_alocado(&v)
            }
            (Tipo::Ciclo, Some("nao-confirmadas")) => movs(&|m| !m.confirmada),
            (Tipo::Ciclo, Some(r @ ("vencidos" | "compromissos" | "ainda-vai-sair"))) => {
                let comp: Vec<Ref> = self
                    .compromissos_do_ciclo()
                    .into_iter()
                    .filter(|x| match r {
                        "vencidos" => x.1.as_str() < HOJE,
                        "ainda-vai-sair" => x.1.as_str() >= HOJE,
                        _ => true,
                    })
                    .map(|x| x.0)
                    .collect();
                self.tabela_de(None, &comp)
            }
            (Tipo::Ciclo, Some("disponivel")) => {
                let v: Vec<Ref> = self
                    .contas
                    .iter()
                    .map(|c| Ref::nova(Tipo::Conta, c.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Conta), &v)
            }
            (Tipo::Ciclo, Some("faturas-abertas")) => {
                let v: Vec<Ref> = self
                    .faturas
                    .iter()
                    .filter(|f| f.total > f.pago)
                    .map(|f| Ref::nova(Tipo::Fatura, f.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Fatura), &v)
            }
            (Tipo::Ciclo, Some("dividas")) => {
                let v: Vec<Ref> = self
                    .dividas
                    .iter()
                    .map(|d| Ref::nova(Tipo::Divida, d.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Divida), &v)
            }
            (Tipo::Ciclo, Some("recorrentes")) => {
                let v: Vec<Ref> = self
                    .recorrentes
                    .iter()
                    .filter(|r| r.ativa)
                    .map(|r| Ref::nova(Tipo::Recorrente, r.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Recorrente), &v)
            }
            (Tipo::Ciclo, Some("documentos-a-revisar")) => {
                let v: Vec<Ref> = self
                    .documentos
                    .iter()
                    .filter(|d| d.estado == "Precisa revisar")
                    .map(|d| Ref::nova(Tipo::Documento, d.id.clone()))
                    .collect();
                self.tabela_de(Some(Tipo::Documento), &v)
            }
            (Tipo::Fatura, Some("pagamento")) => movs(&|m| {
                m.tipo == TipoMov::PagamentoDeFatura && m.fatura.as_deref() == Some(&de.chave)
            }),
            (Tipo::Fatura, _) => movs(&|m| {
                m.fatura.as_deref() == Some(&de.chave) && m.tipo != TipoMov::PagamentoDeFatura
            }),
            (Tipo::Centro, Some(r)) => {
                let ids: BTreeSet<&str> = self
                    .rateios
                    .iter()
                    .filter(|x| x.centro == de.chave)
                    .map(|x| x.mov.as_str())
                    .collect();
                let quer = |m: &Mov| match r {
                    "entradas" => m.tipo.entra(),
                    "saidas" => !m.tipo.entra() && m.tipo != TipoMov::Ajuste,
                    _ => m.tipo != TipoMov::Ajuste,
                };
                let v: Vec<&Mov> = self
                    .movs
                    .iter()
                    .filter(|m| ids.contains(m.id.as_str()) && no_ciclo(m) && quer(m))
                    .collect();
                let mut t = Tabela {
                    colunas: vec![
                        col("Data"),
                        col("Movimentação"),
                        coln("Valor da movimentação"),
                        coln("Rateio para este centro"),
                        coln("Percentual"),
                    ],
                    linhas: Vec::new(),
                };
                for m in v {
                    let r = self
                        .rateios
                        .iter()
                        .filter(|x| x.centro == de.chave && x.mov == m.id)
                        .map(|x| x.valor)
                        .sum::<Centavos>();
                    let sinal = if m.tipo.entra() { 1 } else { -1 };
                    let pct = if m.valor != 0 {
                        r * 1000 / m.valor.abs()
                    } else {
                        0
                    };
                    let alvo = Ref::nova(Tipo::Movimentacao, m.id.clone());
                    t.linhas.push(Linha {
                        alvo: Some(alvo.clone()),
                        celulas: vec![
                            Celula::Data(m.data.clone()),
                            Celula::Link {
                                alvo,
                                texto: m.descricao.clone(),
                            },
                            valor(m.valor, None),
                            valor(sinal * r, None),
                            txt(format!("{},{}%", pct / 10, pct % 10)),
                        ],
                    });
                }
                t.linhas.sort_by(|a, b| {
                    formato::texto_da_celula(&b.celulas[0])
                        .cmp(&formato::texto_da_celula(&a.celulas[0]))
                });
                t
            }
            (Tipo::Compromisso, Some("origem")) => {
                let v: Vec<Ref> = self
                    .ligados_a(&de, Tipo::Recorrente)
                    .into_iter()
                    .chain(self.ligados_a(&de, Tipo::Contrato))
                    .collect();
                self.tabela_de(None, &v)
            }
            (Tipo::Compromisso | Tipo::Parcela, Some("pagamento")) => self.tabela_de(
                Some(Tipo::Movimentacao),
                &self.ligados_a(&de, Tipo::Movimentacao),
            ),
            (Tipo::Recorrente, Some("proximos")) => {
                let mut v: Vec<Ref> = self
                    .compromissos
                    .iter()
                    .filter(|c| c.recorrente == de.chave && c.mov.is_none())
                    .map(|c| Ref::nova(Tipo::Compromisso, c.id.clone()))
                    .collect();
                self.ordenar(Tipo::Compromisso, &mut v);
                self.tabela_de(Some(Tipo::Compromisso), &v)
            }
            (Tipo::Recorrente, Some("fornecedor-e-contrato")) => {
                let v: Vec<Ref> = self
                    .ligados_a(&de, Tipo::Fornecedor)
                    .into_iter()
                    .chain(self.ligados_a(&de, Tipo::Contrato))
                    .collect();
                self.tabela_de(None, &v)
            }
            (Tipo::Divida, Some("simulacao")) => self.simulacao(&de.chave),
            (Tipo::Cartao, Some("compromissos-e-dividas")) => {
                let v: Vec<Ref> = self
                    .ligados_a(&de, Tipo::Divida)
                    .into_iter()
                    .chain(self.ligados_a(&de, Tipo::Fatura).into_iter().filter(|f| {
                        self.faturas
                            .iter()
                            .any(|x| x.id == f.chave && x.total > x.pago)
                    }))
                    .collect();
                self.tabela_de(None, &v)
            }
            (_, Some("parcelas-e-dividas")) => {
                let v: Vec<Ref> = self.ligados_a(&de, Tipo::Divida);
                let mut todas = v.clone();
                for d in &v {
                    todas.extend(self.ligados_a(d, Tipo::Parcela));
                }
                self.tabela_de(None, &todas)
            }
            // Conta, cartão, fornecedor, categoria, tag…: as movimentações ligadas.
            _ => {
                let refs: Vec<Ref> = self
                    .ligados_a(&de, Tipo::Movimentacao)
                    .into_iter()
                    .filter(|r| {
                        self.mov(&r.chave)
                            .is_some_and(|m| no_ciclo(&m) && m.tipo != TipoMov::Ajuste)
                    })
                    .collect();
                self.tabela_de(Some(Tipo::Movimentacao), &refs)
            }
        })
    }

    /// SAC × Price sobre o saldo devedor de hoje, nas parcelas restantes.
    fn simulacao(&self, divida: &str) -> Tabela {
        let mut t = Tabela {
            colunas: vec![
                col("Sistema"),
                coln("Primeira parcela"),
                coln("Última parcela"),
                coln("Juros até o fim"),
                coln("Total a pagar"),
            ],
            linhas: Vec::new(),
        };
        let Some(d) = self.dividas.iter().find(|d| d.id == divida) else {
            return t;
        };
        if d.taxa_bp == 0 {
            return t;
        }
        let saldo = self.saldo_da_divida(divida);
        let n = self.restantes(divida).max(1) as u32;
        for sistema in ["SAC", "Price"] {
            let c = cronograma(sistema, saldo, d.taxa_bp, n);
            let juros: Centavos = c.iter().map(|x| x.1).sum();
            let totalp: Centavos = c.iter().map(|x| x.0).sum();
            t.linhas.push(Linha {
                alvo: None,
                celulas: vec![
                    txt(if sistema == d.sistema {
                        format!("{sistema} (o contratado)")
                    } else {
                        sistema.to_owned()
                    }),
                    valor(-c.first().map(|x| x.0).unwrap_or(0), None),
                    valor(-c.last().map(|x| x.0).unwrap_or(0), None),
                    valor(-juros, None),
                    valor(-totalp, None),
                ],
            });
        }
        t
    }

    // -----------------------------------------------------------------------
    // Página e rastro
    // -----------------------------------------------------------------------

    fn campo(rotulo: &str, v: Celula) -> sbf_inventario::consulta::Campo {
        sbf_inventario::consulta::Campo {
            rotulo: rotulo.to_owned(),
            valor: v,
        }
    }

    fn item(&self, r: &Ref) -> Option<Item> {
        let r = self.normalizar(r);
        let rotulo = self.rotulo(&r)?;
        let k = r.chave.as_str();
        let c = Self::campo;
        let campos = match r.tipo {
            Tipo::Movimentacao => {
                let m = self.mov(k)?;
                let mut v = vec![
                    c("Valor", valor(m.valor, None)),
                    c("Data", Celula::Data(m.data.clone())),
                    c("Tipo", txt(m.tipo.rotulo())),
                    c(
                        "Categoria",
                        match &m.categoria {
                            Some(x) => self.link(Tipo::Categoria, x),
                            None => txt("Sem categoria"),
                        },
                    ),
                    c("Fornecedor", self.link_opt(Tipo::Fornecedor, &m.fornecedor)),
                    c(
                        "Conta ou cartão",
                        match (&m.conta, &m.cartao) {
                            (_, Some(x)) => self.link(Tipo::Cartao, x),
                            (Some(x), None) => self.link(Tipo::Conta, x),
                            _ => Celula::Vazia,
                        },
                    ),
                    c("Fatura", self.link_opt(Tipo::Fatura, &m.fatura)),
                    c("Parcela", self.link_opt(Tipo::Parcela, &m.parcela)),
                    c("Ciclo", self.link(Tipo::Ciclo, &ciclo_de(&m.data))),
                    c(
                        "Situação",
                        estado(if self.aguardando_para(&m.id).is_some() {
                            "Alteração aguardando"
                        } else if m.confirmada {
                            "Confirmada"
                        } else {
                            "Prevista"
                        }),
                    ),
                ];
                if m.tipo.rateavel_como_saida() {
                    v.push(c(
                        "Não alocado",
                        valor(-self.nao_alocado(m), total(r.clone(), None, None)),
                    ));
                }
                if !m.tags.is_empty() {
                    v.push(c(
                        "Tags",
                        Celula::Links(
                            m.tags
                                .iter()
                                .filter_map(|t| {
                                    let x = Ref::nova(Tipo::Tag, t.clone());
                                    self.rotulo(&x).map(|n| (x, n))
                                })
                                .collect(),
                        ),
                    ));
                }
                v
            }
            Tipo::Conta | Tipo::Produto => {
                let x = self
                    .contas
                    .iter()
                    .chain(self.produtos.iter())
                    .find(|x| x.id == k)?;
                let movs = self.ligados_a(&r, Tipo::Movimentacao);
                vec![
                    c(
                        "Saldo",
                        valor(x.saldo, ligados(r.clone(), Tipo::Movimentacao)),
                    ),
                    c("Instituição", self.link(Tipo::Instituicao, &x.instituicao)),
                    c("Tipo", txt(x.tipo)),
                    c("Atualizado", Celula::Data(x.atualizado.clone())),
                    c("Origem do saldo", txt(x.origem)),
                    c(
                        "Movimentações",
                        contagem(movs.len(), ligados(r.clone(), Tipo::Movimentacao)),
                    ),
                ]
            }
            Tipo::Cartao => {
                let x = self.cartoes.iter().find(|x| x.id == k)?;
                let aberta: Centavos = self
                    .faturas
                    .iter()
                    .filter(|f| f.cartao == x.id)
                    .map(|f| (f.total - f.pago).max(0))
                    .sum();
                vec![
                    c(
                        "Fatura atual",
                        valor(
                            -aberta,
                            total(r.clone(), None, Some("compromissos-e-dividas")),
                        ),
                    ),
                    c("Limite", valor(x.limite, None)),
                    c("Limite livre", valor(x.limite - aberta, None)),
                    c("Instituição", self.link(Tipo::Instituicao, &x.instituicao)),
                    c("Paga pela", self.link(Tipo::Conta, &x.conta_de_pagamento)),
                    c(
                        "Fecha e vence",
                        txt(format!("dia {} · dia {}", x.fecha_dia, x.vence_dia)),
                    ),
                ]
            }
            Tipo::Fatura => {
                let f = self.faturas.iter().find(|f| f.id == k)?;
                vec![
                    c("Total", valor(-f.total, total(r.clone(), None, None))),
                    c(
                        "Pago",
                        valor(f.pago, total(r.clone(), None, Some("pagamento"))),
                    ),
                    c("Saldo", valor(-(f.total - f.pago), None)),
                    c("Cartão", self.link(Tipo::Cartao, &f.cartao)),
                    c("Fecha", Celula::Data(f.fecha.clone())),
                    c("Vence", Celula::Data(f.vence.clone())),
                    c(
                        "Situação",
                        estado(if f.pago >= f.total {
                            "Paga"
                        } else if f.fecha.as_str() > HOJE {
                            "Aberta"
                        } else {
                            "Fechada"
                        }),
                    ),
                ]
            }
            Tipo::Instituicao => {
                let n = |t| self.ligados_a(&r, t).len();
                vec![
                    c(
                        "Contas",
                        contagem(n(Tipo::Conta), ligados(r.clone(), Tipo::Conta)),
                    ),
                    c(
                        "Cartões",
                        contagem(n(Tipo::Cartao), ligados(r.clone(), Tipo::Cartao)),
                    ),
                    c(
                        "Dívidas",
                        contagem(n(Tipo::Divida), ligados(r.clone(), Tipo::Divida)),
                    ),
                ]
            }
            Tipo::Categoria | Tipo::Tag => {
                let movs = self.ligados_a(&r, Tipo::Movimentacao);
                let soma: Centavos = movs
                    .iter()
                    .filter_map(|m| self.mov(&m.chave))
                    .filter(|m| ciclo_de(&m.data) == CICLO_ATUAL)
                    .map(|m| m.valor)
                    .sum();
                vec![
                    c(
                        "No ciclo atual",
                        valor(soma, total(r.clone(), Some(CICLO_ATUAL), None)),
                    ),
                    c(
                        "Movimentações",
                        contagem(movs.len(), ligados(r.clone(), Tipo::Movimentacao)),
                    ),
                ]
            }
            Tipo::Centro => {
                let (e, s) = self.saldo_do_centro(k);
                vec![
                    c(
                        "Saldo",
                        valor(e - s, total(r.clone(), None, Some("rateios"))),
                    ),
                    c("Entrou", valor(e, total(r.clone(), None, Some("entradas")))),
                    c("Saiu", valor(-s, total(r.clone(), None, Some("saidas")))),
                ]
            }
            Tipo::Compromisso => {
                let x = self.compromissos.iter().find(|x| x.id == k)?;
                vec![
                    c(
                        "Valor",
                        match x.valor {
                            Some(v) => valor(-v, None),
                            None => txt("sem valor esperado"),
                        },
                    ),
                    c("Vence", Celula::Data(x.vence.clone())),
                    c("Recorrente", self.link(Tipo::Recorrente, &x.recorrente)),
                    c("Conta", self.link_opt(Tipo::Conta, &x.conta)),
                    c("Pagamento", self.link_opt(Tipo::Movimentacao, &x.mov)),
                ]
            }
            Tipo::Recorrente => {
                let x = self.recorrentes.iter().find(|x| x.id == k)?;
                vec![
                    c(
                        "Por mês",
                        match Self::equivalente_mensal(x) {
                            Some(v) => valor(-v, total(r.clone(), None, Some("proximos"))),
                            None => txt("estimativa: sem valor esperado"),
                        },
                    ),
                    c("Frequência", txt(x.frequencia)),
                    c("Fornecedor", self.link_opt(Tipo::Fornecedor, &x.fornecedor)),
                    c("Categoria", self.link_opt(Tipo::Categoria, &x.categoria)),
                    c("Contrato", self.link_opt(Tipo::Contrato, &x.contrato)),
                    c(
                        "Situação",
                        estado(if x.ativa { "Ativa" } else { "Pausada" }),
                    ),
                ]
            }
            Tipo::Divida => {
                let d = self.dividas.iter().find(|d| d.id == k)?;
                let prox = self.proxima_parcela(&d.id);
                vec![
                    c(
                        "Saldo devedor",
                        valor(
                            -self.saldo_da_divida(&d.id),
                            ligados(r.clone(), Tipo::Parcela),
                        ),
                    ),
                    c(
                        "Próxima parcela",
                        prox.map_or(Celula::Vazia, |p| self.link(Tipo::Parcela, &p.id)),
                    ),
                    c("Sistema", txt(d.sistema)),
                    c(
                        "Taxa",
                        txt(if d.taxa_bp > 0 {
                            percentual(d.taxa_bp)
                        } else {
                            "sem juros".to_owned()
                        }),
                    ),
                    c("Credor", self.link(Tipo::Instituicao, &d.credor)),
                    c("Contrato", self.link_opt(Tipo::Contrato, &d.contrato)),
                    c(
                        "Restam",
                        contagem(
                            self.restantes(&d.id) as usize,
                            ligados(r.clone(), Tipo::Parcela),
                        ),
                    ),
                ]
            }
            Tipo::Parcela => {
                let p = self.parcelas.iter().find(|p| p.id == k)?;
                vec![
                    c("Valor", valor(-p.valor, None)),
                    c("Juros", valor(-p.juros, None)),
                    c("Amortização", valor(-p.amortizacao, None)),
                    c("Vence", Celula::Data(p.vence.clone())),
                    c("Dívida", self.link(Tipo::Divida, &p.divida)),
                    c("Pagamento", self.link_opt(Tipo::Movimentacao, &p.mov)),
                    c("Situação", estado(if p.paga { "Paga" } else { "A vencer" })),
                ]
            }
            Tipo::Fornecedor => {
                let f = self.fornecedor(k)?;
                let movs = self.ligados_a(&r, Tipo::Movimentacao);
                let soma: Centavos = movs
                    .iter()
                    .filter_map(|m| self.mov(&m.chave))
                    .map(|m| m.valor)
                    .sum();
                vec![
                    c(
                        "Total no livro-caixa",
                        valor(soma, total(r.clone(), None, None)),
                    ),
                    c(
                        "Movimentações",
                        contagem(movs.len(), ligados(r.clone(), Tipo::Movimentacao)),
                    ),
                    c("Categoria", self.link_opt(Tipo::Categoria, &f.categoria)),
                    c("Apelidos", txt(f.apelidos.join(" · "))),
                ]
            }
            Tipo::Contrato => {
                let x = self.contratos.iter().find(|x| x.id == k)?;
                vec![
                    c("Início", Celula::Data(x.inicio.clone())),
                    c("Fim", x.fim.clone().map_or(Celula::Vazia, Celula::Data)),
                    c("Fornecedor", self.link_opt(Tipo::Fornecedor, &x.fornecedor)),
                    c(
                        "Instituição",
                        self.link_opt(Tipo::Instituicao, &x.instituicao),
                    ),
                ]
            }
            Tipo::Transferencia => {
                let t = self.transferencias.iter().find(|t| t.id == k)?;
                vec![
                    c("Valor", valor(t.valor, None)),
                    c("Data", Celula::Data(t.data.clone())),
                    c("De", self.link(Tipo::Conta, &t.de)),
                    c("Para", self.link(Tipo::Conta, &t.para)),
                ]
            }
            Tipo::Documento => {
                let d = self.documentos.iter().find(|d| d.id == k)?;
                vec![
                    c("Tipo", txt(d.tipo)),
                    c("Data", Celula::Data(d.data.clone())),
                    c("Situação", estado(d.estado)),
                    c(
                        "Ligado a",
                        Celula::Links(
                            d.ligado_a
                                .iter()
                                .filter_map(|x| self.rotulo(x).map(|n| (x.clone(), n)))
                                .collect(),
                        ),
                    ),
                ]
            }
            Tipo::Ciclo => {
                let k = k.to_owned();
                vec![
                    c(
                        "Resultado do mês",
                        valor(
                            self.entrou(&k) + self.saiu(&k),
                            total(r.clone(), Some(&k), Some("resultado")),
                        ),
                    ),
                    c(
                        "Entrou",
                        valor(self.entrou(&k), total(r.clone(), Some(&k), Some("entrou"))),
                    ),
                    c(
                        "Saiu",
                        valor(self.saiu(&k), total(r.clone(), Some(&k), Some("saiu"))),
                    ),
                    c(
                        "No cartão",
                        valor(
                            self.no_cartao(&k),
                            total(r.clone(), Some(&k), Some("no-cartao")),
                        ),
                    ),
                ]
            }
            _ => return None,
        };
        Some(Item {
            alvo: r,
            rotulo,
            campos,
        })
    }

    fn rastro(&self, r: &Ref) -> Rastro {
        let r = self.normalizar(r);
        let elo = |alvo: Ref, papel: &str| {
            self.rotulo(&alvo)
                .map(|rotulo| sbf_inventario::consulta::Elo {
                    alvo,
                    rotulo,
                    papel: papel.to_owned(),
                    quem: None,
                    quando: None,
                })
        };
        let mut rastro = Rastro::default();
        match r.tipo {
            Tipo::Movimentacao => {
                let Some(m) = self.mov(&r.chave) else {
                    return rastro;
                };
                for d in self.documentos.iter().filter(|d| d.ligado_a.contains(&r)) {
                    rastro.acima.extend(elo(
                        Ref::nova(Tipo::Documento, d.id.clone()),
                        "documento no servidor",
                    ));
                }
                if let Some(f) = &m.fatura {
                    rastro
                        .acima
                        .extend(elo(Ref::nova(Tipo::Fatura, f.clone()), "na fatura"));
                }
                if let Some(c) = &m.cartao {
                    rastro
                        .acima
                        .extend(elo(Ref::nova(Tipo::Cartao, c.clone()), "no cartão"));
                }
                if let Some(c) = &m.conta {
                    rastro
                        .acima
                        .extend(elo(Ref::nova(Tipo::Conta, c.clone()), "na conta"));
                }
                if let Some(p) = &m.parcela {
                    rastro
                        .acima
                        .extend(elo(Ref::nova(Tipo::Parcela, p.clone()), "paga a parcela"));
                }
                for x in self.rateios.iter().filter(|x| x.mov == m.id) {
                    let pct = if m.valor != 0 {
                        x.valor * 100 / m.valor.abs()
                    } else {
                        0
                    };
                    rastro.abaixo.extend(elo(
                        Ref::nova(Tipo::Centro, x.centro.clone()),
                        &format!("rateio de {} ({pct}%)", formato::dinheiro(x.valor)),
                    ));
                }
                let n = self.nao_alocado(m);
                if n > 0 {
                    rastro.abaixo.extend(elo(
                        Ref::nova(Tipo::Ciclo, ciclo_de(&m.data)),
                        &format!("não alocado: {}", formato::dinheiro(n)),
                    ));
                }
            }
            _ => {
                if let Some(viz) = self.arestas.get(&r) {
                    for v in viz.iter().filter(|v| v.tipo != Tipo::Movimentacao).take(12) {
                        rastro.acima.extend(elo(v.clone(), "ligado a"));
                    }
                }
            }
        }
        rastro
    }

    fn por_ciclo(&self, alvo: &Ref, ciclos: usize) -> Vec<TotalDoCiclo> {
        let alvo = self.normalizar(alvo);
        let ids: BTreeSet<String> = self
            .ligados_a(&alvo, Tipo::Movimentacao)
            .into_iter()
            .map(|r| r.chave)
            .collect();
        let inicio = CICLOS.len().saturating_sub(ciclos);
        CICLOS[inicio..]
            .iter()
            .map(|c| TotalDoCiclo {
                ciclo: (*c).to_owned(),
                centavos: self
                    .movs_do_ciclo(c)
                    .filter(|m| ids.contains(&m.id) && m.tipo != TipoMov::Ajuste)
                    .map(|m| m.valor)
                    .sum(),
                itens: total(alvo.clone(), Some(c), None).map(Box::new),
            })
            .collect()
    }

    /// As perguntas do §11 sobre o livro-caixa: gasto pela data da compra, sem pagamento de
    /// fatura nem transferências (grill Q18). Sem "a conferir".
    fn relatorio(&self, pergunta: Pergunta, filtro: &Filtro) -> Painel {
        let gasto = |m: &Mov| {
            m.confirmada
                && m.valor < 0
                && matches!(
                    m.tipo,
                    TipoMov::Despesa | TipoMov::Tarifa | TipoMov::PagamentoDeDivida
                )
        };
        let (de, ate) = filtro
            .ciclos
            .clone()
            .unwrap_or_else(|| (CICLOS[0].to_owned(), CICLO_ATUAL.to_owned()));
        let gastos: Vec<&Mov> = self
            .movs
            .iter()
            .filter(|m| {
                let c = ciclo_de(&m.data);
                gasto(m) && c >= de && c <= ate
            })
            .collect();
        let nota = Some(
            "livro-caixa · gasto pela data da compra, sem pagamento de fatura nem transferências"
                .to_owned(),
        );
        let agrupar = |chave: &dyn Fn(&Mov) -> Option<Ref>| {
            let mut m: BTreeMap<Option<Ref>, (Centavos, usize)> = BTreeMap::new();
            for g in &gastos {
                let e = m.entry(chave(g)).or_insert((0, 0));
                e.0 += g.valor;
                e.1 += 1;
            }
            let mut v: Vec<(Option<Ref>, (Centavos, usize))> = m.into_iter().collect();
            v.sort_by_key(|x| x.1.0);
            v
        };
        let tabela_agrupada = |rotulo: &str, v: Vec<(Option<Ref>, (Centavos, usize))>| Tabela {
            colunas: vec![col(rotulo), coln("Vezes"), coln("Total")],
            linhas: v
                .into_iter()
                .map(|(r, (soma, n))| Linha {
                    alvo: r.clone(),
                    celulas: vec![
                        match &r {
                            Some(r) => self.link(r.tipo, &r.chave),
                            None => txt(format!("Sem {}", rotulo.to_lowercase())),
                        },
                        txt(n.to_string()),
                        valor(
                            soma,
                            r.and_then(|r| total(r, (de == ate).then_some(de.as_str()), None)),
                        ),
                    ],
                })
                .collect(),
        };
        let secao = |titulo: &str, tabela: Tabela| Painel {
            secoes: vec![Secao {
                titulo: titulo.to_owned(),
                nota: nota.clone(),
                tabela,
            }],
            avisos: Vec::new(),
        };
        match pergunta {
            Pergunta::QuantoSaiu => {
                let mut t = Tabela {
                    colunas: vec![col("Ciclo"), coln("Gastos"), coln("Total")],
                    linhas: Vec::new(),
                };
                for c in CICLOS.iter().rev().filter(|c| **c >= de.as_str() && **c <= ate.as_str()) {
                    let v: Vec<&&Mov> = gastos.iter().filter(|m| ciclo_de(&m.data) == *c).collect();
                    t.linhas.push(Linha {
                        alvo: Some(ciclo_ref(c)),
                        celulas: vec![
                            self.link(Tipo::Ciclo, c),
                            txt(v.len().to_string()),
                            valor(v.iter().map(|m| m.valor).sum(), total(ciclo_ref(c), Some(c), Some("gastos"))),
                        ],
                    });
                }
                secao("Quanto saiu, por ciclo", t)
            }
            Pergunta::MaioresFornecedores => secao(
                "Maiores fornecedores",
                tabela_agrupada(
                    "Fornecedor",
                    agrupar(&|m| m.fornecedor.clone().map(|f| Ref::nova(Tipo::Fornecedor, f))),
                ),
            ),
            Pergunta::OndeSeConcentra => secao(
                "Por categoria",
                tabela_agrupada(
                    "Categoria",
                    agrupar(&|m| m.categoria.clone().map(|c| Ref::nova(Tipo::Categoria, c))),
                ),
            ),
            Pergunta::OQueSeRepete => {
                let v: Vec<Ref> = self
                    .recorrentes
                    .iter()
                    .map(|r| Ref::nova(Tipo::Recorrente, r.id.clone()))
                    .collect();
                secao("Recorrentes do livro-caixa", self.tabela_de(Some(Tipo::Recorrente), &v))
            }
            Pergunta::OQueAumentou => {
                let atual = ate.clone();
                let anterior = mes_anterior(&atual);
                let soma = |c: &str, f: &str| -> Centavos {
                    gastos
                        .iter()
                        .filter(|m| ciclo_de(&m.data) == c && m.fornecedor.as_deref() == Some(f))
                        .map(|m| m.valor)
                        .sum()
                };
                let mut t = Tabela {
                    colunas: vec![col("Fornecedor"), coln("Antes"), coln("Agora")],
                    linhas: Vec::new(),
                };
                for f in &self.fornecedores {
                    let (a, b) = (soma(&anterior, &f.id), soma(&atual, &f.id));
                    if a != 0 && b < a {
                        let r = Ref::nova(Tipo::Fornecedor, f.id.clone());
                        t.linhas.push(Linha {
                            alvo: Some(r.clone()),
                            celulas: vec![
                                self.link(Tipo::Fornecedor, &f.id),
                                valor(a, total(r.clone(), Some(&anterior), None)),
                                valor(b, total(r, Some(&atual), None)),
                            ],
                        });
                    }
                }
                secao(&format!("De {anterior} para {atual}"), t)
            }
            Pergunta::NaoClassificado => {
                let refs = Self::refs_de_movs(self.movs.iter().filter(|m| {
                    let c = ciclo_de(&m.data);
                    c >= de
                        && c <= ate
                        && m.tipo != TipoMov::PagamentoDeFatura
                        && (m.categoria.is_none() || self.nao_alocado(m) > 0)
                }));
                secao(
                    "Movimentações sem categoria ou não alocadas",
                    self.tabela_de(Some(Tipo::Movimentacao), &refs),
                )
            }
            Pergunta::PossiveisDuplicidades | Pergunta::OQueFaltaParaConfiar => Painel {
                secoes: Vec::new(),
                avisos: vec![
                    "Esta pergunta só se aplica ao inventário: o livro-caixa já passou pela conferência. Troque a fonte para Inventário."
                        .to_owned(),
                ],
            },
        }
    }

    // -----------------------------------------------------------------------
    // Alterações e sincronização
    // -----------------------------------------------------------------------

    fn agora(&mut self) -> String {
        self.proxima_troca_minuto += 1;
        let m = 15 + self.proxima_troca_minuto;
        format!("2026-11-13T{:02}:{:02}:00-03:00", 8 + m / 60, m % 60)
    }

    fn propor(&mut self, pedido: &PedidoDeAlteracao) -> ResultadoDeAlteracao<Alteracao> {
        if self.rede == EstadoDaRede::EstacaoRevogada {
            return Err(ErroDeAlteracao::EstacaoRevogada);
        }
        let PedidoDeAlteracao::MudarCategoria {
            movimentacao,
            categoria,
        } = pedido;
        let m = self
            .mov(&movimentacao.chave)
            .cloned()
            .ok_or_else(|| ErroDeAlteracao::NaoEncontrado(movimentacao.clone()))?;
        let atual = m.categoria.clone();
        let nova = categoria.as_ref().map(|c| c.chave.clone());
        if atual == nova {
            return Err(ErroDeAlteracao::NadaMuda);
        }
        let nome = |c: &Option<String>| {
            c.as_ref()
                .and_then(|c| Self::nome_de(&self.categorias, c))
                .unwrap_or_else(|| "Sem categoria".to_owned())
        };
        let (antes, depois) = (nome(&atual), nome(&nova));
        let id = self.proxima_alteracao.to_string();
        self.proxima_alteracao += 1;
        let feita_em = self.agora();
        let esta = self
            .estacoes
            .iter()
            .find(|e| e.esta)
            .map(|e| e.alvo.clone())
            .unwrap_or_else(|| Ref::nova(Tipo::Estacao, "esta"));
        let a = Alteracao {
            id,
            pedido: pedido.clone(),
            rotulo_do_alvo: format!("{} · {}", m.descricao, dia_mes(&m.data)),
            area: "Movimentações".to_owned(),
            antes,
            depois,
            feita_em,
            estacao: esta,
            estado: EstadoDaAlteracao::Aguardando,
            versoes_de_base: vec![(movimentacao.clone(), m.versao.clone())],
            recusa: None,
            refeita_em: None,
        };
        self.alteracoes.push(a.clone());
        Ok(a)
    }

    fn achar(&mut self, id: &str) -> ResultadoDeAlteracao<&mut Alteracao> {
        self.alteracoes
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| ErroDeAlteracao::AlteracaoInexistente(id.to_owned()))
    }

    fn refazer(&mut self, id: &str) -> ResultadoDeAlteracao<Alteracao> {
        let a = self.achar(id)?;
        if a.estado != EstadoDaAlteracao::Recusada {
            return Err(ErroDeAlteracao::NaoEstaRecusada {
                id: id.to_owned(),
                estado: a.estado,
            });
        }
        let pedido = a.pedido.clone();
        // A nova parte da versão que está no servidor agora (a da cópia local).
        self.mudadas_no_servidor.remove(&pedido.alvo().chave);
        let nova = self.propor(&pedido)?;
        let a = self.achar(id)?;
        a.estado = EstadoDaAlteracao::Refeita;
        a.refeita_em = Some(nova.id.clone());
        Ok(nova)
    }

    fn descartar(&mut self, id: &str) -> ResultadoDeAlteracao<()> {
        let a = self.achar(id)?;
        if a.estado != EstadoDaAlteracao::Recusada {
            return Err(ErroDeAlteracao::NaoEstaRecusada {
                id: id.to_owned(),
                estado: a.estado,
            });
        }
        a.estado = EstadoDaAlteracao::Descartada;
        Ok(())
    }

    /// A troca fictícia: envia as que aguardam; aceita, ou recusa as que tocam uma
    /// movimentação que mudou no servidor.
    fn sincronizar(&mut self) {
        let inicio = self.agora();
        match self.rede {
            EstadoDaRede::SemRede => {
                self.trocas.insert(
                    0,
                    Troca {
                        inicio,
                        fim: None,
                        colecoes: Vec::new(),
                        linhas: 0,
                        enviadas: 0,
                        aceitas: 0,
                        recusadas: 0,
                        erro: Some("Sem rede: o servidor não respondeu.".to_owned()),
                    },
                );
                return;
            }
            EstadoDaRede::EstacaoRevogada
            | EstadoDaRede::NaoPareada
            | EstadoDaRede::AtualizeOApp => {
                return;
            }
            _ => {}
        }
        let (mut enviadas, mut aceitas, mut recusadas) = (0, 0, 0);
        let ids: Vec<String> = self
            .alteracoes
            .iter()
            .filter(|a| a.estado == EstadoDaAlteracao::Aguardando)
            .map(|a| a.id.clone())
            .collect();
        for id in ids {
            enviadas += 1;
            let Some(i) = self.alteracoes.iter().position(|a| a.id == id) else {
                continue;
            };
            let a = self.alteracoes[i].clone();
            let PedidoDeAlteracao::MudarCategoria {
                movimentacao,
                categoria,
            } = &a.pedido;
            if let Some((v, versao)) = self.mudadas_no_servidor.get(&movimentacao.chave).cloned() {
                recusadas += 1;
                let m = self.mov(&movimentacao.chave).cloned();
                let cat = |c: &Option<String>| {
                    c.as_ref()
                        .and_then(|c| Self::nome_de(&self.categorias, c))
                        .unwrap_or_else(|| "Sem categoria".to_owned())
                };
                let campos = m
                    .map(|m| {
                        vec![
                            CampoLadoALado::novo("Categoria", a.depois.clone(), cat(&m.categoria)),
                            CampoLadoALado::novo(
                                "Valor",
                                formato::dinheiro(m.valor),
                                formato::dinheiro(v),
                            ),
                            CampoLadoALado::novo(
                                "Data",
                                formato::data(&m.data),
                                formato::data(&m.data),
                            ),
                        ]
                    })
                    .unwrap_or_default();
                if let Some(x) = self.movs.iter_mut().find(|x| x.id == movimentacao.chave) {
                    x.valor = v;
                    x.versao = versao;
                }
                let quando = self.agora();
                let a = &mut self.alteracoes[i];
                a.estado = EstadoDaAlteracao::Recusada;
                a.recusa = Some(Recusa {
                    motivo: "A movimentação mudou no servidor depois que você alterou.".to_owned(),
                    quando,
                    campos,
                });
            } else {
                aceitas += 1;
                let quando = self.agora();
                if let Some(x) = self.movs.iter_mut().find(|x| x.id == movimentacao.chave) {
                    x.categoria = categoria.as_ref().map(|c| c.chave.clone());
                    x.versao = quando;
                }
                self.alteracoes[i].estado = EstadoDaAlteracao::Aceita;
            }
        }
        let fim = self.agora();
        self.ultima_troca = Some(fim.clone());
        self.copia_de = Some(fim.clone());
        if let Some(e) = self.estacoes.iter_mut().find(|e| e.esta) {
            e.ultima_sincronizacao = Some(fim.clone());
        }
        if !matches!(self.rede, EstadoDaRede::RegrasMaisNovas { .. }) {
            self.rede = EstadoDaRede::Sincronizado;
        }
        self.trocas.insert(
            0,
            Troca {
                inicio,
                fim: Some(fim),
                colecoes: if enviadas > 0 {
                    vec!["movimentacoes".to_owned()]
                } else {
                    Vec::new()
                },
                linhas: if enviadas > 0 {
                    self.movs.len() as i64
                } else {
                    0
                },
                enviadas,
                aceitas,
                recusadas,
                erro: None,
            },
        );
        self.indexar();
    }
}

// ---------------------------------------------------------------------------
// As costuras
// ---------------------------------------------------------------------------

/// O livro-caixa fictício. Clonar compartilha o mesmo estado (a leitura e a estação).
#[derive(Debug, Clone)]
pub struct LivroFicticio(Rc<RefCell<Livro>>);

/// Um livro-caixa fictício novo.
pub fn novo() -> LivroFicticio {
    LivroFicticio(Rc::new(RefCell::new(Livro::ficticio())))
}

impl LivroFicticio {
    /// Muda o estado da rede (demonstração e testes: offline, revogada, regras novas).
    pub fn definir_rede(&self, rede: EstadoDaRede) {
        self.0.borrow_mut().rede = rede;
    }

    /// Faz a movimentação "mudar no servidor": a próxima alteração sobre ela é recusada.
    pub fn mudar_no_servidor(&self, movimentacao: &Ref, valor: Centavos) {
        self.0.borrow_mut().mudadas_no_servidor.insert(
            movimentacao.chave.clone(),
            (valor, "2026-11-13T09:00:00Z".to_owned()),
        );
    }

    /// A categoria atual de uma movimentação (para os testes).
    pub fn categoria_de(&self, movimentacao: &Ref) -> Option<String> {
        let l = self.0.borrow();
        let m = l.mov(&movimentacao.chave)?;
        m.categoria
            .as_ref()
            .and_then(|c| Livro::nome_de(&l.categorias, c))
    }

    /// Os números da Hoje, para conferir as fórmulas nos testes:
    /// (disponível agora, compromissos, resultado do mês, no cartão).
    pub fn numeros_da_hoje(&self) -> (Centavos, Centavos, Centavos, Centavos) {
        let l = self.0.borrow();
        let comp: Centavos = l.compromissos_do_ciclo().iter().map(|x| x.3).sum();
        (
            l.disponivel_agora(),
            comp,
            l.entrou(CICLO_ATUAL) + l.saiu(CICLO_ATUAL),
            l.no_cartao(CICLO_ATUAL),
        )
    }
}

impl Consulta for LivroFicticio {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        let l = self.0.borrow();
        match pedido {
            PedidoDeTabela::Area { area, filtro } => l.area(*area, filtro),
            PedidoDeTabela::Ligados { de, tipo } => Ok(l.tabela_ligados(de, *tipo)),
            PedidoDeTabela::ItensDoTotal {
                de,
                fonte: Fonte::LivroCaixa,
                ciclo,
                recorte,
            } => {
                let recorte = match recorte.as_deref() {
                    // "Quanto saiu" do relatório: o gasto do ciclo (Q18).
                    Some("gastos") => {
                        let c = ciclo.clone().unwrap_or_else(|| CICLO_ATUAL.to_owned());
                        let refs = Livro::refs_de_movs(l.movs.iter().filter(|m| {
                            ciclo_de(&m.data) == c
                                && m.confirmada
                                && m.valor < 0
                                && matches!(
                                    m.tipo,
                                    TipoMov::Despesa | TipoMov::Tarifa | TipoMov::PagamentoDeDivida
                                )
                        }));
                        return Ok(l.tabela_de(Some(Tipo::Movimentacao), &refs));
                    }
                    outro => outro,
                };
                l.itens_do_total(de, ciclo.as_deref(), recorte)
            }
            PedidoDeTabela::ItensDoTotal { .. } => Err(ErroDeConsulta::NaoSuportado(
                "números do inventário não são do livro-caixa".to_owned(),
            )),
        }
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        Ok(self.0.borrow().item(alvo))
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        Ok(self.0.borrow().rastro(alvo))
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        if fonte != Fonte::LivroCaixa {
            return Ok(Vec::new());
        }
        Ok(self.0.borrow().por_ciclo(alvo, ciclos))
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        Err(ErroDeConsulta::NaoSuportado(
            "a Visão geral é do inventário".to_owned(),
        ))
    }

    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        if fonte != Fonte::LivroCaixa {
            return Err(ErroDeConsulta::NaoSuportado(
                "o livro-caixa não responde pelo inventário".to_owned(),
            ));
        }
        Ok(self.0.borrow().relatorio(pergunta, filtro))
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        Ok(Contagens::default())
    }

    fn texto(&self, _achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        Ok(None)
    }

    fn painel_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        self.0.borrow().painel(area, filtro)
    }
}

impl Alteracoes for LivroFicticio {
    fn propor(&mut self, pedido: &PedidoDeAlteracao) -> ResultadoDeAlteracao<Alteracao> {
        let mut l = self.0.borrow_mut();
        let a = l.propor(pedido)?;
        l.indexar();
        Ok(a)
    }

    fn alteracoes(&self, so_em_aberto: bool) -> ResultadoDeAlteracao<Vec<Alteracao>> {
        let l = self.0.borrow();
        let mut v: Vec<Alteracao> = l
            .alteracoes
            .iter()
            .filter(|a| !so_em_aberto || a.estado.em_aberto())
            .cloned()
            .collect();
        v.sort_by(|a, b| b.feita_em.cmp(&a.feita_em));
        Ok(v)
    }

    fn alteracao(&self, id: &str) -> ResultadoDeAlteracao<Option<Alteracao>> {
        Ok(self
            .0
            .borrow()
            .alteracoes
            .iter()
            .find(|a| a.id == id)
            .cloned())
    }

    fn refazer(&mut self, id: &str) -> ResultadoDeAlteracao<Alteracao> {
        self.0.borrow_mut().refazer(id)
    }

    fn descartar(&mut self, id: &str) -> ResultadoDeAlteracao<()> {
        self.0.borrow_mut().descartar(id)
    }

    fn categorias(&self) -> ResultadoDeAlteracao<Vec<OpcaoDeCategoria>> {
        let l = self.0.borrow();
        let mut v: Vec<OpcaoDeCategoria> = l
            .categorias
            .iter()
            .map(|c| OpcaoDeCategoria {
                alvo: Ref::nova(Tipo::Categoria, c.id.clone()),
                nome: c.nome.clone(),
                grupo: Some(c.extra.clone()),
            })
            .collect();
        v.sort_by(|a, b| a.nome.cmp(&b.nome));
        Ok(v)
    }
}

impl Sincronizacao for LivroFicticio {
    fn estado(&self) -> EstadoDaSincronizacao {
        let l = self.0.borrow();
        EstadoDaSincronizacao {
            esta_estacao: l.estacoes.iter().find(|e| e.esta).cloned(),
            servidor: "servidor na VPS de São Paulo (demonstração)".to_owned(),
            ultima_troca: l.ultima_troca.clone(),
            copia_de: l.copia_de.clone(),
            rede: l.rede.clone(),
            versao_das_regras: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    fn sincronizar_agora(&mut self) {
        self.0.borrow_mut().sincronizar();
    }

    fn estacoes(&self) -> ResultadoDeAlteracao<Vec<Estacao>> {
        let l = self.0.borrow();
        let mut v = l.estacoes.clone();
        v.sort_by_key(|e| !e.esta);
        Ok(v)
    }

    fn revogar(&mut self, estacao: &Ref) -> ResultadoDeAlteracao<()> {
        let mut l = self.0.borrow_mut();
        let e = l
            .estacoes
            .iter_mut()
            .find(|e| &e.alvo == estacao)
            .ok_or_else(|| ErroDeAlteracao::NaoEncontrado(estacao.clone()))?;
        e.credencial = Credencial::Revogada;
        if e.esta {
            l.rede = EstadoDaRede::EstacaoRevogada;
        }
        Ok(())
    }

    fn renomear(&mut self, estacao: &Ref, nome: &str) -> ResultadoDeAlteracao<()> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(ErroDeAlteracao::Local(
                "o nome não pode ficar vazio".to_owned(),
            ));
        }
        let mut l = self.0.borrow_mut();
        let e = l
            .estacoes
            .iter_mut()
            .find(|e| &e.alvo == estacao)
            .ok_or_else(|| ErroDeAlteracao::NaoEncontrado(estacao.clone()))?;
        e.nome = nome.to_owned();
        Ok(())
    }

    fn historico(&self) -> ResultadoDeAlteracao<Vec<Troca>> {
        Ok(self.0.borrow().trocas.clone())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn price_fecha_o_saldo_em_zero_e_sac_amortiza_igual() {
        let p = cronograma("Price", 1_000_000, 149, 24);
        assert_eq!(p.last().map(|x| x.3), Some(0));
        let primeira = p[0].0;
        assert!(p[..23].iter().all(|x| x.0 == primeira), "prestação fixa");
        let s = cronograma("SAC", 1_200_000, 199, 24);
        assert_eq!(s.last().map(|x| x.3), Some(0));
        assert!(s[0].0 > s[23].0, "SAC decresce");
    }

    #[test]
    fn hoje_segue_as_formulas() {
        let l = novo();
        let (disp, comp, _resultado, cartao) = l.numeros_da_hoje();
        assert_eq!(disp, 481_237 + 1_230_000 + 120_590);
        assert!(comp > 0);
        assert!(cartao < 0, "compras no cartão saem");
    }

    #[test]
    fn ajuste_fica_fora_do_centro() {
        let l = novo();
        let livro = l.0.borrow();
        // O ajuste de +12,34 não tem rateio e não entra no resultado.
        let ajuste = livro
            .movs
            .iter()
            .find(|m| m.tipo == TipoMov::Ajuste)
            .cloned()
            .expect("ajuste");
        assert_eq!(livro.nao_alocado(&ajuste), 0);
        let c = ciclo_de(&ajuste.data);
        let sem_ajuste: Centavos = livro
            .movs_do_ciclo(&c)
            .filter(|m| m.confirmada && m.tipo.entra() && m.cartao.is_none())
            .map(|m| m.valor)
            .sum();
        assert_eq!(livro.entrou(&c), sem_ajuste);
    }

    #[test]
    fn compra_no_cartao_nao_conta_duas_vezes() {
        let l = novo();
        let livro = l.0.borrow();
        // A compra no cartão vai para "No cartão"; o pagamento da fatura, para "Saiu".
        for m in &livro.movs {
            if m.cartao.is_some() {
                assert!(!Livro::e_saida_de_conta(m));
            }
        }
        assert!(
            livro
                .movs
                .iter()
                .any(|m| m.tipo == TipoMov::PagamentoDeFatura)
        );
    }
}
