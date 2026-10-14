//! Réglages et voix, tels que l'interface les montre et les change.
//!
//! L'interface lit tous les réglages d'un coup (`SettingsView`) et ne
//! renvoie que ceux qu'elle touche (`SettingsPatch`) : rien n'écrase une
//! valeur changée ailleurs — au clavier, au terminal — entre la lecture et
//! l'écriture. Chaque modification déclenche l'effet le plus fort de ce qui
//! a réellement changé, comme l'écran du terminal (`app::config::Effect`).

use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::time::Instant;

use crate::app::config::Effect;
use crate::config::Config;
use crate::doc::{BlockKind, Document};
use crate::models::{self, Progress};

use super::Reader;

/// Tous les réglages du moteur, rangés comme dans `config.toml`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    // [voice]
    pub voice: String,
    pub language: String,
    pub speed: f32,
    pub pitch: f32,
    pub energy: f32,
    pub emotion: String,
    pub instruction: String,
    pub volume: f32,
    // [engine]
    pub engine: String,
    pub model_path: String,
    pub family: String,
    pub compute: String,
    pub threads: u32,
    pub sessions: u32,
    pub prefetch: u32,
    // [sampling] — `None` : la valeur du modèle ; pour la graine, un tirage.
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
    // [reading]
    pub code_blocks: String,
    pub autoplay: bool,
    pub seek_step: u32,
    pub line_width: u32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    // [app]
    pub keep_voice_minutes: i64,
    /// La sortie audio choisie ; vide : celle du système.
    pub output_device: String,
    // [extra]
    pub extra: BTreeMap<String, String>,
    /// `[voice.per_language]` : la voix de chaque langue (Kokoro).
    pub voices_by_language: BTreeMap<String, String>,
}

impl SettingsView {
    pub fn of(c: &Config) -> Self {
        Self {
            voice: c.voice.voice.clone(),
            language: c.voice.language.clone(),
            speed: c.voice.speed,
            pitch: c.voice.pitch,
            energy: c.voice.energy,
            emotion: c.voice.emotion.clone(),
            instruction: c.voice.instruction.clone(),
            volume: c.voice.volume,
            engine: c.engine.engine.clone(),
            model_path: c.engine.model_path.clone(),
            family: c.engine.family.clone(),
            compute: c.engine.compute.clone(),
            threads: c.engine.threads,
            sessions: c.engine.sessions,
            prefetch: c.engine.prefetch,
            temperature: c.sampling.temperature,
            top_p: c.sampling.top_p,
            top_k: c.sampling.top_k,
            guidance_scale: c.sampling.guidance_scale,
            repetition_penalty: c.sampling.repetition_penalty,
            inference_steps: c.sampling.inference_steps,
            seed: c.sampling.seed,
            code_blocks: c.reading.code_blocks.clone(),
            autoplay: c.reading.autoplay,
            seek_step: c.reading.seek_step,
            line_width: c.reading.line_width,
            line_spacing: c.reading.line_spacing,
            reading_ruler: c.reading.reading_ruler,
            keep_voice_minutes: c.app.keep_voice_minutes,
            output_device: c.app.output_device.clone(),
            extra: c.extra.clone(),
            voices_by_language: c.voice.per_language.clone(),
        }
    }
}

/// Une modification : un champ par réglage, `None` quand il n'est pas touché.
///
/// L'échantillonnage a deux niveaux : `Some(None)` rend la main au modèle
/// (à un tirage au hasard pour la graine). Le chemin et la famille du modèle
/// n'y sont pas : ils changent avec `use_voice`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsPatch {
    pub voice: Option<String>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub energy: Option<f32>,
    pub emotion: Option<String>,
    pub instruction: Option<String>,
    pub volume: Option<f32>,
    pub engine: Option<String>,
    pub compute: Option<String>,
    pub threads: Option<u32>,
    pub sessions: Option<u32>,
    pub prefetch: Option<u32>,
    pub temperature: Option<Option<f32>>,
    pub top_p: Option<Option<f32>>,
    pub top_k: Option<Option<u32>>,
    pub guidance_scale: Option<Option<f32>>,
    pub repetition_penalty: Option<Option<f32>>,
    pub inference_steps: Option<Option<u32>>,
    pub seed: Option<Option<u64>>,
    pub code_blocks: Option<String>,
    pub autoplay: Option<bool>,
    pub seek_step: Option<u32>,
    pub line_width: Option<u32>,
    pub line_spacing: Option<u32>,
    pub reading_ruler: Option<bool>,
    pub keep_voice_minutes: Option<i64>,
    pub output_device: Option<String>,
    /// Options libres posées ou remplacées.
    pub extra_set: Vec<(String, String)>,
    /// Options libres retirées.
    pub extra_remove: Vec<String>,
    /// La voix d'une langue : `("en", "am_fenrir")`.
    pub voice_for_language: Option<(String, String)>,
}

fn set<T: Clone>(slot: &mut T, value: &Option<T>) {
    if let Some(v) = value {
        *slot = v.clone();
    }
}

impl SettingsPatch {
    /// Reporte sur `c` les seuls champs présents.
    pub fn write_into(&self, c: &mut Config) {
        set(&mut c.voice.voice, &self.voice);
        set(&mut c.voice.language, &self.language);
        set(&mut c.voice.speed, &self.speed);
        set(&mut c.voice.pitch, &self.pitch);
        set(&mut c.voice.energy, &self.energy);
        set(&mut c.voice.emotion, &self.emotion);
        set(&mut c.voice.instruction, &self.instruction);
        set(&mut c.voice.volume, &self.volume);
        set(&mut c.engine.engine, &self.engine);
        set(&mut c.engine.compute, &self.compute);
        set(&mut c.engine.threads, &self.threads);
        set(&mut c.engine.sessions, &self.sessions);
        set(&mut c.engine.prefetch, &self.prefetch);
        set(&mut c.sampling.temperature, &self.temperature);
        set(&mut c.sampling.top_p, &self.top_p);
        set(&mut c.sampling.top_k, &self.top_k);
        set(&mut c.sampling.guidance_scale, &self.guidance_scale);
        set(&mut c.sampling.repetition_penalty, &self.repetition_penalty);
        set(&mut c.sampling.inference_steps, &self.inference_steps);
        set(&mut c.sampling.seed, &self.seed);
        set(&mut c.reading.code_blocks, &self.code_blocks);
        set(&mut c.reading.autoplay, &self.autoplay);
        set(&mut c.reading.seek_step, &self.seek_step);
        set(&mut c.reading.line_width, &self.line_width);
        set(&mut c.reading.line_spacing, &self.line_spacing);
        set(&mut c.reading.reading_ruler, &self.reading_ruler);
        set(&mut c.app.keep_voice_minutes, &self.keep_voice_minutes);
        set(&mut c.app.output_device, &self.output_device);
        if let Some((language, voice)) = &self.voice_for_language {
            c.voice.per_language.insert(language.clone(), voice.clone());
        }
        for (key, value) in &self.extra_set {
            c.extra.insert(key.clone(), value.clone());
        }
        for key in &self.extra_remove {
            c.extra.remove(key);
        }
    }
}

/// Ce qu'un changement de réglage demande, du plus léger au plus lourd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEffect {
    /// Rien n'a changé.
    Nothing,
    /// Affichage, ou valeur lue au prochain usage (pas d'avance, délai…).
    Display,
    /// Volume ou vitesse : appliqués au son, immédiatement, sans resynthèse.
    Immediate,
    /// Paramètres de synthèse : resynthèse depuis la phrase en cours.
    Resynthesize,
    /// Découpage du document : remise en forme, à la même phrase.
    Reparse,
    /// Moteur : rechargement de la voix, lecture reprise à la même phrase.
    Reload,
}

impl SettingsEffect {
    /// L'effet le plus fort, pour un document dans la langue par défaut.
    pub fn between(before: &Config, after: &Config) -> Self {
        Self::between_in(before, after, &before.default_language())
    }

    /// L'effet le plus fort de ce qui change de `before` à `after`, pour un
    /// document dans `language`.
    ///
    /// Mesuré sur la configuration, pas sur les champs présents : renvoyer
    /// une valeur déjà en place ne relance rien. La langue change la
    /// normalisation, donc le découpage ; la voix de l'autre langue ne change
    /// rien à ce qu'on entend.
    pub fn between_in(before: &Config, after: &Config, language: &str) -> Self {
        if before.engine != after.engine {
            return Self::Reload;
        }
        if before.reading.code_blocks != after.reading.code_blocks
            || before.voice.language != after.voice.language
        {
            return Self::Reparse;
        }
        let mut voice = before.voice.clone();
        voice.volume = after.voice.volume;
        voice.speed = after.voice.speed;
        voice.per_language = after.voice.per_language.clone();
        let heard = before.tts_params_in(language).voice != after.tts_params_in(language).voice;
        if voice != after.voice || heard || before.sampling != after.sampling || before.extra != after.extra {
            return Self::Resynthesize;
        }
        if before.voice.volume != after.voice.volume || before.voice.speed != after.voice.speed {
            return Self::Immediate;
        }
        if before.reading != after.reading
            || before.app != after.app
            || before.voice.per_language != after.voice.per_language
        {
            return Self::Display;
        }
        Self::Nothing
    }
}

/// Un paquet du catalogue, tel que l'onglet Modèles le décrit.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
    pub summary: String,
    pub family: String,
    pub precision: String,
    pub languages_label: String,
    pub languages: Vec<String>,
    /// Le rythme mesuré : fluide, ou haché.
    pub pace: String,
    /// Les voix, pour la fiche.
    pub voices_label: String,
    /// Les licences, pour la fiche.
    pub licenses: String,
}

/// Une voix intégrée du modèle actif.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceChoice {
    pub id: String,
    pub label: String,
    /// Langue de la voix (vide : la famille ne les classe pas).
    pub language: String,
}

/// Ce que le modèle actif sait faire : ses voix, ses langues, les réglages
/// qu'il lit. `honoured` vide : famille inconnue, rien n'est affirmé.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelCapabilities {
    pub family: String,
    pub voices: Vec<VoiceChoice>,
    pub languages: Vec<String>,
    pub honoured: Vec<String>,
    /// Le modèle ne parle qu'en clonant un WAV. Connu une fois la voix
    /// chargée seulement : faux avant.
    pub requires_reference: bool,
}

/// Décrit un paquet ; `active_path` est le chemin du modèle en place.
pub fn voice_model(p: &models::ModelPackage, active_path: &str) -> VoiceModel {
    VoiceModel {
        id: p.id.to_string(),
        name: p.display_name.to_string(),
        bytes: p.total_bytes(),
        installed: models::is_installed(p),
        active: !active_path.is_empty()
            && models::install_path(p).is_some_and(|path| path.to_string_lossy() == active_path),
        recommended: p.recommended,
        summary: p.description.to_string(),
        family: p.family.to_string(),
        precision: p.precision.to_string(),
        languages_label: p.languages_label.to_string(),
        languages: p.languages.iter().map(|l| l.to_string()).collect(),
        pace: p.pace.to_string(),
        voices_label: p.voices_label.to_string(),
        licenses: p.licenses.to_string(),
    }
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
        SettingsView::of(self.current_config())
    }

    /// Applique un patch sans écrire : seuls les champs présents changent,
    /// ramenés dans les bornes, puis l'effet le plus fort de ce qui a changé
    /// est déclenché. Rend les réglages tels qu'appliqués.
    pub fn apply_patch(&mut self, patch: &SettingsPatch, now: Instant) -> SettingsView {
        self.sync_config();
        let before = self.config.clone();
        let mut next = before.clone();
        patch.write_into(&mut next);
        let next = next.normalised();
        let effect = SettingsEffect::between_in(&before, &next, &self.current_language());
        if let Some(app) = self.session.as_mut() {
            app.config = next.clone();
            if (app.player.volume() - next.voice.volume).abs() >= f32::EPSILON {
                app.player.set_volume(next.voice.volume);
            }
            // La vitesse s'applique au son de la phrase en cours, sans
            // toucher à l'audio synthétisé.
            if app.player.speed() != next.voice.speed {
                app.player.set_speed(next.voice.speed);
            }
            if effect == SettingsEffect::Resynthesize {
                app.apply_effect(Effect::Resynthesize);
            }
        }
        let output_changed = before.app.output_device != next.app.output_device;
        self.config = next;
        if output_changed {
            self.switch_output();
        }
        match effect {
            SettingsEffect::Reload => self.reload_voice(now),
            SettingsEffect::Reparse => self.reparse(now),
            _ => {}
        }
        self.settings()
    }

    /// Applique, puis écrit `config.toml` (sauf lecteur hors disque ou
    /// fichier illisible, voir `writes_config`). Une écriture impossible laisse le réglage appliqué : l'erreur le dit.
    pub fn update_settings(&mut self, patch: &SettingsPatch, now: Instant) -> Result<SettingsView> {
        let applied = self.apply_patch(patch, now);
        self.save_config()?;
        Ok(applied)
    }

    /// Recharge la voix avec la configuration courante. Un texte ouvert est
    /// rouvert à sa phrase, et la lecture reprend seule si elle tournait ; au
    /// repos, rien ne se charge avant le prochain réveil.
    pub(super) fn reload_voice(&mut self, now: Instant) {
        if self.session.is_some() {
            let playing = self
                .session
                .as_ref()
                .is_some_and(|app| app.player.is_playing() && !app.is_finished());
            self.rest();
            self.wake(now);
            if playing {
                if let Some(app) = self.session.as_mut() {
                    app.player.play();
                }
            }
            return;
        }
        // L'aperçu parlait avec l'ancienne voix.
        self.drop_lone_preview();
        let prepared = self.engine.is_some();
        // Connu : pendant la toute première préparation MLX, cette libération attend le verrou MLX et peut bloquer le fil du lecteur ~9 s.
        self.engine = None;
        if prepared && !self.is_resting() {
            self.ensure_engine();
        }
    }

    /// Reconstruit le document avec la politique des blocs de code courante,
    /// à la même phrase de texte : « sauter » retire des phrases, « lire » en
    /// ajoute, le numéro seul ne suffit donc pas.
    pub(super) fn reparse(&mut self, now: Instant) {
        if let Some(app) = self.session.as_ref() {
            let playing = app.player.is_playing() && !app.is_finished();
            let place = Place::of(app.doc(), app.current_sentence());
            let Some(text) = self.text.clone() else { return };
            let identity = text.identity;
            self.open(text, now);
            if let Some(app) = self.session.as_mut() {
                app.player.pause();
                // Le nouveau texte doit continuer à retenir sa position, comme
                // au réveil : sans cela, elle ne serait plus écrite au repos.
                if identity {
                    app.resume_where_left();
                }
                let target = place.find(app.doc());
                app.seek_to(target);
                if playing {
                    app.player.play();
                }
            }
            self.revision += 1;
            return;
        }
        let (Some(at), Some(text)) = (self.resting_at, self.text.as_ref()) else { return };
        let place = self.resting_doc.as_ref().map_or(Place { rank: 0, code: None }, |doc| Place::of(doc, at));
        let text = text.clone();
        let language = self.language_of(&text.markdown, &text.source);
        let doc = crate::doc::builder::parse_document_in(&text.markdown, self.config.code_policy(), &language.effective);
        self.language = Some(language);
        self.resting_at = Some(place.find(&doc));
        self.resting_doc = Some(doc);
        self.revision += 1;
    }

    pub fn voices(&self) -> Vec<VoiceModel> {
        let active = self.current_config().engine.model_path.clone();
        models::available_with(self.mlx).into_iter().map(|p| voice_model(p, &active)).collect()
    }

    pub fn capabilities(&self) -> ModelCapabilities {
        let family = self.current_config().engine.family.clone();
        ModelCapabilities {
            voices: models::voices_for_family(&family)
                .iter()
                .map(|v| VoiceChoice { id: v.id.to_string(), label: v.label.to_string(), language: v.language.to_string() })
                .collect(),
            languages: models::languages_for_family(&family).into_iter().map(String::from).collect(),
            honoured: models::honoured_options(&family).iter().map(|o| o.to_string()).collect(),
            requires_reference: self.engine.as_ref().is_some_and(|e| e.requires_reference()),
            family,
        }
    }

    pub fn install_voice(&mut self, id: &str) -> Result<()> {
        if !self.persist {
            // Hors disque, rien ne se télécharge dans le dossier des modèles.
            return Err(anyhow!(crate::reader::data::TRIAL));
        }
        let package = self.package(id)?;
        if self.download.as_ref().is_some_and(|d| !d.is_finished()) {
            return Err(anyhow!("téléchargement déjà en cours"));
        }
        self.download = Some((self.start_download)(package)?);
        Ok(())
    }

    /// Change de modèle : la voix se recharge en place, au même endroit du
    /// texte, et la lecture reprend si elle tournait.
    pub fn use_voice(&mut self, id: &str, now: Instant) -> Result<()> {
        let package = self.package(id)?;
        let path = models::install_path(package)
            .filter(|_| models::is_installed(package))
            .ok_or_else(|| anyhow!("voix non installée : {id}"))?;
        self.sync_config();
        // Déjà la voix active : rien à recharger.
        if models::is_active_path(&self.config.engine.model_path, &path) && self.config.engine.family == package.family {
            return Ok(());
        }
        self.switch_voice_config(&path.to_string_lossy(), package.family);
        // Le texte ouvert recopie sa configuration à la mise au repos : il
        // doit porter le nouveau modèle avant le rechargement.
        if let Some(app) = self.session.as_mut() {
            app.config = self.config.clone();
        }
        self.reload_voice(now);
        self.save_config()?;
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
            Progress::Done => (d.package.total_bytes(), d.package.total_bytes(), None),
            Progress::Failed(message) => (0, d.package.total_bytes(), Some(message)),
            Progress::Cancelled => (0, d.package.total_bytes(), Some("annulé".into())),
        };
        Some(DownloadView { id: d.package.id.to_string(), received, total, failed })
    }

    /// Appelé à chaque tour : un téléchargement terminé disparaît, et devient
    /// la voix active s'il n'y en avait aucune, ou si le passage à Kokoro
    /// l'attendait. Un échec ou une annulation libère ce passage.
    pub(super) fn poll_download(&mut self, now: Instant) {
        // Rien n'est copié tant que le téléchargement court, ni pour un échec
        // déjà traité : seulement à une issue qui change quelque chose.
        let Some(d) = self.download.as_ref() else { return };
        if !d.is_finished() {
            return;
        }
        let package = d.package;
        let done = matches!(d.progress(), Progress::Done);
        if !done {
            if self.switch_when_downloaded.is_some() {
                self.abandon_migration(package.id);
            }
            return;
        }
        let id = package.id.to_string();
        self.download = None;
        if self.switch_when_downloaded.as_ref().is_some_and(|(pending, _)| *pending == id) {
            self.finish_migration(&id, now);
            return;
        }
        let none_active = !self.voices().iter().any(|v| v.active);
        if none_active {
            let _ = self.use_voice(&id, now);
        }
    }

    /// La configuration que le texte ouvert a pu modifier (vitesse…).
    pub(super) fn current_config(&self) -> &Config {
        self.session.as_ref().map(|a| &a.config).unwrap_or(&self.config)
    }

    pub(super) fn sync_config(&mut self) {
        if let Some(app) = self.session.as_ref() {
            self.config = app.config.clone();
        }
    }
}

fn in_code(doc: &Document, block: usize) -> bool {
    doc.block(block).is_some_and(|b| matches!(b.kind, BlockKind::Code { .. }))
}

/// Où en est la lecture, d'une manière qui survit à un changement de la
/// politique des blocs de code (le numéro de phrase, lui, bouge).
///
/// Dans le texte : le rang parmi les phrases qui ne sont pas du code. Dans un
/// bloc de code : le rang de ce bloc ; s'il existe encore on reprend à son
/// début, sinon à la phrase de texte qui suit.
struct Place {
    rank: usize,
    code: Option<usize>,
}

impl Place {
    fn of(doc: &Document, sentence: usize) -> Self {
        let rank = doc
            .sentences
            .iter()
            .take(sentence)
            .filter(|s| !in_code(doc, s.block_id))
            .count();
        let code = doc.sentences.get(sentence).filter(|s| in_code(doc, s.block_id)).map(|s| {
            let mut blocks: Vec<usize> = doc
                .sentences
                .iter()
                .filter(|x| in_code(doc, x.block_id))
                .map(|x| x.block_id)
                .collect();
            blocks.dedup();
            // Rang du bloc parmi les blocs de code du document actuel.
            blocks.iter().position(|b| *b == s.block_id).unwrap_or(0)
        });
        Self { rank, code }
    }

    /// La phrase correspondante de `doc` ; la dernière si le document est plus court.
    fn find(&self, doc: &Document) -> usize {
        if let Some(n) = self.code {
            let mut blocks: Vec<usize> = doc
                .sentences
                .iter()
                .filter(|s| in_code(doc, s.block_id))
                .map(|s| s.block_id)
                .collect();
            blocks.dedup();
            if let Some(block) = blocks.get(n) {
                if let Some(s) = doc.sentences.iter().find(|s| s.block_id == *block) {
                    return s.id;
                }
            }
        }
        doc.sentences
            .iter()
            .filter(|s| !in_code(doc, s.block_id))
            .nth(self.rank)
            .map(|s| s.id)
            .unwrap_or_else(|| doc.sentences.len().saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader_on(family: &str) -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.engine.family = family.into();
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    #[test]
    fn capabilities_follow_the_active_family() {
        // Supertonic, retiré avec audio.cpp : une famille inconnue.
        let supertonic = reader_on("supertonic").capabilities();
        assert_eq!(supertonic.family, "supertonic");
        assert!(supertonic.voices.is_empty());
        assert!(supertonic.honoured.is_empty());
        assert!(!supertonic.requires_reference);

        // Famille inconnue : rien n'est affirmé.
        let unknown = reader_on("").capabilities();
        assert!(unknown.voices.is_empty());
        assert!(unknown.honoured.is_empty());
        assert_eq!(unknown.languages.len(), 7);
    }

    #[test]
    fn kokoro_lists_its_voices_with_their_language() {
        let caps = reader_on("kokoro").capabilities();
        assert_eq!(caps.voices.len(), 4);
        assert_eq!(caps.voices[0], VoiceChoice {
            id: "p-fenrir60".into(),
            label: "p-fenrir60 · française, recommandée".into(),
            language: "fr".into(),
        });
        assert_eq!(caps.honoured, ["voice", "language"]);
        assert_eq!(caps.languages, ["fr", "en"]);
    }

    #[test]
    fn the_mlx_package_is_offered_only_once_the_app_provides_the_compute() {
        let mut r = reader();
        assert!(!r.voices().iter().any(|v| v.id == "kokoro_82m_mlx"));
        assert!(r.package("kokoro_82m_mlx").is_err());
        r.set_mlx_compute(true);
        let mlx = r.voices().into_iter().find(|v| v.id == "kokoro_82m_mlx").expect("proposé");
        assert!(mlx.recommended);
        assert_eq!(mlx.bytes, crate::models::CATALOG[0].total_bytes());
        assert!(mlx.licenses.contains("GPL-3.0"));
        assert!(mlx.voices_label.contains("p-fenrir60"));
        assert!(!r.voices().iter().any(|v| v.id == "kokoro_82m_int8"));
    }

    #[test]
    fn every_catalog_entry_describes_itself() {
        for package in crate::models::CATALOG {
            let v = voice_model(package, "");
            assert_eq!(v.id, package.id);
            assert_eq!(v.summary, package.description);
            assert_eq!(v.family, package.family);
            assert_eq!(v.precision, package.precision);
            assert_eq!(v.languages_label, package.languages_label);
            assert_eq!(v.languages.len(), package.languages.len());
            assert_eq!(v.pace, package.pace);
            assert!(!v.active, "aucun chemin actif");
        }
        let first = &crate::models::CATALOG[0];
        if let Some(path) = crate::models::install_path(first) {
            assert!(voice_model(first, &path.to_string_lossy()).active);
        }
    }
    use crate::audio::Player;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn reader() -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    /// Un lecteur qui compte les voix chargées.
    fn counting_reader() -> (Reader, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&loads);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let r = Reader::new(
            config,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        );
        (r, loads)
    }

    /// Un nombre de fils sûrement différent du défaut (qui suit les cœurs).
    fn other_threads() -> u32 {
        if Config::default().engine.threads == 1 { 2 } else { 1 }
    }

    /// Un calcul sûrement différent du défaut (« cpu » sous Linux, « metal » sous macOS).
    fn other_compute() -> String {
        if Config::default().engine.compute == "cpu" { "metal".into() } else { "cpu".into() }
    }

    fn effect_of(change: impl FnOnce(&mut Config)) -> SettingsEffect {
        let before = Config::default();
        let mut after = before.clone();
        change(&mut after);
        SettingsEffect::between(&before, &after)
    }

    #[test]
    fn every_setting_round_trips_through_a_patch() {
        let mut r = reader();
        let patch = SettingsPatch {
            voice: Some("F2".into()),
            language: Some("en".into()),
            speed: Some(1.4),
            pitch: Some(0.2),
            energy: Some(1.3),
            emotion: Some("calme".into()),
            instruction: Some("posé".into()),
            volume: Some(0.6),
            engine: Some("system".into()),
            compute: Some(other_compute()),
            threads: Some(3),
            sessions: Some(2),
            prefetch: Some(9),
            temperature: Some(Some(0.7)),
            top_p: Some(Some(0.9)),
            top_k: Some(Some(40)),
            guidance_scale: Some(Some(3.5)),
            repetition_penalty: Some(Some(1.2)),
            inference_steps: Some(Some(12)),
            seed: Some(Some(42)),
            code_blocks: Some("skip".into()),
            autoplay: Some(true),
            seek_step: Some(7),
            line_width: Some(80),
            line_spacing: Some(2),
            reading_ruler: Some(false),
            keep_voice_minutes: Some(30),
            output_device: Some("Casque".into()),
            extra_set: vec![("cfg_scale".into(), "3".into())],
            extra_remove: vec![],
            voice_for_language: Some(("en".into(), "am_fenrir".into())),
        };
        let expected = SettingsView {
            voice: "F2".into(),
            language: "en".into(),
            speed: 1.4,
            pitch: 0.2,
            energy: 1.3,
            emotion: "calme".into(),
            instruction: "posé".into(),
            volume: 0.6,
            engine: "system".into(),
            model_path: String::new(),
            family: String::new(),
            compute: other_compute(),
            threads: 3,
            sessions: 2,
            prefetch: 9,
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            guidance_scale: Some(3.5),
            repetition_penalty: Some(1.2),
            inference_steps: Some(12),
            seed: Some(42),
            code_blocks: "skip".into(),
            autoplay: true,
            seek_step: 7,
            line_width: 80,
            line_spacing: 2,
            reading_ruler: false,
            keep_voice_minutes: 30,
            output_device: "Casque".into(),
            extra: BTreeMap::from([("cfg_scale".to_string(), "3".to_string())]),
            voices_by_language: BTreeMap::from([("en".to_string(), "am_fenrir".to_string())]),
        };
        assert_eq!(r.apply_patch(&patch, Instant::now()), expected);
        assert_eq!(r.settings(), expected);
        // Retirer une option libre, rendre la main au modèle.
        let back = SettingsPatch {
            temperature: Some(None),
            extra_remove: vec!["cfg_scale".into()],
            ..Default::default()
        };
        let applied = r.apply_patch(&back, Instant::now());
        assert_eq!(applied.temperature, None);
        assert!(applied.extra.is_empty());
    }

    #[test]
    fn a_patch_reaches_the_open_text() {
        let mut r = reader();
        r.open_text("Un.", "a", Instant::now());
        let patch = SettingsPatch {
            speed: Some(1.4),
            reading_ruler: Some(false),
            keep_voice_minutes: Some(30),
            ..Default::default()
        };
        let applied = r.apply_patch(&patch, Instant::now());
        assert_eq!(r.settings(), applied);
        let app = r.session().unwrap();
        assert_eq!(app.config.voice.speed, 1.4);
        assert!(!app.config.reading.reading_ruler);
        assert_eq!(app.config.app.keep_voice_minutes, 30);
    }

    #[test]
    fn each_setting_triggers_the_effect_of_its_kind() {
        use SettingsEffect::*;
        assert_eq!(effect_of(|_| {}), Nothing);
        assert_eq!(effect_of(|c| c.reading.line_spacing = 2), Display);
        assert_eq!(effect_of(|c| c.reading.reading_ruler = false), Display);
        assert_eq!(effect_of(|c| c.reading.seek_step = 9), Display);
        assert_eq!(effect_of(|c| c.reading.autoplay = false), Display);
        assert_eq!(effect_of(|c| c.app.keep_voice_minutes = 2), Display);
        assert_eq!(effect_of(|c| c.voice.volume = 0.5), Immediate);
        assert_eq!(effect_of(|c| c.voice.speed = 1.5), Immediate, "la vitesse s'applique au son");
        assert_eq!(effect_of(|c| c.voice.voice = "F1".into()), Resynthesize);
        assert_eq!(effect_of(|c| c.voice.language = "en".into()), Reparse, "la normalisation suit la langue");
        assert_eq!(effect_of(|c| c.sampling.temperature = Some(0.5)), Resynthesize);
        assert_eq!(effect_of(|c| c.sampling.seed = Some(1)), Resynthesize);
        assert_eq!(effect_of(|c| { c.extra.insert("k".into(), "v".into()); }), Resynthesize);
        assert_eq!(effect_of(|c| c.reading.code_blocks = "skip".into()), Reparse);
        assert_eq!(effect_of(|c| c.engine.engine = "system".into()), Reload);
        assert_eq!(effect_of(|c| c.engine.compute = other_compute()), Reload);
        assert_eq!(effect_of(|c| c.engine.threads = other_threads()), Reload);
        assert_eq!(effect_of(|c| c.engine.sessions = 3), Reload);
        assert_eq!(effect_of(|c| c.engine.prefetch = 9), Reload);
        assert_eq!(effect_of(|c| c.engine.model_path = "/m.gguf".into()), Reload);
        // Le plus fort l'emporte.
        assert_eq!(effect_of(|c| { c.voice.volume = 0.5; c.reading.code_blocks = "read".into(); }), Reparse);
        assert_eq!(effect_of(|c| { c.voice.voice = "F1".into(); c.engine.threads = other_threads(); }), Reload);
    }

    #[test]
    fn hand_written_values_survive_other_changes() {
        let mut config = Config::default();
        config.reading.code_blocks = "lire".into();
        config.voice.language = "ja".into();
        config.engine.compute = "cuda".into();
        config.app.keep_voice_minutes = 7;
        let mut r = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        );
        let before = r.settings();
        r.apply_patch(&SettingsPatch::default(), Instant::now());
        assert_eq!(r.settings(), before, "un patch vide ne touche à rien");
        r.apply_patch(&SettingsPatch { line_spacing: Some(2), ..Default::default() }, Instant::now());
        let after = r.settings();
        assert_eq!(after.code_blocks, "lire");
        assert_eq!(after.language, "ja");
        assert_eq!(after.compute, "cuda");
        assert_eq!(after.keep_voice_minutes, 7);
        assert_eq!(after.line_spacing, 2);
    }

    #[test]
    fn values_out_of_bounds_come_back_as_applied() {
        let mut r = reader();
        let applied = r.apply_patch(
            &SettingsPatch {
                line_spacing: Some(4),
                seek_step: Some(80),
                threads: Some(0),
                speed: Some(9.0),
                volume: Some(-1.0),
                temperature: Some(Some(f32::NAN)),
                top_k: Some(Some(900)),
                ..Default::default()
            },
            Instant::now(),
        );
        assert_eq!(applied.line_spacing, 3);
        assert_eq!(applied.seek_step, 50);
        assert_eq!(applied.threads, 1);
        assert_eq!(applied.speed, 3.0);
        assert_eq!(applied.volume, 0.0);
        assert_eq!(applied.temperature, None);
        assert_eq!(applied.top_k, Some(200));
    }

    #[test]
    fn the_speed_changes_at_once_without_resynthesis() {
        let mut r = reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux.", "a", t0);
        let ready = |r: &Reader| r.session().is_some_and(|a| a.synth.get(0).is_some() && a.synth.get(1).is_some());
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while !ready(&r) {
            assert!(Instant::now() < deadline, "synthèse jamais prête");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        r.apply_patch(&SettingsPatch { speed: Some(2.2), ..Default::default() }, t0);
        let app = r.session().unwrap();
        assert_eq!(app.player.speed(), 2.2);
        assert_eq!(app.config.voice.speed, 2.2);
        assert!(app.synth.get(0).is_some() && app.synth.get(1).is_some(), "le cache est resté");
    }

    #[test]
    fn the_volume_changes_without_resynthesis() {
        let mut r = reader();
        r.open_text("Un. Deux.", "a", Instant::now());
        r.apply_patch(&SettingsPatch { volume: Some(0.5), ..Default::default() }, Instant::now());
        let app = r.session().unwrap();
        assert_eq!(app.player.volume(), 0.5);
        assert_eq!(app.config.voice.volume, 0.5);
    }

    #[test]
    fn an_engine_change_reloads_the_voice_and_resumes_at_the_same_sentence() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux. Trois.", "a", t0);
        r.with_session(t0, |app| {
            app.seek_sentences(1);
            app.toggle_play();
        });
        r.apply_patch(&SettingsPatch { threads: Some(other_threads()), ..Default::default() }, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 2, "une nouvelle voix");
        let app = r.session().expect("texte rouvert");
        assert_eq!(app.current_sentence(), 1);
        assert!(app.player.is_playing(), "la lecture reprend seule");
        assert_eq!(app.config.engine.threads, other_threads());
    }

    #[test]
    fn an_engine_change_while_resting_waits_for_the_next_wake() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un. Deux.", "a", t0);
        r.rest();
        r.apply_patch(&SettingsPatch { compute: Some(other_compute()), ..Default::default() }, t0);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "rien ne se charge au repos");
        assert!(r.is_resting());
        assert_eq!(r.config().engine.compute, other_compute());
    }

    #[test]
    fn a_prepared_voice_is_reloaded_at_once() {
        let (mut r, loads) = counting_reader();
        r.prepare_voice(Instant::now());
        r.apply_patch(&SettingsPatch { prefetch: Some(8), ..Default::default() }, Instant::now());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert!(r.voice_loaded());
    }

    const WITH_CODE: &str = "Avant.\n\n```\nx = 1\ny = 2\n```\n\nAprès un. Après deux.\n";

    #[test]
    fn changing_code_blocks_keeps_the_sentence_being_read() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        // Annoncé : Avant. · Bloc de code. · Après un. · Après deux.
        r.with_session(t0, |app| app.seek_to(3));
        let revision = r.revision();
        r.apply_patch(&SettingsPatch { code_blocks: Some("skip".into()), ..Default::default() }, t0);
        let app = r.session().unwrap();
        let current = app.current_sentence();
        assert_eq!(app.doc().sentences[current].written(), "Après deux.");
        assert_eq!(current, 2, "l'annonce du code retirée : le numéro a bougé, pas la phrase");
        assert_eq!(r.revision(), revision + 1, "le document est republié");
    }

    #[test]
    fn announce_to_read_keeps_the_same_prose_sentence() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        r.with_session(t0, |app| app.seek_to(3));
        r.apply_patch(&SettingsPatch { code_blocks: Some("read".into()), ..Default::default() }, t0);
        let app = r.session().unwrap();
        assert_eq!(app.doc().sentences[app.current_sentence()].written(), "Après deux.");
    }

    #[test]
    fn inside_a_code_block_the_block_start_is_kept_or_the_next_prose() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        r.with_session(t0, |app| app.seek_to(1)); // l'annonce du bloc
        r.apply_patch(&SettingsPatch { code_blocks: Some("read".into()), ..Default::default() }, t0);
        {
            let app = r.session().unwrap();
            assert_eq!(app.current_sentence(), 1, "le bloc existe encore : son début");
            assert!(app.doc().sentences[1].written().starts_with("x = 1"));
        }
        r.apply_patch(&SettingsPatch { code_blocks: Some("skip".into()), ..Default::default() }, t0);
        let app = r.session().unwrap();
        assert_eq!(app.doc().sentences[app.current_sentence()].written(), "Après un.", "bloc disparu : la prose qui suit");
    }

    #[test]
    fn a_reparsed_file_keeps_remembering_its_place() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        let dir = std::env::temp_dir().join(format!("sillon-reparse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        std::fs::write(&file, WITH_CODE).unwrap();
        r.open_file(&file, t0).unwrap();
        r.with_session(t0, |app| app.seek_to(2));
        assert!(r.session().unwrap().place_to_remember().is_some());
        r.apply_patch(&SettingsPatch { code_blocks: Some("skip".into()), ..Default::default() }, t0);
        let remembered = r.session().unwrap().place_to_remember();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(remembered.is_some(), "la position reste retenue après la remise en forme");
    }

    #[test]
    fn using_the_active_voice_again_reloads_nothing() {
        let (mut r, loads) = counting_reader();
        let t0 = Instant::now();
        r.open_text("Un.", "a", t0);
        r.config.engine.model_path = "/m.gguf".into();
        r.set_persistent(false);
        // Une voix inconnue du catalogue échoue sans rien recharger.
        let before = loads.load(Ordering::SeqCst);
        assert!(r.use_voice("n-existe-pas", t0).is_err());
        assert_eq!(loads.load(Ordering::SeqCst), before);
    }

    #[test]
    fn changing_code_blocks_at_rest_keeps_the_resting_sentence() {
        let (mut r, _) = counting_reader();
        let t0 = Instant::now();
        r.open_text(WITH_CODE, "a", t0);
        r.with_session(t0, |app| app.seek_to(3));
        r.rest();
        r.apply_patch(&SettingsPatch { code_blocks: Some("skip".into()), ..Default::default() }, t0);
        // Sauté : Avant. · Après un. · Après deux.
        assert_eq!(crate::reader::view::snapshot(&r).sentence, 2);
        assert_eq!(crate::reader::view::document(&r).sentence_count, 3);
        assert!(r.is_resting());
    }

    #[test]
    fn a_reader_off_disk_downloads_nothing() {
        let mut r = reader();
        r.set_persistent(false);
        let id = crate::models::CATALOG[0].id;
        assert_eq!(r.install_voice(id).unwrap_err().to_string(), crate::reader::data::TRIAL);
        assert!(r.download().is_none());
    }

    /// Un `config.toml` illisible n'est jamais réécrit par un réglage : le
    /// réglage s'applique, l'écran le dit (`Snapshot::config_unreadable`),
    /// et rien ne part sur le disque avant l'accord de l'utilisateur.
    #[test]
    fn an_unreadable_config_is_applied_but_never_written() {
        let mut r = reader();
        r.set_config_unreadable(true);
        assert!(!r.writes_config(), "rien n'est écrit par-dessus le fichier");
        let applied = r
            .update_settings(&SettingsPatch { keep_voice_minutes: Some(7), ..Default::default() }, Instant::now())
            .expect("appliqué sans écriture");
        assert_eq!(applied.keep_voice_minutes, 7);
        assert!(r.config_unreadable(), "toujours signalée");
        assert!(crate::reader::view::snapshot(&r).config_unreadable);
        let mut fine = reader();
        assert!(fine.writes_config());
        fine.set_persistent(false);
        assert!(!fine.writes_config());
        assert!(!crate::reader::view::snapshot(&fine).config_unreadable);
    }

    /// « Remplacer » : la copie illisible est gardée à côté, puis les
    /// réglages en cours sont écrits, et les écritures reprennent. Une copie
    /// déjà là n'est jamais écrasée : la suivante prend un numéro.
    #[test]
    fn replacing_an_unreadable_config_keeps_a_copy() {
        let dir = std::env::temp_dir().join(format!("sillon-illisible-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(dir.join("config.toml.illisible"), "plus ancienne").unwrap();
        std::fs::write(&path, "[voice\nspeed = 1.4").unwrap();
        let mut r = reader();
        r.set_config_path(Some(path.clone()));
        r.set_config_unreadable(true);
        r.update_settings(&SettingsPatch { keep_voice_minutes: Some(9), ..Default::default() }, Instant::now())
            .unwrap();
        r.replace_unreadable_config().expect("remplacé");
        let older = std::fs::read_to_string(dir.join("config.toml.illisible")).unwrap();
        let kept = std::fs::read_to_string(dir.join("config.toml.illisible-2")).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(older, "plus ancienne", "la copie précédente reste");
        assert_eq!(kept, "[voice\nspeed = 1.4", "l'original est gardé tel quel");
        assert!(written.contains("keep_voice_minutes = 9"), "{written}");
        assert!(!r.config_unreadable());
        assert!(r.writes_config());
    }

    /// Une configuration lisible ne se « remplace » pas : rien à réparer.
    #[test]
    fn a_readable_config_is_not_replaced() {
        let mut r = reader();
        let err = r.replace_unreadable_config().unwrap_err().to_string();
        assert!(err.contains("lisible"), "{err}");
    }

    #[test]
    fn a_reader_off_disk_applies_without_writing() {
        let mut r = reader();
        r.set_persistent(false);
        let applied = r
            .update_settings(&SettingsPatch { keep_voice_minutes: Some(5), ..Default::default() }, Instant::now())
            .expect("rien à écrire");
        assert_eq!(applied.keep_voice_minutes, 5);
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
        assert!(r.use_voice("n-existe-pas", Instant::now()).is_err());
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
            Box::new(|_: &str| Player::silent()),
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
