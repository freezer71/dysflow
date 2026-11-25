//! L'aide des touches (`?`) : une ligne par geste, dans l'ordre de l'aide
//! de l'app Mac, et la carte qui la montre par-dessus la lecture.
//!
//! Chaque ligne porte les touches qu'elle décrit (`presses`) : l'essai
//! vérifie qu'elles mènent bien à une action de `keymap`, et que chaque
//! action du clavier a sa ligne.

use gpui::{div, prelude::*, px, IntoElement, SharedString};

use crate::keymap::{KeyPress, Modifier};
use crate::theme::Palette;

pub struct HelpRow {
    /// Les touches telles qu'elles s'écrivent (« ← → », « ⌘O »).
    pub keys: String,
    pub label: &'static str,
    /// Les touches décrites, pour l'essai.
    pub presses: Vec<KeyPress>,
}

fn press(key: &str) -> KeyPress {
    KeyPress {
        key: key.to_string(),
        ..KeyPress::default()
    }
}

fn shifted(key: &str) -> KeyPress {
    KeyPress {
        shift: true,
        ..press(key)
    }
}

fn with_command(key: &str, modifier: Modifier) -> KeyPress {
    KeyPress {
        platform: modifier == Modifier::Command,
        control: modifier == Modifier::Control,
        ..press(key)
    }
}

/// Les lignes de l'aide, avec la touche de commande du système.
pub fn rows(modifier: Modifier) -> Vec<HelpRow> {
    let cmd = |key: &str| match modifier {
        Modifier::Command => format!("⌘{}", key.to_uppercase()),
        Modifier::Control => format!("Ctrl+{}", key.to_uppercase()),
    };
    let c = |key: &str| with_command(key, modifier);
    let row = |keys: String, label: &'static str, presses: Vec<KeyPress>| HelpRow {
        keys,
        label,
        presses,
    };
    use dysflow::tr;
    vec![
        row(
            tr!("Space", "Espace").into(),
            tr!("Play / pause", "Lecture / pause"),
            vec![press("space")],
        ),
        row(
            "← →".into(),
            tr!("Previous / next sentence", "Phrase précédente / suivante"),
            vec![press("left"), press("right")],
        ),
        row(
            "↑ ↓".into(),
            tr!("Previous / next line", "Ligne précédente / suivante"),
            vec![press("up"), press("down")],
        ),
        row(
            tr!("Home", "Début").into(),
            tr!("Back to the start", "Retour au début"),
            vec![press("home")],
        ),
        row(
            "n p".into(),
            tr!("Next / previous heading", "Titre suivant / précédent"),
            vec![press("n"), press("p")],
        ),
        row(
            format!("/ {}", cmd("f")),
            tr!("Search", "Rechercher"),
            vec![press("/"), c("f")],
        ),
        row(
            "⇧N ⇧P".into(),
            tr!("Next / previous result", "Résultat suivant / précédent"),
            vec![shifted("n"), shifted("p")],
        ),
        row("t".into(), tr!("Contents", "Sommaire"), vec![press("t")]),
        row(
            "e".into(),
            tr!("Edit the text", "Modifier le texte"),
            vec![press("e")],
        ),
        row(
            "f".into(),
            tr!("Follow the reading", "Suivre la lecture"),
            vec![press("f")],
        ),
        row(
            "+ −".into(),
            tr!("Faster / slower", "Plus vite / moins vite"),
            vec![press("+"), press("-")],
        ),
        row(
            cmd("c"),
            tr!(
                "Copy the selection, or else the sentence",
                "Copier la sélection, sinon la phrase"
            ),
            vec![c("c")],
        ),
        row(
            cmd("v"),
            tr!("Read the clipboard", "Lire le presse-papiers"),
            vec![c("v")],
        ),
        row(
            cmd("o"),
            tr!("Open a file", "Ouvrir un fichier"),
            vec![c("o")],
        ),
        row(
            cmd("w"),
            tr!("Close (reading stops)", "Fermer (la lecture s'arrête)"),
            vec![c("w")],
        ),
        row(
            tr!("Esc", "Échap").into(),
            tr!(
                "Close help, search or contents, otherwise close",
                "Fermer l'aide, la recherche ou le sommaire, sinon fermer"
            ),
            vec![press("escape")],
        ),
        row("?".into(), tr!("This help", "Cette aide"), vec![press("?")]),
        row(cmd(","), tr!("Settings", "Réglages"), vec![c(",")]),
    ]
}

/// La carte de l'aide, centrée sur la lecture. Trop haute pour la fenêtre,
/// elle défile entre son titre et son pied.
pub fn card(modifier: Modifier, p: &Palette) -> impl IntoElement {
    let keys = rows(modifier).into_iter().map(|row| {
        div()
            .flex()
            .items_center()
            .gap(px(16.0))
            .py(px(3.0))
            .child(
                div().w(px(104.0)).flex().child(
                    div()
                        .px(px(7.0))
                        .py(px(2.0))
                        .rounded(px(6.0))
                        .bg(p.keycap())
                        .text_color(p.text())
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(row.keys)),
                ),
            )
            .child(div().text_color(p.chrome()).child(row.label))
    });
    div()
        .id("aide")
        .flex()
        .flex_col()
        .gap(px(12.0))
        .max_h_full()
        .w(px(460.0))
        .p(px(20.0))
        .rounded(px(16.0))
        .bg(p.background())
        .border_1()
        .border_color(p.border())
        .shadow_lg()
        .text_size(px(13.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(p.text())
                .child(dysflow::tr!("Shortcuts", "Raccourcis")),
        )
        .child(
            div()
                .id("aide-touches")
                .flex()
                .flex_col()
                .flex_shrink(1.0)
                .min_h(px(0.0))
                .overflow_y_scroll()
                .children(keys),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.muted())
                .child(dysflow::tr!("? or Esc to close", "? ou Échap pour fermer")),
        )
}
