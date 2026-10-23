//! Le lecteur qui dure : une voix gardée d'un texte à l'autre.
//!
//! `App` lit un document. `Reader` en ouvre un par texte sans recharger la
//! voix, et la relâche quand rien n'a été lu depuis un moment. Il ne quitte
//! jamais le fil qui l'a créé : la sortie audio n'est pas transférable d'un
//! fil à l'autre.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::audio::Player;
use crate::config::Config;
use crate::doc::DocumentBuilder;
use crate::tts::{Synthesizer, TtsEngine};

pub mod command;
pub mod data;
pub mod handle;
pub mod language;
pub mod library;
pub mod migration;
pub mod preview;
pub mod settings;
pub mod view;

pub type VoiceFactory = Box<dyn Fn(&Config) -> Arc<dyn TtsEngine>>;
/// Ouvre la sortie audio de ce nom (vide : celle du système).
pub type PlayerFactory = Box<dyn Fn(&str) -> Player>;

/// Le texte en cours, gardé pour le rouvrir après une mise au repos.
#[derive(Clone)]
struct Text {
    markdown: String,
    source: String,
    /// Un fichier : sa position se retient d'une ouverture à l'autre.
    identity: bool,
}

pub struct Reader {
    config: Config,
    voices: VoiceFactory,
    players: PlayerFactory,
    engine: Option<Arc<dyn TtsEngine>>,
    session: Option<App>,
    text: Option<Text>,
    /// Phrase où reprendre un texte mis au repos.
    resting_at: Option<usize>,
    revision: u64,
    last_activity: Instant,
    /// Téléchargement de voix en cours, sondé à chaque tour.
    download: Option<crate::models::Download>,
    /// Le document au repos, reconstruit une fois à la mise au repos : la
    /// vue le relit à chaque instantané sans le ré-analyser.
    resting_doc: Option<crate::doc::Document>,
    /// Faux pour le lecteur muet de l'app de test : rien n'est écrit ni
    /// effacé sur le disque de l'utilisateur.
    persist: bool,
    /// Aperçu de voix sans texte ouvert.
    lone_preview: Option<preview::LonePreview>,
    /// Dernier échec de cet aperçu.
    preview_error: Option<String>,
    /// L'aperçu de la bibliothèque des voix, et le modèle chargé pour lui.
    library_preview: Option<library::LibraryPreview>,
    /// Choix de langue faits à la main, par document.
    /// Relus au premier besoin, et seulement si le lecteur écrit sur disque.
    choices: Option<language::LanguageChoices>,
    /// Où les choix s'écrivent (le dossier de données, sauf test).
    choices_path: Option<std::path::PathBuf>,
    /// Langue du document gardé.
    language: Option<language::DocumentLanguage>,
    /// L'app fournit le calcul MLX : le paquet Kokoro MLX est proposé.
    mlx: bool,
    /// `config.toml` existe mais n'a pu être lu : le lecteur tourne sur les
    /// défauts, et le passage à Kokoro ne l'écrase pas.
    config_unreadable: bool,
    /// Où `config.toml` s'écrit (`Config::path`, sauf test).
    config_path: Option<std::path::PathBuf>,
    /// L'avis du passage à Kokoro, jusqu'à ce que l'interface le ferme.
    notice: Option<String>,
    /// La proposition de passer à Kokoro, en attente d'une réponse.
    offer: Option<migration::Offer>,
    /// Paquet accepté en téléchargement, et la voix qu'il remplacera : il
    /// devient actif à la fin du téléchargement.
    switch_when_downloaded: Option<(String, String)>,
    /// Lance un téléchargement (jamais le réseau sous test).
    start_download: fn(&'static crate::models::ModelPackage) -> anyhow::Result<crate::models::Download>,
    /// L'espace libre du volume des modèles (jamais le disque sous test).
    free_space: fn() -> Option<u64>,
    /// La sortie choisie, vue une fois par seconde au plus (`watch_output`).
    output_watch: Option<OutputWatch>,
    /// Une sortie de ce nom est-elle branchée ? Remplaçable sous test.
    output_present: fn(&str) -> bool,
}

/// Le dernier coup d'œil sur la sortie choisie.
#[derive(Debug, Clone, Copy)]
struct OutputWatch {
    checked: Instant,
    present: bool,
}

/// L'intervalle entre deux coups d'œil : l'énumération des sorties coûte
/// environ 2 ms (mesuré sur M5, 14 oct. 2026).
const OUTPUT_WATCH_INTERVAL: Duration = Duration::from_secs(1);

impl Reader {
    pub fn new(config: Config, voices: VoiceFactory, players: PlayerFactory, now: Instant) -> Self {
        Self {
            config,
            voices,
            players,
            engine: None,
            session: None,
            text: None,
            resting_at: None,
            revision: 0,
            last_activity: now,
            download: None,
            resting_doc: None,
            persist: true,
            lone_preview: None,
            preview_error: None,
            library_preview: None,
            choices: None,
            choices_path: language::LanguageChoices::path(),
            language: None,
            mlx: false,
            config_unreadable: false,
            config_path: Config::path(),
            notice: None,
            offer: None,
            switch_when_downloaded: None,
            start_download: crate::models::Download::start,
            free_space: crate::models::free_space,
            output_watch: None,
            output_present: crate::audio::player::output_present,
        }
    }

    /// Sous test : la présence des sorties est décidée par l'essai.
    #[cfg(test)]
    pub(crate) fn set_output_presence(&mut self, present: fn(&str) -> bool) {
        self.output_present = present;
    }

    /// Le son ne sort que par la sortie choisie. Retirée (écouteurs
    /// débranchés, partis vers un autre appareil), la lecture se met en
    /// pause au lieu de passer aux haut-parleurs ; rebranchée, la sortie est
    /// rouverte et la lecture attend qu'on la reprenne. `force` regarde tout
    /// de suite, sans attendre la seconde (une commande vient d'arriver).
    fn watch_output(&mut self, now: Instant, force: bool) {
        let Some(app) = self.session.as_mut() else { return };
        let name = app.config.app.output_device.trim().to_string();
        if name.is_empty() {
            self.output_watch = None;
            return;
        }
        if let Some(watch) = self.output_watch {
            if !force && now.saturating_duration_since(watch.checked) < OUTPUT_WATCH_INTERVAL {
                return;
            }
        }
        let present = (self.output_present)(&name);
        let was = self.output_watch.map_or(app.player.is_available(), |w| w.present);
        self.output_watch = Some(OutputWatch { checked: now, present });
        if was && !present {
            if app.player.is_playing() {
                app.player.pause();
            }
            app.notify(&crate::tr!(en = "{name} unplugged: reading paused", fr = "{name} débranché : lecture en pause"));
        } else if present && !was {
            app.replace_player((self.players)(&name), false);
            app.notify(&crate::tr!(en = "{name} is back: Space to resume", fr = "{name} de retour : Espace pour reprendre"));
        }
    }

    /// La sortie choisie change : le son y passe tout de suite, la lecture
    /// continue si elle tournait.
    fn switch_output(&mut self) {
        self.output_watch = None;
        let Some(app) = self.session.as_mut() else { return };
        let playing = app.player.is_playing();
        let player = (self.players)(&app.config.app.output_device);
        let missing = player.error().map(str::to_string);
        app.replace_player(player, playing && missing.is_none());
        if let Some(err) = missing {
            app.notify(&crate::tr!(en = "audio output unavailable: {err}", fr = "sortie audio indisponible : {err}"));
        }
    }

    /// L'app fournit le calcul MLX : le paquet Kokoro MLX est proposé.
    pub fn set_mlx_compute(&mut self, available: bool) {
        self.mlx = available;
    }

    /// `config.toml` n'a pu être lu (`Config::load` l'a signalé) : la
    /// migration de la voix ne l'écrira pas.
    pub fn set_config_unreadable(&mut self, unreadable: bool) {
        self.config_unreadable = unreadable;
    }

    pub fn config_unreadable(&self) -> bool {
        self.config_unreadable
    }

    /// Un réglage ou une voix choisie s'écrit dans `config.toml` : jamais
    /// hors disque, jamais par-dessus un fichier illisible (il serait
    /// remplacé par les défauts). L'interface le dit et propose de le
    /// remplacer (`replace_unreadable_config`).
    pub fn writes_config(&self) -> bool {
        self.persist && !self.config_unreadable
    }

    /// Où `config.toml` s'écrit (un fichier temporaire sous test).
    pub fn set_config_path(&mut self, path: Option<std::path::PathBuf>) {
        self.config_path = path;
    }

    /// Écrit la configuration, si `writes_config` le permet ; sinon rien.
    pub(super) fn save_config(&self) -> anyhow::Result<()> {
        if !self.writes_config() {
            return Ok(());
        }
        let path = self.config_path.as_deref().ok_or_else(|| anyhow::anyhow!(crate::tr!("configuration directory not found", "répertoire de configuration introuvable")))?;
        self.config.save_to(path)
    }

    /// « Remplacer » : la copie illisible est gardée à côté
    /// (`config.toml.illisible`, puis `-2`, `-3`… : jamais une copie écrasée),
    /// puis les réglages en cours sont écrits ; les écritures reprennent.
    pub fn replace_unreadable_config(&mut self) -> anyhow::Result<()> {
        use anyhow::Context;
        if !self.persist {
            return Err(anyhow::anyhow!(data::TRIAL.get()));
        }
        if !self.config_unreadable {
            return Err(anyhow::anyhow!(crate::tr!(
                "the configuration is readable: nothing to replace",
                "la configuration est lisible : rien à remplacer"
            )));
        }
        let path = self.config_path.clone().ok_or_else(|| anyhow::anyhow!(crate::tr!("configuration directory not found", "répertoire de configuration introuvable")))?;
        if path.exists() {
            let kept = (1..)
                .map(|n| {
                    let mut name = path.as_os_str().to_owned();
                    name.push(if n == 1 { ".illisible".to_string() } else { format!(".illisible-{n}") });
                    std::path::PathBuf::from(name)
                })
                .find(|p| !p.exists())
                .expect("un nom libre");
            std::fs::copy(&path, &kept).with_context(|| crate::tr!(en = "copying {}", fr = "copie de {}", path.display()))?;
        }
        self.sync_config();
        self.config.save_to(&path)?;
        self.config_unreadable = false;
        Ok(())
    }

    /// Un paquet du catalogue que ce lecteur sait charger.
    pub fn package(&self, id: &str) -> anyhow::Result<&'static crate::models::ModelPackage> {
        crate::models::available_with(self.mlx)
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!(crate::tr!(en = "unknown voice: {id}", fr = "voix inconnue : {id}")))
    }

    pub fn session(&self) -> Option<&App> {
        self.session.as_ref()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Hors disque : les réglages s'appliquent sans s'écrire, et rien ne
    /// s'efface (modèles, cache).
    pub fn set_persistent(&mut self, on: bool) {
        self.persist = on;
        if !on {
            // Hors disque : ni lecture ni écriture des choix de langue.
            self.choices = Some(language::LanguageChoices::default());
        }
    }

    /// Change à chaque nouveau document — pas à chaque réveil de la voix.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn voice_loaded(&self) -> bool {
        self.engine.is_some()
    }

    pub fn resting_document(&self) -> Option<&crate::doc::Document> {
        self.resting_doc.as_ref()
    }

    pub fn is_resting(&self) -> bool {
        self.resting_at.is_some()
    }

    pub fn engine(&self) -> Option<&Arc<dyn TtsEngine>> {
        self.engine.as_ref()
    }

    pub fn text_markdown(&self) -> Option<&str> {
        self.text.as_ref().map(|t| t.markdown.as_str())
    }

    /// Charge la voix sans attendre de texte : le raccourci la demande
    /// pendant que la sélection se lit.
    pub fn prepare_voice(&mut self, now: Instant) {
        self.last_activity = now;
        self.ensure_engine();
    }

    /// Ouvre un texte capturé (sélection, presse-papiers). Son identité — sous
    /// laquelle sa position se range — est sa source suivie d'une empreinte du
    /// texte : deux sélections ne partagent pas leur position, et le même texte,
    /// recapturé, retrouve la sienne.
    pub fn open_text(&mut self, markdown: &str, source: &str, now: Instant) {
        let source = format!(
            "{source}:{:016x}",
            crate::source::cache::stable_hash(markdown.as_bytes())
        );
        self.open(Text { markdown: markdown.to_string(), source, identity: false }, now);
        self.revision += 1;
    }

    /// L'identité du document gardé, ouvert ou au repos.
    pub fn document_identity(&self) -> Option<&str> {
        match &self.session {
            Some(app) => Some(app.source.as_str()),
            None => self.text.as_ref().map(|t| t.source.as_str()),
        }
    }

    pub fn open_file(&mut self, path: &Path, now: Instant) -> anyhow::Result<()> {
        let markdown = crate::source::load(path)?;
        let source = crate::position::document_identity(path);
        // Le choix de langue suit le fichier, comme sa position : il doit
        // être là avant l'ouverture, qui en dépend.
        if let Some(name) = path.file_name() {
            if self.choices().adopt(&name.to_string_lossy(), &source) && self.persist {
                let _ = self.save_choices();
            }
        }
        self.open(Text { markdown, source, identity: true }, now);
        if let Some(app) = self.session.as_mut() {
            if let Some(name) = path.file_name() {
                app.adopt_former_identity(&name.to_string_lossy());
            }
            app.resume_where_left();
        }
        self.revision += 1;
        Ok(())
    }

    /// Applique une commande au document en cours, en réveillant la voix
    /// si elle était au repos.
    pub fn with_session(&mut self, now: Instant, f: impl FnOnce(&mut App)) {
        self.last_activity = now;
        self.wake(now);
        self.watch_output(now, true);
        if let Some(app) = self.session.as_mut() {
            f(app);
        }
    }

    /// Applique une commande de l'interface. Une pause n'a rien à arrêter
    /// sans texte ou voix au repos : elle ne réveille pas la voix pour rien.
    pub fn apply(&mut self, command: command::Command, now: Instant) {
        let idle = self.is_resting() || self.session.is_none();
        if command == command::Command::Pause && idle {
            return;
        }
        let speed = matches!(command, command::Command::SetSpeed(_) | command::Command::ChangeSpeed(_));
        self.with_session(now, |app| command.apply(app));
        // La vitesse du panneau tient au prochain lancement, comme celle des
        // réglages. Le volume, lui, s'écrit au relâchement du curseur.
        if speed && self.session.as_ref().is_some_and(|app| app.config.voice.speed != self.config.voice.speed) {
            self.sync_config();
            let _ = self.save_config();
        }
    }

    pub fn tick(&mut self, now: Instant) {
        self.poll_download(now);
        self.poll_lone_preview();
        self.poll_library_preview();
        let Some(app) = self.session.as_mut() else {
            // Voix préparée sans texte : le délai court quand même.
            // Un modèle chargé pour l'aperçu de la bibliothèque aussi.
            let loaded = self.engine.is_some() || self.library_preview.is_some();
            if loaded && !self.previewing() && self.idle_expired(now) {
                self.rest();
            }
            return;
        };
        app.tick();
        self.watch_output(now, false);
        let Some(app) = self.session.as_mut() else { return };
        // Le délai ne court qu'à l'arrêt : avec « relâchée dès l'arrêt »
        // (0 min), il serait sinon écoulé à chaque tour, lecture comprise.
        if app.player.is_playing() && !app.is_finished() {
            self.last_activity = now;
        } else if !self.previewing() && self.idle_expired(now) {
            self.rest();
        }
    }

    /// Relâche la voix, la synthèse et la sortie audio ; garde le texte et
    /// la phrase pour reprendre.
    pub fn rest(&mut self) {
        self.drop_lone_preview();
        self.drop_library_preview();
        let Some(mut app) = self.session.take() else {
            self.engine = None;
            return;
        };
        app.remember_position();
        self.config = app.config.clone();
        self.resting_at = Some(app.current_sentence());
        // Le document de la session est celui qu'une nouvelle analyse
        // rendrait : il est gardé tel quel, sans relire tout le texte.
        self.resting_doc = self.text.as_ref().map(|_| app.into_document());
        self.engine = None;
    }

    /// Le lecteur fermé : le texte part, lecture arrêtée, rien ne reste à
    /// reprendre (ni position, ni texte au repos). La voix reste chargée :
    /// son délai court comme sans texte.
    pub fn clear(&mut self, now: Instant) {
        if let Some(mut app) = self.session.take() {
            app.player.pause();
            self.config = app.config.clone();
        }
        self.text = None;
        self.resting_at = None;
        self.resting_doc = None;
        self.language = None;
        self.last_activity = now;
        self.revision += 1;
    }

    /// Fermeture de l'app : rien de ce qui a été lu ne se perd.
    pub fn close(&mut self) {
        if let Some(app) = self.session.as_mut() {
            app.remember_position();
        }
    }

    fn idle_expired(&self, now: Instant) -> bool {
        let minutes = self.config.app.keep_voice_minutes;
        minutes >= 0
            && now.saturating_duration_since(self.last_activity)
                >= Duration::from_secs(minutes as u64 * 60)
    }

    fn ensure_engine(&mut self) -> Arc<dyn TtsEngine> {
        if let Some(engine) = &self.engine {
            return Arc::clone(engine);
        }
        let engine = (self.voices)(&self.config);
        self.engine = Some(Arc::clone(&engine));
        engine
    }

    fn open(&mut self, text: Text, now: Instant) {
        self.drop_lone_preview();
        self.last_activity = now;
        self.resting_at = None;
        self.resting_doc = None;
        let engine = self.ensure_engine();
        let language = self.language_of(&text.markdown, &text.source);
        let (synth, player) = match self.session.take() {
            Some(mut old) => {
                old.remember_position();
                self.config = old.config.clone();
                old.into_parts()
            }
            None => (
                Synthesizer::new(
                    engine,
                    self.config.tts_params_in(&language.effective),
                    self.config.sessions() as usize,
                    self.config.engine.prefetch as usize,
                ),
                (self.players)(&self.config.app.output_device),
            ),
        };
        synth.reset();
        let mut builder = DocumentBuilder::with_language(self.config.code_policy(), &language.effective);
        self.language = Some(language);
        for block in crate::doc::blocks::split_blocks(&text.markdown) {
            builder.push_block(&block);
        }
        let mut app = App::from_parts(
            builder,
            None,
            synth,
            self.config.clone(),
            text.source.clone(),
            Vec::new(),
            player,
        );
        // Le terminal liste les avertissements de l'ouverture (sortie audio
        // indisponible, signets illisibles) ; l'interface ne montre que le
        // statut : ils y passent.
        let notices = std::mem::take(&mut app.notices);
        if !notices.is_empty() {
            app.notify(&notices.join(" · "));
        }
        self.session = Some(app);
        self.text = Some(text);
    }

    /// Rouvre le texte mis au repos, à sa phrase, lecture arrêtée : la
    /// commande qui a réveillé la voix décide de la suite.
    fn wake(&mut self, now: Instant) {
        let Some(at) = self.resting_at else { return };
        let Some(text) = self.text.clone() else { return };
        let identity = text.identity;
        self.open(text, now);
        if let Some(app) = self.session.as_mut() {
            app.player.pause();
            if identity {
                app.resume_where_left();
            }
            app.seek_to(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn reader(keep: i64) -> (Reader, Arc<AtomicUsize>, Instant) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.app.keep_voice_minutes = keep;
        let t0 = Instant::now();
        let reader = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(|_: &str| Player::silent()),
            t0,
        );
        (reader, loads, t0)
    }

    fn written(reader: &Reader) -> String {
        reader.session().unwrap().doc().sentences[0].written()
    }

    const MIN: Duration = Duration::from_secs(60);

    /// Un lecteur dont la sortie est « Casque », branché selon `present`, et
    /// des lecteurs audio qui ont une file (sans carte son).
    fn on_headphones(present: fn(&str) -> bool) -> (Reader, Arc<AtomicUsize>, Instant) {
        let opened = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&opened);
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.app.output_device = "Casque".into();
        let t0 = Instant::now();
        let mut r = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(move |name: &str| {
                assert_eq!(name, "Casque");
                counter.fetch_add(1, Ordering::SeqCst);
                Player::idle(48_000).0
            }),
            t0,
        );
        r.set_output_presence(present);
        (r, opened, t0)
    }

    static CASQUE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

    #[test]
    fn unplugged_headphones_pause_instead_of_switching_to_the_speakers() {
        let (mut r, opened, t0) = on_headphones(|_| CASQUE.load(Ordering::SeqCst));
        r.open_text("Une phrase. Une autre.", "t", t0);
        r.with_session(t0, |app| app.player.play());
        assert_eq!(opened.load(Ordering::SeqCst), 1);

        CASQUE.store(false, Ordering::SeqCst);
        // Pas avant la seconde : l'énumération des sorties a un coût.
        r.tick(t0 + Duration::from_millis(300));
        assert!(r.session().unwrap().player.is_playing());
        r.tick(t0 + Duration::from_millis(1100));
        assert!(!r.session().unwrap().player.is_playing(), "en pause, jamais aux haut-parleurs");
        assert!(r.session().unwrap().status().contains("Casque débranché"), "{}", r.session().unwrap().status());

        // De retour : la sortie est rouverte, la lecture attend qu'on la reprenne.
        CASQUE.store(true, Ordering::SeqCst);
        r.tick(t0 + Duration::from_millis(2200));
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        assert!(!r.session().unwrap().player.is_playing());
        assert!(r.session().unwrap().status().contains("Casque de retour"));
        // Une fois rouverte, plus rien à refaire.
        r.tick(t0 + Duration::from_millis(3300));
        assert_eq!(opened.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_system_output_is_never_watched() {
        let (mut r, _, t0) = reader(10);
        r.set_output_presence(|_| panic!("aucune sortie choisie : rien à surveiller"));
        r.open_text("Une phrase.", "t", t0);
        r.tick(t0 + Duration::from_secs(5));
    }

    #[test]
    fn choosing_another_output_moves_the_sound_and_keeps_reading() {
        let (mut r, opened, t0) = on_headphones(|_| true);
        r.open_text("Une phrase. Une autre.", "t", t0);
        r.with_session(t0, |app| app.player.play());
        // La même sortie choisie de nouveau : rien ne se rouvre.
        let same = settings::SettingsPatch { output_device: Some("Casque".into()), ..Default::default() };
        r.apply_patch(&same, t0);
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        // Une autre valeur (ici rendue par la même fabrique) : rouverte, lecture continue.
        r.config.app.output_device = "Autre".into();
        if let Some(app) = r.session.as_mut() {
            app.config.app.output_device = "Autre".into();
        }
        r.apply_patch(&same, t0);
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        assert!(r.session().unwrap().player.is_playing());
    }

    #[test]
    fn two_texts_share_one_voice() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Premier texte.", "a", t0);
        r.open_text("Second texte.", "b", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(r.revision(), 2);
        assert_eq!(written(&r), "Second texte.");
    }

    #[test]
    fn the_second_text_replaces_the_first() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.open_text("Autre.", "b", t0);
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.player.queue_len(), 0);
    }

    #[test]
    fn the_voice_rests_after_the_delay_and_comes_back_in_place() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.seek_sentences(2));
        r.tick(t0 + 9 * MIN);
        assert!(r.voice_loaded());
        r.tick(t0 + 11 * MIN);
        assert!(!r.voice_loaded());
        assert!(r.is_resting());

        // Un déplacement réveille la voix, sans lancer la lecture.
        r.with_session(t0 + 12 * MIN, |app| app.seek_sentences(-1));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(r.revision(), 1, "même document : pas de nouvelle révision");
        let app = r.session().unwrap();
        assert_eq!(app.current_sentence(), 1);
        assert!(!app.player.is_playing());
    }

    #[test]
    fn a_prepared_voice_without_text_is_released_after_the_delay() {
        let (mut r, _, t0) = reader(10);
        r.prepare_voice(t0);
        r.tick(t0 + 9 * MIN);
        assert!(r.voice_loaded());
        r.tick(t0 + 11 * MIN);
        assert!(!r.voice_loaded());
        assert!(!r.is_resting(), "pas de texte à reprendre");
    }

    #[test]
    fn a_prepared_voice_kept_forever_stays_loaded() {
        let (mut r, _, t0) = reader(-1);
        r.prepare_voice(t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
    }

    #[test]
    fn keeping_forever_never_rests() {
        let (mut r, _, t0) = reader(-1);
        r.open_text("Un.", "a", t0);
        r.tick(t0 + 600 * MIN);
        assert!(r.voice_loaded());
    }

    #[test]
    fn released_at_stop_never_rests_while_reading() {
        let (mut r, _, t0) = reader(0);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.toggle_play());
        assert!(r.session().unwrap().player.is_playing());
        r.tick(t0 + MIN);
        assert!(r.voice_loaded(), "la lecture en cours garde sa voix");
        assert!(!r.is_resting());
    }

    #[test]
    fn released_at_stop_rests_once_paused() {
        let (mut r, _, t0) = reader(0);
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| app.toggle_play());
        r.with_session(t0, |app| app.toggle_play());
        assert!(!r.session().unwrap().player.is_playing());
        r.tick(t0);
        assert!(!r.voice_loaded());
        assert!(r.is_resting());
    }

    #[test]
    fn pausing_a_resting_text_does_not_wake_the_voice() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply(command::Command::Pause, t0 + MIN);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "Échap ne recharge pas la voix");
        assert!(r.is_resting());
        assert!(!r.voice_loaded());
    }

    #[test]
    fn clearing_leaves_nothing_to_play() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.apply(command::Command::Sentences(1), t0);
        let revision = r.revision();
        r.clear(t0);
        assert!(r.session().is_none());
        assert!(r.text_markdown().is_none());
        assert!(r.revision() > revision, "le document vide se republie");
        r.apply(command::Command::TogglePlay, t0);
        assert!(r.session().is_none(), "lecture : rien ne revient");
        let s = view::snapshot(&r);
        assert!(!s.has_document);
        assert!(!s.playing);
    }

    #[test]
    fn clearing_a_resting_text_leaves_nothing_to_wake() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.clear(t0);
        r.apply(command::Command::TogglePlay, t0);
        assert!(r.session().is_none());
        assert_eq!(view::document(&r).sentence_count, 0);
    }

    #[test]
    fn pausing_without_a_text_does_nothing() {
        let (mut r, loads, t0) = reader(10);
        r.apply(command::Command::Pause, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        assert!(r.session().is_none());
    }

    #[test]
    fn other_commands_still_wake_a_resting_text() {
        let (mut r, loads, t0) = reader(10);
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply(command::Command::Sentences(1), t0);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(r.session().unwrap().current_sentence(), 1);
    }

    #[test]
    fn each_captured_text_has_its_own_identity() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Premier texte.", "sélection", t0);
        let first = r.document_identity().unwrap().to_string();
        r.open_text("Second texte.", "sélection", t0);
        let second = r.document_identity().unwrap().to_string();
        assert_ne!(first, second, "deux sélections ne partagent pas leur position");
        assert!(first.starts_with("sélection:"));
        r.open_text("Premier texte.", "sélection", t0);
        assert_eq!(r.document_identity().unwrap(), first, "le même texte se retrouve");
        assert_eq!(r.session().unwrap().source, first);
    }

    #[test]
    fn notices_of_the_opening_reach_the_status() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        let app = r.session().unwrap();
        // La sortie muette des essais se dit indisponible : l'interface le
        // montre au lieu de le garder dans une liste qu'elle ne lit pas.
        assert!(app.status().contains("sortie audio indisponible"), "{}", app.status());
        assert!(app.notices.is_empty(), "dites une seule fois");
    }

    #[test]
    fn speed_carries_over_to_the_next_text() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        r.with_session(t0, |app| app.set_speed(1.5));
        r.open_text("Deux.", "b", t0);
        assert_eq!(r.session().unwrap().config.voice.speed, 1.5);
    }

    #[test]
    fn the_volume_is_set_at_once_and_brings_the_sound_back() {
        let (mut r, _, t0) = reader(10);
        r.open_text("Un.", "a", t0);
        r.apply(command::Command::ToggleMute, t0);
        r.apply(command::Command::SetVolume(0.4), t0);
        let app = r.session().unwrap();
        assert_eq!(app.player.volume(), 0.4);
        assert_eq!(app.config.voice.volume, 0.4);
        assert!(!app.player.is_muted(), "régler le volume rétablit le son");
        r.apply(command::Command::SetVolume(9.0), t0);
        assert_eq!(r.session().unwrap().player.volume(), 2.0, "borné à 200 %");
        r.apply(command::Command::SetVolume(f32::NAN), t0);
        assert_eq!(r.session().unwrap().player.volume(), 2.0, "une valeur sans sens est ignorée");
    }

    /// La vitesse choisie dans le panneau (menu, + −, 1 à 4) s'écrit tout
    /// de suite : elle tient au prochain lancement.
    #[test]
    fn a_speed_chosen_in_the_panel_is_saved() {
        let dir = crate::paths::test_scratch("vitesse-enregistree");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let (mut r, _, t0) = reader(10);
        r.set_config_path(Some(path.clone()));
        r.open_text("Un.", "a", t0);
        r.apply(command::Command::SetSpeed(1.5), t0);
        let after_menu = std::fs::read_to_string(&path).unwrap_or_default();
        r.apply(command::Command::ChangeSpeed(0.25), t0);
        let after_key = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(after_menu.contains("speed = 1.5"), "{after_menu}");
        assert!(after_key.contains("speed = 1.75"), "{after_key}");
    }

    #[test]
    fn preparing_the_voice_loads_it_once() {
        let (mut r, loads, t0) = reader(10);
        r.prepare_voice(t0);
        r.prepare_voice(t0);
        r.open_text("Un.", "a", t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
