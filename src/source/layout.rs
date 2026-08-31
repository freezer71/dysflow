//! Reconstruction d'un texte mis en page en markdown lisible.
//!
//! Un PDF ne livre que des lignes placées sur une feuille. Les rendre telles
//! quelles ferait de chaque ligne un bloc, et la navigation deviendrait
//! inutilisable. Ce module leur redonne une structure.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

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

/// Nombre de pages au-delà duquel une ligne répétée est tenue pour courante.
///
/// Deux occurrences peuvent être une coïncidence ; trois font une habitude.
const RUNNING_THRESHOLD: usize = 3;

/// Réduit une ligne à sa forme comparable d'une page à l'autre.
///
/// Le numéro change à chaque page : il ne doit pas empêcher de reconnaître
/// « Page 1 sur 40 » et « Page 2 sur 40 » comme le même pied.
fn normalise_running(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_digits = false;
    for c in line.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
                in_digits = true;
            }
        } else {
            in_digits = false;
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Retire les lignes que chaque page répète en tête ou en pied.
fn strip_running_heads(pages: &[Page]) -> Vec<Vec<String>> {
    let mut kept: Vec<Vec<String>> = pages.iter().map(|p| p.lines.clone()).collect();
    if kept.len() < RUNNING_THRESHOLD {
        return kept;
    }

    let mut heads: HashMap<String, usize> = HashMap::new();
    let mut feet: HashMap<String, usize> = HashMap::new();
    for lines in &kept {
        if let Some(line) = lines.iter().find(|l| !l.trim().is_empty()) {
            *heads.entry(normalise_running(line)).or_default() += 1;
        }
        if let Some(line) = lines.iter().rev().find(|l| !l.trim().is_empty()) {
            *feet.entry(normalise_running(line)).or_default() += 1;
        }
    }
    let running = |seen: &HashMap<String, usize>, line: &str| {
        seen.get(&normalise_running(line))
            .is_some_and(|&n| n >= RUNNING_THRESHOLD)
    };

    for lines in &mut kept {
        // Le pied d'abord : le retirer ne déplace pas l'en-tête.
        if let Some(i) = lines.iter().rposition(|l| !l.trim().is_empty()) {
            if running(&feet, &lines[i]) {
                lines.remove(i);
            }
        }
        if let Some(i) = lines.iter().position(|l| !l.trim().is_empty()) {
            if running(&heads, &lines[i]) {
                lines.remove(i);
            }
        }
    }
    kept
}

/// Ponctuations qui referment une phrase.
const CLOSING: [char; 5] = ['.', '!', '?', ':', ';'];

/// Met les pages bout à bout, en refermant les phrases coupées.
fn weld_pages(pages: Vec<Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for lines in pages {
        let opening = lines.iter().position(|l| !l.trim().is_empty());
        let welds = match (out.iter().rposition(|l| !l.trim().is_empty()), opening) {
            (Some(last), Some(first)) => {
                let tail = out[last].trim_end();
                let head = lines[first].trim_start();
                !tail.ends_with(CLOSING)
                    && head.chars().next().is_some_and(|c| c.is_lowercase())
            }
            _ => false,
        };
        if welds {
            let last = out
                .iter()
                .rposition(|l| !l.trim().is_empty())
                .expect("la branche ci-dessus l'a trouvé");
            let first = opening.expect("la branche ci-dessus l'a trouvé");
            // La fusion est calculée avant d'écrire : `out[last]` ne peut pas
            // être lu et assigné dans la même expression.
            let merged = format!("{} {}", out[last].trim_end(), lines[first].trim_start());
            out.truncate(last + 1);
            out[last] = merged;
            out.extend(lines.into_iter().skip(first + 1));
        } else {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend(lines);
        }
    }
    out
}

/// Longueur au-delà de laquelle une ligne est un paragraphe, pas un titre.
const HEADING_MAX: usize = 80;

/// Niveau de titre d'une ligne isolée, s'il y en a un.
///
/// Le niveau 1 est laissé au titre du document : une numérotation de premier
/// rang ouvre donc un niveau 2.
fn heading_level(line: &str) -> Option<u8> {
    // Même motif que `text::numerals` : un rang suivi de sa ponctuation, ou
    // une numérotation à plusieurs composantes. « 250 ms » n'en est pas un.
    static NUMBERED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:(?P<deep>\d+(?:\.\d+)+)\.?|(?P<flat>\d+)[.)])[ \t]+\S")
            .expect("motif constant")
    });

    let line = line.trim();
    if line.is_empty() || line.chars().count() > HEADING_MAX {
        return None;
    }
    // Un titre n'est pas une phrase : il ne se referme pas.
    if line.ends_with(['.', ',', ';']) {
        return None;
    }
    if let Some(caps) = NUMBERED.captures(line) {
        let number = caps.name("deep").or_else(|| caps.name("flat"))?;
        let depth = number.as_str().split('.').count() as u8;
        return Some((depth + 1).min(6));
    }
    let mut count = 0;
    for c in line.chars().filter(|c| c.is_alphabetic()) {
        if c.is_lowercase() {
            return None;
        }
        count += 1;
    }
    (count >= 3).then_some(2)
}

/// Contenu d'un item de liste, si la ligne en est un.
fn list_item(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = ['•', '●', '▪', '◦', '·', '–', '—', '*', '-']
        .iter()
        .find_map(|bullet| line.strip_prefix(*bullet))?;
    // Sans espace, le tiret appartient au mot qui suit.
    rest.starts_with([' ', '\t'])
        .then(|| rest.trim())
        .filter(|rest| !rest.is_empty())
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

    fn raw(pages: &[&[&str]]) -> Vec<Vec<String>> {
        pages
            .iter()
            .map(|p| p.iter().map(|l| l.to_string()).collect())
            .collect()
    }

    #[test]
    fn a_numbered_line_is_a_heading_of_its_depth() {
        assert_eq!(heading_level("1. Contexte"), Some(2));
        assert_eq!(heading_level("2) Contraintes"), Some(2));
        assert_eq!(heading_level("2.1 Latence"), Some(3));
        assert_eq!(heading_level("4.10.2 Repli"), Some(4));
    }

    #[test]
    fn a_line_in_capitals_is_a_heading() {
        assert_eq!(heading_level("ANNEXE TECHNIQUE"), Some(2));
    }

    #[test]
    fn ordinary_prose_is_not_a_heading() {
        assert_eq!(heading_level("250 ms de latence maximale"), None);
        assert_eq!(heading_level("Le service expose deux routes."), None);
        assert_eq!(heading_level(""), None);
    }

    #[test]
    fn a_long_line_is_never_a_heading() {
        let long = format!("1. {}", "mot ".repeat(40));
        assert_eq!(heading_level(&long), None, "un titre ne fait pas un paragraphe");
    }

    #[test]
    fn a_sentence_that_ends_in_a_full_stop_is_not_a_heading() {
        assert_eq!(heading_level("1. Le service traite les paiements."), None);
    }

    #[test]
    fn the_usual_bullets_are_recognised() {
        assert_eq!(list_item("• latence maximale"), Some("latence maximale"));
        assert_eq!(list_item("– disponibilité"), Some("disponibilité"));
        assert_eq!(list_item("- journalisation"), Some("journalisation"));
        assert_eq!(list_item("* trace"), Some("trace"));
    }

    #[test]
    fn a_bullet_needs_a_space_after_it() {
        assert_eq!(list_item("-sans espace"), None);
        assert_eq!(list_item("Une phrase - avec tiret"), None);
    }

    #[test]
    fn a_sentence_cut_by_a_page_break_closes_again() {
        let out = weld_pages(raw(&[&["aux opérations de"], &["remboursement, en différé."]]));
        assert_eq!(out, lines(&["aux opérations de remboursement, en différé."]));
    }

    #[test]
    fn a_finished_sentence_starts_a_new_line() {
        let out = weld_pages(raw(&[&["Fin de la partie."], &["Nouvelle partie."]]));
        assert_eq!(out, lines(&["Fin de la partie.", "", "Nouvelle partie."]));
    }

    #[test]
    fn a_capital_after_the_break_is_never_welded() {
        // Une majuscule ouvre une phrase : rien n'a été coupé.
        let out = weld_pages(raw(&[&["une liste des seuils"], &["Annexe technique"]]));
        assert_eq!(out, lines(&["une liste des seuils", "", "Annexe technique"]));
    }

    #[test]
    fn a_single_page_comes_back_as_it_is() {
        assert_eq!(weld_pages(raw(&[&["une", "", "deux"]])), lines(&["une", "", "deux"]));
    }

    #[test]
    fn a_header_repeated_on_every_page_disappears() {
        let doc = [
            page(&["Confidentiel", "", "Un"]),
            page(&["Confidentiel", "", "Deux"]),
            page(&["Confidentiel", "", "Trois"]),
        ];
        let kept = strip_running_heads(&doc);
        assert!(
            !kept.iter().flatten().any(|l| l == "Confidentiel"),
            "l'en-tête courant subsiste : {kept:?}"
        );
        assert!(kept.iter().flatten().any(|l| l == "Deux"), "le texte a été emporté");
    }

    #[test]
    fn a_page_number_that_changes_is_still_recognised() {
        let doc = [
            page(&["Un", "", "Page 1 sur 3"]),
            page(&["Deux", "", "Page 2 sur 3"]),
            page(&["Trois", "", "Page 3 sur 3"]),
        ];
        let kept = strip_running_heads(&doc);
        assert!(
            !kept.iter().flatten().any(|l| l.starts_with("Page ")),
            "le pied courant subsiste : {kept:?}"
        );
    }

    #[test]
    fn a_line_that_only_looks_repeated_is_kept() {
        // Deux pages sur cinq ne font pas une ligne courante.
        let doc = [
            page(&["Confidentiel", "", "Un"]),
            page(&["Confidentiel", "", "Deux"]),
            page(&["Trois"]),
            page(&["Quatre"]),
            page(&["Cinq"]),
        ];
        let kept = strip_running_heads(&doc);
        assert_eq!(
            kept.iter().flatten().filter(|l| *l == "Confidentiel").count(),
            2,
            "une ligne vue deux fois n'est pas courante"
        );
    }

    #[test]
    fn a_short_document_keeps_everything() {
        // Sur deux pages, rien ne prouve qu'une ligne est courante.
        let doc = [page(&["Confidentiel", "Un"]), page(&["Confidentiel", "Deux"])];
        let kept = strip_running_heads(&doc);
        assert_eq!(
            kept.iter().flatten().filter(|l| *l == "Confidentiel").count(),
            2
        );
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
