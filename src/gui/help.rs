//! L'aide : les mêmes tables que le terminal, dans une fenêtre flottante.

use eframe::egui::{self, Align2, RichText};

use super::theme::Palette;
use super::widgets::Button;
use crate::app::keys::{CONFIG_HELP, HELP};
use crate::app::App;

/// Ce que la fenêtre ajoute au terminal.
const WINDOW_HELP: &[(&str, &str)] = &[
    ("⌘O · Ctrl+O", "Ouvrir un fichier"),
    ("⌘V · Ctrl+V", "Lire le presse-papiers"),
    ("e", "Ouvrir ou fermer la note"),
    ("t · Tab", "Replier ou déplier la barre latérale"),
];

pub fn show(ctx: &egui::Context, app: &mut App, palette: &Palette) {
    if !app.show_help {
        return;
    }
    let (title, entries) = if app.show_config {
        ("Configuration", CONFIG_HELP)
    } else {
        ("Raccourcis", HELP)
    };
    let response = egui::Window::new(RichText::new(title).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("aide")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    for (keys, description) in entries {
                        ui.label(RichText::new(spelled(keys)).color(palette.accent).strong());
                        ui.label(RichText::new(*description).color(palette.normal));
                        ui.end_row();
                    }
                    if !app.show_config {
                        for (keys, description) in WINDOW_HELP {
                            ui.label(RichText::new(*keys).color(palette.accent).strong());
                            ui.label(RichText::new(*description).color(palette.normal));
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
            if Button::text("Fermer").show(ui, palette).clicked() {
                app.show_help = false;
            }
        });
    // Un clic hors de la fenêtre la referme, comme n'importe quelle touche.
    if let Some(response) = response {
        if ctx.input(|i| i.pointer.any_pressed()) && !response.response.contains_pointer() {
            app.show_help = false;
        }
    }
}

/// Les flèches en mots : la police de lecture n'a pas leurs glyphes, et un mot
/// se lit mieux qu'un signe pour qui lit avec effort.
fn spelled(keys: &str) -> String {
    keys.replace("Pg↑", "Page haut")
        .replace("Pg↓", "Page bas")
        .replace("← →", "Gauche · Droite")
        .replace("↑ ↓", "Haut · Bas")
        .replace('←', "Gauche")
        .replace('→', "Droite")
        .replace('↑', "Haut")
        .replace('↓', "Bas")
}
