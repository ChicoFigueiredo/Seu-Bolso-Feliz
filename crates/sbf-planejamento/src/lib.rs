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
    Parcelas { total: u32, primeira: Competencia },
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
    /// O meio, como na planilha: `CX-0630`, `Nubank`, `Salário` (desconto em folha).
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
    Adiar { nova_data: Option<Data> },
    /// Tira do exigível com motivo. Não é pagar.
    Dispensar { motivo: String },
    ValorSoDesteMes { valor: Centavos },
    PagoInformado { valor: Centavos, data: Option<Data> },
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
    pub prazo_desconhecido: bool,
    /// Há sugestão de conciliação à espera de confirmação.
    pub sugestao_de_conciliacao: bool,
}

// ---------------------------------------------------------------------------------------
// Visão do mês
// ---------------------------------------------------------------------------------------

/// O último saldo conhecido de uma conta, com a data da coleta.
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
// Escrita
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Autor {
    Usuario,
    Importacao,
    Ia { modelo: String },
    Mcp,
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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MudancaDeVersao {
    pub valor: Option<Option<Centavos>>,
    pub certeza: Option<Certeza>,
    pub fonte: Option<Fonte>,
    pub frequencia: Option<Frequencia>,
    pub conta: Option<Option<String>>,
    pub centro: Option<Option<String>>,
    pub fornecedor: Option<Option<String>>,
    pub categoria: Option<Option<String>>,
    pub nota: Option<Option<String>>,
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

    /// Aplica e grava no histórico. A janela e a importação usam; o MCP nunca.
    fn aplicar(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo>;

    /// Guarda uma proposta pendente. A mesma `chave` devolve a mesma proposta.
    fn propor(&mut self, alteracao: Alteracao, origem: &str, chave: &str)
    -> Resultado<Proposta>;

    fn propostas(&self, so_pendentes: bool) -> Resultado<Vec<Proposta>>;

    /// Só a janela chama: revalida a versão de base e aplica com autor `Usuario`.
    fn confirmar_proposta(&mut self, id: &str) -> Resultado<Recibo>;

    fn recusar_proposta(&mut self, id: &str, motivo: &str) -> Resultado<()>;

    fn historico(&self, alvo: Option<&str>, limite: u32) -> Resultado<Vec<EntradaDoHistorico>>;

    /// O recorrente que já nasceu desta origem, se houver.
    fn origem_importada(&self, origem: &OrigemImportada) -> Resultado<Option<String>>;
}

/// Os saldos das contas, vindos de fora (a Pluggy no inventário).
pub trait FonteDeSaldos {
    fn saldos(&self) -> Vec<Saldo>;
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
    fn id_da_ocorrencia_vai_e_volta() {
        for t in ["rec_1:2027-01", "rec_1:2027-01:p3", "rec:com:dois-pontos:2027-01"] {
            let id = IdOcorrencia::de_texto(t).expect(t);
            assert_eq!(id.to_string(), t);
        }
    }
}
