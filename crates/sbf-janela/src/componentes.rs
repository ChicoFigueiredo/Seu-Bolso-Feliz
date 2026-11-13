//! Peças pequenas de interface: link de item, selos, células, botões de Ação.
//!
//! Toda cor vem da [`Paleta`](crate::tema::Paleta) do tema em uso; nada de hex aqui.

use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Frame, Margin, Response, RichText, Sense, Stroke, Ui,
};
use sbf_inventario::Ref;
use sbf_inventario::consulta::{Celula, Fonte, PedidoDeTabela};

use crate::acoes::{self, ContextoDasAcoes, IdAcao};
use crate::formato::{self, A_CONFERIR};
use crate::icones;
use crate::navegacao::{Destino, Pedido};
use crate::tema::{self, familia_forte};

/// Ctrl (ou Cmd) apertado neste quadro: o clique abre num painel flutuante.
pub fn ctrl(ui: &Ui) -> bool {
    ui.input(|i| i.modifiers.command)
}

/// Um texto clicável que abre um destino. Ctrl+clique abre no painel flutuante.
pub fn link_para(
    ui: &mut Ui,
    texto: &str,
    destino: Destino,
    pedidos: &mut Vec<Pedido>,
) -> Response {
    let p = tema::paleta(ui.ctx());
    let resposta = ui.add(
        egui::Label::new(RichText::new(texto).color(p.destaque_tinta))
            .sense(Sense::click())
            .truncate(),
    );
    let resposta = resposta.on_hover_cursor(CursorIcon::PointingHand);
    if resposta.hovered() {
        let r = resposta.rect;
        ui.painter().hline(
            r.x_range(),
            r.bottom() - 1.0,
            Stroke::new(1.0, p.destaque_tinta),
        );
    }
    if resposta.clicked() {
        let flutuante = ctrl(ui);
        crate::navegacao::pedir_abrir(ui.ctx(), pedidos, destino, flutuante);
    } else if resposta.middle_clicked() {
        crate::navegacao::pedir_abrir(ui.ctx(), pedidos, destino, true);
    }
    resposta
}

/// Link para a página de um item.
pub fn link_item(ui: &mut Ui, alvo: &Ref, texto: &str, pedidos: &mut Vec<Pedido>) -> Response {
    link_para(ui, texto, Destino::Item(alvo.clone()), pedidos).on_hover_text(format!(
        "{} · clique abre, Ctrl+clique abre ao lado",
        alvo.tipo.singular()
    ))
}

/// Um selo pequeno (pílula) com texto e cor de fundo.
pub fn selo(ui: &mut Ui, texto: &str, fundo: Color32, cor: Color32) -> Response {
    Frame::new()
        .fill(fundo)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(texto).size(11.5).color(cor));
        })
        .response
}

/// O selo da fatia em que algo chega ("fatia 2").
pub fn selo_fatia(ui: &mut Ui, fatia: u8) -> Response {
    let p = tema::paleta(ui.ctx());
    selo(ui, &format!("fatia {fatia}"), p.superficie_3, p.tinta_suave)
}

/// Cores de um estado (achado, registro, lote) pelo rótulo.
pub fn cores_do_estado(ui: &Ui, rotulo: &str) -> (Color32, Color32, &'static str) {
    let p = tema::paleta(ui.ctx());
    let r = rotulo.to_lowercase();
    if r.starts_with("documento") || r.starts_with("aprovad") || r.starts_with("respondid") {
        (p.sucesso_sutil, p.sucesso, "check_circle")
    } else if r.starts_with("descartad") {
        (p.superficie_3, p.tinta_suave, "block")
    } else if r.contains("dúvida") || r.contains("duvida") || r.starts_with("aberta") {
        (p.aviso_sutil, p.aviso, "help")
    } else if r.starts_with("divergente") || r.starts_with("rejeitad") {
        (p.perigo_sutil, p.perigo, "error")
    } else if r.starts_with("a triar") {
        (p.info_sutil, p.info, "schedule")
    } else {
        (p.superficie_3, p.tinta, "radio_button_unchecked")
    }
}

/// O estado com ícone (cor nunca é o único sinal) e quem decidiu.
pub fn estado(ui: &mut Ui, rotulo: &str, quem: Option<&sbf_inventario::consulta::Quem>) {
    let p = tema::paleta(ui.ctx());
    let (fundo, cor, icone) = cores_do_estado(ui, rotulo);
    Frame::new()
        .fill(fundo)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(icones::icone_cor(icone, 13.0, cor));
            ui.label(RichText::new(rotulo).size(12.0).color(cor).strong());
        });
    let quem_texto = match quem {
        Some(q) => Some(format!("por {}", q.rotulo())),
        None if rotulo == "A triar" => Some("ninguém decidiu ainda".to_owned()),
        None => None,
    };
    if let Some(t) = quem_texto {
        ui.label(RichText::new(t).size(12.0).color(p.tinta_suave));
    }
}

/// Desenha uma célula. Links e totais clicáveis viram pedidos.
pub fn celula(ui: &mut Ui, c: &Celula, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    match c {
        Celula::Vazia => {
            ui.label(RichText::new("—").color(p.tinta_desabilitada));
        }
        Celula::Texto(t) => {
            ui.add(egui::Label::new(t.as_str()).truncate());
        }
        Celula::Data(d) => {
            ui.label(formato::data(d));
        }
        Celula::Link { alvo, texto } => {
            link_item(ui, alvo, texto, pedidos);
        }
        Celula::Links(lista) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (i, (alvo, texto)) in lista.iter().enumerate() {
                    if i > 0 {
                        ui.label(RichText::new("·").color(p.tinta_suave));
                    }
                    link_item(ui, alvo, texto, pedidos);
                }
            });
        }
        Celula::Valor {
            centavos,
            fonte,
            itens,
        } => {
            let texto = formato::dinheiro(*centavos);
            let dica = match fonte {
                Fonte::Inventario => {
                    format!("{texto} · {A_CONFERIR}: vem dos registros do inventário")
                }
                Fonte::LivroCaixa => format!("{texto} · livro-caixa"),
            };
            total(ui, &texto, itens.as_deref(), &dica, pedidos);
        }
        Celula::Contagem { n, itens } => {
            let texto = formato::inteiro(*n);
            total(
                ui,
                &texto,
                itens.as_deref(),
                "Abre os itens que esta contagem conta",
                pedidos,
            );
        }
        Celula::Estado { rotulo, quem } => {
            ui.horizontal(|ui| estado(ui, rotulo, quem.as_ref()));
        }
    }
}

/// Um número: clicável (sublinhado pontilhado) quando sabe o que o soma.
pub fn total(
    ui: &mut Ui,
    texto: &str,
    itens: Option<&PedidoDeTabela>,
    dica: &str,
    pedidos: &mut Vec<Pedido>,
) -> Response {
    let p = tema::paleta(ui.ctx());
    total_rico(
        ui,
        RichText::new(texto).color(p.tinta),
        texto,
        itens,
        dica,
        pedidos,
        p.borda_forte,
    )
}

/// Um total com estilo próprio (cartões de destaque). `cor_do_traco` é o pontilhado.
pub fn total_rico(
    ui: &mut Ui,
    rico: RichText,
    texto: &str,
    itens: Option<&PedidoDeTabela>,
    dica: &str,
    pedidos: &mut Vec<Pedido>,
    cor_do_traco: egui::Color32,
) -> Response {
    match itens {
        Some(pedido) => {
            let r = ui
                .add(egui::Label::new(rico).sense(Sense::click()))
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(format!("{dica}\nClique abre os itens que o somam"));
            // Sublinhado pontilhado: o sinal de "todo total é clicável".
            let y = r.rect.bottom() - 1.0;
            let mut x = r.rect.left();
            while x < r.rect.right() {
                ui.painter().hline(
                    x..=(x + 2.0).min(r.rect.right()),
                    y,
                    Stroke::new(1.0, cor_do_traco),
                );
                x += 4.0;
            }
            if r.clicked() {
                let flutuante = ctrl(ui);
                crate::navegacao::pedir_abrir(
                    ui.ctx(),
                    pedidos,
                    Destino::Total {
                        titulo: format!("O que soma {texto}"),
                        pedido: pedido.clone(),
                    },
                    flutuante,
                );
            }
            r
        }
        None => ui.label(rico).on_hover_text(dica),
    }
}

/// Um botão de Ação da lista única: ícone, rótulo, atalho no tooltip, desabilitado com o
/// motivo. Devolve `true` se clicado.
pub fn botao_de_acao(ui: &mut Ui, id: IdAcao, contexto: &ContextoDasAcoes) -> bool {
    let def = acoes::definicao(id);
    let disp = acoes::disponibilidade(id, contexto);
    let p = tema::paleta(ui.ctx());
    let cor = if disp.habilitada() {
        p.tinta
    } else {
        p.tinta_desabilitada
    };
    let botao = egui::Button::new((
        icones::icone_cor(def.icone, 16.0, cor),
        RichText::new(def.rotulo).color(cor),
    ))
    .frame_when_inactive(false);
    let mut dica = def.dica.to_owned();
    if let Some(at) = def.atalho {
        dica.push_str(&format!(" ({})", at.texto()));
    }
    let r = ui.add_enabled(disp.habilitada(), botao);
    let r = match disp.motivo() {
        Some(m) => r.on_disabled_hover_text(format!("{}: {m}", def.rotulo)),
        None => r.on_hover_text(dica),
    };
    r.clicked()
}

/// Um botão pequeno só de ícone.
pub fn botao_icone(ui: &mut Ui, icone: &str, dica: &str, habilitado: bool) -> Response {
    let p = tema::paleta(ui.ctx());
    let cor = if habilitado {
        p.tinta
    } else {
        p.tinta_desabilitada
    };
    ui.add_enabled(
        habilitado,
        egui::Button::new(icones::icone_cor(icone, 18.0, cor)).frame_when_inactive(false),
    )
    .on_hover_text(dica)
}

/// Rótulo de seção em caixa alta, como no protótipo ("DINHEIRO", "TOTAL POR CICLO").
pub fn rotulo_de_secao(ui: &mut Ui, texto: &str) {
    let p = tema::paleta(ui.ctx());
    ui.label(
        RichText::new(texto.to_uppercase())
            .font(FontId::new(11.5, familia_forte()))
            .color(p.tinta_suave),
    );
}

/// Um título forte.
pub fn titulo(ui: &mut Ui, texto: &str, tamanho: f32) -> Response {
    let p = tema::paleta(ui.ctx());
    ui.label(
        RichText::new(texto)
            .font(FontId::new(tamanho, familia_forte()))
            .color(p.tinta),
    )
}

/// Uma caixa com borda e fundo de superfície 1 (cartões, avisos).
pub fn cartao(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) -> Response {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(p.superficie_1)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, conteudo)
        .response
}

/// Um aviso com ícone (cor nunca é o único sinal).
pub fn aviso(ui: &mut Ui, icone: &str, texto: &str, fundo: Color32, borda: Color32, cor: Color32) {
    Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor(icone, 16.0, cor));
                ui.label(RichText::new(texto).color(cor));
            });
        });
}

/// O selo de fatia futura com o motivo, para páginas e áreas que ainda não funcionam.
pub fn fatia_futura(ui: &mut Ui, icone: &str, titulo_da_pagina: &str, fatia: u8, motivo: &str) {
    let p = tema::paleta(ui.ctx());
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(icones::icone_cor(icone, 40.0, p.tinta_suave));
        ui.add_space(6.0);
        titulo(ui, titulo_da_pagina, 20.0);
        ui.add_space(4.0);
        selo_fatia(ui, fatia);
        ui.add_space(6.0);
        ui.label(RichText::new(motivo).color(p.tinta_suave));
    });
}
