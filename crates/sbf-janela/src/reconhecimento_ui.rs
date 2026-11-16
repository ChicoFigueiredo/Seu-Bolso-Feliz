//! O arrastar de arquivos na tela: a camada "Solte para reconhecer" e o painel
//! "Reconhecimento" do dock, com um cartão por arquivo. O estado e as regras estão em
//! [`crate::reconhecimento`]; aqui só se desenha e se juntam os cliques.

use std::path::PathBuf;
use std::time::Duration;

use egui::{
    Align2, Color32, Context, CornerRadius, FontId, Id, LayerId, Order, RichText, ScrollArea,
    Stroke, StrokeKind, Ui,
};
use sbf_planejamento::{AlvoDoDocumento, Centavos, IdOcorrencia};
use sbf_reconhecimento::{EstadoDoTexto, Reconhecido, Situacao, Sugestao};

use crate::componentes;
use crate::controle::{competencia_texto, reais};
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
        ScrollArea::vertical()
            .id_salt("dock-reconhecimento")
            .auto_shrink([false, false])
            .show(ui, |ui| {
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
    ui.horizontal_wrapped(|ui| {
        if feita {
            ui.label(icones::icone_cor("check_circle", 15.0, p.sucesso));
        }
        ui.label(RichText::new(sug.rotulo()).color(if ignorada {
            p.tinta_desabilitada
        } else {
            p.tinta
        }));
    });
    if let Sugestao::UsarRubricas {
        divergencia: Some(d),
        ..
    } = sug
    {
        let sinal = if d.0 > 0 { "acima" } else { "abaixo" };
        aviso_com_triangulo(
            ui,
            p,
            &format!(
                "O líquido das rubricas fica {} {sinal} do valor digitado no compromisso.",
                reais(Centavos(d.0.abs()))
            ),
        );
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
