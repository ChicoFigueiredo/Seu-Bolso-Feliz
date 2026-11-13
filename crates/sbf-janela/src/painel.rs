//! Visão geral e Relatórios: desenham um [`Painel`] (avisos e seções com tabela).
//! Os Relatórios têm o seletor de pergunta e o de fonte: "Inventário · a conferir" e
//! "Livro-caixa", desabilitado até a fatia 2 (o mesmo código, só muda a fonte).

use egui::{RichText, Ui};
use sbf_inventario::consulta::{Filtro, Fonte, Painel, Pergunta};

use crate::componentes;
use crate::formato::A_CONFERIR;
use crate::icones;
use crate::leitura::Leitor;
use crate::navegacao::{Destino, Pedido};
use crate::pagina::Vista;
use crate::tabela;
use crate::tema;

/// As perguntas do §11, na ordem da spec.
pub const PERGUNTAS: [Pergunta; 8] = [
    Pergunta::QuantoSaiu,
    Pergunta::MaioresFornecedores,
    Pergunta::OQueSeRepete,
    Pergunta::OQueAumentou,
    Pergunta::OndeSeConcentra,
    Pergunta::NaoClassificado,
    Pergunta::PossiveisDuplicidades,
    Pergunta::OQueFaltaParaConfiar,
];

pub fn rotulo_da_pergunta(p: Pergunta) -> &'static str {
    match p {
        Pergunta::QuantoSaiu => "Quanto saiu",
        Pergunta::MaioresFornecedores => "Maiores fornecedores",
        Pergunta::OQueSeRepete => "O que se repete",
        Pergunta::OQueAumentou => "O que aumentou",
        Pergunta::OndeSeConcentra => "Onde se concentra",
        Pergunta::NaoClassificado => "O que não foi classificado",
        Pergunta::PossiveisDuplicidades => "Possíveis duplicidades",
        Pergunta::OQueFaltaParaConfiar => "O que falta para confiar",
    }
}

pub fn icone_da_pergunta(p: Pergunta) -> &'static str {
    match p {
        Pergunta::QuantoSaiu => "trending_down",
        Pergunta::MaioresFornecedores => "storefront",
        Pergunta::OQueSeRepete => "repeat",
        Pergunta::OQueAumentou => "north",
        Pergunta::OndeSeConcentra => "donut_small",
        Pergunta::NaoClassificado => "help",
        Pergunta::PossiveisDuplicidades => "difference",
        Pergunta::OQueFaltaParaConfiar => "verified",
    }
}

/// Desenha um painel: avisos no topo e cada seção com a sua tabela.
pub fn desenhar_painel(
    ui: &mut Ui,
    chave: &str,
    painel: &Painel,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    for a in &painel.avisos {
        componentes::aviso(ui, "warning", a, p.aviso_sutil, p.aviso_borda, p.aviso);
        ui.add_space(4.0);
    }
    for (i, s) in painel.secoes.iter().enumerate() {
        ui.add_space(8.0);
        componentes::cartao(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                componentes::titulo(ui, &s.titulo, 15.0);
                if let Some(n) = &s.nota {
                    ui.label(RichText::new(n).size(12.0).color(p.tinta_suave));
                }
            });
            ui.add_space(4.0);
            let tabela = std::sync::Arc::new(s.tabela.clone());
            let id = format!("{chave}:{i}");
            let mut estado = vista.tabelas.remove(&id).unwrap_or_default();
            let altura = (s.tabela.linhas.len() as f32 * 26.0 + 30.0).min(360.0);
            tabela::desenhar(
                ui,
                &id,
                &tabela,
                &mut estado,
                &mut vista.selecao,
                pedidos,
                Some(altura),
            );
            vista.tabelas.insert(id, estado);
        });
    }
}

/// A área Visão geral.
pub fn visao_geral(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    match leitor.visao_geral() {
        Ok(painel) => {
            egui::ScrollArea::vertical()
                .id_salt("visao-geral")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    desenhar_painel(ui, "visao-geral", &painel, vista, pedidos)
                });
        }
        Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
    }
}

/// O estado da área Relatórios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstadoDosRelatorios {
    pub pergunta: Pergunta,
    pub fonte: Fonte,
}

impl Default for EstadoDosRelatorios {
    fn default() -> Self {
        Self {
            pergunta: Pergunta::QuantoSaiu,
            fonte: Fonte::Inventario,
        }
    }
}

/// A área Relatórios.
pub fn relatorios(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    estado: &mut EstadoDosRelatorios,
    filtro: &Filtro,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    // Seletor de fonte.
    ui.horizontal(|ui| {
        ui.label(RichText::new("Fonte").color(p.tinta_suave));
        ui.selectable_value(
            &mut estado.fonte,
            Fonte::Inventario,
            format!("Inventário · {A_CONFERIR}"),
        );
        ui.add_enabled(false, egui::Button::selectable(false, "Livro-caixa"))
            .on_disabled_hover_text("Chega na fatia 2, com o livro-caixa");
        ui.separator();
        ui.label(
            RichText::new("Movimento entre contas não conta como gasto.")
                .size(12.0)
                .color(p.tinta_suave),
        );
    });
    ui.add_space(6.0);
    // Seletor de pergunta.
    ui.horizontal_wrapped(|ui| {
        for q in PERGUNTAS {
            let ativa = estado.pergunta == q;
            let cor = if ativa { p.destaque_tinta } else { p.tinta };
            if ui
                .add(egui::Button::selectable(
                    ativa,
                    (
                        icones::icone_cor(icone_da_pergunta(q), 15.0, cor),
                        RichText::new(rotulo_da_pergunta(q)).color(cor),
                    ),
                ))
                .clicked()
            {
                estado.pergunta = q;
            }
        }
        let r = ui.add(
            egui::Button::new((
                icones::icone_cor("lightbulb", 15.0, p.tinta_suave),
                RichText::new("O que parece cortável").color(p.tinta_suave),
            ))
            .frame_when_inactive(false),
        );
        let r = r.on_hover_text(
            "Fica em Pontos de atenção, que chega na fatia 6. Esta fatia não julga gasto.",
        );
        if r.clicked() {
            crate::navegacao::pedir_abrir(ui.ctx(), pedidos, Destino::AreaFutura("pontos"), false);
        }
    });
    ui.add_space(6.0);
    match leitor.relatorio(estado.pergunta, estado.fonte, filtro) {
        Ok(painel) => {
            let chave = format!("relatorio:{:?}", estado.pergunta);
            egui::ScrollArea::vertical()
                .id_salt("relatorios")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    desenhar_painel(ui, &chave, &painel, vista, pedidos)
                });
        }
        Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
    }
}
