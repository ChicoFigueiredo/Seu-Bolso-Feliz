//! O desenho da Caixa de entrada: a área do painel da esquerda (seção Documentos) e a seção
//! "Sem padrão de reconhecimento" de A triar. Só desenha e põe [`PedidoDaFila`]s em
//! `fila.pendentes`; quem aplica é [`crate::Janela::tique_da_fila`]. O estado e as regras
//! estão em [`crate::caixa_de_entrada`].

use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use sbf_inventario::caixa_de_entrada::{Entrada, Origem, Situacao};

use crate::caixa_de_entrada::{Escopo, EstadoDaFila, PedidoDaFila, chegada_em_texto};
use crate::componentes;
use crate::formato;
use crate::tema;

/// O texto de uma linha: o tipo e o resumo.
fn o_que_e(e: &Entrada) -> String {
    match (&e.tipo, &e.resumo) {
        (Some(t), Some(r)) if r.starts_with(t.as_str()) => r.clone(),
        (Some(t), Some(r)) => format!("{t} · {r}"),
        (Some(t), None) => t.clone(),
        (None, Some(r)) => r.clone(),
        (None, None) => "—".to_owned(),
    }
}

fn tabela(
    ui: &mut Ui,
    id: &str,
    entradas: &[Entrada],
    selecionada: Option<&str>,
    pendentes: &mut Vec<PedidoDaFila>,
    altura: Option<f32>,
) {
    let p = tema::paleta(ui.ctx());
    let mut construtor = TableBuilder::new(ui)
        .id_salt(id)
        .striped(false)
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .auto_shrink([false, true])
        .column(Column::initial(96.0).at_least(70.0).clip(true))
        .column(Column::initial(96.0).at_least(60.0).clip(true))
        .column(Column::initial(220.0).at_least(80.0).clip(true))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::initial(170.0).at_least(110.0).clip(true));
    if let Some(h) = altura {
        construtor = construtor.max_scroll_height(h).min_scrolled_height(60.0);
    }
    construtor
        .header(24.0, |mut c| {
            for t in ["Chegada", "Origem", "Arquivo", "Reconhecido", "Estado"] {
                c.col(|ui| {
                    ui.label(RichText::new(t).strong().size(12.5).color(p.tinta_suave));
                });
            }
        })
        .body(|corpo| {
            corpo.rows(26.0, entradas.len(), |mut linha| {
                let e = &entradas[linha.index()];
                linha.set_selected(selecionada == Some(e.content_hash.as_str()));
                let fechada = e.situacao != Situacao::Aberta;
                let cor = if fechada { p.tinta_suave } else { p.tinta };
                linha.col(|ui| {
                    ui.label(RichText::new(chegada_em_texto(&e.chegou_em)).color(cor));
                });
                linha.col(|ui| {
                    ui.label(RichText::new(e.origem.rotulo()).color(cor));
                });
                linha.col(|ui| {
                    ui.add(egui::Label::new(RichText::new(&e.nome).color(cor)).truncate())
                        .on_hover_text(&e.nome);
                });
                linha.col(|ui| {
                    let texto = o_que_e(e);
                    ui.add(egui::Label::new(RichText::new(&texto).color(cor)).truncate())
                        .on_hover_text(texto);
                });
                linha.col(|ui| match e.situacao {
                    Situacao::Aberta => componentes::estado_compacto(ui, e.estado.rotulo(), None),
                    Situacao::Resolvida => {
                        ui.label(RichText::new("Resolvido").color(p.sucesso));
                    }
                    Situacao::Dispensada => {
                        ui.label(RichText::new("Dispensado").color(p.tinta_suave));
                    }
                });
                if linha.response().clicked() {
                    pendentes.push(PedidoDaFila::Selecionar(e.content_hash.clone()));
                }
            });
        });
}

/// A faixa de ações da linha escolhida: Resolver, Dispensar, Reconhecer de novo.
fn acoes(ui: &mut Ui, fila: &EstadoDaFila, pendentes: &mut Vec<PedidoDaFila>) {
    let p = tema::paleta(ui.ctx());
    let Some(sha) = fila.selecionada.as_deref() else {
        ui.label(
            RichText::new("Escolha uma linha para ver o cartão e a prévia no painel da direita.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        return;
    };
    let escolhida = fila
        .caixa
        .iter()
        .chain(&fila.sem_padrao)
        .find(|e| e.content_hash == sha);
    ui.horizontal_wrapped(|ui| {
        match escolhida {
            Some(e) => {
                ui.label(RichText::new(&e.nome).strong());
            }
            None => {
                ui.label(RichText::new("Item escolhido").strong());
            }
        }
        let aberta = escolhida.is_none_or(|e| e.situacao == Situacao::Aberta);
        if aberta {
            if ui
                .button("Resolver")
                .on_hover_text("Você aplicou o que queria: o item sai da lista")
                .clicked()
            {
                pendentes.push(PedidoDaFila::Resolver(sha.to_owned()));
            }
            if ui
                .button("Dispensar")
                .on_hover_text("Isto não é nada para o planejamento: o item sai da lista")
                .clicked()
            {
                pendentes.push(PedidoDaFila::Dispensar(sha.to_owned()));
            }
        } else if ui.button("Reabrir").clicked() {
            pendentes.push(PedidoDaFila::Reabrir(sha.to_owned()));
        }
        if ui
            .button("Reconhecer de novo")
            .on_hover_text("Lê o documento outra vez com as regras de agora, sem gastar IA")
            .clicked()
        {
            pendentes.push(PedidoDaFila::ReconhecerDeNovo(sha.to_owned()));
        }
    });
}

fn erro(ui: &mut Ui, fila: &EstadoDaFila) {
    if let Some(e) = &fila.erro {
        let p = tema::paleta(ui.ctx());
        componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        ui.add_space(4.0);
    }
}

/// A área "Caixa de entrada".
pub fn area(ui: &mut Ui, fila: &mut EstadoDaFila) {
    let p = tema::paleta(ui.ctx());
    let mut pendentes = Vec::new();
    ui.horizontal(|ui| {
        componentes::titulo(ui, "Caixa de entrada", 20.0);
        let n = fila.contagens.reconhecidos;
        ui.label(
            RichText::new(format!(
                "{} {}",
                formato::inteiro(n),
                if n == 1 {
                    "não resolvido"
                } else {
                    "não resolvidos"
                }
            ))
            .color(p.tinta_suave),
        );
    });
    ui.label(
        RichText::new(
            "O que chegou e foi reconhecido (Telegram, Gmail, pastas, arquivos arrastados). \
             Decida aqui as ligações com o planejamento.",
        )
        .size(12.5)
        .color(p.tinta_suave),
    );
    ui.add_space(6.0);
    erro(ui, fila);
    ui.horizontal(|ui| {
        for escopo in [Escopo::NaoResolvidos, Escopo::Todos] {
            if ui
                .selectable_label(fila.escopo == escopo, escopo.rotulo())
                .clicked()
                && fila.escopo != escopo
            {
                fila.escopo = escopo;
                pendentes.push(PedidoDaFila::Recarregar);
            }
        }
        ui.separator();
        let atual = fila.origem.map_or("Todas as origens", Origem::rotulo);
        egui::ComboBox::from_id_salt("caixa-origem")
            .selected_text(atual)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut fila.origem, None, "Todas as origens")
                    .changed()
                {
                    pendentes.push(PedidoDaFila::Recarregar);
                }
                for o in Origem::TODAS {
                    if ui
                        .selectable_value(&mut fila.origem, Some(o), o.rotulo())
                        .changed()
                    {
                        pendentes.push(PedidoDaFila::Recarregar);
                    }
                }
            });
    });
    ui.add_space(4.0);
    acoes(ui, fila, &mut pendentes);
    ui.add_space(6.0);
    if fila.caixa.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new(if fila.escopo == Escopo::NaoResolvidos {
                "Nada esperando decisão. O que o reconhecimento reconhecer, de agora em diante, aparece aqui."
            } else {
                "Nada chegou ainda por aqui."
            })
            .color(p.tinta_suave),
        );
    } else {
        tabela(
            ui,
            "caixa-de-entrada",
            &fila.caixa,
            fila.selecionada.as_deref(),
            &mut pendentes,
            None,
        );
    }
    fila.pendentes.extend(pendentes);
}

/// A seção "Sem padrão de reconhecimento", no topo de A triar: os novos que nenhuma regra
/// casou (Não reconhecido e Protegido por senha), os mais recentes primeiro.
pub fn secao_sem_padrao(ui: &mut Ui, fila: &mut EstadoDaFila) {
    if !fila.ligada() {
        return;
    }
    let p = tema::paleta(ui.ctx());
    let mut pendentes = Vec::new();
    let n = fila.contagens.sem_padrao;
    let titulo = format!("Sem padrão de reconhecimento · {}", formato::inteiro(n));
    let aberta = fila.secao_aberta;
    let resposta = egui::CollapsingHeader::new(RichText::new(titulo).strong())
        .id_salt("a-triar-sem-padrao")
        .open(Some(aberta))
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Chegaram e nenhuma regra casou. Escolha um item, crie a regra a partir \
                     do cartão e reconheça de novo: se passar a reconhecer, ele vai para a \
                     Caixa de entrada.",
                )
                .size(12.0)
                .color(p.tinta_suave),
            );
            erro(ui, fila);
            if fila.sem_padrao.is_empty() {
                ui.label(RichText::new("Nada sem padrão.").color(p.tinta_suave));
            } else {
                acoes(ui, fila, &mut pendentes);
                tabela(
                    ui,
                    "a-triar-sem-padrao-tabela",
                    &fila.sem_padrao,
                    fila.selecionada.as_deref(),
                    &mut pendentes,
                    Some(190.0),
                );
            }
        });
    if resposta.header_response.clicked() {
        fila.secao_aberta = !aberta;
    }
    ui.add_space(8.0);
    fila.pendentes.extend(pendentes);
}
