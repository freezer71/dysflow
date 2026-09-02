//! Sommaire et signets : les mêmes listes qu'au terminal, cliquables.

use eframe::egui::{self, RichText, ScrollArea, Ui};

use super::theme::Palette;
use crate::app::{App, Panel};

pub fn show(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let title = match app.panel {
        Panel::Contents => "Sommaire",
        Panel::Bookmarks => "Signets",
        Panel::None => return,
    };
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading(RichText::new(title).color(palette.accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Échap").clicked() {
                app.panel = Panel::None;
            }
        });
    });
    ui.separator();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match app.panel {
            Panel::Contents => contents(ui, app, palette),
            Panel::Bookmarks => bookmarks(ui, app, palette),
            Panel::None => {}
        });
}

fn contents(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let current = app
        .doc()
        .section_of_sentence(app.current_sentence())
        .map(|s| s.id);
    // Même liste que celle qu'interroge la sélection : les index concordent.
    let rows: Vec<(usize, u8, String, bool)> = app
        .navigable_sections()
        .into_iter()
        .filter_map(|id| app.doc().section(id))
        .enumerate()
        .map(|(i, s)| (i, s.level, s.title.clone(), current == Some(s.id)))
        .collect();
    for (i, level, title, active) in rows {
        ui.horizontal(|ui| {
            ui.add_space(12.0 * f32::from(level.saturating_sub(1)));
            // Sur la ligne sélectionnée, le fond est l'accent : le texte
            // passe en sombre pour rester lisible.
            let selected = i == app.selection;
            let text = if selected {
                RichText::new(title).color(palette.on_accent).strong()
            } else if active {
                RichText::new(title).color(palette.accent).strong()
            } else {
                RichText::new(title).color(palette.normal)
            };
            if ui.selectable_label(selected, text).clicked() {
                app.selection = i;
                app.activate_selection();
            }
        });
    }
}

fn bookmarks(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let rows: Vec<(String, String)> = app
        .bookmarks
        .for_document(&app.source)
        .map(|b| (b.snippet.clone(), format!("{} · {}", b.timestamp(), b.section_title)))
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new("Aucun signet. b pour en poser un.").color(palette.dim));
        return;
    }
    for (i, (snippet, meta)) in rows.into_iter().enumerate() {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let selected = i == app.selection;
                let color = if selected { palette.on_accent } else { palette.normal };
                let text = RichText::new(truncate(&snippet, 70)).color(color);
                if ui.selectable_label(selected, text).clicked() {
                    app.selection = i;
                    app.activate_selection();
                }
                ui.label(RichText::new(meta).small().color(palette.faint));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if ui.small_button("×").on_hover_text("d — retirer").clicked() {
                    app.selection = i;
                    app.delete_selection();
                }
            });
        });
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}
