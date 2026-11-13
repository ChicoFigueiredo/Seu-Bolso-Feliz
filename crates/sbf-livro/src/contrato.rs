//! De linhas do contrato HTTP (`sbf-contrato`) para os tipos do livro.
//!
//! O servidor devolve cada coleção inteira como linhas ([`LinhasDaColecao`]); a estação
//! junta as coleções do cache num [`LivroCaixa`] com [`livro_das_linhas`] e calcula os
//! números com as regras deste crate (grill Q8).
//!
//! - Os enums do contrato (nomes em português) voltam aos códigos do banco que os tipos
//!   do livro usam (`Entrada` → `income`…).
//! - `saldo_atual`, `total` e `pago` nulos viram zero, o `DEFAULT 0` do banco.
//! - A taxa de juros vem em texto exato (`"0.012500"`) e vira [`Taxa`] sem ponto flutuante.
//! - Data inválida, taxa inválida ou dia fora do mês é erro: linha que não se lê não entra
//!   calada num total.

use sbf_contrato as c;

use crate::ciclo::DiaDeCorte;
use crate::data::Data;
use crate::dinheiro::{Centavos, ErroDeValor, Taxa};
use crate::tipos::{
    Cartao, Categoria, Centro, Conta, Divida, Fatura, Fornecedor, Frequencia, Instituicao,
    ItemDeFatura, LivroCaixa, Movimentacao, Ocorrencia, Parcela, Rateio, Recorrente,
    SistemaDeAmortizacao, StatusDaDivida, StatusDaFatura, StatusDaOcorrencia, StatusDaParcela,
    StatusDoCentro, Tag, TipoDeConta, TipoDeMovimentacao, Transferencia,
};

pub use sbf_contrato::LinhasDaColecao;

/// Uma linha do contrato que não vira tipo do livro.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeConversao {
    #[error("{colecao} {id}: data inválida {texto:?}")]
    Data {
        colecao: &'static str,
        id: String,
        texto: String,
    },
    #[error("{colecao} {id}: taxa de juros inválida: {erro}")]
    Taxa {
        colecao: &'static str,
        id: String,
        erro: ErroDeValor,
    },
    #[error("{colecao} {id}: dia fora do mês: {dia}")]
    Dia {
        colecao: &'static str,
        id: String,
        dia: i32,
    },
    #[error("dia de corte fora de 1..=31: {0}")]
    DiaDeCorte(u8),
}

type Resultado<T> = Result<T, ErroDeConversao>;

fn data(colecao: &'static str, id: &c::Id, texto: &str) -> Resultado<Data> {
    Data::de_iso(texto).ok_or_else(|| ErroDeConversao::Data {
        colecao,
        id: id.to_string(),
        texto: texto.to_owned(),
    })
}

fn data_opt(colecao: &'static str, id: &c::Id, texto: Option<&str>) -> Resultado<Option<Data>> {
    texto.map(|t| data(colecao, id, t)).transpose()
}

fn dia(colecao: &'static str, id: &c::Id, dia: Option<i32>) -> Resultado<Option<u8>> {
    dia.map(|d| {
        u8::try_from(d)
            .ok()
            .filter(|d| (1..=31).contains(d))
            .ok_or_else(|| ErroDeConversao::Dia {
                colecao,
                id: id.to_string(),
                dia: d,
            })
    })
    .transpose()
}

fn id(i: &c::Id) -> String {
    i.to_string()
}

fn id_opt(i: Option<&c::Id>) -> Option<String> {
    i.map(id)
}

fn centavos(v: Option<c::Centavos>) -> Centavos {
    Centavos(v.unwrap_or(0))
}

/// O tipo da movimentação do contrato no código do banco.
pub fn tipo_de_movimentacao(t: c::TipoDeMovimentacao) -> TipoDeMovimentacao {
    use TipoDeMovimentacao as L;
    use c::TipoDeMovimentacao as C;
    match t {
        C::Entrada => L::Receita,
        C::Saida => L::Despesa,
        C::Estorno => L::Estorno,
        C::Ajuste => L::Ajuste,
        C::Juros => L::Juros,
        C::Tarifa => L::Tarifa,
        C::PagamentoDeFatura => L::PagamentoDeFatura,
        C::PagamentoDeDivida => L::PagamentoDeDivida,
    }
}

fn tipo_de_conta(t: c::TipoDeConta) -> TipoDeConta {
    use TipoDeConta as L;
    use c::TipoDeConta as C;
    match t {
        C::ContaCorrente => L::ContaCorrente,
        C::Poupanca => L::Poupanca,
        C::CartaoDeCredito => L::CartaoDeCredito,
        C::ChequeEspecial => L::ChequeEspecial,
        C::EmprestimoPessoal => L::Emprestimo,
        C::Financiamento => L::Financiamento,
        C::Investimento => L::Investimento,
        C::Outra => L::Outra,
    }
}

fn tipo_de_instituicao(t: c::TipoDeInstituicao) -> &'static str {
    use c::TipoDeInstituicao as C;
    match t {
        C::Banco => "bank",
        C::Cooperativa => "credit_union",
        C::Corretora => "brokerage",
        C::Fintech => "fintech",
        C::CorretoraAntiga => "broker",
        C::Outra => "other",
    }
}

fn tipo_de_centro(t: c::TipoDeCentro) -> &'static str {
    match t {
        c::TipoDeCentro::Projeto => "projeto",
        c::TipoDeCentro::Pessoa => "pessoa",
        c::TipoDeCentro::Negocio => "negocio",
    }
}

fn tipo_de_divida(t: c::TipoDeDivida) -> &'static str {
    use c::TipoDeDivida as C;
    match t {
        C::EmprestimoPessoal => "personal_loan",
        C::Financiamento => "mortgage",
        C::ChequeEspecial => "overdraft",
        C::Parcelamento => "installment_plan",
        C::Outra => "other",
    }
}

impl From<&c::Instituicao> for Instituicao {
    fn from(l: &c::Instituicao) -> Self {
        Instituicao {
            id: id(&l.id),
            nome: l.nome.clone(),
            tipo: tipo_de_instituicao(l.tipo).to_owned(),
            ativa: l.ativa,
        }
    }
}

impl From<&c::Conta> for Conta {
    fn from(l: &c::Conta) -> Self {
        Conta {
            id: id(&l.id),
            instituicao_id: id(&l.instituicao),
            nome: l.nome.clone(),
            tipo: tipo_de_conta(l.tipo),
            saldo: centavos(l.saldo_atual),
            limite: l.limite_de_credito.map(Centavos),
            ativa: l.ativa,
            ordem: l.ordem,
        }
    }
}

impl TryFrom<&c::Cartao> for Cartao {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Cartao) -> Resultado<Self> {
        Ok(Cartao {
            id: id(&l.id),
            conta_id: id(&l.conta),
            final_do_numero: l.final_do_numero.clone(),
            bandeira: l.bandeira.clone(),
            limite: l.limite_de_credito.map(Centavos),
            dia_de_fechamento: dia("cartões", &l.id, l.dia_de_fechamento)?,
            dia_de_vencimento: dia("cartões", &l.id, l.dia_de_vencimento)?,
            ativo: l.ativo,
        })
    }
}

impl TryFrom<&c::Fatura> for Fatura {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Fatura) -> Resultado<Self> {
        let d = |t: &str| data("faturas", &l.id, t);
        Ok(Fatura {
            id: id(&l.id),
            cartao_id: id(&l.cartao),
            mes_de_referencia: d(&l.mes_de_referencia)?,
            inicio: d(&l.inicio)?,
            fechamento: d(&l.fim)?,
            vencimento: d(&l.vencimento)?,
            total: centavos(l.total),
            pago: centavos(l.pago),
            status: match l.estado {
                c::EstadoDaFatura::Aberta => StatusDaFatura::Aberta,
                c::EstadoDaFatura::Fechada => StatusDaFatura::Fechada,
                c::EstadoDaFatura::Paga => StatusDaFatura::Paga,
                c::EstadoDaFatura::Parcial => StatusDaFatura::Parcial,
                c::EstadoDaFatura::Vencida => StatusDaFatura::Vencida,
            },
        })
    }
}

impl TryFrom<&c::ItemDeFatura> for ItemDeFatura {
    type Error = ErroDeConversao;
    fn try_from(l: &c::ItemDeFatura) -> Resultado<Self> {
        Ok(ItemDeFatura {
            id: id(&l.id),
            fatura_id: id(&l.fatura),
            movimentacao_id: id_opt(l.movimentacao.as_ref()),
            descricao: l.descricao.clone(),
            valor: Centavos(l.valor),
            data_da_compra: data_opt("itens de fatura", &l.id, l.data.as_deref())?,
            parcela: l.parcela,
            total_de_parcelas: l.total_de_parcelas,
            fornecedor_id: id_opt(l.fornecedor.as_ref()),
        })
    }
}

impl TryFrom<&c::Movimentacao> for Movimentacao {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Movimentacao) -> Resultado<Self> {
        Ok(Movimentacao {
            id: id(&l.id),
            conta_id: id(&l.conta),
            tipo: tipo_de_movimentacao(l.tipo),
            valor: Centavos(l.valor),
            descricao: l.descricao.clone(),
            data: data("movimentações", &l.id, &l.data)?,
            data_competencia: data_opt("movimentações", &l.id, l.competencia.as_deref())?,
            fatura_id: id_opt(l.fatura.as_ref()),
            parcela_id: id_opt(l.parcela_de_divida.as_ref()),
            categoria_id: id_opt(l.categoria.as_ref()),
            fornecedor_id: id_opt(l.fornecedor.as_ref()),
            ocorrencia_id: id_opt(l.ocorrencia.as_ref()),
            documento_id: id_opt(l.documento.as_ref()),
            confirmada: l.confirmada,
            versao: Some(l.versao.0.clone()),
        })
    }
}

impl TryFrom<&c::Transferencia> for Transferencia {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Transferencia) -> Resultado<Self> {
        Ok(Transferencia {
            id: id(&l.id),
            conta_origem_id: id(&l.origem),
            conta_destino_id: id(&l.destino),
            valor: Centavos(l.valor),
            descricao: l.descricao.clone(),
            data: data("transferências", &l.id, &l.data)?,
            confirmada: l.confirmada,
        })
    }
}

impl From<&c::Rateio> for Rateio {
    fn from(l: &c::Rateio) -> Self {
        Rateio {
            id: id(&l.id),
            movimentacao_id: id(&l.movimentacao),
            centro_id: id(&l.centro),
            valor: Centavos(l.valor),
            nota: l.nota.clone(),
        }
    }
}

impl From<&c::Categoria> for Categoria {
    fn from(l: &c::Categoria) -> Self {
        Categoria {
            id: id(&l.id),
            nome: l.nome.clone(),
            pai_id: id_opt(l.pai.as_ref()),
            ativa: l.ativa,
        }
    }
}

impl From<&c::Tag> for Tag {
    fn from(l: &c::Tag) -> Self {
        Tag {
            id: id(&l.id),
            nome: l.nome.clone(),
        }
    }
}

impl From<&c::Centro> for Centro {
    fn from(l: &c::Centro) -> Self {
        Centro {
            id: id(&l.id),
            nome: l.nome.clone(),
            tipo: tipo_de_centro(l.tipo).to_owned(),
            status: match l.estado {
                c::EstadoDoCentro::Ativo => StatusDoCentro::Ativo,
                c::EstadoDoCentro::Arquivado => StatusDoCentro::Arquivado,
            },
            descricao: l.descricao.clone(),
            icone: l.icone.clone(),
            cor: l.cor.clone(),
        }
    }
}

impl From<&c::Fornecedor> for Fornecedor {
    fn from(l: &c::Fornecedor) -> Self {
        Fornecedor {
            id: id(&l.id),
            nome: l.nome.clone(),
            nome_fantasia: l.nome_fantasia.clone(),
            razao_social: l.razao_social.clone(),
            ativo: l.ativo,
        }
    }
}

impl TryFrom<&c::Divida> for Divida {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Divida) -> Resultado<Self> {
        let taxa = l
            .taxa_de_juros
            .as_deref()
            .map(Taxa::de_texto)
            .transpose()
            .map_err(|erro| ErroDeConversao::Taxa {
                colecao: "dívidas",
                id: id(&l.id),
                erro,
            })?;
        Ok(Divida {
            id: id(&l.id),
            conta_id: id(&l.conta),
            nome: l.nome.clone(),
            tipo: tipo_de_divida(l.tipo).to_owned(),
            valor_original: Centavos(l.valor_original),
            saldo_devedor: Centavos(l.saldo_devedor),
            taxa,
            tipo_de_taxa: l.periodo_da_taxa.map(|p| {
                match p {
                    c::PeriodoDaTaxa::Mensal => "monthly",
                    c::PeriodoDaTaxa::Anual => "annual",
                }
                .to_owned()
            }),
            sistema: l.sistema_de_amortizacao.map(|s| match s {
                c::SistemaDeAmortizacao::Sac => SistemaDeAmortizacao::Sac,
                c::SistemaDeAmortizacao::Price => SistemaDeAmortizacao::Price,
                c::SistemaDeAmortizacao::Misto => SistemaDeAmortizacao::Misto,
                c::SistemaDeAmortizacao::Outro => SistemaDeAmortizacao::Outro,
                c::SistemaDeAmortizacao::Nenhum => SistemaDeAmortizacao::Nenhum,
            }),
            total_de_parcelas: l.total_de_parcelas,
            parcelas_pagas: l.parcelas_pagas,
            inicio: data_opt("dívidas", &l.id, l.inicio.as_deref())?,
            fim: data_opt("dívidas", &l.id, l.fim.as_deref())?,
            status: match l.estado {
                c::EstadoDaDivida::Ativa => StatusDaDivida::Ativa,
                c::EstadoDaDivida::Quitada => StatusDaDivida::Quitada,
                c::EstadoDaDivida::Renegociada => StatusDaDivida::Renegociada,
                c::EstadoDaDivida::Inadimplente => StatusDaDivida::Inadimplente,
            },
            fornecedor_id: id_opt(l.fornecedor.as_ref()),
        })
    }
}

impl TryFrom<&c::Parcela> for Parcela {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Parcela) -> Resultado<Self> {
        Ok(Parcela {
            id: id(&l.id),
            divida_id: id(&l.divida),
            numero: l.numero,
            vencimento: data("parcelas", &l.id, &l.vencimento)?,
            total: Centavos(l.total),
            principal: l.principal.map(Centavos),
            juros: l.juros.map(Centavos),
            seguro: l.seguro.map(Centavos),
            tarifa: l.tarifa.map(Centavos),
            pago: Centavos(l.pago),
            data_do_pagamento: data_opt("parcelas", &l.id, l.data_do_pagamento.as_deref())?,
            status: match l.estado {
                c::EstadoDaParcela::Pendente => StatusDaParcela::Pendente,
                c::EstadoDaParcela::Paga => StatusDaParcela::Paga,
                c::EstadoDaParcela::Parcial => StatusDaParcela::Parcial,
                c::EstadoDaParcela::Vencida => StatusDaParcela::Vencida,
                c::EstadoDaParcela::Dispensada => StatusDaParcela::Dispensada,
            },
        })
    }
}

impl TryFrom<&c::Recorrente> for Recorrente {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Recorrente) -> Resultado<Self> {
        Ok(Recorrente {
            id: id(&l.id),
            nome: l.nome.clone(),
            tipo: match l.tipo {
                c::TipoDeRecorrente::Entrada => TipoDeMovimentacao::Receita,
                c::TipoDeRecorrente::Saida => TipoDeMovimentacao::Despesa,
                c::TipoDeRecorrente::PagamentoDeDivida => TipoDeMovimentacao::PagamentoDeDivida,
                c::TipoDeRecorrente::PagamentoDeFatura => TipoDeMovimentacao::PagamentoDeFatura,
            },
            conta_id: id_opt(l.conta.as_ref()),
            valor: l.valor.map(Centavos),
            valor_variavel: l.valor_variavel,
            frequencia: match l.frequencia {
                c::Frequencia::Mensal => Frequencia::Mensal,
                c::Frequencia::Semanal => Frequencia::Semanal,
                c::Frequencia::Quinzenal => Frequencia::Quinzenal,
                c::Frequencia::Trimestral => Frequencia::Trimestral,
                c::Frequencia::Anual => Frequencia::Anual,
                c::Frequencia::Personalizada => Frequencia::Personalizada,
            },
            dia_do_mes: dia("recorrentes", &l.id, l.dia_do_mes)?,
            intervalo_dias: l.intervalo_em_dias,
            categoria_id: id_opt(l.categoria.as_ref()),
            fornecedor_id: id_opt(l.fornecedor.as_ref()),
            inicia_em: data_opt("recorrentes", &l.id, l.inicio.as_deref())?,
            termina_em: data_opt("recorrentes", &l.id, l.fim.as_deref())?,
            ativo: l.ativo,
        })
    }
}

impl TryFrom<&c::Ocorrencia> for Ocorrencia {
    type Error = ErroDeConversao;
    fn try_from(l: &c::Ocorrencia) -> Resultado<Self> {
        Ok(Ocorrencia {
            id: id(&l.id),
            recorrente_id: id(&l.recorrente),
            data_esperada: data("ocorrências", &l.id, &l.data_prevista)?,
            valor_esperado: l.valor_previsto.map(Centavos),
            valor_real: l.valor_real.map(Centavos),
            status: match l.estado {
                c::EstadoDaOcorrencia::Pendente => StatusDaOcorrencia::Pendente,
                c::EstadoDaOcorrencia::Paga => StatusDaOcorrencia::Paga,
                c::EstadoDaOcorrencia::Parcial => StatusDaOcorrencia::Parcial,
                c::EstadoDaOcorrencia::Pulada => StatusDaOcorrencia::Pulada,
                c::EstadoDaOcorrencia::Vencida => StatusDaOcorrencia::Vencida,
                c::EstadoDaOcorrencia::Cancelada => StatusDaOcorrencia::Cancelada,
            },
            data_do_pagamento: data_opt("ocorrências", &l.id, l.data_do_pagamento.as_deref())?,
            movimentacao_id: id_opt(l.movimentacao.as_ref()),
        })
    }
}

fn todas<'a, L: 'a, T>(linhas: &'a [L], f: impl Fn(&'a L) -> Resultado<T>) -> Resultado<Vec<T>> {
    linhas.iter().map(f).collect()
}

/// Acrescenta as linhas de uma coleção ao livro. Coleções sem regra no livro (apelidos,
/// contratos, documentos) são ignoradas aqui; a estação as guarda à parte.
pub fn acrescentar(livro: &mut LivroCaixa, linhas: &LinhasDaColecao) -> Resultado<()> {
    match linhas {
        LinhasDaColecao::Instituicoes(v) => {
            livro.instituicoes.extend(v.iter().map(Instituicao::from))
        }
        LinhasDaColecao::Contas(v) => livro.contas.extend(v.iter().map(Conta::from)),
        LinhasDaColecao::Cartoes(v) => livro.cartoes.extend(todas(v, Cartao::try_from)?),
        LinhasDaColecao::Faturas(v) => livro.faturas.extend(todas(v, Fatura::try_from)?),
        LinhasDaColecao::ItensDeFatura(v) => {
            livro
                .itens_de_fatura
                .extend(todas(v, ItemDeFatura::try_from)?);
        }
        LinhasDaColecao::Movimentacoes(v) => {
            livro
                .movimentacoes
                .extend(todas(v, Movimentacao::try_from)?);
            livro.tags_das_movimentacoes.extend(
                v.iter()
                    .flat_map(|m| m.tags.iter().map(move |t| (id(&m.id), id(t)))),
            );
        }
        LinhasDaColecao::Transferencias(v) => {
            livro
                .transferencias
                .extend(todas(v, Transferencia::try_from)?);
        }
        LinhasDaColecao::Rateios(v) => livro.rateios.extend(v.iter().map(Rateio::from)),
        LinhasDaColecao::Categorias(v) => livro.categorias.extend(v.iter().map(Categoria::from)),
        LinhasDaColecao::Tags(v) => livro.tags.extend(v.iter().map(Tag::from)),
        LinhasDaColecao::Centros(v) => livro.centros.extend(v.iter().map(Centro::from)),
        LinhasDaColecao::Fornecedores(v) => {
            livro.fornecedores.extend(v.iter().map(Fornecedor::from));
        }
        LinhasDaColecao::Dividas(v) => livro.dividas.extend(todas(v, Divida::try_from)?),
        LinhasDaColecao::Parcelas(v) => livro.parcelas.extend(todas(v, Parcela::try_from)?),
        LinhasDaColecao::Recorrentes(v) => {
            livro.recorrentes.extend(todas(v, Recorrente::try_from)?);
        }
        LinhasDaColecao::Ocorrencias(v) => {
            livro.ocorrencias.extend(todas(v, Ocorrencia::try_from)?);
        }
        LinhasDaColecao::Apelidos(_)
        | LinhasDaColecao::Contratos(_)
        | LinhasDaColecao::Documentos(_) => {}
    }
    Ok(())
}

/// O livro-caixa de um dono a partir do `/estado` (o dia de corte) e das coleções.
pub fn livro_das_linhas<'a>(
    dia_de_corte: u8,
    colecoes: impl IntoIterator<Item = &'a LinhasDaColecao>,
) -> Resultado<LivroCaixa> {
    let mut livro = LivroCaixa {
        corte: DiaDeCorte::novo(dia_de_corte).ok_or(ErroDeConversao::DiaDeCorte(dia_de_corte))?,
        ..LivroCaixa::default()
    };
    for linhas in colecoes {
        acrescentar(&mut livro, linhas)?;
    }
    Ok(livro)
}
