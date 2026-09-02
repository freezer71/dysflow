//! L'en-tête : ouvrir, coller, où l'on est, le sommaire, les réglages.
//!
//! Le champ de recherche n'y vit pas en permanence : il s'ouvre sur `/` ou
//! depuis « Plus », et se referme une fois la recherche lancée.

use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use super::pixel::{self, Button};
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
    let px = pixel::step(ui);
    let mut actions = Actions::default();
    ui.add_space(2.0 * px);
    ui.horizontal(|ui| {
        ui.add_space(px);
        if Button::icon(&pixel::FOLDER)
            .label("Ouvrir")
            .show(ui, palette)
            .on_hover_text("⌘O · Ctrl+O")
            .clicked()
        {
            actions.open_file = true;
        }
        if Button::icon(&pixel::CLIPBOARD)
            .label("Coller")
            .show(ui, palette)
            .on_hover_text("⌘V · Ctrl+V — texte, ou image reconnue")
            .clicked()
        {
            actions.paste = true;
        }
        ui.add_space(3.0 * px);

        // Où l'on est : du texte qui se lit, dans la police de lecture.
        let doc = gui.app.doc();
        if doc.is_empty() && gui.app.input_complete() {
            ui.label(RichText::new("Aucun document").color(palette.faint));
        } else {
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
        }
        if !gui.app.input_complete() {
            ui.label(RichText::new("⟳ flux").color(palette.dim));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(px);
            if Button::icon(&pixel::GEAR)
                .label("Réglages")
                .selected(gui.app.show_config)
                .show(ui, palette)
                .on_hover_text("s")
                .clicked()
            {
                gui.app.toggle_config();
            }
            if Button::icon(&pixel::LIST)
                .label("Sommaire")
                .selected(gui.app.panel == Panel::Contents)
                .show(ui, palette)
                .on_hover_text("t")
                .clicked()
            {
                gui.app.toggle_panel(Panel::Contents);
            }
            if gui.search_open {
                ui.add_space(2.0 * px);
                search(ui, gui, palette);
            }
        });
    });
    ui.add_space(2.0 * px);
    actions
}

/// Le champ de recherche, ouvert le temps d'une requête.
fn search(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if Button::icon(&pixel::CLOSE).show(ui, palette).on_hover_text("Échap").clicked() {
        gui.search_open = false;
        gui.search_text.clear();
        gui.app.search_cancel();
        return;
    }
    if Button::icon(&pixel::NEXT).show(ui, palette).on_hover_text("Résultat suivant  N").clicked() {
        gui.app.search_advance(1);
    }
    if Button::icon(&pixel::PREV).show(ui, palette).on_hover_text("Résultat précédent  P").clicked() {
        gui.app.search_advance(-1);
    }
    if let Some(search) = gui.app.search() {
        if !search.is_empty() {
            ui.label(
                RichText::new(format!("{} / {}", search.position(), search.count()))
                    .color(palette.normal),
            );
        }
    }
    let response = ui.add(
        TextEdit::singleline(&mut gui.search_text)
            .hint_text("Chercher…")
            .desired_width(220.0),
    );
    if gui.focus_search {
        response.request_focus();
        gui.focus_search = false;
    }
    // Entrée lance la recherche depuis le curseur et referme le champ ; les
    // résultats restent, et N, P ou « Plus » les parcourent.
    if response.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let query = gui.search_text.clone();
            gui.app.search_with(&query);
            gui.search_open = false;
        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            gui.search_text.clear();
            gui.app.search_cancel();
            gui.search_open = false;
        }
    }
}
