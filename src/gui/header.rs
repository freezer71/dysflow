//! La barre du haut : où l'on est, la recherche, le signet, la note.
//!
//! Le champ de recherche n'y vit pas en permanence : il s'ouvre sur `/`, sur
//! la loupe ou depuis `⋯`, et se referme une fois la recherche lancée.

use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use super::icons::{self, Icon};
use super::theme::Palette;
use super::widgets::Button;
use super::window::Gui;

pub const HEIGHT: f32 = 44.0;

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.horizontal(|ui| {
        if !gui.sidebar_open
            && Button::icon(Icon::Sidebar).show(ui, palette).on_hover_text("Sommaire  t").clicked()
        {
            gui.sidebar_open = true;
        }
        ui.add_space(4.0);
        // Où l'on est : le titre, puis la section.
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
            if !section.is_empty() && section != title {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                icons::paint(ui.painter(), rect, Icon::ChevronRight, palette.faint);
                ui.label(RichText::new(section).color(palette.dim));
            }
        }
        if !gui.app.input_complete() {
            ui.add(egui::Spinner::new().size(12.0).color(palette.dim));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if Button::icon(Icon::Note)
                .selected(gui.notes_open)
                .show(ui, palette)
                .on_hover_text("Note  e")
                .clicked()
            {
                gui.notes_open = !gui.notes_open;
                gui.focus_note = gui.notes_open;
            }
            let here = gui.app.bookmarked_here();
            let icon = if here { Icon::BookmarkFilled } else { Icon::Bookmark };
            if Button::icon(icon)
                .selected(here)
                .show(ui, palette)
                .on_hover_text(if here { "Retirer le signet  b" } else { "Poser un signet  b" })
                .clicked()
            {
                gui.app.toggle_bookmark();
            }
            if Button::icon(Icon::Search)
                .selected(gui.search_open)
                .show(ui, palette)
                .on_hover_text("Chercher  /")
                .clicked()
            {
                gui.search_open = !gui.search_open;
                gui.focus_search = gui.search_open;
                if !gui.search_open {
                    gui.search_text.clear();
                    gui.app.search_cancel();
                }
            }
            if gui.search_open {
                ui.add_space(4.0);
                search(ui, gui, palette);
            }
        });
    });
}

/// Le champ de recherche, ouvert le temps d'une requête.
fn search(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if Button::icon(Icon::Next).show(ui, palette).on_hover_text("Résultat suivant  N").clicked() {
        gui.app.search_advance(1);
    }
    if Button::icon(Icon::Prev).show(ui, palette).on_hover_text("Résultat précédent  P").clicked() {
        gui.app.search_advance(-1);
    }
    if let Some(search) = gui.app.search() {
        if !search.is_empty() {
            ui.label(
                RichText::new(format!("{} / {}", search.position(), search.count()))
                    .small()
                    .color(palette.dim),
            );
        }
    }
    let response = ui.add(
        TextEdit::singleline(&mut gui.search_text)
            .hint_text("Chercher…")
            .desired_width(200.0),
    );
    if gui.focus_search {
        response.request_focus();
        gui.focus_search = false;
    }
    // Entrée lance la recherche depuis le curseur et referme le champ ; les
    // résultats restent, et N, P ou `⋯` les parcourent.
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
