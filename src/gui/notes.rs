//! La note : une page de texte libre à droite du texte lu.
//!
//! Elle s'enregistre seule — une seconde après la dernière frappe, au
//! changement de document, à la fermeture — dans le répertoire de données,
//! par `crate::notes`. Le bouton Repère y pose la section et la minute en
//! cours, pour que la note se relise avec le document.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, FontId, Layout, RichText, ScrollArea, TextEdit, Ui};

use super::icons::Icon;
use super::theme::Palette;
use super::widgets::Button;
use super::window::Gui;
use super::{fonts, timeline};
use crate::app::App;

/// Le temps sans frappe avant d'écrire.
const SETTLE: Duration = Duration::from_secs(1);

pub struct Note {
    pub text: String,
    /// Document de la note : source et titre.
    key: Option<(String, String)>,
    /// Depuis quand le texte diffère du fichier.
    dirty_since: Option<Instant>,
    /// Dernier échec d'écriture, à montrer.
    pub error: Option<String>,
}

impl Note {
    pub fn new() -> Self {
        Self { text: String::new(), key: None, dirty_since: None, error: None }
    }

    /// Met la note au diapason du document : celle d'avant est écrite, celle
    /// du nouveau est lue.
    pub fn sync(&mut self, source: &str, title: &str) {
        if self.key.as_ref().is_some_and(|(s, t)| s == source && t == title) {
            return;
        }
        self.flush();
        self.text = match crate::notes::dir() {
            Some(dir) => crate::notes::load(&dir, source, title),
            None => String::new(),
        };
        self.key = Some((source.to_string(), title.to_string()));
        self.dirty_since = None;
    }

    pub fn touch(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Écrit si la frappe s'est posée.
    pub fn tick(&mut self) {
        if self.dirty_since.is_some_and(|since| since.elapsed() >= SETTLE) {
            self.flush();
        }
    }

    /// Écrit tout de suite ce qui doit l'être.
    pub fn flush(&mut self) {
        if self.dirty_since.is_none() {
            return;
        }
        let Some((source, title)) = self.key.as_ref() else { return };
        let Some(dir) = crate::notes::dir() else {
            self.error = Some("aucun répertoire de données".to_string());
            return;
        };
        match crate::notes::save(&dir, source, title, &self.text) {
            Ok(()) => {
                self.error = None;
                self.dirty_since = None;
            }
            Err(err) => {
                self.error = Some(err.to_string());
                // On réessaiera à la prochaine occasion.
                self.dirty_since = Some(Instant::now());
            }
        }
    }

    /// Ajoute un repère vers le passage en cours.
    pub fn add_marker(&mut self, app: &App) {
        let (section, position) = place(app);
        let marker = crate::notes::marker(&section, &position);
        crate::notes::append_marker(&mut self.text, &marker);
        self.touch();
    }
}

impl Default for Note {
    fn default() -> Self {
        Self::new()
    }
}

/// Où l'on est : la section, et la minute ou le rang de la phrase.
pub fn place(app: &App) -> (String, String) {
    let sentence = app.current_sentence();
    let section = app
        .doc()
        .section_of_sentence(sentence)
        .map(|s| s.title.clone())
        .filter(|t| *t != app.doc().title)
        .unwrap_or_default();
    let position = match app.estimated_elapsed() {
        Some(elapsed) => timeline::clock(elapsed),
        None => format!("phrase {}", sentence + 1),
    };
    (section, position)
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new("Note").strong().color(palette.bright));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if Button::icon(Icon::Close).show(ui, palette).on_hover_text("Fermer  e").clicked() {
                gui.notes_open = false;
            }
            if Button::icon(Icon::Marker)
                .label("Repère")
                .small()
                .show(ui, palette)
                .on_hover_text("Insère la section et la minute en cours")
                .clicked()
            {
                gui.note.add_marker(&gui.app);
                gui.focus_note = true;
            }
        });
    });
    let title = gui.app.doc().title.clone();
    if !title.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(RichText::new(title).small().color(palette.dim));
        });
    }
    ui.add_space(6.0);

    // Le pied : l'état de l'enregistrement, en un mot.
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        let state = if let Some(err) = &gui.note.error {
            RichText::new(format!("non enregistrée : {err}")).small().color(palette.accent)
        } else if gui.note.is_dirty() {
            RichText::new("…").small().color(palette.faint)
        } else if gui.note.text.trim().is_empty() {
            RichText::new("Écrire ici. La note se garde toute seule.").small().color(palette.faint)
        } else {
            RichText::new("enregistrée").small().color(palette.faint)
        };
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(state);
        });
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            let font = FontId::new(15.0, fonts::family(&gui.app.config.gui.font, false, false));
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let rows = ((ui.available_height() / 22.0) as usize).max(4);
                let response = ui.add(
                    TextEdit::multiline(&mut gui.note.text)
                        .font(font)
                        .text_color(palette.bright)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .hint_text("Écrire ici…"),
                );
                if response.changed() {
                    gui.note.touch();
                }
                if gui.focus_note {
                    response.request_focus();
                    gui.focus_note = false;
                }
            });
        });
    });
}
