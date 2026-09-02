//! Rendu de la fenêtre hors écran, pour la regarder sans écran.
//!
//! Une fenêtre occultée — écran éteint, session verrouillée — n'est jamais
//! dessinée, et la capture qu'elle fait d'elle-même n'arrive pas. Ces essais
//! rendent les mêmes zones dans une image, par le GPU mais sans fenêtre, et la
//! posent dans `target/snapshots/`. Ils ne vérifient rien d'eux-mêmes : c'est
//! l'œil qui juge, et ils restent ignorés par défaut.
//!
//! ```bash
//! cargo test --features snapshots gui::snapshot -- --ignored
//! ```

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Pos2};
use egui_kittest::Harness;

use super::window::Gui;
use crate::app::tests::app_reading;
use crate::app::App;

const DEMO: &str = include_str!("../../docs/demo.md");

fn out(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/snapshots");
    std::fs::create_dir_all(&dir).expect("target/snapshots");
    dir.join(format!("{name}.png"))
}

/// Un lecteur au milieu de la démonstration, la voix sur un mot.
fn reading() -> App {
    let mut app = app_reading(DEMO);
    app.seek_to(2);
    app.player.simulate(2, Duration::from_millis(1200), Duration::from_secs(4));
    app
}

fn harness(app: App) -> Harness<'static, Gui> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .with_pixels_per_point(2.0)
        .with_max_steps(64)
        .build_ui_state(
            |ui, gui: &mut Gui| {
                gui.advance(&ui.ctx().clone());
                gui.frame(ui);
            },
            Gui::new(app),
        )
}

fn save(harness: &mut Harness<'static, Gui>, name: &str) {
    // La première trame pose les polices et ne dessine rien ; quelques-unes
    // de plus laissent le texte se composer.
    harness.run_steps(6);
    let image = harness.render().expect("rendu hors écran");
    let path = out(name);
    image.save(&path).expect("écriture du PNG");
    eprintln!("écrit : {}", path.display());
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn reading_bar() {
    let mut harness = harness(reading());
    save(&mut harness, "lecture");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn more_menu() {
    let mut harness = harness(reading());
    harness.run_steps(4);
    let rect = harness.state().more_rect.expect("le bouton Plus s'est dessiné");
    let at = rect.center();
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.event(egui::Event::PointerMoved(Pos2::new(at.x, at.y - 200.0)));
    save(&mut harness, "menu-plus");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn settings_screen() {
    let mut app = reading();
    app.open_config();
    let mut harness = harness(app);
    save(&mut harness, "reglages");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn frozen_view_and_search() {
    let mut app = reading();
    app.search_with("ligne");
    app.scroll_view(1);
    let mut harness = harness(app);
    harness.state_mut().search_open = true;
    save(&mut harness, "recherche-vue-figee");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn light_theme() {
    let mut app = reading();
    app.config.gui.theme = "light".to_string();
    let mut harness = harness(app);
    save(&mut harness, "clair");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn empty_state() {
    let mut harness = harness(app_reading(""));
    save(&mut harness, "vide");
}

#[test]
#[ignore = "rend une image à regarder ; demande un GPU"]
fn help_window() {
    let mut app = reading();
    app.show_help = true;
    let mut harness = harness(app);
    save(&mut harness, "aide");
}
