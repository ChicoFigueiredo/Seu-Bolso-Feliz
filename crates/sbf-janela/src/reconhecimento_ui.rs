//! O arrastar de arquivos na tela: a camada "Solte para reconhecer" e o painel
//! "Reconhecimento" do dock, com um cartão por arquivo. O estado e as regras estão em
//! [`crate::reconhecimento`]; aqui só se desenha e se juntam os cliques.

use std::path::PathBuf;
use std::time::Duration;

use egui::{
    Align2, Color32, Context, CornerRadius, FontId, Id, LayerId, Order, RichText, ScrollArea,
    Stroke, StrokeKind, Ui,
};
use sbf_planejamento::{AlvoDoDocumento, Centavos, IdOcorrencia, TipoDeRubrica};
use sbf_reconhecimento::{
    EstadoDoTexto, PROVAS_POSSIVEIS, Reconhecido, Situacao, Sugestao, UsoDeRubricas,
};

use crate::componentes;
use crate::controle::{competencia_texto, data_texto, reais};
use crate::formato;
use crate::icones;
use crate::janela::Janela;
use crate::navegacao::{self, Destino};
use crate::reconhecimento::{Cartao, SituacaoDoCartao};
use crate::tema::{self, Paleta};
use sbf_inventario::consulta::Area;

/// O que o usuário pediu num cartão (aplicado depois do desenho).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pedido {
    Confirmar(usize, usize),
    Ignorar(usize, usize),
    /// (cartão, sugestão, rubrica): troca a rubrica entre fixa e só deste mês.
    AlternarEventual(usize, usize, usize),
    Cancelar(usize),
    Limpar,
    AbrirNoControle(IdOcorrencia),
    AbrirNaTriagem,
}

impl Janela {
    /// Chamada a cada quadro do centro: lê o que foi arrastado, desenha a camada, soltando
    /// põe os arquivos na fila e acompanha a thread do reconhecimento.
    pub fn arrastar_ui(&mut self, ctx: &Context) {
        let (pairando, soltos) = ctx.input(|i| {
            let soltos: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect();
            (!i.raw.hovered_files.is_empty(), soltos)
        });
        self.reconhecimento.pairando = pairando;
        if pairando {
            camada_de_soltar(ctx);
        }
        if !soltos.is_empty() {
            self.soltar_arquivos(soltos, Some(ctx));
        }
        if self.tique_do_reconhecimento(Some(ctx)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// O conteúdo do painel "Reconhecimento".
    pub fn reconhecimento_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let mut pedidos: Vec<Pedido> = Vec::new();
        if let Some(m) = &self.mensagem
            && m.erro
            && !self.reconhecimento.ligado()
        {
            ui.add_space(4.0);
            componentes::aviso(ui, "error", &m.texto, p.perigo_sutil, p.borda, p.perigo);
        }
        let ocupado = self.reconhecimento.ocupado();
        let cartoes = &self.reconhecimento.cartoes;
        if cartoes.is_empty() {
            ui.add_space(6.0);
            let texto = if self.reconhecimento.ligado() {
                "Solte um arquivo na janela para reconhecer. O original fica onde está, e nada muda no planejamento sem o seu clique."
            } else {
                "O reconhecimento não está ligado nesta janela."
            };
            ui.label(RichText::new(texto).color(p.tinta_suave));
            return;
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} arquivo{}",
                    cartoes.len(),
                    if cartoes.len() == 1 { "" } else { "s" }
                ))
                .color(p.tinta_suave)
                .size(12.0),
            );
            if ui
                .add_enabled(!ocupado, egui::Button::new("Limpar").small())
                .on_hover_text("Tira os cartões da lista (o inventário e o planejamento não mudam)")
                .clicked()
            {
                pedidos.push(Pedido::Limpar);
            }
        });
        let mut area = ScrollArea::vertical()
            .id_salt("dock-reconhecimento")
            .auto_shrink([false, false]);
        if let Some(y) = self.reconhecimento.rolar_para.take() {
            area = area.vertical_scroll_offset(y);
        }
        area.show(ui, |ui| {
            for (i, c) in cartoes.iter().enumerate() {
                cartao(ui, &p, i, c, &mut pedidos);
                ui.add_space(6.0);
            }
        });
        self.aplicar(ui.ctx(), pedidos);
    }

    fn aplicar(&mut self, ctx: &Context, pedidos: Vec<Pedido>) {
        for pedido in pedidos {
            match pedido {
                Pedido::Confirmar(c, s) => self.confirmar_sugestao(c, s),
                Pedido::Ignorar(c, s) => self.ignorar_sugestao(c, s),
                Pedido::AlternarEventual(c, s, r) => self.alternar_eventual(c, s, r),
                Pedido::Cancelar(c) => self.cancelar_reconhecimento(c),
                Pedido::Limpar => self.limpar_reconhecimento(),
                Pedido::AbrirNoControle(id) => {
                    let _ = self.controle.navegador.ir_para(id.competencia);
                    self.controle.selecionada = Some(id);
                    navegacao::pedir_abrir(
                        ctx,
                        &mut self.pedidos,
                        Destino::AreaFutura("controle"),
                        false,
                    );
                }
                Pedido::AbrirNaTriagem => {
                    navegacao::pedir_abrir(
                        ctx,
                        &mut self.pedidos,
                        Destino::Area(Area::ATriar),
                        false,
                    );
                }
            }
        }
    }
}

/// A camada sobre a janela inteira enquanto um arquivo paira.
fn camada_de_soltar(ctx: &Context) {
    let p = tema::paleta(ctx);
    let area = ctx.content_rect();
    let pintor = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("sbf-soltar")));
    pintor.rect_filled(area, 0.0, p.destaque.gamma_multiply(0.16));
    pintor.rect_stroke(
        area.shrink(10.0),
        CornerRadius::same(10),
        Stroke::new(2.5, p.destaque),
        StrokeKind::Inside,
    );
    let centro = area.center();
    let caixa = egui::Rect::from_center_size(centro, egui::vec2(340.0, 150.0));
    pintor.rect_filled(caixa, CornerRadius::same(10), p.superficie_1);
    pintor.rect_stroke(
        caixa,
        CornerRadius::same(10),
        Stroke::new(1.0, p.borda_forte),
        StrokeKind::Inside,
    );
    pintor.text(
        centro - egui::vec2(0.0, 32.0),
        Align2::CENTER_CENTER,
        icones::texto("upload_file"),
        FontId::new(44.0, tema::familia_icones()),
        p.destaque,
    );
    pintor.text(
        centro + egui::vec2(0.0, 20.0),
        Align2::CENTER_CENTER,
        "Solte para reconhecer",
        FontId::proportional(20.0),
        p.tinta,
    );
    pintor.text(
        centro + egui::vec2(0.0, 46.0),
        Align2::CENTER_CENTER,
        "O arquivo original fica onde está",
        FontId::proportional(12.0),
        p.tinta_suave,
    );
}

fn pequeno(ui: &mut Ui, texto: impl Into<String>, cor: Color32) {
    ui.label(RichText::new(texto.into()).size(12.0).color(cor));
}

fn aviso_com_triangulo(ui: &mut Ui, p: &Paleta, texto: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(icones::icone_cor("warning", 15.0, p.aviso))
            .on_hover_text(format!("Atenção: {texto}"));
        ui.label(RichText::new(texto).size(12.5).color(p.aviso));
    });
}

fn cartao(ui: &mut Ui, p: &Paleta, indice: usize, c: &Cartao, pedidos: &mut Vec<Pedido>) {
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        cabecalho(ui, p, indice, c, pedidos);
        ui.add_space(4.0);
        etapas(ui, p, c);
        match &c.situacao {
            SituacaoDoCartao::Pronto(r) => {
                ui.add_space(6.0);
                ui.separator();
                reconhecido(ui, p, &r.reconhecido, pedidos);
                if let EstadoDoTexto::SemTexto(m) | EstadoDoTexto::Protegido(m) = &r.texto {
                    aviso_com_triangulo(ui, p, m);
                }
                for a in &r.avisos {
                    aviso_com_triangulo(ui, p, a);
                }
                if !r.sugestoes.is_empty() {
                    ui.add_space(6.0);
                    componentes::rotulo_de_secao(ui, "Sugestões");
                    for (s, sug) in r.sugestoes.iter().enumerate() {
                        sugestao(ui, p, indice, c, s, sug, pedidos);
                    }
                }
            }
            SituacaoDoCartao::Falhou(m) => {
                ui.add_space(4.0);
                componentes::aviso(ui, "error", m, p.perigo_sutil, p.borda, p.perigo);
            }
            SituacaoDoCartao::Cancelado => {
                pequeno(ui, "Cancelado.", p.tinta_suave);
            }
            SituacaoDoCartao::Aguardando | SituacaoDoCartao::Rodando => {}
        }
    });
}

fn cabecalho(ui: &mut Ui, p: &Paleta, indice: usize, c: &Cartao, pedidos: &mut Vec<Pedido>) {
    ui.horizontal(|ui| {
        match &c.situacao {
            SituacaoDoCartao::Aguardando => {
                ui.label(icones::icone_cor("schedule", 17.0, p.tinta_suave))
                    .on_hover_text("Na fila");
            }
            SituacaoDoCartao::Rodando => {
                ui.add(egui::Spinner::new().size(15.0));
            }
            SituacaoDoCartao::Pronto(_) => {
                ui.label(icones::icone_cor("check_circle", 17.0, p.sucesso));
            }
            SituacaoDoCartao::Falhou(_) => {
                ui.label(icones::icone_cor("error", 17.0, p.perigo));
            }
            SituacaoDoCartao::Cancelado => {
                ui.label(icones::icone_cor("block", 17.0, p.tinta_suave));
            }
        }
        ui.label(RichText::new(&c.nome).strong().color(p.tinta))
            .on_hover_text(c.caminho.display().to_string());
        if c.resultado().is_some_and(|r| r.ja_existia) {
            componentes::selo(ui, "já estava no inventário", p.info_sutil, p.info);
        }
        if c.em_andamento()
            && ui
                .small_button("Cancelar")
                .on_hover_text("Para antes da próxima etapa")
                .clicked()
        {
            pedidos.push(Pedido::Cancelar(indice));
        }
    });
}

fn etapas(ui: &mut Ui, p: &Paleta, c: &Cartao) {
    for (etapa, situacao) in &c.etapas {
        // Cartão terminado: etapa que nunca anunciou nada não aparece.
        if situacao.is_none() && !c.em_andamento() {
            continue;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            match situacao {
                None => {
                    ui.label(icones::icone_cor("schedule", 14.0, p.tinta_desabilitada));
                    pequeno(ui, etapa.rotulo(), p.tinta_desabilitada);
                }
                Some(Situacao::Rodando) => {
                    ui.add(egui::Spinner::new().size(13.0));
                    pequeno(ui, etapa.rotulo(), p.tinta);
                }
                Some(Situacao::Feita) => {
                    ui.label(icones::icone_cor("check_circle", 14.0, p.sucesso));
                    pequeno(ui, etapa.rotulo(), p.tinta);
                }
                Some(Situacao::Pulada(motivo)) => {
                    ui.label(icones::icone_cor("do_not_disturb_on", 14.0, p.tinta_suave))
                        .on_hover_text(motivo);
                    ui.label(
                        RichText::new(etapa.rotulo())
                            .size(12.0)
                            .color(p.tinta_suave),
                    )
                    .on_hover_text(format!("Pulada: {motivo}"));
                }
                Some(Situacao::Falhou(motivo)) => {
                    ui.label(icones::icone_cor("error", 14.0, p.perigo));
                    pequeno(ui, format!("{}: {motivo}", etapa.rotulo()), p.perigo);
                }
            }
        });
    }
}

fn par(ui: &mut Ui, p: &Paleta, rotulo: &str, valor: Option<Centavos>) {
    ui.label(RichText::new(rotulo).size(12.0).color(p.tinta_suave));
    ui.label(valor.map_or_else(|| "—".to_owned(), reais));
    ui.end_row();
}

fn reconhecido(ui: &mut Ui, p: &Paleta, r: &Reconhecido, pedidos: &mut Vec<Pedido>) {
    match r {
        Reconhecido::Contracheque {
            contracheque: c,
            consignacoes,
        } => {
            ui.horizontal(|ui| {
                ui.label(icones::icone_cor("task_alt", 16.0, p.sucesso));
                ui.label(
                    RichText::new(format!("Contracheque · {}", competencia_texto(c.pagamento)))
                        .strong(),
                );
                if !c.conferido {
                    ui.label(icones::icone_cor("warning", 15.0, p.aviso))
                        .on_hover_text(
                            "Atenção: a soma das rubricas não bate com o líquido do documento.",
                        );
                }
            });
            egui::Grid::new(("reconhecido-contracheque", c.pagamento.ano, c.pagamento.mes))
                .num_columns(2)
                .spacing([14.0, 2.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Rubricas").size(12.0).color(p.tinta_suave));
                    ui.label(formato::inteiro(c.rubricas.len() as i64));
                    ui.end_row();
                    par(ui, p, "Bruto", c.bruto.map(|v| Centavos(v.0)));
                    par(ui, p, "Descontos", c.descontos.map(|v| Centavos(v.0)));
                    par(ui, p, "Líquido", c.liquido.map(|v| Centavos(v.0)));
                    if let Some(d) = c.credito {
                        ui.label(RichText::new("Crédito").size(12.0).color(p.tinta_suave));
                        ui.label(data_texto(d));
                        ui.end_row();
                    }
                });
            if !c.conferido {
                aviso_com_triangulo(
                    ui,
                    p,
                    "As rubricas não conferem com o líquido do documento.",
                );
            }
            if !consignacoes.is_empty() {
                ui.add_space(3.0);
                pequeno(ui, "Consignações com prazo", p.tinta_suave);
                for k in consignacoes {
                    pequeno(
                        ui,
                        format!("{} · termina {}", k.nome, competencia_texto(k.termina)),
                        p.tinta,
                    );
                }
            }
        }
        Reconhecido::PelaIa { resumo, .. } => {
            ui.horizontal(|ui| {
                ui.label(icones::icone_cor("visibility", 16.0, p.info));
                ui.label(RichText::new("Sugestão da IA").strong());
            });
            ui.label(resumo);
            if ui.button("Abrir na Triagem").clicked() {
                pedidos.push(Pedido::AbrirNaTriagem);
            }
        }
        Reconhecido::Nada { motivo } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("block", 16.0, p.tinta_suave));
                ui.label(RichText::new(format!("Não reconhecido: {motivo}")).color(p.tinta_suave));
            });
        }
    }
}

fn sugestao(
    ui: &mut Ui,
    p: &Paleta,
    cartao: usize,
    c: &Cartao,
    s: usize,
    sug: &Sugestao,
    pedidos: &mut Vec<Pedido>,
) {
    let feita = c.confirmadas.contains(&s);
    let ignorada = c.ignoradas.contains(&s);
    let marcacao = c.eventuais_da(s);
    let rotulo = match (sug, &marcacao) {
        (Sugestao::UsarRubricas { uso, .. }, Some(m)) => uso.rotulo(m),
        _ => sug.rotulo().to_owned(),
    };
    ui.horizontal_wrapped(|ui| {
        if feita {
            ui.label(icones::icone_cor("check_circle", 15.0, p.sucesso));
        }
        ui.label(RichText::new(rotulo).color(if ignorada {
            p.tinta_desabilitada
        } else {
            p.tinta
        }));
    });
    match (sug, &marcacao) {
        (Sugestao::UsarRubricas { uso, .. }, Some(m)) => {
            rubricas_em_grupos(ui, p, cartao, s, uso, m, !(feita || ignorada), pedidos);
        }
        (
            Sugestao::Conciliar {
                conciliacao,
                provas,
                ..
            },
            _,
        ) => {
            componentes::selo(
                ui,
                &format!("{provas} provas batem"),
                if *provas >= PROVAS_POSSIVEIS {
                    p.sucesso_sutil
                } else {
                    p.info_sutil
                },
                if *provas >= PROVAS_POSSIVEIS {
                    p.sucesso
                } else {
                    p.info
                },
            );
            pequeno(
                ui,
                format!(
                    "Crédito de {} em {}. Confirmar liga o Salário do mês a ele.",
                    reais(conciliacao.valor),
                    data_texto(conciliacao.data)
                ),
                p.tinta_suave,
            );
            let liquido = match c.resultado().map(|r| &r.reconhecido) {
                Some(Reconhecido::Contracheque { contracheque, .. }) => contracheque.liquido,
                _ => None,
            };
            if let Some(l) = liquido.filter(|l| *l != conciliacao.valor) {
                aviso_com_triangulo(
                    ui,
                    p,
                    &format!(
                        "O crédito difere do líquido do contracheque ({}).",
                        reais(l)
                    ),
                );
            }
        }
        _ => {}
    }
    if let Some((k, erro)) = &c.erro_de_sugestao
        && *k == s
    {
        componentes::aviso(ui, "error", erro, p.perigo_sutil, p.borda, p.perigo);
    }
    ui.horizontal(|ui| {
        if feita {
            pequeno(ui, "Feito", p.sucesso);
            if let Sugestao::Vincular {
                alvo: AlvoDoDocumento::Ocorrencia { id },
                ..
            } = sug
                && ui.small_button("Abrir no Controle").clicked()
            {
                pedidos.push(Pedido::AbrirNoControle(id.clone()));
            }
        } else if ignorada {
            pequeno(ui, "Ignorada", p.tinta_desabilitada);
        } else {
            if ui.button("Confirmar").clicked() {
                pedidos.push(Pedido::Confirmar(cartao, s));
            }
            if ui.button("Ignorar").clicked() {
                pedidos.push(Pedido::Ignorar(cartao, s));
            }
        }
    });
    ui.add_space(3.0);
}

/// As rubricas do contracheque em dois grupos, com uma caixa por rubrica para trocar de
/// grupo (caixa marcada = "fixa": vale daqui para frente), mais os líquidos.
#[allow(clippy::too_many_arguments)]
fn rubricas_em_grupos(
    ui: &mut Ui,
    p: &Paleta,
    cartao: usize,
    sugestao: usize,
    uso: &UsoDeRubricas,
    marcacao: &[bool],
    editavel: bool,
    pedidos: &mut Vec<Pedido>,
) {
    let c = &uso.contracheque;
    let eventual = |i: usize| marcacao.get(i).copied().unwrap_or(false);
    let informativa = |i: usize| c.informativas.get(i).copied().unwrap_or(false);
    let fixas_desde = uso.fixas_desde(marcacao);
    let fixas = (0..c.rubricas.len())
        .filter(|i| !informativa(*i) && !eventual(*i))
        .collect::<Vec<_>>();
    let do_mes = (0..c.rubricas.len())
        .filter(|i| !informativa(*i) && eventual(*i))
        .collect::<Vec<_>>();
    let fora = (0..c.rubricas.len())
        .filter(|i| informativa(*i))
        .collect::<Vec<_>>();
    ui.add_space(3.0);
    pequeno(
        ui,
        "Caixa marcada: a rubrica se repete todo mês (fixa).",
        p.tinta_suave,
    );
    let grupo = |ui: &mut Ui, titulo: String, indices: &[usize], pedidos: &mut Vec<Pedido>| {
        ui.add_space(3.0);
        ui.label(RichText::new(titulo).strong().size(12.5).color(p.tinta));
        if indices.is_empty() {
            pequeno(ui, "nenhuma", p.tinta_desabilitada);
            return;
        }
        ui.spacing_mut().item_spacing.y = 1.0;
        for &i in indices {
            let r = &c.rubricas[i];
            let mut fixa = !eventual(i);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(editavel, egui::Checkbox::without_text(&mut fixa))
                    .on_hover_text(if fixa {
                        "Fixa: vale daqui para frente. Desmarque para valer só deste mês."
                    } else {
                        "Só deste mês. Marque para valer daqui para frente."
                    })
                    .changed()
                {
                    pedidos.push(Pedido::AlternarEventual(cartao, sugestao, i));
                }
                let valor = match r.tipo {
                    TipoDeRubrica::Provento => reais(r.valor),
                    TipoDeRubrica::Desconto => format!("− {}", reais(r.valor)),
                };
                let mut detalhe = Vec::new();
                if let Some(parcela) = c.parcelas_atuais.get(i).copied().flatten() {
                    detalhe.push(format!("parcela {parcela}"));
                }
                if let Some(k) = r.competencia
                    && k != c.pagamento
                    && c.prazo_e_parcela_atual
                {
                    detalhe.push(format!("de {}", competencia_texto(k)));
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        let largura = (ui.available_width() - 84.0).max(40.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(largura, 16.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&r.nome).size(12.0).color(p.tinta),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&r.nome);
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(valor).size(12.0).color(p.tinta));
                        });
                    });
                    if !detalhe.is_empty() {
                        ui.label(
                            RichText::new(detalhe.join(" · "))
                                .size(11.0)
                                .color(p.tinta_suave),
                        );
                    }
                });
            });
        }
    };
    grupo(
        ui,
        format!("Fixas (valem desde {})", competencia_texto(fixas_desde)),
        &fixas,
        pedidos,
    );
    grupo(ui, "Só deste mês".to_owned(), &do_mes, pedidos);
    if !fora.is_empty() {
        ui.add_space(3.0);
        ui.label(
            RichText::new("Fora do total (informativa)")
                .strong()
                .size(12.5)
                .color(p.tinta_desabilitada),
        );
        for i in fora {
            let r = &c.rubricas[i];
            ui.label(
                RichText::new(format!("{} · {}", r.nome, reais(r.valor)))
                    .size(12.0)
                    .color(p.tinta_desabilitada),
            );
        }
    }
    ui.add_space(4.0);
    let tem_eventuais = uso.tem_eventuais(marcacao);
    egui::Grid::new(("liquidos", cartao, sugestao))
        .num_columns(2)
        .spacing([14.0, 2.0])
        .show(ui, |ui| {
            ui.label(
                RichText::new("Líquido do mês")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            ui.label(reais(uso.liquido_do_mes()));
            ui.end_row();
            if tem_eventuais {
                ui.label(
                    RichText::new("Líquido das fixas")
                        .size(12.0)
                        .color(p.tinta_suave),
                );
                ui.horizontal(|ui| {
                    ui.label(reais(uso.liquido_das_fixas(marcacao)));
                    if uso.divergencia(marcacao).is_some() {
                        ui.label(icones::icone_cor("warning", 15.0, p.aviso));
                    }
                });
                ui.end_row();
            } else if uso.divergencia(marcacao).is_some() {
                ui.label("");
                ui.label(icones::icone_cor("warning", 15.0, p.aviso));
                ui.end_row();
            }
        });
    if let (Some(d), Some(digitado)) = (uso.divergencia(marcacao), uso.digitado_das_fixas(marcacao))
    {
        let sinal = if d.0 > 0 { "acima" } else { "abaixo" };
        aviso_com_triangulo(
            ui,
            p,
            &format!(
                "O líquido das {} fica {} {sinal} do valor digitado no compromisso ({}).",
                if tem_eventuais { "fixas" } else { "rubricas" },
                reais(Centavos(d.0.abs())),
                reais(digitado)
            ),
        );
    }
}
