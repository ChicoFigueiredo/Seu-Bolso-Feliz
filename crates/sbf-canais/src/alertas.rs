//! As regras de alerta, puras: recebem o livro-caixa e o dia de hoje e devolvem os alertas
//! que valem agora. Não sabem de Telegram, banco nem relógio.
//!
//! Regras portadas ou novas (o domínio TS só tinha as da tela Hoje e Para resolver):
//! - **conta a vencer** em N dias e **conta vencida**: os compromissos da tela Hoje
//!   (`painel-hoje.ts`: saída não confirmada, ocorrência pendente de recorrente, fatura de
//!   cartão em aberto), mas sem o corte do ciclo, para não perder o vencimento do ciclo
//!   seguinte nem a vencida de ciclo antigo (as vencidas valem até 90 dias);
//! - **cobrança duplicada**: mesmo fornecedor e mesmo valor, mais de uma vez no mesmo ciclo
//!   (nova: o domínio só deduplica fonte, `deduplicateExpenses`);
//! - **gasto fora do padrão**: gasto da categoria no ciclo acima da média dos 3 ciclos
//!   anteriores mais um limiar (nova).
//!
//! Um [`Alerta`] tem **chave** (a identidade) e **estado** (o que mudou): o alerta só sai de
//! novo quando o estado muda.

use std::collections::{BTreeMap, HashMap};

use sbf_livro::gastos::gastos_pela_data_da_compra;
use sbf_livro::hoje::e_saida;
use sbf_livro::{
    Centavos, Ciclo, Data, LivroCaixa, StatusDaFatura, StatusDaOcorrencia, TipoDeMovimentacao,
};

/// Há quantos dias, no máximo, uma conta vencida ainda avisa.
const DIAS_DE_VENCIDA: i64 = 90;

/// Os limites das regras. Os padrões estão em [`Limites::default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    /// Avisa a conta que vence em até N dias (padrão 3).
    pub dias_a_vencer: i64,
    /// Fora do padrão: precisa passar da média em pelo menos esta porcentagem (padrão 50).
    pub limiar_percentual: i64,
    /// ... e em pelo menos este valor (padrão R$ 100,00).
    pub limiar_minimo: Centavos,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            dias_a_vencer: 3,
            limiar_percentual: 50,
            limiar_minimo: Centavos(10_000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TipoDeAlerta {
    ContaAVencer,
    ContaVencida,
    CobrancaDuplicada,
    GastoForaDoPadrao,
    /// Ponto de extensão: as sugestões da triagem moram na estação, não na VPS.
    SugestoesDaTriagem,
}

impl TipoDeAlerta {
    pub fn codigo(self) -> &'static str {
        match self {
            TipoDeAlerta::ContaAVencer => "conta_a_vencer",
            TipoDeAlerta::ContaVencida => "conta_vencida",
            TipoDeAlerta::CobrancaDuplicada => "cobranca_duplicada",
            TipoDeAlerta::GastoForaDoPadrao => "gasto_fora_do_padrao",
            TipoDeAlerta::SugestoesDaTriagem => "sugestoes_da_triagem",
        }
    }
}

/// Os dados de um alerta, ainda sem texto (o texto depende do nível do destinatário).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dados {
    Conta {
        /// Descrição ou nome do recorrente, como no livro (pode ter número de conta).
        descricao: String,
        /// O nome da conta ou do cartão de onde sai.
        conta: Option<String>,
        valor: Centavos,
        estimado: bool,
        vence_em: Data,
        /// Negativo quando já venceu.
        dias: i64,
    },
    Duplicada {
        fornecedor: String,
        valor: Centavos,
        quantidade: usize,
    },
    ForaDoPadrao {
        categoria: String,
        gasto: Centavos,
        media: Centavos,
    },
    Triagem {
        quantidade: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alerta {
    pub tipo: TipoDeAlerta,
    pub chave: String,
    pub estado: String,
    pub dados: Dados,
}

/// Uma fonte de alertas além das regras do livro: o ponto de extensão para o que a VPS não
/// vê (as sugestões prontas da triagem são locais).
pub trait FonteDeAlertas: Send + Sync {
    fn alertas(&self, livro: &LivroCaixa, hoje: Data) -> Vec<Alerta>;
}

/// Todos os alertas que valem hoje, ordenados por tipo e chave.
pub fn avaliar(
    livro: &LivroCaixa,
    hoje: Data,
    limites: &Limites,
    extras: &[Box<dyn FonteDeAlertas>],
) -> Vec<Alerta> {
    let mut alertas = Vec::new();
    alertas.extend(contas(livro, hoje, limites));
    alertas.extend(duplicadas(livro, hoje));
    alertas.extend(fora_do_padrao(livro, hoje, limites));
    for fonte in extras {
        alertas.extend(fonte.alertas(livro, hoje));
    }
    alertas.sort_by(|a, b| (a.tipo, &a.chave).cmp(&(b.tipo, &b.chave)));
    alertas
}

struct Compromisso {
    chave: String,
    descricao: String,
    conta: Option<String>,
    valor: Centavos,
    estimado: bool,
    data: Data,
}

/// Os compromissos abertos, sem o corte do ciclo.
fn compromissos_abertos(livro: &LivroCaixa) -> Vec<Compromisso> {
    let nome_da_conta: HashMap<&str, &str> = livro
        .contas
        .iter()
        .map(|c| (c.id.as_str(), c.nome.as_str()))
        .collect();
    let mut saida = Vec::new();
    for m in &livro.movimentacoes {
        if m.confirmada || !e_saida(m.tipo) || m.fatura_id.is_some() || m.parcela_id.is_some() {
            continue;
        }
        saida.push(Compromisso {
            chave: format!("mov:{}", m.id),
            descricao: m
                .descricao
                .clone()
                .unwrap_or_else(|| "Sem descrição".to_owned()),
            conta: nome_da_conta
                .get(m.conta_id.as_str())
                .map(|n| (*n).to_owned()),
            valor: m.valor.abs(),
            estimado: false,
            data: m.data,
        });
    }
    let recorrentes: HashMap<&str, &str> = livro
        .recorrentes
        .iter()
        .filter(|r| r.ativo)
        .map(|r| (r.id.as_str(), r.nome.as_str()))
        .collect();
    for o in &livro.ocorrencias {
        let Some(nome) = recorrentes.get(o.recorrente_id.as_str()) else {
            continue;
        };
        if o.status != StatusDaOcorrencia::Pendente || o.movimentacao_id.is_some() {
            continue;
        }
        saida.push(Compromisso {
            chave: format!("rec:{}", o.id),
            descricao: (*nome).to_owned(),
            conta: None,
            valor: o.valor_esperado.unwrap_or_default().abs(),
            estimado: o.valor_esperado.is_none(),
            data: o.data_esperada,
        });
    }
    let produto_do_cartao: HashMap<&str, &str> = livro
        .cartoes
        .iter()
        .map(|c| (c.id.as_str(), c.conta_id.as_str()))
        .collect();
    for f in &livro.faturas {
        let aberto = f.em_aberto();
        if f.status == StatusDaFatura::Paga || aberto <= Centavos::ZERO {
            continue;
        }
        let cartao = produto_do_cartao
            .get(f.cartao_id.as_str())
            .and_then(|p| nome_da_conta.get(p));
        saida.push(Compromisso {
            chave: format!("fat:{}", f.id),
            descricao: format!("Fatura {}", cartao.copied().unwrap_or("do cartão")),
            conta: None,
            valor: aberto,
            estimado: false,
            data: f.vencimento,
        });
    }
    saida
}

fn contas(livro: &LivroCaixa, hoje: Data, limites: &Limites) -> Vec<Alerta> {
    let mut alertas = Vec::new();
    for c in compromissos_abertos(livro) {
        let dias = hoje.dias_ate(c.data);
        let tipo = if dias < 0 {
            if -dias > DIAS_DE_VENCIDA {
                continue;
            }
            TipoDeAlerta::ContaVencida
        } else if dias <= limites.dias_a_vencer {
            TipoDeAlerta::ContaAVencer
        } else {
            continue;
        };
        alertas.push(Alerta {
            tipo,
            chave: c.chave,
            // Vencimento e valor: se o valor ou a data mudam, avisa de novo; o passar dos
            // dias não muda o estado (avisa uma vez a vencer e uma vez vencida).
            estado: format!("{}|{}|{}", tipo.codigo(), c.data, c.valor),
            dados: Dados::Conta {
                descricao: c.descricao,
                conta: c.conta,
                valor: c.valor,
                estimado: c.estimado,
                vence_em: c.data,
                dias,
            },
        });
    }
    alertas
}

/// Mesmo fornecedor e mesmo valor, mais de uma vez no mesmo ciclo (o atual e o anterior).
fn duplicadas(livro: &LivroCaixa, hoje: Data) -> Vec<Alerta> {
    let atual = Ciclo::da_data(hoje, livro.corte);
    let anterior = atual.anterior();
    let nome_do_fornecedor: HashMap<&str, &str> = livro
        .fornecedores
        .iter()
        .map(|f| (f.id.as_str(), f.nome.as_str()))
        .collect();
    let mut grupos: BTreeMap<(String, i64, String), usize> = BTreeMap::new();
    for m in &livro.movimentacoes {
        let conta = matches!(
            m.tipo,
            TipoDeMovimentacao::Despesa | TipoDeMovimentacao::Tarifa | TipoDeMovimentacao::Juros
        );
        let (true, Some(fornecedor)) = (conta, m.fornecedor_id.as_ref()) else {
            continue;
        };
        if m.parcela_id.is_some() || m.valor.abs() == Centavos::ZERO {
            continue;
        }
        let ciclo = Ciclo::da_data(m.data, livro.corte);
        if ciclo != atual && ciclo != anterior {
            continue;
        }
        *grupos
            .entry((fornecedor.clone(), m.valor.abs().0, ciclo.nome()))
            .or_default() += 1;
    }
    grupos
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|((fornecedor, valor, ciclo), quantidade)| Alerta {
            tipo: TipoDeAlerta::CobrancaDuplicada,
            chave: format!("dup:{fornecedor}:{valor}:{ciclo}"),
            estado: quantidade.to_string(),
            dados: Dados::Duplicada {
                fornecedor: nome_do_fornecedor
                    .get(fornecedor.as_str())
                    .map_or_else(|| "um fornecedor".to_owned(), |n| (*n).to_owned()),
                valor: Centavos(valor),
                quantidade,
            },
        })
        .collect()
}

/// Gasto da categoria no ciclo atual acima da média dos 3 ciclos anteriores mais o limiar.
/// Só avisa com histórico: pelo menos 2 dos 3 ciclos anteriores com gasto na categoria.
fn fora_do_padrao(livro: &LivroCaixa, hoje: Data, limites: &Limites) -> Vec<Alerta> {
    let atual = Ciclo::da_data(hoje, livro.corte);
    let anteriores = {
        let a1 = atual.anterior();
        let a2 = a1.anterior();
        [a1, a2, a2.anterior()]
    };
    let mut totais: BTreeMap<String, [i64; 4]> = BTreeMap::new();
    for g in gastos_pela_data_da_compra(livro) {
        let Some(categoria) = g.categoria_id else {
            continue;
        };
        let posicao = if atual.contem(g.data) {
            0
        } else if let Some(i) = anteriores.iter().position(|c| c.contem(g.data)) {
            i + 1
        } else {
            continue;
        };
        totais.entry(categoria).or_default()[posicao] += g.valor.0;
    }
    let nome: HashMap<&str, &str> = livro
        .categorias
        .iter()
        .map(|c| (c.id.as_str(), c.nome.as_str()))
        .collect();
    let mut alertas = Vec::new();
    for (categoria, t) in totais {
        let historico = &t[1..];
        if historico.iter().filter(|v| **v > 0).count() < 2 {
            continue;
        }
        let media = historico.iter().sum::<i64>() / 3;
        let gasto = t[0];
        let limiar = (media * limites.limiar_percentual / 100).max(limites.limiar_minimo.0);
        if media <= 0 || gasto <= media + limiar {
            continue;
        }
        // A faixa cresce de meio em meio da média: o gasto subindo um pouco por dia não
        // repete o aviso; dobrar a média, sim.
        let faixa = gasto * 2 / media;
        alertas.push(Alerta {
            tipo: TipoDeAlerta::GastoForaDoPadrao,
            chave: format!("padrao:{categoria}:{}", atual.nome()),
            estado: format!("faixa{faixa}"),
            dados: Dados::ForaDoPadrao {
                categoria: nome
                    .get(categoria.as_str())
                    .map_or_else(|| "uma categoria".to_owned(), |n| (*n).to_owned()),
                gasto: Centavos(gasto),
                media: Centavos(media),
            },
        });
    }
    alertas
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_livro::{Categoria, Fornecedor, Movimentacao};

    fn d(a: i32, m: u8, dia: u8) -> Data {
        Data::nova(a, m, dia).expect("data")
    }

    fn mov(id: &str, valor: i64, data: Data) -> Movimentacao {
        Movimentacao {
            id: id.into(),
            conta_id: "c1".into(),
            tipo: TipoDeMovimentacao::Despesa,
            valor: Centavos(valor),
            descricao: Some(format!("Compra {id}")),
            data,
            data_competencia: None,
            fatura_id: None,
            parcela_id: None,
            categoria_id: None,
            fornecedor_id: None,
            ocorrencia_id: None,
            documento_id: None,
            confirmada: true,
            versao: None,
        }
    }

    #[test]
    fn conta_a_vencer_e_vencida_sem_corte_de_ciclo() {
        let hoje = d(2026, 11, 18); // ciclo 20/10 a 19/11
        let mut livro = LivroCaixa::default();
        let mut a = mov("a", 14230, d(2026, 11, 20)); // no ciclo seguinte, em 2 dias
        a.confirmada = false;
        let mut b = mov("b", 5000, d(2026, 9, 1)); // vencida de ciclo antigo
        b.confirmada = false;
        let mut c = mov("c", 9900, d(2026, 11, 25)); // longe
        c.confirmada = false;
        livro.movimentacoes = vec![a, b, c];
        let r = avaliar(&livro, hoje, &Limites::default(), &[]);
        let tipos: Vec<_> = r.iter().map(|x| (x.tipo, x.chave.as_str())).collect();
        assert_eq!(
            tipos,
            vec![
                (TipoDeAlerta::ContaAVencer, "mov:a"),
                (TipoDeAlerta::ContaVencida, "mov:b")
            ]
        );
        // 90 dias de teto: a vencida de setembro (78 dias) entra; uma de junho não.
        livro.movimentacoes[1].data = d(2026, 6, 1);
        assert_eq!(avaliar(&livro, hoje, &Limites::default(), &[]).len(), 1);
    }

    #[test]
    fn estado_da_conta_nao_muda_com_o_passar_dos_dias_mas_muda_ao_vencer() {
        let mut livro = LivroCaixa::default();
        let mut a = mov("a", 1000, d(2026, 11, 20));
        a.confirmada = false;
        livro.movimentacoes = vec![a];
        let l = Limites::default();
        let e1 = avaliar(&livro, d(2026, 11, 17), &l, &[])[0].estado.clone();
        let e2 = avaliar(&livro, d(2026, 11, 19), &l, &[])[0].estado.clone();
        let e3 = avaliar(&livro, d(2026, 11, 21), &l, &[])[0].estado.clone();
        assert_eq!(e1, e2);
        assert_ne!(e2, e3);
    }

    #[test]
    fn duplicada_so_com_mesmo_fornecedor_valor_e_ciclo() {
        let hoje = d(2026, 11, 14);
        let mut livro = LivroCaixa::default();
        livro.fornecedores.push(Fornecedor {
            id: "f1".into(),
            nome: "Padaria Exemplo".into(),
            nome_fantasia: None,
            razao_social: None,
            ativo: true,
        });
        let mut m1 = mov("1", 3500, d(2026, 11, 2));
        let mut m2 = mov("2", 3500, d(2026, 11, 3));
        let mut m3 = mov("3", 3500, d(2026, 10, 5)); // outro ciclo
        let mut m4 = mov("4", 4000, d(2026, 11, 3)); // outro valor
        for m in [&mut m1, &mut m2, &mut m3, &mut m4] {
            m.fornecedor_id = Some("f1".into());
        }
        livro.movimentacoes = vec![m1, m2, m3, m4];
        let r = avaliar(&livro, hoje, &Limites::default(), &[]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].tipo, TipoDeAlerta::CobrancaDuplicada);
        assert!(matches!(
            &r[0].dados,
            Dados::Duplicada { fornecedor, quantidade: 2, .. } if fornecedor == "Padaria Exemplo"
        ));
    }

    #[test]
    fn fora_do_padrao_pede_historico_e_passar_do_limiar() {
        let hoje = d(2026, 11, 14); // ciclo atual 20/10 a 19/11
        let mut livro = LivroCaixa::default();
        livro.categorias.push(Categoria {
            id: "k1".into(),
            nome: "Mercado".into(),
            pai_id: None,
            ativa: true,
        });
        let gasto = |id: &str, valor, data| {
            let mut m = mov(id, valor, data);
            m.categoria_id = Some("k1".into());
            m
        };
        // 3 ciclos anteriores de R$ 600 cada; o atual com R$ 1.500.
        livro.movimentacoes = vec![
            gasto("h1", 60_000, d(2026, 9, 25)),
            gasto("h2", 60_000, d(2026, 8, 25)),
            gasto("h3", 60_000, d(2026, 7, 25)),
            gasto("a1", 150_000, d(2026, 11, 1)),
        ];
        let r = avaliar(&livro, hoje, &Limites::default(), &[]);
        assert_eq!(r.len(), 1);
        assert!(matches!(
            &r[0].dados,
            Dados::ForaDoPadrao { categoria, gasto, media }
                if categoria == "Mercado" && *gasto == Centavos(150_000) && *media == Centavos(60_000)
        ));
        // Dentro do limiar (R$ 800 contra média 600 + 50%): sem alerta.
        livro.movimentacoes[3].valor = Centavos(80_000);
        assert!(avaliar(&livro, hoje, &Limites::default(), &[]).is_empty());
        // Sem histórico (só 1 ciclo anterior): sem alerta.
        livro.movimentacoes[3].valor = Centavos(150_000);
        livro.movimentacoes.remove(2);
        livro.movimentacoes.remove(1);
        assert!(avaliar(&livro, hoje, &Limites::default(), &[]).is_empty());
    }

    #[test]
    fn fonte_extra_entra_na_lista() {
        struct Triagem;
        impl FonteDeAlertas for Triagem {
            fn alertas(&self, _: &LivroCaixa, _: Data) -> Vec<Alerta> {
                vec![Alerta {
                    tipo: TipoDeAlerta::SugestoesDaTriagem,
                    chave: "triagem".into(),
                    estado: "3".into(),
                    dados: Dados::Triagem { quantidade: 3 },
                }]
            }
        }
        let r = avaliar(
            &LivroCaixa::default(),
            d(2026, 11, 14),
            &Limites::default(),
            &[Box::new(Triagem)],
        );
        assert_eq!(r.len(), 1);
    }
}
