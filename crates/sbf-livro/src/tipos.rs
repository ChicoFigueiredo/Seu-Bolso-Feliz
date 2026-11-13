//! Os tipos do livro-caixa, um por tabela do Neon que as telas de Dinheiro leem.
//!
//! Os nomes seguem o `CONTEXT.md`; o comentário de cada tipo diz de que tabela ele vem.
//! Os enums serializam com o código do banco (`income`, `credit_card`…), para a conversão
//! das linhas do contrato ser direta. Dinheiro é [`Centavos`]; datas são [`Data`].

use serde::{Deserialize, Serialize};

use crate::ciclo::DiaDeCorte;
use crate::data::Data;
use crate::dinheiro::{Centavos, Taxa};

/// O identificador de uma linha do livro-caixa: o UUID do Neon, em texto.
pub type Id = String;

/// Declara um enum com o código do banco de cada variante, `codigo()` e `de_codigo()`.
macro_rules! codigos {
    ($(#[$doc:meta])* $nome:ident { $($(#[$vdoc:meta])* $variante:ident = $codigo:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $nome {
            $($(#[$vdoc])* #[serde(rename = $codigo)] $variante,)+
        }

        impl $nome {
            pub const TODOS: &'static [$nome] = &[$($nome::$variante),+];

            /// O código gravado no banco.
            pub fn codigo(self) -> &'static str {
                match self { $($nome::$variante => $codigo),+ }
            }

            /// O valor de um código do banco; `None` se o código não existe.
            pub fn de_codigo(codigo: &str) -> Option<$nome> {
                match codigo { $($codigo => Some($nome::$variante),)+ _ => None }
            }
        }
    };
}

codigos! {
    /// `transactions.type`.
    TipoDeMovimentacao {
        Receita = "income",
        Despesa = "expense",
        Estorno = "refund",
        /// Correção de saldo. Fora do resultado, do centro e do não alocado (grill Q15).
        Ajuste = "adjustment",
        Juros = "interest_charge",
        Tarifa = "fee",
        PagamentoDeFatura = "statement_payment",
        PagamentoDeDivida = "liability_payment",
    }
}

codigos! {
    /// `financial_products.type`.
    TipoDeConta {
        ContaCorrente = "checking_account",
        Poupanca = "savings_account",
        CartaoDeCredito = "credit_card",
        ChequeEspecial = "overdraft",
        Emprestimo = "personal_loan",
        Financiamento = "mortgage",
        Investimento = "investment",
        Outra = "other",
    }
}

codigos! {
    /// `statement_cycles.status`.
    StatusDaFatura {
        Aberta = "open",
        Fechada = "closed",
        Paga = "paid",
        Parcial = "partial",
        Vencida = "overdue",
    }
}

codigos! {
    /// `liabilities.status`.
    StatusDaDivida {
        Ativa = "active",
        Quitada = "paid_off",
        Renegociada = "renegotiated",
        Inadimplente = "defaulted",
    }
}

codigos! {
    /// `liability_installments.status`.
    StatusDaParcela {
        Pendente = "pending",
        Paga = "paid",
        Parcial = "partial",
        Vencida = "overdue",
        Dispensada = "waived",
    }
}

codigos! {
    /// `recurring_templates.frequency`.
    Frequencia {
        Mensal = "monthly",
        Semanal = "weekly",
        Quinzenal = "biweekly",
        Trimestral = "quarterly",
        Anual = "annual",
        /// A cada `intervalo_dias` dias.
        Personalizada = "custom",
    }
}

codigos! {
    /// `recurring_instances.status`.
    StatusDaOcorrencia {
        Pendente = "pending",
        Paga = "paid",
        Parcial = "partial",
        Pulada = "skipped",
        Vencida = "overdue",
        Cancelada = "cancelled",
    }
}

codigos! {
    /// `liabilities.amortization_system`.
    SistemaDeAmortizacao {
        Sac = "sac",
        Price = "price",
        Misto = "mixed",
        Outro = "other",
        Nenhum = "none",
    }
}

codigos! {
    /// `financial_centers.status`.
    StatusDoCentro {
        Ativo = "active",
        Arquivado = "archived",
    }
}

/// Movimentação (`transactions`): um fato financeiro que ocorreu ou vai ocorrer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movimentacao {
    pub id: Id,
    /// A conta (produto financeiro) por onde o dinheiro passou. Compra no cartão aponta
    /// para o produto do cartão.
    pub conta_id: Id,
    pub tipo: TipoDeMovimentacao,
    /// Como gravado; as regras usam o módulo.
    pub valor: Centavos,
    pub descricao: Option<String>,
    /// `event_date`: quando o dinheiro se moveu (ou a compra no cartão foi feita).
    pub data: Data,
    pub data_competencia: Option<Data>,
    /// `statement_cycle_id`: compra dentro de uma fatura de cartão.
    pub fatura_id: Option<Id>,
    /// `liability_installment_id`: pagamento de uma parcela de dívida.
    pub parcela_id: Option<Id>,
    pub categoria_id: Option<Id>,
    pub fornecedor_id: Option<Id>,
    /// `recurring_instance_id`.
    pub ocorrencia_id: Option<Id>,
    /// `source_document_id`.
    pub documento_id: Option<Id>,
    pub confirmada: bool,
    /// `updated_at`: a versão de base de uma alteração (grill Q6).
    pub versao: Option<String>,
}

/// Transferência entre contas (`transfers`). Nunca é gasto nem receita.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transferencia {
    pub id: Id,
    pub conta_origem_id: Id,
    pub conta_destino_id: Id,
    pub valor: Centavos,
    pub descricao: Option<String>,
    pub data: Data,
    pub confirmada: bool,
}

/// Instituição (`institutions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instituicao {
    pub id: Id,
    pub nome: String,
    /// `bank`, `fintech`, `broker`, `other`.
    pub tipo: String,
    pub ativa: bool,
}

/// Conta, no sentido largo de produto financeiro (`financial_products`): conta corrente,
/// poupança, cartão, empréstimo…
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conta {
    pub id: Id,
    pub instituicao_id: Id,
    pub nome: String,
    pub tipo: TipoDeConta,
    /// `current_balance`: **gravado** (digitado ou vindo da Pluggy), não derivado.
    pub saldo: Centavos,
    pub limite: Option<Centavos>,
    pub ativa: bool,
    pub ordem: Option<i32>,
}

/// Cartão (`cards`). Tem UUID próprio, diferente do produto (`conta_id`); a fatura
/// aponta para o cartão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cartao {
    pub id: Id,
    /// `financial_product_id`.
    pub conta_id: Id,
    pub final_do_numero: Option<String>,
    pub bandeira: Option<String>,
    pub limite: Option<Centavos>,
    pub dia_de_fechamento: Option<u8>,
    pub dia_de_vencimento: Option<u8>,
    pub ativo: bool,
}

/// Fatura de cartão (`statement_cycles`). O período dela não é um ciclo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fatura {
    pub id: Id,
    pub cartao_id: Id,
    pub mes_de_referencia: Data,
    pub inicio: Data,
    /// `cycle_end_date`: o fechamento.
    pub fechamento: Data,
    pub vencimento: Data,
    pub total: Centavos,
    pub pago: Centavos,
    pub status: StatusDaFatura,
}

impl Fatura {
    /// O que falta pagar: total − pago.
    pub fn em_aberto(&self) -> Centavos {
        self.total - self.pago
    }
}

/// Item de fatura (`statement_items`): uma linha da fatura. Pode já ter virado
/// movimentação (`movimentacao_id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDeFatura {
    pub id: Id,
    pub fatura_id: Id,
    pub movimentacao_id: Option<Id>,
    pub descricao: Option<String>,
    pub valor: Centavos,
    /// `transaction_date`: a data da compra.
    pub data_da_compra: Option<Data>,
    pub parcela: Option<i32>,
    pub total_de_parcelas: Option<i32>,
    pub fornecedor_id: Option<Id>,
}

/// Dívida (`liabilities`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divida {
    pub id: Id,
    pub conta_id: Id,
    pub nome: String,
    /// `personal_loan`, `mortgage`, `overdraft`, `installment_plan`, `other`.
    pub tipo: String,
    pub valor_original: Centavos,
    pub saldo_devedor: Centavos,
    pub taxa: Option<Taxa>,
    /// `monthly` ou `annual`.
    pub tipo_de_taxa: Option<String>,
    pub sistema: Option<SistemaDeAmortizacao>,
    pub total_de_parcelas: Option<i32>,
    pub parcelas_pagas: i32,
    pub inicio: Option<Data>,
    pub fim: Option<Data>,
    pub status: StatusDaDivida,
    pub fornecedor_id: Option<Id>,
}

/// Parcela de dívida (`liability_installments`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parcela {
    pub id: Id,
    pub divida_id: Id,
    pub numero: i32,
    pub vencimento: Data,
    pub total: Centavos,
    pub principal: Option<Centavos>,
    pub juros: Option<Centavos>,
    pub seguro: Option<Centavos>,
    pub tarifa: Option<Centavos>,
    pub pago: Centavos,
    pub data_do_pagamento: Option<Data>,
    pub status: StatusDaParcela,
}

/// Recorrente (`recurring_templates`): algo que se repete em intervalo previsível.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorrente {
    pub id: Id,
    pub nome: String,
    /// Só `Receita`, `Despesa`, `PagamentoDeDivida` ou `PagamentoDeFatura`.
    pub tipo: TipoDeMovimentacao,
    pub conta_id: Option<Id>,
    pub valor: Option<Centavos>,
    pub valor_variavel: bool,
    pub frequencia: Frequencia,
    pub dia_do_mes: Option<u8>,
    /// `custom_interval_days`, para a frequência personalizada.
    pub intervalo_dias: Option<i32>,
    pub categoria_id: Option<Id>,
    pub fornecedor_id: Option<Id>,
    pub inicia_em: Option<Data>,
    pub termina_em: Option<Data>,
    /// Pausado ou excluído é `false` (o banco não distingue).
    pub ativo: bool,
}

/// Ocorrência de um recorrente (`recurring_instances`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ocorrencia {
    pub id: Id,
    pub recorrente_id: Id,
    pub data_esperada: Data,
    pub valor_esperado: Option<Centavos>,
    pub valor_real: Option<Centavos>,
    pub status: StatusDaOcorrencia,
    pub data_do_pagamento: Option<Data>,
    /// Já virou movimentação: daí em diante, conta a movimentação.
    pub movimentacao_id: Option<Id>,
}

/// Centro (`financial_centers`): para quem ou para qual projeto o dinheiro foi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Centro {
    pub id: Id,
    pub nome: String,
    /// `projeto`, `pessoa`, `negocio`.
    pub tipo: String,
    pub status: StatusDoCentro,
    pub descricao: Option<String>,
    pub icone: Option<String>,
    pub cor: Option<String>,
}

/// Rateio (`transaction_allocations`): a parte de uma movimentação que foi para um centro.
/// Guarda valor, nunca percentual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rateio {
    pub id: Id,
    pub movimentacao_id: Id,
    pub centro_id: Id,
    pub valor: Centavos,
    pub nota: Option<String>,
}

/// Categoria (`categories`): a natureza do gasto ou da receita.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Categoria {
    pub id: Id,
    pub nome: String,
    pub pai_id: Option<Id>,
    pub ativa: bool,
}

/// Tag (`tags`): rótulo livre, sem valor e sem saldo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: Id,
    pub nome: String,
}

/// Fornecedor (`suppliers`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fornecedor {
    pub id: Id,
    pub nome_fantasia: Option<String>,
    pub razao_social: Option<String>,
    pub ativo: bool,
}

impl Fornecedor {
    /// O nome que a tela mostra: o fantasia, senão a razão social.
    pub fn nome(&self) -> &str {
        self.nome_fantasia
            .as_deref()
            .or(self.razao_social.as_deref())
            .unwrap_or("Sem nome")
    }
}

/// O livro-caixa de um dono, inteiro (grill Q10): o que o cache da estação guarda e o que
/// as regras recebem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LivroCaixa {
    /// `user_financial_preferences.financial_cycle_start_day`, ou 20 (grill Q14).
    pub corte: DiaDeCorte,
    pub instituicoes: Vec<Instituicao>,
    pub contas: Vec<Conta>,
    pub cartoes: Vec<Cartao>,
    pub faturas: Vec<Fatura>,
    pub itens_de_fatura: Vec<ItemDeFatura>,
    pub movimentacoes: Vec<Movimentacao>,
    pub transferencias: Vec<Transferencia>,
    pub dividas: Vec<Divida>,
    pub parcelas: Vec<Parcela>,
    pub recorrentes: Vec<Recorrente>,
    pub ocorrencias: Vec<Ocorrencia>,
    pub centros: Vec<Centro>,
    pub rateios: Vec<Rateio>,
    pub categorias: Vec<Categoria>,
    pub tags: Vec<Tag>,
    /// `transaction_tags`: (movimentação, tag).
    pub tags_das_movimentacoes: Vec<(Id, Id)>,
    pub fornecedores: Vec<Fornecedor>,
    /// Documentos que esperam revisão (`ingestion_jobs` em `pending_review` ou `drafted`).
    /// Só a contagem: o documento não é do livro-caixa.
    pub documentos_a_revisar: u32,
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todo_codigo_do_banco_volta_ao_mesmo_valor() {
        for t in TipoDeMovimentacao::TODOS {
            assert_eq!(TipoDeMovimentacao::de_codigo(t.codigo()), Some(*t));
        }
        assert_eq!(TipoDeMovimentacao::TODOS.len(), 8);
        assert_eq!(
            TipoDeConta::de_codigo("credit_card"),
            Some(TipoDeConta::CartaoDeCredito)
        );
        assert_eq!(TipoDeMovimentacao::de_codigo("transfer"), None);
    }

    #[test]
    fn serializa_com_o_codigo_do_banco() {
        let json = serde_json::to_string(&TipoDeMovimentacao::PagamentoDeFatura).expect("json");
        assert_eq!(json, "\"statement_payment\"");
    }

    #[test]
    fn fornecedor_sem_fantasia_usa_a_razao_social() {
        let f = Fornecedor {
            id: "f".into(),
            nome_fantasia: None,
            razao_social: Some("Padaria Exemplo Ltda".into()),
            ativo: true,
        };
        assert_eq!(f.nome(), "Padaria Exemplo Ltda");
    }
}
