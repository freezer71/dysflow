//! Icônes au trait : tracées par le peintre, nettes à toute échelle.
//!
//! Chaque icône se dessine dans une case de seize unités, remise à l'échelle
//! du rectangle qu'on lui donne. Un trait fin, pas de police de symboles : la
//! police de lecture n'a pas ces glyphes, et une icône dessinée ne dépend de
//! rien.

use std::f32::consts::TAU;

use eframe::egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
};

/// Taille courante d'une icône, en points.
pub const SIZE: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Prev,
    Next,
    Search,
    Close,
    Gear,
    Note,
    Bookmark,
    BookmarkFilled,
    Folder,
    Clipboard,
    Speaker,
    Muted,
    More,
    Help,
    Sidebar,
    Restart,
    Home,
    Follow,
    Plus,
    Minus,
    Check,
    Marker,
    Trash,
    Download,
    ChevronRight,
}

/// Dessine `icon` dans `rect`, en `color`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 16.0;
    let origin = rect.center() - Vec2::splat(8.0 * scale);
    let p = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
    let thin = Stroke::new((1.5 * scale).max(1.0), color);
    let thick = Stroke::new((2.0 * scale).max(1.0), color);
    let poly = |points: &[(f32, f32)]| -> Vec<Pos2> { points.iter().map(|&(x, y)| p(x, y)).collect() };
    let filled = |points: &[(f32, f32)]| Shape::convex_polygon(poly(points), color, Stroke::NONE);
    let outline = |points: &[(f32, f32)]| Shape::closed_line(poly(points), thin);

    match icon {
        Icon::Play => {
            painter.add(filled(&[(4.5, 2.5), (4.5, 13.5), (13.5, 8.0)]));
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_two_pos(p(3.5, 2.5), p(6.5, 13.5)), 1.0 * scale, color);
            painter.rect_filled(Rect::from_two_pos(p(9.5, 2.5), p(12.5, 13.5)), 1.0 * scale, color);
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_two_pos(p(3.5, 3.5), p(12.5, 12.5)), 1.5 * scale, color);
        }
        Icon::Prev => {
            painter.line_segment([p(3.5, 3.0), p(3.5, 13.0)], thick);
            painter.add(filled(&[(13.0, 3.0), (13.0, 13.0), (6.0, 8.0)]));
        }
        Icon::Next => {
            painter.line_segment([p(12.5, 3.0), p(12.5, 13.0)], thick);
            painter.add(filled(&[(3.0, 3.0), (3.0, 13.0), (10.0, 8.0)]));
        }
        Icon::Search => {
            painter.circle_stroke(p(7.0, 7.0), 4.5 * scale, thin);
            painter.line_segment([p(10.4, 10.4), p(14.0, 14.0)], thick);
        }
        Icon::Close => {
            painter.line_segment([p(4.0, 4.0), p(12.0, 12.0)], thin);
            painter.line_segment([p(12.0, 4.0), p(4.0, 12.0)], thin);
        }
        Icon::Gear => {
            painter.circle_stroke(p(8.0, 8.0), 2.5 * scale, thin);
            for i in 0..8 {
                let angle = i as f32 * TAU / 8.0;
                let dir = Vec2::angled(angle);
                painter.line_segment([p(8.0, 8.0) + dir * 4.6 * scale, p(8.0, 8.0) + dir * 6.8 * scale], thick);
            }
        }
        Icon::Note => {
            painter.add(outline(&[(3.5, 2.0), (10.0, 2.0), (12.5, 4.5), (12.5, 14.0), (3.5, 14.0)]));
            painter.line_segment([p(10.0, 2.0), p(10.0, 4.5)], thin);
            painter.line_segment([p(10.0, 4.5), p(12.5, 4.5)], thin);
            painter.line_segment([p(6.0, 8.0), p(10.0, 8.0)], thin);
            painter.line_segment([p(6.0, 11.0), p(10.0, 11.0)], thin);
        }
        Icon::Bookmark => {
            painter.add(outline(&[(4.0, 2.0), (12.0, 2.0), (12.0, 14.0), (8.0, 10.5), (4.0, 14.0)]));
        }
        Icon::BookmarkFilled => {
            // Le ruban n'est pas convexe : deux moitiés qui le sont.
            painter.add(filled(&[(4.0, 2.0), (8.0, 2.0), (8.0, 10.5), (4.0, 14.0)]));
            painter.add(filled(&[(8.0, 2.0), (12.0, 2.0), (12.0, 14.0), (8.0, 10.5)]));
        }
        Icon::Folder => {
            painter.add(outline(&[(2.0, 4.0), (6.5, 4.0), (8.0, 6.0), (14.0, 6.0), (14.0, 13.0), (2.0, 13.0)]));
        }
        Icon::Clipboard => {
            painter.rect_stroke(Rect::from_two_pos(p(3.5, 3.0), p(12.5, 14.0)), 1.5 * scale, thin, StrokeKind::Middle);
            painter.rect_filled(Rect::from_two_pos(p(6.0, 1.5), p(10.0, 4.0)), 1.0 * scale, color);
            painter.line_segment([p(6.0, 8.0), p(10.0, 8.0)], thin);
            painter.line_segment([p(6.0, 10.5), p(10.0, 10.5)], thin);
        }
        Icon::Speaker | Icon::Muted => {
            painter.add(filled(&[(2.5, 6.0), (5.0, 6.0), (8.5, 3.0), (8.5, 13.0), (5.0, 10.0), (2.5, 10.0)]));
            if icon == Icon::Speaker {
                painter.line(arc(p(8.5, 8.0), 3.0 * scale, -0.22 * TAU, 0.22 * TAU), thin);
                painter.line(arc(p(8.5, 8.0), 5.5 * scale, -0.2 * TAU, 0.2 * TAU), thin);
            } else {
                painter.line_segment([p(10.5, 6.0), p(14.5, 10.0)], thin);
                painter.line_segment([p(14.5, 6.0), p(10.5, 10.0)], thin);
            }
        }
        Icon::More => {
            for x in [3.5, 8.0, 12.5] {
                painter.circle_filled(p(x, 8.0), 1.4 * scale, color);
            }
        }
        Icon::Help => {
            painter.circle_stroke(p(8.0, 8.0), 6.5 * scale, thin);
            painter.text(
                p(8.0, 8.1),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(10.5 * scale),
                color,
            );
        }
        Icon::Sidebar => {
            painter.rect_stroke(Rect::from_two_pos(p(2.0, 3.0), p(14.0, 13.0)), 1.5 * scale, thin, StrokeKind::Middle);
            painter.line_segment([p(6.0, 3.0), p(6.0, 13.0)], thin);
        }
        Icon::Restart => {
            let points = arc(p(8.0, 8.5), 5.0 * scale, -0.42 * TAU, 0.33 * TAU);
            // Une pointe de flèche au départ de l'arc.
            let tip = points[0];
            painter.line(points, thin);
            painter.line_segment([tip, tip + Vec2::new(-2.6, -0.6) * scale], thin);
            painter.line_segment([tip, tip + Vec2::new(0.4, -2.8) * scale], thin);
        }
        Icon::Home => {
            painter.line(poly(&[(2.5, 8.0), (8.0, 3.0), (13.5, 8.0)]), thin);
            painter.line(poly(&[(4.0, 7.0), (4.0, 13.5), (12.0, 13.5), (12.0, 7.0)]), thin);
        }
        Icon::Follow => {
            painter.circle_stroke(p(8.0, 8.0), 5.0 * scale, thin);
            painter.circle_filled(p(8.0, 8.0), 1.8 * scale, color);
            painter.line_segment([p(8.0, 1.5), p(8.0, 4.0)], thin);
            painter.line_segment([p(8.0, 12.0), p(8.0, 14.5)], thin);
            painter.line_segment([p(1.5, 8.0), p(4.0, 8.0)], thin);
            painter.line_segment([p(12.0, 8.0), p(14.5, 8.0)], thin);
        }
        Icon::Plus => {
            painter.line_segment([p(3.0, 8.0), p(13.0, 8.0)], thick);
            painter.line_segment([p(8.0, 3.0), p(8.0, 13.0)], thick);
        }
        Icon::Minus => {
            painter.line_segment([p(3.0, 8.0), p(13.0, 8.0)], thick);
        }
        Icon::Check => {
            painter.line(poly(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)]), thick);
        }
        Icon::Marker => {
            painter.circle_stroke(p(8.0, 6.0), 4.0 * scale, thin);
            painter.circle_filled(p(8.0, 6.0), 1.4 * scale, color);
            painter.line_segment([p(8.0, 10.0), p(8.0, 14.5)], thin);
        }
        Icon::Trash => {
            painter.line_segment([p(3.0, 4.0), p(13.0, 4.0)], thin);
            painter.line(poly(&[(6.5, 4.0), (6.5, 2.5), (9.5, 2.5), (9.5, 4.0)]), thin);
            painter.line(poly(&[(4.0, 4.0), (4.8, 14.0), (11.2, 14.0), (12.0, 4.0)]), thin);
            painter.line_segment([p(7.0, 6.5), p(7.0, 11.5)], thin);
            painter.line_segment([p(9.0, 6.5), p(9.0, 11.5)], thin);
        }
        Icon::Download => {
            painter.line_segment([p(8.0, 2.0), p(8.0, 10.0)], thin);
            painter.line(poly(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)]), thin);
            painter.line(poly(&[(2.5, 10.5), (2.5, 14.0), (13.5, 14.0), (13.5, 10.5)]), thin);
        }
        Icon::ChevronRight => {
            painter.line(poly(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]), thin);
        }
    }
}

/// Les points d'un arc, du plus petit angle au plus grand.
fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 14;
    (0..=n)
        .map(|i| {
            let t = from + (to - from) * i as f32 / n as f32;
            center + Vec2::angled(t) * radius
        })
        .collect()
}
