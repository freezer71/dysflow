//! La fenêtre : une trame est un tour du contrôleur, puis les zones.
//!
//! Pour vérifier le rendu sans passer par l'écran, `SILLON_GUI_CAPTURE=/x.png`
//! fait enregistrer la trame par la fenêtre elle-même après une seconde et
//! demie — ou `SILLON_GUI_CAPTURE_AFTER_MS` —, puis quitte.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use eframe::egui::{self, CentralPanel, Panel, ViewportBuilder};

use super::reader::Reader;
use super::settings::Rubric;
use super::{fonts, header, help, keys, open, panel, pixel, player, settings, theme};
use crate::app::{App, Panel as AppPanel};

pub struct Gui {
    pub(super) app: App,
    /// Police, thème et taille appliqués au contexte : on ne refait que ce
    /// qui change.
    applied: Option<(String, String, f32)>,
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
    /// Où le bouton « Plus » s'est dessiné : les tests hors écran cliquent là.
    pub(super) more_rect: Option<egui::Rect>,
}

pub fn run(app: App) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("sillon")
            .with_app_id("org.koumalabs.sillon")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([640.0, 420.0])
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
        Self {
            app,
            applied: None,
            capture,
            reader: Reader::new(),
            last_source,
            search_open: false,
            search_text: String::new(),
            focus_search: false,
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
        let wanted = (gui.font.clone(), gui.theme.clone(), gui.font_size());
        if self.applied.as_ref() == Some(&wanted) {
            return;
        }
        if self.applied.as_ref().map(|a| &a.0) != Some(&wanted.0) {
            fonts::install(ctx, &wanted.0);
        }
        theme::apply(ctx, &wanted.1, wanted.2);
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

        let requests = keys::dispatch(&ctx, &mut self.app);
        if requests.page != 0 {
            self.reader.page(requests.page);
        }
        if requests.focus_search {
            self.search_open = true;
            self.focus_search = true;
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

        let mut actions =
            Panel::top("header").show(ui, |ui| header::show(ui, self, &palette)).inner;
        // Les panneaux du bas se posent de l'extérieur vers l'intérieur : la
        // ligne d'état tout en bas, la barre de lecture au-dessus.
        let opening = self.opening.as_ref().map(|o| o.name.clone());
        // Une hauteur fixe : la ligne d'état se tait sans faire sauter le texte.
        let status_h = pixel::step(ui) * 14.0 + 6.0;
        Panel::bottom("status").exact_size(status_h).show(ui, |ui| {
            player::status(ui, &mut self.app, &palette, opening.as_deref())
        });
        let bar = Panel::bottom("player").show(ui, |ui| player::show(ui, self, &palette)).inner;
        if bar.open_search {
            self.search_open = true;
            self.focus_search = true;
        }
        if self.app.panel != AppPanel::None && !self.app.show_config {
            Panel::right("panneau")
                .default_size(320.0)
                .show(ui, |ui| panel::show(ui, &mut self.app, &palette));
        }
        let empty = self.app.doc().is_empty() && self.app.input_complete();
        if self.app.show_config {
            CentralPanel::default()
                .frame(egui::Frame::new().fill(palette.background))
                .show(ui, |ui| settings::show(ui, self, &palette));
        } else {
            CentralPanel::default()
                .frame(egui::Frame::new().fill(palette.background).inner_margin(16.0))
                .show(ui, |ui| {
                    if empty {
                        let more = open::empty_state(ui, self, &palette);
                        actions.open_file |= more.open_file;
                        actions.paste |= more.paste;
                    } else {
                        self.reader.show(ui, &mut self.app, &palette);
                    }
                });
        }
        help::show(&ctx, &mut self.app, &palette);
        open::drop_overlay(&ctx, self, &palette);

        if actions.open_file || requests.open_file {
            if let Some(path) = open::pick_file() {
                open::start_path(self, path);
            }
        }
        if actions.paste || requests.paste {
            open::start_clipboard(self);
        }
    }
}

impl eframe::App for Gui {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.advance(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn on_exit(&mut self) {
        self.app.remember_position();
        // Ce qui devait être écrit l'est. Détruire le lecteur attendrait la fin
        // des synthèses en cours — mesuré : vingt-quatre secondes avec MagpieTTS
        // en train de compiler ses noyaux. Une fenêtre qu'on ferme doit
        // disparaître ; le système reprend ce qui reste.
        std::process::exit(0);
    }
}
