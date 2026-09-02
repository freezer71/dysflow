//! La barre de lecture : cinq gestes en gros boutons, le reste dans « Plus ».
//!
//! Lecture, phrase précédente, phrase suivante, vitesse, volume : c'est tout
//! ce que la barre montre. Signets, sommaire, recherche, moteur et statistiques
//! attendent derrière un seul bouton, pour que l'œil qui revient du texte ne
//! trouve que ce qu'il cherche.

use eframe::egui::{self, Frame, Popup, PopupCloseBehavior, RichText, Stroke, Ui};

use super::fonts;
use super::pixel::{self, Button};
use super::theme::Palette;
use super::window::Gui;
use crate::app::config::Field;
use crate::app::{App, Panel};
use crate::models;

/// Ce que la barre demande à la fenêtre.
#[derive(Debug, Default, Clone, Copy)]
pub struct Actions {
    pub open_search: bool,
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let px = pixel::step(ui);
    let mut actions = Actions::default();
    ui.add_space(2.0 * px);
    ui.horizontal(|ui| {
        ui.add_space(px);
        match gui.app.startup_phase() {
            // Avant le premier son, la barre n'a rien à montrer : elle cède
            // la place à l'étape en cours.
            Some(phase) => {
                let elapsed = gui.app.startup_elapsed();
                ui.add(egui::Spinner::new().color(palette.accent));
                ui.label(
                    RichText::new(phase.label(&gui.app.config.engine.family))
                        .color(palette.normal),
                );
                ui.label(
                    RichText::new(format!("{:.1} s", elapsed.as_secs_f32())).color(palette.dim),
                );
            }
            None => progress(ui, gui, px, palette),
        }
        ui.add_space(px);
    });
    ui.add_space(2.0 * px);
    ui.horizontal(|ui| {
        ui.add_space(px);
        transport(ui, gui, palette);
        ui.add_space(4.0 * px);
        speed(ui, gui, palette);
        ui.add_space(4.0 * px);
        volume(ui, gui, px, palette);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(px);
            actions = more(ui, gui, palette);
        });
    });
    ui.add_space(2.0 * px);
    actions
}

/// La barre en blocs et le compteur de phrases ; les temps au survol.
fn progress(ui: &mut Ui, gui: &mut Gui, px: f32, palette: &Palette) {
    let total = gui.app.doc().sentences.len().max(1);
    let current = gui.app.current_sentence();
    let font = fonts::pixel(egui::TextStyle::Body.resolve(ui.style()).size * 1.05);
    let counter = format!("{} / {}", current + 1, total);
    let reserved = ui
        .painter()
        .layout_no_wrap(counter.clone(), font.clone(), palette.normal)
        .size()
        .x
        + 4.0 * px;

    let times = match (gui.app.estimated_elapsed(), gui.app.estimated_total()) {
        (Some(elapsed), Some(total_secs)) => {
            format!("{} écoulées · ~{} en tout", clock(elapsed), clock(total_secs))
        }
        // Aucune mesure encore : afficher un temps serait l'inventer.
        _ => "durée inconnue tant que rien n'a été synthétisé".to_string(),
    };
    let ratio = current as f32 / total as f32;
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width() - reserved);
        let (response, picked) = pixel::blocks_bar(ui, ratio, px, palette);
        response.on_hover_text(times);
        if let Some(at) = picked {
            gui.app.seek_to((at * total as f32) as usize);
        }
    });
    ui.add_space(2.0 * px);
    ui.label(RichText::new(counter).font(font).color(palette.normal));
}

/// Précédente, lecture, suivante : les trois gros boutons.
fn transport(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if Button::icon(&pixel::PREV)
        .big()
        .show(ui, palette)
        .on_hover_text("Phrase précédente  ←")
        .clicked()
    {
        gui.app.seek_sentences(-1);
    }
    let playing = gui.app.player.is_playing();
    let (icon, label) = if playing { (&pixel::PAUSE, "PAUSE") } else { (&pixel::PLAY, "LECTURE") };
    if Button::icon(icon)
        .label(label)
        .primary()
        .big()
        .min_width(150.0)
        .show(ui, palette)
        .on_hover_text("Espace")
        .clicked()
    {
        gui.app.toggle_play();
    }
    if Button::icon(&pixel::NEXT)
        .big()
        .show(ui, palette)
        .on_hover_text("Phrase suivante  →")
        .clicked()
    {
        gui.app.seek_sentences(1);
    }
}

/// Moins, la vitesse en clair, plus.
fn speed(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if Button::icon(&pixel::MINUS)
        .show(ui, palette)
        .on_hover_text("Moins vite  −")
        .clicked()
    {
        gui.app.change_speed(-0.1);
    }
    let shown = format!("{:.2}×", gui.app.config.voice.speed).replace(".", ",");
    let size = egui::TextStyle::Body.resolve(ui.style()).size * 1.15;
    ui.label(RichText::new(shown).font(fonts::pixel(size)).color(palette.bright));
    if Button::icon(&pixel::PLUS)
        .show(ui, palette)
        .on_hover_text("Plus vite  +")
        .clicked()
    {
        gui.app.change_speed(0.1);
    }
}

/// Le haut-parleur, et cinq crans.
fn volume(ui: &mut Ui, gui: &mut Gui, px: f32, palette: &Palette) {
    let muted = gui.app.player.is_muted();
    let icon = if muted { &pixel::MUTED } else { &pixel::SPEAKER };
    let hint = if muted { "Rétablir le son  m" } else { "Couper le son  m" };
    if Button::icon(icon)
        .selected(muted)
        .show(ui, palette)
        .on_hover_text(hint)
        .clicked()
    {
        gui.app.toggle_mute();
    }
    ui.add_space(px);
    let level = (gui.app.player.volume() * 5.0).round() as usize;
    if let Some(chosen) = pixel::level(ui, level, 5, px, palette) {
        gui.app.config_set_number(Field::Volume, Some(chosen as f64 / 5.0));
    }
}

/// Le bouton « Plus » et son menu : tout ce que la barre ne montre pas.
fn more(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    let response = Button::icon(&pixel::MORE)
        .show(ui, palette)
        .on_hover_text("Plus…");
    gui.more_rect = Some(response.rect);
    let frame = Frame::new()
        .fill(palette.panel)
        .stroke(Stroke::new(2.0, palette.edge))
        .inner_margin(6.0);
    Popup::menu(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClick)
        .frame(frame)
        .width(300.0)
        .show(|ui| {
            let here = gui.app.bookmarked_here();
            let bookmark = if here { "Retirer le signet" } else { "Poser un signet" };
            if pixel::menu_item(ui, Some(&pixel::FLAG), bookmark, "b", palette).clicked() {
                gui.app.toggle_bookmark();
            }
            if pixel::menu_item(ui, Some(&pixel::LIST), "Sommaire", "t", palette).clicked() {
                gui.app.toggle_panel(Panel::Contents);
            }
            if pixel::menu_item(ui, Some(&pixel::FLAG), "Signets", "B", palette).clicked() {
                gui.app.toggle_panel(Panel::Bookmarks);
            }
            if pixel::menu_item(ui, Some(&pixel::SEARCH), "Chercher", "/", palette).clicked() {
                actions.open_search = true;
            }
            if gui.app.search().is_some_and(|s| !s.is_empty()) {
                if pixel::menu_item(ui, None, "Résultat suivant", "N", palette).clicked() {
                    gui.app.search_advance(1);
                }
                if pixel::menu_item(ui, None, "Résultat précédent", "P", palette).clicked() {
                    gui.app.search_advance(-1);
                }
            }
            pixel::rule(ui, palette);
            if pixel::menu_item(ui, Some(&pixel::RESTART), "Reprendre la phrase", "r", palette)
                .clicked()
            {
                gui.app.restart_sentence();
            }
            if pixel::menu_item(ui, Some(&pixel::HOME), "Début du document", "g", palette)
                .clicked()
            {
                gui.app.seek_start();
            }
            if !gui.app.view.is_following()
                && pixel::menu_item(ui, Some(&pixel::FOLLOW), "Revenir à la lecture", "f", palette)
                    .clicked()
            {
                gui.app.follow_reading();
            }
            pixel::rule(ui, palette);
            if pixel::menu_item(ui, Some(&pixel::GEAR), "Réglages", "s", palette).clicked() {
                gui.app.toggle_config();
            }
            if pixel::menu_item(ui, Some(&pixel::HELP), "Aide", "?", palette).clicked() {
                gui.app.show_help = true;
            }
            pixel::rule(ui, palette);
            // Le moteur et son avance : à savoir, pas à surveiller.
            let stats = gui.app.synth.stats();
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
                    RichText::new(format!(
                        "⚠ {} phrase(s) refusée(s) par le moteur",
                        stats.failed
                    ))
                    .small()
                    .color(palette.accent),
                );
            }
        });
    actions
}

/// La ligne d'état : elle ne parle que quand elle a quelque chose à dire.
///
/// Sa hauteur ne change pas pour autant : une barre qui apparaît et disparaît
/// ferait sauter le texte au-dessus, et l'œil qui le suit avec.
pub fn status(ui: &mut Ui, app: &mut App, palette: &Palette, opening: Option<&str>) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let on_fallback = !models::available().is_empty() && models::first_installed().is_none();
        if let Some(name) = opening {
            ui.add(egui::Spinner::new().color(palette.accent));
            ui.label(RichText::new(format!("extraction de {name}…")).color(palette.accent));
        } else if app.reference_missing() {
            // Un modèle qui ne sait que cloner et n'a pas de référence ne
            // produira jamais un son : le dire ici, en permanence.
            badge(ui, palette, "aucune voix de référence");
            ui.label(
                RichText::new("ce modèle clone : Réglages › Voix pour désigner un WAV")
                    .color(palette.normal),
            );
            return;
        } else if !app.view.is_following() {
            // La vue figée est un état, pas un événement : il survit au
            // message qui l'a annoncé.
            badge(ui, palette, "vue figée");
            if Button::icon(&pixel::FOLLOW)
                .label("Revenir à la lecture")
                .show(ui, palette)
                .on_hover_text("f")
                .clicked()
            {
                app.follow_reading();
            }
        } else if !app.status().is_empty() {
            ui.label(RichText::new(app.status().to_string()).color(palette.accent));
        } else if on_fallback {
            badge(ui, palette, "voix système");
            ui.label(
                RichText::new("Réglages › Modèles pour installer un vrai modèle")
                    .color(palette.normal),
            );
        } else if let Some(notice) = app.notices.last() {
            ui.label(RichText::new(notice.clone()).color(palette.dim));
        }
        if let Some(search) = app.search() {
            if !search.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "« {} »  {} / {}",
                        search.query,
                        search.position(),
                        search.count()
                    ))
                    .color(palette.normal),
                );
            }
        }
        if app.skipped_count() > 0 {
            ui.label(
                RichText::new(format!("{} phrase(s) sautée(s)", app.skipped_count()))
                    .color(palette.dim),
            );
        }
        if app.is_finished() && !app.doc().is_empty() {
            ui.label(RichText::new("lecture terminée").color(palette.dim));
        }
    });
}

/// Un mot sur fond accent : l'état qui doit se voir.
fn badge(ui: &mut Ui, palette: &Palette, text: &str) {
    let px = pixel::step(ui);
    let font = fonts::pixel(egui::TextStyle::Body.resolve(ui.style()).size * 1.05);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, palette.on_accent);
    let (rect, _) = ui.allocate_exact_size(
        galley.size() + egui::Vec2::new(4.0 * px, 2.0 * px),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, palette.accent);
    ui.painter().galley(
        (rect.min + egui::Vec2::new(2.0 * px, px)).round(),
        galley,
        palette.on_accent,
    );
}

/// Formate des secondes en `M:SS` ou `H:MM:SS`.
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
