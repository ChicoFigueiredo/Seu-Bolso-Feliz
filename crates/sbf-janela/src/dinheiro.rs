//! As áreas de Dinheiro e a área Sincronização (spec 07): os números do topo de cada área,
//! as seções, a faixa offline, a barra de status da sincronização, as páginas de Estação
//! e de Alteração e o componente **lado a lado** da alteração recusada.
//!
//! Lê o livro-caixa pela mesma `Consulta` da janela (`painel_da_area`, tabelas) e a
//! estação pelos traits de `sbf_inventario::alteracao`. Só leitura: as escritas (mudar
//! categoria, refazer, descartar, sincronizar, revogar, renomear) viram [`Pedido`]s, que
//! a janela aplica fora do quadro.

use egui::{Align, CornerRadius, FontId, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use sbf_inventario::alteracao::{
    Alteracao, Alteracoes, CampoLadoALado, Credencial, EstadoDaAlteracao, EstadoDaRede,
    EstadoDaSincronizacao, Recusa, Sincronizacao,
};
use sbf_inventario::consulta::{Area, Celula, Coluna, Indicador, Linha, PainelDeArea, Tabela};
use sbf_inventario::{Ref, Tipo};

use crate::componentes;
use crate::formato;
use crate::icones;
use crate::navegacao::{Destino, Pedido};
use crate::pagina::Vista;
use crate::tabela;
use crate::tema::{self, familia_forte};

/// A estação vista pela janela: as alterações e a sincronização. A estação real
/// (`sbf-estacao`) e o livro fictício implementam os dois traits.
pub trait EstacaoDoLivro: Alteracoes + Sincronizacao {}

impl<T: Alteracoes + Sincronizacao> EstacaoDoLivro for T {}

/// Em que pé está o pareamento desta estação (login Google e `POST /estacoes/pareamento`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AndamentoDoPareamento {
    /// Nada em andamento.
    #[default]
    Parado,
    /// Rodando; o texto diz o passo ("esperando o login no navegador"…).
    Pareando(String),
    /// O último pareamento não deu certo; o texto diz por quê (sem credencial nem token).
    Falhou(String),
}

/// O pareamento da estação, que roda fora do quadro. Quem liga a estação real (o binário)
/// implementa; a janela só dispara e mostra o andamento.
pub trait Pareamento: std::fmt::Debug {
    /// Começa o pareamento (não bloqueia). Sem efeito se já está pareando.
    fn iniciar(&mut self);
    /// O andamento, lido a cada quadro.
    fn andamento(&self) -> AndamentoDoPareamento;
}

/// A estação ligada à janela.
pub struct EstacaoLigada(pub Box<dyn EstacaoDoLivro>);

impl std::fmt::Debug for EstacaoLigada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstacaoLigada").finish_non_exhaustive()
    }
}

/// O motivo das Ações de edição do livro-caixa (spec 07: só mudar categoria nesta fatia).
pub const MOTIVO_FATIA_3: &str = "Chega na fatia 3, com a edição do livro-caixa";

// ---------------------------------------------------------------------------
// Hora e textos
// ---------------------------------------------------------------------------

/// Um instante ISO 8601 como `HH:MM` em São Paulo (UTC−3). Com `hoje` (`AAAA-MM-DD`)
/// diferente do dia do instante, vem com a data: `12/11 18:40`.
pub fn hora(instante: &str, hoje: &str) -> String {
    let (Some(dia), Some(h), Some(m)) = (
        instante.get(0..10),
        instante.get(11..13).and_then(|h| h.parse::<i64>().ok()),
        instante.get(14..16).and_then(|m| m.parse::<i64>().ok()),
    ) else {
        return instante.to_owned();
    };
    // O deslocamento: `Z`, `+hh:mm` ou `-hh:mm` depois dos segundos.
    let resto = instante.get(16..).unwrap_or("");
    let resto = resto.trim_start_matches(|c: char| c == ':' || c.is_ascii_digit() || c == '.');
    let desloc = if resto.starts_with('Z') {
        0
    } else if let (Some(sinal), Some(hh), Some(mm)) = (
        resto.chars().next(),
        resto.get(1..3).and_then(|x| x.parse::<i64>().ok()),
        resto.get(4..6).and_then(|x| x.parse::<i64>().ok()),
    ) {
        let v = hh * 60 + mm;
        if sinal == '-' { -v } else { v }
    } else {
        -180
    };
    // Em minutos UTC, depois em São Paulo.
    let local = h * 60 + m - desloc - 180;
    let dias = local.div_euclid(1440);
    let minutos = local.rem_euclid(1440);
    let texto = format!("{:02}:{:02}", minutos / 60, minutos % 60);
    let dia_local = if dias == 0 {
        dia.to_owned()
    } else {
        somar_dias(dia, dias)
    };
    if dia_local == hoje {
        texto
    } else {
        format!(
            "{}/{} {texto}",
            dia_local.get(8..10).unwrap_or(""),
            dia_local.get(5..7).unwrap_or("")
        )
    }
}

fn somar_dias(dia: &str, n: i64) -> String {
    let (Some(a), Some(m), Some(d)) = (
        dia.get(0..4).and_then(|x| x.parse::<i64>().ok()),
        dia.get(5..7).and_then(|x| x.parse::<i64>().ok()),
        dia.get(8..10).and_then(|x| x.parse::<i64>().ok()),
    ) else {
        return dia.to_owned();
    };
    let bissexto = |a: i64| (a % 4 == 0 && a % 100 != 0) || a % 400 == 0;
    let dias_no_mes = |a: i64, m: i64| match m {
        2 if bissexto(a) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let (mut a, mut m, mut d) = (a, m, d + n);
    while d < 1 {
        m -= 1;
        if m < 1 {
            m = 12;
            a -= 1;
        }
        d += dias_no_mes(a, m);
    }
    while d > dias_no_mes(a, m) {
        d -= dias_no_mes(a, m);
        m += 1;
        if m > 12 {
            m = 1;
            a += 1;
        }
    }
    format!("{a:04}-{m:02}-{d:02}")
}

/// O que a barra de status diz da sincronização: texto, ícone e se é aviso.
pub fn texto_da_barra(estado: &EstadoDaSincronizacao, hoje: &str) -> (String, &'static str, bool) {
    let copia = estado
        .copia_de
        .as_deref()
        .map(|c| hora(c, hoje))
        .unwrap_or_else(|| "—".to_owned());
    match &estado.rede {
        EstadoDaRede::Sincronizado => (
            match &estado.ultima_troca {
                Some(t) => format!("Sincronizado às {}", hora(t, hoje)),
                None => "Sincronizado".to_owned(),
            },
            "cloud_done",
            false,
        ),
        EstadoDaRede::Sincronizando => ("Sincronizando…".to_owned(), "cloud_sync", false),
        EstadoDaRede::SemRede => (
            format!("Sem rede, mostrando a cópia de {copia}"),
            "cloud_off",
            true,
        ),
        EstadoDaRede::EstacaoRevogada => ("Estação revogada".to_owned(), "key_off", true),
        EstadoDaRede::RegrasMaisNovas { .. } => {
            ("Servidor com regras mais novas".to_owned(), "warning", true)
        }
        EstadoDaRede::AtualizeOApp => (
            "O servidor pede uma versão nova: atualize o app".to_owned(),
            "warning",
            true,
        ),
        EstadoDaRede::NaoPareada => ("Estação não pareada".to_owned(), "cloud_off", true),
        EstadoDaRede::Falhou(_) => (
            "A última sincronização falhou".to_owned(),
            "sync_problem",
            true,
        ),
    }
}

/// A explicação longa do estado (tooltip da barra, cabeçalho da Sincronização).
pub fn explicacao(estado: &EstadoDaSincronizacao) -> Option<String> {
    match &estado.rede {
        EstadoDaRede::EstacaoRevogada => Some(
            "Esta estação foi revogada: a cópia do livro-caixa e a credencial foram apagadas. O cofre e o inventário continuam aqui. Pareie de novo para voltar a sincronizar."
                .to_owned(),
        ),
        EstadoDaRede::RegrasMaisNovas { versao_do_servidor } => Some(format!(
            "O servidor roda as regras {versao_do_servidor}; esta estação, {}. Os números continuam valendo; atualize o app quando puder.",
            estado.versao_das_regras
        )),
        EstadoDaRede::AtualizeOApp => Some(
            "O servidor não aceita mais o contrato desta versão. A cópia continua legível, mas nada sincroniza até você atualizar o app."
                .to_owned(),
        ),
        EstadoDaRede::NaoPareada => Some(
            "Faça o login Google para esta máquina virar uma estação e receber o livro-caixa."
                .to_owned(),
        ),
        EstadoDaRede::Falhou(e) => Some(e.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Faixa offline e números do topo
// ---------------------------------------------------------------------------

/// A faixa "Sem rede, mostrando a cópia de HH:MM" (grill Q13) e os avisos da estação, no
/// topo das áreas de Dinheiro.
pub fn faixa_da_estacao(ui: &mut Ui, estado: Option<&EstadoDaSincronizacao>, hoje: &str) {
    let p = tema::paleta(ui.ctx());
    let Some(e) = estado else {
        return;
    };
    let (texto, icone, aviso) = texto_da_barra(e, hoje);
    if !aviso {
        return;
    }
    let mut t = texto;
    if let Some(x) = explicacao(e) {
        t.push_str(". ");
        t.push_str(&x);
    }
    componentes::aviso(ui, icone, &t, p.aviso_sutil, p.aviso_borda, p.aviso);
    ui.add_space(6.0);
}

/// Um número de destaque (o primeiro de cada área vem em destaque, como no protótipo).
fn cartao_do_indicador(
    ui: &mut Ui,
    i: &Indicador,
    destaque: bool,
    hora_da_copia: Option<&str>,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
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
            ui.set_min_size(vec2(200.0, 82.0));
            ui.set_max_width(260.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(i.rotulo.to_uppercase())
                            .font(FontId::new(11.5, familia_forte()))
                            .color(sub),
                    );
                    if i.estimativa.is_some() {
                        componentes::selo(ui, "estimativa", p.aviso_sutil, p.aviso);
                    }
                });
                match &i.valor {
                    Celula::Valor {
                        centavos, itens, ..
                    } => {
                        let texto = formato::dinheiro(*centavos);
                        componentes::total_rico(
                            ui,
                            RichText::new(&texto)
                                .font(FontId::new(22.0, familia_forte()))
                                .color(cor),
                            &texto,
                            itens.as_deref(),
                            &format!("{} · {texto} · livro-caixa", i.rotulo),
                            pedidos,
                            sub,
                        );
                    }
                    outra => componentes::celula(ui, outra, pedidos),
                }
                if let (true, Some(h)) = (i.instantaneo, hora_da_copia) {
                    ui.label(
                        RichText::new(format!("cópia de {h} · sem rede"))
                            .size(12.0)
                            .color(p.aviso)
                            .strong(),
                    );
                }
                if let Some(n) = &i.nota {
                    ui.add(egui::Label::new(RichText::new(n).size(12.0).color(sub)).wrap());
                }
                if let Some(e) = &i.estimativa {
                    ui.add(
                        egui::Label::new(RichText::new(e).size(12.0).color(if destaque {
                            sub
                        } else {
                            p.aviso
                        }))
                        .wrap(),
                    );
                }
            });
        });
}

/// A hora da cópia para pôr ao lado dos números "neste instante", quando offline.
pub fn hora_da_copia(sinc: Option<&EstadoDaSincronizacao>, hoje: &str) -> Option<String> {
    sinc.filter(|s| s.offline())
        .and_then(|s| s.copia_de.as_deref())
        .map(|c| hora(c, hoje))
}

/// Os números do topo e os avisos do painel de uma área de Dinheiro.
pub fn indicadores(
    ui: &mut Ui,
    painel: &PainelDeArea,
    hora_da_copia: Option<&str>,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    if !painel.indicadores.is_empty() {
        // Quebra em linhas pela largura: um cartão tem até 260 px.
        let por_linha = ((ui.available_width() / 272.0).floor() as usize).max(1);
        for (linha, cartoes) in painel.indicadores.chunks(por_linha).enumerate() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for (n, i) in cartoes.iter().enumerate() {
                    cartao_do_indicador(ui, i, linha == 0 && n == 0, hora_da_copia, pedidos);
                }
            });
            ui.add_space(6.0);
        }
        ui.add_space(8.0);
    }
    for a in &painel.avisos {
        componentes::aviso(ui, "info", a, p.info_sutil, p.info_borda, p.info);
        ui.add_space(4.0);
    }
}

/// As seções do painel de uma área de Dinheiro (Para resolver, Cartões, Não alocado…).
pub fn secoes(
    ui: &mut Ui,
    painel: &PainelDeArea,
    area: Area,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let chave = format!("painel:{area:?}");
    for (i, s) in painel.secoes.iter().enumerate() {
        ui.add_space(6.0);
        componentes::cartao(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                componentes::titulo(ui, &s.titulo, 15.0);
                if let Some(n) = &s.nota {
                    ui.label(RichText::new(n).size(12.0).color(p.tinta_suave));
                }
            });
            ui.add_space(4.0);
            desenhar_tabela(
                ui,
                &format!("{chave}:{i}"),
                s.tabela.clone(),
                vista,
                pedidos,
                300.0,
            );
        });
    }
}

/// Um erro de leitura: `NaoSuportado` (livro-caixa não ligado, fatia futura) vira aviso
/// com o motivo; o resto, erro.
pub fn motivo_ou_erro(ui: &mut Ui, e: &str) {
    let p = tema::paleta(ui.ctx());
    match e.strip_prefix("ainda não suportado: ") {
        Some(motivo) => componentes::aviso(ui, "info", motivo, p.info_sutil, p.info_borda, p.info),
        None => componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo),
    }
}

// ---------------------------------------------------------------------------
// Alterações
// ---------------------------------------------------------------------------

fn col(r: &str) -> Coluna {
    Coluna {
        rotulo: r.to_owned(),
        numerica: false,
    }
}

/// A tabela das alterações (Aguardando, aba Alterações).
pub fn tabela_de_alteracoes(lista: &[Alteracao], hoje: &str) -> Tabela {
    Tabela {
        colunas: vec![
            col("Alteração"),
            col("Feita"),
            col("Item"),
            col("Antes"),
            col("Depois"),
            col("Estado"),
        ],
        linhas: lista
            .iter()
            .map(|a| Linha {
                alvo: Some(a.alvo_da_pagina()),
                celulas: vec![
                    Celula::Link {
                        alvo: a.alvo_da_pagina(),
                        texto: a.pedido.o_que_muda().to_owned(),
                    },
                    Celula::Texto(hora(&a.feita_em, hoje)),
                    Celula::Link {
                        alvo: a.pedido.alvo().clone(),
                        texto: a.rotulo_do_alvo.clone(),
                    },
                    Celula::Texto(a.antes.clone()),
                    Celula::Texto(a.depois.clone()),
                    Celula::Estado {
                        rotulo: a.estado.rotulo().to_owned(),
                        quem: None,
                    },
                ],
            })
            .collect(),
    }
}

fn desenhar_tabela(
    ui: &mut Ui,
    id: &str,
    t: Tabela,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
    maximo: f32,
) {
    let n = t.linhas.len();
    let t = std::sync::Arc::new(t);
    let mut estado = vista.tabelas.remove(id).unwrap_or_default();
    let altura = tabela::altura_para(ui, n, maximo);
    tabela::desenhar(
        ui,
        id,
        &t,
        &mut estado,
        &mut vista.selecao,
        pedidos,
        Some(altura),
    );
    vista.tabelas.insert(id.to_owned(), estado);
}

/// O componente **lado a lado** de uma recusa: "Sua versão" × "No servidor agora", com o
/// que difere marcado (cor e ícone, nunca só cor), e Refazer sobre a nova / Descartar.
pub fn lado_a_lado(ui: &mut Ui, a: &Alteracao, recusa: &Recusa, pedidos: &mut Vec<Pedido>) {
    lado_a_lado_de_campos(
        ui,
        &a.id,
        &recusa.motivo,
        &recusa.campos,
        Pedido::RefazerAlteracao(a.id.clone()),
        Pedido::DescartarAlteracao(a.id.clone()),
        pedidos,
    );
}

/// O mesmo componente, para qualquer coisa que tenha "sua versão" e "no servidor agora"
/// (a caixa de entrada e a edição usam). `chave` identifica a grade; `refazer` e
/// `descartar` são os pedidos dos dois botões.
pub fn lado_a_lado_de_campos(
    ui: &mut Ui,
    chave: &str,
    motivo: &str,
    campos: &[CampoLadoALado],
    refazer: Pedido,
    descartar: Pedido,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new(motivo).color(p.tinta_suave));
    ui.add_space(6.0);
    let largura = ((ui.available_width() - 12.0) / 2.0).max(160.0);
    ui.horizontal_top(|ui| {
        for (titulo, servidor) in [("Sua versão", false), ("No servidor agora", true)] {
            Frame::new()
                .fill(if servidor {
                    p.superficie_1
                } else {
                    p.destaque_sutil
                })
                .stroke(Stroke::new(
                    1.0,
                    if servidor { p.borda } else { p.destaque_borda },
                ))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(largura - 22.0);
                        ui.label(
                            RichText::new(titulo.to_uppercase())
                                .font(FontId::new(11.5, familia_forte()))
                                .color(p.tinta_suave),
                        );
                        ui.add_space(4.0);
                        egui::Grid::new(("lado", chave, servidor))
                            .num_columns(2)
                            .spacing(vec2(12.0, 4.0))
                            .show(ui, |ui| {
                                for c in campos {
                                    ui.label(RichText::new(&c.rotulo).color(p.tinta_suave));
                                    let v = if servidor { &c.servidor } else { &c.sua };
                                    if c.difere() {
                                        ui.horizontal(|ui| {
                                            ui.label(icones::icone_cor(
                                                "difference",
                                                14.0,
                                                p.aviso,
                                            ));
                                            ui.label(RichText::new(v).color(p.aviso).strong());
                                        })
                                        .response
                                        .on_hover_text("difere entre as duas versões");
                                    } else {
                                        ui.label(RichText::new(v).color(p.tinta));
                                    }
                                    ui.end_row();
                                }
                            });
                    })
                });
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new((
                    icones::icone_cor("redo", 16.0, p.sobre_destaque),
                    RichText::new("Refazer sobre a nova").color(p.sobre_destaque),
                ))
                .fill(p.destaque),
            )
            .on_hover_text("Aplica a sua mudança sobre o que está no servidor agora")
            .clicked()
        {
            pedidos.push(refazer.clone());
        }
        if ui
            .button((
                icones::icone_cor("delete", 16.0, p.tinta),
                RichText::new("Descartar"),
            ))
            .on_hover_text("Mantém a versão do servidor")
            .clicked()
        {
            pedidos.push(descartar.clone());
        }
    });
    ui.label(
        RichText::new(
            "Refazer aplica a sua mudança sobre o que está no servidor agora. Descartar mantém a versão do servidor.",
        )
        .size(12.0)
        .color(p.tinta_suave),
    );
}

/// Um cartão de alteração recusada, como na área Sincronização.
pub fn cartao_da_recusa(ui: &mut Ui, a: &Alteracao, hoje: &str, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    let Some(recusa) = &a.recusa else {
        return;
    };
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("block", 16.0, p.perigo));
            componentes::link_item(ui, &a.alvo_da_pagina(), a.pedido.o_que_muda(), pedidos);
            ui.label("·");
            componentes::link_item(ui, a.pedido.alvo(), &a.rotulo_do_alvo, pedidos);
            ui.label(
                RichText::new(format!(
                    "· {} → {} · feita {}",
                    a.antes,
                    a.depois,
                    hora(&a.feita_em, hoje)
                ))
                .color(p.tinta_suave),
            );
        });
        ui.add_space(4.0);
        lado_a_lado(ui, a, recusa, pedidos);
    });
}

// ---------------------------------------------------------------------------
// A área Sincronização
// ---------------------------------------------------------------------------

/// "Parear esta estação", quando a estação não está pareada e há como parear; com o
/// andamento (ou o motivo da falha) ao lado.
fn botao_de_parear(
    ui: &mut Ui,
    pareamento: Option<&dyn Pareamento>,
    precisa: bool,
    pedidos: &mut Vec<Pedido>,
) {
    let (true, Some(par)) = (precisa, pareamento) else {
        return;
    };
    let p = tema::paleta(ui.ctx());
    let andamento = par.andamento();
    let ocupado = matches!(andamento, AndamentoDoPareamento::Pareando(_));
    if ui
        .add_enabled(
            !ocupado,
            egui::Button::new((
                icones::icone_cor("key", 16.0, p.sobre_destaque),
                RichText::new("Parear esta estação").color(p.sobre_destaque),
            ))
            .fill(p.destaque),
        )
        .clicked()
    {
        pedidos.push(Pedido::Parear);
    }
    match andamento {
        AndamentoDoPareamento::Parado => {}
        AndamentoDoPareamento::Pareando(passo) => {
            ui.add(egui::Spinner::new());
            ui.label(RichText::new(passo).color(p.tinta_suave));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        AndamentoDoPareamento::Falhou(motivo) => {
            ui.label(RichText::new(motivo).color(p.perigo));
        }
    }
}

/// A área Sincronização: cabeçalho, Sincronizar agora, Aguardando, Recusadas lado a lado,
/// Estações e o histórico de trocas.
pub fn area_sincronizacao(
    ui: &mut Ui,
    estacao: Option<&dyn EstacaoDoLivro>,
    pareamento: Option<&dyn Pareamento>,
    caixa: Option<&dyn crate::caixa::FonteDaCaixa>,
    hoje: &str,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let Some(est) = estacao else {
        componentes::aviso(
            ui,
            "cloud_off",
            "Esta máquina ainda não é uma estação: o pareamento com o servidor (login Google) acontece aqui quando a estação estiver ligada a esta janela. O inventário continua funcionando sem ela.",
            p.info_sutil,
            p.info_borda,
            p.info,
        );
        botao_de_parear(ui, pareamento, true, pedidos);
        return;
    };
    let estado = est.estado();
    // Cabeçalho.
    ui.horizontal_wrapped(|ui| {
        match &estado.esta_estacao {
            Some(e) => {
                componentes::link_item(ui, &e.alvo, &format!("Estação {}", e.nome), pedidos);
            }
            None => {
                ui.label("Estação não pareada");
            }
        }
        let ultima = estado
            .ultima_troca
            .as_deref()
            .map(|t| hora(t, hoje))
            .unwrap_or_else(|| "nunca".to_owned());
        ui.label(
            RichText::new(format!(
                "· última sincronização {ultima} · {} · O inventário não sincroniza.",
                estado.servidor
            ))
            .color(p.tinta_suave),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let precisa_parear = matches!(
                estado.rede,
                EstadoDaRede::NaoPareada | EstadoDaRede::EstacaoRevogada
            );
            botao_de_parear(ui, pareamento, precisa_parear, pedidos);
            let pode = !matches!(
                estado.rede,
                EstadoDaRede::EstacaoRevogada
                    | EstadoDaRede::NaoPareada
                    | EstadoDaRede::AtualizeOApp
            );
            if ui
                .add_enabled(
                    pode,
                    egui::Button::new((
                        icones::icone_cor("cloud_sync", 16.0, p.sobre_destaque),
                        RichText::new("Sincronizar agora").color(p.sobre_destaque),
                    ))
                    .fill(p.destaque),
                )
                .on_disabled_hover_text(explicacao(&estado).unwrap_or_default())
                .clicked()
            {
                pedidos.push(Pedido::Sincronizar);
            }
        });
    });
    ui.add_space(6.0);
    faixa_da_estacao(ui, Some(&estado), hoje);

    let todas = est.alteracoes(false).unwrap_or_default();
    let aguardando: Vec<Alteracao> = todas
        .iter()
        .filter(|a| a.estado == EstadoDaAlteracao::Aguardando)
        .cloned()
        .collect();
    let recusadas: Vec<&Alteracao> = todas
        .iter()
        .filter(|a| a.estado == EstadoDaAlteracao::Recusada)
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("sincronizacao")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(c) = caixa {
                secao(
                    ui,
                    "Caixa de entrada e edição",
                    "aprovações, correções e edições do livro-caixa",
                    |ui| crate::caixa_ui::secoes_da_sincronizacao(ui, c, hoje, pedidos),
                );
            }
            secao(ui, "Aguardando o servidor", &aguardando.len().to_string(), |ui| {
                if aguardando.is_empty() {
                    ui.label(
                        RichText::new("Nada esperando. Tudo o que você alterou já foi aceito.")
                            .color(p.tinta_suave),
                    );
                } else {
                    ui.label(
                        RichText::new("Vão na próxima sincronização. O servidor aplica inteira ou recusa, nunca pela metade.")
                            .size(12.0)
                            .color(p.tinta_suave),
                    );
                    desenhar_tabela(
                        ui,
                        "sinc:aguardando",
                        tabela_de_alteracoes(&aguardando, hoje),
                        vista,
                        pedidos,
                        200.0,
                    );
                }
            });
            secao(
                ui,
                "Recusadas",
                &format!(
                    "{} · o servidor não aplicou e espera você decidir",
                    recusadas.len()
                ),
                |ui| {
                    if recusadas.is_empty() {
                        ui.label(RichText::new("Nenhuma alteração recusada.").color(p.tinta_suave));
                    }
                    for a in &recusadas {
                        cartao_da_recusa(ui, a, hoje, pedidos);
                        ui.add_space(6.0);
                    }
                },
            );
            let estacoes = est.estacoes().unwrap_or_default();
            secao(
                ui,
                "Estações",
                "revogar não apaga o que já foi sincronizado no servidor",
                |ui| {
                    let t = Tabela {
                        colunas: vec![
                            col("Estação"),
                            col("Sistema"),
                            col("Última sincronização"),
                            col("Credencial"),
                        ],
                        linhas: estacoes
                            .iter()
                            .map(|e| Linha {
                                alvo: Some(e.alvo.clone()),
                                celulas: vec![
                                    Celula::Link {
                                        alvo: e.alvo.clone(),
                                        texto: if e.esta {
                                            format!("{} (esta)", e.nome)
                                        } else {
                                            e.nome.clone()
                                        },
                                    },
                                    Celula::Texto(e.sistema.clone()),
                                    e.ultima_sincronizacao
                                        .as_deref()
                                        .map_or(Celula::Vazia, |t| Celula::Texto(hora(t, hoje))),
                                    Celula::Estado {
                                        rotulo: match e.credencial {
                                            Credencial::Ativa => "Ativa".to_owned(),
                                            Credencial::Revogada => "Revogada".to_owned(),
                                        },
                                        quem: None,
                                    },
                                ],
                            })
                            .collect(),
                    };
                    desenhar_tabela(ui, "sinc:estacoes", t, vista, pedidos, 200.0);
                    ui.label(
                        RichText::new("Selecione uma estação para Revogar ou Renomear (aba Livro-caixa do Ribbon).")
                            .size(12.0)
                            .color(p.tinta_suave),
                    );
                },
            );
            let trocas = est.historico().unwrap_or_default();
            secao(ui, "Histórico de trocas", "fica nesta estação", |ui| {
                desenhar_tabela(
                    ui,
                    "sinc:trocas",
                    tabela_de_trocas(&trocas, hoje),
                    vista,
                    pedidos,
                    220.0,
                );
            });
        });
}

fn tabela_de_trocas(trocas: &[sbf_inventario::alteracao::Troca], hoje: &str) -> Tabela {
    Tabela {
        colunas: vec![
            col("Início"),
            col("Fim"),
            col("Coleções trocadas"),
            Coluna {
                rotulo: "Linhas".to_owned(),
                numerica: true,
            },
            col("Alterações"),
            col("Resultado"),
        ],
        linhas: trocas
            .iter()
            .map(|t| Linha {
                alvo: None,
                celulas: vec![
                    Celula::Texto(hora(&t.inicio, hoje)),
                    t.fim
                        .as_deref()
                        .map_or(Celula::Vazia, |f| Celula::Texto(hora(f, hoje))),
                    Celula::Texto(if t.colecoes.is_empty() {
                        "nenhuma mudou".to_owned()
                    } else {
                        t.colecoes.join(", ")
                    }),
                    Celula::Contagem {
                        n: t.linhas,
                        itens: None,
                    },
                    Celula::Texto(format!(
                        "{} enviadas · {} aceitas · {} recusadas",
                        t.enviadas, t.aceitas, t.recusadas
                    )),
                    match &t.erro {
                        Some(e) => Celula::Estado {
                            rotulo: format!("Falhou: {e}"),
                            quem: None,
                        },
                        None => Celula::Estado {
                            rotulo: "Concluída".to_owned(),
                            quem: None,
                        },
                    },
                ],
            })
            .collect(),
    }
}

fn secao(ui: &mut Ui, titulo: &str, sub: &str, conteudo: impl FnOnce(&mut Ui)) {
    let p = tema::paleta(ui.ctx());
    ui.add_space(6.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            componentes::titulo(ui, titulo, 15.0);
            ui.label(RichText::new(sub).size(12.0).color(p.tinta_suave));
        });
        ui.add_space(4.0);
        conteudo(ui);
    });
}

// ---------------------------------------------------------------------------
// Páginas de Estação e de Alteração
// ---------------------------------------------------------------------------

fn kpi(ui: &mut Ui, rotulo: &str, valor: &str, sub: &str) {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(p.superficie_1)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(rotulo.to_uppercase())
                        .font(FontId::new(11.0, familia_forte()))
                        .color(p.tinta_suave),
                );
                ui.label(RichText::new(valor).size(16.0).color(p.tinta).strong());
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(12.0).color(p.tinta_suave));
                }
            });
        });
}

/// As abas das páginas de Estação e de Alteração.
pub fn abas(ui: &mut Ui, alvo: &Ref, rotulos: &[(&str, Option<&str>)], vista: &mut Vista) -> usize {
    let p = tema::paleta(ui.ctx());
    let escolhida = vista
        .abas
        .get(alvo)
        .copied()
        .unwrap_or(0)
        .min(rotulos.len().saturating_sub(1));
    ui.horizontal(|ui| {
        for (i, (rotulo, motivo)) in rotulos.iter().enumerate() {
            let ativa = i == escolhida;
            let cor = if motivo.is_some() {
                p.tinta_desabilitada
            } else if ativa {
                p.tinta
            } else {
                p.tinta_suave
            };
            let mut t = RichText::new(*rotulo).color(cor);
            if ativa {
                t = t.strong();
            }
            let r = ui.add_enabled(motivo.is_none(), egui::Button::new(t).frame(false));
            if ativa {
                ui.painter().hline(
                    r.rect.x_range(),
                    r.rect.bottom() + 2.0,
                    Stroke::new(2.0, p.destaque),
                );
            }
            let r = match motivo {
                Some(m) => r.on_disabled_hover_text(*m),
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
    escolhida
}

/// A página de uma estação: abas Sincronização, Alterações e Conexões (fatia 4).
pub fn pagina_da_estacao(
    ui: &mut Ui,
    est: &dyn EstacaoDoLivro,
    alvo: &Ref,
    hoje: &str,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let estacoes = est.estacoes().unwrap_or_default();
    let Some(e) = estacoes.iter().find(|e| &e.alvo == alvo) else {
        componentes::aviso(
            ui,
            "help",
            "Estação desconhecida nesta cópia.",
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        return;
    };
    let alteracoes: Vec<Alteracao> = est
        .alteracoes(false)
        .unwrap_or_default()
        .into_iter()
        .filter(|a| &a.estacao == alvo)
        .collect();
    let ag = alteracoes
        .iter()
        .filter(|a| a.estado == EstadoDaAlteracao::Aguardando)
        .count();
    let rec = alteracoes
        .iter()
        .filter(|a| a.estado == EstadoDaAlteracao::Recusada)
        .count();
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor("computer", 22.0, p.tinta));
        componentes::titulo(ui, &format!("Estação {}", e.nome), 20.0);
        match e.credencial {
            Credencial::Ativa => {
                componentes::selo(ui, "Credencial ativa", p.sucesso_sutil, p.sucesso)
            }
            Credencial::Revogada => {
                componentes::selo(ui, "Credencial revogada", p.superficie_3, p.tinta_suave)
            }
        };
        if e.esta {
            componentes::selo(ui, "esta máquina", p.info_sutil, p.info);
        }
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        kpi(ui, "Sistema", &e.sistema, "");
        kpi(
            ui,
            "Última sincronização",
            &e.ultima_sincronizacao
                .as_deref()
                .map(|t| hora(t, hoje))
                .unwrap_or_else(|| "—".to_owned()),
            "",
        );
        kpi(
            ui,
            "Aguardando",
            &ag.to_string(),
            "ainda não aceitas pelo servidor",
        );
        kpi(
            ui,
            "Recusadas",
            &rec.to_string(),
            if rec > 0 {
                "ficam visíveis até você resolver"
            } else {
                "nenhuma"
            },
        );
    });
    ui.label(
        RichText::new("Cada estação tem identidade própria perante o servidor, com credencial que pode ser revogada. Revogar não apaga o que já foi sincronizado.")
            .size(12.0)
            .color(p.tinta_suave),
    );
    ui.add_space(8.0);
    let aba = abas(
        ui,
        alvo,
        &[
            ("Sincronização", None),
            ("Alterações", None),
            (
                "Conexões",
                Some("Chega na fatia 4, com conexões, alertas e canais"),
            ),
        ],
        vista,
    );
    match aba {
        0 => {
            let estado = est.estado();
            egui::Grid::new(("estacao", alvo))
                .num_columns(2)
                .spacing(vec2(16.0, 6.0))
                .show(ui, |ui| {
                    let linha = |ui: &mut Ui, a: &str, b: String| {
                        ui.label(RichText::new(a).color(p.tinta_suave));
                        ui.label(b);
                        ui.end_row();
                    };
                    linha(ui, "Sistema", e.sistema.clone());
                    linha(
                        ui,
                        "Credencial",
                        match e.credencial {
                            Credencial::Ativa => "ativa".to_owned(),
                            Credencial::Revogada => "revogada".to_owned(),
                        },
                    );
                    linha(
                        ui,
                        "Quem decide o livro-caixa",
                        "o servidor; a cópia local é cache".to_owned(),
                    );
                    if e.esta {
                        linha(ui, "Servidor", estado.servidor.clone());
                        linha(ui, "Regras desta estação", estado.versao_das_regras.clone());
                        linha(ui, "Estado", texto_da_barra(&estado, hoje).0);
                    }
                });
        }
        1 => {
            let abertas: Vec<Alteracao> = alteracoes
                .iter()
                .filter(|a| a.estado.em_aberto())
                .cloned()
                .collect();
            if alteracoes.is_empty() {
                componentes::aviso(
                    ui,
                    "info",
                    "Nenhuma alteração desta estação.",
                    p.info_sutil,
                    p.info_borda,
                    p.info,
                );
            } else {
                desenhar_tabela(
                    ui,
                    &format!("estacao:{alvo}:alteracoes"),
                    tabela_de_alteracoes(
                        if abertas.is_empty() {
                            &alteracoes
                        } else {
                            &abertas
                        },
                        hoje,
                    ),
                    vista,
                    pedidos,
                    300.0,
                );
            }
        }
        _ => {}
    }
}

/// A página de uma alteração: abas Mudança (com o lado a lado, se recusada), Item
/// alterado e Estação.
pub fn pagina_da_alteracao(
    ui: &mut Ui,
    est: &dyn EstacaoDoLivro,
    alvo: &Ref,
    hoje: &str,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let Some(a) = est.alteracao(&alvo.chave).ok().flatten() else {
        componentes::aviso(
            ui,
            "help",
            "Alteração não encontrada nesta estação.",
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        return;
    };
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor("edit_note", 22.0, p.tinta));
        componentes::titulo(
            ui,
            &format!("{} em {}", a.pedido.o_que_muda(), a.rotulo_do_alvo),
            20.0,
        );
        componentes::estado(ui, a.estado.rotulo(), None);
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        kpi(ui, "O que mudou", a.pedido.o_que_muda(), &a.area);
        kpi(
            ui,
            "Antes → depois",
            &format!("{} → {}", a.antes, a.depois),
            "",
        );
        kpi(ui, "Feita em", &hora(&a.feita_em, hoje), "");
    });
    if a.estado == EstadoDaAlteracao::Recusada {
        componentes::aviso(
            ui,
            "block",
            &format!(
                "O servidor recusou. {} Fica aqui até você refazer ou descartar.",
                a.recusa.as_ref().map(|r| r.motivo.as_str()).unwrap_or("")
            ),
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
    }
    ui.add_space(8.0);
    let aba = abas(
        ui,
        alvo,
        &[
            ("Mudança", None),
            ("Item alterado", None),
            ("Estação", None),
        ],
        vista,
    );
    match aba {
        0 => {
            componentes::rotulo_de_secao(ui, "Antes e depois");
            ui.horizontal(|ui| {
                ui.label(RichText::new(&a.antes).color(p.tinta_suave));
                ui.label(icones::icone_cor("arrow_forward", 16.0, p.tinta_suave));
                ui.label(RichText::new(&a.depois).strong());
            });
            ui.add_space(8.0);
            match (&a.estado, &a.recusa) {
                (EstadoDaAlteracao::Recusada, Some(r)) => {
                    componentes::rotulo_de_secao(ui, "As duas versões");
                    lado_a_lado(ui, &a, r, pedidos);
                }
                (EstadoDaAlteracao::Aguardando, _) => {
                    ui.label(
                        RichText::new("Será enviada na próxima sincronização. O servidor aplica inteira ou recusa, nunca pela metade.")
                            .color(p.tinta_suave),
                    );
                }
                (EstadoDaAlteracao::Refeita, _) => {
                    if let Some(nova) = &a.refeita_em {
                        ui.horizontal(|ui| {
                            ui.label("Refeita sobre a versão nova em");
                            componentes::link_item(
                                ui,
                                &Ref::nova(Tipo::Alteracao, nova.clone()),
                                &format!("alteração {nova}"),
                                pedidos,
                            );
                        });
                    }
                }
                _ => {}
            }
            ui.add_space(6.0);
            componentes::rotulo_de_secao(ui, "Versão de base");
            for (r, versao) in &a.versoes_de_base {
                ui.label(
                    RichText::new(format!("{} · {versao}", r.tipo.singular()))
                        .size(12.0)
                        .color(p.tinta_suave),
                );
            }
        }
        1 => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(a.pedido.alvo().tipo.singular()).color(p.tinta_suave));
                componentes::link_item(ui, a.pedido.alvo(), &a.rotulo_do_alvo, pedidos);
            });
        }
        _ => {
            let estacoes = est.estacoes().unwrap_or_default();
            match estacoes.iter().find(|e| e.alvo == a.estacao) {
                Some(e) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Feita em").color(p.tinta_suave));
                        componentes::link_item(
                            ui,
                            &e.alvo,
                            &format!("Estação {}", e.nome),
                            pedidos,
                        );
                        ui.label(RichText::new(&e.sistema).color(p.tinta_suave));
                    });
                }
                None => {
                    ui.label(RichText::new("Estação não identificada.").color(p.tinta_suave));
                }
            }
        }
    }
}

/// A aba Alterações da página de uma movimentação.
pub fn aba_alteracoes_do_item(
    ui: &mut Ui,
    est: Option<&dyn EstacaoDoLivro>,
    alvo: &Ref,
    hoje: &str,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let lista = est
        .and_then(|e| e.alteracoes_do_item(alvo).ok())
        .unwrap_or_default();
    if lista.is_empty() {
        componentes::aviso(
            ui,
            "info",
            "Nenhuma alteração desta movimentação nesta estação. Mudar categoria fica na aba Livro-caixa do Ribbon.",
            p.info_sutil,
            p.info_borda,
            p.info,
        );
        return;
    }
    for a in lista
        .iter()
        .filter(|a| a.estado == EstadoDaAlteracao::Recusada)
    {
        cartao_da_recusa(ui, a, hoje, pedidos);
        ui.add_space(6.0);
    }
    desenhar_tabela(
        ui,
        &format!("alteracoes:{alvo}"),
        tabela_de_alteracoes(&lista, hoje),
        vista,
        pedidos,
        240.0,
    );
}

/// O destino da área de uma rota de Dinheiro (para o atalho da barra de status).
pub fn destino_da_sincronizacao() -> Destino {
    Destino::Area(Area::Sincronizacao)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hora_em_sao_paulo_de_qualquer_deslocamento() {
        assert_eq!(hora("2026-11-13T08:12:00-03:00", "2026-11-13"), "08:12");
        assert_eq!(hora("2026-11-13T11:12:00Z", "2026-11-13"), "08:12");
        // 01:30 UTC do dia 14 ainda é dia 13 em São Paulo.
        assert_eq!(hora("2026-11-14T01:30:00Z", "2026-11-13"), "22:30");
        assert_eq!(
            hora("2026-11-12T18:40:00-03:00", "2026-11-13"),
            "12/11 18:40"
        );
    }

    #[test]
    fn barra_diz_o_estado_da_rede() {
        let mut e = EstadoDaSincronizacao {
            esta_estacao: None,
            servidor: "servidor".to_owned(),
            ultima_troca: Some("2026-11-13T08:12:00-03:00".to_owned()),
            copia_de: Some("2026-11-13T08:12:00-03:00".to_owned()),
            rede: EstadoDaRede::Sincronizado,
            versao_das_regras: "0.1.0".to_owned(),
        };
        assert_eq!(texto_da_barra(&e, "2026-11-13").0, "Sincronizado às 08:12");
        e.rede = EstadoDaRede::SemRede;
        assert_eq!(
            texto_da_barra(&e, "2026-11-13").0,
            "Sem rede, mostrando a cópia de 08:12"
        );
        e.rede = EstadoDaRede::EstacaoRevogada;
        assert_eq!(texto_da_barra(&e, "2026-11-13").0, "Estação revogada");
        e.rede = EstadoDaRede::RegrasMaisNovas {
            versao_do_servidor: "0.2.0".to_owned(),
        };
        assert_eq!(
            texto_da_barra(&e, "2026-11-13").0,
            "Servidor com regras mais novas"
        );
    }
}
