//! O desenho da Caixa de entrada: a área do painel da esquerda (seção Documentos) e a seção
//! "Sem padrão de reconhecimento" de A triar. Só desenha e põe [`PedidoDaCaixaDeEntrada`]s em
//! `caixa_de_entrada.pendentes`; quem aplica é [`crate::Janela::tique_da_caixa_de_entrada`].
//! O estado e as regras estão em [`crate::caixa_de_entrada`].

use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use sbf_inventario::caixa_de_entrada::{Canal, Entrada, Situacao};

use crate::caixa_de_entrada::{
    Escopo, EstadoDaCaixaDeEntrada, PedidoDaCaixaDeEntrada, chegada_em_texto,
};
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

/// Quais linhas aparecem (pelo filtro de cor) e a marcação para pintar, quando ligada.
struct Marcas<'a> {
    guarda: std::sync::MutexGuard<'a, crate::marcacao::Marcacao>,
}

#[allow(clippy::too_many_arguments)]
fn tabela(
    ui: &mut Ui,
    id: &str,
    entradas: &[Entrada],
    selecionada: Option<&str>,
    pendentes: &mut Vec<PedidoDaCaixaDeEntrada>,
    altura: Option<f32>,
) {
    let p = tema::paleta(ui.ctx());
    // A marcação (verde, âmbar) é opcional: sem catálogo de padrões ligado, a tabela é a de
    // sempre, sem coluna nem filtro.
    let compartilhada = crate::marcacao::do_contexto(ui.ctx());
    let mut marcas = compartilhada
        .as_ref()
        .and_then(|m| m.lock().ok())
        .map(|guarda| Marcas { guarda });
    let mut visiveis: Vec<&Entrada> = entradas.iter().collect();
    if let Some(m) = marcas.as_mut() {
        crate::marcacao::faixa_do_filtro(ui, &mut m.guarda, id);
        let ids: Vec<&str> = entradas.iter().map(|e| e.content_hash.as_str()).collect();
        let passam = m.guarda.passam(id, &ids);
        visiveis = entradas
            .iter()
            .zip(passam)
            .filter_map(|(e, ok)| ok.then_some(e))
            .collect();
        if visiveis.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("Nenhum documento desta cor.").color(p.tinta_suave));
            return;
        }
    }
    let com_marcas = marcas.is_some();
    let mut construtor = TableBuilder::new(ui)
        .id_salt(id)
        .striped(false)
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .auto_shrink([false, true])
        .column(Column::initial(96.0).at_least(70.0).clip(true))
        .column(Column::initial(110.0).at_least(60.0).clip(true))
        .column(Column::initial(220.0).at_least(80.0).clip(true))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::initial(170.0).at_least(110.0).clip(true));
    if com_marcas {
        construtor = construtor.column(Column::initial(96.0).at_least(80.0).clip(true));
    }
    if let Some(h) = altura {
        construtor = construtor.max_scroll_height(h).min_scrolled_height(60.0);
    }
    construtor
        .header(24.0, |mut c| {
            let mut titulos = vec!["Chegada", "Chegou por", "Arquivo", "Reconhecido", "Estado"];
            if com_marcas {
                titulos.push("Padrões");
            }
            for t in titulos {
                c.col(|ui| {
                    ui.label(RichText::new(t).strong().size(12.5).color(p.tinta_suave));
                });
            }
        })
        .body(|corpo| {
            corpo.rows(26.0, visiveis.len(), |mut linha| {
                let e = visiveis[linha.index()];
                let esta = selecionada == Some(e.content_hash.as_str());
                linha.set_selected(esta);
                let fechada = e.situacao != Situacao::Aberta;
                let cor = if fechada { p.tinta_suave } else { p.tinta };
                let marca = marcas
                    .as_mut()
                    .map(|m| m.guarda.marca(&e.content_hash))
                    .unwrap_or_default();
                let da_cor = marca.cor();
                linha.col(|ui| {
                    crate::marcacao::pintar_celula(ui, da_cor, esta, true);
                    ui.label(RichText::new(chegada_em_texto(&e.chegou_em)).color(cor));
                });
                linha.col(|ui| {
                    crate::marcacao::pintar_celula(ui, da_cor, esta, false);
                    ui.label(RichText::new(e.canal.rotulo()).color(cor));
                });
                linha.col(|ui| {
                    crate::marcacao::pintar_celula(ui, da_cor, esta, false);
                    ui.add(egui::Label::new(RichText::new(&e.nome).color(cor)).truncate())
                        .on_hover_text(&e.nome);
                });
                linha.col(|ui| {
                    crate::marcacao::pintar_celula(ui, da_cor, esta, false);
                    let texto = o_que_e(e);
                    ui.add(egui::Label::new(RichText::new(&texto).color(cor)).truncate())
                        .on_hover_text(texto);
                });
                linha.col(|ui| {
                    crate::marcacao::pintar_celula(ui, da_cor, esta, false);
                    match e.situacao {
                        Situacao::Aberta => {
                            componentes::estado_compacto(ui, e.estado.rotulo(), None);
                        }
                        Situacao::Resolvida => {
                            ui.label(RichText::new("Resolvido").color(p.sucesso));
                        }
                        Situacao::Dispensada => {
                            ui.label(RichText::new("Dispensado").color(p.tinta_suave));
                        }
                    }
                });
                if com_marcas {
                    linha.col(|ui| {
                        crate::marcacao::pintar_celula(ui, da_cor, esta, false);
                        crate::marcacao::selo_de_padroes(ui, marca);
                    });
                }
                if linha.response().clicked() {
                    pendentes.push(PedidoDaCaixaDeEntrada::Selecionar(e.content_hash.clone()));
                }
            });
        });
    // Uma consulta em lote para o que as linhas desenhadas pediram (e outro quadro para pintar).
    if let Some(m) = marcas.as_mut()
        && m.guarda.resolver()
    {
        ui.ctx().request_repaint();
    }
}

/// A faixa de ações da linha escolhida: Resolver, Dispensar, Reconhecer de novo.
fn acoes(ui: &mut Ui, caixa: &EstadoDaCaixaDeEntrada, pendentes: &mut Vec<PedidoDaCaixaDeEntrada>) {
    let p = tema::paleta(ui.ctx());
    let Some(sha) = caixa.selecionada.as_deref() else {
        ui.label(
            RichText::new("Escolha uma linha para ver o cartão e a prévia no painel da direita.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        return;
    };
    let escolhida = caixa
        .caixa
        .iter()
        .chain(&caixa.sem_padrao)
        .find(|e| e.content_hash == sha);
    let reconhecendo = caixa.esta_reconhecendo(sha);
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
                .on_hover_text("Você aplicou o que queria: o item sai da lista (entra no Rastro)")
                .clicked()
            {
                pendentes.push(PedidoDaCaixaDeEntrada::Resolver(sha.to_owned()));
            }
            if ui
                .button("Dispensar")
                .on_hover_text(
                    "Isto não é nada para o planejamento: o item sai da lista (entra no Rastro)",
                )
                .clicked()
            {
                pendentes.push(PedidoDaCaixaDeEntrada::Dispensar(sha.to_owned()));
            }
        } else if ui.button("Reabrir").clicked() {
            pendentes.push(PedidoDaCaixaDeEntrada::Reabrir(sha.to_owned()));
        }
        let de_novo = ui
            .add_enabled(
                !caixa.reconhecendo_algum(),
                egui::Button::new("Reconhecer de novo"),
            )
            .on_hover_text(
                "Lê o documento outra vez com as regras de agora (extrai o texto que faltar), \
                 sem IA. Não reabre o que você já decidiu.",
            );
        if de_novo.clicked() {
            pendentes.push(PedidoDaCaixaDeEntrada::ReconhecerDeNovo(sha.to_owned()));
        }
        if reconhecendo {
            ui.add(egui::Spinner::new().size(14.0));
            ui.label(
                RichText::new("Reconhecendo…")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        }
    });
}

fn erro(ui: &mut Ui, caixa: &EstadoDaCaixaDeEntrada) {
    if let Some(e) = &caixa.erro {
        let p = tema::paleta(ui.ctx());
        componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        ui.add_space(4.0);
    }
}

fn escolher_escopo(ui: &mut Ui, escopo: &mut Escopo, pendentes: &mut Vec<PedidoDaCaixaDeEntrada>) {
    for e in [Escopo::NaoResolvidos, Escopo::Todos] {
        if ui.selectable_label(*escopo == e, e.rotulo()).clicked() && *escopo != e {
            *escopo = e;
            pendentes.push(PedidoDaCaixaDeEntrada::Recarregar);
        }
    }
}

/// A área "Caixa de entrada".
pub fn area(ui: &mut Ui, caixa: &mut EstadoDaCaixaDeEntrada) {
    let p = tema::paleta(ui.ctx());
    let mut pendentes = Vec::new();
    ui.horizontal(|ui| {
        componentes::titulo(ui, "Caixa de entrada", 20.0);
        let n = caixa.contagens.reconhecidos;
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
        // Processando fica fora das listas e do contador: um indicador à parte.
        if caixa.contagens.processando > 0 {
            ui.add(egui::Spinner::new().size(13.0));
            ui.label(
                RichText::new(format!("{} processando", caixa.contagens.processando))
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        }
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
    erro(ui, caixa);
    ui.horizontal(|ui| {
        escolher_escopo(ui, &mut caixa.escopo, &mut pendentes);
        ui.separator();
        let atual = caixa.canal.map_or("Todos os canais", Canal::rotulo);
        egui::ComboBox::from_id_salt("caixa-canal")
            .selected_text(atual)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut caixa.canal, None, "Todos os canais")
                    .changed()
                {
                    pendentes.push(PedidoDaCaixaDeEntrada::Recarregar);
                }
                for c in Canal::TODOS {
                    if ui
                        .selectable_value(&mut caixa.canal, Some(c), c.rotulo())
                        .changed()
                    {
                        pendentes.push(PedidoDaCaixaDeEntrada::Recarregar);
                    }
                }
            });
    });
    ui.add_space(4.0);
    acoes(ui, caixa, &mut pendentes);
    ui.add_space(6.0);
    if caixa.caixa.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new(if caixa.escopo == Escopo::NaoResolvidos {
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
            &caixa.caixa,
            caixa.selecionada.as_deref(),
            &mut pendentes,
            None,
        );
    }
    caixa.pendentes.extend(pendentes);
}

/// A seção "Sem padrão de reconhecimento", no topo de A triar: os novos que nenhuma regra
/// casou (Não reconhecido e Protegido por senha), os mais recentes primeiro. Com o mesmo
/// filtro Não resolvidos / Todos da Caixa de entrada.
pub fn secao_sem_padrao(ui: &mut Ui, caixa: &mut EstadoDaCaixaDeEntrada) {
    if !caixa.ligada() {
        return;
    }
    let p = tema::paleta(ui.ctx());
    let mut pendentes = Vec::new();
    let n = caixa.contagens.sem_padrao;
    let titulo = format!("Sem padrão de reconhecimento · {}", formato::inteiro(n));
    let aberta = caixa.secao_aberta;
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
            erro(ui, caixa);
            ui.horizontal(|ui| {
                escolher_escopo(ui, &mut caixa.escopo_sem_padrao, &mut pendentes);
            });
            if caixa.sem_padrao.is_empty() {
                ui.label(RichText::new("Nada sem padrão.").color(p.tinta_suave));
            } else {
                acoes(ui, caixa, &mut pendentes);
                tabela(
                    ui,
                    "a-triar-sem-padrao-tabela",
                    &caixa.sem_padrao,
                    caixa.selecionada.as_deref(),
                    &mut pendentes,
                    Some(190.0),
                );
            }
        });
    if resposta.header_response.clicked() {
        caixa.secao_aberta = !aberta;
    }
    ui.add_space(8.0);
    caixa.pendentes.extend(pendentes);
}
