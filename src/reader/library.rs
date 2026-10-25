//! La bibliothèque des voix : toutes les voix des modèles de lecture
//! installés, et l'écoute de n'importe laquelle sur un texte choisi.
//!
//! Écouter une voix d'un autre modèle que l'actif charge ce modèle pour
//! l'aperçu seulement (Kokoro : quelques secondes la première fois) ; il est
//! gardé tant que la bibliothèque est ouverte, puis relâché
//! (`release_voice_preview`). L'aperçu a sa propre sortie : sa vitesse ne
//! touche jamais celle de la lecture.

use std::sync::Arc;
use std::time::Instant;

use crate::audio::Player;
use crate::config::Config;
use crate::i18n::Tr;
use crate::models::{self, ModelPackage};
use crate::tts::preview::{Previewer, AUDIO_MISSING};
use crate::tts::system_voices::SystemVoice;
use crate::tts::{EnginePreference, Readiness, TtsEngine};

use super::Reader;

/// L'identifiant de la voix de macOS, comme dans `use_voice`.
pub const SYSTEM: &str = "system";
/// La voix de macOS quand aucune n'est choisie (`voice` vide).
pub const MAC_DEFAULT_VOICE: Tr = Tr::new("macOS default voice", "Voix par défaut de macOS");
/// Refus d'un texte vide.
pub const NOTHING_TO_READ: Tr = Tr::new("Nothing to read", "Rien à lire");
/// Au-delà, le texte de l'aperçu est coupé : un aperçu reste court.
pub const MAX_PREVIEW_CHARS: usize = 500;
/// Les bornes de la vitesse de l'aperçu, celles du lecteur.
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 3.0;

/// Une voix de la bibliothèque.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryVoice {
    /// Le paquet (`kokoro_82m_mlx`…) ou `system`.
    pub model_id: String,
    /// Le nom court du modèle : « Kokoro », « macOS ».
    pub model_label: String,
    pub voice_id: String,
    /// Le nom affiché.
    pub label: String,
    /// La langue de l'accent : `fr`, `en`.
    pub language: String,
    /// L'accent en toutes lettres : « française », « américaine »…
    pub accent: String,
    /// « femme », « homme », vide si le catalogue ne le dit pas (voix de
    /// macOS : l'app le lit dans AVFoundation).
    pub gender: String,
    /// La locale d'une voix de macOS (`fr_FR`), vide pour Kokoro.
    pub locale: String,
    pub recommended: bool,
    /// Le modèle de la voix est celui qui lit.
    pub model_active: bool,
    /// La voix qui lit en ce moment.
    pub active: bool,
}

/// L'aperçu de la bibliothèque : son moteur (celui du lecteur, ou un moteur
/// chargé pour l'aperçu), sa fabrique et sa sortie.
pub(super) struct LibraryPreview {
    model: String,
    engine: Arc<dyn TtsEngine>,
    /// Le moteur a été chargé pour l'aperçu : il part avec lui.
    own: bool,
    previewer: Previewer,
    player: Player,
    error: Option<String>,
}

/// L'accent et le genre d'une voix de Kokoro, d'après son préfixe
/// (`af_` : américaine, femme ; `bm_` : britannique, homme…). `p-fenrir60`
/// a le timbre de `ff_siwis` : française, femme. L'accent suit la langue de
/// l'interface ; le genre reste « femme » ou « homme », la clé que l'app
/// compare à ses filtres.
pub fn kokoro_traits(id: &str) -> (&'static str, &'static str) {
    if id.starts_with("p-") {
        return (crate::tr!("French", "française"), "femme");
    }
    let mut chars = id.chars();
    let accent = match chars.next() {
        Some('a') => crate::tr!("American", "américaine"),
        Some('b') => crate::tr!("British", "britannique"),
        Some('f') => crate::tr!("French", "française"),
        _ => "",
    };
    let gender = match chars.next() {
        Some('f') => "femme",
        Some('m') => "homme",
        _ => "",
    };
    (accent, gender)
}

/// L'accent d'une locale de macOS (`fr_CA` : « canadienne »). Une région
/// inconnue : la locale telle quelle.
pub fn accent_of_locale(locale: &str) -> String {
    // `fr_FR@rg=bezzzz` : la région de mise en forme ne dit rien de l'accent.
    let locale = locale.split('@').next().unwrap_or(locale);
    let normalised = locale.replace('-', "_");
    let mut parts = normalised.split('_');
    let language = parts.next().unwrap_or("").to_ascii_lowercase();
    let region = parts.next().unwrap_or("").to_ascii_uppercase();
    let accent = match (language.as_str(), region.as_str()) {
        ("fr", "FR") | ("fr", "") => crate::tr!("French", "française"),
        ("fr", "CA") => crate::tr!("Canadian", "canadienne"),
        ("fr", "BE") => crate::tr!("Belgian", "belge"),
        ("fr", "CH") => crate::tr!("Swiss", "suisse"),
        ("en", "US") | ("en", "") => crate::tr!("American", "américaine"),
        ("en", "GB") => crate::tr!("British", "britannique"),
        ("en", "AU") => crate::tr!("Australian", "australienne"),
        ("en", "IE") => crate::tr!("Irish", "irlandaise"),
        ("en", "IN") => crate::tr!("Indian", "indienne"),
        ("en", "ZA") => crate::tr!("South African", "sud-africaine"),
        ("en", "NZ") => crate::tr!("New Zealand", "néo-zélandaise"),
        ("en", "SC") => crate::tr!("Scottish", "écossaise"),
        _ => return locale.to_string(),
    };
    accent.to_string()
}

/// Le nom court d'un modèle, pour le filtre.
/// L'accent et le genre d'une voix nommée par le catalogue.
pub fn voice_traits(family: &str, id: &str) -> (&'static str, &'static str) {
    match family {
        "kokoro" => kokoro_traits(id),
        "qwen3_tts_06b_base_8bit" => (crate::tr!("French", "française"), "femme"),
        "magpie_tts_357m_8bit" => match id {
            "sofia" | "aria" => (crate::tr!("American", "américaine"), "femme"),
            "jason" | "leo" | "john_van_stan" => (crate::tr!("American", "américaine"), "homme"),
            _ => ("", ""),
        },
        _ => ("", ""),
    }
}

pub(crate) fn model_label(family: &str) -> String {
    match family {
        "kokoro" => "Kokoro".to_string(),
        "qwen3_tts_06b_base_8bit" => "Qwen3 0.6B".to_string(),
        "magpie_tts_357m_8bit" => "Magpie".to_string(),
        other => other.to_string(),
    }
}

/// Le modèle qui lit : `system`, ou le paquet dont le chemin est configuré
/// (à défaut, le premier paquet de sa famille ; sans paquet, la voix de
/// macOS, qui lit alors à sa place).
pub fn active_model(config: &Config, packages: &[&'static ModelPackage]) -> String {
    if config.engine_preference() == EnginePreference::System {
        return SYSTEM.to_string();
    }
    let path = config.engine.model_path.as_str();
    packages
        .iter()
        .find(|p| models::install_path(p).is_some_and(|installed| models::is_active_path(path, &installed)))
        .or_else(|| packages.iter().find(|p| !p.family.is_empty() && p.family == config.engine.family))
        .map_or_else(|| SYSTEM.to_string(), |p| p.id.to_string())
}

/// La voix en place : Kokoro, celle du français (une voix pour tout) ;
/// macOS, `voice.voice` (vide : sa voix par défaut).
pub fn active_voice(config: &Config) -> String {
    config.tts_params_in("fr").voice
}

/// Les voix de la bibliothèque : celles des paquets de lecture installés
/// (`installed`), puis celles de macOS (toujours), sa voix par défaut en
/// tête (identifiant vide, accent de `system_locale`, la langue du Mac).
pub fn library_entries(
    config: &Config,
    installed: &[&'static ModelPackage],
    active_model_id: &str,
    system: &[SystemVoice],
    system_locale: &str,
) -> Vec<LibraryVoice> {
    let current = active_voice(config);
    let mut out = Vec::new();
    for package in installed.iter().filter(|p| p.kind == models::Kind::Speech) {
        let model_active = package.id == active_model_id;
        for voice in models::voices_for_family(package.family) {
            let (accent, gender) = voice_traits(package.family, voice.id);
            out.push(LibraryVoice {
                model_id: package.id.to_string(),
                model_label: model_label(package.family),
                voice_id: voice.id.to_string(),
                label: voice.id.to_string(),
                language: voice.language.to_string(),
                accent: accent.to_string(),
                gender: gender.to_string(),
                locale: String::new(),
                recommended: voice.label.fr.contains("recommandée"),
                model_active,
                active: model_active && voice.id == current,
            });
        }
    }
    let model_active = active_model_id == SYSTEM;
    let locale_language = crate::text::language::Lang::parse(system_locale).map_or("fr", |l| l.code());
    out.push(LibraryVoice {
        model_id: SYSTEM.to_string(),
        model_label: "macOS".to_string(),
        voice_id: String::new(),
        label: MAC_DEFAULT_VOICE.get().to_string(),
        language: locale_language.to_string(),
        accent: if system_locale.trim().is_empty() { String::new() } else { accent_of_locale(system_locale) },
        gender: String::new(),
        locale: system_locale.to_string(),
        recommended: false,
        model_active,
        active: model_active && current.is_empty(),
    });
    for voice in system {
        out.push(LibraryVoice {
            model_id: SYSTEM.to_string(),
            model_label: "macOS".to_string(),
            voice_id: voice.name.clone(),
            label: voice.name.clone(),
            language: voice.language().to_string(),
            accent: accent_of_locale(&voice.locale),
            gender: String::new(),
            locale: voice.locale.clone(),
            recommended: false,
            model_active,
            active: model_active && voice.name == current,
        });
    }
    out
}

/// Le texte de l'aperçu, sans blancs autour, coupé à `MAX_PREVIEW_CHARS`
/// (à un blanc si possible). `None` : rien à lire.
pub fn preview_text(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_PREVIEW_CHARS {
        return Some(text.to_string());
    }
    let cut: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    let at_word = cut
        .rfind(char::is_whitespace)
        .filter(|&i| cut[..i].chars().count() > MAX_PREVIEW_CHARS / 2)
        .map_or(cut.as_str(), |i| &cut[..i]);
    Some(at_word.trim_end().to_string())
}

/// La langue où lire le texte : celle qu'il montre, sinon celle de
/// l'accent de la voix.
pub fn preview_language(text: &str, voice_language: &str) -> String {
    crate::text::language::detect(text).map_or_else(
        || crate::text::language::Lang::parse(voice_language).map_or("fr", |l| l.code()).to_string(),
        |d| d.language.code().to_string(),
    )
}

/// La langue de l'accent d'une voix de la bibliothèque.
fn voice_language(model: &str, voice: &str) -> String {
    if model == SYSTEM {
        return crate::tts::system_voices::system_voices()
            .into_iter()
            .find(|v| v.name == voice)
            .map(|v| v.language().to_string())
            .unwrap_or_else(|| "fr".to_string());
    }
    models::CATALOG
        .iter()
        .find(|p| p.id == model)
        .and_then(|p| models::voices_for_family(p.family).iter().find(|v| v.id == voice))
        .map_or_else(|| "fr".to_string(), |v| v.language.to_string())
}

impl Reader {
    /// Les paquets de lecture que l'app sait charger.
    fn reading_packages(&self) -> Vec<&'static ModelPackage> {
        models::available_with(self.mlx).into_iter().filter(|p| p.kind == models::Kind::Speech).collect()
    }

    /// Le modèle qui lit (`system` ou un paquet).
    pub fn active_model_id(&self) -> String {
        active_model(self.current_config(), &self.reading_packages())
    }

    /// Toutes les voix des modèles de lecture installés, et celles de macOS
    /// (`system_locale` : la locale du Mac, pour sa voix par défaut).
    pub fn voice_library(&self, system_locale: &str) -> Vec<LibraryVoice> {
        let installed: Vec<_> = self.reading_packages().into_iter().filter(|p| models::is_installed(p)).collect();
        library_entries(
            self.current_config(),
            &installed,
            &self.active_model_id(),
            &crate::tts::system_voices::system_voices(),
            system_locale,
        )
    }

    /// La configuration qui ferait lire `model` : la courante, son moteur changé.
    fn config_for_model(&self, model: &str) -> Result<Config, String> {
        let mut config = self.current_config().clone();
        if model == SYSTEM {
            config.engine.engine = "system".to_string();
            return Ok(config);
        }
        let package = self
            .reading_packages()
            .into_iter()
            .find(|p| p.id == model)
            .ok_or_else(|| crate::tr!(en = "unknown reading model: {model}", fr = "modèle de lecture inconnu : {model}"))?;
        let path = models::install_path(package)
            .filter(|_| models::is_installed(package))
            .ok_or_else(|| {
                crate::tr!(en = "{} is not installed", fr = "{} n'est pas installé", package.display_name.get())
            })?;
        config.engine.model_path = path.to_string_lossy().into_owned();
        config.engine.family = package.family.to_string();
        config.engine.engine = "auto".to_string();
        Ok(config)
    }

    /// Fait entendre `text` avec la voix `voice` du modèle `model` (un paquet
    /// ou `system`), à `speed` — pour l'aperçu seulement. Un autre modèle que
    /// l'actif est chargé pour l'aperçu, et gardé jusqu'à
    /// `release_voice_preview`. Rend la raison d'un refus.
    pub fn preview_voice_with(
        &mut self,
        model: &str,
        voice: &str,
        text: &str,
        speed: f32,
        now: Instant,
    ) -> Result<(), String> {
        // Un refus coupe aussi l'aperçu précédent : rien ne sonne sous lui.
        self.stop_preview();
        let text = preview_text(text).ok_or_else(|| NOTHING_TO_READ.get().to_string())?;
        self.last_activity = now;
        let active = model == self.active_model_id();
        let config = if active { self.current_config().clone() } else { self.config_for_model(model)? };
        let engine = if active {
            self.ensure_engine()
        } else {
            match self.library_preview.as_ref().filter(|lib| lib.own && lib.model == model) {
                Some(lib) => Arc::clone(&lib.engine),
                None => (self.voices)(&config),
            }
        };
        let reusable = self
            .library_preview
            .as_ref()
            .is_some_and(|lib| lib.model == model && Arc::ptr_eq(&lib.engine, &engine));
        if !reusable {
            self.drop_library_preview();
            let player = (self.players)(&config.app.output_device);
            self.library_preview = Some(LibraryPreview {
                model: model.to_string(),
                engine: Arc::clone(&engine),
                own: !active,
                previewer: Previewer::new(engine),
                player,
                error: None,
            });
        }
        // La lecture se tait : l'aperçu parle seul.
        if let Some(app) = self.session.as_mut() {
            app.player.pause();
        }
        let speed = if speed.is_finite() { speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };
        let lib = self.library_preview.as_mut().expect("posé juste au-dessus");
        lib.error = None;
        lib.player.set_volume(config.voice.volume);
        lib.player.set_speed(speed);
        if !lib.player.is_available() {
            return Err(AUDIO_MISSING.get().to_string());
        }
        let language = preview_language(&text, &voice_language(model, voice));
        let mut params = config.tts_params_in(&language);
        params.voice = voice.to_string();
        lib.previewer
            .request(text, params, format!("{voice} · {speed:.2}×"))
            .map_err(|err| crate::tr!(en = "preview failed: {err}", fr = "aperçu impossible : {err}"))
    }

    /// La bibliothèque se ferme : le moteur chargé pour l'aperçu est relâché.
    pub fn release_voice_preview(&mut self) {
        self.drop_library_preview();
    }

    /// Le modèle chargé pour l'aperçu de la bibliothèque, s'il y en a un.
    pub fn preview_model_loaded(&self) -> Option<&str> {
        self.library_preview.as_ref().filter(|lib| lib.own).map(|lib| lib.model.as_str())
    }

    /// L'aperçu de la bibliothèque : se synthétise, sonne, prépare sa voix,
    /// a échoué.
    pub(super) fn library_preview_state(&self) -> Option<(bool, bool, bool, Option<String>)> {
        let lib = self.library_preview.as_ref()?;
        let pending = lib.previewer.pending_for().is_some();
        let preparing = pending && matches!(lib.engine.readiness(), Readiness::Loading);
        Some((pending, lib.player.preview_playing(), preparing, lib.error.clone()))
    }

    pub(super) fn stop_library_preview(&mut self) {
        if let Some(lib) = self.library_preview.as_mut() {
            lib.previewer.cancel();
            lib.player.stop_preview();
        }
    }

    /// Joue l'aperçu de la bibliothèque dès qu'il est prêt. Un moteur passé
    /// en repli ne fait pas entendre une autre voix que celle demandée.
    pub(super) fn poll_library_preview(&mut self) {
        let Some(lib) = self.library_preview.as_mut() else { return };
        match lib.previewer.poll() {
            Some(Ok(audio)) => match lib.engine.readiness() {
                Readiness::Fallback(reason) => {
                    lib.error = Some(crate::tr!(en = "voice unavailable: {reason}", fr = "voix indisponible : {reason}"))
                }
                _ => lib.player.play_preview(&audio),
            },
            Some(Err(err)) => lib.error = Some(crate::tr!(en = "preview failed: {err}", fr = "aperçu impossible : {err}")),
            None => {}
        }
        // Le lecteur de l'aperçu ferme sa sortie audio, une fois l'aperçu
        // dit, comme celui de la lecture.
        let _ = lib.player.reconcile();
    }

    /// Relâche l'aperçu de la bibliothèque. La fabrique et le moteur partent
    /// hors de ce fil : leur arrêt attend la synthèse en cours (et MLX).
    pub(super) fn drop_library_preview(&mut self) {
        if let Some(LibraryPreview { previewer, player, engine, .. }) = self.library_preview.take() {
            crate::sync::spawn_named("dysflow-liberation", move || {
                drop(previewer);
                drop(engine);
            });
            drop(player);
        }
    }
}

/// Un modèle tenu par l'aperçu de la bibliothèque, son aperçu fini : rien
/// ne se synthétise ni ne sonne, seul le délai de garde court.
#[cfg(test)]
pub(super) fn settled_library_preview(r: &mut Reader) {
    let engine: Arc<dyn TtsEngine> = Arc::new(crate::tts::silent::SilentEngine);
    r.library_preview = Some(LibraryPreview {
        model: "qwen3_tts_06b_base_8bit".into(),
        engine: Arc::clone(&engine),
        own: true,
        previewer: Previewer::new(engine),
        player: Player::silent(),
        error: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsSession;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    fn kokoro_package() -> &'static ModelPackage {
        models::CATALOG.iter().find(|p| p.id == "kokoro_82m_mlx").unwrap()
    }

    fn kokoro_config(voice: &str) -> Config {
        let mut c = Config::default();
        c.engine.family = "kokoro".into();
        c.engine.engine = "auto".into();
        c.engine.model_path = "/nulle/part/kokoro-v1_0.safetensors".into();
        c.voice.per_language.insert("fr".into(), voice.into());
        c.voice.per_language.insert("en".into(), voice.into());
        c
    }

    fn mac_voices() -> Vec<SystemVoice> {
        vec![
            SystemVoice { name: "Thomas".into(), locale: "fr_FR".into() },
            SystemVoice { name: "Amélie".into(), locale: "fr_CA".into() },
            SystemVoice { name: "Daniel".into(), locale: "en_GB".into() },
        ]
    }

    #[test]
    fn kokoro_voices_say_their_accent_and_gender() {
        assert_eq!(kokoro_traits("af_bella"), ("américaine", "femme"));
        assert_eq!(kokoro_traits("am_fenrir"), ("américaine", "homme"));
        assert_eq!(kokoro_traits("bf_emma"), ("britannique", "femme"));
        assert_eq!(kokoro_traits("bm_lewis"), ("britannique", "homme"));
        assert_eq!(kokoro_traits("ff_siwis"), ("française", "femme"));
        assert_eq!(kokoro_traits("p-fenrir60"), ("française", "femme"), "le timbre de ff_siwis");
    }

    #[test]
    fn every_kokoro_voice_has_an_accent_and_a_gender() {
        for v in models::voices_for_family("kokoro") {
            let (accent, gender) = kokoro_traits(v.id);
            assert!(!accent.is_empty() && !gender.is_empty(), "{}", v.id);
            // Le catalogue et le préfixe disent le même accent.
            assert!(v.label.fr.contains(accent), "{} : {accent}", v.label.fr);
        }
    }

    #[test]
    fn a_mac_locale_becomes_an_accent() {
        assert_eq!(accent_of_locale("fr_FR"), "française");
        assert_eq!(accent_of_locale("fr_CA"), "canadienne");
        assert_eq!(accent_of_locale("en_US"), "américaine");
        assert_eq!(accent_of_locale("en-GB"), "britannique");
        assert_eq!(accent_of_locale("en_AU"), "australienne");
        assert_eq!(accent_of_locale("fr_FR@rg=bezzzz"), "française");
        assert_eq!(accent_of_locale("xx_YY"), "xx_YY", "inconnue : la locale telle quelle");
    }

    #[test]
    fn the_library_lists_every_kokoro_voice_then_the_mac_ones() {
        let config = kokoro_config("am_fenrir");
        let entries = library_entries(&config, &[kokoro_package()], "kokoro_82m_mlx", &mac_voices(), "fr_FR");
        assert_eq!(entries.len(), 30 + 1 + 3);
        let fenrir = entries.iter().find(|e| e.voice_id == "am_fenrir").unwrap();
        assert!(fenrir.active && fenrir.model_active);
        assert_eq!(fenrir.model_label, "Kokoro");
        assert_eq!((fenrir.accent.as_str(), fenrir.gender.as_str()), ("américaine", "homme"));
        assert_eq!(entries.iter().filter(|e| e.active).count(), 1, "une seule voix lit");
        let bella = entries.iter().find(|e| e.voice_id == "af_bella").unwrap();
        assert!(bella.recommended && !bella.active);
        let amelie = entries.iter().find(|e| e.voice_id == "Amélie").unwrap();
        assert_eq!(amelie.model_id, SYSTEM);
        assert_eq!(amelie.model_label, "macOS");
        assert_eq!((amelie.language.as_str(), amelie.accent.as_str()), ("fr", "canadienne"));
        assert_eq!(amelie.locale, "fr_CA");
        assert!(!amelie.model_active && !amelie.active);
    }

    #[test]
    fn the_default_mac_voice_has_its_own_row_checked_when_it_reads() {
        let mut config = Config::default();
        config.engine.engine = "system".into();
        config.voice.voice = String::new();
        let entries = library_entries(&config, &[], SYSTEM, &mac_voices(), "fr_CA");
        let default = entries.iter().find(|e| e.voice_id.is_empty()).expect("la ligne de la voix par défaut");
        assert_eq!(default.label, MAC_DEFAULT_VOICE.get());
        assert_eq!(default.model_id, SYSTEM);
        assert_eq!((default.language.as_str(), default.accent.as_str()), ("fr", "canadienne"));
        assert!(default.active && default.model_active);
        assert_eq!(entries.iter().filter(|e| e.active).count(), 1);
        // Une voix nommée lit : la ligne par défaut n'est pas cochée.
        config.voice.voice = "Thomas".into();
        let entries = library_entries(&config, &[], SYSTEM, &mac_voices(), "fr_FR");
        assert!(!entries.iter().find(|e| e.voice_id.is_empty()).unwrap().active);
        // Kokoro lit : pas cochée non plus.
        let entries = library_entries(&kokoro_config("af_bella"), &[kokoro_package()], "kokoro_82m_mlx",
                                      &mac_voices(), "en_US");
        let default = entries.iter().find(|e| e.voice_id.is_empty()).unwrap();
        assert!(!default.active && !default.model_active);
        assert_eq!(default.language, "en");
    }

    #[test]
    fn without_kokoro_installed_only_the_mac_voices_are_listed() {
        let mut config = Config::default();
        config.engine.engine = "system".into();
        config.voice.voice = "Daniel".into();
        let entries = library_entries(&config, &[], SYSTEM, &mac_voices(), "fr_FR");
        assert_eq!(entries.len(), 1 + 3);
        assert!(entries.iter().all(|e| e.model_active));
        let active: Vec<_> = entries.iter().filter(|e| e.active).map(|e| e.voice_id.as_str()).collect();
        assert_eq!(active, ["Daniel"]);
    }

    #[test]
    fn the_active_model_follows_the_configuration() {
        let packages = [kokoro_package()];
        assert_eq!(active_model(&kokoro_config("af_bella"), &packages), "kokoro_82m_mlx");
        let mut system = kokoro_config("af_bella");
        system.engine.engine = "system".into();
        assert_eq!(active_model(&system, &packages), SYSTEM);
        assert_eq!(active_model(&Config::default(), &packages), SYSTEM, "aucun modèle : macOS lit");
    }

    #[test]
    fn an_empty_text_is_refused_and_a_long_one_is_cut() {
        assert_eq!(preview_text("  \n "), None);
        assert_eq!(preview_text("  Bonjour. ").as_deref(), Some("Bonjour."));
        let long = "mot ".repeat(400);
        let cut = preview_text(&long).unwrap();
        assert!(cut.chars().count() <= MAX_PREVIEW_CHARS);
        assert!(cut.ends_with("mot"), "coupé entre deux mots : {cut:?}");
        let unbroken = "a".repeat(800);
        assert_eq!(preview_text(&unbroken).unwrap().chars().count(), MAX_PREVIEW_CHARS);
    }

    #[test]
    fn the_preview_reads_in_the_language_of_its_text() {
        let english = "The quick brown fox jumps over the lazy dog, and then it runs far away into the woods.";
        assert_eq!(preview_language(english, "fr"), "en");
        let french = "Bonjour, voici un aperçu de la voix sélectionnée, à cette vitesse, pour l'entendre.";
        assert_eq!(preview_language(french, "en"), "fr");
        assert_eq!(preview_language("Salut.", "en"), "en", "trop court : l'accent de la voix");
        assert_eq!(preview_language("Salut.", "en_GB"), "en");
    }

    // ---- Le lecteur ----

    /// Un lecteur dont la fabrique de voix compte les chargements et retient
    /// le moteur demandé.
    fn reader(config: Config) -> (Reader, Arc<AtomicUsize>, Arc<Mutex<Vec<EnginePreference>>>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (counter, log) = (Arc::clone(&loads), Arc::clone(&asked));
        let mut r = Reader::new(
            config,
            Box::new(move |c: &Config| {
                counter.fetch_add(1, Ordering::SeqCst);
                log.lock().unwrap().push(c.engine_preference());
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>
            }),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        );
        r.set_mlx_compute(true);
        r.set_persistent(false);
        (r, loads, asked)
    }

    #[test]
    fn an_empty_text_is_refused_without_loading_anything() {
        let (mut r, loads, _) = reader(kokoro_config("af_bella"));
        let err = r.preview_voice_with(SYSTEM, "Thomas", "   ", 1.0, Instant::now()).unwrap_err();
        assert_eq!(err, NOTHING_TO_READ.get());
        assert_eq!(loads.load(Ordering::SeqCst), 0);
    }

    /// Un aperçu de la bibliothèque qui se synthétise encore (silence muet).
    fn pending_library_preview(r: &mut Reader, own: bool) {
        let engine: Arc<dyn TtsEngine> = Arc::new(SilentEngine);
        let mut previewer = Previewer::new(Arc::clone(&engine));
        previewer.request("Bonjour.".into(), r.config.tts_params(), "Thomas".into()).unwrap();
        r.library_preview =
            Some(LibraryPreview { model: SYSTEM.into(), engine, own, previewer, player: Player::silent(), error: None });
    }

    #[test]
    fn a_refused_text_stops_the_voice_that_was_previewing() {
        let (mut r, _, _) = reader(kokoro_config("af_bella"));
        pending_library_preview(&mut r, true);
        assert!(r.preview_view().pending);
        let err = r.preview_voice_with(SYSTEM, "Thomas", " ", 1.0, Instant::now()).unwrap_err();
        assert_eq!(err, NOTHING_TO_READ.get());
        assert!(!r.preview_view().pending, "l'aperçu précédent ne continue pas sous un refus");
    }

    #[test]
    fn the_view_of_the_library_shows_only_its_own_error() {
        let (mut r, _, _) = reader(kokoro_config("af_bella"));
        r.preview_error = Some("un vieil échec d'« Écouter »".into());
        pending_library_preview(&mut r, true);
        assert_eq!(r.preview_view().error, None);
        r.library_preview.as_mut().unwrap().error = Some("le sien".into());
        assert_eq!(r.preview_view().error.as_deref(), Some("le sien"));
    }

    #[test]
    fn a_model_loaded_for_a_preview_rests_with_the_voice() {
        let mut config = kokoro_config("af_bella");
        config.app.keep_voice_minutes = 1;
        let (mut r, _, _) = reader(config);
        let t0 = Instant::now();
        let _ = r.preview_voice_with(SYSTEM, "Thomas", "Bonjour.", 1.0, t0);
        assert_eq!(r.preview_model_loaded(), Some(SYSTEM));
        r.tick(t0 + Duration::from_secs(30));
        assert_eq!(r.preview_model_loaded(), Some(SYSTEM), "pas avant le délai");
        r.tick(t0 + Duration::from_secs(120));
        assert_eq!(r.preview_model_loaded(), None, "jamais gardé en mémoire au-delà du délai");
    }

    #[test]
    fn a_voice_of_the_active_model_uses_the_reader_voice() {
        let mut config = Config::default();
        config.engine.engine = "system".into();
        let (mut r, loads, _) = reader(config);
        let err = r.preview_voice_with(SYSTEM, "Thomas", "Bonjour.", 1.0, Instant::now()).unwrap_err();
        assert_eq!(err, AUDIO_MISSING.get(), "sortie muette sous test");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert!(r.voice_loaded(), "la voix du lecteur sert");
        assert_eq!(r.preview_model_loaded(), None, "aucun moteur à part");
    }

    #[test]
    fn a_voice_of_another_model_loads_it_once_for_the_preview_then_releases_it() {
        let (mut r, loads, asked) = reader(kokoro_config("af_bella"));
        assert_eq!(r.active_model_id(), "kokoro_82m_mlx");
        let _ = r.preview_voice_with(SYSTEM, "Thomas", "Bonjour.", 1.0, Instant::now());
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(asked.lock().unwrap().as_slice(), [EnginePreference::System], "la voix de macOS");
        assert!(!r.voice_loaded(), "la voix du lecteur n'est pas touchée");
        assert_eq!(r.preview_model_loaded(), Some(SYSTEM));
        let _ = r.preview_voice_with(SYSTEM, "Daniel", "Hello.", 1.0, Instant::now());
        assert_eq!(loads.load(Ordering::SeqCst), 1, "gardé d'un aperçu à l'autre");
        r.release_voice_preview();
        assert_eq!(r.preview_model_loaded(), None);
        assert_eq!(r.preview_view(), super::super::preview::PreviewView::default());
    }

    #[test]
    fn the_preview_speed_never_changes_the_reading_speed() {
        let (mut r, _, _) = reader(kokoro_config("af_bella"));
        let before = r.settings().speed;
        let _ = r.preview_voice_with(SYSTEM, "Thomas", "Bonjour.", 2.5, Instant::now());
        assert_eq!(r.library_preview.as_ref().unwrap().player.speed(), 2.5);
        assert_eq!(r.settings().speed, before);
        let _ = r.preview_voice_with(SYSTEM, "Thomas", "Bonjour.", 9.0, Instant::now());
        assert_eq!(r.library_preview.as_ref().unwrap().player.speed(), MAX_SPEED, "borné");
    }

    #[test]
    fn a_model_not_installed_is_refused() {
        let (mut r, _, _) = reader(Config::default());
        let err = r.preview_voice_with("inconnu", "x", "Bonjour.", 1.0, Instant::now()).unwrap_err();
        assert!(err.contains("inconnu"), "{err}");
    }

    /// Un moteur qui charge encore, et dont la synthèse attend le feu vert.
    struct Loading {
        gate: Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
    }
    struct GatedSession(Arc<Mutex<std::sync::mpsc::Receiver<()>>>);
    impl TtsEngine for Loading {
        fn name(&self) -> String {
            "chargement".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn TtsSession>> {
            Ok(Box::new(GatedSession(Arc::clone(&self.gate))))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn readiness(&self) -> Readiness {
            Readiness::Loading
        }
    }
    impl TtsSession for GatedSession {
        fn synthesize(&mut self, text: &str, params: &crate::tts::TtsParams) -> anyhow::Result<crate::tts::PcmBuffer> {
            let _ = self.0.lock().unwrap().recv_timeout(Duration::from_secs(5));
            SilentEngine.open_session()?.synthesize(text, params)
        }
    }

    #[test]
    fn the_view_says_the_voice_is_being_prepared_and_keeps_the_reader_awake() {
        let (release, gate) = std::sync::mpsc::channel();
        let engine: Arc<dyn TtsEngine> = Arc::new(Loading { gate: Arc::new(Mutex::new(gate)) });
        let (mut r, _, _) = reader(kokoro_config("af_bella"));
        let mut previewer = Previewer::new(Arc::clone(&engine));
        previewer.request("Bonjour.".into(), r.config.tts_params(), "Thomas".into()).unwrap();
        r.library_preview = Some(LibraryPreview {
            model: SYSTEM.into(),
            engine,
            own: true,
            previewer,
            player: Player::silent(),
            error: None,
        });
        let view = r.preview_view();
        assert!(view.pending && view.preparing, "{view:?}");
        assert!(r.previewing(), "la voix ne se repose pas sous l'aperçu");
        r.stop_preview();
        assert!(!r.preview_view().pending, "Arrêter coupe aussi la bibliothèque");
        release.send(()).unwrap();
        r.release_voice_preview();
    }

    #[test]
    fn the_new_voices_say_their_accent_and_gender() {
        assert_eq!(voice_traits("qwen3_tts_06b_base_8bit", "siwis"), ("française", "femme"));
        assert_eq!(voice_traits("magpie_tts_357m_8bit", "sofia"), ("américaine", "femme"));
        assert_eq!(voice_traits("magpie_tts_357m_8bit", "john_van_stan"), ("américaine", "homme"));
        assert_eq!(voice_traits("kokoro", "bf_emma"), ("britannique", "femme"));
    }

    #[test]
    fn an_installed_text_model_lists_its_voices_under_its_short_name() {
        let c = Config::default();
        let installed = [models::CATALOG.iter().find(|p| p.id == "magpie_tts_357m_8bit").unwrap()];
        let entries = library_entries(&c, &installed, SYSTEM, &[], "fr_FR");
        let magpie: Vec<_> = entries.iter().filter(|v| v.model_id == "magpie_tts_357m_8bit").collect();
        assert_eq!(magpie.len(), 5);
        assert!(magpie.iter().all(|v| v.model_label == "Magpie"));
        assert!(magpie.iter().any(|v| v.voice_id == "sofia" && v.recommended && v.gender == "femme"));
    }
}
