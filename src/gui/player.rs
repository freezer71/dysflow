//! La barre de lecture et la ligne d'état : la même chose qu'au terminal, en widgets.

use eframe::egui::{self, Pos2, Rect, RichText, Sense, Slider, Ui, Vec2};

use super::theme::Palette;
use crate::app::config::Field;
use crate::app::App;
use crate::models;

pub fn show(ui: &mut Ui, app: &mut App, palette: &Palette) {
    ui.add_space(6.0);
    ui.horizontal(|ui| match app.startup_phase() {
        // Avant le premier son, la barre de progression n'a rien à montrer :
        // elle cède la place à l'étape en cours.
        Some(phase) => {
            let elapsed = app.startup_elapsed();
            ui.add(egui::Spinner::new().color(palette.accent));
            ui.label(RichText::new(phase.label(&app.config.engine.family)).color(palette.normal));
            ui.label(RichText::new(format!("{:.1} s", elapsed.as_secs_f32())).color(palette.dim));
        }
        None => transport(ui, app, palette),
    });
    ui.horizontal(|ui| controls(ui, app, palette));
    ui.add_space(4.0);
}

/// Lecture, progression, temps.
fn transport(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let playing = app.player.is_playing();
    let icon = if playing { "⏸" } else { "▶" };
    if ui
        .add(
            egui::Button::new(RichText::new(icon).color(palette.on_accent).strong())
                .fill(palette.accent),
        )
        .on_hover_text("Espace")
        .clicked()
    {
        app.toggle_play();
    }

    let total = app.doc().sentences.len().max(1);
    let current = app.current_sentence();
    let times = match (app.estimated_elapsed(), app.estimated_total()) {
        (Some(elapsed), Some(total_secs)) => {
            format!("{} / ~{}", clock(elapsed), clock(total_secs))
        }
        // Aucune mesure encore : afficher un temps serait l'inventer.
        _ => "--:-- / --:--".to_string(),
    };
    let right = format!("{times}   {}/{}", current + 1, total);
    let right_w = 200.0;
    let bar_w = (ui.available_width() - right_w - 16.0).max(60.0);

    // Une barre qu'on peut saisir : cliquer y déplace la lecture.
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(bar_w, 18.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 6.0));
    ui.painter().rect_filled(track, 3.0, palette.faint);
    let ratio = (current as f32 / total as f32).clamp(0.0, 1.0);
    let filled = Rect::from_min_size(track.min, Vec2::new(track.width() * ratio, track.height()));
    ui.painter().rect_filled(filled, 3.0, palette.accent);
    ui.painter()
        .circle_filled(Pos2::new(filled.max.x, track.center().y), 6.0, palette.accent);
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let at = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            app.seek_to((at * total as f32) as usize);
        }
    }
    ui.label(RichText::new(right).color(palette.normal));
}

/// Vitesse, volume, moteur, avance de synthèse, signet.
fn controls(ui: &mut Ui, app: &mut App, palette: &Palette) {
    if ui.button("−").on_hover_text("-").clicked() {
        app.change_speed(-0.1);
    }
    ui.label(
        RichText::new(format!("{:.2}×", app.config.voice.speed))
            .strong()
            .color(palette.bright),
    );
    if ui.button("+").on_hover_text("+").clicked() {
        app.change_speed(0.1);
    }
    for (key, speed, label) in [
        ("1", 0.75, "0,75×"),
        ("2", 1.0, "1×"),
        ("3", 1.5, "1,5×"),
        ("4", 2.0, "2×"),
    ] {
        let selected = (app.config.voice.speed - speed).abs() < 0.01;
        if ui.selectable_label(selected, label).on_hover_text(key).clicked() {
            app.set_speed(speed);
        }
    }
    ui.separator();

    let muted = app.player.is_muted();
    if ui
        .selectable_label(muted, if muted { "🔇" } else { "🔊" })
        .on_hover_text("m — couper le son")
        .clicked()
    {
        app.toggle_mute();
    }
    let before = f64::from(app.player.volume());
    let mut volume = before;
    let slider = ui.add(Slider::new(&mut volume, 0.0..=1.0).show_value(false).step_by(0.05));
    // Le curseur arrondit au pas dès le premier dessin : seul un vrai
    // déplacement compte.
    if slider.changed() && (volume - before).abs() > 1e-6 {
        app.config_set_number(Field::Volume, Some(volume));
    }
    ui.label(
        RichText::new(format!("{:>3.0} %", app.player.volume() * 100.0)).color(palette.normal),
    );
    ui.separator();

    ui.label(RichText::new(app.engine_name()).color(palette.dim));
    // L'avance de synthèse est visible : on sait si le moteur suit.
    let stats = app.synth.stats();
    ui.label(
        RichText::new(format!(
            "synthèse · {} prêtes · {} en cours · {} sessions",
            stats.ready, stats.in_flight, stats.workers
        ))
        .color(palette.faint),
    );
    if stats.failed > 0 {
        ui.label(RichText::new(format!("⚠ {} échec(s)", stats.failed)).color(palette.accent));
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let here = app.bookmarked_here();
        if ui.selectable_label(here, "⚑ signet").on_hover_text("b").clicked() {
            app.toggle_bookmark();
        }
    });
}

/// La ligne d'état : les mêmes priorités qu'au terminal.
pub fn status(ui: &mut Ui, app: &mut App, palette: &Palette, opening: Option<&str>) {
    ui.horizontal_wrapped(|ui| {
        let on_fallback = !models::available().is_empty() && models::first_installed().is_none();
        if let Some(name) = opening {
            ui.add(egui::Spinner::new().color(palette.accent));
            ui.label(RichText::new(format!("extraction de {name}…")).color(palette.accent));
        } else if app.reference_missing() {
            // Un modèle qui ne sait que cloner et n'a pas de référence ne
            // produira jamais un son : le dire ici, en permanence.
            badge(ui, palette, "⚠ aucune voix de référence");
            ui.label(
                RichText::new("ce modèle clone : Réglages › Voix pour désigner un WAV")
                    .color(palette.normal),
            );
            return;
        } else if !app.view.is_following() {
            // La vue figée est un état, pas un événement : il survit au message
            // qui l'a annoncé.
            badge(ui, palette, "⏸ vue figée");
            if ui.button("↧ revenir à la lecture").on_hover_text("f").clicked() {
                app.follow_reading();
            }
        } else if !app.status().is_empty() {
            ui.label(RichText::new(app.status().to_string()).color(palette.accent));
        } else if on_fallback {
            badge(ui, palette, "⚠ voix système");
            ui.label(
                RichText::new("Réglages › Modèles pour installer un vrai modèle")
                    .color(palette.normal),
            );
        } else if let Some(notice) = app.notices.last() {
            ui.label(RichText::new(notice.clone()).color(palette.dim));
        } else {
            ui.label(
                RichText::new(
                    "s réglages · ? aide · espace lecture · / chercher · n/p titre · b signet · clic sur un mot",
                )
                .color(palette.faint),
            );
        }
        if let Some(search) = app.search() {
            if !search.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "· « {} » {}/{}",
                        search.query,
                        search.position(),
                        search.count()
                    ))
                    .color(palette.normal),
                );
            }
        }
        // Quelle marque de ponctuation le lecteur est en train de dire.
        if let Some(index) = app.current_word() {
            let punctuation = app
                .doc()
                .sentence(app.current_sentence())
                .and_then(|s| s.words.get(index))
                .and_then(|w| w.punctuation);
            if let Some(p) = punctuation {
                ui.label(RichText::new(format!("· {}", p.label())).color(palette.accent).strong());
            }
        }
        if app.skipped_count() > 0 {
            ui.label(
                RichText::new(format!("· {} phrase(s) sautée(s)", app.skipped_count()))
                    .color(palette.dim),
            );
        }
        if app.is_finished() && !app.doc().is_empty() {
            ui.label(RichText::new("· lecture terminée").color(palette.dim));
        }
    });
}

/// Un mot sur fond accent : l'état qui doit se voir.
fn badge(ui: &mut Ui, palette: &Palette, text: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, palette.on_accent);
    let (rect, _) = ui.allocate_exact_size(galley.size() + Vec2::new(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, palette.accent);
    ui.painter().galley(rect.min + Vec2::new(6.0, 2.0), galley, palette.on_accent);
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
