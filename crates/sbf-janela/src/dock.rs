//! Os painéis do dock como conteúdo: Detalhe, Prévia, Origens, Texto extraído e Histórico
//! de decisões. Cada um mostra o item em foco (a última linha clicada, ou a página aberta).
//! Quem arruma os painéis (egui_tiles, abas, fechar) é a casca; aqui só o conteúdo.

use egui::{RichText, Ui};
use sbf_inventario::comando::IdDecisao;
use sbf_inventario::consulta::{Celula, PedidoDeTabela};
use sbf_inventario::{Ref, Tipo};

use crate::componentes;
use crate::icones;
use crate::leitura::Leitor;
use crate::navegacao::Pedido;
use crate::pagina;
use crate::tema;

/// Os painéis do dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PainelDoDock {
    Detalhe,
    Origens,
    Texto,
    Historico,
    /// A prévia do arquivo do item em foco (a miniatura; [`crate::previa`]). Visível por
    /// padrão, embaixo do Detalhe.
    Previa,
}

impl PainelDoDock {
    pub const TODOS: [PainelDoDock; 5] = [
        PainelDoDock::Detalhe,
        PainelDoDock::Origens,
        PainelDoDock::Texto,
        PainelDoDock::Historico,
        PainelDoDock::Previa,
    ];

    pub fn titulo(self) -> &'static str {
        match self {
            PainelDoDock::Detalhe => "Detalhe",
            PainelDoDock::Origens => "Origens",
            PainelDoDock::Texto => "Texto extraído",
            PainelDoDock::Historico => "Histórico de decisões",
            PainelDoDock::Previa => "Prévia",
        }
    }

    pub fn icone(self) -> &'static str {
        match self {
            PainelDoDock::Detalhe => "info",
            PainelDoDock::Origens => "folder_open",
            PainelDoDock::Texto => "description",
            PainelDoDock::Historico => "history",
            PainelDoDock::Previa => "visibility",
        }
    }
}

fn vazio(ui: &mut Ui, texto: &str) {
    let p = tema::paleta(ui.ctx());
    ui.add_space(6.0);
    ui.label(RichText::new(texto).color(p.tinta_suave));
}

fn e_achado(r: &Ref) -> bool {
    matches!(r.tipo, Tipo::Achado | Tipo::Documento)
}

/// Desenha o conteúdo de um painel do dock.
///
/// `decisoes_da_sessao` são as decisões que você tomou nesta sessão (o Histórico sem
/// item em foco mostra essas).
pub fn desenhar(
    ui: &mut Ui,
    painel: PainelDoDock,
    leitor: &mut Leitor<'_>,
    foco: Option<&Ref>,
    decisoes_da_sessao: &[IdDecisao],
    pedidos: &mut Vec<Pedido>,
) {
    match painel {
        PainelDoDock::Detalhe => detalhe(ui, leitor, foco, pedidos),
        PainelDoDock::Origens => origens(ui, leitor, foco, pedidos),
        PainelDoDock::Texto => match foco.filter(|r| e_achado(r)) {
            Some(r) => match leitor.texto(r) {
                Ok(t) => pagina::texto_extraido(ui, t.as_deref()),
                Err(e) => vazio(ui, &e),
            },
            None => vazio(ui, "Selecione um achado para ver o texto extraído."),
        },
        PainelDoDock::Historico => historico(ui, leitor, foco, decisoes_da_sessao, pedidos),
        // A prévia precisa do estado da janela (a fila de fundo): `Janela::dock_ui`.
        PainelDoDock::Previa => vazio(ui, "Selecione um achado ou documento."),
    }
}

fn detalhe(ui: &mut Ui, leitor: &mut Leitor<'_>, foco: Option<&Ref>, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    let Some(alvo) = foco else {
        vazio(
            ui,
            "Selecione uma linha para ver o detalhe aqui. As Ações dela ficam na faixa de Ações. Os nomes são links: clique abre, Ctrl+clique abre num painel flutuante.",
        );
        return;
    };
    if let Some(m) = alvo.tipo.motivo_da_fatia() {
        vazio(ui, &m);
        return;
    }
    match leitor.item(alvo) {
        Ok(i) => match i.as_ref() {
            Some(item) => {
                ui.horizontal(|ui| {
                    ui.label(icones::icone_cor(alvo.tipo.icone(), 18.0, p.tinta));
                    componentes::link_item(ui, alvo, &item.rotulo, pedidos);
                });
                ui.label(
                    RichText::new(alvo.tipo.singular())
                        .size(12.0)
                        .color(p.tinta_suave),
                );
                ui.add_space(6.0);
                egui::Grid::new(("detalhe", alvo))
                    .num_columns(2)
                    .spacing(egui::vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        for c in &item.campos {
                            ui.label(RichText::new(&c.rotulo).size(12.5).color(p.tinta_suave));
                            ui.horizontal_wrapped(|ui| componentes::celula(ui, &c.valor, pedidos));
                            ui.end_row();
                        }
                    });
            }
            None => vazio(ui, "Item não encontrado."),
        },
        Err(e) => vazio(ui, &e),
    }
}

fn origens(ui: &mut Ui, leitor: &mut Leitor<'_>, foco: Option<&Ref>, pedidos: &mut Vec<Pedido>) {
    let Some(alvo) =
        foco.filter(|r| matches!(r.tipo, Tipo::Achado | Tipo::Documento | Tipo::Registro))
    else {
        vazio(
            ui,
            "Selecione um achado ou registro para ver onde ele foi achado.",
        );
        return;
    };
    let pedido = PedidoDeTabela::Ligados {
        de: alvo.clone(),
        tipo: Tipo::Origem,
    };
    match leitor.tabela(&pedido) {
        Ok(t) if t.linhas.is_empty() => vazio(ui, "Nenhuma origem."),
        Ok(t) => {
            let p = tema::paleta(ui.ctx());
            ui.label(
                RichText::new(format!(
                    "O mesmo conteúdo, byte a byte, achado em {} lugar{}.",
                    t.linhas.len(),
                    if t.linhas.len() == 1 { "" } else { "es" }
                ))
                .color(p.tinta_suave),
            );
            for l in &t.linhas {
                ui.separator();
                for (col, c) in t.colunas.iter().zip(&l.celulas) {
                    if matches!(c, Celula::Vazia) {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&col.rotulo).size(12.0).color(p.tinta_suave));
                        componentes::celula(ui, c, pedidos);
                    });
                }
            }
        }
        Err(e) => vazio(ui, &e),
    }
}

fn historico(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    foco: Option<&Ref>,
    da_sessao: &[IdDecisao],
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let refs: Vec<Ref> = match foco {
        Some(alvo) if alvo.tipo.motivo_da_fatia().is_none() => {
            let pedido = PedidoDeTabela::Ligados {
                de: alvo.clone(),
                tipo: Tipo::Decisao,
            };
            match leitor.tabela(&pedido) {
                Ok(t) => t.linhas.iter().filter_map(|l| l.alvo.clone()).collect(),
                Err(e) => {
                    vazio(ui, &e);
                    return;
                }
            }
        }
        _ => da_sessao
            .iter()
            .rev()
            .map(|id| Ref::nova(Tipo::Decisao, id.to_string()))
            .collect(),
    };
    match foco {
        Some(a) => {
            let rotulo = leitor.rotulo(a);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Decisões sobre").color(p.tinta_suave));
                componentes::link_item(ui, a, &rotulo, pedidos);
            });
        }
        None => {
            ui.label(
                RichText::new("Suas decisões nesta sessão, da mais nova para a mais antiga.")
                    .color(p.tinta_suave),
            );
        }
    }
    if refs.is_empty() {
        vazio(ui, "Nenhuma decisão ainda.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("historico-de-decisoes")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for r in refs {
                ui.separator();
                let item = leitor.item(&r);
                let Ok(item) = item else { continue };
                let Some(item) = item.as_ref() else { continue };
                ui.horizontal(|ui| {
                    ui.label(icones::icone_cor("gavel", 16.0, p.tinta_suave));
                    componentes::link_item(ui, &r, &item.rotulo, pedidos);
                });
                for c in &item.campos {
                    if matches!(c.valor, Celula::Vazia) {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&c.rotulo).size(12.0).color(p.tinta_suave));
                        componentes::celula(ui, &c.valor, pedidos);
                    });
                }
                if let Ok(id) = r.chave.parse::<IdDecisao>()
                    && ui
                        .add(
                            egui::Button::new((
                                icones::icone_cor("undo", 15.0, p.tinta),
                                "Desfazer",
                            ))
                            .small(),
                        )
                        .on_hover_text("Grava uma nova decisão que restaura o antes")
                        .clicked()
                {
                    pedidos.push(Pedido::Desfazer(id));
                }
            }
        });
}
