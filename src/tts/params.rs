//! Ce que la configuration demande à la synthèse : paramètres d'une phrase,
//! moteur et réglages de chargement. Ici plutôt que dans `config`, pour que
//! la configuration ne dépende ni de la synthèse ni du catalogue.

use super::{EnginePreference, EngineSettings, TtsParams};
use crate::config::Config;

impl Config {
    /// Paramètres de synthèse dans la langue par défaut.
    pub fn tts_params(&self) -> TtsParams {
        self.tts_params_in(&self.default_language())
    }

    /// Paramètres de synthèse pour un document dans `language`.
    pub fn tts_params_in(&self, language: &str) -> TtsParams {
        let family = &self.engine.family;
        let system = self.engine_preference() == EnginePreference::System;
        let voice = if !system && crate::models::voices_follow_language(family) {
            let code = crate::text::language::Lang::parse(language).map_or(language, |l| l.code());
            let configured = self.voice.per_language.get(code).map(String::as_str);
            crate::models::voice_for_language(family, code, configured).to_string()
        } else {
            // Hors Kokoro (la voix du système), la voix configurée part
            // telle quelle, sans filtre (ou la voix de macOS choisie) ; un `~` initial est développé.
            crate::paths::expand_tilde(self.voice.voice.trim())
        };
        TtsParams {
            voice,
            language: language.to_string(),
            pitch_shift: self.voice.pitch.clamp(-1.0, 1.0),
        }
    }

    pub fn engine_preference(&self) -> EnginePreference {
        EnginePreference::parse(&self.engine.engine).unwrap_or(EnginePreference::Auto)
    }

    pub fn engine_settings(&self) -> EngineSettings {
        EngineSettings {
            model_path: crate::paths::expand_tilde(&self.engine.model_path),
            family: self.engine.family.clone(),
            threads: self.engine.threads.max(1),
            sessions: self.sessions(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn the_system_voice_never_receives_a_kokoro_voice() {
        let mut c = Config::default();
        c.engine.family = "kokoro".into();
        c.engine.engine = "system".into();
        c.voice.voice = "Thomas".into();
        c.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        assert_eq!(c.tts_params_in("fr").voice, "Thomas");
    }
}
