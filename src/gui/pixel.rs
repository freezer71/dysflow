//! Le pixel art des contrôles : sprites, boutons, barres en blocs, menu.
//!
//! Tout ce qui se manipule est dessiné pixel par pixel, sur une grille dont le
//! pas suit la taille de l'interface. Le texte lu ne passe pas par ici : il
//! garde sa police faite pour être lue longtemps. Ici, les libellés sont
//! courts — un verbe, un chiffre — et la police pixel leur suffit.

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Response, Sense, Ui, Vec2};

use super::fonts;
use super::theme::Palette;

/// Un dessin sur grille : `#` plein, `.` vide. Neuf colonnes, neuf lignes.
pub type Sprite = [&'static str; 9];

pub const PLAY: Sprite = [
    ".##......",
    ".####....",
    ".#####...",
    ".######..",
    ".#######.",
    ".######..",
    ".#####...",
    ".####....",
    ".##......",
];

pub const PAUSE: Sprite = [
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
    ".##...##.",
];

pub const PREV: Sprite = [
    ".........",
    "...#....#",
    "..##...##",
    ".###..###",
    "####.####",
    ".###..###",
    "..##...##",
    "...#....#",
    ".........",
];

pub const NEXT: Sprite = [
    ".........",
    "#....#...",
    "##...##..",
    "###..###.",
    "####.####",
    "###..###.",
    "##...##..",
    "#....#...",
    ".........",
];

pub const MINUS: Sprite = [
    ".........",
    ".........",
    ".........",
    ".#######.",
    ".#######.",
    ".#######.",
    ".........",
    ".........",
    ".........",
];

pub const PLUS: Sprite = [
    ".........",
    "...###...",
    "...###...",
    ".#######.",
    ".#######.",
    ".#######.",
    "...###...",
    "...###...",
    ".........",
];

pub const SPEAKER: Sprite = [
    "....#....",
    "...##..#.",
    "..###...#",
    "#####.#.#",
    "#####.#.#",
    "#####.#.#",
    "..###...#",
    "...##..#.",
    "....#....",
];

pub const MUTED: Sprite = [
    "....#....",
    "...##....",
    "..###.#.#",
    "#####..#.",
    "#####.#.#",
    "#####....",
    "..###....",
    "...##....",
    "....#....",
];

pub const MORE: Sprite = [
    ".........",
    ".........",
    ".........",
    ".........",
    ".##.##.##",
    ".##.##.##",
    ".........",
    ".........",
    ".........",
];

pub const FLAG: Sprite = [
    ".#.......",
    ".######..",
    ".#######.",
    ".#######.",
    ".######..",
    ".#.......",
    ".#.......",
    ".#.......",
    ".#.......",
];

pub const LIST: Sprite = [
    ".........",
    "##.######",
    ".........",
    "##.######",
    ".........",
    "##.######",
    ".........",
    "##.######",
    ".........",
];

pub const GEAR: Sprite = [
    "...###...",
    ".#.###.#.",
    ".#######.",
    "###...###",
    "###...###",
    "###...###",
    ".#######.",
    ".#.###.#.",
    "...###...",
];

pub const SEARCH: Sprite = [
    ".####....",
    "#....#...",
    "#....#...",
    "#....#...",
    ".####....",
    "....##...",
    ".....##..",
    "......##.",
    ".......##",
];

pub const FOLDER: Sprite = [
    ".........",
    ".####....",
    "#########",
    "#.......#",
    "#.......#",
    "#.......#",
    "#.......#",
    "#########",
    ".........",
];

pub const CLIPBOARD: Sprite = [
    "...###...",
    ".#######.",
    ".#.....#.",
    ".#.###.#.",
    ".#.....#.",
    ".#.###.#.",
    ".#.....#.",
    ".#######.",
    ".........",
];

pub const CLOSE: Sprite = [
    ".........",
    ".##...##.",
    "..##.##..",
    "...###...",
    "....#....",
    "...###...",
    "..##.##..",
    ".##...##.",
    ".........",
];

pub const HELP: Sprite = [
    "..#####..",
    ".##...##.",
    ".##...##.",
    "......##.",
    "....###..",
    "...##....",
    ".........",
    "...##....",
    "...##....",
];

pub const RESTART: Sprite = [
    "..#####..",
    ".##...##.",
    "##.....##",
    "#.......#",
    "........#",
    "#...#...#",
    "##..##.##",
    ".##.###..",
    "....####.",
];

pub const HOME: Sprite = [
    "##..#....",
    "##..##...",
    "##..###..",
    "##..####.",
    "##..#####",
    "##..####.",
    "##..###..",
    "##..##...",
    "##..#....",
];

pub const FOLLOW: Sprite = [
    "....#....",
    "....#....",
    "....#....",
    "....#....",
    ".#..#..#.",
    ".##.#.##.",
    "..#####..",
    "...###...",
    ".#######.",
];

/// Pas de la grille, en points : suit la taille de l'interface.
pub fn step(ui: &Ui) -> f32 {
    let size = egui::TextStyle::Body.resolve(ui.style()).size;
    (size / 6.0).round().clamp(2.0, 5.0)
}

/// Dessine un sprite, une rangée de pixels à la fois.
pub fn draw_sprite(painter: &egui::Painter, top_left: Pos2, sprite: &Sprite, px: f32, color: Color32) {
    let top_left = top_left.round();
    for (row, line) in sprite.iter().enumerate() {
        let y = top_left.y + row as f32 * px;
        let mut run: Option<usize> = None;
        for (col, c) in line.chars().chain(std::iter::once('.')).enumerate() {
            match (c, run) {
                ('#', None) => run = Some(col),
                ('#', Some(_)) => {}
                (_, Some(start)) => {
                    let x = top_left.x + start as f32 * px;
                    let w = (col - start) as f32 * px;
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, px)),
                        CornerRadius::ZERO,
                        color,
                    );
                    run = None;
                }
                _ => {}
            }
        }
    }
}

/// Un bouton pixel : bordure épaisse aux coins coupés, ombre en bloc, qui
/// s'enfonce d'un cran au clic.
pub struct Button<'a> {
    icon: Option<&'static Sprite>,
    label: Option<&'a str>,
    primary: bool,
    selected: bool,
    big: bool,
    min_width: f32,
}

impl<'a> Button<'a> {
    pub fn icon(sprite: &'static Sprite) -> Self {
        Self { icon: Some(sprite), label: None, primary: false, selected: false, big: false, min_width: 0.0 }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Le geste principal : fond accent.
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// Un état actif : bordure accent.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Les gestes de lecture : plus hauts, plus larges.
    pub fn big(mut self) -> Self {
        self.big = true;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn show(self, ui: &mut Ui, palette: &Palette) -> Response {
        let px = step(ui);
        let body_size = egui::TextStyle::Body.resolve(ui.style()).size;
        let font = fonts::pixel(if self.big { body_size * 1.25 } else { body_size * 1.05 });

        // Le contenu d'abord : le bouton se dimensionne dessus.
        let label_color = self.content_color(palette, false);
        let galley = self
            .label
            .map(|text| ui.painter().layout_no_wrap(text.to_string(), font.clone(), label_color));
        let icon_w = if self.icon.is_some() { 9.0 * px } else { 0.0 };
        let text_w = galley.as_ref().map_or(0.0, |g| g.size().x);
        let gap = if self.icon.is_some() && galley.is_some() { 2.0 * px } else { 0.0 };
        let pad_x = if self.big { 4.0 * px } else { 3.0 * px };
        let pad_y = if self.big { 3.0 * px } else { 2.0 * px };
        let content_h = (9.0 * px).max(galley.as_ref().map_or(0.0, |g| g.size().y));
        let size = Vec2::new(
            (icon_w + gap + text_w + 2.0 * pad_x).max(self.min_width),
            content_h + 2.0 * pad_y,
        );
        // L'ombre déborde d'un pas en bas et à droite.
        let (outer, response) = ui.allocate_exact_size(size + Vec2::splat(px), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        if !ui.is_rect_visible(outer) {
            return response;
        }

        let pressed = response.is_pointer_button_down_on();
        let hovered = response.hovered();
        let rest = Rect::from_min_size(outer.min, size);
        let body = if pressed { rest.translate(Vec2::splat(px)) } else { rest };
        let painter = ui.painter();

        if !pressed {
            block_rect(painter, rest.translate(Vec2::splat(px)), px, palette.shadow);
        }
        let (fill, edge) = self.colors(palette, hovered);
        block_rect(painter, body, px, edge);
        painter.rect_filled(body.shrink(px), CornerRadius::ZERO, fill);

        // Le contenu, centré.
        let content_w = icon_w + gap + text_w;
        let mut x = body.center().x - content_w / 2.0;
        if let Some(sprite) = self.icon {
            let y = body.center().y - 4.5 * px;
            draw_sprite(painter, Pos2::new(x, y), sprite, px, self.content_color(palette, hovered));
            x += icon_w + gap;
        }
        if let Some(galley) = galley {
            let y = body.center().y - galley.size().y / 2.0;
            painter.galley(Pos2::new(x, y).round(), galley, self.content_color(palette, hovered));
        }
        response
    }

    fn colors(&self, palette: &Palette, hovered: bool) -> (Color32, Color32) {
        if self.primary {
            (palette.accent, if hovered { palette.bright } else { palette.accent })
        } else if self.selected {
            (palette.surface, palette.accent)
        } else {
            (palette.surface, if hovered { palette.accent } else { palette.edge })
        }
    }

    fn content_color(&self, palette: &Palette, hovered: bool) -> Color32 {
        if self.primary {
            palette.on_accent
        } else if self.selected || hovered {
            palette.accent
        } else {
            palette.bright
        }
    }
}

/// Un rectangle plein aux coins coupés d'un pixel : la façon pixel d'arrondir.
fn block_rect(painter: &egui::Painter, rect: Rect, px: f32, color: Color32) {
    let (x0, y0, x1, y1) = (rect.min.x, rect.min.y, rect.max.x, rect.max.y);
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(x0 + px, y0), Pos2::new(x1 - px, y1)),
        CornerRadius::ZERO,
        color,
    );
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(x0, y0 + px), Pos2::new(x1, y1 - px)),
        CornerRadius::ZERO,
        color,
    );
}

/// Une barre de progression en blocs, qu'on peut saisir.
///
/// Rend la position cliquée, entre 0 et 1, quand il y en a une.
pub fn blocks_bar(ui: &mut Ui, ratio: f32, px: f32, palette: &Palette) -> (Response, Option<f32>) {
    let width = ui.available_width().max(6.0 * px);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, 3.0 * px), Sense::click_and_drag());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    // Deux pixels de bloc, un d'écart.
    let count = ((rect.width() + px) / (3.0 * px)).floor().max(1.0) as usize;
    let filled = (ratio.clamp(0.0, 1.0) * count as f32).round() as usize;
    let painter = ui.painter();
    for i in 0..count {
        let x = rect.min.x + i as f32 * 3.0 * px;
        let color = if i < filled { palette.accent } else { palette.edge };
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.min.y), Vec2::new(2.0 * px, 3.0 * px)),
            CornerRadius::ZERO,
            color,
        );
    }
    let picked = if response.clicked() || response.dragged() {
        response
            .interact_pointer_pos()
            .map(|pos| ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0))
    } else {
        None
    };
    (response, picked)
}

/// Des crans en escalier — le volume — dont on choisit la hauteur d'un clic.
///
/// Rend le cran choisi, de 1 à `steps`, quand il y en a un.
pub fn level(ui: &mut Ui, level: usize, steps: usize, px: f32, palette: &Palette) -> Option<usize> {
    let height = (steps as f32 + 1.0) * px;
    let width = steps as f32 * 3.0 * px - px;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    for i in 0..steps {
        let x = rect.min.x + i as f32 * 3.0 * px;
        let h = (i as f32 + 2.0) * px;
        let color = if i < level { palette.accent } else { palette.edge };
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.max.y - h), Vec2::new(2.0 * px, h)),
            CornerRadius::ZERO,
            color,
        );
    }
    if response.clicked() {
        response
            .interact_pointer_pos()
            .map(|pos| (((pos.x - rect.min.x) / (3.0 * px)).floor() as usize + 1).clamp(1, steps))
    } else {
        None
    }
}

/// Une ligne de menu : icône, libellé, et la touche qui fait pareil.
pub fn menu_item(
    ui: &mut Ui,
    icon: Option<&'static Sprite>,
    label: &str,
    shortcut: &str,
    palette: &Palette,
) -> Response {
    let px = step(ui);
    let body_size = egui::TextStyle::Body.resolve(ui.style()).size;
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, 9.0 * px + 2.0 * px), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, CornerRadius::ZERO, palette.surface);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(px, rect.height())),
            CornerRadius::ZERO,
            palette.accent,
        );
    }
    let color = if hovered { palette.accent } else { palette.bright };
    let mut x = rect.min.x + 3.0 * px;
    if let Some(sprite) = icon {
        draw_sprite(painter, Pos2::new(x, rect.center().y - 4.5 * px), sprite, px, color);
    }
    x += 9.0 * px + 3.0 * px;
    let galley = painter.layout_no_wrap(label.to_string(), fonts::pixel(body_size * 1.05), color);
    painter.galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0).round(), galley, color);
    if !shortcut.is_empty() {
        let key = painter.layout_no_wrap(
            shortcut.to_string(),
            egui::TextStyle::Small.resolve(ui.style()),
            palette.dim,
        );
        painter.galley(
            Pos2::new(rect.max.x - key.size().x - 2.0 * px, rect.center().y - key.size().y / 2.0)
                .round(),
            key,
            palette.dim,
        );
    }
    response
}

/// Un filet pixel, pour séparer les groupes d'un menu.
pub fn rule(ui: &mut Ui, palette: &Palette) {
    let px = step(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 3.0 * px), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 4.0 * px, px)),
        CornerRadius::ZERO,
        palette.edge,
    );
}
