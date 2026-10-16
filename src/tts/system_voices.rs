//! Les voix que `say` sait prendre : celles installées sur ce Mac.
//!
//! L'app ne peut pas en télécharger d'autres (aucune API) : l'utilisateur
//! les ajoute dans Réglages Système > Accessibilité > Contenu énoncé.

use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemVoice {
    pub name: String,
    /// `fr_FR`, `en_US`…
    pub locale: String,
}

impl SystemVoice {
    /// `fr` ou `en`.
    pub fn language(&self) -> &str {
        self.locale.split('_').next().unwrap_or("")
    }
}

/// Lit la sortie de `say -v '?'` : « Nom (peut avoir des espaces) locale # phrase ».
/// Garde le français et l'anglais, dans l'ordre de `say`.
pub fn parse_say_voices(listing: &str) -> Vec<SystemVoice> {
    listing
        .lines()
        .filter_map(|line| {
            let head = line.split('#').next()?.trim_end();
            let (name, locale) = head.rsplit_once(char::is_whitespace)?;
            Some(SystemVoice { name: name.trim().to_string(), locale: locale.to_string() })
        })
        .filter(|v| matches!(v.language(), "fr" | "en"))
        .collect()
}

/// Les voix de ce Mac, lues une fois. Vide hors macOS ou si `say` manque.
pub fn system_voices() -> &'static [SystemVoice] {
    static VOICES: OnceLock<Vec<SystemVoice>> = OnceLock::new();
    VOICES.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        Command::new("say")
            .args(["-v", "?"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| parse_say_voices(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "\
Amélie              fr_CA    # Bonjour, je m’appelle Amélie.
Eddy (French (France)) fr_FR    # Bonjour, je m’appelle Eddy.
Samantha            en_US    # Hello! My name is Samantha.
Kyoko               ja_JP    # こんにちは、私の名前はKyokoです。
";

    #[test]
    fn names_may_hold_spaces_and_parentheses() {
        let voices = parse_say_voices(LISTING);
        assert_eq!(voices[1], SystemVoice { name: "Eddy (French (France))".into(), locale: "fr_FR".into() });
    }

    #[test]
    fn only_french_and_english_voices_are_kept() {
        let names: Vec<String> = parse_say_voices(LISTING).into_iter().map(|v| v.name).collect();
        assert_eq!(names, ["Amélie", "Eddy (French (France))", "Samantha"]);
    }

    #[test]
    fn blank_lines_and_lines_without_hash_do_not_panic() {
        let voices = parse_say_voices("\n   \nseul\nThomas fr_FR\n");
        assert_eq!(voices, [SystemVoice { name: "Thomas".into(), locale: "fr_FR".into() }]);
    }
}
