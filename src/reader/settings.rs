//! Réglages et voix, tels que l'interface les montre et les change.

use anyhow::{anyhow, Result};
use std::time::Instant;

use crate::models::{self, Download, Progress};

use super::Reader;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    pub speed: f32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadView {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub failed: Option<String>,
}

impl Reader {
    pub fn settings(&self) -> SettingsView {
        let c = self.current_config();
        SettingsView {
            speed: c.voice.speed,
            line_spacing: c.reading.line_spacing,
            reading_ruler: c.reading.reading_ruler,
            keep_voice_minutes: c.app.keep_voice_minutes,
        }
    }

    /// Applique sans écrire : le texte ouvert suit tout de suite.
    pub fn apply_settings(&mut self, s: SettingsView) {
        self.sync_config();
        self.config.reading.line_spacing = s.line_spacing;
        self.config.reading.reading_ruler = s.reading_ruler;
        self.config.app.keep_voice_minutes = s.keep_voice_minutes;
        if let Some(app) = self.session.as_mut() {
            app.config.reading.line_spacing = s.line_spacing;
            app.config.reading.reading_ruler = s.reading_ruler;
            app.config.app.keep_voice_minutes = s.keep_voice_minutes;
            app.set_speed(s.speed);
        }
        self.config.voice.speed = s.speed;
    }

    pub fn update_settings(&mut self, s: SettingsView) -> Result<()> {
        self.apply_settings(s);
        self.config.save()?;
        Ok(())
    }

    pub fn voices(&self) -> Vec<VoiceModel> {
        let active = &self.current_config().engine.model_path;
        models::available()
            .into_iter()
            .map(|p| VoiceModel {
                id: p.id.to_string(),
                name: p.display_name.to_string(),
                bytes: p.bytes,
                installed: models::is_installed(p),
                active: models::install_path(p)
                    .is_some_and(|path| path.to_string_lossy() == active.as_str()),
                recommended: p.recommended,
            })
            .collect()
    }

    pub fn install_voice(&mut self, id: &str) -> Result<()> {
        let package = package(id)?;
        if self.download.as_ref().is_some_and(|d| !d.is_finished()) {
            return Err(anyhow!("téléchargement déjà en cours"));
        }
        self.download = Some(Download::start(package)?);
        Ok(())
    }

    /// Change de voix : la suivante se chargera à la prochaine commande, au
    /// même endroit du texte.
    pub fn use_voice(&mut self, id: &str) -> Result<()> {
        let package = package(id)?;
        let path = models::install_path(package)
            .filter(|_| models::is_installed(package))
            .ok_or_else(|| anyhow!("voix non installée : {id}"))?;
        // Au repos d'abord : la mise au repos recopie la configuration du
        // texte ouvert, qui écraserait la voix choisie.
        self.rest();
        self.switch_voice_config(&path.to_string_lossy(), package.family);
        self.config.save()?;
        Ok(())
    }

    /// La partie sans disque du changement de voix.
    fn switch_voice_config(&mut self, model_path: &str, family: &str) {
        self.config.engine.model_path = model_path.to_string();
        self.config.engine.family = family.to_string();
        self.config.align_voice_with_family();
    }

    pub fn download(&self) -> Option<DownloadView> {
        let d = self.download.as_ref()?;
        let (received, total, failed) = match d.progress() {
            Progress::Running { received, total } => (received, total, None),
            Progress::Done => (d.package.bytes, d.package.bytes, None),
            Progress::Failed(message) => (0, d.package.bytes, Some(message)),
            Progress::Cancelled => (0, d.package.bytes, Some("annulé".into())),
        };
        Some(DownloadView { id: d.package.id.to_string(), received, total, failed })
    }

    /// Appelé à chaque tour : un téléchargement terminé disparaît, et devient
    /// la voix active s'il n'y en avait aucune.
    pub(super) fn poll_download(&mut self, _now: Instant) {
        let done = self
            .download
            .as_ref()
            .is_some_and(|d| matches!(d.progress(), Progress::Done));
        if !done {
            return;
        }
        let id = self.download.take().map(|d| d.package.id.to_string());
        let none_active = !self.voices().iter().any(|v| v.active);
        if let (Some(id), true) = (id, none_active) {
            let _ = self.use_voice(&id);
        }
    }

    /// La configuration que le texte ouvert a pu modifier (vitesse…).
    fn current_config(&self) -> &crate::config::Config {
        self.session.as_ref().map(|a| &a.config).unwrap_or(&self.config)
    }

    fn sync_config(&mut self) {
        if let Some(app) = self.session.as_ref() {
            self.config = app.config.clone();
        }
    }
}

fn package(id: &str) -> Result<&'static models::ModelPackage> {
    models::available()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!("voix inconnue : {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn reader() -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    #[test]
    fn settings_reach_the_open_text() {
        let mut r = reader();
        r.open_text("Un.", "a", Instant::now());
        let mut s = r.settings();
        s.speed = 1.4;
        s.reading_ruler = false;
        s.keep_voice_minutes = 30;
        r.apply_settings(s.clone());
        assert_eq!(r.settings(), s);
        let app = r.session().unwrap();
        assert_eq!(app.config.voice.speed, 1.4);
        assert!(!app.config.reading.reading_ruler);
    }

    #[test]
    fn the_catalog_lists_every_available_model_once() {
        let r = reader();
        let voices = r.voices();
        let mut ids: Vec<_> = voices.iter().map(|v| v.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), voices.len());
        assert_eq!(voices.len(), crate::models::available().len());
    }

    #[test]
    fn using_an_unknown_voice_is_an_error() {
        let mut r = reader();
        assert!(r.use_voice("n-existe-pas").is_err());
    }

    #[test]
    fn a_voice_switch_survives_the_rest_and_reaches_the_next_engine() {
        use std::sync::Mutex;
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut r = Reader::new(
            config,
            Box::new(move |c| {
                log.lock().unwrap().push(c.engine.model_path.clone());
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(Player::silent),
            Instant::now(),
        );
        r.open_text("Un. Deux.", "a", Instant::now());
        r.rest();
        r.switch_voice_config("/nouvelle/voix.onnx", "supertonic");
        r.with_session(Instant::now(), |_| {});
        assert_eq!(seen.lock().unwrap().last().unwrap(), "/nouvelle/voix.onnx");
        assert_eq!(r.config().engine.model_path, "/nouvelle/voix.onnx");
    }

    #[test]
    fn the_resting_document_is_kept_once_and_dropped_on_open() {
        let mut r = reader();
        r.open_text("Un. Deux.", "a", Instant::now());
        assert!(r.resting_document().is_none());
        r.rest();
        assert!(r.resting_document().is_some());
        assert_eq!(crate::reader::view::document(&r).sentence_count, 2);
        r.open_text("Trois.", "b", Instant::now());
        assert!(r.resting_document().is_none());
    }
}
