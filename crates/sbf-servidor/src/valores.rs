//! Os valores fechados do banco (os `CHECK` de `schema.sql` + migrações) traduzidos para
//! os enums do contrato. Um valor fora desta lista é erro de leitura, não silêncio.

use sbf_contrato::*;

pub fn tipo_de_instituicao(v: &str) -> Option<TipoDeInstituicao> {
    Some(match v {
        "bank" => TipoDeInstituicao::Banco,
        "credit_union" => TipoDeInstituicao::Cooperativa,
        "brokerage" => TipoDeInstituicao::Corretora,
        "fintech" => TipoDeInstituicao::Fintech,
        "broker" => TipoDeInstituicao::CorretoraAntiga,
        "other" => TipoDeInstituicao::Outra,
        _ => return None,
    })
}

pub fn tipo_de_conta(v: &str) -> Option<TipoDeConta> {
    Some(match v {
        "checking_account" => TipoDeConta::ContaCorrente,
        "savings_account" => TipoDeConta::Poupanca,
        "credit_card" => TipoDeConta::CartaoDeCredito,
        "overdraft" => TipoDeConta::ChequeEspecial,
        "personal_loan" => TipoDeConta::EmprestimoPessoal,
        "mortgage" => TipoDeConta::Financiamento,
        "investment" => TipoDeConta::Investimento,
        "other" => TipoDeConta::Outra,
        _ => return None,
    })
}

pub fn tipo_de_movimentacao(v: &str) -> Option<TipoDeMovimentacao> {
    Some(match v {
        "income" => TipoDeMovimentacao::Entrada,
        "expense" => TipoDeMovimentacao::Saida,
        "refund" => TipoDeMovimentacao::Estorno,
        "adjustment" => TipoDeMovimentacao::Ajuste,
        "interest_charge" => TipoDeMovimentacao::Juros,
        "fee" => TipoDeMovimentacao::Tarifa,
        "statement_payment" => TipoDeMovimentacao::PagamentoDeFatura,
        "liability_payment" => TipoDeMovimentacao::PagamentoDeDivida,
        _ => return None,
    })
}

pub fn origem_da_movimentacao(v: &str) -> Option<OrigemDaMovimentacao> {
    Some(match v {
        "manual" => OrigemDaMovimentacao::Manual,
        "import" => OrigemDaMovimentacao::Importacao,
        "recurring" => OrigemDaMovimentacao::Recorrente,
        "statement_link" => OrigemDaMovimentacao::Fatura,
        _ => return None,
    })
}

pub fn prioridade(v: &str) -> Option<Prioridade> {
    Some(match v {
        "essential" => Prioridade::Essencial,
        "high" => Prioridade::Alta,
        "medium" => Prioridade::Media,
        "low" => Prioridade::Baixa,
        "optional" => Prioridade::Opcional,
        _ => return None,
    })
}

pub fn estado_da_fatura(v: &str) -> Option<EstadoDaFatura> {
    Some(match v {
        "open" => EstadoDaFatura::Aberta,
        "closed" => EstadoDaFatura::Fechada,
        "paid" => EstadoDaFatura::Paga,
        "partial" => EstadoDaFatura::Parcial,
        "overdue" => EstadoDaFatura::Vencida,
        _ => return None,
    })
}

pub fn tipo_de_centro(v: &str) -> Option<TipoDeCentro> {
    Some(match v {
        "projeto" => TipoDeCentro::Projeto,
        "pessoa" => TipoDeCentro::Pessoa,
        "negocio" => TipoDeCentro::Negocio,
        _ => return None,
    })
}

pub fn estado_do_centro(v: &str) -> Option<EstadoDoCentro> {
    Some(match v {
        "active" => EstadoDoCentro::Ativo,
        "archived" => EstadoDoCentro::Arquivado,
        _ => return None,
    })
}

pub fn tipo_de_fornecedor(v: &str) -> Option<TipoDeFornecedor> {
    Some(match v {
        "company" => TipoDeFornecedor::Empresa,
        "individual" => TipoDeFornecedor::PessoaFisica,
        "government" => TipoDeFornecedor::Governo,
        "utility" => TipoDeFornecedor::Concessionaria,
        "telecom" => TipoDeFornecedor::Telecom,
        "saas" => TipoDeFornecedor::Software,
        "platform" => TipoDeFornecedor::Plataforma,
        "other" => TipoDeFornecedor::Outro,
        _ => return None,
    })
}

pub fn tipo_de_apelido(v: &str) -> Option<TipoDeApelido> {
    Some(match v {
        "former_name" => TipoDeApelido::NomeAntigo,
        "abbreviation" => TipoDeApelido::Abreviacao,
        "trade_name" => TipoDeApelido::NomeFantasia,
        "billing_name" => TipoDeApelido::NomeDeCobranca,
        "other" => TipoDeApelido::Outro,
        _ => return None,
    })
}

pub fn tipo_de_contrato(v: &str) -> Option<TipoDeContrato> {
    Some(match v {
        "service" => TipoDeContrato::Servico,
        "subscription" => TipoDeContrato::Assinatura,
        "utility" => TipoDeContrato::Consumo,
        "loan" => TipoDeContrato::Emprestimo,
        "insurance" => TipoDeContrato::Seguro,
        "maintenance" => TipoDeContrato::Manutencao,
        "other" => TipoDeContrato::Outro,
        _ => return None,
    })
}

pub fn tipo_de_divida(v: &str) -> Option<TipoDeDivida> {
    Some(match v {
        "personal_loan" => TipoDeDivida::EmprestimoPessoal,
        "mortgage" => TipoDeDivida::Financiamento,
        "overdraft" => TipoDeDivida::ChequeEspecial,
        "installment_plan" => TipoDeDivida::Parcelamento,
        "other" => TipoDeDivida::Outra,
        _ => return None,
    })
}

pub fn periodo_da_taxa(v: &str) -> Option<PeriodoDaTaxa> {
    Some(match v {
        "monthly" => PeriodoDaTaxa::Mensal,
        "annual" => PeriodoDaTaxa::Anual,
        _ => return None,
    })
}

pub fn sistema_de_amortizacao(v: &str) -> Option<SistemaDeAmortizacao> {
    Some(match v {
        "sac" => SistemaDeAmortizacao::Sac,
        "price" => SistemaDeAmortizacao::Price,
        "mixed" => SistemaDeAmortizacao::Misto,
        "other" => SistemaDeAmortizacao::Outro,
        "none" => SistemaDeAmortizacao::Nenhum,
        _ => return None,
    })
}

pub fn estado_da_divida(v: &str) -> Option<EstadoDaDivida> {
    Some(match v {
        "active" => EstadoDaDivida::Ativa,
        "paid_off" => EstadoDaDivida::Quitada,
        "renegotiated" => EstadoDaDivida::Renegociada,
        "defaulted" => EstadoDaDivida::Inadimplente,
        _ => return None,
    })
}

pub fn estado_da_parcela(v: &str) -> Option<EstadoDaParcela> {
    Some(match v {
        "pending" => EstadoDaParcela::Pendente,
        "paid" => EstadoDaParcela::Paga,
        "partial" => EstadoDaParcela::Parcial,
        "overdue" => EstadoDaParcela::Vencida,
        "waived" => EstadoDaParcela::Dispensada,
        _ => return None,
    })
}

pub fn tipo_de_recorrente(v: &str) -> Option<TipoDeRecorrente> {
    Some(match v {
        "income" => TipoDeRecorrente::Entrada,
        "expense" => TipoDeRecorrente::Saida,
        "liability_payment" => TipoDeRecorrente::PagamentoDeDivida,
        "statement_payment" => TipoDeRecorrente::PagamentoDeFatura,
        _ => return None,
    })
}

pub fn frequencia(v: &str) -> Option<Frequencia> {
    Some(match v {
        "monthly" => Frequencia::Mensal,
        "weekly" => Frequencia::Semanal,
        "biweekly" => Frequencia::Quinzenal,
        "quarterly" => Frequencia::Trimestral,
        "annual" => Frequencia::Anual,
        "custom" => Frequencia::Personalizada,
        _ => return None,
    })
}

pub fn estado_da_ocorrencia(v: &str) -> Option<EstadoDaOcorrencia> {
    Some(match v {
        "pending" => EstadoDaOcorrencia::Pendente,
        "paid" => EstadoDaOcorrencia::Paga,
        "partial" => EstadoDaOcorrencia::Parcial,
        "skipped" => EstadoDaOcorrencia::Pulada,
        "overdue" => EstadoDaOcorrencia::Vencida,
        "cancelled" => EstadoDaOcorrencia::Cancelada,
        _ => return None,
    })
}

pub fn origem_do_documento(v: &str) -> Option<OrigemDoDocumento> {
    Some(match v {
        "gmail" => OrigemDoDocumento::Gmail,
        "local_file" => OrigemDoDocumento::ArquivoLocal,
        "manual_upload" => OrigemDoDocumento::EnvioManual,
        _ => return None,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todo_tipo_de_movimentacao_do_check_tem_traducao() {
        for v in [
            "income",
            "expense",
            "refund",
            "adjustment",
            "interest_charge",
            "fee",
            "statement_payment",
            "liability_payment",
        ] {
            assert!(tipo_de_movimentacao(v).is_some(), "{v}");
        }
        assert_eq!(tipo_de_movimentacao("transfer"), None);
    }

    #[test]
    fn os_seis_tipos_de_instituicao_da_migracao_0003() {
        for v in [
            "bank",
            "credit_union",
            "brokerage",
            "fintech",
            "broker",
            "other",
        ] {
            assert!(tipo_de_instituicao(v).is_some(), "{v}");
        }
    }
}
