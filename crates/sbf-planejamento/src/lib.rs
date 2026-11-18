//! Planejamento do controle mensal (spec 11, ADR-019).
//!
//! **Costura.** Os tipos e o trait [`Planejamento`] foram fixados antes da implementação
//! paralela. O motor e o banco, a importação da planilha, a janela e o MCP usam só o que
//! está aqui. O vocabulário é o do `CONTEXT.md`: recorrente, ocorrência, versão, exceção,
//! pausa, cancelamento, pago informado, conciliada, em atraso, desconto em folha,
//! dinheiro de passagem, grupo, prioridade.
//!
//! Regras que valem para toda implementação:
//! - **Dinheiro em centavos inteiros** ([`Centavos`]), nunca `f64`. Valor desconhecido é
//!   `None` ("sem valor"), nunca zero.
//! - **Ocorrência é calculada, nunca gravada.** Vão ao banco só os recorrentes, as
//!   versões, as pausas, os cancelamentos, as exceções, as conciliações, as propostas e o
//!   histórico.
//! - **Nada daqui movimenta dinheiro.** Não escreve movimentação, não paga e não cancela
//!   nada fora do app.
//! - **Toda escrita passa por [`Alteracao`]** e deixa uma linha no histórico, que só
//!   cresce.
//! - **Excluir é lógico** ([`Exclusao`]). Nunca há `DELETE` de recorrente, exceção ou
//!   histórico.
//! - **Datas civis de São Paulo** ([`Data`]). `desde` é inclusivo e `ate` é exclusivo.
//! - **Vencimento em dia 29, 30 ou 31** cai no último dia válido do mês.
//! - **Nenhum texto de valor ou nome** vai a log.

use std::fmt;

mod banco;
pub mod conciliacao;
mod escrita;
mod modelo;
mod projecao;
mod resumo;
mod servico;

pub use banco::Banco;
pub use modelo::FABRICA_NOME_DO_MODELO;
pub use projecao::{
    projetar, projetar_competencias, projetar_competencias_desde, projetar_desde, versao_vigente,
};
pub use servico::PlanejamentoSqlite;

use serde::{Deserialize, Serialize};

pub use sbf_livro::{Centavos, Ciclo, Data, DiaDeCorte};

/// Quantos meses o controle mensal alcança a partir do mês escolhido (30 anos).
pub const HORIZONTE_MESES: u16 = 360;

// ---------------------------------------------------------------------------------------
// Tempo
// ---------------------------------------------------------------------------------------

/// Um mês civil, a competência de uma ocorrência. Ordena cronologicamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Competencia {
    pub ano: i32,
    pub mes: u8,
}

impl Competencia {
    pub fn nova(ano: i32, mes: u8) -> Option<Competencia> {
        (1..=12).contains(&mes).then_some(Competencia { ano, mes })
    }

    pub fn de_data(data: Data) -> Competencia {
        Competencia {
            ano: data.ano,
            mes: data.mes,
        }
    }

    /// Soma (ou subtrai) meses.
    pub fn somar_meses(self, meses: i32) -> Competencia {
        let total = self.ano * 12 + i32::from(self.mes) - 1 + meses;
        Competencia {
            ano: total.div_euclid(12),
            mes: (total.rem_euclid(12) + 1) as u8,
        }
    }

    pub fn seguinte(self) -> Competencia {
        self.somar_meses(1)
    }

    /// Meses de `self` até `outra` (positivo se `outra` vem depois).
    pub fn meses_ate(self, outra: Competencia) -> i32 {
        (outra.ano * 12 + i32::from(outra.mes)) - (self.ano * 12 + i32::from(self.mes))
    }

    /// O dia `dia` deste mês, ou o último dia válido quando o mês é mais curto.
    pub fn dia(self, dia: u8) -> Data {
        let ultimo = sbf_livro::data::dias_no_mes(self.ano, self.mes);
        Data {
            ano: self.ano,
            mes: self.mes,
            dia: dia.clamp(1, ultimo),
        }
    }

    /// `AAAA-MM`.
    pub fn de_texto(texto: &str) -> Option<Competencia> {
        let (a, m) = texto.split_once('-')?;
        if a.len() != 4 || m.len() != 2 {
            return None;
        }
        Competencia::nova(a.parse().ok()?, m.parse().ok()?)
    }
}

impl fmt::Display for Competencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.ano, self.mes)
    }
}

/// O período que o controle mensal mostra. O padrão é o ciclo (corte no dia 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Periodo {
    Ciclo(Ciclo),
    MesCivil(Competencia),
}

impl Periodo {
    pub fn inicio(&self) -> Data {
        match self {
            Periodo::Ciclo(c) => c.inicio,
            Periodo::MesCivil(m) => m.dia(1),
        }
    }

    /// Último dia, inclusive.
    pub fn fim(&self) -> Data {
        match self {
            Periodo::Ciclo(c) => c.fim,
            Periodo::MesCivil(m) => m.dia(31),
        }
    }

    pub fn contem(&self, data: Data) -> bool {
        self.inicio() <= data && data <= self.fim()
    }
}

// ---------------------------------------------------------------------------------------
// Regras
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentido {
    Entrada,
    Saida,
}

/// Como o dinheiro se move. Decide o que conta no caixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Natureza {
    /// Entra na conta: salário líquido, reembolso, transferência recebida.
    Entrada,
    /// Sai da conta ou do cartão: despesa, assinatura ou parcela de dívida.
    Saida,
    /// Retido no contracheque antes do depósito. Aparece, mas não sai do caixa de novo.
    DescontoEmFolha,
    /// Dinheiro de passagem (bolão): fica fora de receita e de despesa.
    Passagem { sentido: Sentido },
    /// Compra no cartão de crédito: aparece e é rastreada, mas o caixa sai pela fatura, que já
    /// está no plano. Fica fora das saídas de caixa para não contar duas vezes.
    DentroDaFatura,
}

/// Quando a regra gera ocorrência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Frequencia {
    /// Todo mês, no `dia`.
    Mensal { dia: u8 },
    /// A cada `n` meses, a partir de `primeira` (trimestral é `n = 3`).
    CadaNMeses {
        n: u8,
        dia: u8,
        primeira: Competencia,
    },
    /// Uma vez por ano.
    Anual { mes: u8, dia: u8 },
    /// Evento único, como um plano avulso (um presente) ou um resto antigo da planilha.
    Unica { data: Data },
}

/// Até quando a regra gera ocorrência. Prazo do contrato vence o horizonte de 360 meses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Prazo {
    SemFim,
    /// A última ocorrência é a da competência anterior a esta (exclusivo).
    AteCompetencia(Competencia),
    /// N parcelas, a primeira na competência `primeira`. Gera exatamente N.
    Parcelas {
        total: u32,
        primeira: Competencia,
    },
    /// Sem prova do fim: projeta até o horizonte, com `prazo_desconhecido = true`.
    Desconhecido,
}

/// De onde veio o valor de uma versão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fonte {
    Contrato,
    Planilha,
    /// Média das transações da Pluggy (os itens ocultos).
    Pluggy,
    Usuario,
    Ia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certeza {
    Confirmado,
    Estimado,
}

/// Os dados de um recorrente que valem a partir de `desde`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versao {
    pub id: String,
    /// Vale desta competência em diante, inclusive.
    pub desde: Competencia,
    /// `None` = sem valor (a conferir). Nunca vira zero.
    pub valor: Option<Centavos>,
    pub certeza: Certeza,
    pub fonte: Fonte,
    pub frequencia: Frequencia,
    /// O meio, como na planilha: `CX-0001`, `Nubank`, `Salário` (desconto em folha).
    pub conta: Option<String>,
    pub centro: Option<String>,
    pub fornecedor: Option<String>,
    pub categoria: Option<String>,
    pub nota: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pausa {
    pub id: String,
    pub desde: Competencia,
    /// Exclusivo. `None` = até retomar.
    pub ate: Option<Competencia>,
    pub motivo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancelamento {
    pub desde: Competencia,
    pub motivo: String,
    /// `true` só com documento do fornecedor. Sem ele, o app não afirma o cancelamento.
    pub comprovado: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusao {
    /// RFC 3339 UTC.
    pub quando: String,
    pub autor: Autor,
    pub motivo: String,
}

/// Um recorrente: a regra que gera as ocorrências.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorrente {
    pub id: String,
    pub nome: String,
    /// Grupo de nível 1, como na planilha (Essencial, Lar, Parcial...).
    pub grupo: String,
    /// Nível 2, opcional.
    pub subgrupo: Option<String>,
    /// A prioridade dentro do grupo: menor vem antes.
    pub posicao: i64,
    pub natureza: Natureza,
    pub prazo: Prazo,
    /// Ordenadas por `desde`, sempre ao menos uma.
    pub versoes: Vec<Versao>,
    pub pausas: Vec<Pausa>,
    pub cancelamento: Option<Cancelamento>,
    pub arquivado: bool,
    pub excluido: Option<Exclusao>,
    /// Concorrência otimista: sobe a cada alteração aceita.
    pub versao_do_registro: i64,
}

/// A ordem do grupo é a prioridade de nível 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grupo {
    pub nome: String,
    pub posicao: i64,
}

// ---------------------------------------------------------------------------------------
// Ocorrências
// ---------------------------------------------------------------------------------------

/// A identidade de uma ocorrência: não muda ao navegar, recalcular ou trocar de versão.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IdOcorrencia {
    pub recorrente: String,
    pub competencia: Competencia,
    /// Número da parcela, quando o prazo é de parcelas.
    pub parcela: Option<u32>,
}

impl fmt::Display for IdOcorrencia {
    /// `rec:AAAA-MM` ou `rec:AAAA-MM:p3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.recorrente, self.competencia)?;
        if let Some(p) = self.parcela {
            write!(f, ":p{p}")?;
        }
        Ok(())
    }
}

impl IdOcorrencia {
    /// Lê `rec:AAAA-MM` ou `rec:AAAA-MM:pN`, da direita para a esquerda (o id do
    /// recorrente pode ter `:`).
    pub fn de_texto(texto: &str) -> Option<IdOcorrencia> {
        let (resto, ultimo) = texto.rsplit_once(':')?;
        if let Some(n) = ultimo.strip_prefix('p').and_then(|n| n.parse::<u32>().ok()) {
            let (rec, comp) = resto.rsplit_once(':')?;
            return Some(IdOcorrencia {
                recorrente: rec.to_owned(),
                competencia: Competencia::de_texto(comp)?,
                parcela: Some(n),
            });
        }
        Some(IdOcorrencia {
            recorrente: resto.to_owned(),
            competencia: Competencia::de_texto(ultimo)?,
            parcela: None,
        })
    }
}

/// Uma decisão sobre uma única ocorrência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Excecao {
    /// `None` = adiada sem data: continua devida.
    Adiar {
        nova_data: Option<Data>,
    },
    /// Tira do exigível com motivo. Não é pagar.
    Dispensar {
        motivo: String,
    },
    ValorSoDesteMes {
        valor: Centavos,
    },
    PagoInformado {
        valor: Centavos,
        data: Option<Data>,
    },
    /// O usuário digitou a quantidade (as diárias) só deste mês: o Cálculo do Modelo de
    /// compromisso usa esta quantidade no lugar da que ele mesmo conta. Só vale em
    /// compromisso ligado a um modelo.
    QuantidadeSoDesteMes {
        quantidade: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcecaoGravada {
    pub id: String,
    pub ocorrencia: IdOcorrencia,
    pub excecao: Excecao,
    pub quando: String,
    pub autor: Autor,
    pub desfeita: bool,
}

/// A ligação de uma ocorrência com o movimento real que a pagou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conciliacao {
    pub ocorrencia: IdOcorrencia,
    /// `pluggy:txn_<id>` (o `external_ref` do inventário) ou o id da movimentação.
    pub movimento: String,
    pub valor: Centavos,
    pub data: Data,
    /// `false` = sugestão, à espera da confirmação do usuário.
    pub confirmada: bool,
}

/// Uma conta do planejamento (o apelido da planilha, como `CX-0001`) e a conta do extrato a
/// que ela corresponde (o `conta_provavel` do inventário, como `BANCO EXEMPLO · 000000-1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContaDoPlanejamento {
    pub apelido: String,
    /// `None` enquanto o usuário não confirmou um par.
    pub conta_do_extrato: Option<String>,
}

/// Um par apelido ↔ conta do extrato proposto pelo histórico, para o usuário confirmar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParDeConta {
    pub apelido: String,
    pub conta_do_extrato: String,
    /// Quantas conciliações (confirmadas ou sugeridas) e assinaturas apontam para o par.
    pub evidencias: u32,
    /// Em português, para a tela ("3 conciliações e o crédito do Salário").
    pub motivo: String,
}

/// Como achar no extrato o movimento de um recorrente de valor variável (o salário): o texto
/// que a descrição do banco traz e a janela em dias em volta do vencimento. A conta vem da
/// versão (o apelido) e do par confirmado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assinatura {
    /// Comparado sem acento e sem caixa, como "contém" ("SALARIO CAIXA").
    pub texto: String,
    pub janela_dias: u8,
}

/// Um movimento do extrato (a Pluggy no inventário), visto pelo planejamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovimentoDoExtrato {
    /// `pluggy:txn_<id>` (o `external_ref`).
    pub movimento: String,
    pub data: Data,
    /// Positivo entrou, negativo saiu.
    pub valor: Centavos,
    pub conta_do_extrato: String,
    pub descricao: String,
    /// O `tipo` do inventário (`credito`, `pagamento_fatura`, `transferencia_interna`…).
    pub tipo: String,
    /// O nome do favorecido (ou de quem pagou) que o banco informa à parte da descrição.
    /// Na CAIXA a descrição é só "PIX ENVIADO": o favorecido só aparece aqui.
    #[serde(default)]
    pub contraparte: Option<String>,
    /// O CPF/CNPJ da contraparte, às vezes formatado. Dado pessoal: nunca vai a log.
    #[serde(default)]
    pub contraparte_doc: Option<String>,
}

/// Uma conciliação gravada da ocorrência, com o movimento do extrato quando ele ainda aparece
/// lá (para o Detalhe mostrar data, conta e descrição do que foi confirmado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConciliacaoComMovimento {
    pub conciliacao: Conciliacao,
    pub movimento: Option<MovimentoDoExtrato>,
    /// A conta do movimento é a conta (par confirmado) da ocorrência.
    pub mesma_conta: bool,
    /// Quem gravou foi uma regra (`Autor::Regra`, as três provas): a janela mostra
    /// "automática" e oferece Desfazer.
    pub automatica: bool,
}

/// Uma baixa pelo contracheque (spec 12, D3): a rubrica de desconto ligada a um compromisso de
/// desconto em folha prova o desconto do mês, sem passar pelo extrato.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaixaPeloContracheque {
    /// `movimento = "contracheque:<sha>:<índice da rubrica>"`, o valor da rubrica e
    /// `confirmada: true`, na ocorrência do compromisso naquele mês.
    pub conciliacao: Conciliacao,
    /// O nome da rubrica no contracheque.
    pub rubrica: String,
    /// O previsto da ocorrência; `None` sem valor.
    pub previsto: Option<Centavos>,
    /// A rubrica menos o previsto, quando diferem (a janela mostra ⚠).
    pub diferenca: Option<Centavos>,
    /// A ocorrência já tem esta baixa gravada.
    pub ja_baixada: bool,
}

/// Um movimento candidato a uma ocorrência, para a escolha manual ("Conciliar com outro…") e
/// para a sugestão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovimentoCandidato {
    pub movimento: MovimentoDoExtrato,
    /// A descrição bate com a assinatura do recorrente.
    pub bate_assinatura: bool,
    /// A conta do movimento é a conta (par confirmado) da ocorrência.
    pub mesma_conta: bool,
    /// O valor esperado (líquido da composição do mês, se houver; senão o previsto) menos o
    /// valor do movimento, em módulo. `None` sem valor esperado.
    pub diferenca: Option<Centavos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaOcorrencia {
    Prevista,
    PagoInformado,
    /// Pago (informado ou conciliado) menor que o previsto.
    Parcial,
    Conciliada,
    /// Venceu sem pagamento conhecido e continua devida.
    EmAtraso,
    Adiada,
    Dispensada,
    /// Só aparece com o filtro de pausados e cancelados.
    SuprimidaPorPausa,
    SuprimidaPorCancelamento,
}

/// Um recorrente num mês. Calculada a partir das regras.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ocorrencia {
    pub id: IdOcorrencia,
    pub nome: String,
    pub grupo: String,
    pub subgrupo: Option<String>,
    pub posicao: i64,
    pub natureza: Natureza,
    /// Com o adiamento aplicado.
    pub vencimento: Data,
    pub vencimento_original: Data,
    pub previsto: Option<Centavos>,
    pub certeza: Certeza,
    pub fonte: Fonte,
    pub pago_informado: Option<Centavos>,
    pub conciliado: Option<Centavos>,
    pub estado: EstadoDaOcorrencia,
    /// (parcela, total).
    pub parcela: Option<(u32, u32)>,
    pub conta: Option<String>,
    pub centro: Option<String>,
    pub fornecedor: Option<String>,
    /// O id da versão que explica o valor.
    pub versao: String,
    /// O Cálculo do Modelo de compromisso que deu o `previsto`, se o compromisso usa um.
    pub calculo: Option<CalculoDaOcorrencia>,
    pub prazo_desconhecido: bool,
    /// Há sugestão de conciliação à espera de confirmação.
    pub sugestao_de_conciliacao: bool,
}

// ---------------------------------------------------------------------------------------
// Visão do mês
// ---------------------------------------------------------------------------------------

/// O último saldo conhecido de uma conta, com a data da coleta.
/// De onde veio o previsto de uma ocorrência de compromisso com modelo (Cálculo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalculoDaOcorrencia {
    /// O nome do Modelo de compromisso.
    pub modelo: String,
    /// O que o script diz ("diária R$ 210,00 × 5 sextas"); vazio quando o cálculo falhou.
    pub explicacao: String,
    /// A quantidade usada (as sextas do mês, ou a que o usuário ajustou).
    pub quantidade: Option<i64>,
    /// A quantidade foi digitada para este mês ([`Excecao::QuantidadeSoDesteMes`]).
    pub ajustada: bool,
    /// Por que o cálculo não valeu (⚠): o previsto cai no último valor conhecido.
    pub aviso: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saldo {
    pub conta: String,
    pub valor: Centavos,
    /// RFC 3339 UTC da coleta.
    pub quando: String,
}

/// Os números do topo do controle mensal. Cada um tem de ser rastreável às linhas.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Resumo {
    pub disponivel_agora: Vec<Saldo>,
    pub entradas_previstas: Centavos,
    /// Saídas de caixa: sem desconto em folha e sem dinheiro de passagem.
    pub saidas_de_caixa_previstas: Centavos,
    pub descontos_em_folha: Centavos,
    /// Compras no cartão do período: já estão dentro das faturas, fora das saídas de caixa.
    pub compras_no_cartao: Centavos,
    pub em_atraso: Centavos,
    /// Ocorrências sem valor ou só com pago informado.
    pub a_conferir: u32,
    /// Dinheiro de passagem que entrou e ainda não foi repassado.
    pub passagem_pendente: Centavos,
    pub livre_apos_compromissos: Centavos,
    /// `true` sempre que algum número por trás é estimado ou falta valor.
    pub livre_e_estimativa: bool,
    /// O que falta para o número ficar certo, em português, sem valor nem nome sensível.
    pub lacunas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Controle {
    pub periodo: Periodo,
    pub grupos: Vec<Grupo>,
    /// As ocorrências que vencem no período, na ordem de prioridade.
    pub linhas: Vec<Ocorrencia>,
    /// Em atraso de períodos anteriores, ainda devidas: aparecem junto do período.
    pub atrasadas_de_antes: Vec<Ocorrencia>,
    pub resumo: Resumo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filtro {
    #[default]
    Todos,
    APagar,
    EmAtraso,
    Pagos,
    SemComprovante,
    SemValor,
    PausadosECancelados,
}

/// Os totais de um mês do calendário de 360 meses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotaisDoMes {
    pub competencia: Competencia,
    pub entradas: Centavos,
    pub saidas_de_caixa: Centavos,
    pub descontos_em_folha: Centavos,
    pub sem_valor: u32,
    pub estimado: bool,
}

// ---------------------------------------------------------------------------------------
// Composição do salário e documentos
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeRubrica {
    Provento,
    Desconto,
}

/// Uma rubrica do contracheque. O salário continua digitado (decisão do CEO): as rubricas
/// são o detalhe que explica o líquido e servem para conferir a soma.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rubrica {
    /// O código do contracheque (`2002`), quando houver.
    pub codigo: Option<String>,
    pub nome: String,
    pub tipo: TipoDeRubrica,
    pub valor: Centavos,
    /// A competência que a rubrica traz (numa consignação, o início do contrato).
    pub competencia: Option<Competencia>,
    /// O prazo em meses que o contracheque informa (numa consignação, o total de parcelas).
    pub prazo: Option<u32>,
    /// O recorrente de desconto em folha que esta rubrica representa, quando houver.
    pub recorrente: Option<String>,
}

/// As rubricas de um recorrente de entrada (o salário) a partir de uma competência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composicao {
    pub recorrente: String,
    /// Vale desta competência em diante, como uma versão.
    pub desde: Competencia,
    pub rubricas: Vec<Rubrica>,
    /// O sha256 do documento de onde as rubricas vieram (o contracheque), quando houver.
    pub documento: Option<String>,
    /// De onde vieram, em português ("planilha 2026.10", "contracheque 11/2026").
    pub origem: String,
}

impl Composicao {
    /// Proventos menos descontos.
    pub fn liquido(&self) -> Centavos {
        self.rubricas
            .iter()
            .map(|r| match r.tipo {
                TipoDeRubrica::Provento => r.valor,
                TipoDeRubrica::Desconto => Centavos(-r.valor.0),
            })
            .sum()
    }

    /// A diferença entre o líquido das rubricas e o valor digitado; `None` quando batem ou
    /// quando não há valor digitado para comparar.
    pub fn divergencia(&self, digitado: Option<Centavos>) -> Option<Centavos> {
        let d = digitado?;
        let dif = Centavos(self.liquido().0 - d.0);
        (dif != Centavos(0)).then_some(dif)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeDocumento {
    Contracheque,
    /// Cobrança: fatura, conta. Não prova pagamento.
    Fatura,
    Boleto,
    /// Prova de pagamento.
    Comprovante,
    Contrato,
    NotaFiscal,
    Outro,
}

/// A que o documento se liga: à regra inteira (um contrato) ou a um mês (uma fatura).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum AlvoDoDocumento {
    Recorrente { id: String },
    Ocorrencia { id: IdOcorrencia },
}

/// Um documento do cofre (pelo sha256 do achado) ligado a um recorrente ou ocorrência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentoVinculado {
    pub id: String,
    pub alvo: AlvoDoDocumento,
    /// O `content_hash` do achado no inventário.
    pub sha256: String,
    pub tipo: TipoDeDocumento,
    /// Nome para mostrar (o nome do arquivo); nunca vai a log.
    pub nome: String,
    pub data: Option<Data>,
    pub quando: String,
    pub autor: Autor,
    pub desvinculado: bool,
}

// ---------------------------------------------------------------------------------------
// Modelo de compromisso e Cálculo
// ---------------------------------------------------------------------------------------

/// O tipo de um parâmetro de Modelo de compromisso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeParametro {
    /// Dinheiro, em centavos inteiros (≥ 0).
    Centavos,
    /// De 1 (segunda) a 7 (domingo).
    DiaDaSemana,
    Inteiro,
    Texto,
}

/// Um parâmetro que o modelo declara.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParametroDeModelo {
    /// Minúsculas, sem acento, `_` e dígitos: é a chave em `entrada.parametros`.
    pub nome: String,
    pub tipo: TipoDeParametro,
}

/// O valor de um parâmetro: centavos, dia da semana e inteiro são [`Inteiro`](Self::Inteiro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValorDoParametro {
    Inteiro(i64),
    Texto(String),
}

/// O valor de um parâmetro num compromisso, a partir de uma competência (inclusive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValorDeParametro {
    pub parametro: String,
    pub desde: Competencia,
    pub valor: ValorDoParametro,
}

/// Um Modelo de compromisso, com a versão mais recente do Cálculo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modelo {
    pub id: String,
    pub nome: String,
    pub parametros: Vec<ParametroDeModelo>,
    /// A versão mais recente do Cálculo (1, 2...).
    pub versao_do_calculo: u32,
    /// O sha256 do script dessa versão.
    pub checksum: String,
    /// O script Luau dessa versão.
    pub calculo: String,
    /// Veio de fábrica (semeado ao abrir o banco).
    pub de_fabrica: bool,
}

/// A ligação de um compromisso a um modelo, com os valores que ele preencheu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LigacaoDeModelo {
    pub recorrente: String,
    pub modelo_id: String,
    pub modelo: String,
    /// A versão do Cálculo a que a ligação está presa.
    pub versao_do_calculo: u32,
    pub checksum: String,
    pub parametros: Vec<ParametroDeModelo>,
    /// Os valores vigentes, do mais antigo ao mais novo, por parâmetro.
    pub valores: Vec<ValorDeParametro>,
}

// ---------------------------------------------------------------------------------------
// Escrita
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Autor {
    Usuario,
    Importacao,
    Ia {
        modelo: String,
    },
    Mcp,
    /// Uma regra que agiu sozinha (`"tres-provas"`); o usuário pode desfazer.
    Regra {
        regra: String,
    },
}

/// Onde a mudança vale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Escopo {
    /// Só o valor deste mês (vira [`Excecao::ValorSoDesteMes`]). Outros campos: inválido.
    SoEsteMes(Competencia),
    /// Versão nova a partir desta competência; as anteriores não mudam.
    DaquiEmDiante(Competencia),
}

/// Um remendo de versão: `None` mantém o campo; `Some(None)` limpa.
///
/// No JSON (propostas, inversos do Desfazer) o `Some(None)` vira `{"limpar": true}`: com o
/// serde padrão ele viraria `null` e voltaria como `None`, e "limpar a nota" se perderia.
/// `null` continua lendo como "não muda", como no que já foi gravado.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MudancaDeVersao {
    #[serde(with = "limpavel")]
    pub valor: Option<Option<Centavos>>,
    pub certeza: Option<Certeza>,
    pub fonte: Option<Fonte>,
    pub frequencia: Option<Frequencia>,
    #[serde(with = "limpavel")]
    pub conta: Option<Option<String>>,
    #[serde(with = "limpavel")]
    pub centro: Option<Option<String>>,
    #[serde(with = "limpavel")]
    pub fornecedor: Option<Option<String>>,
    #[serde(with = "limpavel")]
    pub categoria: Option<Option<String>>,
    #[serde(with = "limpavel")]
    pub nota: Option<Option<String>>,
}

/// `Option<Option<T>>` que sobrevive ao JSON: `null` = não muda, `{"limpar": true}` = limpa.
mod limpavel {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Forma<T> {
        Limpar { limpar: bool },
        Valor(T),
    }

    #[allow(clippy::option_option)]
    pub fn serialize<S: Serializer, T: Serialize>(
        v: &Option<Option<T>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match v {
            None => s.serialize_none(),
            Some(None) => Forma::<&T>::Limpar { limpar: true }.serialize(s),
            Some(Some(x)) => Forma::Valor(x).serialize(s),
        }
    }

    #[allow(clippy::option_option)]
    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Ok(match Option::<Forma<T>>::deserialize(d)? {
            None => None,
            Some(Forma::Limpar { limpar }) => limpar.then_some(None),
            Some(Forma::Valor(x)) => Some(Some(x)),
        })
    }
}

/// Uma linha de origem importada: a mesma origem nunca gera duas regras.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrigemImportada {
    pub arquivo_sha256: String,
    pub aba: String,
    pub linha: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRecorrente {
    pub nome: String,
    pub grupo: String,
    pub subgrupo: Option<String>,
    pub posicao: i64,
    pub natureza: Natureza,
    pub prazo: Prazo,
    /// O `id` é ignorado: o banco gera.
    pub primeira_versao: Versao,
}

/// Toda escrita no planejamento. As que tocam um recorrente levam `versao_base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Alteracao {
    CriarRecorrente {
        recorrente: NovoRecorrente,
        origem: Option<OrigemImportada>,
    },
    Editar {
        recorrente: String,
        escopo: Escopo,
        mudanca: MudancaDeVersao,
        versao_base: i64,
    },
    Pausar {
        recorrente: String,
        desde: Competencia,
        ate: Option<Competencia>,
        motivo: String,
        versao_base: i64,
    },
    /// Encerra a pausa aberta nesta competência (exclusivo).
    Retomar {
        recorrente: String,
        desde: Competencia,
        versao_base: i64,
    },
    Cancelar {
        recorrente: String,
        desde: Competencia,
        motivo: String,
        comprovado: bool,
        versao_base: i64,
    },
    Excecao {
        ocorrencia: IdOcorrencia,
        excecao: Excecao,
    },
    DesfazerExcecao {
        id: String,
    },
    /// Grava a conciliação; com `confirmada = false` é só sugestão.
    Conciliar {
        conciliacao: Conciliacao,
    },
    Excluir {
        recorrente: String,
        motivo: String,
        versao_base: i64,
    },
    Restaurar {
        recorrente: String,
        versao_base: i64,
    },
    Arquivar {
        recorrente: String,
        arquivado: bool,
        versao_base: i64,
    },
    /// Muda o nome do compromisso (o histórico guarda o nome de antes; dá para desfazer).
    Renomear {
        recorrente: String,
        nome: String,
        versao_base: i64,
    },
    /// Muda o grupo e/ou a posição (prioridade).
    Mover {
        recorrente: String,
        grupo: String,
        posicao: i64,
        versao_base: i64,
    },
    CriarGrupo {
        nome: String,
        posicao: i64,
    },
    /// Grava as rubricas de um recorrente a partir de `composicao.desde` (substitui a de
    /// mesmo `desde`). Não muda o valor digitado.
    DefinirComposicao {
        composicao: Composicao,
        versao_base: i64,
    },
    VincularDocumento {
        alvo: AlvoDoDocumento,
        sha256: String,
        tipo_documento: TipoDeDocumento,
        nome: String,
        data: Option<Data>,
    },
    DesvincularDocumento {
        id: String,
    },
    /// Desfaz uma conciliação (confirmada ou sugerida): exclusão lógica, com histórico. A
    /// ocorrência volta a poder ser sugerida.
    Desconciliar {
        ocorrencia: IdOcorrencia,
        movimento: String,
    },
    /// Confirma (ou desfaz, com `None`) o par apelido ↔ conta do extrato.
    MapearConta {
        apelido: String,
        conta_do_extrato: Option<String>,
    },
    /// Define (ou tira, com `None`) a assinatura de um recorrente.
    DefinirAssinatura {
        recorrente: String,
        assinatura: Option<Assinatura>,
        versao_base: i64,
    },
    /// Cria um Modelo de compromisso: os parâmetros e a versão 1 do Cálculo (script Luau com
    /// `calcular(entrada, ajuda)`).
    CriarModelo {
        nome: String,
        parametros: Vec<ParametroDeModelo>,
        calculo: String,
    },
    /// Uma versão nova do Cálculo do modelo `modelo` (id). As anteriores não mudam; os
    /// compromissos continuam presos à versão que usavam até serem ligados de novo.
    NovaVersaoDoCalculo {
        modelo: String,
        calculo: String,
    },
    /// Liga o compromisso ao modelo `modelo` (id), na versão mais recente do Cálculo, e
    /// grava os `valores` dos parâmetros com vigência.
    AplicarModelo {
        recorrente: String,
        modelo: String,
        valores: Vec<ValorDeParametro>,
        versao_base: i64,
    },
    /// Um valor de parâmetro com vigência (o de mesmo parâmetro e `desde` é substituído).
    DefinirParametro {
        recorrente: String,
        valor: ValorDeParametro,
        versao_base: i64,
    },
}

/// O que uma alteração aceita devolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recibo {
    pub alteracao_id: String,
    /// O id do recorrente, da exceção ou da ocorrência tocada.
    pub alvo: String,
    pub versao_nova: i64,
    pub quando: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaProposta {
    Pendente,
    Confirmada,
    Recusada,
    Expirada,
}

/// Uma alteração que a IA (MCP) propôs. Só vale depois de confirmada na janela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposta {
    pub id: String,
    pub alteracao: Alteracao,
    /// `mcp`, `ocultos`, `conciliacao`...
    pub origem: String,
    /// Idempotência: a mesma chave devolve a mesma proposta.
    pub chave: String,
    pub criada_em: String,
    pub expira_em: String,
    pub estado: EstadoDaProposta,
    /// Por que foi recusada ou expirou.
    pub motivo: Option<String>,
}

/// O efeito de uma alteração, antes de aplicar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Simulacao {
    pub meses_afetados: Vec<Competencia>,
    /// (competência, previsto antes, previsto depois) dos meses que mudam.
    pub mudancas: Vec<(Competencia, Option<Centavos>, Option<Centavos>)>,
    /// Exceções, pagamentos informados ou conciliações que a alteração atinge.
    pub conflitos: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaDoHistorico {
    pub id: String,
    pub quando: String,
    pub autor: Autor,
    pub alvo: String,
    /// Em português, sem valores sensíveis de terceiros.
    pub resumo: String,
    /// A alteração aplicada, em JSON.
    pub alteracao: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroDoPlanejamento {
    /// A versão de base ficou velha: recarregue e refaça.
    #[error("versão de base velha (a atual é {atual})")]
    VersaoVelha { atual: i64 },
    #[error("não encontrado: {0}")]
    NaoEncontrado(String),
    #[error("alteração inválida: {0}")]
    Invalida(String),
    #[error("banco do planejamento: {0}")]
    Banco(String),
}

pub type Resultado<T> = Result<T, ErroDoPlanejamento>;

// ---------------------------------------------------------------------------------------
// Serviço
// ---------------------------------------------------------------------------------------

/// O serviço único do controle mensal, usado pela janela e pelo MCP.
pub trait Planejamento {
    /// O período, com as linhas na ordem de prioridade e o resumo.
    fn controle(
        &self,
        periodo: &Periodo,
        filtro: Filtro,
        busca: Option<&str>,
        hoje: Data,
    ) -> Resultado<Controle>;

    fn recorrentes(&self, incluir_excluidos: bool) -> Resultado<Vec<Recorrente>>;
    fn recorrente(&self, id: &str) -> Resultado<Option<Recorrente>>;
    fn grupos(&self) -> Resultado<Vec<Grupo>>;

    /// As próximas ocorrências de um recorrente (aba "Próximas ocorrências").
    fn proximas(
        &self,
        recorrente: &str,
        desde: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<Ocorrencia>>;

    /// Totais mês a mês. `meses <= HORIZONTE_MESES`.
    fn calendario(
        &self,
        inicio: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<TotaisDoMes>>;

    /// O efeito de uma alteração, sem gravar nada.
    fn simular(&self, alteracao: &Alteracao, hoje: Data) -> Resultado<Simulacao>;

    /// Aplica e grava no histórico. A janela, a importação e (D15) o MCP usam.
    fn aplicar(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo>;

    /// Desfaz uma alteração do histórico (`alteracao_id`): aplica o inverso que ela guardou,
    /// com novo registro no histórico (autor `autor`), e marca a original como desfeita.
    /// Recusa a que já foi desfeita, a que é um Desfazer e a que não tem inverso. O padrão
    /// recusa tudo (quem não guarda o inverso não sabe desfazer).
    fn desfazer(&mut self, alteracao_id: &str, autor: Autor) -> Resultado<Recibo> {
        let _ = (alteracao_id, autor);
        Err(ErroDoPlanejamento::Invalida(
            "este planejamento não sabe desfazer alterações".to_owned(),
        ))
    }

    /// A alteração mais recente de `autor` que ainda dá para desfazer: não foi desfeita, não
    /// é ela mesma um Desfazer e tem inverso. O padrão é nenhuma.
    fn ultima_desfazivel(&self, autor: &Autor) -> Resultado<Option<EntradaDoHistorico>> {
        let _ = autor;
        Ok(None)
    }

    /// Guarda uma proposta pendente. A mesma `chave` devolve a mesma proposta.
    fn propor(&mut self, alteracao: Alteracao, origem: &str, chave: &str) -> Resultado<Proposta>;

    fn propostas(&self, so_pendentes: bool) -> Resultado<Vec<Proposta>>;

    /// A janela chama: revalida a versão de base e aplica com autor `Usuario`.
    fn confirmar_proposta(&mut self, id: &str) -> Resultado<Recibo>;

    /// Como [`Planejamento::confirmar_proposta`], mas com o autor dado no histórico (D15: o MCP
    /// aplica as propostas que ele mesmo fez, com autor `Mcp`). O padrão recusa.
    fn confirmar_proposta_como(&mut self, id: &str, autor: Autor) -> Resultado<Recibo> {
        let _ = (id, autor);
        Err(ErroDoPlanejamento::Invalida(
            "este planejamento não confirma proposta em nome de outro autor".to_owned(),
        ))
    }

    fn recusar_proposta(&mut self, id: &str, motivo: &str) -> Resultado<()>;

    fn historico(&self, alvo: Option<&str>, limite: u32) -> Resultado<Vec<EntradaDoHistorico>>;

    /// Uma linha do histórico pelo id (o `alteracao_id` do recibo).
    fn entrada_do_historico(&self, id: &str) -> Resultado<Option<EntradaDoHistorico>> {
        Ok(self
            .historico(None, 100_000)?
            .into_iter()
            .find(|e| e.id == id))
    }

    /// O recorrente que já nasceu desta origem, se houver.
    fn origem_importada(&self, origem: &OrigemImportada) -> Resultado<Option<String>>;

    /// A conciliação sugerida (não confirmada) de uma ocorrência, com o movimento, o valor
    /// e a data. A janela precisa dela para o botão "Confirmar conciliação": o
    /// [`Ocorrencia::sugestao_de_conciliacao`] só diz que ela existe. Quem não guarda
    /// sugestões devolve `None` (o padrão).
    fn sugestao_de_conciliacao(&self, ocorrencia: &IdOcorrencia) -> Resultado<Option<Conciliacao>> {
        let _ = ocorrencia;
        Ok(None)
    }

    /// A composição (rubricas) vigente de um recorrente na competência. O padrão é nenhuma.
    fn composicao(
        &self,
        recorrente: &str,
        competencia: Competencia,
    ) -> Resultado<Option<Composicao>> {
        let _ = (recorrente, competencia);
        Ok(None)
    }

    /// Os documentos ligados a um alvo (sem os desvinculados). O padrão é nenhum.
    fn documentos(&self, alvo: &AlvoDoDocumento) -> Resultado<Vec<DocumentoVinculado>> {
        let _ = alvo;
        Ok(Vec::new())
    }

    /// As exceções de um recorrente, inclusive as desfeitas (para idempotência e para a aba
    /// "Exceções"). O padrão é nenhuma, para implementações que não as guardam.
    fn excecoes_do_recorrente(&self, recorrente: &str) -> Resultado<Vec<ExcecaoGravada>> {
        let _ = recorrente;
        Ok(Vec::new())
    }

    /// Os apelidos de conta em uso (nas versões) e o par confirmado de cada um.
    fn contas(&self) -> Resultado<Vec<ContaDoPlanejamento>> {
        Ok(Vec::new())
    }

    /// Os pares apelido ↔ conta do extrato que o histórico sugere, para apelidos ainda sem par
    /// confirmado. Não grava nada.
    fn pares_de_conta_sugeridos(&self) -> Resultado<Vec<ParDeConta>> {
        Ok(Vec::new())
    }

    /// As contas que aparecem no extrato (para escolher o par de um apelido), em ordem.
    fn contas_do_extrato(&self) -> Resultado<Vec<String>> {
        Ok(Vec::new())
    }

    /// A assinatura de um recorrente, se houver.
    fn assinatura(&self, recorrente: &str) -> Resultado<Option<Assinatura>> {
        let _ = recorrente;
        Ok(None)
    }

    /// Os movimentos do extrato candidatos a uma ocorrência, do melhor ao pior: mesma natureza
    /// (entrada/saída), janela de ±10 dias (ou a da assinatura), primeiro os que batem a
    /// assinatura e a conta. Não grava nada.
    fn candidatos_de_conciliacao(
        &self,
        ocorrencia: &IdOcorrencia,
    ) -> Resultado<Vec<MovimentoCandidato>> {
        let _ = ocorrencia;
        Ok(Vec::new())
    }

    /// As conciliações gravadas da ocorrência (confirmadas e sugestão), com o movimento.
    fn conciliacoes_da_ocorrencia(
        &self,
        ocorrencia: &IdOcorrencia,
    ) -> Resultado<Vec<ConciliacaoComMovimento>> {
        let _ = ocorrencia;
        Ok(Vec::new())
    }

    /// As ocorrências a que este movimento do extrato está ligado (conciliações confirmadas e
    /// sugestões, sem as desfeitas): é como a página do registro diz "Concilia". O padrão é
    /// nenhuma.
    fn conciliacoes_do_movimento(&self, movimento: &str) -> Resultado<Vec<Conciliacao>> {
        let _ = movimento;
        Ok(Vec::new())
    }

    /// A conciliação deste par ocorrência e movimento já foi desfeita antes (exclusão lógica):
    /// uma regra não a refaz sozinha. O padrão é `false`.
    fn conciliacao_desfeita(&self, ocorrencia: &IdOcorrencia, movimento: &str) -> Resultado<bool> {
        let _ = (ocorrencia, movimento);
        Ok(false)
    }

    /// As baixas que o contracheque da competência prova (D3): cada rubrica de desconto da
    /// composição do Salário **do mês** (o `desde` é a competência) com `Rubrica::recorrente`
    /// cai na ocorrência daquele compromisso. Não grava nada; o padrão é nenhuma.
    fn baixas_pelo_contracheque(
        &self,
        competencia: Competencia,
    ) -> Resultado<Vec<BaixaPeloContracheque>> {
        let _ = competencia;
        Ok(Vec::new())
    }

    /// Os Modelos de compromisso (sem os desfeitos), com a versão mais recente do Cálculo.
    fn modelos(&self) -> Resultado<Vec<Modelo>> {
        Ok(Vec::new())
    }

    /// A ligação do compromisso a um modelo e os valores que ele preencheu, se houver.
    fn ligacao_de_modelo(&self, recorrente: &str) -> Resultado<Option<LigacaoDeModelo>> {
        let _ = recorrente;
        Ok(None)
    }

    /// Todas as ligações vigentes (para listar quem usa cada modelo).
    fn ligacoes_de_modelo(&self) -> Resultado<Vec<LigacaoDeModelo>> {
        Ok(Vec::new())
    }

    /// Procura no extrato as sugestões de conciliação do período e grava as novas
    /// (`confirmada: false`). Devolve quantas gravou.
    fn buscar_conciliacoes(&mut self, periodo: &Periodo, hoje: Data) -> Resultado<usize> {
        let _ = (periodo, hoje);
        Ok(0)
    }
}

/// Os saldos das contas, vindos de fora (a Pluggy no inventário).
pub trait FonteDeSaldos {
    fn saldos(&self) -> Vec<Saldo>;
}

/// Os movimentos do extrato, vindos de fora (a Pluggy no inventário), entre duas datas
/// inclusive.
pub trait FonteDeMovimentos {
    fn movimentos(&self, de: Data, ate: Data) -> Vec<MovimentoDoExtrato>;
}

/// Sem extrato.
#[derive(Debug, Default, Clone, Copy)]
pub struct SemMovimentos;

impl FonteDeMovimentos for SemMovimentos {
    fn movimentos(&self, _de: Data, _ate: Data) -> Vec<MovimentoDoExtrato> {
        Vec::new()
    }
}

/// Sem saldo conhecido.
#[derive(Debug, Default, Clone, Copy)]
pub struct SemSaldos;

impl FonteDeSaldos for SemSaldos {
    fn saldos(&self) -> Vec<Saldo> {
        Vec::new()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn competencia_soma_e_vira_ano() {
        let c = Competencia { ano: 2026, mes: 11 };
        assert_eq!(c.somar_meses(2), Competencia { ano: 2027, mes: 1 });
        assert_eq!(c.somar_meses(-11), Competencia { ano: 2025, mes: 12 });
        assert_eq!(c.somar_meses(359), Competencia { ano: 2056, mes: 10 });
        assert_eq!(c.meses_ate(Competencia { ano: 2056, mes: 10 }), 359);
    }

    #[test]
    fn dia_31_cai_no_ultimo_dia_valido() {
        assert_eq!(Competencia { ano: 2027, mes: 2 }.dia(31).dia, 28);
        assert_eq!(Competencia { ano: 2028, mes: 2 }.dia(31).dia, 29);
        assert_eq!(Competencia { ano: 2026, mes: 4 }.dia(31).dia, 30);
    }

    #[test]
    fn composicao_soma_e_acusa_divergencia() {
        let r = |tipo, v| Rubrica {
            codigo: None,
            nome: "x".into(),
            tipo,
            valor: Centavos(v),
            competencia: None,
            prazo: None,
            recorrente: None,
        };
        let c = Composicao {
            recorrente: "sal".into(),
            desde: Competencia { ano: 2026, mes: 11 },
            rubricas: vec![
                r(TipoDeRubrica::Provento, 100000),
                r(TipoDeRubrica::Desconto, 25000),
            ],
            documento: None,
            origem: "teste".into(),
        };
        assert_eq!(c.liquido(), Centavos(75000));
        assert_eq!(c.divergencia(Some(Centavos(75000))), None);
        assert_eq!(c.divergencia(Some(Centavos(70000))), Some(Centavos(5000)));
        assert_eq!(c.divergencia(None), None);
    }

    #[test]
    fn id_da_ocorrencia_vai_e_volta() {
        for t in [
            "rec_1:2027-01",
            "rec_1:2027-01:p3",
            "rec:com:dois-pontos:2027-01",
        ] {
            let id = IdOcorrencia::de_texto(t).expect(t);
            assert_eq!(id.to_string(), t);
        }
    }
}
