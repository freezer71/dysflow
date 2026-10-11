//! La langue d'un document : français ou anglais, d'après ses lettres.
//!
//! Une seule langue par document, celle qui a le plus de lettres. Les blocs de
//! code n'en disent rien et sont écartés. Un texte trop court, ou dont la
//! détection hésite, prend la langue par défaut : se tromper de voix sur une
//! phrase gêne plus que garder celle qu'on attend.

use whatlang::{Detector, Lang as Detected};

/// Les deux langues de la voix Kokoro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// `fr`, `fr-FR`, `en_US`… ; toute autre langue : `None`.
    pub fn parse(code: &str) -> Option<Lang> {
        let base = code.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        match base.as_str() {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// Ce que la détection a vu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub language: Lang,
    /// Part des lettres dans l'autre langue, de 0 à 0,5.
    pub minority_share: f32,
}

/// Lettres minimum pour se prononcer.
pub const MIN_LETTERS: usize = 40;
/// Au-delà, l'onglet Voix signale un document mêlé.
pub const MIXED_THRESHOLD: f32 = 0.30;
/// Valeur de `voice.language` qui laisse chaque document décider.
pub const AUTO: &str = "auto";

fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

fn detector() -> Detector {
    Detector::with_allowlist(vec![Detected::Fra, Detected::Eng])
}

fn lang_of(detected: Detected) -> Option<Lang> {
    match detected {
        Detected::Fra => Some(Lang::Fr),
        Detected::Eng => Some(Lang::En),
        _ => None,
    }
}

/// La langue d'un texte brut, ou `None` s'il est trop court ou ambigu.
///
/// La fiabilité se juge sur le texte entier ; la part de chaque langue se
/// compte paragraphe par paragraphe, la détection globale ne disant pas
/// combien de lettres sont dans l'autre.
pub fn detect(text: &str) -> Option<Detection> {
    if letters(text) < MIN_LETTERS {
        return None;
    }
    let detector = detector();
    let whole = detector.detect(text)?;
    if !whole.is_reliable() {
        return None;
    }
    let (mut fr, mut en) = (0usize, 0usize);
    for paragraph in text.split("\n\n") {
        let n = letters(paragraph);
        if n == 0 {
            continue;
        }
        match detector.detect(paragraph).and_then(|info| lang_of(info.lang())) {
            Some(Lang::Fr) => fr += n,
            Some(Lang::En) => en += n,
            None => {}
        }
    }
    let total = fr + en;
    if total == 0 {
        return lang_of(whole.lang()).map(|language| Detection { language, minority_share: 0.0 });
    }
    let language = if fr >= en { Lang::Fr } else { Lang::En };
    Some(Detection { language, minority_share: fr.min(en) as f32 / total as f32 })
}

/// La langue d'un markdown, blocs et extraits de code écartés.
pub fn detect_markdown(markdown: &str) -> Option<Detection> {
    detect(&without_code(markdown))
}

fn without_code(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut fence: Option<&'static str> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            out.push('\n');
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            out.push('\n');
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            out.push('\n');
            continue;
        }
        let mut inside = false;
        for c in line.chars() {
            if c == '`' {
                inside = !inside;
            } else if !inside {
                out.push(c);
            }
        }
        out.push('\n');
    }
    out
}

/// La langue d'un document : le choix fait pour lui, sinon la langue imposée
/// par la configuration (tout sauf `auto`), sinon celle détectée, sinon le
/// français.
pub fn resolve(configured: &str, chosen: Option<&str>, detected: Option<Detection>) -> String {
    if let Some(choice) = chosen.map(str::trim).filter(|c| !c.is_empty()) {
        return choice.to_string();
    }
    let configured = configured.trim();
    if !configured.is_empty() && configured != AUTO {
        return configured.to_string();
    }
    detected.map_or(Lang::Fr, |d| d.language).code().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FR: &str = "Les monades permettent d'enchaîner des calculs qui portent un contexte, \
                      sans le manipuler à la main à chaque étape du programme.";
    const EN: &str = "Monads let you chain computations that carry a context, without handling \
                      it by hand at every single step of the program.";

    #[test]
    fn a_french_paragraph_is_french() {
        let d = detect(FR).expect("assez de lettres");
        assert_eq!(d.language, Lang::Fr);
        assert!(d.minority_share < 0.05, "{}", d.minority_share);
    }

    #[test]
    fn an_english_paragraph_is_english() {
        assert_eq!(detect(EN).map(|d| d.language), Some(Lang::En));
    }

    #[test]
    fn under_forty_letters_nothing_is_said() {
        assert_eq!(detect("Hello world."), None);
        assert_eq!(detect(""), None);
    }

    #[test]
    fn a_mixed_document_takes_its_majority_and_says_how_much_is_other() {
        let text = format!("{FR}\n\n{FR}\n\n{EN}");
        let d = detect(&text).unwrap();
        assert_eq!(d.language, Lang::Fr);
        assert!((0.2..0.45).contains(&d.minority_share), "{}", d.minority_share);
    }

    #[test]
    fn code_blocks_say_nothing_about_the_language() {
        let code = "```rust\nfn main() { println!(\"the quick brown fox jumps over the lazy dog\"); }\n\
                    // another english comment line that is quite long indeed\n```";
        let markdown = format!("{FR}\n\n{code}\n\nUn `inline english code span here` aussi.");
        let d = detect_markdown(&markdown).unwrap();
        assert_eq!(d.language, Lang::Fr);
        assert!(d.minority_share < 0.05, "{}", d.minority_share);
    }

    #[test]
    fn codes_are_read_loosely() {
        assert_eq!(Lang::parse("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::parse(" EN_us "), Some(Lang::En));
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::En.code(), "en");
    }

    #[test]
    fn a_choice_beats_the_configuration_which_beats_the_detection() {
        let en = Some(Detection { language: Lang::En, minority_share: 0.0 });
        assert_eq!(resolve("auto", None, en), "en");
        assert_eq!(resolve("auto", None, None), "fr", "rien de détecté : le français");
        assert_eq!(resolve("fr", None, en), "fr", "langue imposée par la configuration");
        assert_eq!(resolve("de", None, en), "de", "une langue d'un autre modèle reste telle quelle");
        assert_eq!(resolve("fr", Some("en"), None), "en", "le choix fait pour ce document");
    }
}
