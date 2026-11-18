//! A seção "Movimentos" do Detalhe do Controle mensal (aba Resumo): o que o extrato diz
//! desta ocorrência. A conciliação é **sugestão**; confirmar é um clique. O que decide o que
//! mostrar (`montar`) é puro e testado sem egui; `secao` só desenha.

use egui::{Frame, Margin, RichText, Stroke, Ui};
use sbf_planejamento::{
    Assinatura, Centavos, Competencia, Composicao, Conciliacao, ConciliacaoComMovimento, Data,
    DocumentoVinculado, IdOcorrencia, MovimentoCandidato, TipoDeDocumento,
};

use crate::componentes;
use crate::controle::{AcaoDoControle, data_texto, reais};
use crate::controle_ui::PedidoDoControle;
use crate::icones;
use crate::navegacao::Pedido;
use crate::tema;

/// A janela, em dias, da assinatura que a janela oferece depois de uma conciliação manual.
pub const JANELA_OFERECIDA: u8 = 5;

/// O texto do selo que diz que documento, valor e conta batem.
pub const TRES_PROVAS: &str = "3 provas batem";

/// Um movimento do extrato como a seção o mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimentoVm {
    /// O id do movimento (`pluggy:txn_…`), quando se sabe qual é.
    pub id: Option<String>,
    pub data: Option<Data>,
    /// Em módulo: o que entrou ou saiu.
    pub valor: Centavos,
    pub conta: Option<String>,
    pub descricao: Option<String>,
    /// Quanto o movimento difere do esperado (líquido do contracheque ou o previsto); só
    /// quando difere.
    pub diferenca: Option<Centavos>,
    /// Documento, valor e conta batem.
    pub tres_provas: bool,
    /// Foi uma regra que concilou sozinha (as três provas): mostra "automática" e Desfazer.
    pub automatica: bool,
    /// A baixa veio do contracheque, não do extrato: o `MM/AAAA` dele.
    pub pelo_contracheque: Option<String>,
}

/// O que a seção mostra.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VmDeMovimentos {
    /// As conciliações já confirmadas, com data, conta e descrição do movimento quando o
    /// extrato ainda o tem.
    pub confirmadas: Vec<MovimentoVm>,
    pub sugestao: Option<MovimentoVm>,
    /// Todos os débitos sugeridos quando o banco informa o favorecido (um Pix pode ser
    /// pago em vários): o Confirmar concilia todos, um por débito. Vazio sem favorecido.
    pub sugeridas: Vec<MovimentoVm>,
    /// "Pix para Fulana de Tal · 2 débitos somam o previsto", ou o aviso da soma.
    pub rotulo_da_sugestao: Option<String>,
    pub candidatos: Vec<MovimentoCandidato>,
    /// Desconto em folha: não passa pelo extrato, e a seção só mostra a baixa pelo contracheque.
    pub de_folha: bool,
}

/// O que `montar` precisa saber.
#[derive(Debug, Clone, Copy)]
pub struct EntradaDosMovimentos<'a> {
    pub conciliado: Option<Centavos>,
    /// As conciliações gravadas da ocorrência, com o movimento do extrato.
    pub gravadas: &'a [ConciliacaoComMovimento],
    pub sugestao: Option<&'a Conciliacao>,
    pub candidatos: &'a [MovimentoCandidato],
    pub composicao: Option<&'a Composicao>,
    /// Os documentos do tipo contracheque ligados à ocorrência ou ao compromisso.
    pub contracheques: &'a [&'a DocumentoVinculado],
    pub previsto: Option<Centavos>,
}

/// O valor que o movimento deveria ter: o líquido da composição do mês, senão o previsto.
fn esperado(e: &EntradaDosMovimentos<'_>) -> Option<Centavos> {
    e.composicao.map(Composicao::liquido).or(e.previsto)
}

/// Os três pontos de apoio: o contracheque vinculado (de onde vieram as rubricas), o líquido
/// igual ao valor do movimento e a conta do movimento igual à do par confirmado.
fn tres_provas(e: &EntradaDosMovimentos<'_>, valor: Centavos, mesma_conta: bool) -> bool {
    let Some(c) = e.composicao else {
        return false;
    };
    let do_contracheque = c
        .documento
        .as_ref()
        .is_some_and(|d| e.contracheques.iter().any(|x| &x.sha256 == d));
    mesma_conta && do_contracheque && c.liquido() == valor
}

fn so_se_difere(d: Option<Centavos>) -> Option<Centavos> {
    d.filter(|d| d.0 != 0)
}

fn de_candidato(e: &EntradaDosMovimentos<'_>, c: &MovimentoCandidato) -> MovimentoVm {
    let valor = Centavos(c.movimento.valor.0.abs());
    MovimentoVm {
        id: Some(c.movimento.movimento.clone()),
        data: Some(c.movimento.data),
        valor,
        conta: Some(c.movimento.conta_do_extrato.clone()),
        descricao: Some(c.movimento.descricao.clone()),
        diferenca: so_se_difere(c.diferenca),
        tres_provas: tres_provas(e, valor, c.mesma_conta),
        automatica: false,
        pelo_contracheque: None,
    }
}

/// Monta a seção. Puro.
pub fn montar(e: &EntradaDosMovimentos<'_>) -> VmDeMovimentos {
    let mut vm = VmDeMovimentos {
        candidatos: e.candidatos.to_vec(),
        ..Default::default()
    };
    // As baixas pelo contracheque (spec 12, D3) somam contra o previsto do compromisso: a
    // diferença é uma só, na primeira linha.
    let pelo_contracheque =
        |g: &ConciliacaoComMovimento| g.conciliacao.movimento.starts_with("contracheque:");
    let total_das_baixas = Centavos(
        e.gravadas
            .iter()
            .filter(|g| g.conciliacao.confirmada && pelo_contracheque(g))
            .map(|g| g.conciliacao.valor.0)
            .sum(),
    );
    let mut diferenca_das_baixas_dada = false;
    for g in e.gravadas.iter().filter(|g| g.conciliacao.confirmada) {
        let valor = g.conciliacao.valor;
        let baixa = pelo_contracheque(g);
        let diferenca = if baixa {
            let d = so_se_difere(
                e.previsto
                    .map(|p| Centavos((p.0 - total_das_baixas.0).abs()))
                    .filter(|_| !diferenca_das_baixas_dada),
            );
            diferenca_das_baixas_dada = true;
            d
        } else {
            so_se_difere(esperado(e).map(|x| Centavos((x.0 - valor.0).abs())))
        };
        let competencia = g.conciliacao.ocorrencia.competencia;
        vm.confirmadas.push(MovimentoVm {
            id: Some(g.conciliacao.movimento.clone()),
            data: Some(g.conciliacao.data),
            valor,
            conta: g.movimento.as_ref().map(|m| m.conta_do_extrato.clone()),
            descricao: g.movimento.as_ref().map(|m| m.descricao.clone()),
            diferenca,
            tres_provas: tres_provas(e, valor, g.mesma_conta),
            automatica: g.automatica,
            pelo_contracheque: baixa
                .then(|| format!("{:02}/{:04}", competencia.mes, competencia.ano)),
        });
    }
    if let Some(v) = e.conciliado.filter(|_| vm.confirmadas.is_empty()) {
        let achado = e
            .candidatos
            .iter()
            .find(|c| c.movimento.valor.0.abs() == v.0);
        vm.confirmadas.push(match achado {
            Some(c) => de_candidato(e, c),
            None => MovimentoVm {
                id: None,
                data: None,
                valor: v,
                conta: None,
                descricao: None,
                diferenca: so_se_difere(esperado(e).map(|x| Centavos((x.0 - v.0).abs()))),
                tres_provas: false,
                automatica: false,
                pelo_contracheque: None,
            },
        });
    }
    if let Some(s) = e.sugestao {
        let achado = e
            .candidatos
            .iter()
            .find(|c| c.movimento.movimento == s.movimento);
        vm.sugestao = Some(match achado {
            Some(c) => de_candidato(e, c),
            None => MovimentoVm {
                id: Some(s.movimento.clone()),
                data: Some(s.data),
                valor: s.valor,
                conta: None,
                descricao: None,
                diferenca: so_se_difere(esperado(e).map(|x| Centavos((x.0 - s.valor.0).abs()))),
                tres_provas: false,
                automatica: false,
                pelo_contracheque: None,
            },
        });
    }
    agrupar_sugestoes(e, &mut vm);
    vm
}

/// Quando as sugestões pendentes trazem o favorecido do banco (`contraparte`), mostra todas
/// juntas com o rótulo do que a soma faz. O Confirmar da seção concilia todas.
fn agrupar_sugestoes(e: &EntradaDosMovimentos<'_>, vm: &mut VmDeMovimentos) {
    let pendentes: Vec<&ConciliacaoComMovimento> = e
        .gravadas
        .iter()
        .filter(|g| {
            !g.conciliacao.confirmada && !g.conciliacao.movimento.starts_with("contracheque:")
        })
        .collect();
    let favorecido = |g: &ConciliacaoComMovimento| {
        g.movimento
            .as_ref()
            .and_then(|m| m.contraparte.as_deref())
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
    };
    if pendentes.is_empty() || pendentes.iter().any(|g| favorecido(g).is_none()) {
        return;
    }
    let soma: i64 = pendentes.iter().map(|g| g.conciliacao.valor.0).sum();
    vm.rotulo_da_sugestao = Some(sbf_reconhecimento::favorecido::rotulo_da_sugestao(
        pendentes.first().and_then(|g| favorecido(g)).as_deref(),
        pendentes.len(),
        soma,
        e.previsto.map(|p| p.0.abs()),
    ));
    vm.sugeridas = pendentes
        .iter()
        .map(|g| MovimentoVm {
            id: Some(g.conciliacao.movimento.clone()),
            data: Some(g.conciliacao.data),
            valor: g.conciliacao.valor,
            conta: g.movimento.as_ref().map(|m| m.conta_do_extrato.clone()),
            descricao: g.movimento.as_ref().map(|m| m.descricao.clone()),
            diferenca: None,
            tres_provas: false,
            automatica: false,
            pelo_contracheque: None,
        })
        .collect();
}

/// O contracheque mais recente (pela data do documento) entre os vinculados.
pub fn contracheque_vinculado<'a>(
    docs: &[&'a DocumentoVinculado],
) -> Option<&'a DocumentoVinculado> {
    docs.iter()
        .copied()
        .filter(|d| d.tipo == TipoDeDocumento::Contracheque && !d.desvinculado)
        .max_by_key(|d| d.data)
}

/// A faixa da aba Rubricas: há contracheque vinculado e a composição vigente não veio dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaixaDoContracheque {
    pub sha256: String,
    /// `MM/AAAA`.
    pub mes_ano: String,
}

pub fn faixa_do_contracheque(
    composicao: Option<&Composicao>,
    docs: &[&DocumentoVinculado],
    competencia: Competencia,
) -> Option<FaixaDoContracheque> {
    let doc = contracheque_vinculado(docs)?;
    if composicao.is_some_and(|c| c.documento.as_deref() == Some(doc.sha256.as_str())) {
        return None;
    }
    let (mes, ano) = doc
        .data
        .map_or((competencia.mes, competencia.ano), |d| (d.mes, d.ano));
    Some(FaixaDoContracheque {
        sha256: doc.sha256.clone(),
        mes_ano: format!("{mes:02}/{ano:04}"),
    })
}

/// A oferta que aparece depois de uma conciliação manual de um recorrente sem assinatura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfertaDeAssinatura {
    pub recorrente: String,
    pub texto: String,
}

impl OfertaDeAssinatura {
    pub fn assinatura(&self) -> Assinatura {
        Assinatura {
            texto: self.texto.clone(),
            janela_dias: JANELA_OFERECIDA,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------------------

fn pequeno(ui: &mut Ui, t: impl Into<String>) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new(t.into()).size(11.5).color(p.tinta_suave));
}

/// "Ignorar": a sugestão some e a regra (se foi ela) não a repete.
fn botao_de_ignorar(ui: &mut Ui, ocorrencia: &IdOcorrencia, pedidos: &mut Vec<Pedido>) {
    if ui
        .button("Ignorar")
        .on_hover_text(
            "Descarta a sugestão; uma regra de reconhecimento registra que foi rejeitada",
        )
        .clicked()
    {
        pedidos.push(Pedido::Controle(PedidoDoControle::IgnorarSugestao {
            ocorrencia: ocorrencia.clone(),
        }));
    }
}

fn linha_do_movimento(ui: &mut Ui, m: &MovimentoVm) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if let Some(d) = m.data {
            ui.label(RichText::new(data_texto(d)).strong());
        }
        ui.label(RichText::new(reais(m.valor)).strong());
        if m.tres_provas {
            componentes::selo(
                ui,
                &format!("{}  {TRES_PROVAS}", icones::texto("verified")),
                p.sucesso_sutil,
                p.sucesso,
            )
            .on_hover_text("O contracheque vinculado, o valor e a conta do extrato conferem");
        }
    });
    if let Some(mes) = &m.pelo_contracheque {
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("description", 15.0, p.tinta_suave));
            ui.label(
                RichText::new(format!("Pelo contracheque {mes}"))
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        });
    }
    if let Some(c) = &m.conta {
        pequeno(ui, c.clone());
    }
    if let Some(d) = &m.descricao {
        pequeno(ui, d.clone());
    }
    if let Some(d) = m.diferenca {
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("warning", 15.0, p.aviso));
            let texto = if m.pelo_contracheque.is_some() {
                format!("O contracheque difere do previsto em {}", reais(d))
            } else {
                format!("Difere do esperado em {}", reais(d))
            };
            ui.label(RichText::new(texto).size(12.0).color(p.aviso));
        });
    }
}

fn moldura(ui: &mut Ui, fundo: egui::Color32, conteudo: impl FnOnce(&mut Ui)) {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            conteudo(ui);
        });
    ui.add_space(4.0);
}

/// A seção de um desconto em folha: só a baixa pelo contracheque (spec 12, D3).
fn secao_de_folha(ui: &mut Ui, vm: &VmDeMovimentos) {
    let p = tema::paleta(ui.ctx());
    ui.label(
        RichText::new("Baixa pelo contracheque")
            .strong()
            .size(13.0)
            .color(p.tinta),
    );
    ui.add_space(2.0);
    for m in &vm.confirmadas {
        moldura(ui, p.superficie_2, |ui| {
            ui.horizontal_wrapped(|ui| {
                componentes::selo(ui, "Conciliado", p.sucesso_sutil, p.sucesso);
            });
            linha_do_movimento(ui, m);
        });
    }
    if vm.confirmadas.is_empty() {
        pequeno(
            ui,
            "O contracheque do mês dá baixa aqui quando a rubrica está ligada a este compromisso.",
        );
        ui.add_space(4.0);
    }
}

/// Desenha a seção. `aberto` é a lista de "Conciliar com outro…". Devolve `true` quando o
/// usuário dispensou a oferta de assinatura.
pub fn secao(
    ui: &mut Ui,
    vm: &VmDeMovimentos,
    ocorrencia: &IdOcorrencia,
    aberto: &mut bool,
    oferta: Option<&OfertaDeAssinatura>,
    pedidos: &mut Vec<Pedido>,
) -> bool {
    let p = tema::paleta(ui.ctx());
    if vm.de_folha {
        secao_de_folha(ui, vm);
        return false;
    }
    let mut dispensou = false;
    ui.label(
        RichText::new("Movimentos")
            .strong()
            .size(13.0)
            .color(p.tinta),
    );
    ui.add_space(2.0);
    for m in &vm.confirmadas {
        moldura(ui, p.superficie_2, |ui| {
            ui.horizontal_wrapped(|ui| {
                componentes::selo(ui, "Conciliado", p.sucesso_sutil, p.sucesso);
                if m.automatica {
                    componentes::selo(ui, "automática", p.info_sutil, p.info);
                }
            });
            linha_do_movimento(ui, m);
            if m.automatica {
                ui.horizontal_wrapped(|ui| {
                    pequeno(ui, "Conciliado automaticamente · as três provas batem.");
                    if let Some(movimento) = &m.id
                        && ui
                            .small_button("Desfazer")
                            .on_hover_text("Desfaz a conciliação; ela volta a ser uma sugestão")
                            .clicked()
                    {
                        pedidos.push(Pedido::Controle(PedidoDoControle::Desconciliar {
                            ocorrencia: ocorrencia.clone(),
                            movimento: movimento.clone(),
                        }));
                    }
                });
            }
        });
    }
    if !vm.sugeridas.is_empty() {
        moldura(ui, p.info_sutil, |ui| {
            ui.horizontal_wrapped(|ui| {
                componentes::selo(ui, "Sugestão", p.superficie_3, p.info);
                if let Some(r) = &vm.rotulo_da_sugestao {
                    ui.label(RichText::new(r).strong());
                }
            });
            ui.horizontal_wrapped(|ui| {
                let n = vm.sugeridas.len();
                if ui
                    .button(format!(
                        "{}  {}",
                        icones::texto("task_alt"),
                        if n == 1 {
                            "Confirmar".to_owned()
                        } else {
                            format!("Confirmar os {n}")
                        }
                    ))
                    .on_hover_text("Confirma uma conciliação por débito, todas nesta ocorrência")
                    .clicked()
                {
                    pedidos.push(Pedido::Controle(PedidoDoControle::Acao(
                        AcaoDoControle::ConfirmarConciliacao,
                    )));
                }
                botao_de_ignorar(ui, ocorrencia, pedidos);
            });
            for m in &vm.sugeridas {
                linha_do_movimento(ui, m);
            }
        });
    } else if let Some(s) = &vm.sugestao {
        moldura(ui, p.info_sutil, |ui| {
            ui.horizontal_wrapped(|ui| {
                componentes::selo(ui, "Sugestão", p.superficie_3, p.info);
                if ui
                    .button(format!("{}  Confirmar", icones::texto("task_alt")))
                    .on_hover_text("Confirma o casamento sugerido com o movimento do banco")
                    .clicked()
                {
                    pedidos.push(Pedido::Controle(PedidoDoControle::Acao(
                        AcaoDoControle::ConfirmarConciliacao,
                    )));
                }
                botao_de_ignorar(ui, ocorrencia, pedidos);
            });
            linha_do_movimento(ui, s);
        });
    }
    if vm.confirmadas.is_empty() && vm.sugestao.is_none() && vm.sugeridas.is_empty() {
        pequeno(ui, "Nenhum movimento do extrato ligado a esta ocorrência.");
        ui.add_space(4.0);
    }
    if let Some(o) = oferta {
        moldura(ui, p.aviso_sutil, |ui| {
            ui.label(format!(
                "Reconhecer este compromisso no extrato por \u{201c}{}\u{201d}?",
                o.texto
            ));
            pequeno(
                ui,
                format!(
                    "Da próxima vez, o que tiver esse texto (até {JANELA_OFERECIDA} dias do vencimento) vira candidato, com qualquer valor."
                ),
            );
            ui.horizontal(|ui| {
                if ui.button("Reconhecer").clicked() {
                    pedidos.push(Pedido::Controle(PedidoDoControle::DefinirAssinatura {
                        recorrente: o.recorrente.clone(),
                        assinatura: Some(o.assinatura()),
                    }));
                }
                if ui.button("Agora não").clicked() {
                    dispensou = true;
                }
            });
        });
    }
    if ui
        .selectable_label(*aberto, "Conciliar com outro…")
        .on_hover_text("Escolhe, entre os movimentos do extrato, o que paga esta ocorrência")
        .clicked()
    {
        *aberto = !*aberto;
    }
    if *aberto {
        ui.add_space(4.0);
        if vm.candidatos.is_empty() {
            pequeno(ui, "Nenhum movimento do extrato nesta janela.");
        }
        for c in &vm.candidatos {
            moldura(ui, p.superficie_2, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(data_texto(c.movimento.data)).strong());
                    ui.label(RichText::new(reais(Centavos(c.movimento.valor.0.abs()))).strong());
                    if c.bate_assinatura {
                        componentes::selo(ui, "assinatura", p.sucesso_sutil, p.sucesso);
                    }
                    if c.mesma_conta {
                        componentes::selo(ui, "conta", p.sucesso_sutil, p.sucesso);
                    }
                });
                pequeno(ui, c.movimento.conta_do_extrato.clone());
                pequeno(ui, c.movimento.descricao.clone());
                if let Some(d) = c.diferenca.filter(|d| d.0 != 0) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(icones::icone_cor("warning", 15.0, p.aviso));
                        ui.label(
                            RichText::new(format!("Difere do esperado em {}", reais(d)))
                                .size(12.0)
                                .color(p.aviso),
                        );
                    });
                }
                if ui.button("Conciliar com este").clicked() {
                    pedidos.push(Pedido::Controle(PedidoDoControle::ConciliarManual {
                        conciliacao: Conciliacao {
                            ocorrencia: ocorrencia.clone(),
                            movimento: c.movimento.movimento.clone(),
                            valor: Centavos(c.movimento.valor.0.abs()),
                            data: c.movimento.data,
                            confirmada: true,
                        },
                        descricao: c.movimento.descricao.clone(),
                    }));
                    *aberto = false;
                }
            });
        }
    }
    dispensou
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_planejamento::{AlvoDoDocumento, Autor, MovimentoDoExtrato, Rubrica, TipoDeRubrica};

    fn candidato(id: &str, valor: i64, mesma_conta: bool, dif: i64) -> MovimentoCandidato {
        MovimentoCandidato {
            movimento: MovimentoDoExtrato {
                movimento: id.into(),
                data: Data::nova(2026, 11, 4).unwrap(),
                valor: Centavos(valor),
                conta_do_extrato: "BANCO EXEMPLO · 000000-1".into(),
                descricao: "SALARIO EMPRESA EXEMPLO".into(),
                tipo: "credito".into(),
                contraparte: None,
                contraparte_doc: None,
            },
            bate_assinatura: true,
            mesma_conta,
            diferenca: Some(Centavos(dif)),
        }
    }

    fn composicao(liquido: i64, documento: Option<&str>) -> Composicao {
        Composicao {
            recorrente: "rec_salario".into(),
            desde: Competencia::nova(2026, 11).unwrap(),
            rubricas: vec![Rubrica {
                codigo: None,
                nome: "Vencimento".into(),
                tipo: TipoDeRubrica::Provento,
                valor: Centavos(liquido),
                competencia: None,
                prazo: None,
                recorrente: None,
            }],
            documento: documento.map(Into::into),
            origem: "teste".into(),
        }
    }

    fn contracheque(sha: &str, data: &str) -> DocumentoVinculado {
        DocumentoVinculado {
            id: format!("doc_{sha}"),
            alvo: AlvoDoDocumento::Recorrente {
                id: "rec_salario".into(),
            },
            sha256: sha.into(),
            tipo: TipoDeDocumento::Contracheque,
            nome: "contracheque.pdf".into(),
            data: Data::de_iso(data),
            quando: "2026-11-16T12:00:00Z".into(),
            autor: Autor::Usuario,
            desvinculado: false,
        }
    }

    fn sugestao(id: &str, valor: i64) -> Conciliacao {
        Conciliacao {
            ocorrencia: IdOcorrencia {
                recorrente: "rec_salario".into(),
                competencia: Competencia::nova(2026, 11).unwrap(),
                parcela: None,
            },
            movimento: id.into(),
            valor: Centavos(valor),
            data: Data::nova(2026, 11, 4).unwrap(),
            confirmada: false,
        }
    }

    #[test]
    fn confirmada_mostra_o_movimento_gravado_mesmo_sem_candidato() {
        let c = composicao(822_600, Some("sha-a"));
        let doc = contracheque("sha-a", "2026-11-05");
        let docs = [&doc];
        let mut conc = sugestao("m9", 822_600);
        conc.confirmada = true;
        let m = candidato("m9", 822_600, true, 0).movimento;
        let gravadas = [ConciliacaoComMovimento {
            conciliacao: conc,
            movimento: Some(m.clone()),
            mesma_conta: true,
            automatica: false,
        }];
        let vm = montar(&EntradaDosMovimentos {
            conciliado: Some(Centavos(822_600)),
            gravadas: &gravadas,
            sugestao: None,
            candidatos: &[],
            composicao: Some(&c),
            contracheques: &docs,
            previsto: Some(Centavos(845_000)),
        });
        assert_eq!(vm.confirmadas.len(), 1);
        let x = &vm.confirmadas[0];
        assert_eq!(x.descricao.as_deref(), Some(m.descricao.as_str()));
        assert_eq!(x.conta.as_deref(), Some(m.conta_do_extrato.as_str()));
        assert!(x.tres_provas);
    }

    #[test]
    fn tres_provas_pedem_contracheque_valor_e_conta() {
        let c = composicao(822_600, Some("sha-a"));
        let doc = contracheque("sha-a", "2026-11-05");
        let docs = [&doc];
        let s = sugestao("m1", 822_600);
        let cands = [candidato("m1", 822_600, true, 0)];
        let base = EntradaDosMovimentos {
            conciliado: None,
            gravadas: &[],
            sugestao: Some(&s),
            candidatos: &cands,
            composicao: Some(&c),
            contracheques: &docs,
            previsto: Some(Centavos(845_000)),
        };
        let vm = montar(&base);
        let sug = vm.sugestao.expect("sugestão");
        assert!(sug.tres_provas);
        assert_eq!(sug.diferenca, None);
        // Conta de outro par: não bate.
        let outra = [candidato("m1", 822_600, false, 0)];
        let vm = montar(&EntradaDosMovimentos {
            candidatos: &outra,
            ..base
        });
        assert!(!vm.sugestao.unwrap().tres_provas);
        // Valor diferente do líquido: não bate, e a diferença aparece.
        let s2 = sugestao("m1", 800_000);
        let longe = [candidato("m1", 800_000, true, 22_600)];
        let vm = montar(&EntradaDosMovimentos {
            sugestao: Some(&s2),
            candidatos: &longe,
            ..base
        });
        let sug = vm.sugestao.unwrap();
        assert!(!sug.tres_provas);
        assert_eq!(sug.diferenca, Some(Centavos(22_600)));
        // Sem contracheque vinculado ou com rubricas de outra origem: não bate.
        let vm = montar(&EntradaDosMovimentos {
            contracheques: &[],
            ..base
        });
        assert!(!vm.sugestao.unwrap().tres_provas);
        let de_fora = composicao(822_600, Some("sha-b"));
        let vm = montar(&EntradaDosMovimentos {
            composicao: Some(&de_fora),
            ..base
        });
        assert!(!vm.sugestao.unwrap().tres_provas);
    }

    #[test]
    fn confirmada_pega_os_detalhes_do_candidato_de_mesmo_valor() {
        let cands = [candidato("m1", 822_600, true, 0)];
        let vm = montar(&EntradaDosMovimentos {
            gravadas: &[],
            conciliado: Some(Centavos(822_600)),
            sugestao: None,
            candidatos: &cands,
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(822_600)),
        });
        let m = &vm.confirmadas[0];
        assert_eq!(m.descricao.as_deref(), Some("SALARIO EMPRESA EXEMPLO"));
        assert!(m.data.is_some());
        // Sem candidato correspondente, mostra só o valor, sem inventar o resto.
        let vm = montar(&EntradaDosMovimentos {
            gravadas: &[],
            conciliado: Some(Centavos(1_000)),
            sugestao: None,
            candidatos: &cands,
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(1_500)),
        });
        let m = &vm.confirmadas[0];
        assert_eq!((m.data, m.descricao.clone()), (None, None));
        assert_eq!(m.diferenca, Some(Centavos(500)));
    }

    #[test]
    fn faixa_so_quando_a_composicao_nao_veio_do_contracheque_vinculado() {
        let doc = contracheque("sha-a", "2026-11-05");
        let comp = Competencia::nova(2026, 11).unwrap();
        let docs = [&doc];
        let do_doc = composicao(1, Some("sha-a"));
        assert_eq!(faixa_do_contracheque(Some(&do_doc), &docs, comp), None);
        let da_planilha = composicao(1, None);
        let f = faixa_do_contracheque(Some(&da_planilha), &docs, comp).expect("faixa");
        assert_eq!(
            (f.sha256.as_str(), f.mes_ano.as_str()),
            ("sha-a", "11/2026")
        );
        // Sem composição também vale: o contracheque tem rubricas a revisar.
        assert!(faixa_do_contracheque(None, &docs, comp).is_some());
        // Sem contracheque, ou só com fatura, nada.
        assert_eq!(faixa_do_contracheque(Some(&da_planilha), &[], comp), None);
        let mut fatura = contracheque("sha-f", "2026-11-05");
        fatura.tipo = TipoDeDocumento::Fatura;
        assert_eq!(
            faixa_do_contracheque(Some(&da_planilha), &[&fatura], comp),
            None
        );
    }

    #[test]
    fn o_contracheque_mais_recente_vence() {
        let velho = contracheque("sha-1", "2026-10-05");
        let novo = contracheque("sha-2", "2026-11-05");
        let docs = [&velho, &novo];
        assert_eq!(contracheque_vinculado(&docs).unwrap().sha256, "sha-2");
    }

    fn gravada(
        movimento: &str,
        valor: i64,
        confirmada: bool,
        automatica: bool,
    ) -> ConciliacaoComMovimento {
        let mut c = sugestao(movimento, valor);
        c.confirmada = confirmada;
        ConciliacaoComMovimento {
            conciliacao: c,
            movimento: None,
            mesma_conta: false,
            automatica,
        }
    }

    #[test]
    fn a_conciliacao_da_regra_vem_marcada_como_automatica() {
        let gravadas = [gravada("m1", 822_600, true, true)];
        let vm = montar(&EntradaDosMovimentos {
            conciliado: Some(Centavos(822_600)),
            gravadas: &gravadas,
            sugestao: None,
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(822_600)),
        });
        assert!(vm.confirmadas[0].automatica);
        assert_eq!(vm.confirmadas[0].pelo_contracheque, None);
        let manual = [gravada("m1", 822_600, true, false)];
        let vm = montar(&EntradaDosMovimentos {
            gravadas: &manual,
            conciliado: Some(Centavos(822_600)),
            sugestao: None,
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(822_600)),
        });
        assert!(!vm.confirmadas[0].automatica);
    }

    #[test]
    fn a_baixa_pelo_contracheque_mostra_o_mes_e_a_diferenca_so_uma_vez() {
        let gravadas = [
            gravada("contracheque:abc:3", 98_199, true, false),
            gravada("contracheque:abc:5", 31_800, true, false),
        ];
        let vm = montar(&EntradaDosMovimentos {
            conciliado: Some(Centavos(129_999)),
            gravadas: &gravadas,
            sugestao: None,
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(120_000)),
        });
        assert_eq!(vm.confirmadas.len(), 2);
        assert_eq!(
            vm.confirmadas[0].pelo_contracheque.as_deref(),
            Some("11/2026")
        );
        // A soma das rubricas (1.299,99) difere do previsto (1.200,00) em R$ 99,99, na primeira.
        assert_eq!(vm.confirmadas[0].diferenca, Some(Centavos(9_999)));
        assert_eq!(vm.confirmadas[1].diferenca, None);
        // Sem diferença, nada de aviso.
        let vm = montar(&EntradaDosMovimentos {
            conciliado: Some(Centavos(129_999)),
            gravadas: &gravadas,
            sugestao: None,
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(129_999)),
        });
        assert_eq!(vm.confirmadas[0].diferenca, None);
    }

    fn pix_sugerido(
        movimento: &str,
        valor: i64,
        favorecido: Option<&str>,
    ) -> ConciliacaoComMovimento {
        let mut g = gravada(movimento, valor, false, false);
        g.movimento = Some(MovimentoDoExtrato {
            movimento: movimento.into(),
            data: Data::nova(2026, 11, 5).unwrap(),
            valor: Centavos(-valor),
            conta_do_extrato: "BANCO EXEMPLO · 000000-1".into(),
            descricao: "PIX ENVIADO".into(),
            tipo: "debito".into(),
            contraparte: favorecido.map(str::to_owned),
            contraparte_doc: Some("000.000.000-00".into()),
        });
        g
    }

    #[test]
    fn varios_debitos_sugeridos_do_mesmo_favorecido_viram_um_grupo_com_rotulo() {
        let gravadas = [
            pix_sugerido("pluggy:txn_1", 15_000, Some("Fulana de Tal")),
            pix_sugerido("pluggy:txn_2", 15_000, Some("Fulana de Tal")),
        ];
        let vm = montar(&EntradaDosMovimentos {
            conciliado: None,
            gravadas: &gravadas,
            sugestao: Some(&gravadas[0].conciliacao),
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(30_000)),
        });
        assert_eq!(vm.sugeridas.len(), 2);
        assert_eq!(
            vm.rotulo_da_sugestao.as_deref(),
            Some("Pix para Fulana de Tal \u{b7} 2 d\u{e9}bitos somam o previsto")
        );
        assert_eq!(vm.sugeridas[0].descricao.as_deref(), Some("PIX ENVIADO"));
        // Soma que não bate: o aviso entra no rótulo.
        let vm = montar(&EntradaDosMovimentos {
            conciliado: None,
            gravadas: &gravadas[..1],
            sugestao: Some(&gravadas[0].conciliacao),
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(30_000)),
        });
        assert!(
            vm.rotulo_da_sugestao
                .as_deref()
                .is_some_and(|r| r.contains("\u{26a0} soma R$ 150,00 difere do previsto R$ 300,00"))
        );
    }

    #[test]
    fn sem_favorecido_do_banco_a_sugestao_segue_a_de_sempre() {
        let gravadas = [pix_sugerido("pluggy:txn_1", 15_000, None)];
        let vm = montar(&EntradaDosMovimentos {
            conciliado: None,
            gravadas: &gravadas,
            sugestao: Some(&gravadas[0].conciliacao),
            candidatos: &[],
            composicao: None,
            contracheques: &[],
            previsto: Some(Centavos(15_000)),
        });
        assert!(vm.sugeridas.is_empty() && vm.rotulo_da_sugestao.is_none());
        assert!(vm.sugestao.is_some());
    }
}
