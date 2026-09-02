//! Ouvrir un document depuis la fenêtre : dialogue, fichier déposé, presse-papiers.
//!
//! L'extraction d'un PDF ou d'un scan prend des secondes : elle part dans un
//! thread, la ligne d'état le dit, et le document arrive quand il est prêt.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use anyhow::Result;
use eframe::egui::{self, Align2, Color32, RichText, Ui};

use super::header::Actions;
use super::theme::Palette;
use super::window::Gui;
use crate::doc::DocumentBuilder;
use crate::input;

/// Une extraction en cours.
pub struct Opening {
    pub name: String,
    /// Vrai pour un fichier : sa position retenue est reprise.
    resumes: bool,
    rx: Receiver<Result<String>>,
}

pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Documents",
            &["md", "markdown", "txt", "pdf", "png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp"],
        )
        .pick_file()
}

pub fn start_path(gui: &mut Gui, path: PathBuf) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::source::load_with_cache(&path, true));
    });
    gui.opening = Some(Opening { name, resumes: true, rx });
}

pub fn start_clipboard(gui: &mut Gui) {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        #[cfg(feature = "clipboard")]
        let result = crate::source::clipboard();
        #[cfg(not(feature = "clipboard"))]
        let result = Err(anyhow::anyhow!("binaire compilé sans l'option `clipboard`"));
        let _ = tx.send(result);
    });
    gui.opening = Some(Opening { name: "presse-papiers".to_string(), resumes: false, rx });
}

/// Range un document arrivé, ou l'erreur qui en tient lieu.
pub fn poll(gui: &mut Gui) {
    let Some(opening) = gui.opening.as_ref() else { return };
    let result = match opening.rx.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("extraction interrompue")),
    };
    let Some(Opening { name, resumes, .. }) = gui.opening.take() else { return };
    match result {
        Ok(text) => {
            let mut builder = DocumentBuilder::new(gui.app.config.code_policy());
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            if builder.sentence_count() == 0 {
                gui.app.notify(&format!("{name} : aucun contenu lisible"));
                return;
            }
            gui.app.open_document(builder, None, name);
            if resumes {
                gui.app.resume_where_left();
            }
            gui.reader.reset();
        }
        Err(err) => gui.app.notify(&format!("{name} : {err}")),
    }
}

/// Un fichier survole ou tombe sur la fenêtre.
pub fn handle_drops(ctx: &egui::Context, gui: &mut Gui) {
    gui.hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    if let Some(path) = dropped.into_iter().next() {
        start_path(gui, path);
    }
}

/// Le voile « Déposer pour lire », pendant le survol.
pub fn drop_overlay(ctx: &egui::Context, gui: &Gui, palette: &Palette) {
    if !gui.hovering_file {
        return;
    }
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("dépôt"),
    ));
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Déposer pour lire",
        egui::FontId::proportional(32.0),
        palette.accent,
    );
}

/// Sans document : deux boutons au milieu du vide, pas un écran de réglages.
pub fn empty_state(ui: &mut Ui, gui: &Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    ui.add_space(ui.available_height() * 0.3);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("sillon").size(40.0).color(palette.accent).strong());
        ui.label(
            RichText::new("Un document à lire, à écouter, à suivre des yeux.").color(palette.dim),
        );
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 340.0).max(0.0) / 2.0);
            if ui.button(RichText::new("Ouvrir un fichier…").size(18.0)).clicked() {
                actions.open_file = true;
            }
            if ui.button(RichText::new("Lire le presse-papiers").size(18.0)).clicked() {
                actions.paste = true;
            }
        });
        ui.add_space(12.0);
        ui.label(
            RichText::new("ou déposez un fichier sur la fenêtre — markdown, texte, PDF, image")
                .color(palette.faint),
        );
        if let Some(opening) = &gui.opening {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                ui.add(egui::Spinner::new().color(palette.accent));
                ui.label(
                    RichText::new(format!("extraction de {}…", opening.name)).color(palette.accent),
                );
            });
        }
    });
    actions
}
