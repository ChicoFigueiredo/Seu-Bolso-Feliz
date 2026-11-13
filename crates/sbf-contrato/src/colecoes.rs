//! As coleções do livro-caixa como **linhas** (o servidor devolve linhas, não telas;
//! grill Q8). Cada linha tem o `id` e, quando a tabela tem `updated_at`, a [`Versao`].
//! Os números das telas são calculados na estação pelo `sbf-livro`.
//!
//! Os enums trazem os valores fechados do banco (os `CHECK` de `schema.sql` mais as
//! migrações), com nomes em português no fio (`snake_case`).

use serde::{Deserialize, Serialize};

use crate::{Centavos, Data, Id, Instante, Versao};

/// As coleções de `GET /livro/{colecao}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Colecao {
    /// Produtos financeiros (`financial_products`): conta corrente, poupança, cartão…
    Contas,
    Instituicoes,
    Cartoes,
    /// Faturas de cartão (`statement_cycles`).
    Faturas,
    ItensDeFatura,
    Movimentacoes,
    Transferencias,
    Rateios,
    Categorias,
    Tags,
    Centros,
    Fornecedores,
    /// Apelidos de fornecedor (`supplier_aliases`).
    Apelidos,
    /// Contratos com fornecedor (`supplier_contracts`).
    Contratos,
    Dividas,
    /// Parcelas de dívida (`liability_installments`).
    Parcelas,
    Recorrentes,
    /// Ocorrências de recorrente (`recurring_instances`).
    Ocorrencias,
    /// Metadados dos documentos do servidor (`source_documents`), sem os bytes.
    Documentos,
}

impl Colecao {
    /// Todas, na ordem em que a sincronização as baixa.
    pub const TODAS: [Colecao; 19] = [
        Colecao::Instituicoes,
        Colecao::Contas,
        Colecao::Cartoes,
        Colecao::Faturas,
        Colecao::ItensDeFatura,
        Colecao::Categorias,
        Colecao::Tags,
        Colecao::Centros,
        Colecao::Fornecedores,
        Colecao::Apelidos,
        Colecao::Contratos,
        Colecao::Movimentacoes,
        Colecao::Transferencias,
        Colecao::Rateios,
        Colecao::Dividas,
        Colecao::Parcelas,
        Colecao::Recorrentes,
        Colecao::Ocorrencias,
        Colecao::Documentos,
    ];

    /// O segmento do caminho: `GET /api/v1/livro/{caminho}`.
    pub fn caminho(self) -> &'static str {
        match self {
            Colecao::Contas => "contas",
            Colecao::Instituicoes => "instituicoes",
            Colecao::Cartoes => "cartoes",
            Colecao::Faturas => "faturas",
            Colecao::ItensDeFatura => "itens-de-fatura",
            Colecao::Movimentacoes => "movimentacoes",
            Colecao::Transferencias => "transferencias",
            Colecao::Rateios => "rateios",
            Colecao::Categorias => "categorias",
            Colecao::Tags => "tags",
            Colecao::Centros => "centros",
            Colecao::Fornecedores => "fornecedores",
            Colecao::Apelidos => "apelidos",
            Colecao::Contratos => "contratos",
            Colecao::Dividas => "dividas",
            Colecao::Parcelas => "parcelas",
            Colecao::Recorrentes => "recorrentes",
            Colecao::Ocorrencias => "ocorrencias",
            Colecao::Documentos => "documentos",
        }
    }

    /// O inverso de [`Colecao::caminho`].
    pub fn do_caminho(caminho: &str) -> Option<Colecao> {
        Colecao::TODAS.into_iter().find(|c| c.caminho() == caminho)
    }
}

/// O corpo de `GET /livro/{colecao}`: a coleção inteira, marcada pelo nome.
///
/// No fio: `{"colecao":"movimentacoes","linhas":[...]}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "colecao", content = "linhas", rename_all = "kebab-case")]
pub enum LinhasDaColecao {
    Contas(Vec<Conta>),
    Instituicoes(Vec<Instituicao>),
    Cartoes(Vec<Cartao>),
    Faturas(Vec<Fatura>),
    ItensDeFatura(Vec<ItemDeFatura>),
    Movimentacoes(Vec<Movimentacao>),
    Transferencias(Vec<Transferencia>),
    Rateios(Vec<Rateio>),
    Categorias(Vec<Categoria>),
    Tags(Vec<Tag>),
    Centros(Vec<Centro>),
    Fornecedores(Vec<Fornecedor>),
    Apelidos(Vec<Apelido>),
    Contratos(Vec<Contrato>),
    Dividas(Vec<Divida>),
    Parcelas(Vec<Parcela>),
    Recorrentes(Vec<Recorrente>),
    Ocorrencias(Vec<Ocorrencia>),
    Documentos(Vec<Documento>),
}

impl LinhasDaColecao {
    /// A coleção destas linhas.
    pub fn colecao(&self) -> Colecao {
        match self {
            LinhasDaColecao::Contas(_) => Colecao::Contas,
            LinhasDaColecao::Instituicoes(_) => Colecao::Instituicoes,
            LinhasDaColecao::Cartoes(_) => Colecao::Cartoes,
            LinhasDaColecao::Faturas(_) => Colecao::Faturas,
            LinhasDaColecao::ItensDeFatura(_) => Colecao::ItensDeFatura,
            LinhasDaColecao::Movimentacoes(_) => Colecao::Movimentacoes,
            LinhasDaColecao::Transferencias(_) => Colecao::Transferencias,
            LinhasDaColecao::Rateios(_) => Colecao::Rateios,
            LinhasDaColecao::Categorias(_) => Colecao::Categorias,
            LinhasDaColecao::Tags(_) => Colecao::Tags,
            LinhasDaColecao::Centros(_) => Colecao::Centros,
            LinhasDaColecao::Fornecedores(_) => Colecao::Fornecedores,
            LinhasDaColecao::Apelidos(_) => Colecao::Apelidos,
            LinhasDaColecao::Contratos(_) => Colecao::Contratos,
            LinhasDaColecao::Dividas(_) => Colecao::Dividas,
            LinhasDaColecao::Parcelas(_) => Colecao::Parcelas,
            LinhasDaColecao::Recorrentes(_) => Colecao::Recorrentes,
            LinhasDaColecao::Ocorrencias(_) => Colecao::Ocorrencias,
            LinhasDaColecao::Documentos(_) => Colecao::Documentos,
        }
    }

    /// Quantas linhas.
    pub fn quantas(&self) -> usize {
        match self {
            LinhasDaColecao::Contas(v) => v.len(),
            LinhasDaColecao::Instituicoes(v) => v.len(),
            LinhasDaColecao::Cartoes(v) => v.len(),
            LinhasDaColecao::Faturas(v) => v.len(),
            LinhasDaColecao::ItensDeFatura(v) => v.len(),
            LinhasDaColecao::Movimentacoes(v) => v.len(),
            LinhasDaColecao::Transferencias(v) => v.len(),
            LinhasDaColecao::Rateios(v) => v.len(),
            LinhasDaColecao::Categorias(v) => v.len(),
            LinhasDaColecao::Tags(v) => v.len(),
            LinhasDaColecao::Centros(v) => v.len(),
            LinhasDaColecao::Fornecedores(v) => v.len(),
            LinhasDaColecao::Apelidos(v) => v.len(),
            LinhasDaColecao::Contratos(v) => v.len(),
            LinhasDaColecao::Dividas(v) => v.len(),
            LinhasDaColecao::Parcelas(v) => v.len(),
            LinhasDaColecao::Recorrentes(v) => v.len(),
            LinhasDaColecao::Ocorrencias(v) => v.len(),
            LinhasDaColecao::Documentos(v) => v.len(),
        }
    }
}

// ───────────────────────────── valores fechados ─────────────────────────────

/// `institutions.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeInstituicao {
    Banco,
    Cooperativa,
    Corretora,
    Fintech,
    /// O valor antigo `broker`, que convive com `brokerage` desde a migração 0003.
    CorretoraAntiga,
    Outra,
}

/// `financial_products.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeConta {
    ContaCorrente,
    Poupanca,
    CartaoDeCredito,
    ChequeEspecial,
    EmprestimoPessoal,
    Financiamento,
    Investimento,
    Outra,
}

/// `transactions.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeMovimentacao {
    /// `income`
    Entrada,
    /// `expense`
    Saida,
    /// `refund`
    Estorno,
    /// `adjustment`: ajuste de saldo; não entra em lado nenhum (grill Q15).
    Ajuste,
    /// `interest_charge`
    Juros,
    /// `fee`
    Tarifa,
    /// `statement_payment`
    PagamentoDeFatura,
    /// `liability_payment`
    PagamentoDeDivida,
}

/// `transactions.origin_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrigemDaMovimentacao {
    Manual,
    Importacao,
    Recorrente,
    Fatura,
}

/// `priority` (movimentação, recorrente, tag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prioridade {
    Essencial,
    Alta,
    Media,
    Baixa,
    Opcional,
}

/// `statement_cycles.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaFatura {
    Aberta,
    Fechada,
    Paga,
    Parcial,
    Vencida,
}

/// `financial_centers.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeCentro {
    Projeto,
    Pessoa,
    Negocio,
}

/// `financial_centers.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDoCentro {
    Ativo,
    Arquivado,
}

/// `suppliers.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeFornecedor {
    Empresa,
    PessoaFisica,
    Governo,
    Concessionaria,
    Telecom,
    Software,
    Plataforma,
    Outro,
}

/// `supplier_aliases.alias_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeApelido {
    NomeAntigo,
    Abreviacao,
    NomeFantasia,
    NomeDeCobranca,
    Outro,
}

/// `supplier_contracts.contract_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeContrato {
    Servico,
    Assinatura,
    Consumo,
    Emprestimo,
    Seguro,
    Manutencao,
    Outro,
}

/// `liabilities.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeDivida {
    EmprestimoPessoal,
    Financiamento,
    ChequeEspecial,
    Parcelamento,
    Outra,
}

/// `liabilities.rate_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodoDaTaxa {
    Mensal,
    Anual,
}

/// `liabilities.amortization_system`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SistemaDeAmortizacao {
    Sac,
    Price,
    Misto,
    Outro,
    Nenhum,
}

/// `liabilities.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaDivida {
    Ativa,
    Quitada,
    Renegociada,
    Inadimplente,
}

/// `liability_installments.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaParcela {
    Pendente,
    Paga,
    Parcial,
    Vencida,
    Dispensada,
}

/// `recurring_templates.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeRecorrente {
    Entrada,
    Saida,
    PagamentoDeDivida,
    PagamentoDeFatura,
}

/// `recurring_templates.frequency`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequencia {
    Mensal,
    Semanal,
    Quinzenal,
    Trimestral,
    Anual,
    /// A cada `intervalo_em_dias`.
    Personalizada,
}

/// `recurring_instances.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDaOcorrencia {
    Pendente,
    Paga,
    Parcial,
    Pulada,
    Vencida,
    Cancelada,
}

/// `source_documents.origin_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrigemDoDocumento {
    Gmail,
    ArquivoLocal,
    EnvioManual,
}

// ───────────────────────────────── linhas ─────────────────────────────────

/// `institutions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instituicao {
    pub id: Id,
    pub versao: Versao,
    pub nome: String,
    pub tipo: TipoDeInstituicao,
    pub cor: Option<String>,
    pub icone_url: Option<String>,
    pub ativa: bool,
    pub ordem: Option<i32>,
}

/// `financial_products`: a conta (ou produto financeiro). O saldo é **gravado**, não
/// derivado das movimentações.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conta {
    pub id: Id,
    pub versao: Versao,
    pub instituicao: Id,
    pub nome: String,
    pub tipo: TipoDeConta,
    pub saldo_atual: Option<Centavos>,
    pub limite_de_credito: Option<Centavos>,
    pub ativa: bool,
    pub ordem: Option<i32>,
}

/// `cards`. Tem UUID próprio, diferente da [`Conta`] do cartão (`conta`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cartao {
    pub id: Id,
    pub versao: Versao,
    /// A [`Conta`] (produto financeiro) do cartão.
    pub conta: Id,
    pub final_do_numero: Option<String>,
    pub bandeira: Option<String>,
    pub principal: bool,
    pub titular: Option<String>,
    pub limite_de_credito: Option<Centavos>,
    pub dia_de_fechamento: Option<i32>,
    pub dia_de_vencimento: Option<i32>,
    pub ativo: bool,
}

/// `statement_cycles`. A fatura aponta para o [`Cartao`], não para a conta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fatura {
    pub id: Id,
    pub versao: Versao,
    pub cartao: Id,
    pub mes_de_referencia: Data,
    pub inicio: Data,
    pub fim: Data,
    pub vencimento: Data,
    pub total: Option<Centavos>,
    pub pago: Option<Centavos>,
    pub estado: EstadoDaFatura,
}

/// `statement_items`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDeFatura {
    pub id: Id,
    pub versao: Versao,
    pub fatura: Id,
    pub movimentacao: Option<Id>,
    pub descricao: Option<String>,
    pub valor: Centavos,
    pub data: Option<Data>,
    pub parcela: Option<i32>,
    pub total_de_parcelas: Option<i32>,
    pub fornecedor: Option<Id>,
}

/// `transactions`, com as tags dela (`transaction_tags`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Movimentacao {
    pub id: Id,
    /// A versão de base de qualquer alteração desta movimentação.
    pub versao: Versao,
    pub conta: Id,
    pub tipo: TipoDeMovimentacao,
    pub valor: Centavos,
    pub descricao: Option<String>,
    /// A data do fato (`event_date`).
    pub data: Data,
    pub competencia: Option<Data>,
    pub fatura: Option<Id>,
    pub parcela_de_divida: Option<Id>,
    pub categoria: Option<Id>,
    pub prioridade: Option<Prioridade>,
    pub confirmada: bool,
    pub notas: Option<String>,
    pub origem: OrigemDaMovimentacao,
    pub ocorrencia: Option<Id>,
    pub fornecedor: Option<Id>,
    pub documento: Option<Id>,
    pub tags: Vec<Id>,
    pub criada_em: Instante,
}

/// `transfers`: movimento entre contas, fora de `transactions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transferencia {
    pub id: Id,
    pub versao: Versao,
    pub origem: Id,
    pub destino: Id,
    pub valor: Centavos,
    pub descricao: Option<String>,
    pub data: Data,
    pub competencia: Option<Data>,
    pub confirmada: bool,
    pub notas: Option<String>,
}

/// `transaction_allocations`. Sem `updated_at` no banco, então sem versão própria: a
/// versão de base de um rateio é a da movimentação.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rateio {
    pub id: Id,
    pub movimentacao: Id,
    pub centro: Id,
    pub valor: Centavos,
    pub nota: Option<String>,
    pub criado_em: Instante,
}

/// `categories`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Categoria {
    pub id: Id,
    pub versao: Versao,
    pub nome: String,
    pub pai: Option<Id>,
    pub icone: Option<String>,
    pub cor: Option<String>,
    pub ordem: Option<i32>,
    pub ativa: bool,
}

/// `tags`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: Id,
    pub versao: Versao,
    pub nome: String,
    pub cor: Option<String>,
    pub influencia_prioridade: bool,
    pub prioridade_sugerida: Option<Prioridade>,
}

/// `financial_centers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Centro {
    pub id: Id,
    pub versao: Versao,
    pub nome: String,
    pub tipo: TipoDeCentro,
    pub descricao: Option<String>,
    pub estado: EstadoDoCentro,
    pub icone: Option<String>,
    pub cor: Option<String>,
}

/// `suppliers`, com as tags dele (`supplier_tags`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fornecedor {
    pub id: Id,
    pub versao: Versao,
    pub nome: String,
    pub nome_fantasia: Option<String>,
    pub razao_social: Option<String>,
    /// CNPJ ou CPF do fornecedor, como cadastrado.
    pub documento: Option<String>,
    pub tipo: TipoDeFornecedor,
    pub site: Option<String>,
    pub notas: Option<String>,
    pub instituicao: Option<Id>,
    pub ativo: bool,
    pub ordem: Option<i32>,
    pub tags: Vec<Id>,
}

/// `supplier_aliases`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Apelido {
    pub id: Id,
    pub versao: Versao,
    pub fornecedor: Id,
    pub nome: String,
    pub tipo: TipoDeApelido,
    pub ativo: bool,
    pub valido_desde: Option<Data>,
    pub valido_ate: Option<Data>,
    pub notas: Option<String>,
}

/// `supplier_contracts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contrato {
    pub id: Id,
    pub versao: Versao,
    pub fornecedor: Id,
    pub tipo: TipoDeContrato,
    pub identificador: Option<String>,
    pub rotulo: Option<String>,
    pub ativo: bool,
    pub inicio: Option<Data>,
    pub fim: Option<Data>,
    pub notas: Option<String>,
}

/// `liabilities`, com as tags dela (`liability_tags`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Divida {
    pub id: Id,
    pub versao: Versao,
    pub conta: Id,
    pub nome: String,
    pub tipo: TipoDeDivida,
    pub valor_original: Centavos,
    pub saldo_devedor: Centavos,
    /// A taxa de juros como decimal exato em texto (`numeric(8,6)`, ex.: `"0.012500"`
    /// é 1,25%), no período de `periodo_da_taxa`. Não é dinheiro: fica em texto para a
    /// estação decidir a precisão.
    pub taxa_de_juros: Option<String>,
    pub periodo_da_taxa: Option<PeriodoDaTaxa>,
    pub sistema_de_amortizacao: Option<SistemaDeAmortizacao>,
    pub total_de_parcelas: Option<i32>,
    pub parcelas_pagas: i32,
    pub inicio: Option<Data>,
    pub fim: Option<Data>,
    pub estado: EstadoDaDivida,
    pub fornecedor: Option<Id>,
    pub tags: Vec<Id>,
}

/// `liability_installments`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parcela {
    pub id: Id,
    pub versao: Versao,
    pub divida: Id,
    pub numero: i32,
    pub vencimento: Data,
    pub total: Centavos,
    pub principal: Option<Centavos>,
    pub juros: Option<Centavos>,
    pub seguro: Option<Centavos>,
    pub tarifa: Option<Centavos>,
    pub pago: Centavos,
    pub data_do_pagamento: Option<Data>,
    pub estado: EstadoDaParcela,
}

/// `recurring_templates`, com as tags dele (`recurring_template_tags`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recorrente {
    pub id: Id,
    pub versao: Versao,
    pub conta: Option<Id>,
    pub nome: String,
    pub tipo: TipoDeRecorrente,
    pub valor: Option<Centavos>,
    pub valor_variavel: bool,
    pub frequencia: Frequencia,
    pub dia_do_mes: Option<i32>,
    pub intervalo_em_dias: Option<i32>,
    pub categoria: Option<Id>,
    pub prioridade: Option<Prioridade>,
    pub inicio: Option<Data>,
    pub fim: Option<Data>,
    pub ativo: bool,
    pub notas: Option<String>,
    pub fornecedor: Option<Id>,
    pub tags: Vec<Id>,
}

/// `recurring_instances`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ocorrencia {
    pub id: Id,
    pub versao: Versao,
    pub recorrente: Id,
    pub data_prevista: Data,
    pub valor_previsto: Option<Centavos>,
    pub valor_real: Option<Centavos>,
    pub estado: EstadoDaOcorrencia,
    pub data_do_pagamento: Option<Data>,
    pub movimentacao: Option<Id>,
    pub notas: Option<String>,
}

/// `source_documents`: só metadados. O caminho no Blob e o caminho local da máquina de
/// origem ficam no servidor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Documento {
    pub id: Id,
    /// `updated_at` aceita nulo nesta tabela.
    pub versao: Option<Versao>,
    pub origem: OrigemDoDocumento,
    pub nome_do_arquivo: String,
    pub tipo_mime: Option<String>,
    pub tamanho_em_bytes: Option<i64>,
    /// O estado bruto do servidor (`new`, `processed`…), texto livre no banco.
    pub estado: String,
    /// SHA-256 do conteúdo, em hexadecimal, quando o servidor o calculou.
    pub hash_do_conteudo: Option<String>,
    pub tipo_de_documento: Option<String>,
    pub fornecedor: Option<Id>,
    pub nome_do_fornecedor_bruto: Option<String>,
    pub assunto_do_email: Option<String>,
    pub remetente_do_email: Option<String>,
    pub recebido_em: Option<Instante>,
    pub criado_em: Option<Instante>,
}
