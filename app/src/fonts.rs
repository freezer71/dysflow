//! Les polices de lecture, embarquées dans le binaire : Literata, Atkinson
//! Hyperlegible et OpenDyslexic (SIL Open Font License 1.1, sans
//! modification ; licences dans `assets/fonts/`, avis dans `NOTICE`).
//!
//! Elles s'enregistrent au démarrage dans le système de texte de GPUI
//! (`register`) : le texte s'affiche pareil sous macOS, Windows et Linux,
//! qu'elles soient installées ou non. Atkinson Hyperlegible sert aussi à
//! l'interface (`theme::UI_FONT`), ses chiffres à chasse fixe (`tnum`) au
//! temps de la barre de lecture.

use std::borrow::Cow;

use gpui::App;

/// Un fichier de police embarqué.
pub struct EmbeddedFont {
    pub file: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! embed {
    ($($file:literal),* $(,)?) => {
        &[$(EmbeddedFont {
            file: $file,
            bytes: include_bytes!(concat!("../assets/fonts/", $file)),
        }),*]
    };
}

/// Chaque style de chaque famille : Literata est variable (graisse et taille
/// optique dans un fichier, l'italique dans l'autre).
pub const EMBEDDED: &[EmbeddedFont] = embed![
    "Literata[opsz,wght].ttf",
    "Literata-Italic[opsz,wght].ttf",
    "AtkinsonHyperlegible-Regular.ttf",
    "AtkinsonHyperlegible-Bold.ttf",
    "AtkinsonHyperlegible-Italic.ttf",
    "AtkinsonHyperlegible-BoldItalic.ttf",
    "OpenDyslexic-Regular.otf",
    "OpenDyslexic-Bold.otf",
    "OpenDyslexic-Italic.otf",
    "OpenDyslexic-BoldItalic.otf",
];

/// La famille d'une police des réglages (`config::READING_FONTS`) ; une
/// valeur inconnue lit en Literata, comme l'app Mac.
pub fn family(id: &str) -> &'static str {
    match id {
        "atkinson" => "Atkinson Hyperlegible",
        "openDyslexic" => "OpenDyslexic",
        _ => "Literata",
    }
}

/// Le nom d'une police des réglages, tel que la page le montre.
pub fn label(id: &str) -> &'static str {
    match id {
        "atkinson" => "Atkinson",
        "openDyslexic" => "OpenDyslexic",
        _ => "Literata",
    }
}

/// Enregistre les polices embarquées dans le système de texte de GPUI. Un
/// échec (fichier refusé par la plateforme) se note sur la sortie
/// d'erreur : GPUI retombe alors sur une police du système.
pub fn register(cx: &App) {
    let fonts = EMBEDDED.iter().map(|f| Cow::Borrowed(f.bytes)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!(
            "{}",
            dysflow::tr!(
                en = "dysflow: embedded fonts refused: {:#}",
                fr = "dysflow : polices embarquées refusées : {:#}",
                err
            )
        );
    }
}
