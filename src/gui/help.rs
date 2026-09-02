//! L'aide : les mêmes tables que le terminal, dans une fenêtre flottante.

use eframe::egui::{self, Align2, RichText};

use super::theme::Palette;
use crate::app::keys::{CONFIG_HELP, HELP};
use crate::app::App;

pub fn show(ctx: &egui::Context, app: &mut App, palette: &Palette) {
    if !app.show_help {
        return;
    }
    let (title, entries) = if app.show_config {
        ("Configuration", CONFIG_HELP)
    } else {
        ("Raccourcis", HELP)
    };
    let response = egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("aide")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (keys, description) in entries {
                        ui.label(RichText::new(*keys).color(palette.accent).strong());
                        ui.label(RichText::new(*description).color(palette.normal));
                        ui.end_row();
                    }
                });
            ui.add_space(8.0);
            if ui.button("Fermer").clicked() {
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
