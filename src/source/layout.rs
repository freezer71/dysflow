//! Reconstruction d'un texte mis en page en markdown lisible.
//!
//! Un PDF ne livre que des lignes placées sur une feuille. Les rendre telles
//! quelles ferait de chaque ligne un bloc, et la navigation deviendrait
//! inutilisable. Ce module leur redonne une structure.

use super::pdf::Page;

/// Assemble des pages en un markdown que `input::split_blocks` sait découper.
pub fn to_markdown(pages: &[Page]) -> String {
    let mut out: Vec<String> = Vec::new();
    for page in pages {
        // Une page qui s'ouvre ne colle pas à la précédente, mais ne creuse
        // pas non plus un second blanc là où il y en a déjà un.
        if out.last().is_some_and(|last| !last.is_empty()) {
            out.push(String::new());
        }
        for line in &page.lines {
            let line = line.trim_end();
            if line.is_empty() {
                if out.last().is_some_and(|last| !last.is_empty()) {
                    out.push(String::new());
                }
            } else {
                out.push(line.to_string());
            }
        }
    }
    while out.last().is_some_and(|last| last.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Rend leurs lettres aux ligatures typographiques.
///
/// Un moteur de synthèse ne reconnaît pas `ﬁ` comme `fi`, et le trait d'union
/// conditionnel se glisse dans le texte sans se voir à l'écran.
fn unligature(text: &str) -> String {
    // La très grande majorité des lignes n'a aucune ligature : ne rien
    // reconstruire quand il n'y a rien à changer.
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬀ' => out.push_str("ff"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' | 'ﬆ' => out.push_str("st"),
            '\u{ad}' => {}
            other => out.push(other),
        }
    }
    out
}

/// Recolle les mots que la mise en page a coupés en fin de ligne.
///
/// Un mot composé garde son trait : on le reconnaît à ce que la suite en porte
/// un elle-même — `arc-` suivi de `en-ciel`.
fn join_hyphenated(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(previous) = out.last_mut() else {
            out.push(line);
            continue;
        };
        let cut = previous
            .strip_suffix('-')
            .filter(|head| head.chars().last().is_some_and(|c| c.is_alphabetic()));
        let follows = line
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() && c.is_lowercase());
        let Some(head) = cut.filter(|_| follows) else {
            out.push(line);
            continue;
        };
        let head = head.to_string();
        let compound = line
            .split_whitespace()
            .next()
            .is_some_and(|word| word.contains('-'));
        *previous = if compound {
            format!("{head}-{line}")
        } else {
            format!("{head}{line}")
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> Page {
        Page {
            lines: lines.iter().map(|l| l.to_string()).collect(),
        }
    }

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_word_split_across_lines_is_made_whole() {
        assert_eq!(
            join_hyphenated(lines(&["les règlements diffé-", "rés sur trente jours"])),
            lines(&["les règlements différés sur trente jours"])
        );
    }

    #[test]
    fn a_compound_word_keeps_its_hyphen() {
        assert_eq!(
            join_hyphenated(lines(&["il voit un arc-", "en-ciel superbe"])),
            lines(&["il voit un arc-en-ciel superbe"])
        );
    }

    #[test]
    fn a_dash_before_a_capital_is_left_alone() {
        // Un tiret suivi d'une majuscule termine une énumération, il ne coupe
        // pas un mot.
        assert_eq!(
            join_hyphenated(lines(&["voir annexe -", "Tableau des seuils"])),
            lines(&["voir annexe -", "Tableau des seuils"])
        );
    }

    #[test]
    fn a_blank_line_stops_the_join() {
        assert_eq!(
            join_hyphenated(lines(&["fin de section diffé-", "", "rés"])),
            lines(&["fin de section diffé-", "", "rés"])
        );
    }

    #[test]
    fn several_hyphenations_in_a_row_all_close() {
        assert_eq!(
            join_hyphenated(lines(&["pre-", "mier deu-", "xième"])),
            lines(&["premier deuxième"])
        );
    }

    #[test]
    fn ligatures_become_plain_letters() {
        assert_eq!(unligature("ﬁchier de conﬁguration"), "fichier de configuration");
        assert_eq!(unligature("aﬄux et eﬀort"), "afflux et effort");
    }

    #[test]
    fn a_soft_hyphen_leaves_no_trace() {
        assert_eq!(unligature("exem\u{ad}ple"), "exemple");
    }

    #[test]
    fn ordinary_text_passes_through_untouched() {
        assert_eq!(
            unligature("Spécification — « en attente »"),
            "Spécification — « en attente »"
        );
    }

    #[test]
    fn a_single_page_keeps_its_lines_in_order() {
        let out = to_markdown(&[page(&["1. Contexte", "", "Le service expose deux routes."])]);
        assert_eq!(out, "1. Contexte\n\nLe service expose deux routes.");
    }

    #[test]
    fn runs_of_blank_lines_collapse_into_one() {
        let out = to_markdown(&[page(&["Titre", "", "", "", "Suite"])]);
        assert_eq!(out, "Titre\n\nSuite");
    }

    #[test]
    fn right_padding_disappears() {
        let out = to_markdown(&[page(&["Titre    ", "Suite\t"])]);
        assert_eq!(out, "Titre\nSuite");
    }

    #[test]
    fn two_pages_are_separated_by_a_blank_line() {
        let out = to_markdown(&[page(&["fin de page"]), page(&["début de page"])]);
        assert_eq!(out, "fin de page\n\ndébut de page");
    }

    #[test]
    fn a_page_break_never_doubles_an_existing_blank() {
        let out = to_markdown(&[page(&["fin", ""]), page(&["suite"])]);
        assert_eq!(out, "fin\n\nsuite", "le saut de page a ajouté un blanc de trop");
    }

    #[test]
    fn nothing_trails_at_the_end() {
        let out = to_markdown(&[page(&["seule", "", ""])]);
        assert_eq!(out, "seule");
    }

    #[test]
    fn an_empty_document_yields_nothing() {
        assert_eq!(to_markdown(&[]), "");
        assert_eq!(to_markdown(&[page(&["", "  "])]), "");
    }
}
