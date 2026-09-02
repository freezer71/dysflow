//! Les contrôles : boutons plats, lignes de navigation, lignes de menu, pastilles.
//!
//! Tout est petit et net : vingt-huit points de haut, une bordure d'un point,
//! des coins de six. Un bouton d'icône seul n'a pas de bordure tant qu'on ne
//! le survole pas ; un bouton principal est plein, couleur d'accent.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui,
    Vec2,
};

use super::icons::{self, Icon};
use super::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Size {
    Small,
    Regular,
    Large,
}

pub struct Button<'a> {
    icon: Option<Icon>,
    label: Option<&'a str>,
    primary: bool,
    selected: bool,
    /// Sans bordure au repos : les icônes seules des barres.
    subtle: bool,
    round: bool,
    size: Size,
    min_width: f32,
}

impl<'a> Button<'a> {
    pub fn icon(icon: Icon) -> Self {
        Self {
            icon: Some(icon),
            label: None,
            primary: false,
            selected: false,
            subtle: true,
            round: false,
            size: Size::Regular,
            min_width: 0.0,
        }
    }

    pub fn text(label: &'a str) -> Self {
        Self { icon: None, subtle: false, ..Self::icon(Icon::Play) }.label(label)
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self.subtle = false;
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = Size::Small;
        self
    }

    pub fn large(mut self) -> Self {
        self.size = Size::Large;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn show(self, ui: &mut Ui, palette: &Palette) -> Response {
        let (height, icon_px) = match self.size {
            Size::Small => (24.0, 14.0),
            Size::Regular => (28.0, icons::SIZE),
            Size::Large => (36.0, 18.0),
        };
        let style = if self.size == Size::Small { TextStyle::Small } else { TextStyle::Body };
        let font = style.resolve(ui.style());
        let galley = self
            .label
            .map(|label| ui.painter().layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER));
        let gap = 6.0;
        let pad_x = if galley.is_some() { 10.0 } else { (height - icon_px) / 2.0 };
        let mut width = 2.0 * pad_x;
        if self.icon.is_some() {
            width += icon_px;
        }
        if let Some(galley) = &galley {
            width += galley.size().x;
            if self.icon.is_some() {
                width += gap;
            }
        }
        width = width.max(self.min_width);
        if self.round {
            width = height;
        }
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();
        let (fill, stroke, fg) = if self.primary {
            let fill = if pressed {
                palette.accent
            } else if hovered {
                palette.accent_hover
            } else {
                palette.accent
            };
            (fill, Stroke::NONE, palette.on_accent)
        } else if self.selected {
            (palette.accent_soft, Stroke::new(1.0, palette.accent_soft), palette.accent)
        } else if hovered || pressed {
            let edge = if self.subtle { Stroke::NONE } else { Stroke::new(1.0, palette.dim) };
            (palette.surface, edge, palette.bright)
        } else if self.subtle {
            (Color32::TRANSPARENT, Stroke::NONE, palette.normal)
        } else {
            (Color32::TRANSPARENT, Stroke::new(1.0, palette.edge), palette.normal)
        };
        let radius = if self.round { CornerRadius::same((height / 2.0) as u8) } else { CornerRadius::same(6) };
        let painter = ui.painter();
        painter.rect(rect, radius, fill, stroke, StrokeKind::Inside);
        let mut x = rect.min.x + pad_x;
        if let Some(icon) = self.icon {
            let icon_rect = Rect::from_center_size(
                egui::pos2(x + icon_px / 2.0, rect.center().y),
                Vec2::splat(icon_px),
            );
            icons::paint(painter, icon_rect, icon, fg);
            x += icon_px + gap;
        }
        if let Some(galley) = galley {
            let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0).round();
            painter.galley(pos, galley, fg);
        }
        response
    }
}

/// Une ligne de la barre latérale : icône, libellé, et l'état de la lecture.
pub struct NavRow<'a> {
    pub icon: Option<Icon>,
    pub label: &'a str,
    /// La ligne où l'on est : fond et barre d'accent.
    pub selected: bool,
    /// La section que la voix lit : texte en accent.
    pub current: bool,
    pub indent: f32,
    /// Un mot à droite, en retrait : une heure, un compte.
    pub trailing: Option<&'a str>,
}

impl<'a> NavRow<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { icon: None, label, selected: false, current: false, indent: 0.0, trailing: None }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    pub fn trailing(mut self, text: &'a str) -> Self {
        self.trailing = Some(text);
        self
    }

    pub fn show(self, ui: &mut Ui, palette: &Palette) -> Response {
        let height = 28.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let hovered = response.hovered();
        let painter = ui.painter();
        if self.selected {
            painter.rect_filled(rect, 6.0, palette.surface);
            let bar = Rect::from_min_size(rect.min + Vec2::new(0.0, 6.0), Vec2::new(2.0, height - 12.0));
            painter.rect_filled(bar, 1.0, palette.accent);
        } else if hovered {
            painter.rect_filled(rect, 6.0, palette.surface);
        }
        let fg = if self.current {
            palette.accent
        } else if self.selected || hovered {
            palette.bright
        } else {
            palette.normal
        };
        let mut x = rect.min.x + 10.0 + self.indent;
        if let Some(icon) = self.icon {
            let icon_rect =
                Rect::from_center_size(egui::pos2(x + icons::SIZE / 2.0, rect.center().y), Vec2::splat(icons::SIZE));
            icons::paint(painter, icon_rect, icon, if self.current { palette.accent } else { palette.dim });
            x += icons::SIZE + 8.0;
        }
        let font = TextStyle::Body.resolve(ui.style());
        let mut room = rect.max.x - 10.0 - x;
        if let Some(trailing) = self.trailing {
            let galley = painter.layout_no_wrap(trailing.to_string(), TextStyle::Small.resolve(ui.style()), palette.faint);
            let pos = egui::pos2(rect.max.x - 10.0 - galley.size().x, rect.center().y - galley.size().y / 2.0).round();
            room -= galley.size().x + 8.0;
            painter.galley(pos, galley, palette.faint);
        }
        let galley = painter.layout(self.label.to_string(), font, fg, room.max(20.0));
        // Une seule ligne : ce qui déborde est coupé, pas replié.
        let galley = if galley.rows.len() > 1 {
            let mut text: String = self.label.chars().take(clip_chars(self.label, room)).collect();
            text.push('…');
            painter.layout_no_wrap(text, TextStyle::Body.resolve(ui.style()), fg)
        } else {
            galley
        };
        painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0).round(), galley, fg);
        response
    }
}

/// Le nombre de caractères qui tiennent dans `room`, à la louche : pour
/// couper un titre, pas pour le composer.
fn clip_chars(text: &str, room: f32) -> usize {
    let per_char = super::theme::UI_SIZE * 0.52;
    ((room / per_char).floor() as usize).saturating_sub(1).min(text.chars().count())
}

/// Une ligne de menu : icône, libellé, raccourci en retrait.
pub fn menu_item(ui: &mut Ui, palette: &Palette, icon: Option<Icon>, label: &str, shortcut: &str) -> Response {
    let height = 28.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let hovered = response.hovered();
    if hovered {
        painter.rect_filled(rect, 6.0, palette.surface);
    }
    let fg = if hovered { palette.bright } else { palette.normal };
    let mut x = rect.min.x + 10.0;
    let icon_rect = Rect::from_center_size(egui::pos2(x + icons::SIZE / 2.0, rect.center().y), Vec2::splat(icons::SIZE));
    if let Some(icon) = icon {
        icons::paint(painter, icon_rect, icon, palette.dim);
    }
    x += icons::SIZE + 8.0;
    painter.text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        TextStyle::Body.resolve(ui.style()),
        fg,
    );
    if !shortcut.is_empty() {
        painter.text(
            egui::pos2(rect.max.x - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            shortcut,
            TextStyle::Small.resolve(ui.style()),
            palette.faint,
        );
    }
    response
}

/// Un petit titre de groupe, en retrait : « Sommaire », « Signets ».
pub fn group_label(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(text).small().color(palette.faint));
    });
    ui.add_space(2.0);
}

/// Une pastille : un mot d'état sur fond de carte, bordé d'un point.
pub fn pill(ui: &mut Ui, palette: &Palette, text: &str, accent: bool) -> Response {
    let font = TextStyle::Small.resolve(ui.style());
    let fg = if accent { palette.accent } else { palette.normal };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let size = galley.size() + Vec2::new(16.0, 8.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let stroke = Stroke::new(1.0, if accent { palette.accent_soft } else { palette.edge });
        ui.painter().rect(rect, CornerRadius::same(12), palette.card, stroke, StrokeKind::Inside);
        ui.painter().galley((rect.min + Vec2::new(8.0, 4.0)).round(), galley, fg);
    }
    response
}

/// Un trait d'un point, couleur de bordure.
pub fn rule(ui: &mut Ui, palette: &Palette) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, palette.edge));
    ui.add_space(4.0);
}
