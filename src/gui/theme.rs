//! Palette et visuels de la fenêtre.
//!
//! La palette est celle du terminal : un accent, des gris étagés, en couleurs
//! pleines — pas de dégradé, pas de flou. Le thème clair inverse les gris et
//! garde l'accent ; le mot prononcé, la règle et la phrase en cours gardent
//! leurs contrastes dans les deux.

use eframe::egui::{self, Color32, CornerRadius, FontId, Shadow, Stroke, TextStyle, Visuals};

use super::fonts;

pub const ACCENT: Color32 = Color32::from_rgb(0xF2, 0x6B, 0x27);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub accent: Color32,
    /// Texte posé sur l'accent : le mot prononcé.
    pub on_accent: Color32,
    pub bright: Color32,
    pub normal: Color32,
    pub dim: Color32,
    pub faint: Color32,
    /// Ce qui n'est pas de la prose : un lien, un fragment de code.
    pub technical: Color32,
    /// Fond de la règle de lecture.
    pub ruler: Color32,
    /// Fond de la zone de texte.
    pub background: Color32,
    /// Fond des barres et panneaux.
    pub panel: Color32,
    /// Fond d'un bloc de code ou d'un tableau.
    pub code_bg: Color32,
    /// Fond d'un bouton, d'une ligne de menu survolée.
    pub surface: Color32,
    /// Bordure d'un bouton, bloc vide d'une barre.
    pub edge: Color32,
    /// Ombre en bloc sous un bouton.
    pub shadow: Color32,
}

pub fn palette(theme: &str) -> Palette {
    if theme == "light" {
        Palette {
            accent: ACCENT,
            on_accent: Color32::from_rgb(0x1A, 0x12, 0x0A),
            bright: Color32::from_rgb(0x16, 0x16, 0x16),
            normal: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            dim: Color32::from_rgb(0x70, 0x70, 0x70),
            faint: Color32::from_rgb(0xA6, 0xA6, 0xA6),
            technical: Color32::from_rgb(0x2E, 0x6E, 0x9E),
            ruler: Color32::from_rgb(0xF4, 0xE8, 0xDA),
            background: Color32::from_rgb(0xFB, 0xF9, 0xF5),
            panel: Color32::from_rgb(0xEF, 0xED, 0xE8),
            code_bg: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            surface: Color32::from_rgb(0xE4, 0xE1, 0xDB),
            edge: Color32::from_rgb(0x8A, 0x8A, 0x8A),
            shadow: Color32::from_rgb(0xB8, 0xB4, 0xAC),
        }
    } else {
        Palette {
            accent: ACCENT,
            on_accent: Color32::BLACK,
            bright: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            normal: Color32::from_rgb(0xBA, 0xBA, 0xBA),
            dim: Color32::from_rgb(0x78, 0x78, 0x78),
            faint: Color32::from_rgb(0x52, 0x52, 0x52),
            technical: Color32::from_rgb(0x7E, 0xA8, 0xC4),
            ruler: Color32::from_rgb(0x26, 0x26, 0x2C),
            background: Color32::from_rgb(0x14, 0x14, 0x16),
            panel: Color32::from_rgb(0x1D, 0x1D, 0x20),
            code_bg: Color32::from_rgb(0x1A, 0x1A, 0x1E),
            surface: Color32::from_rgb(0x2A, 0x2A, 0x2E),
            edge: Color32::from_rgb(0x6A, 0x6A, 0x70),
            shadow: Color32::from_rgb(0x08, 0x08, 0x0A),
        }
    }
}

/// Applique le thème au contexte : fond, texte, widgets, tailles.
pub fn apply(ctx: &egui::Context, theme: &str, font_size: f32) {
    let palette = palette(theme);
    let mut visuals = if theme == "light" { Visuals::light() } else { Visuals::dark() };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = palette.background;
    visuals.faint_bg_color = palette.code_bg;
    visuals.override_text_color = Some(palette.normal);
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke.color = palette.on_accent;
    visuals.hyperlink_color = palette.technical;
    // Les widgets d'egui — curseurs, listes, cases des réglages — prennent
    // le même parti que les boutons pixel : coins carrés, bordure épaisse,
    // ombre en bloc, sans dégradé ni flou.
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
    visuals.widgets.inactive.bg_fill = palette.surface;
    visuals.widgets.inactive.weak_bg_fill = palette.surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(2.0, palette.edge);
    visuals.widgets.hovered.bg_fill = palette.surface;
    visuals.widgets.hovered.weak_bg_fill = palette.surface;
    visuals.widgets.hovered.bg_stroke = Stroke::new(2.0, palette.accent);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, palette.accent);
    visuals.widgets.active.fg_stroke.color = palette.on_accent;
    visuals.widgets.open.bg_fill = palette.surface;
    visuals.widgets.open.bg_stroke = Stroke::new(2.0, palette.accent);
    visuals.window_corner_radius = CornerRadius::ZERO;
    visuals.menu_corner_radius = CornerRadius::ZERO;
    visuals.window_stroke = Stroke::new(2.0, palette.edge);
    let block_shadow = Shadow { offset: [4, 4], blur: 0, spread: 0, color: palette.shadow };
    visuals.window_shadow = block_shadow;
    visuals.popup_shadow = block_shadow;
    visuals.slider_trailing_fill = true;
    ctx.set_visuals(visuals);

    // L'interface suit la taille du texte, un peu plus petite : les réglages
    // se lisent, le texte se lit d'abord.
    let ui_size = (font_size * 0.72).clamp(13.0, 20.0);
    ctx.all_styles_mut(|style| {
        // Ce qui se lit reste dans la police de lecture ; ce qui se manipule —
        // boutons, titres de rubriques — passe en pixel.
        style.text_styles.insert(TextStyle::Body, FontId::proportional(ui_size));
        style.text_styles.insert(TextStyle::Button, fonts::pixel(ui_size * 1.05));
        style.text_styles.insert(TextStyle::Heading, fonts::pixel(ui_size * 1.5));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(ui_size * 0.85));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(ui_size * 0.9));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.slider_rail_height = 6.0;
        style.spacing.slider_width = 160.0;
    });
}
