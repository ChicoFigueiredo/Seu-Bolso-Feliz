//! Uma implementação **fictícia** do trait [`Planejamento`], para o modo demonstração, os
//! testes e as capturas de tela. Todos os dados são inventados; nada vem do cofre, do
//! inventário ou da planilha reais.
//!
//! Ela projeta de verdade (versões, pausas, cancelamento, exceções, parcelas, dia 31) com um
//! motor pequeno, só o bastante para a janela mostrar e editar o controle mensal. Não é o
//! motor do serviço real: não use fora da demonstração.

mod extrato;

pub use extrato::{
    CONTA_DO_NUBANK, CONTA_DO_SALARIO, CONTA_SEM_PAR, LIQUIDO_DO_SALARIO, TEXTO_DO_SALARIO,
};

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Assinatura, Autor, Cancelamento, Centavos, Certeza, Ciclo,
    Competencia, Composicao, Conciliacao, ContaDoPlanejamento, Controle, Data, DiaDeCorte,
    DocumentoVinculado, EntradaDoHistorico, ErroDoPlanejamento, EstadoDaOcorrencia,
    EstadoDaProposta, Excecao, ExcecaoGravada, Exclusao, Filtro, Fonte, Frequencia, Grupo,
    HORIZONTE_MESES, IdOcorrencia, MovimentoCandidato, MovimentoDoExtrato, Natureza, Ocorrencia,
    OrigemImportada, ParDeConta, Pausa, Periodo, Planejamento, Prazo, Proposta, Recibo, Recorrente,
    Resultado, Resumo, Rubrica, Saldo, Sentido, Simulacao, TipoDeDocumento, TipoDeRubrica,
    TotaisDoMes, Versao,
};

/// Depois deste ponto, a data de "hoje" da demonstração. A janela passa `hoje` em toda
/// chamada; este valor só serve para o `novo` padrão.
pub const HOJE_DA_DEMONSTRACAO: &str = "2026-11-16";

#[derive(Debug, Clone)]
pub struct PlanejamentoFicticio {
    recorrentes: Vec<Recorrente>,
    grupos: Vec<Grupo>,
    excecoes: Vec<ExcecaoGravada>,
    conciliacoes: Vec<Conciliacao>,
    /// Os pares (ocorrência, movimento) cuja conciliação foi desfeita alguma vez: uma regra
    /// não os refaz sozinha.
    desfeitas: Vec<(IdOcorrencia, String)>,
    /// Os pares conciliados por uma regra (as três provas).
    automaticas: Vec<(IdOcorrencia, String)>,
    propostas: Vec<Proposta>,
    historico: Vec<EntradaDoHistorico>,
    composicoes: Vec<Composicao>,
    documentos: Vec<DocumentoVinculado>,
    saldos: Vec<Saldo>,
    /// O extrato fictício (a Pluggy da demonstração).
    extrato: Vec<MovimentoDoExtrato>,
    /// Apelido do planejamento -> conta do extrato, os pares confirmados.
    mapa_contas: Vec<(String, String)>,
    assinaturas: Vec<(String, Assinatura)>,
    /// Recorrentes cujo valor "disponível" antecede o crédito (feriado bancário), em dias.
    antecipa_dias: Vec<(String, i64)>,
    contador: u64,
    hoje: Data,
}

fn versao(
    valor: Option<Centavos>,
    certeza: Certeza,
    fonte: Fonte,
    frequencia: Frequencia,
    desde: Competencia,
    conta: &str,
    centro: &str,
) -> Versao {
    Versao {
        id: "v1".into(),
        desde,
        valor,
        certeza,
        fonte,
        frequencia,
        conta: Some(conta.into()),
        centro: Some(centro.into()),
        fornecedor: None,
        categoria: None,
        nota: None,
    }
}

impl PlanejamentoFicticio {
    /// Os dados da demonstração, relativos a `hoje`.
    pub fn novo(hoje: Data) -> Self {
        let h = Competencia::de_data(hoje);
        let inicio = h.somar_meses(-14);
        let grupos = [
            "Entradas",
            "Essencial",
            "Lar",
            "Parcial",
            "Dívidas",
            "Lazer",
            "Folha",
            "Passagem",
            "Achados na Pluggy",
        ]
        .iter()
        .enumerate()
        .map(|(i, n)| Grupo {
            nome: (*n).into(),
            posicao: (i as i64) * 10,
        })
        .collect();
        let mut s = PlanejamentoFicticio {
            recorrentes: Vec::new(),
            grupos,
            excecoes: Vec::new(),
            conciliacoes: Vec::new(),
            desfeitas: Vec::new(),
            automaticas: Vec::new(),
            propostas: Vec::new(),
            historico: Vec::new(),
            composicoes: Vec::new(),
            documentos: Vec::new(),
            saldos: Vec::new(),
            extrato: Vec::new(),
            mapa_contas: Vec::new(),
            assinaturas: Vec::new(),
            antecipa_dias: vec![("rec_salario".into(), 2)],
            contador: 0,
            hoje,
        };
        let mensal = |dia| Frequencia::Mensal { dia };
        let cx = "CX-0001";
        let nu = "Nubank";
        // (id, nome, grupo, sub, natureza, prazo, versão)
        #[allow(clippy::type_complexity)]
        let base: Vec<(&str, &str, &str, Option<&str>, Natureza, Prazo, Versao)> = vec![
            (
                "rec_salario",
                "Salário líquido (Exemplo)",
                "Entradas",
                None,
                Natureza::Entrada,
                Prazo::SemFim,
                versao(
                    Some(Centavos(845_000)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(5),
                    inicio,
                    cx,
                    "Pessoal",
                ),
            ),
            (
                "rec_casa",
                "Transferência da casa",
                "Entradas",
                None,
                Natureza::Entrada,
                Prazo::SemFim,
                versao(
                    Some(Centavos(120_000)),
                    Certeza::Estimado,
                    Fonte::Planilha,
                    mensal(10),
                    inicio,
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_aluguel",
                "Aluguel do apartamento",
                "Lar",
                Some("Moradia"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(230_000)),
                    Certeza::Confirmado,
                    Fonte::Contrato,
                    mensal(10),
                    inicio,
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_condominio",
                "Condomínio",
                "Lar",
                Some("Moradia"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(48_000)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(12),
                    inicio,
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_energia",
                "Energia elétrica",
                "Lar",
                Some("Contas"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    None,
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(18),
                    inicio,
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_agua",
                "Água e esgoto",
                "Lar",
                Some("Contas"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(9_640)),
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(20),
                    inicio,
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_internet",
                "Internet fibra",
                "Lar",
                Some("Contas"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(11_990)),
                    Certeza::Confirmado,
                    Fonte::Contrato,
                    mensal(15),
                    inicio,
                    nu,
                    "Casa",
                ),
            ),
            (
                "rec_iptu",
                "IPTU (parcelas)",
                "Lar",
                Some("Impostos"),
                Natureza::Saida,
                Prazo::Parcelas {
                    total: 10,
                    primeira: h.somar_meses(-2),
                },
                versao(
                    Some(Centavos(21_500)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(10),
                    h.somar_meses(-2),
                    cx,
                    "Casa",
                ),
            ),
            (
                "rec_mercado",
                "Mercado do mês",
                "Essencial",
                Some("Alimentação"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(150_000)),
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(8),
                    inicio,
                    nu,
                    "Casa",
                ),
            ),
            (
                "rec_saude",
                "Plano de saúde",
                "Essencial",
                Some("Saúde"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(64_000)),
                    Certeza::Confirmado,
                    Fonte::Contrato,
                    mensal(7),
                    inicio,
                    cx,
                    "Pessoal",
                ),
            ),
            (
                "rec_farmacia",
                "Farmácia",
                "Essencial",
                Some("Saúde"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(22_000)),
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(25),
                    inicio,
                    nu,
                    "Pessoal",
                ),
            ),
            (
                "rec_seguro",
                "Seguro do carro",
                "Essencial",
                Some("Transporte"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(290_000)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    Frequencia::Anual {
                        mes: h.somar_meses(2).mes,
                        dia: 20,
                    },
                    inicio,
                    cx,
                    "Pessoal",
                ),
            ),
            (
                "rec_financiamento",
                "Financiamento do carro",
                "Dívidas",
                None,
                Natureza::Saida,
                Prazo::Parcelas {
                    total: 48,
                    primeira: h.somar_meses(-10),
                },
                versao(
                    Some(Centavos(115_000)),
                    Certeza::Confirmado,
                    Fonte::Contrato,
                    mensal(14),
                    h.somar_meses(-10),
                    cx,
                    "Pessoal",
                ),
            ),
            (
                "rec_emprestimo",
                "Empréstimo pessoal",
                "Parcial",
                Some("Empréstimos"),
                Natureza::Saida,
                Prazo::AteCompetencia(h.somar_meses(7)),
                versao(
                    Some(Centavos(78_000)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(9),
                    inicio,
                    cx,
                    "Pessoal",
                ),
            ),
            (
                "rec_streaming",
                "Streaming de vídeo",
                "Lazer",
                None,
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(3_990)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(21),
                    inicio,
                    nu,
                    "Pessoal",
                ),
            ),
            (
                "rec_revista",
                "Revista digital",
                "Lazer",
                None,
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(2_990)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(22),
                    inicio,
                    nu,
                    "Pessoal",
                ),
            ),
            (
                "rec_presente",
                "Presente de aniversário",
                "Lar",
                Some("Eventos"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(35_000)),
                    Certeza::Estimado,
                    Fonte::Usuario,
                    Frequencia::Unica {
                        data: h.somar_meses(3).dia(15),
                    },
                    h.somar_meses(3),
                    nu,
                    "Pessoal",
                ),
            ),
            (
                "rec_consignado",
                "Consignado do banco (folha)",
                "Folha",
                None,
                Natureza::DescontoEmFolha,
                Prazo::SemFim,
                versao(
                    Some(Centavos(62_000)),
                    Certeza::Confirmado,
                    Fonte::Contrato,
                    mensal(5),
                    inicio,
                    "Salário",
                    "Pessoal",
                ),
            ),
            (
                "rec_previdencia",
                "Previdência (folha)",
                "Folha",
                None,
                Natureza::DescontoEmFolha,
                Prazo::SemFim,
                versao(
                    Some(Centavos(54_000)),
                    Certeza::Confirmado,
                    Fonte::Planilha,
                    mensal(5),
                    inicio,
                    "Salário",
                    "Pessoal",
                ),
            ),
            (
                "rec_bolao_entrada",
                "Bolão: cotas recebidas",
                "Passagem",
                None,
                Natureza::Passagem {
                    sentido: Sentido::Entrada,
                },
                Prazo::SemFim,
                versao(
                    Some(Centavos(40_000)),
                    Certeza::Estimado,
                    Fonte::Usuario,
                    mensal(3),
                    inicio,
                    cx,
                    "Bolão",
                ),
            ),
            (
                "rec_bolao_saida",
                "Bolão: repasse",
                "Passagem",
                None,
                Natureza::Passagem {
                    sentido: Sentido::Saida,
                },
                Prazo::SemFim,
                versao(
                    Some(Centavos(30_000)),
                    Certeza::Estimado,
                    Fonte::Usuario,
                    mensal(4),
                    inicio,
                    cx,
                    "Bolão",
                ),
            ),
            (
                "rec_padaria_cartao",
                "Padaria (cartão)",
                "Achados na Pluggy",
                Some("No cartão"),
                Natureza::DentroDaFatura,
                Prazo::SemFim,
                versao(
                    Some(Centavos(18_000)),
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(12),
                    inicio,
                    nu,
                    "Casa",
                ),
            ),
            (
                "rec_assinatura_debito",
                "Assinatura de música (débito)",
                "Achados na Pluggy",
                Some("Débito em conta"),
                Natureza::Saida,
                Prazo::SemFim,
                versao(
                    Some(Centavos(2_190)),
                    Certeza::Estimado,
                    Fonte::Pluggy,
                    mensal(11),
                    inicio,
                    cx,
                    "Pessoal",
                ),
            ),
        ];
        for (i, (id, nome, grupo, sub, natureza, prazo, v)) in base.into_iter().enumerate() {
            s.recorrentes.push(Recorrente {
                id: id.into(),
                nome: nome.into(),
                grupo: grupo.into(),
                subgrupo: sub.map(Into::into),
                posicao: (i as i64 + 1) * 10,
                natureza,
                prazo,
                versoes: vec![v],
                pausas: Vec::new(),
                cancelamento: None,
                arquivado: false,
                excluido: None,
                versao_do_registro: 1,
            });
        }
        // O streaming para no mês que vem (viagem); a revista foi cancelada no mês passado.
        if let Some(r) = s.recorrentes.iter_mut().find(|r| r.id == "rec_streaming") {
            r.pausas.push(Pausa {
                id: "pausa_1".into(),
                desde: h.seguinte(),
                ate: Some(h.somar_meses(3)),
                motivo: "Viagem de fim de ano".into(),
            });
        }
        if let Some(r) = s.recorrentes.iter_mut().find(|r| r.id == "rec_revista") {
            r.cancelamento = Some(Cancelamento {
                desde: h.somar_meses(-1),
                motivo: "Não leio mais".into(),
                comprovado: false,
            });
        }
        // Saldos: um fresco e um desatualizado.
        s.saldos = vec![
            Saldo {
                conta: "CX-0001".into(),
                valor: Centavos(341_270),
                quando: format!("{hoje}T10:30:00Z"),
            },
            Saldo {
                conta: "Nubank".into(),
                valor: Centavos(128_015),
                quando: format!("{}T10:30:00Z", hoje.somar_dias(-3)),
            },
        ];
        s.semear_pagos(hoje);
        s.semear_propostas();
        s.semear_composicao_e_documentos(inicio, h);
        s.semear_extrato(hoje);
        s
    }

    /// As rubricas do salário fictício (que somam diferente do digitado, para mostrar o aviso)
    /// e alguns documentos vinculados.
    fn semear_composicao_e_documentos(&mut self, inicio: Competencia, h: Competencia) {
        let r = |codigo: &str,
                 nome: &str,
                 tipo: TipoDeRubrica,
                 valor: i64,
                 competencia: Option<Competencia>,
                 prazo: Option<u32>,
                 recorrente: Option<&str>| Rubrica {
            codigo: Some(codigo.into()),
            nome: nome.into(),
            tipo,
            valor: Centavos(valor),
            competencia,
            prazo,
            recorrente: recorrente.map(Into::into),
        };
        use TipoDeRubrica::{Desconto, Provento};
        self.composicoes.push(Composicao {
            recorrente: "rec_salario".into(),
            desde: inicio,
            rubricas: vec![
                r("1001", "Vencimento", Provento, 1_020_000, None, None, None),
                r(
                    "1005",
                    "Adicional por tempo de serviço",
                    Provento,
                    115_000,
                    None,
                    None,
                    None,
                ),
                r("2001", "INSS", Desconto, 95_162, None, None, None),
                r(
                    "2002",
                    "Imposto de renda",
                    Desconto,
                    101_238,
                    None,
                    None,
                    None,
                ),
                r(
                    "2040",
                    "Consignado do banco",
                    Desconto,
                    62_000,
                    Competencia::nova(h.ano - 1, 3),
                    Some(84),
                    Some("rec_consignado"),
                ),
                r(
                    "2050",
                    "Previdência",
                    Desconto,
                    54_000,
                    None,
                    None,
                    Some("rec_previdencia"),
                ),
            ],
            documento: Some(crate::demonstracao::hash_do_nome(
                "contracheque-2026-11.pdf",
            )),
            origem: format!("contracheque {:02}/{}", h.mes, h.ano),
        });
        let agora = self.agora();
        let mut vincular =
            |alvo: AlvoDoDocumento, nome: &str, tipo: TipoDeDocumento, data: &str| {
                self.contador += 1;
                self.documentos.push(DocumentoVinculado {
                    id: format!("doc_{}", self.contador),
                    alvo,
                    sha256: crate::demonstracao::hash_do_nome(nome),
                    tipo,
                    nome: nome.into(),
                    data: Data::de_iso(data),
                    quando: agora.clone(),
                    autor: Autor::Usuario,
                    desvinculado: false,
                });
            };
        vincular(
            AlvoDoDocumento::Recorrente {
                id: "rec_salario".into(),
            },
            "contracheque-2026-11.pdf",
            TipoDeDocumento::Contracheque,
            "2026-11-05",
        );
        vincular(
            AlvoDoDocumento::Ocorrencia {
                id: IdOcorrencia {
                    recorrente: "rec_energia".into(),
                    competencia: h.seguinte(),
                    parcela: None,
                },
            },
            "fatura-2026-11-recebida.pdf",
            TipoDeDocumento::Fatura,
            "2026-11-10",
        );
    }

    /// O que já passou (até 70 dias) está pago, menos poucos casos que ficam em atraso.
    fn semear_pagos(&mut self, hoje: Data) {
        let h = Competencia::de_data(hoje);
        let sem_pagar = ["rec_iptu", "rec_agua"];
        let mut n = 0;
        let ids: Vec<String> = self.recorrentes.iter().map(|r| r.id.clone()).collect();
        for id in ids {
            for comp in (-3..=0).map(|d| h.somar_meses(d)) {
                let Some(oc) = self.ocorrencia_de(&id, comp, hoje) else {
                    continue;
                };
                if oc.vencimento >= hoje || oc.vencimento < hoje.somar_dias(-70) {
                    continue;
                }
                // Os atrasados de propósito são os dos últimos 45 dias.
                if sem_pagar.contains(&id.as_str()) && oc.vencimento >= hoje.somar_dias(-45) {
                    continue;
                }
                // A energia não tem previsto (sem valor), mas o que foi pago tem valor.
                let Some(valor) = oc.previsto.or(Some(Centavos(31_250))) else {
                    continue;
                };
                n += 1;
                let sugestao =
                    oc.id.recorrente == "rec_mercado" && oc.vencimento >= hoje.somar_dias(-20);
                if (n % 3 == 0 || sugestao) && !matches!(oc.natureza, Natureza::DescontoEmFolha) {
                    self.conciliacoes.push(Conciliacao {
                        ocorrencia: oc.id.clone(),
                        movimento: format!("pluggy:txn_ficticia_{n:03}"),
                        valor,
                        data: oc.vencimento,
                        confirmada: !sugestao,
                    });
                } else {
                    self.excecoes.push(ExcecaoGravada {
                        id: format!("exc_semente_{n}"),
                        ocorrencia: oc.id.clone(),
                        excecao: Excecao::PagoInformado {
                            valor,
                            data: Some(oc.vencimento),
                        },
                        quando: self.agora(),
                        autor: Autor::Usuario,
                        desfeita: false,
                    });
                }
            }
        }
    }

    fn semear_propostas(&mut self) {
        let h = Competencia::de_data(self.hoje);
        let internet = self
            .recorrente_interno("rec_internet")
            .map(|r| r.versao_do_registro);
        let streaming = self
            .recorrente_interno("rec_streaming")
            .map(|r| r.versao_do_registro);
        if let Some(base) = internet {
            let _ = self.propor(
                Alteracao::Editar {
                    recorrente: "rec_internet".into(),
                    escopo: sbf_planejamento::Escopo::DaquiEmDiante(h.somar_meses(2)),
                    mudanca: sbf_planejamento::MudancaDeVersao {
                        valor: Some(Some(Centavos(13_990))),
                        ..Default::default()
                    },
                    versao_base: base,
                },
                "ocultos",
                "internet-reajuste",
            );
        }
        if let Some(base) = streaming {
            let _ = self.propor(
                Alteracao::Cancelar {
                    recorrente: "rec_streaming".into(),
                    desde: h.somar_meses(4),
                    motivo: "Troquei de plano".into(),
                    comprovado: false,
                    versao_base: base,
                },
                "mcp",
                "streaming-cancelar",
            );
        }
    }

    fn agora(&self) -> String {
        format!("{}T12:{:02}:00Z", self.hoje, self.contador % 60)
    }

    fn recorrente_interno(&self, id: &str) -> Option<&Recorrente> {
        self.recorrentes.iter().find(|r| r.id == id)
    }

    fn proximo_id(&mut self, prefixo: &str) -> String {
        self.contador += 1;
        format!("{prefixo}_{}", self.contador)
    }

    // ------------------------------------------------------------------ projeção

    fn versao_vigente(r: &Recorrente, comp: Competencia) -> Option<&Versao> {
        r.versoes
            .iter()
            .rev()
            .find(|v| v.desde <= comp)
            .or_else(|| r.versoes.first())
    }

    /// A regra gera ocorrência nesta competência? E com que parcela e dia?
    fn gera(r: &Recorrente, comp: Competencia) -> Option<(Option<u32>, Data)> {
        let v = Self::versao_vigente(r, comp)?;
        if comp < r.versoes.first()?.desde {
            return None;
        }
        let data = match v.frequencia {
            Frequencia::Mensal { dia } => comp.dia(dia),
            Frequencia::CadaNMeses { n, dia, primeira } => {
                let d = primeira.meses_ate(comp);
                if n == 0 || d < 0 || d % i32::from(n) != 0 {
                    return None;
                }
                comp.dia(dia)
            }
            Frequencia::Anual { mes, dia } => {
                if comp.mes != mes {
                    return None;
                }
                comp.dia(dia)
            }
            Frequencia::Unica { data } => {
                if Competencia::de_data(data) != comp {
                    return None;
                }
                data
            }
        };
        let parcela = match r.prazo {
            Prazo::SemFim | Prazo::Desconhecido => None,
            Prazo::AteCompetencia(ate) => {
                if comp >= ate {
                    return None;
                }
                None
            }
            Prazo::Parcelas { total, primeira } => {
                let i = primeira.meses_ate(comp);
                if i < 0 || i >= total as i32 {
                    return None;
                }
                Some(i as u32 + 1)
            }
        };
        Some((parcela, data))
    }

    fn excecoes_de(&self, id: &IdOcorrencia) -> impl Iterator<Item = &ExcecaoGravada> {
        self.excecoes
            .iter()
            .filter(move |e| !e.desfeita && &e.ocorrencia == id)
    }

    /// A ocorrência de um recorrente numa competência, com tudo aplicado.
    fn ocorrencia_de(&self, recorrente: &str, comp: Competencia, hoje: Data) -> Option<Ocorrencia> {
        let r = self.recorrente_interno(recorrente)?;
        let (parcela, data_regra) = Self::gera(r, comp)?;
        let v = Self::versao_vigente(r, comp)?;
        let id = IdOcorrencia {
            recorrente: r.id.clone(),
            competencia: comp,
            parcela,
        };
        let antecipa = self
            .antecipa_dias
            .iter()
            .find(|(i, _)| *i == r.id)
            .map_or(0, |(_, d)| *d);
        let mut vencimento_original = data_regra;
        let mut vencimento = data_regra.somar_dias(-antecipa);
        if antecipa != 0 {
            // O crédito é o dia da regra; a disponibilidade vem antes (feriado bancário).
            vencimento_original = data_regra;
        }
        let mut previsto = v.valor;
        if matches!(r.natureza, Natureza::DescontoEmFolha)
            && let Some(seguido) = self.valor_seguido_pela_rubrica(&r.id, comp)
        {
            previsto = Some(seguido);
        }
        let mut pago_informado = None;
        let mut adiada = false;
        let mut dispensada = false;
        for e in self.excecoes_de(&id) {
            match &e.excecao {
                Excecao::Adiar { nova_data } => {
                    adiada = true;
                    if let Some(d) = nova_data {
                        vencimento = *d;
                    }
                }
                Excecao::Dispensar { .. } => dispensada = true,
                Excecao::ValorSoDesteMes { valor } => previsto = Some(*valor),
                Excecao::PagoInformado { valor, .. } => pago_informado = Some(*valor),
                // O conjunto fictício não tem modelos: não há cálculo a ajustar.
                Excecao::QuantidadeSoDesteMes { .. } => {}
            }
        }
        let confirmadas: Vec<Centavos> = self
            .conciliacoes
            .iter()
            .filter(|x| x.ocorrencia == id && x.confirmada)
            .map(|x| x.valor)
            .collect();
        let conciliado = (!confirmadas.is_empty()).then(|| confirmadas.iter().copied().sum());
        let sugestao = self
            .conciliacoes
            .iter()
            .any(|x| x.ocorrencia == id && !x.confirmada);
        let paga_algo = pago_informado.is_some() || conciliado.is_some();
        let pausa = r
            .pausas
            .iter()
            .any(|p| p.desde <= comp && p.ate.is_none_or(|a| comp < a));
        let cancelada = r.cancelamento.as_ref().is_some_and(|x| comp >= x.desde);
        let estado = if dispensada {
            EstadoDaOcorrencia::Dispensada
        } else if (pausa || cancelada) && !paga_algo && !adiada {
            if cancelada {
                EstadoDaOcorrencia::SuprimidaPorCancelamento
            } else {
                EstadoDaOcorrencia::SuprimidaPorPausa
            }
        } else {
            let pago = conciliado.or(pago_informado);
            match (pago, previsto) {
                (Some(p), Some(prev)) if p < prev => EstadoDaOcorrencia::Parcial,
                (Some(_), _) if conciliado.is_some() => EstadoDaOcorrencia::Conciliada,
                (Some(_), _) => EstadoDaOcorrencia::PagoInformado,
                (None, _) if adiada && vencimento >= hoje => EstadoDaOcorrencia::Adiada,
                (None, _) if vencimento < hoje => EstadoDaOcorrencia::EmAtraso,
                _ => EstadoDaOcorrencia::Prevista,
            }
        };
        let total = match r.prazo {
            Prazo::Parcelas { total, .. } => Some(total),
            _ => None,
        };
        Some(Ocorrencia {
            id,
            nome: r.nome.clone(),
            grupo: r.grupo.clone(),
            subgrupo: r.subgrupo.clone(),
            posicao: r.posicao,
            natureza: r.natureza,
            vencimento,
            vencimento_original,
            previsto,
            certeza: v.certeza,
            fonte: v.fonte,
            pago_informado,
            conciliado,
            estado,
            parcela: parcela.zip(total),
            conta: v.conta.clone(),
            centro: v.centro.clone(),
            fornecedor: v.fornecedor.clone(),
            versao: v.id.clone(),
            prazo_desconhecido: matches!(r.prazo, Prazo::Desconhecido),
            sugestao_de_conciliacao: sugestao,
            calculo: None,
        })
    }

    fn visivel(&self, r: &Recorrente) -> bool {
        r.excluido.is_none() && !r.arquivado
    }

    /// Todas as ocorrências cujo vencimento efetivo cai em `[inicio, fim]`.
    fn no_intervalo(&self, inicio: Data, fim: Data, hoje: Data) -> Vec<Ocorrencia> {
        let a = Competencia::de_data(inicio).somar_meses(-2);
        let b = Competencia::de_data(fim).somar_meses(2);
        let n = a.meses_ate(b);
        let mut v = Vec::new();
        for r in self.recorrentes.iter().filter(|r| self.visivel(r)) {
            for d in 0..=n {
                let comp = a.somar_meses(d);
                if let Some(o) = self.ocorrencia_de(&r.id, comp, hoje)
                    && o.vencimento >= inicio
                    && o.vencimento <= fim
                {
                    v.push(o);
                }
            }
        }
        v
    }

    fn posicao_do_grupo(&self, nome: &str) -> i64 {
        self.grupos
            .iter()
            .find(|g| g.nome == nome)
            .map_or(i64::MAX, |g| g.posicao)
    }

    fn ordenar(&self, linhas: &mut [Ocorrencia]) {
        linhas.sort_by(|a, b| {
            self.posicao_do_grupo(&a.grupo)
                .cmp(&self.posicao_do_grupo(&b.grupo))
                .then(a.posicao.cmp(&b.posicao))
                .then(a.vencimento.cmp(&b.vencimento))
                .then(a.nome.cmp(&b.nome))
        });
    }

    fn suprimida(e: EstadoDaOcorrencia) -> bool {
        matches!(
            e,
            EstadoDaOcorrencia::SuprimidaPorPausa | EstadoDaOcorrencia::SuprimidaPorCancelamento
        )
    }

    fn passa(filtro: Filtro, busca: Option<&str>, o: &Ocorrencia) -> bool {
        use EstadoDaOcorrencia as E;
        let ok = match filtro {
            Filtro::PausadosECancelados => Self::suprimida(o.estado),
            _ if Self::suprimida(o.estado) => false,
            Filtro::Todos => true,
            Filtro::APagar => {
                matches!(o.estado, E::Prevista | E::Parcial | E::EmAtraso | E::Adiada)
                    && matches!(o.natureza, Natureza::Saida)
            }
            Filtro::EmAtraso => o.estado == E::EmAtraso,
            Filtro::Pagos => matches!(o.estado, E::PagoInformado | E::Conciliada),
            Filtro::SemComprovante => o.estado == E::PagoInformado,
            Filtro::SemValor => o.previsto.is_none(),
        };
        let busca_ok = match busca.map(str::trim).filter(|b| !b.is_empty()) {
            None => true,
            Some(b) => {
                let b = b.to_lowercase();
                [
                    Some(&o.nome),
                    o.conta.as_ref(),
                    o.centro.as_ref(),
                    o.fornecedor.as_ref(),
                    Some(&o.grupo),
                ]
                .into_iter()
                .flatten()
                .any(|t| t.to_lowercase().contains(&b))
            }
        };
        ok && busca_ok
    }

    fn falta_de(o: &Ocorrencia) -> Centavos {
        match (o.previsto, o.estado) {
            (_, EstadoDaOcorrencia::Dispensada) => Centavos::ZERO,
            (Some(p), _) => (p - o.conciliado.or(o.pago_informado).unwrap_or_default()).ou_zero(),
            (None, _) => Centavos::ZERO,
        }
    }

    fn resumo(&self, linhas: &[Ocorrencia], atrasadas: &[Ocorrencia], hoje: Data) -> Resumo {
        use EstadoDaOcorrencia as E;
        let conta = |o: &&Ocorrencia| !Self::suprimida(o.estado) && o.estado != E::Dispensada;
        let mut r = Resumo {
            disponivel_agora: self.saldos.clone(),
            ..Resumo::default()
        };
        let mut estimado = false;
        let mut sem_valor = 0u32;
        let mut estimados = 0u32;
        let mut saidas_a_pagar = Centavos::ZERO;
        let mut entradas_a_receber = Centavos::ZERO;
        let (mut p_entra, mut p_sai) = (Centavos::ZERO, Centavos::ZERO);
        for o in linhas.iter().filter(conta) {
            let prev = o.previsto.unwrap_or_default();
            if o.previsto.is_none() {
                sem_valor += 1;
            } else if o.certeza == Certeza::Estimado {
                estimados += 1;
            }
            if o.previsto.is_none() || (o.pago_informado.is_some() && o.conciliado.is_none()) {
                r.a_conferir += 1;
            }
            match o.natureza {
                Natureza::Entrada => {
                    r.entradas_previstas += prev;
                    if o.pago_informado.is_none() && o.conciliado.is_none() {
                        entradas_a_receber += prev;
                    }
                }
                Natureza::Saida => {
                    r.saidas_de_caixa_previstas += prev;
                    if o.estado == E::EmAtraso {
                        r.em_atraso += Self::falta_de(o);
                    } else {
                        saidas_a_pagar += Self::falta_de(o);
                    }
                }
                Natureza::DescontoEmFolha => r.descontos_em_folha += prev,
                Natureza::Passagem {
                    sentido: Sentido::Entrada,
                } => p_entra += prev,
                Natureza::Passagem {
                    sentido: Sentido::Saida,
                } => p_sai += prev,
                Natureza::DentroDaFatura => r.compras_no_cartao += prev,
            }
        }
        for o in atrasadas {
            r.em_atraso += Self::falta_de(o);
            if o.previsto.is_none() {
                sem_valor += 1;
            }
        }
        r.passagem_pendente = (p_entra - p_sai).ou_zero();
        let disponivel: Centavos = self.saldos.iter().map(|s| s.valor).sum();
        r.livre_apos_compromissos =
            disponivel + entradas_a_receber - saidas_a_pagar - r.em_atraso - r.passagem_pendente;
        if self.saldos.is_empty() {
            r.lacunas
                .push("Sem saldo coletado: o disponível agora é desconhecido.".into());
            estimado = true;
        }
        for s in &self.saldos {
            let coleta = s.quando.get(..10).and_then(Data::de_iso);
            if coleta.is_none_or(|d| d.somar_dias(1) < hoje) {
                r.lacunas
                    .push(format!("O saldo de {} está desatualizado.", s.conta));
                estimado = true;
            }
        }
        if sem_valor > 0 {
            r.lacunas.push(format!(
                "{sem_valor} ocorrência(s) sem valor entram como zero na conta."
            ));
            estimado = true;
        }
        if estimados > 0 {
            r.lacunas.push(format!(
                "{estimados} valor(es) são estimados, não confirmados."
            ));
            estimado = true;
        }
        r.livre_e_estimativa = estimado;
        r
    }

    fn periodo_mes_inicio(periodo: &Periodo) -> Data {
        periodo.inicio()
    }
}

fn dia_do_mes(comp: Competencia, dia: u8) -> Data {
    comp.dia(dia)
}

impl Planejamento for PlanejamentoFicticio {
    fn controle(
        &self,
        periodo: &Periodo,
        filtro: Filtro,
        busca: Option<&str>,
        hoje: Data,
    ) -> Resultado<Controle> {
        let inicio = Self::periodo_mes_inicio(periodo);
        let todas = self.no_intervalo(inicio, periodo.fim(), hoje);
        let mut linhas: Vec<Ocorrencia> = todas
            .iter()
            .filter(|o| Self::passa(filtro, busca, o))
            .cloned()
            .collect();
        self.ordenar(&mut linhas);
        // Em atraso de antes: o que venceu antes do período (e antes de hoje), segue devido.
        let corte_atraso = inicio.min(hoje);
        let mut atrasadas: Vec<Ocorrencia> = if inicio <= hoje {
            self.no_intervalo(hoje.somar_dias(-70), corte_atraso.somar_dias(-1), hoje)
                .into_iter()
                .filter(|o| o.estado == EstadoDaOcorrencia::EmAtraso && o.vencimento < inicio)
                .collect()
        } else {
            Vec::new()
        };
        self.ordenar(&mut atrasadas);
        let atrasadas_filtradas: Vec<Ocorrencia> = atrasadas
            .iter()
            .filter(|o| busca.is_none_or(|_| Self::passa(Filtro::Todos, busca, o)))
            .cloned()
            .collect();
        let resumo = self.resumo(&todas, &atrasadas, hoje);
        let mut grupos = self.grupos.clone();
        grupos.sort_by_key(|g| g.posicao);
        Ok(Controle {
            periodo: *periodo,
            grupos,
            linhas,
            atrasadas_de_antes: atrasadas_filtradas,
            resumo,
        })
    }

    fn recorrentes(&self, incluir_excluidos: bool) -> Resultado<Vec<Recorrente>> {
        Ok(self
            .recorrentes
            .iter()
            .filter(|r| incluir_excluidos || r.excluido.is_none())
            .cloned()
            .collect())
    }

    fn recorrente(&self, id: &str) -> Resultado<Option<Recorrente>> {
        Ok(self.recorrente_interno(id).cloned())
    }

    fn grupos(&self) -> Resultado<Vec<Grupo>> {
        let mut g = self.grupos.clone();
        g.sort_by_key(|g| g.posicao);
        Ok(g)
    }

    fn proximas(
        &self,
        recorrente: &str,
        desde: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<Ocorrencia>> {
        if self.recorrente_interno(recorrente).is_none() {
            return Err(ErroDoPlanejamento::NaoEncontrado(recorrente.into()));
        }
        let meses = meses.min(HORIZONTE_MESES);
        Ok((0..i32::from(meses))
            .filter_map(|d| self.ocorrencia_de(recorrente, desde.somar_meses(d), hoje))
            .collect())
    }

    fn calendario(
        &self,
        inicio: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<TotaisDoMes>> {
        if meses > HORIZONTE_MESES {
            return Err(ErroDoPlanejamento::Invalida(format!(
                "no máximo {HORIZONTE_MESES} meses"
            )));
        }
        let mut v = Vec::new();
        for d in 0..i32::from(meses) {
            let comp = inicio.somar_meses(d);
            let linhas = self.no_intervalo(comp.dia(1), comp.dia(31), hoje);
            let mut t = TotaisDoMes {
                competencia: comp,
                entradas: Centavos::ZERO,
                saidas_de_caixa: Centavos::ZERO,
                descontos_em_folha: Centavos::ZERO,
                sem_valor: 0,
                estimado: false,
            };
            for o in linhas.iter().filter(|o| !Self::suprimida(o.estado)) {
                let prev = o.previsto.unwrap_or_default();
                t.estimado |= o.certeza == Certeza::Estimado;
                match o.natureza {
                    Natureza::Entrada => t.entradas += prev,
                    Natureza::Saida => t.saidas_de_caixa += prev,
                    Natureza::DescontoEmFolha => t.descontos_em_folha += prev,
                    Natureza::Passagem { .. } | Natureza::DentroDaFatura => {}
                }
                if o.previsto.is_none() {
                    t.sem_valor += 1;
                }
            }
            v.push(t);
        }
        Ok(v)
    }

    fn simular(&self, alteracao: &Alteracao, hoje: Data) -> Resultado<Simulacao> {
        let mut depois = self.clone();
        let alvo = alvo_da(alteracao);
        let comp_inicial = comp_da(alteracao).unwrap_or(Competencia::de_data(hoje));
        depois.aplicar_interno(alteracao.clone(), Autor::Usuario)?;
        // Um compromisso novo ainda não tem id: o da cópia é o último criado.
        let (alvo, comp_inicial) = match alteracao {
            Alteracao::CriarRecorrente { recorrente, .. } => (
                depois.recorrentes.last().map(|r| r.id.clone()),
                recorrente.primeira_versao.desde,
            ),
            _ => (alvo, comp_inicial),
        };
        let Some(rec) = alvo else {
            return Ok(Simulacao {
                meses_afetados: Vec::new(),
                mudancas: Vec::new(),
                conflitos: Vec::new(),
            });
        };
        let h = Competencia::de_data(hoje);
        let ini = comp_inicial.min(h);
        let mut meses_afetados = Vec::new();
        let mut mudancas = Vec::new();
        for d in 0..48 {
            let comp = ini.somar_meses(d);
            let a = self.ocorrencia_de(&rec, comp, hoje);
            let b = depois.ocorrencia_de(&rec, comp, hoje);
            let chave = |o: &Option<Ocorrencia>| {
                o.as_ref().map(|o| {
                    (
                        o.previsto,
                        o.estado,
                        o.vencimento,
                        o.conta.clone(),
                        o.grupo.clone(),
                    )
                })
            };
            if chave(&a) != chave(&b) {
                meses_afetados.push(comp);
                let fora = |o: &Option<Ocorrencia>| {
                    o.as_ref()
                        .filter(|o| !Self::suprimida(o.estado))
                        .and_then(|o| o.previsto)
                };
                mudancas.push((comp, fora(&a), fora(&b)));
            }
        }
        let mut conflitos = Vec::new();
        for comp in &meses_afetados {
            let Some(o) = self.ocorrencia_de(&rec, *comp, hoje) else {
                continue;
            };
            if o.pago_informado.is_some() || o.conciliado.is_some() {
                conflitos.push(format!(
                    "{:02}/{:04}: há pagamento informado ou conciliado; ele continua valendo e visível.",
                    comp.mes, comp.ano
                ));
            }
            if self.excecoes_de(&o.id).next().is_some()
                && matches!(alteracao, Alteracao::Editar { .. })
            {
                conflitos.push(format!(
                    "{:02}/{:04}: já existe uma exceção neste mês; ela prevalece.",
                    comp.mes, comp.ano
                ));
            }
        }
        Ok(Simulacao {
            meses_afetados,
            mudancas,
            conflitos,
        })
    }

    fn aplicar(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo> {
        self.aplicar_interno(alteracao, autor)
    }

    fn propor(&mut self, alteracao: Alteracao, origem: &str, chave: &str) -> Resultado<Proposta> {
        if let Some(p) = self.propostas.iter().find(|p| p.chave == chave) {
            return Ok(p.clone());
        }
        let id = self.proximo_id("prop");
        let p = Proposta {
            id,
            alteracao,
            origem: origem.into(),
            chave: chave.into(),
            criada_em: self.agora(),
            expira_em: format!("{}T23:59:00Z", self.hoje.somar_dias(7)),
            estado: EstadoDaProposta::Pendente,
            motivo: None,
        };
        self.propostas.push(p.clone());
        Ok(p)
    }

    fn propostas(&self, so_pendentes: bool) -> Resultado<Vec<Proposta>> {
        Ok(self
            .propostas
            .iter()
            .filter(|p| !so_pendentes || p.estado == EstadoDaProposta::Pendente)
            .cloned()
            .collect())
    }

    fn confirmar_proposta(&mut self, id: &str) -> Resultado<Recibo> {
        let i = self
            .propostas
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(id.into()))?;
        if self.propostas[i].estado != EstadoDaProposta::Pendente {
            return Err(ErroDoPlanejamento::Invalida(
                "a proposta não está pendente".into(),
            ));
        }
        let alteracao = self.propostas[i].alteracao.clone();
        match self.aplicar_interno(alteracao, Autor::Usuario) {
            Ok(r) => {
                self.propostas[i].estado = EstadoDaProposta::Confirmada;
                Ok(r)
            }
            Err(ErroDoPlanejamento::VersaoVelha { atual }) => {
                self.propostas[i].estado = EstadoDaProposta::Expirada;
                self.propostas[i].motivo = Some("a linha mudou depois da proposta".into());
                Err(ErroDoPlanejamento::VersaoVelha { atual })
            }
            Err(e) => Err(e),
        }
    }

    fn recusar_proposta(&mut self, id: &str, motivo: &str) -> Resultado<()> {
        let p = self
            .propostas
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(id.into()))?;
        if p.estado != EstadoDaProposta::Pendente {
            return Err(ErroDoPlanejamento::Invalida(
                "a proposta não está pendente".into(),
            ));
        }
        p.estado = EstadoDaProposta::Recusada;
        p.motivo = Some(motivo.into());
        Ok(())
    }

    fn historico(&self, alvo: Option<&str>, limite: u32) -> Resultado<Vec<EntradaDoHistorico>> {
        Ok(self
            .historico
            .iter()
            .rev()
            .filter(|e| alvo.is_none_or(|a| e.alvo == a))
            .take(limite as usize)
            .cloned()
            .collect())
    }

    fn origem_importada(&self, _origem: &OrigemImportada) -> Resultado<Option<String>> {
        Ok(None)
    }

    fn excecoes_do_recorrente(&self, recorrente: &str) -> Resultado<Vec<ExcecaoGravada>> {
        Ok(self
            .excecoes
            .iter()
            .filter(|e| e.ocorrencia.recorrente == recorrente)
            .cloned()
            .collect())
    }

    fn composicao(
        &self,
        recorrente: &str,
        competencia: Competencia,
    ) -> Resultado<Option<Composicao>> {
        Ok(self
            .composicoes
            .iter()
            .filter(|c| c.recorrente == recorrente && c.desde <= competencia)
            .max_by_key(|c| c.desde)
            .cloned())
    }

    fn documentos(&self, alvo: &AlvoDoDocumento) -> Resultado<Vec<DocumentoVinculado>> {
        Ok(self
            .documentos
            .iter()
            .filter(|d| !d.desvinculado && d.alvo == *alvo)
            .cloned()
            .collect())
    }

    fn sugestao_de_conciliacao(&self, ocorrencia: &IdOcorrencia) -> Resultado<Option<Conciliacao>> {
        Ok(self
            .conciliacoes
            .iter()
            .find(|c| &c.ocorrencia == ocorrencia && !c.confirmada)
            .cloned())
    }

    fn conciliacao_desfeita(&self, ocorrencia: &IdOcorrencia, movimento: &str) -> Resultado<bool> {
        Ok(self
            .desfeitas
            .iter()
            .any(|(o, m)| o == ocorrencia && m == movimento))
    }

    fn baixas_pelo_contracheque(
        &self,
        competencia: Competencia,
    ) -> Resultado<Vec<sbf_planejamento::BaixaPeloContracheque>> {
        Ok(self.baixas_impl(competencia))
    }

    fn contas(&self) -> Resultado<Vec<ContaDoPlanejamento>> {
        Ok(self.contas_impl())
    }

    fn pares_de_conta_sugeridos(&self) -> Resultado<Vec<ParDeConta>> {
        Ok(self.pares_sugeridos_impl())
    }

    fn contas_do_extrato(&self) -> Resultado<Vec<String>> {
        Ok(self.contas_do_extrato_impl())
    }

    fn assinatura(&self, recorrente: &str) -> Resultado<Option<Assinatura>> {
        Ok(self.assinatura_impl(recorrente))
    }

    fn candidatos_de_conciliacao(
        &self,
        ocorrencia: &IdOcorrencia,
    ) -> Resultado<Vec<MovimentoCandidato>> {
        Ok(self.candidatos_impl(ocorrencia))
    }

    fn buscar_conciliacoes(&mut self, periodo: &Periodo, hoje: Data) -> Resultado<usize> {
        self.buscar_impl(periodo, hoje)
    }

    fn conciliacoes_da_ocorrencia(
        &self,
        ocorrencia: &IdOcorrencia,
    ) -> Resultado<Vec<sbf_planejamento::ConciliacaoComMovimento>> {
        Ok(self.conciliacoes_da_ocorrencia_impl(ocorrencia))
    }

    fn conciliacoes_do_movimento(&self, movimento: &str) -> Resultado<Vec<Conciliacao>> {
        Ok(self
            .conciliacoes
            .iter()
            .filter(|c| c.movimento == movimento)
            .cloned()
            .collect())
    }
}

fn alvo_da(a: &Alteracao) -> Option<String> {
    match a {
        Alteracao::Editar { recorrente, .. }
        | Alteracao::Pausar { recorrente, .. }
        | Alteracao::Retomar { recorrente, .. }
        | Alteracao::Cancelar { recorrente, .. }
        | Alteracao::Excluir { recorrente, .. }
        | Alteracao::Restaurar { recorrente, .. }
        | Alteracao::Arquivar { recorrente, .. }
        | Alteracao::Renomear { recorrente, .. }
        | Alteracao::Mover { recorrente, .. } => Some(recorrente.clone()),
        Alteracao::Excecao { ocorrencia, .. } => Some(ocorrencia.recorrente.clone()),
        Alteracao::Conciliar { conciliacao } => Some(conciliacao.ocorrencia.recorrente.clone()),
        Alteracao::Desconciliar { ocorrencia, .. } => Some(ocorrencia.recorrente.clone()),
        Alteracao::CriarRecorrente { .. }
        | Alteracao::DesfazerExcecao { .. }
        | Alteracao::CriarGrupo { .. }
        | Alteracao::DefinirComposicao { .. }
        | Alteracao::VincularDocumento { .. }
        | Alteracao::DesvincularDocumento { .. }
        | Alteracao::MapearConta { .. }
        | Alteracao::CriarModelo { .. }
        | Alteracao::NovaVersaoDoCalculo { .. } => None,
        Alteracao::DefinirAssinatura { recorrente, .. }
        | Alteracao::AplicarModelo { recorrente, .. }
        | Alteracao::DefinirParametro { recorrente, .. } => Some(recorrente.clone()),
    }
}

fn comp_da(a: &Alteracao) -> Option<Competencia> {
    match a {
        Alteracao::Editar { escopo, .. } => Some(match escopo {
            sbf_planejamento::Escopo::SoEsteMes(c) | sbf_planejamento::Escopo::DaquiEmDiante(c) => {
                *c
            }
        }),
        Alteracao::Pausar { desde, .. }
        | Alteracao::Retomar { desde, .. }
        | Alteracao::Cancelar { desde, .. } => Some(*desde),
        Alteracao::Excecao { ocorrencia, .. } => Some(ocorrencia.competencia),
        Alteracao::Conciliar { conciliacao } => Some(conciliacao.ocorrencia.competencia),
        _ => None,
    }
}

impl PlanejamentoFicticio {
    fn conferir_base(&self, id: &str, base: i64) -> Resultado<usize> {
        let i = self
            .recorrentes
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(id.into()))?;
        let atual = self.recorrentes[i].versao_do_registro;
        if atual != base {
            return Err(ErroDoPlanejamento::VersaoVelha { atual });
        }
        Ok(i)
    }

    fn registrar(&mut self, autor: Autor, alvo: &str, resumo: String, alt: &Alteracao) -> Recibo {
        let id = self.proximo_id("alt");
        let quando = self.agora();
        self.historico.push(EntradaDoHistorico {
            id: id.clone(),
            quando: quando.clone(),
            autor,
            alvo: alvo.into(),
            resumo,
            alteracao: serde_json::to_string(alt).unwrap_or_default(),
        });
        let versao_nova = self
            .recorrente_interno(alvo)
            .map_or(0, |r| r.versao_do_registro);
        Recibo {
            alteracao_id: id,
            alvo: alvo.into(),
            versao_nova,
            quando,
        }
    }

    fn aplicar_interno(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo> {
        let alt = alteracao.clone();
        let quando = self.agora();
        match alteracao {
            Alteracao::CriarRecorrente { recorrente: n, .. } => {
                let id = self.proximo_id("rec");
                let mut v = n.primeira_versao;
                v.id = "v1".into();
                self.recorrentes.push(Recorrente {
                    id: id.clone(),
                    nome: n.nome.clone(),
                    grupo: n.grupo,
                    subgrupo: n.subgrupo,
                    posicao: n.posicao,
                    natureza: n.natureza,
                    prazo: n.prazo,
                    versoes: vec![v],
                    pausas: Vec::new(),
                    cancelamento: None,
                    arquivado: false,
                    excluido: None,
                    versao_do_registro: 1,
                });
                Ok(self.registrar(autor, &id, "Criou um compromisso.".into(), &alt))
            }
            Alteracao::Editar {
                recorrente,
                escopo,
                mudanca,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                match escopo {
                    sbf_planejamento::Escopo::SoEsteMes(comp) => {
                        let so_valor = mudanca.certeza.is_none()
                            && mudanca.fonte.is_none()
                            && mudanca.frequencia.is_none()
                            && mudanca.conta.is_none()
                            && mudanca.centro.is_none()
                            && mudanca.fornecedor.is_none()
                            && mudanca.categoria.is_none()
                            && mudanca.nota.is_none();
                        let Some(Some(valor)) = mudanca.valor.filter(|_| so_valor) else {
                            return Err(ErroDoPlanejamento::Invalida(
                                "só este mês permite mudar apenas o valor".into(),
                            ));
                        };
                        let (parcela, _) =
                            Self::gera(&self.recorrentes[i], comp).ok_or_else(|| {
                                ErroDoPlanejamento::Invalida(
                                    "o compromisso não ocorre neste mês".into(),
                                )
                            })?;
                        let ocorrencia = IdOcorrencia {
                            recorrente: recorrente.clone(),
                            competencia: comp,
                            parcela,
                        };
                        self.substituir_excecao(
                            ocorrencia,
                            Excecao::ValorSoDesteMes { valor },
                            autor.clone(),
                        );
                    }
                    sbf_planejamento::Escopo::DaquiEmDiante(comp) => {
                        let mut nova = Self::versao_vigente(&self.recorrentes[i], comp)
                            .cloned()
                            .ok_or_else(|| {
                                ErroDoPlanejamento::Invalida("sem versão vigente".into())
                            })?;
                        nova.desde = comp;
                        if let Some(v) = mudanca.valor {
                            nova.valor = v;
                        }
                        if let Some(v) = mudanca.certeza {
                            nova.certeza = v;
                        }
                        if let Some(v) = mudanca.fonte {
                            nova.fonte = v;
                        }
                        if let Some(v) = mudanca.frequencia {
                            nova.frequencia = v;
                        }
                        if let Some(v) = mudanca.conta {
                            nova.conta = v;
                        }
                        if let Some(v) = mudanca.centro {
                            nova.centro = v;
                        }
                        if let Some(v) = mudanca.fornecedor {
                            nova.fornecedor = v;
                        }
                        if let Some(v) = mudanca.categoria {
                            nova.categoria = v;
                        }
                        if let Some(v) = mudanca.nota {
                            nova.nota = v;
                        }
                        let n = self.recorrentes[i].versoes.len() + 1;
                        nova.id = format!("v{n}");
                        let r = &mut self.recorrentes[i];
                        r.versoes.retain(|v| v.desde != comp);
                        r.versoes.push(nova);
                        r.versoes.sort_by_key(|v| v.desde);
                    }
                }
                self.recorrentes[i].versao_do_registro += 1;
                Ok(self.registrar(autor, &recorrente, "Editou o compromisso.".into(), &alt))
            }
            Alteracao::Pausar {
                recorrente,
                desde,
                ate,
                motivo,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let id = self.proximo_id("pausa");
                let r = &mut self.recorrentes[i];
                r.pausas.push(Pausa {
                    id,
                    desde,
                    ate,
                    motivo,
                });
                r.versao_do_registro += 1;
                Ok(self.registrar(
                    autor,
                    &recorrente,
                    format!("Pausou a partir de {desde}."),
                    &alt,
                ))
            }
            Alteracao::Retomar {
                recorrente,
                desde,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                let Some(p) = r
                    .pausas
                    .iter_mut()
                    .find(|p| p.desde <= desde && p.ate.is_none_or(|a| a > desde))
                else {
                    return Err(ErroDoPlanejamento::Invalida(
                        "não há pausa aberta nesse mês".into(),
                    ));
                };
                p.ate = Some(desde);
                // Uma pausa que acaba onde começou nunca suprimiu nada: sai da lista.
                r.pausas.retain(|p| p.ate.is_none_or(|a| a > p.desde));
                r.versao_do_registro += 1;
                Ok(self.registrar(autor, &recorrente, format!("Retomou em {desde}."), &alt))
            }
            Alteracao::Cancelar {
                recorrente,
                desde,
                motivo,
                comprovado,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                if r.cancelamento.is_some() {
                    return Err(ErroDoPlanejamento::Invalida("já está cancelado".into()));
                }
                r.cancelamento = Some(Cancelamento {
                    desde,
                    motivo,
                    comprovado,
                });
                r.versao_do_registro += 1;
                Ok(self.registrar(
                    autor,
                    &recorrente,
                    format!("Cancelou a partir de {desde}."),
                    &alt,
                ))
            }
            Alteracao::Excecao {
                ocorrencia,
                excecao,
            } => {
                let rec = ocorrencia.recorrente.clone();
                let i = self
                    .recorrentes
                    .iter()
                    .position(|r| r.id == rec)
                    .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(rec.clone()))?;
                if matches!(excecao, Excecao::QuantidadeSoDesteMes { .. }) {
                    return Err(ErroDoPlanejamento::Invalida(
                        "o conjunto fictício não tem modelos de compromisso".into(),
                    ));
                }
                self.substituir_excecao(ocorrencia.clone(), excecao.clone(), autor.clone());
                self.recorrentes[i].versao_do_registro += 1;
                let texto = match excecao {
                    Excecao::Adiar { .. } => "Adiou",
                    Excecao::Dispensar { .. } => "Dispensou",
                    Excecao::ValorSoDesteMes { .. } => "Mudou o valor só deste mês de",
                    Excecao::PagoInformado { .. } => "Informou pagamento de",
                    Excecao::QuantidadeSoDesteMes { .. } => "Ajustou a quantidade de",
                };
                Ok(self.registrar(
                    autor,
                    &rec,
                    format!("{texto} {}.", ocorrencia.competencia),
                    &alt,
                ))
            }
            Alteracao::DesfazerExcecao { id } => {
                let e = self
                    .excecoes
                    .iter_mut()
                    .find(|e| e.id == id)
                    .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(id.clone()))?;
                e.desfeita = true;
                let rec = e.ocorrencia.recorrente.clone();
                Ok(self.registrar(autor, &rec, "Desfez uma exceção.".into(), &alt))
            }
            Alteracao::Conciliar { conciliacao } => {
                let rec = conciliacao.ocorrencia.recorrente.clone();
                // Uma conciliação nova substitui a antiga; só as baixas pelo contracheque (uma por
                // rubrica) convivem.
                let e_baixa = |m: &str| m.starts_with("contracheque:");
                self.conciliacoes.retain(|c| {
                    c.ocorrencia != conciliacao.ocorrencia
                        || (c.confirmada
                            && e_baixa(&c.movimento)
                            && e_baixa(&conciliacao.movimento)
                            && c.movimento != conciliacao.movimento)
                });
                self.automaticas.retain(|(o, m)| {
                    !(o == &conciliacao.ocorrencia && m == &conciliacao.movimento)
                });
                if matches!(autor, Autor::Regra { .. }) && conciliacao.confirmada {
                    self.automaticas.push((
                        conciliacao.ocorrencia.clone(),
                        conciliacao.movimento.clone(),
                    ));
                }
                let confirmada = conciliacao.confirmada;
                self.conciliacoes.push(conciliacao);
                let texto = if confirmada {
                    "Confirmou a conciliação."
                } else {
                    "Sugeriu uma conciliação."
                };
                Ok(self.registrar(autor, &rec, texto.into(), &alt))
            }
            Alteracao::Excluir {
                recorrente,
                motivo,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                r.excluido = Some(Exclusao {
                    quando: quando.clone(),
                    autor: autor.clone(),
                    motivo,
                });
                r.versao_do_registro += 1;
                Ok(self.registrar(
                    autor,
                    &recorrente,
                    "Excluiu (de forma lógica).".into(),
                    &alt,
                ))
            }
            Alteracao::Restaurar {
                recorrente,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                if r.excluido.is_none() {
                    return Err(ErroDoPlanejamento::Invalida("não está excluído".into()));
                }
                r.excluido = None;
                r.versao_do_registro += 1;
                Ok(self.registrar(autor, &recorrente, "Restaurou.".into(), &alt))
            }
            Alteracao::Arquivar {
                recorrente,
                arquivado,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                r.arquivado = arquivado;
                r.versao_do_registro += 1;
                let t = if arquivado {
                    "Arquivou."
                } else {
                    "Tirou do arquivo."
                };
                Ok(self.registrar(autor, &recorrente, t.into(), &alt))
            }
            Alteracao::Renomear {
                recorrente,
                nome,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                r.nome = nome.trim().to_owned();
                r.versao_do_registro += 1;
                Ok(self.registrar(autor, &recorrente, "Renomeou.".into(), &alt))
            }
            Alteracao::Mover {
                recorrente,
                grupo,
                posicao,
                versao_base,
            } => {
                let i = self.conferir_base(&recorrente, versao_base)?;
                let r = &mut self.recorrentes[i];
                r.grupo = grupo;
                r.posicao = posicao;
                r.versao_do_registro += 1;
                Ok(self.registrar(autor, &recorrente, "Mudou a prioridade.".into(), &alt))
            }
            Alteracao::CriarGrupo { nome, posicao } => {
                if self.grupos.iter().any(|g| g.nome == nome) {
                    return Err(ErroDoPlanejamento::Invalida("o grupo já existe".into()));
                }
                self.grupos.push(Grupo {
                    nome: nome.clone(),
                    posicao,
                });
                Ok(self.registrar(autor, &nome, "Criou um grupo.".into(), &alt))
            }
            Alteracao::DefinirComposicao {
                composicao,
                versao_base,
            } => {
                let i = self.conferir_base(&composicao.recorrente, versao_base)?;
                let id = self.recorrentes[i].id.clone();
                self.recorrentes[i].versao_do_registro += 1;
                self.composicoes
                    .retain(|c| !(c.recorrente == id && c.desde == composicao.desde));
                self.composicoes.push(composicao);
                Ok(self.registrar(autor, &id, "Definiu as rubricas.".into(), &alt))
            }
            Alteracao::VincularDocumento {
                alvo,
                sha256,
                tipo_documento,
                nome,
                data,
            } => {
                let recorrente = match &alvo {
                    AlvoDoDocumento::Recorrente { id } => id.clone(),
                    AlvoDoDocumento::Ocorrencia { id } => id.recorrente.clone(),
                };
                if self.recorrente_interno(&recorrente).is_none() {
                    return Err(ErroDoPlanejamento::NaoEncontrado(recorrente));
                }
                if sha256.trim().is_empty() {
                    return Err(ErroDoPlanejamento::Invalida("documento sem sha256".into()));
                }
                if self
                    .documentos
                    .iter()
                    .any(|d| !d.desvinculado && d.alvo == alvo && d.sha256 == sha256)
                {
                    return Err(ErroDoPlanejamento::Invalida(
                        "este documento já está vinculado aqui".into(),
                    ));
                }
                let id = self.proximo_id("doc");
                self.documentos.push(DocumentoVinculado {
                    id,
                    alvo,
                    sha256,
                    tipo: tipo_documento,
                    nome,
                    data,
                    quando,
                    autor: autor.clone(),
                    desvinculado: false,
                });
                Ok(self.registrar(autor, &recorrente, "Vinculou um documento.".into(), &alt))
            }
            Alteracao::DesvincularDocumento { id } => {
                let d = self
                    .documentos
                    .iter_mut()
                    .find(|d| d.id == id && !d.desvinculado)
                    .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(id.clone()))?;
                d.desvinculado = true;
                let recorrente = match &d.alvo {
                    AlvoDoDocumento::Recorrente { id } => id.clone(),
                    AlvoDoDocumento::Ocorrencia { id } => id.recorrente.clone(),
                };
                Ok(self.registrar(autor, &recorrente, "Desvinculou um documento.".into(), &alt))
            }
            Alteracao::MapearConta {
                apelido,
                conta_do_extrato,
            } => self.mapear_conta(apelido, conta_do_extrato, autor, &alt),
            Alteracao::Desconciliar {
                ocorrencia,
                movimento,
            } => {
                let antes = self.conciliacoes.len();
                self.conciliacoes
                    .retain(|c| !(c.ocorrencia == ocorrencia && c.movimento == movimento));
                if self.conciliacoes.len() == antes {
                    return Err(ErroDoPlanejamento::NaoEncontrado(format!(
                        "conciliação de {ocorrencia}"
                    )));
                }
                self.automaticas
                    .retain(|(o, m)| !(o == &ocorrencia && m == &movimento));
                self.desfeitas.push((ocorrencia.clone(), movimento));
                Ok(self.registrar(
                    autor,
                    &ocorrencia.recorrente,
                    format!("Desfez a conciliação de {ocorrencia}."),
                    &alt,
                ))
            }
            Alteracao::DefinirAssinatura {
                recorrente,
                assinatura,
                versao_base,
            } => self.definir_assinatura(recorrente, assinatura, versao_base, autor, &alt),
            Alteracao::CriarModelo { .. }
            | Alteracao::NovaVersaoDoCalculo { .. }
            | Alteracao::AplicarModelo { .. }
            | Alteracao::DefinirParametro { .. } => Err(ErroDoPlanejamento::Invalida(
                "o conjunto fictício não tem modelos de compromisso".into(),
            )),
        }
    }

    fn substituir_excecao(&mut self, ocorrencia: IdOcorrencia, excecao: Excecao, autor: Autor) {
        let mesma =
            |a: &Excecao, b: &Excecao| std::mem::discriminant(a) == std::mem::discriminant(b);
        for e in self.excecoes.iter_mut() {
            if e.ocorrencia == ocorrencia && !e.desfeita && mesma(&e.excecao, &excecao) {
                e.desfeita = true;
            }
        }
        let id = self.proximo_id("exc");
        let quando = self.agora();
        self.excecoes.push(ExcecaoGravada {
            id,
            ocorrencia,
            excecao,
            quando,
            autor,
            desfeita: false,
        });
    }

    /// As exceções gravadas de uma ocorrência (aba "Exceções" do detalhe).
    pub fn excecoes_da_ocorrencia(&self, id: &IdOcorrencia) -> Vec<ExcecaoGravada> {
        self.excecoes
            .iter()
            .filter(|e| &e.ocorrencia == id)
            .cloned()
            .collect()
    }
}

/// Um `Ciclo` do padrão (corte 20) que contém `data`; atalho para os testes e as capturas.
pub fn ciclo_de(data: Data) -> Ciclo {
    Ciclo::da_data(data, DiaDeCorte::PADRAO)
}

#[allow(dead_code)]
fn _usa(c: Competencia) -> Data {
    dia_do_mes(c, 1)
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::Escopo;

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn servico() -> PlanejamentoFicticio {
        PlanejamentoFicticio::novo(hoje())
    }

    fn periodo_atual() -> Periodo {
        Periodo::Ciclo(ciclo_de(hoje()))
    }

    #[test]
    fn o_ciclo_atual_tem_linhas_e_resumo() {
        let s = servico();
        let c = s
            .controle(&periodo_atual(), Filtro::Todos, None, hoje())
            .expect("controle");
        assert!(c.linhas.len() > 10);
        assert!(c.resumo.livre_e_estimativa, "há valor sem valor e estimado");
        assert!(!c.resumo.lacunas.is_empty());
        assert!(c.resumo.descontos_em_folha > Centavos::ZERO);
        // Desconto em folha não entra nas saídas de caixa (AC-06).
        let saidas: Centavos = c
            .linhas
            .iter()
            .filter(|o| matches!(o.natureza, Natureza::Saida))
            .filter_map(|o| o.previsto)
            .sum();
        assert_eq!(c.resumo.saidas_de_caixa_previstas, saidas);
    }

    #[test]
    fn salario_tem_duas_datas_diferentes() {
        let s = servico();
        let c = s
            .controle(&periodo_atual(), Filtro::Todos, None, hoje())
            .expect("controle");
        let sal = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_salario")
            .expect("salário");
        assert_ne!(sal.vencimento, sal.vencimento_original);
        assert!(sal.vencimento < sal.vencimento_original);
    }

    #[test]
    fn atrasadas_de_antes_trazem_o_iptu() {
        let s = servico();
        let c = s
            .controle(&periodo_atual(), Filtro::Todos, None, hoje())
            .expect("controle");
        assert!(
            c.atrasadas_de_antes
                .iter()
                .any(|o| o.id.recorrente == "rec_iptu")
        );
        assert!(c.resumo.em_atraso > Centavos::ZERO);
    }

    #[test]
    fn janeiro_de_2050_nasce_da_regra_sem_herdar_pago() {
        let s = servico();
        let p = Periodo::MesCivil(Competencia { ano: 2050, mes: 1 });
        let c = s
            .controle(&p, Filtro::Todos, None, hoje())
            .expect("controle");
        assert!(!c.linhas.is_empty());
        assert!(
            c.linhas
                .iter()
                .all(|o| o.pago_informado.is_none() && o.conciliado.is_none())
        );
        assert!(
            c.linhas
                .iter()
                .all(|o| o.estado != EstadoDaOcorrencia::EmAtraso)
        );
        // O financiamento tem 48 parcelas e já acabou em 2050.
        assert!(
            !c.linhas
                .iter()
                .any(|o| o.id.recorrente == "rec_financiamento")
        );
    }

    #[test]
    fn pausa_so_aparece_no_filtro_de_pausados() {
        let s = servico();
        let h = Competencia::de_data(hoje());
        let futuro = Periodo::MesCivil(h.seguinte());
        let todos = s
            .controle(&futuro, Filtro::Todos, None, hoje())
            .expect("controle");
        assert!(
            !todos
                .linhas
                .iter()
                .any(|o| o.id.recorrente == "rec_streaming")
        );
        let pausados = s
            .controle(&futuro, Filtro::PausadosECancelados, None, hoje())
            .expect("controle");
        let o = pausados
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_streaming")
            .expect("pausada");
        assert_eq!(o.estado, EstadoDaOcorrencia::SuprimidaPorPausa);
    }

    #[test]
    fn editar_so_este_mes_e_daqui_em_diante() {
        let mut s = servico();
        let h = Competencia::de_data(hoje());
        let base = s
            .recorrente("rec_internet")
            .unwrap()
            .unwrap()
            .versao_do_registro;
        let alt = Alteracao::Editar {
            recorrente: "rec_internet".into(),
            escopo: Escopo::DaquiEmDiante(h.somar_meses(2)),
            mudanca: sbf_planejamento::MudancaDeVersao {
                valor: Some(Some(Centavos(15_000))),
                ..Default::default()
            },
            versao_base: base,
        };
        let sim = s.simular(&alt, hoje()).expect("simula");
        assert!(sim.meses_afetados.contains(&h.somar_meses(2)));
        assert!(!sim.meses_afetados.contains(&h.seguinte()));
        s.aplicar(alt, Autor::Usuario).expect("aplica");
        let antes = s
            .ocorrencia_de("rec_internet", h.seguinte(), hoje())
            .unwrap();
        let depois = s
            .ocorrencia_de("rec_internet", h.somar_meses(2), hoje())
            .unwrap();
        assert_eq!(antes.previsto, Some(Centavos(11_990)));
        assert_eq!(depois.previsto, Some(Centavos(15_000)));
        // Só este mês: não cria versão.
        let base = s
            .recorrente("rec_internet")
            .unwrap()
            .unwrap()
            .versao_do_registro;
        s.aplicar(
            Alteracao::Editar {
                recorrente: "rec_internet".into(),
                escopo: Escopo::SoEsteMes(h.somar_meses(5)),
                mudanca: sbf_planejamento::MudancaDeVersao {
                    valor: Some(Some(Centavos(1))),
                    ..Default::default()
                },
                versao_base: base,
            },
            Autor::Usuario,
        )
        .expect("aplica");
        assert_eq!(
            s.recorrente("rec_internet").unwrap().unwrap().versoes.len(),
            2
        );
        assert_eq!(
            s.ocorrencia_de("rec_internet", h.somar_meses(5), hoje())
                .unwrap()
                .previsto,
            Some(Centavos(1))
        );
        assert_eq!(
            s.ocorrencia_de("rec_internet", h.somar_meses(6), hoje())
                .unwrap()
                .previsto,
            Some(Centavos(15_000))
        );
    }

    #[test]
    fn versao_velha_nao_aplica() {
        let mut s = servico();
        let h = Competencia::de_data(hoje());
        let r = s.aplicar(
            Alteracao::Pausar {
                recorrente: "rec_internet".into(),
                desde: h,
                ate: None,
                motivo: "x".into(),
                versao_base: 99,
            },
            Autor::Usuario,
        );
        assert!(matches!(r, Err(ErroDoPlanejamento::VersaoVelha { .. })));
        assert!(
            s.recorrente("rec_internet")
                .unwrap()
                .unwrap()
                .pausas
                .is_empty()
        );
    }

    #[test]
    fn exclusao_e_logica_e_restaura() {
        let mut s = servico();
        let base = s
            .recorrente("rec_farmacia")
            .unwrap()
            .unwrap()
            .versao_do_registro;
        s.aplicar(
            Alteracao::Excluir {
                recorrente: "rec_farmacia".into(),
                motivo: "duplicado".into(),
                versao_base: base,
            },
            Autor::Usuario,
        )
        .expect("exclui");
        assert!(
            s.recorrentes(false)
                .unwrap()
                .iter()
                .all(|r| r.id != "rec_farmacia")
        );
        let r = s
            .recorrentes(true)
            .unwrap()
            .into_iter()
            .find(|r| r.id == "rec_farmacia")
            .unwrap();
        assert!(r.excluido.is_some());
        s.aplicar(
            Alteracao::Restaurar {
                recorrente: "rec_farmacia".into(),
                versao_base: r.versao_do_registro,
            },
            Autor::Usuario,
        )
        .expect("restaura");
        assert!(
            s.recorrentes(false)
                .unwrap()
                .iter()
                .any(|r| r.id == "rec_farmacia")
        );
        assert_eq!(s.historico(Some("rec_farmacia"), 10).unwrap().len(), 2);
    }

    #[test]
    fn proposta_confirma_com_usuario_e_recusa_com_motivo() {
        let mut s = servico();
        let ps = s.propostas(true).unwrap();
        assert_eq!(ps.len(), 2);
        s.confirmar_proposta(&ps[0].id).expect("confirma");
        s.recusar_proposta(&ps[1].id, "não quero").expect("recusa");
        assert!(s.propostas(true).unwrap().is_empty());
        let todas = s.propostas(false).unwrap();
        assert_eq!(todas[1].motivo.as_deref(), Some("não quero"));
        let h = s.historico(None, 5).unwrap();
        assert_eq!(h[0].autor, Autor::Usuario);
        // Idempotente pela chave.
        let a = s.propor(todas[0].alteracao.clone(), "mcp", "k").unwrap();
        let b = s.propor(todas[0].alteracao.clone(), "mcp", "k").unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn calendario_tem_360_meses_sem_faltar() {
        let s = servico();
        let h = Competencia::de_data(hoje());
        let v = s
            .calendario(h, HORIZONTE_MESES, hoje())
            .expect("calendário");
        assert_eq!(v.len(), 360);
        for (i, t) in v.iter().enumerate() {
            assert_eq!(t.competencia, h.somar_meses(i as i32));
        }
        assert!(s.calendario(h, HORIZONTE_MESES + 1, hoje()).is_err());
    }

    #[test]
    fn saldo_antigo_deixa_o_livre_como_estimativa_com_lacuna() {
        let s = servico();
        let c = s
            .controle(&periodo_atual(), Filtro::Todos, None, hoje())
            .expect("controle");
        assert!(c.resumo.lacunas.iter().any(|l| l.contains("desatualizado")));
    }

    #[test]
    fn busca_e_filtros() {
        let s = servico();
        let c = s
            .controle(&periodo_atual(), Filtro::Todos, Some("energia"), hoje())
            .unwrap();
        assert!(
            c.linhas
                .iter()
                .all(|o| o.nome.to_lowercase().contains("energia"))
        );
        let sv = s
            .controle(&periodo_atual(), Filtro::SemValor, None, hoje())
            .unwrap();
        assert!(sv.linhas.iter().all(|o| o.previsto.is_none()) && !sv.linhas.is_empty());
    }

    fn salario_de_novembro() -> IdOcorrencia {
        IdOcorrencia {
            recorrente: "rec_salario".into(),
            competencia: Competencia::de_data(hoje()),
            parcela: None,
        }
    }

    fn composicao_de_novembro(s: &mut PlanejamentoFicticio, liquido: i64, previdencia: i64) {
        let h = Competencia::de_data(hoje());
        let base = s
            .recorrente("rec_salario")
            .unwrap()
            .unwrap()
            .versao_do_registro;
        let r = |codigo: &str, nome: &str, tipo, valor: i64, rec: Option<&str>| Rubrica {
            codigo: Some(codigo.into()),
            nome: nome.into(),
            tipo,
            valor: Centavos(valor),
            competencia: None,
            prazo: None,
            recorrente: rec.map(Into::into),
        };
        s.aplicar(
            Alteracao::DefinirComposicao {
                composicao: Composicao {
                    recorrente: "rec_salario".into(),
                    desde: h,
                    rubricas: vec![
                        r(
                            "1001",
                            "Vencimento",
                            TipoDeRubrica::Provento,
                            liquido + previdencia,
                            None,
                        ),
                        r(
                            "2050",
                            "Previdência",
                            TipoDeRubrica::Desconto,
                            previdencia,
                            Some("rec_previdencia"),
                        ),
                    ],
                    documento: Some(crate::demonstracao::hash_do_nome(
                        "contracheque-2026-11.pdf",
                    )),
                    origem: "contracheque 11/2026".into(),
                },
                versao_base: base,
            },
            Autor::Usuario,
        )
        .expect("composição");
    }

    #[test]
    fn desconciliar_tira_a_conciliacao_deixa_historico_e_a_regra_nao_refaz() {
        let mut s = servico();
        let id = salario_de_novembro();
        let sugestao = s.sugestao_de_conciliacao(&id).unwrap().expect("sugestão");
        let confirmada = Conciliacao {
            confirmada: true,
            ..sugestao.clone()
        };
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: confirmada,
            },
            Autor::Regra {
                regra: "tres-provas".into(),
            },
        )
        .unwrap();
        let v = s.conciliacoes_da_ocorrencia(&id).unwrap();
        assert!(v[0].automatica && v[0].conciliacao.confirmada);
        s.aplicar(
            Alteracao::Desconciliar {
                ocorrencia: id.clone(),
                movimento: sugestao.movimento.clone(),
            },
            Autor::Usuario,
        )
        .unwrap();
        assert!(s.conciliacoes_da_ocorrencia(&id).unwrap().is_empty());
        assert!(s.conciliacao_desfeita(&id, &sugestao.movimento).unwrap());
        assert!(
            s.historico(Some("rec_salario"), 50)
                .unwrap()
                .iter()
                .any(|e| e.resumo.contains("Desfez"))
        );
        assert!(matches!(
            s.aplicar(
                Alteracao::Desconciliar {
                    ocorrencia: id,
                    movimento: sugestao.movimento,
                },
                Autor::Usuario,
            ),
            Err(ErroDoPlanejamento::NaoEncontrado(_))
        ));
    }

    #[test]
    fn buscar_concilia_sozinho_so_com_as_tres_provas() {
        let mut s = servico();
        let id = salario_de_novembro();
        // Sem composição do mês, a sugestão que já existe continua sugestão.
        s.aplicar(
            Alteracao::MapearConta {
                apelido: "CX-0001".into(),
                conta_do_extrato: Some(CONTA_DO_SALARIO.into()),
            },
            Autor::Usuario,
        )
        .unwrap();
        s.buscar_conciliacoes(&periodo_atual(), hoje()).unwrap();
        assert!(s.sugestao_de_conciliacao(&id).unwrap().is_some());
        // Com o líquido de um centavo a menos: ainda sugestão.
        composicao_de_novembro(&mut s, 822_599, 54_000);
        s.buscar_conciliacoes(&periodo_atual(), hoje()).unwrap();
        assert!(s.sugestao_de_conciliacao(&id).unwrap().is_some());
        // Líquido exato: promovida, com a regra.
        composicao_de_novembro(&mut s, 822_600, 54_000);
        assert_eq!(s.buscar_conciliacoes(&periodo_atual(), hoje()).unwrap(), 1);
        assert!(s.sugestao_de_conciliacao(&id).unwrap().is_none());
        assert!(s.conciliacoes_da_ocorrencia(&id).unwrap()[0].automatica);
    }

    #[test]
    fn a_baixa_pelo_contracheque_e_o_previsto_que_segue_a_rubrica_no_ficticio() {
        let mut s = servico();
        let h = Competencia::de_data(hoje());
        composicao_de_novembro(&mut s, 822_600, 56_000);
        // O previsto da previdência segue a rubrica (R$ 560,00), não o R$ 540,00 de antes.
        let o = s.proximas("rec_previdencia", h, 1, hoje()).unwrap();
        assert_eq!(o[0].previsto, Some(Centavos(56_000)));
        let baixas = s.baixas_pelo_contracheque(h).unwrap();
        assert_eq!(baixas.len(), 1);
        assert!(baixas[0].conciliacao.movimento.starts_with("contracheque:"));
        assert_eq!(baixas[0].diferenca, None);
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: baixas[0].conciliacao.clone(),
            },
            Autor::Usuario,
        )
        .unwrap();
        let o = s.proximas("rec_previdencia", h, 1, hoje()).unwrap();
        assert_eq!(o[0].conciliado, Some(Centavos(56_000)));
        assert!(s.baixas_pelo_contracheque(h).unwrap()[0].ja_baixada);
        // Outro mês não tem contracheque próprio.
        assert!(s.baixas_pelo_contracheque(h.seguinte()).unwrap().is_empty());
    }
}
