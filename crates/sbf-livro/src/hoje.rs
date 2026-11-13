//! Os números da tela Hoje: quanto eu tenho, quanto disso é meu, o que vence e como está o
//! ciclo, **sem contar a mesma coisa duas vezes**.
//!
//! Portado de `apps/web/src/app/actions/painel-hoje.ts` (`carregarPainelHoje` e
//! `contarPendencias`). As três armadilhas de dupla contagem, como lá:
//!
//! 1. **Compra no cartão × fatura:** o compromisso do cartão vem da fatura; a movimentação
//!    com fatura não é compromisso avulso, e no resultado vai para "No cartão", não "Saiu".
//! 2. **Recorrente × movimentação gerada:** só a ocorrência pendente sem movimentação conta.
//! 3. **Parcela × pagamento:** a movimentação com parcela não é compromisso avulso; o
//!    compromisso mensal de dívidas é informativo e não entra no livre após compromissos.
//!
//! Diferenças intencionais em relação ao TS:
//! - O ciclo é o de São Paulo com o corte do livro (padrão 20, grill Q14); o web usa
//!   corte 1 sem preferência e erra o fuso em UTC (ver [`crate::ciclo`]).
//! - O saldo do centro ignora o ajuste (grill Q15).
//! - "Não alocadas", em Para resolver, não conta o pagamento de fatura nem o ajuste, que
//!   não aceitam rateio e nunca sairiam da lista.
//! - A lista se chama **Para resolver** (grill Q16) e cada item traz as movimentações que
//!   o somam; os compromissos e os centros vêm todos (a tela corta).
//!
//! Teste de ouro: `tests/ouro_paineis.rs`.

use std::collections::{HashMap, HashSet};

use crate::ciclo::Ciclo;
use crate::data::Data;
use crate::dinheiro::{Centavos, Fracao};
use crate::rateio::{Direcao, aceita_rateio, direcao_no_centro};
use crate::tipos::{
    Conta, Id, LivroCaixa, Movimentacao, StatusDaDivida, StatusDaFatura, StatusDaOcorrencia,
    StatusDoCentro, TipoDeConta, TipoDeMovimentacao,
};

/// Uma conta bancária em Disponível agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDeConta {
    pub id: Id,
    pub nome: String,
    pub instituicao: Option<String>,
    pub saldo: Centavos,
}

/// Um cartão com a fatura atual (a não paga de vencimento mais tarde).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDeCartao {
    /// O produto do cartão (`financial_products.id`), não o `cards.id`.
    pub conta_id: Id,
    pub nome: String,
    pub instituicao: Option<String>,
    pub fatura_id: Option<Id>,
    pub fatura_atual: Centavos,
    pub limite: Option<Centavos>,
    pub limite_livre: Option<Centavos>,
    pub fechamento: Option<Data>,
    pub vencimento: Option<Data>,
}

/// De onde vem um compromisso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrigemDoCompromisso {
    Movimentacao,
    Recorrente,
    Fatura,
}

/// Um compromisso: uma movimentação futura já conhecida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compromisso {
    /// Da movimentação, da ocorrência ou da fatura, conforme a origem.
    pub id: Id,
    pub descricao: String,
    pub valor: Centavos,
    pub data: Data,
    pub origem: OrigemDoCompromisso,
    /// O nome da conta (ou do cartão).
    pub conta: Option<String>,
    pub vencido: bool,
    /// Sem valor esperado: o livre após compromissos vira estimativa.
    pub valor_estimado: bool,
}

/// O resultado do mês (do ciclo) e a projeção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResultadoDoMes {
    /// `income` e `refund` confirmados.
    pub entrou: Centavos,
    /// Saídas confirmadas que deixaram uma conta (sem fatura).
    pub saiu: Centavos,
    /// Comprado no cartão no ciclo; sai da conta quando a fatura for paga.
    pub no_cartao: Centavos,
    /// Compromissos de hoje até o fim do ciclo.
    pub ainda_vai_sair: Centavos,
    /// Disponível agora − ainda vai sair. Receita prevista não entra.
    pub livre_projetado: Centavos,
}

/// Um centro ativo na Hoje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDeCentro {
    pub id: Id,
    pub nome: String,
    pub icone: Option<String>,
    /// Rateios de saídas do ciclo.
    pub gasto_no_ciclo: Centavos,
    /// Entradas − saídas desde sempre.
    pub saldo: Centavos,
}

/// Por que algo está em Para resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotivoParaResolver {
    /// Saída não confirmada com data até hoje.
    Vencidas,
    /// Saída sem categoria.
    SemCategoria,
    /// Saída que aceita rateio e não tem nenhum.
    NaoAlocadas,
    /// Qualquer movimentação não confirmada.
    NaoConfirmadas,
    /// Documentos esperando revisão.
    DocumentosARevisar,
}

impl MotivoParaResolver {
    /// O rótulo do web.
    pub fn rotulo(self) -> &'static str {
        match self {
            MotivoParaResolver::Vencidas => "Vencidas",
            MotivoParaResolver::SemCategoria => "Sem categoria",
            MotivoParaResolver::NaoAlocadas => "Não alocadas",
            MotivoParaResolver::NaoConfirmadas => "Não confirmadas",
            MotivoParaResolver::DocumentosARevisar => "Precisam de revisão",
        }
    }
}

/// Um item da lista Para resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemParaResolver {
    pub motivo: MotivoParaResolver,
    pub quantidade: u32,
    /// As movimentações contadas (vazio para documentos).
    pub movimentacoes: Vec<Id>,
}

/// Os números da Hoje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainelHoje {
    pub ciclo: Ciclo,
    /// Σ saldos gravados das contas correntes e poupanças ativas.
    pub disponivel_agora: Centavos,
    /// Disponível agora − Σ compromissos.
    pub livre_apos_compromissos: Centavos,
    /// Há compromisso sem valor confirmado: o livre é estimativa.
    pub estimativa: bool,
    pub contas: Vec<ResumoDeConta>,
    pub cartoes: Vec<ResumoDeCartao>,
    /// Σ saldo devedor das dívidas ativas.
    pub divida_total: Centavos,
    /// Σ saldo ÷ parcelas restantes. Informativo: não entra no livre.
    pub compromisso_mensal_de_dividas: Centavos,
    /// Todos os compromissos, por data.
    pub compromissos: Vec<Compromisso>,
    /// máx(0, ainda vai sair − disponível agora).
    pub falta_ate_o_fim_do_ciclo: Centavos,
    pub resultado: ResultadoDoMes,
    /// Só os motivos com quantidade > 0.
    pub para_resolver: Vec<ItemParaResolver>,
    /// Os centros ativos, por nome.
    pub centros: Vec<ResumoDeCentro>,
}

/// As saídas do web (`SAIDAS`): o ajuste fica fora.
pub fn e_saida(tipo: TipoDeMovimentacao) -> bool {
    use TipoDeMovimentacao as T;
    matches!(
        tipo,
        T::Despesa | T::Tarifa | T::Juros | T::PagamentoDeFatura | T::PagamentoDeDivida
    )
}

/// `income` ou `refund`.
pub fn e_entrada(tipo: TipoDeMovimentacao) -> bool {
    matches!(
        tipo,
        TipoDeMovimentacao::Receita | TipoDeMovimentacao::Estorno
    )
}

/// As contas ativas na ordem do web: `display_order` (sem ordem por último), depois nome.
pub fn contas_em_ordem(livro: &LivroCaixa) -> Vec<&Conta> {
    let mut contas: Vec<&Conta> = livro.contas.iter().filter(|c| c.ativa).collect();
    contas.sort_by(|a, b| {
        (a.ordem.is_none(), a.ordem, &a.nome).cmp(&(b.ordem.is_none(), b.ordem, &b.nome))
    });
    contas
}

/// Saldo e gasto no ciclo de cada centro, pelos rateios.
struct Somas {
    saldo: HashMap<Id, Centavos>,
    gasto_no_ciclo: HashMap<Id, Centavos>,
}

fn somas_dos_centros(livro: &LivroCaixa, ciclo: &Ciclo) -> Somas {
    let movs: HashMap<&str, &Movimentacao> = livro
        .movimentacoes
        .iter()
        .map(|m| (m.id.as_str(), m))
        .collect();
    let mut somas = Somas {
        saldo: HashMap::new(),
        gasto_no_ciclo: HashMap::new(),
    };
    for r in &livro.rateios {
        let Some(m) = movs.get(r.movimentacao_id.as_str()) else {
            continue;
        };
        match direcao_no_centro(m.tipo) {
            Some(Direcao::Entrada) => {
                *somas.saldo.entry(r.centro_id.clone()).or_default() += r.valor
            }
            Some(Direcao::Saida) => *somas.saldo.entry(r.centro_id.clone()).or_default() -= r.valor,
            None => {}
        }
        if e_saida(m.tipo) && ciclo.contem(m.data) {
            *somas.gasto_no_ciclo.entry(r.centro_id.clone()).or_default() += r.valor;
        }
    }
    somas
}

/// A lista Para resolver, sobre todas as movimentações do livro.
pub fn para_resolver(livro: &LivroCaixa, hoje: Data) -> Vec<ItemParaResolver> {
    let com_rateio: HashSet<&str> = livro
        .rateios
        .iter()
        .map(|r| r.movimentacao_id.as_str())
        .collect();
    let ids = |filtro: &dyn Fn(&Movimentacao) -> bool| -> Vec<Id> {
        livro
            .movimentacoes
            .iter()
            .filter(|m| filtro(m))
            .map(|m| m.id.clone())
            .collect()
    };
    let listas = [
        (
            MotivoParaResolver::Vencidas,
            ids(&|m| !m.confirmada && m.data <= hoje && e_saida(m.tipo)),
        ),
        (
            MotivoParaResolver::SemCategoria,
            ids(&|m| m.categoria_id.is_none() && e_saida(m.tipo)),
        ),
        (
            MotivoParaResolver::NaoAlocadas,
            ids(&|m| {
                e_saida(m.tipo) && aceita_rateio(m.tipo) && !com_rateio.contains(m.id.as_str())
            }),
        ),
        (MotivoParaResolver::NaoConfirmadas, ids(&|m| !m.confirmada)),
    ];
    let mut itens: Vec<ItemParaResolver> = listas
        .into_iter()
        .map(|(motivo, movimentacoes)| ItemParaResolver {
            motivo,
            quantidade: u32::try_from(movimentacoes.len()).unwrap_or(u32::MAX),
            movimentacoes,
        })
        .collect();
    itens.push(ItemParaResolver {
        motivo: MotivoParaResolver::DocumentosARevisar,
        quantidade: livro.documentos_a_revisar,
        movimentacoes: Vec::new(),
    });
    itens.retain(|i| i.quantidade > 0);
    itens
}

/// Os números da Hoje para o dia `hoje` (de São Paulo), no ciclo do corte do livro.
pub fn painel_hoje(livro: &LivroCaixa, hoje: Data) -> PainelHoje {
    let ciclo = Ciclo::da_data(hoje, livro.corte);
    let instituicao = |id: &str| {
        livro
            .instituicoes
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.nome.clone())
    };
    let produtos = contas_em_ordem(livro);
    let nome_da_conta: HashMap<&str, &str> = livro
        .contas
        .iter()
        .map(|c| (c.id.as_str(), c.nome.as_str()))
        .collect();

    // ── Posição ──────────────────────────────────────────────────────────────
    let contas: Vec<ResumoDeConta> = produtos
        .iter()
        .filter(|p| matches!(p.tipo, TipoDeConta::ContaCorrente | TipoDeConta::Poupanca))
        .map(|p| ResumoDeConta {
            id: p.id.clone(),
            nome: p.nome.clone(),
            instituicao: instituicao(&p.instituicao_id),
            saldo: p.saldo,
        })
        .collect();
    let disponivel_agora: Centavos = contas.iter().map(|c| c.saldo).sum();

    // Faturas não pagas com o produto do cartão (a fatura aponta para `cards`).
    let produto_do_cartao: HashMap<&str, &str> = livro
        .cartoes
        .iter()
        .map(|c| (c.id.as_str(), c.conta_id.as_str()))
        .collect();
    let mut faturas_abertas: Vec<(&str, &crate::tipos::Fatura)> = livro
        .faturas
        .iter()
        .filter(|f| f.status != StatusDaFatura::Paga)
        .filter_map(|f| produto_do_cartao.get(f.cartao_id.as_str()).map(|p| (*p, f)))
        .collect();
    faturas_abertas.sort_by(|a, b| (a.1.vencimento, &a.1.id).cmp(&(b.1.vencimento, &b.1.id)));
    // Como o `Map` do web: a última por vencimento vence.
    let fatura_do_produto: HashMap<&str, &crate::tipos::Fatura> =
        faturas_abertas.iter().map(|(p, f)| (*p, *f)).collect();

    let cartoes: Vec<ResumoDeCartao> = produtos
        .iter()
        .filter(|p| p.tipo == TipoDeConta::CartaoDeCredito)
        .map(|p| {
            let fatura = fatura_do_produto.get(p.id.as_str());
            let atual = fatura.map_or(Centavos::ZERO, |f| f.em_aberto());
            ResumoDeCartao {
                conta_id: p.id.clone(),
                nome: p.nome.clone(),
                instituicao: instituicao(&p.instituicao_id),
                fatura_id: fatura.map(|f| f.id.clone()),
                fatura_atual: atual,
                limite: p.limite,
                limite_livre: p.limite.map(|l| l - atual),
                fechamento: fatura.map(|f| f.fechamento),
                vencimento: fatura.map(|f| f.vencimento),
            }
        })
        .collect();

    let dividas_ativas = livro
        .dividas
        .iter()
        .filter(|d| d.status == StatusDaDivida::Ativa);
    let divida_total: Centavos = dividas_ativas.clone().map(|d| d.saldo_devedor).sum();
    let compromisso_mensal_de_dividas = dividas_ativas
        .filter_map(|d| {
            let restantes = i64::from(d.total_de_parcelas.unwrap_or(0) - d.parcelas_pagas);
            (restantes > 0).then(|| d.saldo_devedor / restantes)
        })
        .fold(Fracao::ZERO, |s, f| s + f)
        .arredondar();

    // ── Compromissos ─────────────────────────────────────────────────────────
    let mut compromissos: Vec<Compromisso> = Vec::new();
    for m in livro.movimentacoes.iter().filter(|m| ciclo.contem(m.data)) {
        if m.confirmada || !e_saida(m.tipo) || m.fatura_id.is_some() || m.parcela_id.is_some() {
            continue;
        }
        compromissos.push(Compromisso {
            id: m.id.clone(),
            descricao: m
                .descricao
                .clone()
                .unwrap_or_else(|| "Sem descrição".to_owned()),
            valor: m.valor,
            data: m.data,
            origem: OrigemDoCompromisso::Movimentacao,
            conta: nome_da_conta
                .get(m.conta_id.as_str())
                .map(|n| (*n).to_owned()),
            vencido: m.data < hoje,
            valor_estimado: false,
        });
    }
    let recorrentes_ativos: HashMap<&str, &str> = livro
        .recorrentes
        .iter()
        .filter(|r| r.ativo)
        .map(|r| (r.id.as_str(), r.nome.as_str()))
        .collect();
    for o in &livro.ocorrencias {
        let Some(nome) = recorrentes_ativos.get(o.recorrente_id.as_str()) else {
            continue;
        };
        if o.status != StatusDaOcorrencia::Pendente
            || o.movimentacao_id.is_some()
            || o.data_esperada > ciclo.fim
        {
            continue;
        }
        compromissos.push(Compromisso {
            id: o.id.clone(),
            descricao: (*nome).to_owned(),
            valor: o.valor_esperado.unwrap_or_default(),
            data: o.data_esperada,
            origem: OrigemDoCompromisso::Recorrente,
            conta: None,
            vencido: o.data_esperada < hoje,
            valor_estimado: o.valor_esperado.is_none(),
        });
    }
    for (produto, f) in &faturas_abertas {
        let aberto = f.em_aberto();
        if aberto <= Centavos::ZERO {
            continue;
        }
        let cartao = cartoes.iter().find(|c| c.conta_id == *produto);
        compromissos.push(Compromisso {
            id: f.id.clone(),
            descricao: format!("Fatura {}", cartao.map_or("do cartão", |c| c.nome.as_str())),
            valor: aberto,
            data: f.vencimento,
            origem: OrigemDoCompromisso::Fatura,
            conta: cartao.map(|c| c.nome.clone()),
            vencido: f.vencimento < hoje,
            valor_estimado: false,
        });
    }
    // Por data; no mesmo dia, movimentação, recorrente, fatura (a ordem do web).
    compromissos.sort_by(|a, b| (a.data, a.origem, &a.id).cmp(&(b.data, b.origem, &b.id)));

    let total_compromissos: Centavos = compromissos.iter().map(|c| c.valor).sum();
    let estimativa = compromissos.iter().any(|c| c.valor_estimado);
    let ainda_vai_sair: Centavos = compromissos
        .iter()
        .filter(|c| c.data >= hoje && c.data <= ciclo.fim)
        .map(|c| c.valor)
        .sum();

    // ── Resultado do mês ─────────────────────────────────────────────────────
    let mut resultado = ResultadoDoMes {
        ainda_vai_sair,
        livre_projetado: disponivel_agora - ainda_vai_sair,
        ..ResultadoDoMes::default()
    };
    for m in livro
        .movimentacoes
        .iter()
        .filter(|m| m.confirmada && ciclo.contem(m.data))
    {
        if e_entrada(m.tipo) {
            resultado.entrou += m.valor;
        } else if e_saida(m.tipo) {
            if m.fatura_id.is_some() {
                resultado.no_cartao += m.valor;
            } else {
                resultado.saiu += m.valor;
            }
        }
    }

    // ── Centros ──────────────────────────────────────────────────────────────
    let somas = somas_dos_centros(livro, &ciclo);
    let mut centros: Vec<ResumoDeCentro> = livro
        .centros
        .iter()
        .filter(|c| c.status == StatusDoCentro::Ativo)
        .map(|c| ResumoDeCentro {
            id: c.id.clone(),
            nome: c.nome.clone(),
            icone: c.icone.clone(),
            gasto_no_ciclo: somas.gasto_no_ciclo.get(&c.id).copied().unwrap_or_default(),
            saldo: somas.saldo.get(&c.id).copied().unwrap_or_default(),
        })
        .collect();
    centros.sort_by(|a, b| (&a.nome, &a.id).cmp(&(&b.nome, &b.id)));

    PainelHoje {
        ciclo,
        disponivel_agora,
        livre_apos_compromissos: disponivel_agora - total_compromissos,
        estimativa,
        contas,
        cartoes,
        divida_total,
        compromisso_mensal_de_dividas,
        compromissos,
        falta_ate_o_fim_do_ciclo: (ainda_vai_sair - disponivel_agora).ou_zero(),
        resultado,
        para_resolver: para_resolver(livro, hoje),
        centros,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::tipos::{Cartao, Fatura, Ocorrencia, Recorrente};

    fn d(t: &str) -> Data {
        Data::de_iso(t).expect("data")
    }

    fn conta(id: &str, tipo: TipoDeConta, saldo: i64) -> Conta {
        Conta {
            id: id.into(),
            instituicao_id: "i".into(),
            nome: id.into(),
            tipo,
            saldo: Centavos(saldo),
            limite: None,
            ativa: true,
            ordem: None,
        }
    }

    fn mov(
        id: &str,
        tipo: TipoDeMovimentacao,
        valor: i64,
        data: &str,
        confirmada: bool,
    ) -> Movimentacao {
        Movimentacao {
            id: id.into(),
            conta_id: "corrente".into(),
            tipo,
            valor: Centavos(valor),
            descricao: None,
            data: d(data),
            data_competencia: None,
            fatura_id: None,
            parcela_id: None,
            categoria_id: Some("cat".into()),
            fornecedor_id: None,
            ocorrencia_id: None,
            documento_id: None,
            confirmada,
            versao: None,
        }
    }

    fn livro() -> LivroCaixa {
        LivroCaixa {
            contas: vec![
                conta("corrente", TipoDeConta::ContaCorrente, 100_000),
                conta("cartao", TipoDeConta::CartaoDeCredito, 0),
            ],
            cartoes: vec![Cartao {
                id: "card".into(),
                conta_id: "cartao".into(),
                final_do_numero: None,
                bandeira: None,
                limite: None,
                dia_de_fechamento: None,
                dia_de_vencimento: None,
                ativo: true,
            }],
            ..LivroCaixa::default()
        }
    }

    const HOJE: &str = "2026-11-13";

    #[test]
    fn compra_no_cartao_nao_soma_com_a_fatura() {
        let mut l = livro();
        l.faturas.push(Fatura {
            id: "fat".into(),
            cartao_id: "card".into(),
            mes_de_referencia: d("2026-11-01"),
            inicio: d("2026-10-25"),
            fechamento: d("2026-11-24"),
            vencimento: d("2026-11-18"),
            total: Centavos(30_000),
            pago: Centavos::ZERO,
            status: StatusDaFatura::Aberta,
        });
        let mut compra = mov(
            "compra",
            TipoDeMovimentacao::Despesa,
            30_000,
            "2026-11-10",
            false,
        );
        compra.fatura_id = Some("fat".into());
        l.movimentacoes.push(compra.clone());
        let p = painel_hoje(&l, d(HOJE));
        assert_eq!(p.compromissos.len(), 1, "só a fatura");
        assert_eq!(p.livre_apos_compromissos, Centavos(70_000));
        // Confirmada, a compra vai para "No cartão", não para "Saiu".
        compra.confirmada = true;
        l.movimentacoes = vec![compra];
        let p = painel_hoje(&l, d(HOJE));
        assert_eq!(
            (p.resultado.saiu, p.resultado.no_cartao),
            (Centavos::ZERO, Centavos(30_000))
        );
    }

    #[test]
    fn recorrente_que_virou_movimentacao_conta_uma_vez() {
        let mut l = livro();
        l.recorrentes.push(Recorrente {
            id: "rec".into(),
            nome: "Assinatura".into(),
            tipo: TipoDeMovimentacao::Despesa,
            conta_id: None,
            valor: Some(Centavos(5_000)),
            valor_variavel: false,
            frequencia: crate::tipos::Frequencia::Mensal,
            dia_do_mes: None,
            intervalo_dias: None,
            categoria_id: None,
            fornecedor_id: None,
            inicia_em: None,
            termina_em: None,
            ativo: true,
        });
        l.ocorrencias.push(Ocorrencia {
            id: "oc".into(),
            recorrente_id: "rec".into(),
            data_esperada: d("2026-11-15"),
            valor_esperado: Some(Centavos(5_000)),
            valor_real: None,
            status: StatusDaOcorrencia::Pendente,
            data_do_pagamento: None,
            movimentacao_id: Some("gerada".into()),
        });
        l.movimentacoes.push(mov(
            "gerada",
            TipoDeMovimentacao::Despesa,
            5_000,
            "2026-11-15",
            false,
        ));
        let p = painel_hoje(&l, d(HOJE));
        assert_eq!(p.compromissos.len(), 1);
        assert_eq!(p.compromissos[0].origem, OrigemDoCompromisso::Movimentacao);
    }

    #[test]
    fn pagamento_de_parcela_nao_vira_compromisso_avulso() {
        let mut l = livro();
        let mut pagamento = mov(
            "pag",
            TipoDeMovimentacao::PagamentoDeDivida,
            45_000,
            "2026-11-18",
            false,
        );
        pagamento.parcela_id = Some("parcela".into());
        l.movimentacoes.push(pagamento);
        let p = painel_hoje(&l, d(HOJE));
        assert!(p.compromissos.is_empty());
        assert_eq!(p.livre_apos_compromissos, p.disponivel_agora);
    }

    #[test]
    fn ocorrencia_sem_valor_torna_o_livre_estimativa() {
        let mut l = livro();
        l.recorrentes.push(Recorrente {
            id: "luz".into(),
            nome: "Luz".into(),
            tipo: TipoDeMovimentacao::Despesa,
            conta_id: None,
            valor: None,
            valor_variavel: true,
            frequencia: crate::tipos::Frequencia::Mensal,
            dia_do_mes: None,
            intervalo_dias: None,
            categoria_id: None,
            fornecedor_id: None,
            inicia_em: None,
            termina_em: None,
            ativo: true,
        });
        l.ocorrencias.push(Ocorrencia {
            id: "oc".into(),
            recorrente_id: "luz".into(),
            data_esperada: d("2026-11-17"),
            valor_esperado: None,
            valor_real: None,
            status: StatusDaOcorrencia::Pendente,
            data_do_pagamento: None,
            movimentacao_id: None,
        });
        assert!(painel_hoje(&l, d(HOJE)).estimativa);
    }

    #[test]
    fn ajuste_fica_fora_do_resultado_e_do_centro() {
        let mut l = livro();
        l.movimentacoes.push(mov(
            "aj",
            TipoDeMovimentacao::Ajuste,
            9_999,
            "2026-11-10",
            true,
        ));
        l.rateios.push(crate::tipos::Rateio {
            id: "r".into(),
            movimentacao_id: "aj".into(),
            centro_id: "c".into(),
            valor: Centavos(9_999),
            nota: None,
        });
        l.centros.push(crate::tipos::Centro {
            id: "c".into(),
            nome: "Centro".into(),
            tipo: "projeto".into(),
            status: StatusDoCentro::Ativo,
            descricao: None,
            icone: None,
            cor: None,
        });
        let p = painel_hoje(&l, d(HOJE));
        assert_eq!(
            p.resultado,
            ResultadoDoMes {
                livre_projetado: Centavos(100_000),
                ..ResultadoDoMes::default()
            }
        );
        assert_eq!(p.centros[0].saldo, Centavos::ZERO);
        assert!(p.para_resolver.is_empty());
    }
}
