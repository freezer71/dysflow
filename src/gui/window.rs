//! La fenêtre : une trame est un tour du contrôleur, puis les zones.
//!
//! Pour vérifier le rendu sans passer par l'écran, `SILLON_GUI_CAPTURE=/x.png`
//! fait enregistrer la trame par la fenêtre elle-même après une seconde et
//! demie — ou `SILLON_GUI_CAPTURE_AFTER_MS` —, puis quitte.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use eframe::egui::{self, CentralPanel, Panel, ViewportBuilder};

use super::reader::Reader;
use super::{fonts, theme};
use crate::app::App;

pub struct Gui {
    pub(super) app: App,
    /// Police, thème et taille appliqués au contexte : on ne refait que ce
    /// qui change.
    applied: Option<(String, String, f32)>,
    /// Capture demandée par l'environnement : chemin, départ, déjà demandée.
    capture: Option<(PathBuf, Instant, bool)>,
    pub(super) reader: Reader,
    /// Source du document composé : quand elle change, tout se recompose.
    last_source: String,
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
        Box::new(move |cc| Ok(Box::new(Gui::new(app, &cc.egui_ctx)))),
    )
    .map_err(|err| anyhow!("fenêtre : {err}"))
}

impl Gui {
    fn new(app: App, ctx: &egui::Context) -> Self {
        let capture = std::env::var_os("SILLON_GUI_CAPTURE")
            .map(|path| (PathBuf::from(path), Instant::now(), false));
        let last_source = app.source.clone();
        let mut gui = Self { app, applied: None, capture, reader: Reader::new(), last_source };
        gui.refresh_theme(ctx);
        gui
    }

    /// Enregistre la fenêtre telle qu'elle se dessine, puis quitte.
    fn capture_if_asked(&mut self, ctx: &egui::Context) {
        let Some((path, since, requested)) = self.capture.as_mut() else { return };
        // Une fenêtre occultée n'est plus dessinée : pour la capturer, il faut
        // qu'elle passe devant.
        if since.elapsed() < Duration::from_millis(100) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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

impl eframe::App for Gui {
    /// Le tour du contrôleur, à chaque trame — et fenêtre cachée aussi : la
    /// sortie audio se nourrit ici, et la voix ne doit pas s'arrêter parce
    /// qu'on a réduit la fenêtre.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.refresh_theme(&ctx);
        self.capture_if_asked(&ctx);
        let palette = theme::palette(&self.app.config.gui.theme);
        if self.app.source != self.last_source {
            self.reader.reset();
            self.last_source = self.app.source.clone();
        }

        Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.app.doc().title).strong().color(palette.bright));
                ui.label(egui::RichText::new(&self.app.source).color(palette.dim));
            });
        });
        CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.background).inner_margin(16.0))
            .show(ui, |ui| {
                self.reader.show(ui, &mut self.app, &palette);
            });
    }

    fn on_exit(&mut self) {
        self.app.remember_position();
    }
}
