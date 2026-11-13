//! A tabela densa: virtualizada (só desenha as linhas visíveis), ordenável pelo
//! cabeçalho, com seleção múltipla (clique, Ctrl+clique, Shift+clique) e duplo clique
//! para abrir a página do item. Toda `Celula::Link` é link e todo total é clicável.

use std::sync::Arc;

use egui::{Align, CursorIcon, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use sbf_inventario::Ref;
use sbf_inventario::consulta::{Celula, Fonte, Tabela};

use crate::componentes;
use crate::formato::{self, A_CONFERIR, Ordem};
use crate::navegacao::{Destino, Pedido};
use crate::tema;

/// A seleção da tabela do centro (e de onde o dock lê).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selecao {
    pub itens: Vec<Ref>,
    /// O último item clicado: o que o dock mostra.
    pub foco: Option<Ref>,
    /// Índice (na ordem exibida) da âncora do Shift+clique.
    ancora: Option<usize>,
}

impl Selecao {
    pub fn limpar(&mut self) {
        *self = Selecao::default();
    }

    pub fn contem(&self, r: &Ref) -> bool {
        self.itens.contains(r)
    }

    /// Seleciona só este item.
    pub fn so(&mut self, r: Ref) {
        self.itens = vec![r.clone()];
        self.foco = Some(r);
    }
}

/// Endereço da tabela, número de linhas e ordem: muda, reordena.
type Assinatura = (usize, usize, Option<(usize, bool)>);

/// O estado de exibição de uma tabela: a ordem e o cache dos índices ordenados.
#[derive(Debug, Clone, Default)]
pub struct EstadoDaTabela {
    /// Coluna e sentido (`true` = crescente).
    pub ordem: Option<(usize, bool)>,
    /// A tabela do centro responde ao teclado: ↑/↓ movem a seleção, Shift estende,
    /// Enter abre a página do item.
    pub teclado: bool,
    indices: Vec<usize>,
    assinatura: Option<Assinatura>,
}

impl EstadoDaTabela {
    /// Os índices das linhas na ordem exibida. Só reordena quando a tabela ou a ordem mudam.
    pub fn indices(&mut self, tabela: &Arc<Tabela>) -> &[usize] {
        let assinatura = (
            Arc::as_ptr(tabela) as usize,
            tabela.linhas.len(),
            self.ordem,
        );
        if self.assinatura != Some(assinatura) {
            let mut idx: Vec<usize> = (0..tabela.linhas.len()).collect();
            if let Some((col, crescente)) = self.ordem {
                let chave = |i: &usize| {
                    tabela.linhas[*i]
                        .celulas
                        .get(col)
                        .map(formato::ordem_da_celula)
                        .unwrap_or(Ordem::Vazia)
                };
                idx.sort_by(|a, b| {
                    let o = chave(a)
                        .partial_cmp(&chave(b))
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if crescente { o } else { o.reverse() }
                });
            }
            self.indices = idx;
            self.assinatura = Some(assinatura);
        }
        &self.indices
    }
}

const ALTURA_DA_LINHA: f32 = 26.0;

fn coluna_do_inventario(tabela: &Tabela, col: usize) -> bool {
    tabela.linhas.iter().take(64).any(|l| {
        matches!(
            l.celulas.get(col),
            Some(Celula::Valor {
                fonte: Fonte::Inventario,
                ..
            })
        )
    })
}

/// Larguras iniciais pelo conteúdo (cabeçalho e as primeiras linhas), entre 48 e 360 px.
fn larguras_das_colunas(tabela: &Tabela) -> Vec<f32> {
    tabela
        .colunas
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut maior = c.rotulo.chars().count()
                + if coluna_do_inventario(tabela, i) {
                    14
                } else {
                    2
                };
            for l in tabela.linhas.iter().take(200) {
                if let Some(cel) = l.celulas.get(i) {
                    let extra = if matches!(cel, Celula::Estado { .. }) {
                        6
                    } else {
                        0
                    };
                    let n = match cel {
                        Celula::Valor { centavos, .. } => {
                            formato::dinheiro(*centavos).chars().count()
                        }
                        outra => formato::texto_da_celula(outra).chars().count(),
                    };
                    maior = maior.max(n + extra);
                }
            }
            (maior as f32 * 6.8 + 18.0).clamp(48.0, 360.0)
        })
        .collect()
}

/// Desenha a tabela. `altura` limita a altura (abas de página); `None` ocupa o resto.
pub fn desenhar(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    tabela: &Arc<Tabela>,
    estado: &mut EstadoDaTabela,
    selecao: &mut Selecao,
    pedidos: &mut Vec<Pedido>,
    altura: Option<f32>,
) {
    let p = tema::paleta(ui.ctx());
    if tabela.linhas.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("Nada aqui.").color(p.tinta_suave));
        return;
    }
    let n_colunas = tabela.colunas.len().max(1);
    let indices = estado.indices(tabela).to_vec();
    let marcas: Vec<bool> = (0..n_colunas)
        .map(|c| coluna_do_inventario(tabela, c))
        .collect();
    let mut ordem_nova = None;
    let mut clique: Option<(usize, bool, bool, bool)> = None; // (pos, ctrl, shift, duplo)

    let mut larguras = larguras_das_colunas(tabela);
    // Cabe na largura disponível: um teto comum para as colunas mais largas (as
    // estreitas, como data e valor, ficam inteiras).
    let disponivel = ui.available_width() - 8.0 * n_colunas as f32;
    if larguras.iter().sum::<f32>() > disponivel {
        let (mut baixo, mut alto) = (48.0f32, 360.0f32);
        for _ in 0..24 {
            let meio = (baixo + alto) / 2.0;
            let total: f32 = tabela
                .colunas
                .iter()
                .zip(&larguras)
                .map(|(c, l)| if c.numerica { *l } else { l.min(meio) })
                .sum();
            if total > disponivel {
                alto = meio;
            } else {
                baixo = meio;
            }
        }
        for (c, l) in tabela.colunas.iter().zip(larguras.iter_mut()) {
            if !c.numerica {
                *l = l.min(baixo);
            }
        }
    }
    {
        // Teclado: ↑/↓ movem a seleção (Shift estende), Enter abre o item em foco.
        let mut rolar_para = None;
        if estado.teclado && !ui.ctx().egui_wants_keyboard_input() && !indices.is_empty() {
            let (baixo, cima, enter, shift) = ui.input_mut(|i| {
                let shift = i.modifiers.shift;
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                        || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                        || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    shift,
                )
            });
            if baixo || cima || enter {
                let atual = selecao.foco.as_ref().and_then(|f| {
                    indices
                        .iter()
                        .position(|i| tabela.linhas[*i].alvo.as_ref() == Some(f))
                });
                if enter {
                    if let Some(f) = selecao.foco.clone() {
                        pedidos.push(Pedido::Abrir(Destino::Item(f)));
                    }
                } else {
                    let nova = match (atual, baixo) {
                        (None, _) => 0,
                        (Some(p), true) => (p + 1).min(indices.len() - 1),
                        (Some(p), false) => p.saturating_sub(1),
                    };
                    if let Some(alvo) = tabela.linhas[indices[nova]].alvo.clone() {
                        if shift {
                            if !selecao.itens.contains(&alvo) {
                                selecao.itens.push(alvo.clone());
                            }
                            selecao.foco = Some(alvo);
                        } else {
                            selecao.so(alvo);
                            selecao.ancora = Some(nova);
                        }
                    }
                    rolar_para = Some(nova);
                }
            }
        }
        let mut construtor = TableBuilder::new(ui)
            .id_salt(id)
            .striped(false)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .auto_shrink([false, true]);
        if let Some(linha) = rolar_para {
            construtor = construtor.scroll_to_row(linha, None);
        }
        if let Some(h) = altura {
            construtor = construtor
                .max_scroll_height(h)
                .min_scrolled_height(h.min(160.0));
        }
        for (i, coluna) in tabela.colunas.iter().enumerate() {
            let c = if i + 1 == n_colunas {
                Column::remainder().at_least(60.0).clip(true)
            } else if coluna.numerica {
                Column::initial(larguras[i]).at_least(56.0).clip(true)
            } else {
                Column::initial(larguras[i]).at_least(40.0).clip(true)
            };
            construtor = construtor.column(c);
        }
        construtor
            .header(26.0, |mut cabecalho| {
                for (i, coluna) in tabela.colunas.iter().enumerate() {
                    cabecalho.col(|ui| {
                        let seta = match estado.ordem {
                            Some((c, true)) if c == i => " ↑",
                            Some((c, false)) if c == i => " ↓",
                            _ => "",
                        };
                        let mut texto = coluna.rotulo.clone();
                        if marcas[i] {
                            texto.push_str(&format!(" · {A_CONFERIR}"));
                        }
                        let layout = if coluna.numerica {
                            Layout::right_to_left(Align::Center)
                        } else {
                            Layout::left_to_right(Align::Center)
                        };
                        ui.with_layout(layout, |ui| {
                            let r = ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(format!("{texto}{seta}"))
                                            .strong()
                                            .size(12.5)
                                            .color(p.tinta_suave),
                                    )
                                    .sense(Sense::click())
                                    .truncate(),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("Clique para ordenar");
                            if r.clicked() {
                                ordem_nova = Some(match estado.ordem {
                                    Some((c, true)) if c == i => Some((i, false)),
                                    Some((c, false)) if c == i => None,
                                    _ => Some((i, !coluna.numerica)),
                                });
                            }
                        });
                    });
                }
            })
            .body(|corpo| {
                corpo.rows(ALTURA_DA_LINHA, indices.len(), |mut linha_ui| {
                    let pos = linha_ui.index();
                    let linha = &tabela.linhas[indices[pos]];
                    let selecionada = linha.alvo.as_ref().is_some_and(|a| selecao.contem(a));
                    linha_ui.set_selected(selecionada);
                    for (i, c) in linha.celulas.iter().enumerate().take(n_colunas) {
                        let numerica = tabela.colunas.get(i).is_some_and(|c| c.numerica);
                        linha_ui.col(|ui| {
                            if numerica {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    componentes::celula(ui, c, pedidos);
                                });
                            } else {
                                componentes::celula(ui, c, pedidos);
                            }
                        });
                    }
                    let r = linha_ui.response();
                    if r.clicked() || r.double_clicked() {
                        let (ctrl, shift) =
                            r.ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
                        clique = Some((pos, ctrl, shift, r.double_clicked()));
                    }
                });
            });
    }

    if let Some(o) = ordem_nova {
        estado.ordem = o;
    }
    if let Some((pos, ctrl, shift, duplo)) = clique {
        let alvo_de = |pos: usize| {
            indices
                .get(pos)
                .and_then(|i| tabela.linhas.get(*i))
                .and_then(|l| l.alvo.clone())
        };
        if let Some(alvo) = alvo_de(pos) {
            if duplo {
                pedidos.push(Pedido::abrir(Destino::Item(alvo.clone()), ctrl));
                selecao.so(alvo);
                selecao.ancora = Some(pos);
            } else if shift {
                let de = selecao.ancora.unwrap_or(pos);
                let (a, b) = if de <= pos { (de, pos) } else { (pos, de) };
                selecao.itens = (a..=b).filter_map(alvo_de).collect();
                selecao.foco = Some(alvo);
            } else if ctrl {
                if let Some(i) = selecao.itens.iter().position(|r| *r == alvo) {
                    selecao.itens.remove(i);
                } else {
                    selecao.itens.push(alvo.clone());
                }
                selecao.foco = Some(alvo);
                selecao.ancora = Some(pos);
            } else {
                selecao.so(alvo);
                selecao.ancora = Some(pos);
            }
        }
    }
}
