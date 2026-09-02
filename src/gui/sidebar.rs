//! La barre latérale : ouvrir, le sommaire, les signets, les réglages, l'aide.
//!
//! Toujours là, repliable d'un clic ou avec `t`. Quand l'écran de réglages est
//! ouvert, ses rubriques prennent la place du sommaire : une seule colonne de
//! navigation, jamais deux.

use eframe::egui::{Align, Layout, RichText, ScrollArea, Ui};

use super::icons::Icon;
use super::theme::Palette;
use super::widgets::{self, Button, NavRow};
use super::window::Gui;
use super::settings;

pub const WIDTH: f32 = 240.0;

/// Ce que la barre demande à la fenêtre.
#[derive(Debug, Default, Clone, Copy)]
pub struct Actions {
    pub open_file: bool,
    pub paste: bool,
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new("sillon").size(15.0).strong().color(palette.accent));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if Button::icon(Icon::Sidebar).show(ui, palette).on_hover_text("Replier  t").clicked() {
                gui.sidebar_open = false;
            }
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        if Button::icon(Icon::Folder)
            .label("Ouvrir")
            .show(ui, palette)
            .on_hover_text("⌘O · Ctrl+O")
            .clicked()
        {
            actions.open_file = true;
        }
        if Button::icon(Icon::Clipboard)
            .label("Coller")
            .show(ui, palette)
            .on_hover_text("⌘V · Ctrl+V — texte, ou image reconnue")
            .clicked()
        {
            actions.paste = true;
        }
    });

    // Le pied se pose d'abord, pour que la liste prenne ce qui reste.
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.add_space(4.0);
        let engine = gui.app.engine_name();
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(engine).small().color(palette.faint));
        });
        if NavRow::new("Aide").icon(Icon::Help).show(ui, palette).on_hover_text("?").clicked() {
            gui.app.show_help = true;
        }
        if NavRow::new("Réglages")
            .icon(Icon::Gear)
            .selected(gui.app.show_config)
            .show(ui, palette)
            .on_hover_text("s")
            .clicked()
        {
            gui.app.toggle_config();
        }
        widgets::rule(ui, palette);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if gui.app.show_config {
                    settings::rubrics(ui, gui, palette);
                } else {
                    contents(ui, gui, palette);
                    bookmarks(ui, gui, palette);
                }
            });
        });
    });
    actions
}

fn contents(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    widgets::group_label(ui, palette, "Sommaire");
    let app = &gui.app;
    let current = app.doc().section_of_sentence(app.current_sentence()).map(|s| s.id);
    let rows: Vec<(u8, String, bool, Option<usize>)> = app
        .navigable_sections()
        .into_iter()
        .filter_map(|id| app.doc().section(id))
        .map(|s| (s.level, s.title.clone(), current == Some(s.id), s.first_sentence))
        .collect();
    if rows.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let text = if app.doc().is_empty() { "Aucun document" } else { "Sans titres" };
            ui.label(RichText::new(text).color(palette.faint));
        });
        return;
    }
    for (level, title, active, first) in rows {
        let indent = 12.0 * f32::from(level.saturating_sub(1));
        if NavRow::new(&title).current(active).indent(indent).show(ui, palette).clicked() {
            if let Some(target) = first {
                gui.app.seek_to(target);
            }
        }
    }
}

fn bookmarks(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    let source = gui.app.source.clone();
    let rows: Vec<(usize, String, String)> = gui
        .app
        .bookmarks
        .for_document(&source)
        .map(|b| (b.sentence_id, b.snippet.clone(), b.timestamp()))
        .collect();
    if rows.is_empty() {
        return;
    }
    widgets::group_label(ui, palette, "Signets");
    let current = gui.app.current_sentence();
    for (sentence_id, snippet, stamp) in rows {
        // L'heure seule : la date tient dans l'infobulle.
        let hour = stamp.get(11..).unwrap_or("").to_string();
        let mut remove = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.scope(|ui| {
                ui.set_width(ui.available_width() - 30.0);
                let response = NavRow::new(&snippet)
                    .icon(Icon::Bookmark)
                    .current(sentence_id == current)
                    .trailing(&hour)
                    .show(ui, palette)
                    .on_hover_text(format!("{stamp}\n{snippet}"));
                if response.clicked() {
                    gui.app.seek_to(sentence_id);
                }
            });
            if Button::icon(Icon::Trash).show(ui, palette).on_hover_text("Retirer").clicked() {
                remove = true;
            }
        });
        if remove {
            gui.app.bookmarks.remove(&source, sentence_id);
            let _ = gui.app.bookmarks.save();
            gui.app.notify("Signet supprimé");
        }
    }
}
