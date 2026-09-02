//! La carte de lecture : lecture, la phrase en cours, la ligne de temps.
//!
//! Une carte posée sous le texte : le bouton de lecture, le début de la
//! phrase que la voix dit, le temps, une ligne que l'on glisse pour avancer
//! ou reculer, et à droite les petits gestes — phrase précédente et suivante,
//! vitesse, volume, le reste derrière `⋯`.

use eframe::egui::{
    self, Align, CornerRadius, Frame, Layout, Popup, PopupCloseBehavior, RichText, Sense, Slider,
    Stroke, Ui, Vec2,
};

use super::icons::Icon;
use super::theme::Palette;
use super::widgets::{self, Button};
use super::window::Gui;
use super::timeline;
use crate::app::config::Field;
use crate::models;

/// Ce que la carte demande à la fenêtre.
#[derive(Debug, Default, Clone, Copy)]
pub struct Actions {
    pub open_search: bool,
    pub add_marker: bool,
}

/// Hauteur de la ligne des pastilles, au-dessus de la carte : fixe, pour que
/// le texte ne saute pas quand un message vient ou s'en va.
const PILLS_H: f32 = 30.0;

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette, opening: Option<&str>) -> Actions {
    let mut actions = Actions::default();
    pills(ui, gui, palette, opening);
    let card = Frame::new()
        .fill(palette.card)
        .stroke(Stroke::new(1.0, palette.edge))
        .corner_radius(CornerRadius::same(10))
        .shadow(ui.visuals().window_shadow)
        .inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 10 });
    card.show(ui, |ui| {
        ui.horizontal(|ui| {
            let playing = gui.app.player.is_playing();
            let icon = if playing { Icon::Pause } else { Icon::Play };
            let hint = if playing { "Pause  Espace" } else { "Lecture  Espace" };
            if Button::icon(icon).primary().round().large().show(ui, palette).on_hover_text(hint).clicked() {
                gui.app.toggle_play();
            }
            ui.add_space(4.0);
            // Les petits gestes à droite prennent une largeur connue ; le
            // texte du milieu prend le reste.
            let controls_w = 5.0 * 28.0 + 4.0 * 4.0 + 48.0;
            let middle_w = (ui.available_width() - controls_w).max(80.0);
            ui.allocate_ui_with_layout(Vec2::new(middle_w, 36.0), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                middle(ui, gui, palette, middle_w);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                actions = more(ui, gui, palette);
                volume(ui, gui, palette);
                speed(ui, gui, palette);
                if Button::icon(Icon::Next).show(ui, palette).on_hover_text("Phrase suivante  →").clicked() {
                    gui.app.seek_sentences(1);
                }
                if Button::icon(Icon::Prev).show(ui, palette).on_hover_text("Phrase précédente  ←").clicked() {
                    gui.app.seek_sentences(-1);
                }
            });
        });
        ui.add_space(6.0);
        line(ui, gui, palette);
    });
    actions
}

/// La phrase en cours et le temps — ou l'étape de démarrage.
fn middle(ui: &mut Ui, gui: &mut Gui, palette: &Palette, width: f32) {
    if let Some(phase) = gui.app.startup_phase() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0).color(palette.accent));
            ui.label(RichText::new(phase.label(&gui.app.config.engine.family)).color(palette.normal));
        });
        ui.label(
            RichText::new(format!("{:.1} s", gui.app.startup_elapsed().as_secs_f32()))
                .small()
                .color(palette.dim),
        );
        return;
    }
    let total = gui.app.doc().sentences.len();
    let current = gui.app.current_sentence();
    let snippet = gui
        .app
        .doc()
        .sentences
        .get(current)
        .map(|s| s.text.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let snippet = clip(&snippet, width);
    ui.label(RichText::new(snippet).color(palette.bright));
    let times = match (gui.app.estimated_elapsed(), gui.app.estimated_total()) {
        (Some(elapsed), Some(all)) => {
            format!("{}  /  ~{}", timeline::clock(elapsed), timeline::clock(all))
        }
        _ => format!("phrase {} sur {}", current + 1, total.max(1)),
    };
    let mut extra = String::new();
    if gui.app.is_finished() && total > 0 {
        extra.push_str("  ·  terminé");
    } else if gui.app.skipped_count() > 0 {
        extra.push_str(&format!("  ·  {} sautée(s)", gui.app.skipped_count()));
    }
    ui.label(RichText::new(format!("{times}{extra}")).small().color(palette.dim));
}

/// Coupe une phrase à ce qui tient sur une ligne de `width` points.
fn clip(text: &str, width: f32) -> String {
    let max = ((width / (super::theme::UI_SIZE * 0.5)) as usize).max(12);
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// La ligne de temps : un point par phrase, un repère par section.
fn line(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    let total = gui.app.doc().sentences.len();
    let current = gui.app.current_sentence();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let rail = egui::Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 12.0, 4.0));
    let at = |x: f32| ((x - rail.min.x) / rail.width()).clamp(0.0, 1.0);
    let ratio = timeline::ratio(current, total);
    let dragging = response.dragged();
    let shown = response
        .interact_pointer_pos()
        .filter(|_| dragging)
        .map(|p| at(p.x))
        .unwrap_or(ratio);
    let hovered = response.hovered() || dragging;
    let painter = ui.painter();
    painter.rect_filled(rail, 2.0, palette.surface);
    let done = egui::Rect::from_min_max(rail.min, egui::pos2(rail.min.x + rail.width() * shown, rail.max.y));
    painter.rect_filled(done, 2.0, palette.accent);
    let firsts = gui.app.doc().sections.iter().filter_map(|s| s.first_sentence);
    for tick in timeline::ticks(firsts, total) {
        let x = rail.min.x + rail.width() * tick;
        painter.vline(x, (rail.min.y - 3.0)..=(rail.max.y + 3.0), Stroke::new(1.0, palette.dim));
    }
    let knob = egui::pos2(rail.min.x + rail.width() * shown, rail.center().y);
    painter.circle_filled(knob, if hovered { 7.0 } else { 5.5 }, palette.accent);
    painter.circle_stroke(knob, if hovered { 7.0 } else { 5.5 }, Stroke::new(1.5, palette.card));

    if response.clicked() || response.drag_stopped() {
        if let Some(pos) = response.interact_pointer_pos() {
            gui.app.seek_to(timeline::sentence_at(at(pos.x), total));
        }
    }
    if let Some(pos) = response.hover_pos() {
        let target = timeline::sentence_at(at(pos.x), total);
        let section = gui
            .app
            .doc()
            .section_of_sentence(target)
            .map(|s| s.title.clone())
            .unwrap_or_default();
        let text = if section.is_empty() {
            format!("phrase {} sur {}", target + 1, total.max(1))
        } else {
            format!("{section}\nphrase {} sur {}", target + 1, total.max(1))
        };
        response.on_hover_text(text);
    }
}

/// La vitesse : un mot, et un menu de crans.
fn speed(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    let value = gui.app.config.voice.speed;
    let shown = speed_label(value);
    let response = Button::text(&shown).min_width(48.0).show(ui, palette).on_hover_text("Vitesse  − +");
    let frame = menu_frame(ui, palette);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClick)
        .frame(frame)
        .width(150.0)
        .show(|ui| {
            for preset in [0.75_f32, 1.0, 1.25, 1.5, 2.0] {
                let label = speed_label(preset);
                let icon = ((preset - value).abs() < 0.01).then_some(Icon::Check);
                if widgets::menu_item(ui, palette, icon, &label, "").clicked() {
                    gui.app.set_speed(preset);
                }
            }
            widgets::rule(ui, palette);
            if widgets::menu_item(ui, palette, Some(Icon::Minus), "Moins vite", "−").clicked() {
                gui.app.change_speed(-0.1);
            }
            if widgets::menu_item(ui, palette, Some(Icon::Plus), "Plus vite", "+").clicked() {
                gui.app.change_speed(0.1);
            }
        });
}

fn speed_label(speed: f32) -> String {
    let text = if (speed * 100.0).round() % 10.0 == 0.0 {
        format!("{:.1}×", speed)
    } else {
        format!("{:.2}×", speed)
    };
    text.replace(".", ",")
}

/// Le haut-parleur : un curseur et le silence.
fn volume(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    let muted = gui.app.player.is_muted();
    let icon = if muted { Icon::Muted } else { Icon::Speaker };
    let response = Button::icon(icon).selected(muted).show(ui, palette).on_hover_text("Volume  [ ]  ·  m");
    let frame = menu_frame(ui, palette);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .frame(frame)
        .width(210.0)
        .show(|ui| {
            let mut percent = (gui.app.player.volume() * 100.0).round();
            let before = percent;
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.spacing_mut().slider_width = 150.0;
                if ui
                    .add(Slider::new(&mut percent, 0.0..=100.0).step_by(5.0).fixed_decimals(0).suffix(" %"))
                    .changed()
                    && (percent - before).abs() > 0.5
                {
                    gui.app.config_set_number(Field::Volume, Some(f64::from(percent) / 100.0));
                }
            });
            let label = if muted { "Rétablir le son" } else { "Couper le son" };
            if widgets::menu_item(ui, palette, Some(if muted { Icon::Speaker } else { Icon::Muted }), label, "m")
                .clicked()
            {
                gui.app.toggle_mute();
            }
        });
}

fn menu_frame(ui: &Ui, palette: &Palette) -> Frame {
    Frame::new()
        .fill(palette.card)
        .stroke(Stroke::new(1.0, palette.edge))
        .corner_radius(CornerRadius::same(8))
        .shadow(ui.visuals().popup_shadow)
        .inner_margin(6.0)
}

/// Le bouton `⋯` et son menu : ce que la carte ne montre pas.
fn more(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    let response = Button::icon(Icon::More).show(ui, palette).on_hover_text("Plus…");
    gui.more_rect = Some(response.rect);
    let frame = menu_frame(ui, palette);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClick)
        .frame(frame)
        .width(280.0)
        .show(|ui| {
            let here = gui.app.bookmarked_here();
            let (icon, label) = if here {
                (Icon::BookmarkFilled, "Retirer le signet")
            } else {
                (Icon::Bookmark, "Poser un signet")
            };
            if widgets::menu_item(ui, palette, Some(icon), label, "b").clicked() {
                gui.app.toggle_bookmark();
            }
            if widgets::menu_item(ui, palette, Some(Icon::Marker), "Repère dans la note", "").clicked() {
                actions.add_marker = true;
            }
            if widgets::menu_item(ui, palette, Some(Icon::Search), "Chercher", "/").clicked() {
                actions.open_search = true;
            }
            if gui.app.search().is_some_and(|s| !s.is_empty()) {
                if widgets::menu_item(ui, palette, None, "Résultat suivant", "N").clicked() {
                    gui.app.search_advance(1);
                }
                if widgets::menu_item(ui, palette, None, "Résultat précédent", "P").clicked() {
                    gui.app.search_advance(-1);
                }
            }
            widgets::rule(ui, palette);
            if widgets::menu_item(ui, palette, Some(Icon::Restart), "Reprendre la phrase", "r").clicked() {
                gui.app.restart_sentence();
            }
            if widgets::menu_item(ui, palette, Some(Icon::Home), "Début du document", "g").clicked() {
                gui.app.seek_start();
            }
            if !gui.app.view.is_following()
                && widgets::menu_item(ui, palette, Some(Icon::Follow), "Revenir à la lecture", "f").clicked()
            {
                gui.app.follow_reading();
            }
            widgets::rule(ui, palette);
            if widgets::menu_item(ui, palette, Some(Icon::Gear), "Réglages", "s").clicked() {
                gui.app.toggle_config();
            }
            if widgets::menu_item(ui, palette, Some(Icon::Help), "Aide", "?").clicked() {
                gui.app.show_help = true;
            }
            widgets::rule(ui, palette);
            // Le moteur et son avance : à savoir, pas à surveiller.
            let stats = gui.app.synth.stats();
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(gui.app.engine_name()).small().color(palette.dim));
                    ui.label(
                        RichText::new(format!(
                            "{} prêtes · {} en cours · {} session{}",
                            stats.ready,
                            stats.in_flight,
                            stats.workers,
                            if stats.workers > 1 { "s" } else { "" }
                        ))
                        .small()
                        .color(palette.faint),
                    );
                    if stats.failed > 0 {
                        ui.label(
                            RichText::new(format!("{} phrase(s) refusée(s) par le moteur", stats.failed))
                                .small()
                                .color(palette.accent),
                        );
                    }
                    if let Some(notice) = gui.app.notices.last() {
                        ui.label(RichText::new(notice.clone()).small().color(palette.faint));
                    }
                });
            });
            ui.add_space(4.0);
        });
    actions
}

/// Les pastilles au-dessus de la carte : ce qui vaut d'être dit, et rien
/// quand il n'y a rien à dire — la place reste.
fn pills(ui: &mut Ui, gui: &mut Gui, palette: &Palette, opening: Option<&str>) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), PILLS_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let on_fallback = !models::available().is_empty() && models::first_installed().is_none();
            if let Some(name) = opening {
                ui.add(egui::Spinner::new().size(14.0).color(palette.accent));
                widgets::pill(ui, palette, &format!("extraction de {name}…"), true);
            } else if gui.app.reference_missing() {
                widgets::pill(ui, palette, "aucune voix de référence", true)
                    .on_hover_text("Ce modèle clone une voix : Réglages › Voix pour désigner un WAV.");
            } else if !gui.app.status().is_empty() && !gui.app.status().contains("figée") {
                // La vue figée a sa pastille à elle, juste après.
                widgets::pill(ui, palette, &gui.app.status().to_string(), true);
            } else if on_fallback {
                widgets::pill(ui, palette, "voix système", false)
                    .on_hover_text("Réglages › Modèles pour installer un vrai modèle.");
            }
            if !gui.app.view.is_following() {
                widgets::pill(ui, palette, "vue figée", false);
                if Button::icon(Icon::Follow)
                    .label("Suivre la lecture")
                    .small()
                    .show(ui, palette)
                    .on_hover_text("f")
                    .clicked()
                {
                    gui.app.follow_reading();
                }
            }
            if let Some(search) = gui.app.search() {
                if !search.is_empty() {
                    let text = format!("« {} »  {} / {}", search.query, search.position(), search.count());
                    widgets::pill(ui, palette, &text, false);
                }
            }
        },
    );
}
