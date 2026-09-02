//! La fenêtre : une trame est un tour du contrôleur, puis les zones.
//!
//! De gauche à droite : la barre latérale, le texte lu avec sa carte de
//! lecture, la note. Pour vérifier le rendu sans passer par l'écran,
//! `SILLON_GUI_CAPTURE=/x.png` fait enregistrer la trame par la fenêtre
//! elle-même après une seconde et demie — ou `SILLON_GUI_CAPTURE_AFTER_MS` —,
//! puis quitte ; mieux, les rendus hors écran de `snapshot`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use eframe::egui::{self, CentralPanel, Frame, Panel, Stroke, ViewportBuilder};

use super::notes::Note;
use super::reader::Reader;
use super::settings::Rubric;
use super::{fonts, header, help, keys, notes, open, player, settings, sidebar, theme};
use crate::app::App;

pub struct Gui {
    pub(super) app: App,
    /// Police et thème appliqués au contexte : on ne refait que ce qui change.
    applied: Option<(String, String)>,
    /// Capture demandée par l'environnement : chemin, départ, déjà demandée,
    /// fenêtre déjà passée devant.
    capture: Option<(PathBuf, Instant, bool, bool)>,
    pub(super) reader: Reader,
    /// Source du document composé : quand elle change, tout se recompose.
    last_source: String,
    /// Le champ de recherche de l'en-tête : ouvert le temps d'une requête,
    /// son texte, et la demande de lui donner le clavier.
    pub(super) search_open: bool,
    pub(super) search_text: String,
    pub(super) focus_search: bool,
    pub(super) sidebar_open: bool,
    pub(super) notes_open: bool,
    pub(super) focus_note: bool,
    pub(super) note: Note,
    /// Rubrique de configuration ouverte.
    pub(super) rubric: Rubric,
    /// Vrai à la trame précédente : pour caler la rubrique quand l'écran s'ouvre.
    config_was_open: bool,
    /// Champs texte en cours de saisie, par libellé.
    pub(super) edits: HashMap<&'static str, String>,
    /// Suppression d'un modèle armée : ligne, et instant du premier clic.
    pub(super) confirm_delete: Option<(usize, Instant)>,
    /// Extraction d'un document en cours.
    pub(super) opening: Option<open::Opening>,
    /// Un fichier survole la fenêtre.
    pub(super) hovering_file: bool,
    /// Où le bouton `⋯` s'est dessiné : les tests hors écran cliquent là.
    pub(super) more_rect: Option<egui::Rect>,
}

pub fn run(app: App) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("sillon")
            .with_app_id("org.koumalabs.sillon")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([720.0, 420.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "sillon",
        options,
        // Les polices s'installent avant la première trame : une famille
        // demandée avant d'être connue ferait paniquer la composition.
        Box::new(move |cc| {
            let mut gui = Gui::new(app);
            gui.refresh_theme(&cc.egui_ctx);
            Ok(Box::new(gui))
        }),
    )
    .map_err(|err| anyhow!("fenêtre : {err}"))
}

impl Gui {
    pub(super) fn new(app: App) -> Self {
        let capture = std::env::var_os("SILLON_GUI_CAPTURE")
            .map(|path| (PathBuf::from(path), Instant::now(), false, false));
        let last_source = app.source.clone();
        let mut note = Note::new();
        note.sync(&app.source, &app.doc().title);
        Self {
            app,
            applied: None,
            capture,
            reader: Reader::new(),
            last_source,
            search_open: false,
            search_text: String::new(),
            focus_search: false,
            sidebar_open: true,
            notes_open: false,
            focus_note: false,
            note,
            rubric: Rubric::Section(0),
            config_was_open: false,
            edits: HashMap::new(),
            confirm_delete: None,
            opening: None,
            hovering_file: false,
            more_rect: None,
        }
    }

    /// Enregistre la fenêtre telle qu'elle se dessine, puis quitte.
    fn capture_if_asked(&mut self, ctx: &egui::Context) {
        let Some((path, since, requested, focused)) = self.capture.as_mut() else { return };
        // Une fenêtre occultée n'est plus dessinée : pour la capturer, il faut
        // qu'elle passe devant — à la première trame, quel que soit le temps
        // que la fenêtre a mis à s'ouvrir.
        if !*focused {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            *focused = true;
        }
        let after = std::env::var("SILLON_GUI_CAPTURE_AFTER_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1500);
        if !*requested && since.elapsed() > Duration::from_millis(after) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            *requested = true;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [width, height] = image.size;
            if let Err(err) = image::save_buffer(
                path,
                image.as_raw(),
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("capture impossible : {err}");
            }
            self.app.should_quit = true;
        }
    }

    /// Met le contexte au diapason de la configuration quand elle change.
    fn refresh_theme(&mut self, ctx: &egui::Context) {
        let gui = &self.app.config.gui;
        let wanted = (gui.font.clone(), gui.theme.clone());
        if self.applied.as_ref() == Some(&wanted) {
            return;
        }
        if self.applied.as_ref().map(|a| &a.0) != Some(&wanted.0) {
            fonts::install(ctx, &wanted.0);
        }
        theme::apply(ctx, &wanted.1);
        self.applied = Some(wanted);
    }
}

impl Gui {
    /// Le tour du contrôleur, à chaque trame — et fenêtre cachée aussi : la
    /// sortie audio se nourrit ici, et la voix ne doit pas s'arrêter parce
    /// qu'on a réduit la fenêtre.
    pub(super) fn advance(&mut self, ctx: &egui::Context) {
        self.app.tick();
        open::poll(self);
        self.note.tick();
        if self.app.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Le mot suivi doit rester fluide : trente trames par seconde en
        // lecture, moins au repos — juste assez pour voir passer un message.
        ctx.request_repaint_after(if self.app.player.is_playing() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        });
    }

    /// Une trame : les zones, dans l'ordre où elles se posent.
    pub(super) fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Un contexte qui n'a pas encore ses polices ne dessine rien cette
        // trame : les définitions ne prennent effet qu'à la suivante.
        if self.applied.is_none() {
            self.refresh_theme(&ctx);
            ctx.request_repaint();
            return;
        }
        self.refresh_theme(&ctx);
        self.capture_if_asked(&ctx);
        let palette = theme::palette(&self.app.config.gui.theme);
        if self.app.source != self.last_source {
            self.reader.reset();
            self.last_source = self.app.source.clone();
        }
        self.note.sync(&self.app.source, &self.app.doc().title);

        let requests = keys::dispatch(&ctx, &mut self.app);
        if requests.page != 0 {
            self.reader.page(requests.page);
        }
        if requests.focus_search {
            self.search_open = true;
            self.focus_search = true;
        }
        if requests.toggle_sidebar {
            self.sidebar_open = !self.sidebar_open;
        }
        if requests.toggle_notes {
            self.notes_open = !self.notes_open;
            self.focus_note = self.notes_open;
        }
        if requests.escape {
            self.search_open = false;
        }
        open::handle_drops(&ctx, self);
        // L'écran de configuration s'ouvre sur la rubrique que le contrôleur
        // désigne — celle des modèles quand aucun n'est installé.
        if self.app.show_config && !self.config_was_open {
            self.rubric = Rubric::Section(self.app.screen.section);
            self.edits.clear();
        }
        self.config_was_open = self.app.show_config;

        let mut open_file = requests.open_file;
        let mut paste = requests.paste;
        if self.sidebar_open {
            let side = Frame::new().fill(palette.panel).inner_margin(egui::Margin::symmetric(8, 10));
            let actions = Panel::left("sidebar")
                .exact_size(sidebar::WIDTH)
                .resizable(false)
                .show_separator_line(false)
                .frame(side)
                .show(ui, |ui| sidebar::show(ui, self, &palette))
                .inner;
            open_file |= actions.open_file;
            paste |= actions.paste;
            edge(ui, &palette, Side::Right);
        }
        let top = Frame::new().fill(palette.background).inner_margin(egui::Margin::symmetric(12, 8));
        Panel::top("header")
            .exact_size(header::HEIGHT)
            .show_separator_line(false)
            .frame(top)
            .show(ui, |ui| header::show(ui, self, &palette));
        edge(ui, &palette, Side::Bottom);
        if self.notes_open && !self.app.show_config {
            let side = Frame::new().fill(palette.panel).inner_margin(egui::Margin::symmetric(12, 10));
            Panel::right("note")
                .default_size(300.0)
                .size_range(220.0..=520.0)
                .show_separator_line(false)
                .frame(side)
                .show(ui, |ui| notes::show(ui, self, &palette));
            edge(ui, &palette, Side::Left);
        }

        let empty = self.app.doc().is_empty() && self.app.input_complete();
        if self.app.show_config {
            CentralPanel::default()
                .frame(Frame::new().fill(palette.background).inner_margin(20.0))
                .show(ui, |ui| settings::show(ui, self, &palette));
        } else {
            CentralPanel::default()
                .frame(Frame::new().fill(palette.background))
                .show(ui, |ui| {
                    // Sans document, pas de carte : rien à lire, rien à faire avancer.
                    if !empty {
                        let opening = self.opening.as_ref().map(|o| o.name.clone());
                        let below =
                            Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 14 });
                        let actions = Panel::bottom("carte")
                            .show_separator_line(false)
                            .frame(below)
                            .show(ui, |ui| player::show(ui, self, &palette, opening.as_deref()))
                            .inner;
                        if actions.open_search {
                            self.search_open = true;
                            self.focus_search = true;
                        }
                        if actions.add_marker {
                            self.note.add_marker(&self.app);
                            self.notes_open = true;
                            self.focus_note = true;
                        }
                    }
                    CentralPanel::default()
                        .frame(Frame::new().fill(palette.background).inner_margin(egui::Margin::symmetric(16, 8)))
                        .show(ui, |ui| {
                            if empty {
                                let more = open::empty_state(ui, self, &palette);
                                open_file |= more.open_file;
                                paste |= more.paste;
                            } else {
                                self.reader.show(ui, &mut self.app, &palette);
                            }
                        });
                });
        }
        help::show(&ctx, &mut self.app, &palette);
        open::drop_overlay(&ctx, self, &palette);

        if open_file {
            if let Some(path) = open::pick_file() {
                open::start_path(self, path);
            }
        }
        if paste {
            open::start_clipboard(self);
        }
    }
}

#[derive(Clone, Copy)]
enum Side {
    Right,
    Bottom,
    Left,
}

/// Un trait d'un point au bord du dernier panneau posé, à la place du
/// séparateur d'egui — que la palette ne connaît pas.
fn edge(ui: &mut egui::Ui, palette: &theme::Palette, side: Side) {
    let rect = ui.max_rect();
    let stroke = Stroke::new(1.0, palette.edge);
    let painter = ui.painter();
    match side {
        Side::Right => painter.vline(rect.min.x, rect.y_range(), stroke),
        Side::Left => painter.vline(rect.max.x, rect.y_range(), stroke),
        Side::Bottom => painter.hline(rect.x_range(), rect.min.y, stroke),
    };
}

impl eframe::App for Gui {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.advance(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn on_exit(&mut self) {
        self.note.flush();
        self.app.remember_position();
        // Ce qui devait être écrit l'est. Détruire le lecteur attendrait la fin
        // des synthèses en cours — mesuré : vingt-quatre secondes avec MagpieTTS
        // en train de compiler ses noyaux. Une fenêtre qu'on ferme doit
        // disparaître ; le système reprend ce qui reste.
        std::process::exit(0);
    }
}
