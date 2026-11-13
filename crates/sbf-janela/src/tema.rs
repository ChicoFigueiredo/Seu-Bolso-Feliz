//! O tema da janela: os papéis de cor dos 7 temas do SBF e as fontes.
//!
//! É o **único** lugar que traduz os tokens (`docs/design/desktop/{tokens,temas}.json`)
//! para o egui. A casca (`bancada-casca`) vai assumir os temas depois: basta trocar
//! [`aplicar`] pela dela e continuar guardando a [`Paleta`] no contexto, porque as
//! vistas da janela leem as cores só por [`paleta`].

use std::collections::BTreeMap;
use std::sync::OnceLock;

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily, VariationCoords};
use egui::{Color32, Context, CornerRadius, FontData, FontFamily, FontId, Stroke, TextStyle};
use serde::Deserialize;

const TOKENS: &str = include_str!("../../../docs/design/desktop/tokens.json");
const TEMAS: &str = include_str!("../../../docs/design/desktop/temas.json");

const MANROPE: &[u8] = include_bytes!("../assets/fontes/Manrope[wght].ttf");
const INSTRUMENT_SERIF: &[u8] = include_bytes!("../assets/fontes/InstrumentSerif-Regular.ttf");
const SIMBOLOS: &[u8] = include_bytes!("../assets/fontes/MaterialSymbolsRounded-subconjunto.ttf");

/// Família da Manrope em peso 700, para títulos e números de destaque.
pub fn familia_forte() -> FontFamily {
    FontFamily::Name("sbf-forte".into())
}

/// Família da Instrument Serif: só para os números de destaque (`hero-number`).
pub fn familia_serifa() -> FontFamily {
    FontFamily::Name("sbf-serifa".into())
}

/// Família dos ícones (Material Symbols Rounded, peso 400, FILL 0).
pub fn familia_icones() -> FontFamily {
    FontFamily::Name("sbf-icones".into())
}

/// Os papéis de cor que a janela usa (nomes do `tokens.json`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paleta {
    pub escuro: bool,
    pub superficie_0: Color32,
    pub superficie_1: Color32,
    pub superficie_2: Color32,
    pub superficie_3: Color32,
    pub superficie_4: Color32,
    pub borda: Color32,
    pub borda_forte: Color32,
    pub tinta: Color32,
    pub tinta_suave: Color32,
    pub tinta_desabilitada: Color32,
    pub destaque: Color32,
    pub destaque_hover: Color32,
    pub sobre_destaque: Color32,
    pub destaque_sutil: Color32,
    pub destaque_borda: Color32,
    pub destaque_tinta: Color32,
    pub info: Color32,
    pub info_sutil: Color32,
    pub info_borda: Color32,
    pub sucesso: Color32,
    pub sucesso_sutil: Color32,
    pub aviso: Color32,
    pub aviso_sutil: Color32,
    pub aviso_borda: Color32,
    pub perigo: Color32,
    pub perigo_sutil: Color32,
    pub ouro: Color32,
    pub ouro_sutil: Color32,
    pub ouro_tinta: Color32,
    pub hero_fundo: Color32,
    pub hero_texto: Color32,
    pub hero_sub: Color32,
    pub veu: Color32,
}

/// Um tema do `temas.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tema {
    pub id: String,
    pub nome: String,
    pub familia: String,
    pub escuro: bool,
}

#[derive(Deserialize)]
struct ArquivoDeTokens {
    color: CoresDosTokens,
}

#[derive(Deserialize)]
struct CoresDosTokens {
    tokens: Vec<TokenDeCor>,
}

#[derive(Deserialize)]
struct TokenDeCor {
    name: String,
    value: ValorPorBase,
}

#[derive(Deserialize)]
struct ValorPorBase {
    dark: String,
    light: String,
}

#[derive(Deserialize)]
struct ArquivoDeTemas {
    themes: Vec<TemaJson>,
}

#[derive(Deserialize)]
struct TemaJson {
    id: String,
    name: String,
    base: String,
    family: String,
    colors: BTreeMap<String, String>,
}

struct Catalogo {
    base_clara: BTreeMap<String, Color32>,
    base_escura: BTreeMap<String, Color32>,
    temas: Vec<(Tema, BTreeMap<String, Color32>)>,
}

fn catalogo() -> &'static Catalogo {
    static CATALOGO: OnceLock<Catalogo> = OnceLock::new();
    CATALOGO.get_or_init(|| {
        let mut base_clara = BTreeMap::new();
        let mut base_escura = BTreeMap::new();
        // Os JSON vêm do repositório e são conferidos por teste; se um dia não lerem,
        // a janela cai nas cores de reserva de `cor()` em vez de não abrir.
        if let Ok(tokens) = serde_json::from_str::<ArquivoDeTokens>(TOKENS) {
            for t in tokens.color.tokens {
                if let Some(c) = cor_hex(&t.value.light) {
                    base_clara.insert(t.name.clone(), c);
                }
                if let Some(c) = cor_hex(&t.value.dark) {
                    base_escura.insert(t.name, c);
                }
            }
        }
        let mut temas = Vec::new();
        if let Ok(arquivo) = serde_json::from_str::<ArquivoDeTemas>(TEMAS) {
            for t in arquivo.themes {
                let cores = t
                    .colors
                    .iter()
                    .filter_map(|(k, v)| cor_hex(v).map(|c| (k.clone(), c)))
                    .collect();
                temas.push((
                    Tema {
                        id: t.id,
                        nome: t.name,
                        familia: t.family,
                        escuro: t.base == "dark",
                    },
                    cores,
                ));
            }
        }
        Catalogo {
            base_clara,
            base_escura,
            temas,
        }
    })
}

/// `#rrggbb` ou `#rrggbbaa`.
pub fn cor_hex(texto: &str) -> Option<Color32> {
    let h = texto.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            byte(6)?,
        )),
        _ => None,
    }
}

/// Os 7 temas, na ordem do `temas.json`.
pub fn temas() -> Vec<Tema> {
    catalogo().temas.iter().map(|(t, _)| t.clone()).collect()
}

/// O tema padrão claro (sálvia).
pub const TEMA_PADRAO: &str = "salvia";
/// O tema padrão escuro (noite).
pub const TEMA_ESCURO: &str = "noite";

/// A paleta de um tema; tema desconhecido cai no padrão.
pub fn paleta_do_tema(id: &str) -> Paleta {
    let cat = catalogo();
    let (tema, cores) = cat
        .temas
        .iter()
        .find(|(t, _)| t.id == id)
        .or_else(|| cat.temas.iter().find(|(t, _)| t.id == TEMA_PADRAO))
        .map(|(t, c)| (Some(t), Some(c)))
        .unwrap_or((None, None));
    let escuro = tema.is_some_and(|t| t.escuro);
    let base = if escuro {
        &cat.base_escura
    } else {
        &cat.base_clara
    };
    let cor = |nome: &str, reserva: Color32| -> Color32 {
        cores
            .and_then(|c| c.get(nome))
            .or_else(|| base.get(nome))
            .copied()
            .unwrap_or(reserva)
    };
    let (claro, escuro_c) = (Color32::WHITE, Color32::from_gray(20));
    let fundo = if escuro { escuro_c } else { claro };
    let texto = if escuro { claro } else { escuro_c };
    Paleta {
        escuro,
        superficie_0: cor("surface-0", fundo),
        superficie_1: cor("surface-1", fundo),
        superficie_2: cor("surface-2", fundo),
        superficie_3: cor("surface-3", fundo),
        superficie_4: cor("surface-4", fundo),
        borda: cor("border", Color32::GRAY),
        borda_forte: cor("border-strong", Color32::GRAY),
        tinta: cor("ink", texto),
        tinta_suave: cor("ink-muted", Color32::GRAY),
        tinta_desabilitada: cor("ink-disabled", Color32::GRAY),
        destaque: cor("accent", Color32::DARK_GREEN),
        destaque_hover: cor("accent-hover", Color32::DARK_GREEN),
        sobre_destaque: cor("on-accent", Color32::WHITE),
        destaque_sutil: cor("accent-subtle", fundo),
        destaque_borda: cor("accent-border", Color32::GRAY),
        destaque_tinta: cor("accent-ink", Color32::DARK_GREEN),
        info: cor("info", Color32::LIGHT_BLUE),
        info_sutil: cor("info-subtle", fundo),
        info_borda: cor("info-border", Color32::GRAY),
        sucesso: cor("success", Color32::GREEN),
        sucesso_sutil: cor("success-subtle", fundo),
        aviso: cor("warning", Color32::ORANGE),
        aviso_sutil: cor("warning-subtle", fundo),
        aviso_borda: cor("warning-border", Color32::GRAY),
        perigo: cor("danger", Color32::RED),
        perigo_sutil: cor("danger-subtle", fundo),
        ouro: cor("gold", Color32::GOLD),
        ouro_sutil: cor("gold-subtle", fundo),
        ouro_tinta: cor("gold-ink", Color32::GOLD),
        hero_fundo: cor("hero-bg", Color32::DARK_GREEN),
        hero_texto: cor("hero-text", Color32::WHITE),
        hero_sub: cor("hero-sub", Color32::LIGHT_GRAY),
        veu: cor("scrim", Color32::from_black_alpha(140)),
    }
}

fn id_da_paleta() -> egui::Id {
    egui::Id::new("sbf-janela/paleta")
}

/// A paleta em uso. Sem tema aplicado, devolve a do tema padrão.
pub fn paleta(ctx: &Context) -> Paleta {
    ctx.data(|d| d.get_temp::<Paleta>(id_da_paleta()))
        .unwrap_or_else(|| paleta_do_tema(TEMA_PADRAO))
}

/// Instala as fontes (Manrope, Instrument Serif e Material Symbols Rounded).
/// Chame uma vez, antes do primeiro quadro.
pub fn instalar_fontes(ctx: &Context) {
    let manrope = |peso: f32| {
        let mut dados = FontData::from_static(MANROPE);
        dados.tweak.coords = VariationCoords::new([("wght", peso)]);
        dados
    };
    ctx.add_font(FontInsert::new(
        "manrope",
        manrope(500.0),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
    ctx.add_font(FontInsert::new(
        "manrope-forte",
        manrope(700.0),
        vec![InsertFontFamily {
            family: familia_forte(),
            priority: FontPriority::Highest,
        }],
    ));
    ctx.add_font(FontInsert::new(
        "instrument-serif",
        FontData::from_static(INSTRUMENT_SERIF),
        vec![InsertFontFamily {
            family: familia_serifa(),
            priority: FontPriority::Highest,
        }],
    ));
    let mut simbolos = FontData::from_static(SIMBOLOS);
    simbolos.tweak.coords = VariationCoords::new([
        ("FILL", 0.0),
        ("GRAD", 0.0),
        ("opsz", 20.0),
        ("wght", 400.0),
    ]);
    simbolos.tweak.y_offset_factor = 0.12;
    ctx.add_font(FontInsert::new(
        "material-symbols-rounded",
        simbolos,
        vec![
            InsertFontFamily {
                family: familia_icones(),
                priority: FontPriority::Highest,
            },
            // Reserva: um ícone dentro de texto comum ainda aparece.
            InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
        ],
    ));
    // As famílias novas precisam de reserva para letras que a fonte não tem.
    for familia in [familia_forte(), familia_serifa(), familia_icones()] {
        ctx.add_font(FontInsert::new(
            &format!("reserva-{familia}"),
            FontData::from_static(MANROPE),
            vec![InsertFontFamily {
                family: familia,
                priority: FontPriority::Lowest,
            }],
        ));
    }
}

/// Aplica um tema: cores dos widgets, tipografia densa e a paleta no contexto.
pub fn aplicar(ctx: &Context, id_do_tema: &str) {
    let p = paleta_do_tema(id_do_tema);
    ctx.data_mut(|d| d.insert_temp(id_da_paleta(), p));
    let tema = if p.escuro {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(tema);
    ctx.style_mut_of(tema, |estilo| {
        estilo.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(20.0, familia_forte())),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        estilo.spacing.item_spacing = egui::vec2(8.0, 4.0);
        estilo.spacing.button_padding = egui::vec2(8.0, 3.0);
        estilo.spacing.interact_size.y = 24.0;
        estilo.interaction.selectable_labels = false;

        let v = &mut estilo.visuals;
        v.dark_mode = p.escuro;
        v.override_text_color = None;
        v.panel_fill = p.superficie_0;
        v.window_fill = p.superficie_1;
        v.window_stroke = Stroke::new(1.0, p.borda);
        v.window_corner_radius = CornerRadius::same(6);
        v.menu_corner_radius = CornerRadius::same(6);
        v.extreme_bg_color = p.superficie_1;
        v.text_edit_bg_color = Some(p.superficie_1);
        v.faint_bg_color = p.superficie_2;
        v.code_bg_color = p.superficie_3;
        v.hyperlink_color = p.destaque_tinta;
        v.warn_fg_color = p.aviso;
        v.error_fg_color = p.perigo;
        v.selection.bg_fill = p.destaque_sutil;
        v.selection.stroke = Stroke::new(1.0, p.destaque_tinta);
        v.striped = false;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = p.superficie_1;
        w.noninteractive.weak_bg_fill = p.superficie_1;
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.borda);
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.tinta);
        w.noninteractive.corner_radius = CornerRadius::same(4);
        w.inactive.bg_fill = p.superficie_2;
        w.inactive.weak_bg_fill = p.superficie_2;
        w.inactive.bg_stroke = Stroke::new(1.0, p.borda);
        w.inactive.fg_stroke = Stroke::new(1.0, p.tinta);
        w.inactive.corner_radius = CornerRadius::same(4);
        w.hovered.bg_fill = p.superficie_3;
        w.hovered.weak_bg_fill = p.superficie_3;
        w.hovered.bg_stroke = Stroke::new(1.0, p.borda_forte);
        w.hovered.fg_stroke = Stroke::new(1.0, p.tinta);
        w.hovered.corner_radius = CornerRadius::same(4);
        w.active.bg_fill = p.superficie_4;
        w.active.weak_bg_fill = p.superficie_4;
        w.active.bg_stroke = Stroke::new(1.0, p.borda_forte);
        w.active.fg_stroke = Stroke::new(1.0, p.tinta);
        w.active.corner_radius = CornerRadius::same(4);
        w.open.bg_fill = p.superficie_3;
        w.open.weak_bg_fill = p.superficie_3;
        w.open.bg_stroke = Stroke::new(1.0, p.borda);
        w.open.fg_stroke = Stroke::new(1.0, p.tinta);
        w.open.corner_radius = CornerRadius::same(4);
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_sete_temas_carregam() {
        let t = temas();
        assert_eq!(t.len(), 7);
        assert!(t.iter().any(|t| t.id == TEMA_PADRAO && !t.escuro));
        assert!(t.iter().any(|t| t.id == TEMA_ESCURO && t.escuro));
    }

    #[test]
    fn tema_herda_da_base_e_troca_so_o_que_difere() {
        let salvia = paleta_do_tema("salvia");
        let ocre = paleta_do_tema("ocre");
        assert_eq!(salvia.superficie_0, Color32::from_rgb(0xf6, 0xf7, 0xf2));
        assert_eq!(ocre.superficie_0, Color32::from_rgb(0xf7, 0xf2, 0xe8));
        let noite = paleta_do_tema("noite");
        assert!(noite.escuro);
        assert_eq!(noite.tinta, Color32::from_rgb(0xe8, 0xee, 0xe9));
    }

    #[test]
    fn cor_hex_le_com_e_sem_alfa() {
        assert_eq!(cor_hex("#ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert!(cor_hex("#0000008c").is_some());
        assert_eq!(cor_hex("vermelho"), None);
    }
}
