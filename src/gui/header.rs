//! L'en-tête : ouvrir, coller, où l'on est, chercher, ouvrir les panneaux.

use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use super::theme::Palette;
use super::window::Gui;
use crate::app::Panel;

/// Ce que l'en-tête demande à la fenêtre.
#[derive(Debug, Default, Clone, Copy)]
pub struct Actions {
    pub open_file: bool,
    pub paste: bool,
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Ouvrir…").on_hover_text("⌘O · Ctrl+O").clicked() {
            actions.open_file = true;
        }
        if ui
            .button("Coller")
            .on_hover_text("⌘V · Ctrl+V — texte, ou image reconnue")
            .clicked()
        {
            actions.paste = true;
        }
        ui.separator();

        let doc = gui.app.doc();
        let section = doc
            .section_of_sentence(gui.app.current_sentence())
            .map(|s| s.title.clone())
            .unwrap_or_default();
        let title = doc.title.clone();
        ui.label(RichText::new(&title).strong().color(palette.bright));
        ui.label(RichText::new(&gui.app.source).color(palette.dim));
        if !section.is_empty() && section != title {
            ui.label(RichText::new("▸").color(palette.faint));
            ui.label(RichText::new(section).color(palette.accent));
        }
        if !gui.app.input_complete() {
            ui.label(RichText::new("⟳ flux").color(palette.dim));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Aide").on_hover_text("?").clicked() {
                gui.app.show_help = !gui.app.show_help;
            }
            if ui
                .selectable_label(gui.app.show_config, "Réglages")
                .on_hover_text("s")
                .clicked()
            {
                gui.app.toggle_config();
            }
            if ui
                .selectable_label(gui.app.panel == Panel::Bookmarks, "Signets")
                .on_hover_text("B")
                .clicked()
            {
                gui.app.toggle_panel(Panel::Bookmarks);
            }
            if ui
                .selectable_label(gui.app.panel == Panel::Contents, "Sommaire")
                .on_hover_text("t")
                .clicked()
            {
                gui.app.toggle_panel(Panel::Contents);
            }
            ui.separator();
            search(ui, gui, palette);
        });
    });
    ui.add_space(4.0);
    actions
}

/// Le champ de recherche et ses deux flèches.
fn search(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if ui.button("›").on_hover_text("N — résultat suivant").clicked() {
        gui.app.search_advance(1);
    }
    if ui.button("‹").on_hover_text("P — résultat précédent").clicked() {
        gui.app.search_advance(-1);
    }
    if let Some(search) = gui.app.search() {
        if !search.is_empty() {
            ui.label(
                RichText::new(format!("{}/{}", search.position(), search.count()))
                    .color(palette.normal),
            );
        }
    }
    let response = ui.add(
        TextEdit::singleline(&mut gui.search_text)
            .hint_text("Chercher…  ( / )")
            .desired_width(200.0),
    );
    if gui.focus_search {
        response.request_focus();
        gui.focus_search = false;
    }
    // Entrée lance la recherche depuis le curseur ; le champ rend le clavier.
    if response.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let query = gui.search_text.clone();
            gui.app.search_with(&query);
        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            gui.search_text.clear();
            gui.app.search_cancel();
        }
    }
}
