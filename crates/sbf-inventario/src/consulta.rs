//! A leitura que a janela usa: tabelas das áreas, páginas de item, totais clicáveis,
//! rastro, Visão geral e Relatórios.
//!
//! Dono: integrador (o contrato). A implementação sobre o SQLite é do agente de
//! leitura (`leitor`); a janela tem uma implementação fictícia para o modo demonstração.
//!
//! Espelha as APIs `REL` e `RASTRO` do protótipo:
//! - `REL.ligados(id, tipo)` → [`PedidoDeTabela::Ligados`];
//! - `REL.itensDoTotal(id, …)` → [`PedidoDeTabela::ItensDoTotal`], que todo
//!   [`Celula::Valor`] e [`Celula::Contagem`] clicável carrega;
//! - `REL.porCiclo` → [`Consulta::por_ciclo`];
//! - `RASTRO.de(id)` → [`Consulta::rastro`].
//!
//! Livro-caixa e inventário nunca se somam: todo valor diz a sua [`Fonte`], e a
//! janela põe a marca "· a conferir" em todo valor do inventário.

use serde::{Deserialize, Serialize};

use crate::referencia::{Ref, Tipo};

/// Valor em centavos. Positivo entrou, negativo saiu (como `registro.valor_centavos`).
pub type Centavos = i64;

/// De onde vem um número.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fonte {
    /// Os registros do inventário. Todo número leva "a conferir".
    Inventario,
    /// As movimentações e o resto do livro-caixa (fatia 2), lidos da cópia local.
    LivroCaixa,
}

/// Quem tomou uma decisão: "você" ou uma rodada de regras.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quem {
    Usuario,
    /// `regras@N`, ou o responsável de uma coleta (`script@versão`).
    Automatico(String),
}

impl Quem {
    /// Lê `usuario` ou qualquer outro texto (`regras@2`).
    pub fn de_texto(texto: &str) -> Quem {
        if texto == "usuario" {
            Quem::Usuario
        } else {
            Quem::Automatico(texto.to_owned())
        }
    }

    /// Como aparece na interface: "você" ou o texto original.
    pub fn rotulo(&self) -> &str {
        match self {
            Quem::Usuario => "você",
            Quem::Automatico(t) => t,
        }
    }
}

/// Uma célula de tabela ou o valor de um campo do cabeçalho de uma página.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Celula {
    Vazia,
    Texto(String),
    /// Um nome com identidade própria: sempre vira link.
    Link {
        alvo: Ref,
        texto: String,
    },
    /// Data `AAAA-MM-DD` ou instante ISO 8601.
    Data(String),
    /// Um valor em dinheiro. Com `itens`, é clicável e abre o que o soma.
    Valor {
        centavos: Centavos,
        fonte: Fonte,
        itens: Option<Box<PedidoDeTabela>>,
    },
    /// Uma contagem. Com `itens`, é clicável e abre o que ela conta.
    Contagem {
        n: i64,
        itens: Option<Box<PedidoDeTabela>>,
    },
    /// Estado de um achado, registro ou lote, com quem decidiu.
    Estado {
        rotulo: String,
        quem: Option<Quem>,
    },
    /// Uma lista de nomes (apelidos, origens…): cada um vira link.
    Links(Vec<(Ref, String)>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coluna {
    pub rotulo: String,
    /// Alinha à direita e ordena como número.
    pub numerica: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Linha {
    /// O item que a linha representa (seleção, Ctrl+clique, Ações do Ribbon).
    pub alvo: Option<Ref>,
    pub celulas: Vec<Celula>,
}

/// Uma tabela densa. A janela virtualiza; a consulta devolve todas as linhas.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Tabela {
    pub colunas: Vec<Coluna>,
    pub linhas: Vec<Linha>,
}

/// As áreas do grupo Inventário mais Relatórios (spec 06, "Áreas e vistas"), as de
/// Dinheiro e a Sincronização (spec 07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Area {
    VisaoGeral,
    ATriar,
    EmDuvida,
    Documentos,
    Registros,
    Ciclos,
    Fornecedores,
    Faturas,
    ContasLacunas,
    /// As mensagens (vista `emails`).
    Emails,
    /// A outra metade da área E-mails (vista `emails_por_remetente`).
    EmailsPorRemetente,
    /// Os arquivos (vista `arquivos`).
    ArquivosEPastas,
    /// A outra metade da área Arquivos e pastas (vista `pastas_pessoais`).
    PastasPessoais,
    Coletas,
    Pendencias,
    Lotes,
    /// Os pares óbvios entre registros de extrato e de documento, que o usuário
    /// confirma em lote (`conciliacao`).
    SugestoesDeConciliacao,
    Relatorios,
    // Dinheiro (fatia 2): leem o livro-caixa (`Fonte::LivroCaixa`).
    /// Painel: Disponível agora, Livre após compromissos, Resultado do mês, Para
    /// resolver. Sem tabela principal: use [`Consulta::painel_da_area`].
    Hoje,
    Movimentacoes,
    /// Contas (tabela principal) e cartões com a fatura atual (seção do painel).
    ContasECartoes,
    Dividas,
    Recorrentes,
    /// Centros (tabela principal) e o Não alocado do ciclo (seção do painel).
    Centros,
    /// Os fornecedores do livro-caixa. A área `Fornecedores` continua sendo a do
    /// inventário (registros a conferir): as duas fontes nunca se somam.
    FornecedoresDoLivro,
    // Estação (fatia 2)
    /// Lida pelo trait [`crate::alteracao::Sincronizacao`], não pela `Consulta`.
    Sincronizacao,
}

impl Area {
    /// As áreas de Dinheiro, na ordem do painel de navegação.
    pub const DO_LIVRO_CAIXA: [Area; 7] = [
        Area::Hoje,
        Area::Movimentacoes,
        Area::ContasECartoes,
        Area::Dividas,
        Area::Recorrentes,
        Area::Centros,
        Area::FornecedoresDoLivro,
    ];

    /// A área lê o livro-caixa (e não o inventário)?
    pub fn e_do_livro_caixa(self) -> bool {
        Area::DO_LIVRO_CAIXA.contains(&self)
    }
}

/// Os filtros da aba Início do Ribbon.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Filtro {
    /// Pesquisar: texto livre.
    pub texto: Option<String>,
    /// Período: do ciclo `AAAA-MM` ao ciclo `AAAA-MM`, inclusive.
    pub ciclos: Option<(String, String)>,
    /// Estados do achado (`novo`, `documento`, `descartado`, `duvida`).
    pub estados: Vec<String>,
    /// Tipos de documento.
    pub tipos: Vec<String>,
    /// Ids de `fonte`.
    pub fontes: Vec<String>,
    /// Só o que foi decidido por este autor.
    pub quem_decidiu: Option<Quem>,
}

/// O que uma tabela mostra.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PedidoDeTabela {
    /// Uma área do painel de navegação.
    Area { area: Area, filtro: Filtro },
    /// Uma aba de página: os itens de `tipo` ligados a `de`, em qualquer direção.
    Ligados { de: Ref, tipo: Tipo },
    /// O que soma (ou conta) um número. `recorte` distingue números do mesmo item
    /// (por exemplo `saidas` e `entradas`); quem gera o número escolhe o texto.
    ItensDoTotal {
        de: Ref,
        fonte: Fonte,
        ciclo: Option<String>,
        recorte: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Campo {
    pub rotulo: String,
    pub valor: Celula,
}

/// O cabeçalho de uma página de item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub alvo: Ref,
    /// O nome do item, usado no título, na trilha e nos links.
    pub rotulo: String,
    /// O resumo. Todo número aqui é clicável.
    pub campos: Vec<Campo>,
}

/// Um elo do rastro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Elo {
    pub alvo: Ref,
    pub rotulo: String,
    /// "achado em", "extraído de", "decidido por", "aprovado no lote", "promovido para"…
    pub papel: String,
    pub quem: Option<Quem>,
    /// Instante ISO 8601.
    pub quando: Option<String>,
}

/// A cadeia de proveniência de um item.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Rastro {
    /// De onde veio, até a origem e a conexão. O primeiro é o mais próximo.
    pub acima: Vec<Elo>,
    /// No que virou: registro, lote, movimentação, rateio, centros.
    pub abaixo: Vec<Elo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TotalDoCiclo {
    pub ciclo: String,
    pub centavos: Centavos,
    pub itens: Option<Box<PedidoDeTabela>>,
}

/// Uma seção de painel (Visão geral, Relatórios).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Secao {
    pub titulo: String,
    /// Texto curto sob o título (por exemplo, o que a seção não inclui).
    pub nota: Option<String>,
    pub tabela: Tabela,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Painel {
    pub secoes: Vec<Secao>,
    /// Avisos no topo (por exemplo, fontes planejadas sem coleta).
    pub avisos: Vec<String>,
}

/// Um número de destaque no topo de uma área de Dinheiro ("Disponível agora",
/// "Resultado do mês"…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Indicador {
    pub rotulo: String,
    /// Em geral [`Celula::Valor`] clicável (abre o que soma o número).
    pub valor: Celula,
    /// Texto curto sob o número ("de 20/out a 19/nov", "3 contas").
    pub nota: Option<String>,
    /// `Some(premissa)` quando o número é estimativa (grill: "Livre após
    /// compromissos" com ocorrência sem valor esperado). A janela põe a marca.
    pub estimativa: Option<String>,
    /// O número vale "neste instante" (Disponível agora): offline, a janela mostra a
    /// hora da cópia ao lado dele (grill Q13).
    pub instantaneo: bool,
}

/// O painel de uma área de Dinheiro: os números do topo e as seções que não são a
/// tabela principal (a tabela principal vem de [`Consulta::tabela`] com
/// [`PedidoDeTabela::Area`], para virtualizar e selecionar).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PainelDeArea {
    /// O ciclo a que os números se referem (`AAAA-MM`), quando há um.
    pub ciclo: Option<String>,
    pub indicadores: Vec<Indicador>,
    /// Na Hoje, a primeira é **Para resolver** (grill Q16): uma linha por tipo de coisa
    /// (vencidos, sem categoria, não alocadas, não confirmadas, documentos a revisar),
    /// com a [`Celula::Contagem`] clicável.
    pub secoes: Vec<Secao>,
    pub avisos: Vec<String>,
}

/// As perguntas do §11 que a área Relatórios responde (spec 06, "Relatórios do §11").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pergunta {
    QuantoSaiu,
    MaioresFornecedores,
    OQueSeRepete,
    OQueAumentou,
    OndeSeConcentra,
    NaoClassificado,
    PossiveisDuplicidades,
    OQueFaltaParaConfiar,
}

/// As contagens da barra de status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Contagens {
    pub achados: i64,
    pub documentos: i64,
    pub em_duvida: i64,
    pub a_triar: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDeConsulta {
    #[error("item não encontrado: {0}")]
    NaoEncontrado(Ref),
    #[error("ainda não suportado: {0}")]
    NaoSuportado(String),
    #[error("erro do banco: {0}")]
    Banco(String),
}

impl From<rusqlite::Error> for ErroDeConsulta {
    fn from(erro: rusqlite::Error) -> Self {
        ErroDeConsulta::Banco(erro.to_string())
    }
}

pub type ResultadoDeConsulta<T> = Result<T, ErroDeConsulta>;

/// Um pedaço de uma tabela grande: as linhas `[inicio, inicio + tabela.linhas.len())` de
/// `total`. A janela virtualiza o `total` inteiro e pede o próximo pedaço quando a rolagem
/// chega perto do fim do que já carregou.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PaginaDeTabela {
    /// As colunas e só as linhas deste pedaço.
    pub tabela: Tabela,
    /// A posição da primeira linha do pedaço na tabela inteira.
    pub inicio: usize,
    /// Quantas linhas a tabela inteira tem (com o mesmo filtro).
    pub total: usize,
}

/// A leitura do inventário. Só leitura: nada aqui escreve.
pub trait Consulta {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela>;

    /// O pedaço `[inicio, inicio + limite)` da tabela de `pedido`, mais o total de linhas.
    ///
    /// A implementação padrão monta a tabela inteira e corta; quem tem SQL por trás
    /// (`LeitorSqlite`) devolve só o pedaço pedido, em tempo que não depende do total. A
    /// ordem das linhas é a mesma de [`Consulta::tabela`].
    fn tabela_paginada(
        &self,
        pedido: &PedidoDeTabela,
        inicio: usize,
        limite: usize,
    ) -> ResultadoDeConsulta<PaginaDeTabela> {
        let tabela = self.tabela(pedido)?;
        let total = tabela.linhas.len();
        let linhas = tabela
            .linhas
            .into_iter()
            .skip(inicio)
            .take(limite)
            .collect();
        Ok(PaginaDeTabela {
            tabela: Tabela {
                colunas: tabela.colunas,
                linhas,
            },
            inicio,
            total,
        })
    }

    /// O cabeçalho da página. `None` quando o item não existe.
    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>>;
    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro>;
    /// O total de `alvo` nos últimos `ciclos` ciclos, do mais antigo ao mais novo.
    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>>;
    /// O painel de contagens do `relatorio.ts`.
    fn visao_geral(&self) -> ResultadoDeConsulta<Painel>;
    /// Uma pergunta do §11. Com `Fonte::LivroCaixa`, devolve `NaoSuportado` na fatia 1.
    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel>;
    fn contagens(&self) -> ResultadoDeConsulta<Contagens>;

    /// Os números e as seções de uma área de Dinheiro (fatia 2). O inventário não tem
    /// e devolve `NaoSuportado`.
    fn painel_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        let _ = filtro;
        Err(ErroDeConsulta::NaoSuportado(format!(
            "a área {area:?} não tem painel nesta fonte"
        )))
    }
    /// O texto extraído de um achado (painel Texto).
    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>>;
}
