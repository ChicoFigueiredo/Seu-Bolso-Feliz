//! A área **Triagem por IA** (grupo Inventário) e **Opções → IA** (spec 09, "Janela").
//!
//! A IA só sugere. No topo, a rodada atual (barra de andamento, itens, chamadas, consumo
//! nominal, Pausar e Retomar). Abaixo, as sugestões **agrupadas por campo e valor**
//! ("142 registros → categoria Mercado"): aceitar o grupo inteiro, aceitar só os marcados
//! ou recusar, com filtro por confiança. À direita, o item em foco com a justificativa e o
//! link para a página dele (a prévia do documento está lá). Aceitar grava uma decisão do
//! usuário por item, pelo `Comandos` (o padrão das Sugestões de conciliação), e se desfaz.
//!
//! A janela fala com um [`ServicoDeTriagem`]: a fictícia (demonstração) ou a do inventário.
//! A tela só emite [`Pedido::Triagem`]; quem aplica é `Janela::processar`, fora do quadro.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, ScrollArea, Ui};
use sbf_ia::NivelDeConfianca;
use sbf_triagem::piloto::{FAIXA_ALTA, FAIXA_BAIXA, FAIXA_MEDIA, RelatorioDoPiloto};
use sbf_triagem::rodada::{Progresso, SituacaoDaRodada};
use sbf_triagem::servico::{ConfiguracaoDeIa, ProvedorEscolhido, ServicoDeTriagem};
use sbf_triagem::sugestoes::{FiltroDeSugestoes, GrupoDeSugestoes, rotulo_do_campo};

use crate::componentes;
use crate::navegacao::Pedido;
use crate::tema;

/// O que a tela pede ao serviço. Aplicado fora do quadro.
#[derive(Debug, Clone, PartialEq)]
pub enum AcaoDaTriagem {
    Iniciar { piloto: bool },
    Pausar,
    Retomar,
    Aceitar(Vec<i64>),
    Recusar(Vec<i64>),
    SalvarConfiguracao(ConfiguracaoDeIa),
    TestarConexao,
}

/// O que a janela guarda da triagem: o serviço e o estado da tela.
#[derive(Default)]
pub struct EstadoDaTriagem {
    /// `None`: a triagem não está ligada (nenhum provedor configurado nesta máquina).
    pub servico: Option<Box<dyn ServicoDeTriagem>>,
    grupos: Vec<GrupoDeSugestoes>,
    /// As sugestões estão desatualizadas e se leem de novo no próximo quadro.
    sujo: bool,
    lido_em: Option<Instant>,
    pub confianca_minima: f32,
    pub campo: Option<String>,
    marcadas: HashSet<i64>,
    foco: Option<i64>,
    /// A configuração em edição (Opções → IA).
    edicao: Option<ConfiguracaoDeIa>,
    /// O resultado do último teste de conexão.
    pub teste: Option<Result<String, String>>,
    /// Há um teste de conexão em andamento (a tela mostra "Testando…" e desliga o botão).
    pub testando: bool,
    /// O último erro de leitura.
    pub erro: Option<String>,
}

impl std::fmt::Debug for EstadoDaTriagem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstadoDaTriagem")
            .field("ligada", &self.servico.is_some())
            .finish_non_exhaustive()
    }
}

impl EstadoDaTriagem {
    pub fn com_servico(servico: Box<dyn ServicoDeTriagem>) -> Self {
        Self {
            servico: Some(servico),
            sujo: true,
            ..Self::default()
        }
    }

    /// Marca as sugestões como desatualizadas (depois de aceitar, recusar ou iniciar).
    pub fn atualizar(&mut self) {
        self.sujo = true;
    }

    /// As sugestões agrupadas, como estão em cache.
    pub fn grupos(&self) -> &[GrupoDeSugestoes] {
        &self.grupos
    }

    pub fn marcar(&mut self, id: i64, marcada: bool) {
        if marcada {
            self.marcadas.insert(id);
        } else {
            self.marcadas.remove(&id);
        }
    }

    pub fn marcadas(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.marcadas.iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// Lê as sugestões de novo quando preciso (sujo, ou a cada 2 s com a rodada andando).
    pub fn ler_se_preciso(&mut self) {
        let Some(servico) = self.servico.as_deref() else {
            return;
        };
        let rodando = servico.progresso().situacao == SituacaoDaRodada::Rodando;
        let vencido = self
            .lido_em
            .is_none_or(|t| rodando && t.elapsed() > Duration::from_secs(2));
        if !(self.sujo || vencido) {
            return;
        }
        let filtro = FiltroDeSugestoes {
            confianca_minima: Some(f64::from(self.confianca_minima)),
            campo: self.campo.clone(),
        };
        match servico.grupos(&filtro) {
            Ok(g) => {
                self.grupos = g;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e),
        }
        let existentes: HashSet<i64> = self
            .grupos
            .iter()
            .flat_map(|g| g.sugestoes.iter().map(|s| s.id))
            .collect();
        self.marcadas.retain(|id| existentes.contains(id));
        self.sujo = false;
        self.lido_em = Some(Instant::now());
    }
}

/// Dólares com dois decimais a partir de dezmilésimos de dólar.
pub fn dolares(dezmilesimos: u64) -> String {
    let centavos = dezmilesimos / 100;
    format!("US$ {},{:02}", centavos / 100, centavos % 100)
}

fn texto_da_situacao(s: SituacaoDaRodada) -> &'static str {
    match s {
        SituacaoDaRodada::Ociosa => "nenhuma rodada em andamento",
        SituacaoDaRodada::Rodando => "rodando",
        SituacaoDaRodada::Pausada => "pausada",
        SituacaoDaRodada::Concluida => "concluída",
        SituacaoDaRodada::Falhou => "falhou",
        SituacaoDaRodada::Cancelada => "cancelada",
    }
}

/// "142 registros → categoria Mercado".
pub fn rotulo_do_grupo(g: &GrupoDeSugestoes) -> String {
    let (um, varios) = if g.alvo_tipo == "registro" {
        ("registro", "registros")
    } else {
        ("achado", "achados")
    };
    let n = g.sugestoes.len();
    format!(
        "{n} {} → {} {}",
        if n == 1 { um } else { varios },
        rotulo_do_campo(g.alvo_tipo, &g.campo),
        g.valor_texto
    )
}

fn cartao_da_rodada(ui: &mut Ui, p: &Progresso, pedidos: &mut Vec<Pedido>) {
    let cores = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.horizontal(|ui| {
            componentes::titulo(ui, "Rodada atual", 14.0);
            ui.label(RichText::new(texto_da_situacao(p.situacao)).color(cores.tinta_suave));
        });
        ui.add(
            egui::ProgressBar::new(p.fracao())
                .show_percentage()
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new(format!(
                "{} de {} itens · {} chamadas · {} sugestões · {} não classificados · {} subiram de degrau · consumo nominal {}",
                p.processados,
                p.total,
                p.chamadas,
                p.sugestoes,
                p.nao_classificados,
                p.subiram_de_degrau,
                dolares(p.custo_dezmilesimos_usd)
            ))
            .size(12.5)
            .color(cores.tinta_suave),
        );
        if let Some(aviso) = &p.aviso {
            ui.label(RichText::new(aviso).color(cores.aviso));
        }
        ui.horizontal(|ui| {
            let rodando = p.situacao == SituacaoDaRodada::Rodando;
            let pausada = p.situacao == SituacaoDaRodada::Pausada;
            let livre = !rodando && !pausada;
            if ui
                .add_enabled(livre, egui::Button::new("Iniciar rodada"))
                .on_hover_text("Triagem dos itens que você ainda não decidiu")
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Iniciar { piloto: false }));
            }
            if ui
                .add_enabled(livre, egui::Button::new("Piloto de 200"))
                .on_hover_text("Mede o acerto contra 200 itens que você já decidiu")
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Iniciar { piloto: true }));
            }
            if ui
                .add_enabled(rodando, egui::Button::new("Pausar"))
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Pausar));
            }
            if ui
                .add_enabled(pausada, egui::Button::new("Retomar"))
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Retomar));
            }
        });
    });
}

fn cartao_do_piloto(ui: &mut Ui, r: &RelatorioDoPiloto) {
    let cores = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        componentes::titulo(ui, "Piloto", 14.0);
        let taxa = r.geral.taxa().map_or_else(
            || "sem comparações".to_owned(),
            |t| format!("{:.0}%", t * 100.0),
        );
        ui.label(format!(
            "Acerto {taxa} ({} de {} sugestões comparadas) · {} itens · {} não classificados · {} sem decisão sua para comparar",
            r.geral.acertos, r.geral.comparadas, r.itens, r.nao_classificados, r.sem_gabarito
        ));
        for faixa in [FAIXA_BAIXA, FAIXA_MEDIA, FAIXA_ALTA] {
            if let Some(pl) = r.por_faixa.get(faixa) {
                ui.label(
                    RichText::new(format!(
                        "confiança {faixa}: {} de {} certas",
                        pl.acertos, pl.comparadas
                    ))
                    .size(12.5)
                    .color(cores.tinta_suave),
                );
            }
        }
        ui.label(
            RichText::new(format!(
                "Consumo: {} chamadas, {} tokens de entrada, {} de saída, {} nominais{}",
                r.rodada.chamadas,
                r.tokens_entrada,
                r.tokens_saida,
                dolares(r.custo_dezmilesimos_usd),
                r.tokens_por_item
                    .map_or_else(String::new, |t| format!(" (cerca de {t} tokens por item)")),
            ))
            .size(12.5)
            .color(cores.tinta_suave),
        );
    });
}

fn lista_de_grupos(ui: &mut Ui, estado: &mut EstadoDaTriagem, pedidos: &mut Vec<Pedido>) {
    let cores = tema::paleta(ui.ctx());
    if estado.grupos.is_empty() {
        ui.label(
            RichText::new(
                "Nenhuma sugestão aberta. Inicie uma rodada ou ajuste o filtro de confiança.",
            )
            .color(cores.tinta_suave),
        );
        return;
    }
    let grupos = estado.grupos.clone();
    for g in &grupos {
        let titulo = rotulo_do_grupo(g);
        egui::CollapsingHeader::new(RichText::new(&titulo).strong())
            .id_salt(("triagem-grupo", g.alvo_tipo, &g.campo, &g.valor_texto))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(format!("Aceitar o grupo ({})", g.sugestoes.len()))
                        .clicked()
                    {
                        pedidos.push(Pedido::Triagem(AcaoDaTriagem::Aceitar(g.ids())));
                    }
                    if ui.button("Recusar o grupo").clicked() {
                        pedidos.push(Pedido::Triagem(AcaoDaTriagem::Recusar(g.ids())));
                    }
                });
                let altura = 22.0;
                ScrollArea::vertical()
                    .max_height(280.0)
                    .id_salt(("triagem-linhas", g.alvo_tipo, &g.campo, &g.valor_texto))
                    .show_rows(ui, altura, g.sugestoes.len(), |ui, faixa| {
                        for s in &g.sugestoes[faixa] {
                            ui.horizontal(|ui| {
                                let mut marcada = estado.marcadas.contains(&s.id);
                                if ui.checkbox(&mut marcada, "").changed() {
                                    estado.marcar(s.id, marcada);
                                }
                                let em_foco = estado.foco == Some(s.id);
                                if ui.selectable_label(em_foco, &s.titulo).clicked() {
                                    estado.foco = Some(s.id);
                                }
                                ui.label(
                                    RichText::new(format!("{:.2}", s.confianca))
                                        .monospace()
                                        .color(if s.confianca < 0.7 {
                                            cores.aviso
                                        } else {
                                            cores.tinta_suave
                                        }),
                                );
                            });
                        }
                    });
            });
    }
}

fn detalhe_do_foco(ui: &mut Ui, estado: &EstadoDaTriagem, pedidos: &mut Vec<Pedido>) {
    let cores = tema::paleta(ui.ctx());
    let Some(s) = estado.foco.and_then(|id| {
        estado
            .grupos
            .iter()
            .flat_map(|g| &g.sugestoes)
            .find(|s| s.id == id)
    }) else {
        ui.label(
            RichText::new("Clique numa sugestão para ver a justificativa.")
                .color(cores.tinta_suave),
        );
        return;
    };
    componentes::titulo(ui, &s.titulo, 14.0);
    ui.label(format!(
        "{} → {}",
        rotulo_do_campo(s.alvo.tipo(), &s.campo),
        sbf_triagem::sugestoes::texto_do_valor(&s.campo, &s.valor)
    ));
    ui.label(
        RichText::new(format!("confiança {:.2} · {}", s.confianca, s.modelo))
            .color(cores.tinta_suave),
    );
    ui.add_space(4.0);
    ui.label(&s.justificativa);
    ui.add_space(8.0);
    let alvo = s.alvo.referencia();
    componentes::link_item(
        ui,
        &alvo,
        "Abrir o item (com a prévia do documento)",
        pedidos,
    );
}

/// A área Triagem por IA.
pub fn area(ui: &mut Ui, estado: &mut EstadoDaTriagem, pedidos: &mut Vec<Pedido>) {
    let cores = tema::paleta(ui.ctx());
    let Some(progresso) = estado.servico.as_deref().map(|s| s.progresso()) else {
        componentes::aviso(
            ui,
            "smart_toy",
            "A triagem por IA não está ligada nesta máquina. Configure o provedor em Opções → IA.",
            cores.info_sutil,
            cores.info_borda,
            cores.info,
        );
        return;
    };
    estado.ler_se_preciso();
    if progresso.situacao == SituacaoDaRodada::Rodando {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
    cartao_da_rodada(ui, &progresso, pedidos);
    if let Some(r) = estado
        .servico
        .as_deref()
        .and_then(|s| s.relatorio_do_piloto())
    {
        ui.add_space(6.0);
        cartao_do_piloto(ui, &r);
    }
    if let Some(e) = &estado.erro {
        ui.label(RichText::new(e).color(cores.perigo));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Confiança mínima");
        let antes = estado.confianca_minima;
        ui.add(egui::Slider::new(&mut estado.confianca_minima, 0.0..=1.0).step_by(0.05));
        if (antes - estado.confianca_minima).abs() > f32::EPSILON {
            estado.sujo = true;
        }
        let n = estado.marcadas.len();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(n > 0, egui::Button::new(format!("Recusar marcados ({n})")))
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Recusar(estado.marcadas())));
            }
            if ui
                .add_enabled(n > 0, egui::Button::new(format!("Aceitar marcados ({n})")))
                .clicked()
            {
                pedidos.push(Pedido::Triagem(AcaoDaTriagem::Aceitar(estado.marcadas())));
            }
        });
    });
    ui.add_space(4.0);
    let lateral = 300.0;
    let esquerda = (ui.available_width() - lateral - 16.0).max(240.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(esquerda);
            ScrollArea::vertical()
                .id_salt("triagem-lista")
                .show(ui, |ui| lista_de_grupos(ui, estado, pedidos));
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(lateral);
            detalhe_do_foco(ui, estado, pedidos);
        });
    });
}

fn combo_de_provedor(
    ui: &mut Ui,
    id: &str,
    atual: &mut Option<ProvedorEscolhido>,
    com_nenhum: bool,
) {
    let rotulo = atual.map_or("Nenhuma", ProvedorEscolhido::rotulo);
    egui::ComboBox::from_id_salt(id)
        .selected_text(rotulo)
        .show_ui(ui, |ui| {
            if com_nenhum {
                ui.selectable_value(atual, None, "Nenhuma");
            }
            for p in [ProvedorEscolhido::ClaudeCli, ProvedorEscolhido::OpenRouter] {
                ui.selectable_value(atual, Some(p), p.rotulo());
            }
        });
}

fn rotulo_do_nivel(n: NivelDeConfianca) -> &'static str {
    match n {
        NivelDeConfianca::Mascarado => "Mascarado (CPF, conta e cartão escondidos)",
        NivelDeConfianca::Parcial => "Parcial",
        NivelDeConfianca::Completo => "Completo (o texto inteiro vai)",
    }
}

/// Opções → IA: provedor principal, reserva, tetos, nível e o teste de conexão.
pub fn opcoes_ia(ui: &mut Ui, estado: &mut EstadoDaTriagem, pedidos: &mut Vec<Pedido>) {
    let cores = tema::paleta(ui.ctx());
    componentes::rotulo_de_secao(ui, "IA (triagem do inventário)");
    let Some(servico) = estado.servico.as_deref() else {
        ui.label(
            RichText::new("A triagem por IA não está ligada nesta máquina.")
                .color(cores.tinta_suave),
        );
        return;
    };
    if estado.edicao.is_none() {
        estado.edicao = Some(servico.configuracao());
    }
    let Some(c) = estado.edicao.as_mut() else {
        return;
    };
    egui::Grid::new("opcoes-ia").num_columns(2).show(ui, |ui| {
        ui.label("Provedor principal");
        let mut principal = Some(c.principal);
        combo_de_provedor(ui, "ia-principal", &mut principal, false);
        if let Some(p) = principal {
            c.principal = p;
        }
        ui.end_row();

        ui.label("Reserva");
        combo_de_provedor(ui, "ia-reserva", &mut c.reserva, true);
        ui.end_row();

        ui.label("Teto por rodada (US$)");
        let mut r = f64::from(c.teto_da_rodada_centavos_usd) / 100.0;
        ui.add(
            egui::DragValue::new(&mut r)
                .range(0.0..=1000.0)
                .speed(0.5)
                .fixed_decimals(2),
        );
        c.teto_da_rodada_centavos_usd = (r * 100.0).round() as u32;
        ui.end_row();

        ui.label("Teto por mês (US$)");
        let mut m = f64::from(c.teto_do_mes_centavos_usd) / 100.0;
        ui.add(
            egui::DragValue::new(&mut m)
                .range(0.0..=10000.0)
                .speed(1.0)
                .fixed_decimals(2),
        );
        c.teto_do_mes_centavos_usd = (m * 100.0).round() as u32;
        ui.end_row();

        ui.label("Nível do destinatário \u{201c}triagem\u{201d}");
        egui::ComboBox::from_id_salt("ia-nivel")
            .selected_text(rotulo_do_nivel(c.nivel))
            .show_ui(ui, |ui| {
                for n in [
                    NivelDeConfianca::Mascarado,
                    NivelDeConfianca::Parcial,
                    NivelDeConfianca::Completo,
                ] {
                    ui.selectable_value(&mut c.nivel, n, rotulo_do_nivel(n));
                }
            });
        ui.end_row();
    });
    ui.label(
        RichText::new("Os tetos valem para o OpenRouter. Na assinatura o consumo é nominal e só serve de aviso. A reserva só entra se estiver ligada e abaixo do teto.")
            .size(12.0)
            .color(cores.tinta_suave),
    );
    let nova = c.clone();
    ui.horizontal(|ui| {
        if ui.button("Salvar").clicked() {
            pedidos.push(Pedido::Triagem(AcaoDaTriagem::SalvarConfiguracao(nova)));
        }
        if ui
            .add_enabled(!estado.testando, egui::Button::new("Testar conexão"))
            .clicked()
        {
            pedidos.push(Pedido::Triagem(AcaoDaTriagem::TestarConexao));
        }
        if estado.testando {
            ui.spinner();
            ui.label(RichText::new("Testando…").color(cores.tinta_suave));
        }
    });
    match &estado.teste {
        Some(Ok(t)) => {
            ui.label(RichText::new(t).color(cores.sucesso));
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e).color(cores.perigo));
        }
        None => {}
    }
}

#[cfg(test)]
mod testes {
    use sbf_triagem::servico::TriagemFicticia;

    use super::*;

    #[test]
    fn dolares_a_partir_de_dezmilesimos() {
        assert_eq!(dolares(3800), "US$ 0,38");
        assert_eq!(dolares(123_456), "US$ 12,34");
        assert_eq!(dolares(0), "US$ 0,00");
    }

    #[test]
    fn rotulo_do_grupo_diz_quantos_o_campo_e_o_valor() {
        let mut e = EstadoDaTriagem::com_servico(Box::new(TriagemFicticia::nova()));
        e.ler_se_preciso();
        assert_eq!(
            rotulo_do_grupo(&e.grupos()[0]),
            "142 registros → categoria Mercado"
        );
    }

    #[test]
    fn marcar_e_filtrar_por_confianca() {
        let mut e = EstadoDaTriagem::com_servico(Box::new(TriagemFicticia::nova()));
        e.ler_se_preciso();
        let total: usize = e.grupos().iter().map(|g| g.sugestoes.len()).sum();
        e.marcar(1, true);
        e.marcar(2, true);
        assert_eq!(e.marcadas(), vec![1, 2]);
        e.confianca_minima = 0.9;
        e.atualizar();
        e.ler_se_preciso();
        let alta: usize = e.grupos().iter().map(|g| g.sugestoes.len()).sum();
        assert!(alta < total);
    }

    #[test]
    fn a_area_desenha_sem_servico_e_com_servico() {
        let ctx = egui::Context::default();
        tema::instalar_fontes(&ctx);
        tema::aplicar(&ctx, "claro");
        for servico in [
            None,
            Some(Box::new(TriagemFicticia::nova()) as Box<dyn ServicoDeTriagem>),
        ] {
            let mut estado = EstadoDaTriagem {
                servico,
                sujo: true,
                ..EstadoDaTriagem::default()
            };
            let mut pedidos = Vec::new();
            let entrada = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..egui::RawInput::default()
            };
            let mut saida = ctx.run_ui(entrada, |ui| {
                area(ui, &mut estado, &mut pedidos);
                opcoes_ia(ui, &mut estado, &mut pedidos);
            });
            saida.textures_delta.clear();
        }
    }
}
