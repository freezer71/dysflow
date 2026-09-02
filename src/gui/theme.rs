//! Palette et visuels de la fenêtre.
//!
//! Sombre par défaut, un accent orange, des gris étagés ; le thème clair
//! inverse les gris et garde l'accent. L'interface est fine : texte de treize
//! points, bordures d'un point, coins à peine arrondis — et elle ne grandit
//! pas avec le texte lu. Seul ce qui se lit suit la taille réglée.

use eframe::egui::{self, Color32, CornerRadius, FontId, Shadow, Stroke, TextStyle, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0xF2, 0x6B, 0x27);

/// Taille du texte d'interface, en points : fixe, quoi qu'il arrive au texte lu.
pub const UI_SIZE: f32 = 13.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub accent: Color32,
    /// L'accent survolé, un peu plus clair.
    pub accent_hover: Color32,
    /// L'accent en voile : fond d'une ligne choisie, d'une sélection.
    pub accent_soft: Color32,
    /// Texte posé sur l'accent : le mot prononcé, le bouton principal.
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
    /// Fond des barres latérales.
    pub panel: Color32,
    /// Fond d'une carte, d'un menu, d'une fenêtre flottante.
    pub card: Color32,
    /// Fond d'un bloc de code ou d'un tableau.
    pub code_bg: Color32,
    /// Fond d'un bouton survolé, d'une ligne de menu sous le curseur.
    pub surface: Color32,
    /// Bordure d'un point.
    pub edge: Color32,
    /// Ombre douce sous une carte.
    pub shadow: Color32,
}

pub fn palette(theme: &str) -> Palette {
    if theme == "light" {
        Palette {
            accent: ACCENT,
            accent_hover: Color32::from_rgb(0xE0, 0x5E, 0x1C),
            accent_soft: Color32::from_rgba_unmultiplied(0xF2, 0x6B, 0x27, 0x2E),
            on_accent: Color32::WHITE,
            bright: Color32::from_rgb(0x16, 0x16, 0x16),
            normal: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            dim: Color32::from_rgb(0x70, 0x70, 0x70),
            faint: Color32::from_rgb(0xA6, 0xA6, 0xA6),
            technical: Color32::from_rgb(0x2E, 0x6E, 0x9E),
            ruler: Color32::from_rgb(0xF4, 0xE8, 0xDA),
            background: Color32::from_rgb(0xFB, 0xF9, 0xF5),
            panel: Color32::from_rgb(0xF3, 0xF1, 0xEC),
            card: Color32::WHITE,
            code_bg: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            surface: Color32::from_rgb(0xE9, 0xE6, 0xE0),
            edge: Color32::from_rgb(0xDC, 0xD8, 0xD0),
            shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 0x1C),
        }
    } else {
        Palette {
            accent: ACCENT,
            accent_hover: Color32::from_rgb(0xF7, 0x83, 0x47),
            accent_soft: Color32::from_rgba_unmultiplied(0xF2, 0x6B, 0x27, 0x30),
            on_accent: Color32::from_rgb(0x1A, 0x10, 0x08),
            bright: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            normal: Color32::from_rgb(0xBA, 0xBA, 0xBA),
            dim: Color32::from_rgb(0x80, 0x80, 0x84),
            faint: Color32::from_rgb(0x56, 0x56, 0x5A),
            technical: Color32::from_rgb(0x7E, 0xA8, 0xC4),
            ruler: Color32::from_rgb(0x26, 0x26, 0x2C),
            background: Color32::from_rgb(0x14, 0x14, 0x16),
            panel: Color32::from_rgb(0x1A, 0x1A, 0x1D),
            card: Color32::from_rgb(0x1F, 0x1F, 0x23),
            code_bg: Color32::from_rgb(0x1A, 0x1A, 0x1E),
            surface: Color32::from_rgb(0x28, 0x28, 0x2D),
            edge: Color32::from_rgb(0x30, 0x30, 0x36),
            shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 0x66),
        }
    }
}

/// Applique le thème au contexte : fond, texte, widgets, tailles.
pub fn apply(ctx: &egui::Context, theme: &str) {
    let palette = palette(theme);
    let mut visuals = if theme == "light" { Visuals::light() } else { Visuals::dark() };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.card;
    visuals.extreme_bg_color = palette.background;
    visuals.faint_bg_color = palette.code_bg;
    visuals.override_text_color = Some(palette.normal);
    visuals.selection.bg_fill = palette.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.text_cursor.stroke = Stroke::new(1.5, palette.accent);
    visuals.hyperlink_color = palette.technical;
    // Les widgets d'egui — curseurs, listes, cases des réglages — : coins à
    // peine arrondis, bordure d'un point, pas de relief.
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(6);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.edge);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.normal);
    visuals.widgets.inactive.bg_fill = palette.surface;
    visuals.widgets.inactive.weak_bg_fill = palette.card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.edge);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.normal);
    visuals.widgets.hovered.bg_fill = palette.surface;
    visuals.widgets.hovered.weak_bg_fill = palette.surface;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.dim);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.bright);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent_soft;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.bright);
    visuals.widgets.open.bg_fill = palette.surface;
    visuals.widgets.open.weak_bg_fill = palette.surface;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.dim);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0, palette.edge);
    let soft = Shadow { offset: [0, 4], blur: 16, spread: 0, color: palette.shadow };
    visuals.window_shadow = soft;
    visuals.popup_shadow = soft;
    visuals.slider_trailing_fill = true;
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Body, FontId::proportional(UI_SIZE));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(UI_SIZE));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(UI_SIZE * 1.15));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(UI_SIZE * 0.88));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(UI_SIZE * 0.92));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size = egui::vec2(40.0, 24.0);
        style.spacing.slider_rail_height = 4.0;
        style.spacing.slider_width = 160.0;
        style.spacing.combo_width = 140.0;
        style.spacing.icon_width = 14.0;
        style.spacing.icon_width_inner = 8.0;
    });
}
