//! A página de um item (spec 06, "Navegação relacional"): cabeçalho com o resumo,
//! gráfico por ciclo quando faz sentido, abas com tudo o que se liga ao item e a aba
//! Rastro. Tipo de fatia futura: a página com o selo e o motivo.

use std::collections::HashMap;

use egui::{
    Align, CornerRadius, FontId, Frame, IntoAtoms, Layout, Margin, RichText, Sense, Stroke, Ui,
    vec2,
};
use sbf_inventario::consulta::{Celula, Fonte, PedidoDeTabela, Rastro, TotalDoCiclo};
use sbf_inventario::{Ref, Tipo};

use crate::componentes;
use crate::formato::{self, A_CONFERIR};
use crate::icones;
use crate::leitura::Leitor;
use crate::navegacao::{Destino, Pedido};
use crate::tabela::{self, EstadoDaTabela, Selecao};
use crate::tema::{self, familia_forte};

/// O estado de um painel que mostra destinos (o centro ou um flutuante).
#[derive(Debug, Clone, Default)]
pub struct Vista {
    pub selecao: Selecao,
    pub tabelas: HashMap<String, EstadoDaTabela>,
    pub abas: HashMap<Ref, usize>,
}

impl Vista {
    pub fn estado_da_tabela(&mut self, chave: &str) -> &mut EstadoDaTabela {
        self.tabelas.entry(chave.to_owned()).or_default()
    }
}

/// O que uma aba de página mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConteudoDaAba {
    /// Os itens de um tipo ligados ao item (`PedidoDeTabela::Ligados`).
    Ligados(Tipo),
    /// O texto extraído do achado.
    Texto,
    /// A cadeia de proveniência.
    Rastro,
}

/// Uma aba de página. Desabilitada traz o motivo (as abas do livro-caixa nesta fatia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbaDePagina {
    pub rotulo: String,
    pub conteudo: ConteudoDaAba,
    pub motivo: Option<String>,
}

fn aba(rotulo: &str, tipo: Tipo) -> AbaDePagina {
    let rotulo = if tipo == Tipo::Registro && !rotulo.contains(A_CONFERIR) {
        format!("{rotulo} · {A_CONFERIR}")
    } else {
        rotulo.to_owned()
    };
    AbaDePagina {
        rotulo,
        conteudo: ConteudoDaAba::Ligados(tipo),
        motivo: tipo.motivo_da_fatia(),
    }
}

fn aba_especial(rotulo: &str, conteudo: ConteudoDaAba) -> AbaDePagina {
    AbaDePagina {
        rotulo: rotulo.to_owned(),
        conteudo,
        motivo: None,
    }
}

/// As abas da página de um tipo. Toda página termina na aba Rastro.
pub fn abas_de(tipo: Tipo) -> Vec<AbaDePagina> {
    use Tipo as T;
    let mut v = match tipo {
        T::Fornecedor => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
            aba("Compromissos e recorrentes", T::Compromisso),
            aba("Regras de senha", T::RegraDeSenha),
        ],
        T::Achado | T::Documento => vec![
            aba("Origens", T::Origem),
            aba_especial("Texto extraído", ConteudoDaAba::Texto),
            aba("Registros que saíram dele", T::Registro),
            aba("Decisões", T::Decisao),
            aba("Duplicidades", T::Achado),
            aba("Pendências", T::Pendencia),
            aba("Lotes", T::Lote),
        ],
        T::Origem => vec![
            aba("Achado", T::Achado),
            aba("Coleta", T::Coleta),
            aba("Conexão", T::Conexao),
        ],
        T::Registro => vec![
            aba("Documento", T::Documento),
            aba("Decisões", T::Decisao),
            aba("Conciliados", T::Registro),
            aba("Lotes", T::Lote),
            aba("Pendências", T::Pendencia),
            aba("Movimentação", T::Movimentacao),
        ],
        T::Decisao => vec![
            aba("Achados", T::Achado),
            aba("Registros", T::Registro),
            aba("Desfaz ou desfeita por", T::Decisao),
        ],
        T::Lote => vec![
            aba("Documentos", T::Documento),
            aba("Registros", T::Registro),
            aba("Decisões", T::Decisao),
            aba("Pendências", T::Pendencia),
        ],
        T::Pendencia => vec![
            aba("Achados", T::Achado),
            aba("Registros", T::Registro),
            aba("Lotes", T::Lote),
            aba("Decisões", T::Decisao),
        ],
        T::RegraDeSenha => vec![
            aba("Fornecedor", T::Fornecedor),
            aba("Documentos", T::Documento),
            aba("Decisões", T::Decisao),
        ],
        T::Conexao => vec![
            aba("Coletas", T::Coleta),
            aba("Origens", T::Origem),
            aba("Achados", T::Achado),
        ],
        T::Coleta => vec![
            aba("Origens", T::Origem),
            aba("Achados", T::Achado),
            aba("Conexão", T::Conexao),
        ],
        T::Ciclo => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
            aba("Fornecedores", T::Fornecedor),
        ],
        T::Conta => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
            aba("Faturas", T::Fatura),
        ],
        T::Fatura => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
        ],
        // Fatia futura: só o selo, sem abas além do Rastro.
        _ => Vec::new(),
    };
    v.push(aba_especial("Rastro", ConteudoDaAba::Rastro));
    v
}

/// Tipos cuja página mostra o gráfico de totais por ciclo.
pub fn tem_grafico_por_ciclo(tipo: Tipo) -> bool {
    matches!(tipo, Tipo::Fornecedor | Tipo::Conta)
}

/// O texto do rastro para "Copiar rastro".
pub fn texto_do_rastro(alvo: &Ref, rotulo: &str, rastro: &Rastro) -> String {
    let elo = |e: &sbf_inventario::consulta::Elo| {
        let mut s = format!(
            "{} {} ({}) · {}",
            e.alvo.tipo.singular(),
            e.rotulo,
            e.alvo,
            e.papel
        );
        if let Some(q) = &e.quem {
            s.push_str(&format!(" · {}", q.rotulo()));
        }
        if let Some(q) = &e.quando {
            s.push_str(&format!(" · {}", formato::data(q)));
        }
        s
    };
    let mut linhas = vec![format!(
        "Rastro de {} {rotulo} ({alvo})",
        alvo.tipo.singular()
    )];
    for e in rastro.acima.iter().rev() {
        linhas.push(format!("↑ {}", elo(e)));
    }
    linhas.push(format!("● {} {rotulo}", alvo.tipo.singular()));
    for e in &rastro.abaixo {
        linhas.push(format!("↓ {}", elo(e)));
    }
    linhas.join("\n")
}

/// Desenha a página de um item.
pub fn desenhar(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    alvo: &Ref,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    if let Some(motivo) = alvo.tipo.motivo_da_fatia() {
        componentes::fatia_futura(
            ui,
            alvo.tipo.icone(),
            &format!("{} {}", alvo.tipo.singular(), alvo.chave),
            alvo.tipo.fatia(),
            &motivo,
        );
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("A página e o rastro deste item nascem com essa fatia.")
                    .color(p.tinta_suave),
            );
        });
        return;
    }
    let item = match leitor.item(alvo) {
        Ok(i) => i,
        Err(e) => {
            componentes::aviso(
                ui,
                "error",
                &format!("Não deu para ler {alvo}: {e}"),
                p.perigo_sutil,
                p.borda,
                p.perigo,
            );
            return;
        }
    };
    let Some(item) = item.as_ref() else {
        componentes::aviso(
            ui,
            "help",
            &format!("{} não encontrado: {}", alvo.tipo.singular(), alvo.chave),
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        return;
    };

    // Cabeçalho: ícone, nome, selo do tipo, botão de abrir ao lado.
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor(alvo.tipo.icone(), 22.0, p.tinta));
        componentes::titulo(ui, &item.rotulo, 20.0);
        componentes::selo(ui, alvo.tipo.singular(), p.superficie_3, p.tinta_suave);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if componentes::botao_icone(ui, "picture_in_picture", "Abrir em painel flutuante", true)
                .clicked()
            {
                crate::navegacao::pedir_abrir(ui.ctx(), pedidos, Destino::Item(alvo.clone()), true);
            }
        });
    });
    ui.add_space(6.0);

    // Estado em destaque, quando o item tem.
    let (estados, campos): (Vec<_>, Vec<_>) = item
        .campos
        .iter()
        .partition(|c| matches!(c.valor, Celula::Estado { .. }));
    for c in &estados {
        ui.horizontal(|ui| componentes::celula(ui, &c.valor, pedidos));
    }

    // Os valores em cartões (o primeiro em destaque), com o gráfico por ciclo ao lado,
    // como no protótipo; o resto em grade.
    let (valores, campos): (Vec<_>, Vec<_>) = campos
        .into_iter()
        .partition(|c| matches!(c.valor, Celula::Valor { .. }));
    let totais = if tem_grafico_por_ciclo(alvo.tipo) {
        leitor
            .por_ciclo(alvo, Fonte::Inventario, 6)
            .ok()
            .filter(|t| !t.is_empty())
    } else {
        None
    };
    if !valores.is_empty() || totais.is_some() {
        ui.horizontal_wrapped(|ui| {
            for (i, c) in valores.iter().take(4).enumerate() {
                cartao_de_valor(ui, c, i == 0, pedidos);
            }
            if let Some(t) = &totais {
                grafico_por_ciclo(ui, t, pedidos);
            }
        });
        ui.add_space(8.0);
    }
    let campos: Vec<_> = valores.iter().skip(4).copied().chain(campos).collect();
    let largura = ui.available_width();
    let por_linha = ((largura / 190.0).floor() as usize).clamp(1, 5);
    egui::Grid::new(("campos", alvo))
        .num_columns(por_linha)
        .spacing(vec2(24.0, 8.0))
        .min_col_width((largura / por_linha as f32 - 24.0).max(80.0))
        .show(ui, |ui| {
            let largura_da_celula = (largura / por_linha as f32 - 24.0).max(80.0);
            for (i, c) in campos.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.set_max_width(largura_da_celula);
                    componentes::rotulo_de_secao(ui, &c.rotulo);
                    ui.horizontal(|ui| {
                        componentes::celula(ui, &c.valor, pedidos);
                        if let Celula::Valor {
                            fonte: Fonte::Inventario,
                            ..
                        } = c.valor
                        {
                            ui.label(
                                RichText::new(format!("· {A_CONFERIR}"))
                                    .size(12.0)
                                    .color(p.tinta_suave),
                            );
                        }
                    });
                });
                if (i + 1) % por_linha == 0 {
                    ui.end_row();
                }
            }
        });

    // Abas.
    ui.add_space(10.0);
    let abas = abas_de(alvo.tipo);
    let escolhida = vista
        .abas
        .get(alvo)
        .copied()
        .unwrap_or_else(|| abas.iter().position(|a| a.motivo.is_none()).unwrap_or(0));
    let escolhida = escolhida.min(abas.len().saturating_sub(1));
    ui.horizontal(|ui| {
        for (i, a) in abas.iter().enumerate() {
            let ativa = i == escolhida;
            let cor = if a.motivo.is_some() {
                p.tinta_desabilitada
            } else if ativa {
                p.tinta
            } else {
                p.tinta_suave
            };
            let mut texto = RichText::new(&a.rotulo).color(cor);
            if ativa {
                texto = texto.strong();
            }
            let icone = if a.conteudo == ConteudoDaAba::Rastro {
                Some("account_tree")
            } else {
                None
            };
            let r = ui.add_enabled(
                a.motivo.is_none(),
                egui::Button::new(match icone {
                    Some(n) => (icones::icone_cor(n, 15.0, cor), texto).into_atoms(),
                    None => texto.into_atoms(),
                })
                .frame(false),
            );
            if ativa {
                ui.painter().hline(
                    r.rect.x_range(),
                    r.rect.bottom() + 2.0,
                    Stroke::new(2.0, p.destaque),
                );
            }
            let r = match &a.motivo {
                Some(m) => r.on_disabled_hover_text(m.as_str()),
                None => r,
            };
            if r.clicked() {
                vista.abas.insert(alvo.clone(), i);
            }
        }
    });
    let linha = ui.available_rect_before_wrap();
    ui.painter().hline(
        linha.x_range(),
        linha.top() + 2.0,
        Stroke::new(1.0, p.borda),
    );
    ui.add_space(8.0);

    let Some(a) = abas.get(escolhida) else { return };
    if let Some(m) = &a.motivo {
        componentes::aviso(ui, "info", m, p.info_sutil, p.info_borda, p.info);
        return;
    }
    match &a.conteudo {
        ConteudoDaAba::Rastro => match leitor.rastro(alvo) {
            Ok(r) => desenhar_rastro(ui, alvo, &item.rotulo, &r, pedidos),
            Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
        },
        ConteudoDaAba::Texto => match leitor.texto(alvo) {
            Ok(t) => texto_extraido(ui, t.as_deref()),
            Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
        },
        ConteudoDaAba::Ligados(tipo) => {
            let pedido = PedidoDeTabela::Ligados {
                de: alvo.clone(),
                tipo: *tipo,
            };
            match leitor.tabela(&pedido) {
                Ok(t) => {
                    let chave = format!("ligados:{alvo}:{tipo}");
                    let mut estado = vista.tabelas.remove(&chave).unwrap_or_default();
                    let altura = ui.available_height().max(200.0);
                    tabela::desenhar(
                        ui,
                        &chave,
                        &t,
                        &mut estado,
                        &mut vista.selecao,
                        pedidos,
                        Some(altura),
                    );
                    vista.tabelas.insert(chave, estado);
                }
                Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
            }
        }
    }
}

/// Um cartão com um valor do cabeçalho; o primeiro da página vem em destaque (hero).
fn cartao_de_valor(
    ui: &mut Ui,
    c: &sbf_inventario::consulta::Campo,
    destaque: bool,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let Celula::Valor {
        centavos,
        fonte,
        itens,
    } = &c.valor
    else {
        return;
    };
    let (fundo, borda, cor, sub) = if destaque {
        (p.hero_fundo, p.hero_fundo, p.hero_texto, p.hero_sub)
    } else {
        (p.superficie_1, p.borda, p.tinta, p.tinta_suave)
    };
    Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_size(vec2(170.0, 74.0));
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(c.rotulo.to_uppercase())
                        .font(FontId::new(11.5, familia_forte()))
                        .color(sub),
                );
                let texto = formato::dinheiro(*centavos);
                componentes::total_rico(
                    ui,
                    RichText::new(&texto)
                        .font(FontId::new(21.0, familia_forte()))
                        .color(cor),
                    &texto,
                    itens.as_deref(),
                    &format!("{} · {}", c.rotulo, texto),
                    pedidos,
                    sub,
                );
                if *fonte == Fonte::Inventario {
                    ui.label(RichText::new(A_CONFERIR).size(12.0).color(sub));
                }
            });
        });
}

/// O texto extraído, em fonte mono, rolável.
pub fn texto_extraido(ui: &mut Ui, texto: Option<&str>) {
    let p = tema::paleta(ui.ctx());
    match texto {
        Some(t) => {
            egui::ScrollArea::vertical()
                .id_salt("texto-extraido")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    Frame::new()
                        .fill(p.superficie_2)
                        .stroke(Stroke::new(1.0, p.borda))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(t).monospace().size(12.5)).wrap(),
                            );
                        });
                });
        }
        None => {
            ui.label(RichText::new("Sem texto extraído para este achado.").color(p.tinta_suave));
        }
    }
}

/// As barras dos últimos ciclos, cada uma clicável (abre o que soma o ciclo).
pub fn grafico_por_ciclo(ui: &mut Ui, totais: &[TotalDoCiclo], pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                componentes::rotulo_de_secao(ui, "Total por ciclo");
                ui.label(
                    RichText::new(format!(
                        "últimos {} ciclos · inventário · {A_CONFERIR}",
                        totais.len()
                    ))
                    .size(12.0)
                    .color(p.tinta_suave),
                );
            });
            ui.add_space(4.0);
            let maximo = totais
                .iter()
                .map(|t| t.centavos.unsigned_abs())
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            let largura_barra = 54.0;
            ui.horizontal(|ui| {
                for (i, t) in totais.iter().enumerate() {
                    let ultimo = i + 1 == totais.len();
                    ui.vertical(|ui| {
                        ui.set_width(largura_barra);
                        ui.add(
                            egui::Label::new(
                                RichText::new(formato::dinheiro(t.centavos).replace("R$ ", ""))
                                    .size(11.0)
                                    .color(p.tinta_suave),
                            )
                            .extend(),
                        );
                        let (rect, r) = ui
                            .allocate_exact_size(vec2(largura_barra - 12.0, 44.0), Sense::click());
                        let h = (t.centavos.unsigned_abs() as f32 / maximo * 40.0).max(2.0);
                        let barra = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom() - h),
                            rect.max,
                        );
                        let cor = if ultimo {
                            p.destaque
                        } else if r.hovered() {
                            p.destaque_borda
                        } else {
                            p.destaque_sutil
                        };
                        ui.painter().rect_filled(barra, CornerRadius::same(2), cor);
                        let mut texto = RichText::new(&t.ciclo).size(11.0).color(p.tinta_suave);
                        if ultimo {
                            texto = texto.strong().color(p.tinta);
                        }
                        ui.label(texto);
                        if let Some(itens) = &t.itens {
                            let r = r
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(format!(
                                    "Ciclo {}: {} · {A_CONFERIR}\nClique abre os registros",
                                    t.ciclo,
                                    formato::dinheiro(t.centavos)
                                ));
                            if r.clicked() {
                                let flutuante = componentes::ctrl(ui);
                                crate::navegacao::pedir_abrir(
                                    ui.ctx(),
                                    pedidos,
                                    Destino::Total {
                                        titulo: format!(
                                            "Ciclo {} · {}",
                                            t.ciclo,
                                            formato::dinheiro(t.centavos)
                                        ),
                                        pedido: (**itens).clone(),
                                    },
                                    flutuante,
                                );
                            }
                        }
                    });
                }
            });
        });
    });
}

fn desenhar_elo(ui: &mut Ui, e: &sbf_inventario::consulta::Elo, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor(e.alvo.tipo.icone(), 18.0, p.tinta_suave));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(e.alvo.tipo.singular())
                        .size(12.0)
                        .color(p.tinta_suave),
                );
                componentes::link_item(ui, &e.alvo, &e.rotulo, pedidos);
            });
            let mut meta = e.papel.clone();
            if let Some(q) = &e.quem {
                meta.push_str(&format!(" · {}", q.rotulo()));
            }
            if let Some(q) = &e.quando {
                meta.push_str(&format!(" · {}", formato::data(q)));
            }
            ui.label(RichText::new(meta).size(12.0).color(p.tinta_suave));
        });
    });
}

/// A aba Rastro: a cadeia vertical, de onde veio até no que virou.
pub fn desenhar_rastro(
    ui: &mut Ui,
    alvo: &Ref,
    rotulo: &str,
    rastro: &Rastro,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Cada elo é um link e diz quem decidiu e quando.").color(p.tinta_suave),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button((
                    icones::icone_cor("content_copy", 16.0, p.tinta),
                    "Copiar rastro",
                ))
                .clicked()
            {
                pedidos.push(Pedido::Copiar(texto_do_rastro(alvo, rotulo, rastro)));
            }
        });
    });
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt(("rastro", alvo))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let trilho = |ui: &mut Ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 14.0), Sense::hover());
                ui.painter()
                    .vline(rect.center().x, rect.y_range(), Stroke::new(2.0, p.borda));
            };
            componentes::rotulo_de_secao(ui, "De onde veio");
            if rastro.acima.is_empty() {
                ui.label(RichText::new("Nada acima: este item é a origem.").color(p.tinta_suave));
            }
            for e in rastro.acima.iter().rev() {
                desenhar_elo(ui, e, pedidos);
                trilho(ui);
            }
            Frame::new()
                .fill(p.destaque_sutil)
                .stroke(Stroke::new(1.0, p.destaque_borda))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(icones::icone_cor(alvo.tipo.icone(), 18.0, p.destaque_tinta));
                        ui.label(
                            RichText::new(alvo.tipo.singular())
                                .size(12.0)
                                .color(p.tinta_suave),
                        );
                        ui.label(
                            RichText::new(rotulo)
                                .font(FontId::new(14.0, familia_forte()))
                                .color(p.tinta),
                        );
                        ui.label(RichText::new("este item").size(12.0).color(p.tinta_suave));
                    });
                });
            ui.add_space(4.0);
            componentes::rotulo_de_secao(ui, "No que virou");
            if rastro.abaixo.is_empty() {
                ui.label(
                    RichText::new("Ainda não virou nada: nenhum registro, lote ou movimentação.")
                        .color(p.tinta_suave),
                );
            }
            for (i, e) in rastro.abaixo.iter().enumerate() {
                if i > 0 {
                    trilho(ui);
                }
                desenhar_elo(ui, e, pedidos);
            }
            if rastro
                .abaixo
                .iter()
                .all(|e| e.alvo.tipo != Tipo::Movimentacao)
            {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    componentes::selo_fatia(ui, 2);
                    ui.label(
                        RichText::new(
                            "Movimentação, rateio e centros entram no rastro com o livro-caixa.",
                        )
                        .size(12.0)
                        .color(p.tinta_suave),
                    );
                });
            }
        });
}

/// A página de um total: a lista do que soma (ou conta) o número.
pub fn desenhar_total(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    titulo: &str,
    pedido: &PedidoDeTabela,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor("functions", 22.0, p.tinta));
        componentes::titulo(ui, titulo, 20.0);
    });
    if let PedidoDeTabela::ItensDoTotal {
        de, fonte, ciclo, ..
    } = pedido
    {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Itens que compõem o total de").color(p.tinta_suave));
            let rotulo = leitor.rotulo(de);
            componentes::link_item(ui, de, &rotulo, pedidos);
            if let Some(c) = ciclo {
                ui.label(RichText::new(format!("no ciclo {c}")).color(p.tinta_suave));
            }
            if *fonte == Fonte::Inventario {
                ui.label(
                    RichText::new(format!("· registros do inventário · {A_CONFERIR}"))
                        .color(p.tinta_suave),
                );
            }
        });
    }
    ui.add_space(8.0);
    match leitor.tabela(pedido) {
        Ok(t) => {
            let chave = format!(
                "total:{}",
                serde_json::to_string(pedido).unwrap_or_default()
            );
            let mut estado = vista.tabelas.remove(&chave).unwrap_or_default();
            tabela::desenhar(
                ui,
                &chave,
                &t,
                &mut estado,
                &mut vista.selecao,
                pedidos,
                None,
            );
            vista.tabelas.insert(chave, estado);
        }
        Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn toda_pagina_termina_no_rastro() {
        for t in Tipo::TODOS {
            let abas = abas_de(t);
            assert_eq!(
                abas.last().map(|a| &a.conteudo),
                Some(&ConteudoDaAba::Rastro)
            );
        }
    }

    #[test]
    fn fornecedor_tem_as_abas_da_spec_com_o_livro_caixa_desabilitado() {
        let abas = abas_de(Tipo::Fornecedor);
        let mov = abas
            .iter()
            .find(|a| a.rotulo == "Movimentações")
            .expect("aba");
        assert_eq!(
            mov.motivo.as_deref(),
            Some("Chega na fatia 2, com o livro-caixa")
        );
        let reg = abas
            .iter()
            .find(|a| a.rotulo.starts_with("Registros"))
            .expect("aba");
        assert_eq!(reg.rotulo, "Registros · a conferir");
        assert!(reg.motivo.is_none());
        assert!(
            abas.iter()
                .any(|a| a.rotulo == "Documentos" && a.motivo.is_none())
        );
        let comp = abas
            .iter()
            .find(|a| a.rotulo == "Compromissos e recorrentes")
            .expect("aba");
        assert!(comp.motivo.is_some());
    }

    #[test]
    fn rastro_vira_texto_com_cada_elo() {
        use sbf_inventario::consulta::{Elo, Quem};
        let alvo = Ref::nova(Tipo::Registro, "7");
        let r = Rastro {
            acima: vec![Elo {
                alvo: Ref::nova(Tipo::Documento, "abc"),
                rotulo: "conta.pdf".into(),
                papel: "extraído de".into(),
                quem: Some(Quem::Automatico("regras@2".into())),
                quando: Some("2026-10-02T21:41:00".into()),
            }],
            abaixo: vec![],
        };
        let t = texto_do_rastro(&alvo, "Conta de energia", &r);
        assert!(t.contains(
            "↑ Documento conta.pdf (documento:abc) · extraído de · regras@2 · 02/10/2026 21:41"
        ));
        assert!(t.contains("● Registro Conta de energia"));
    }
}
