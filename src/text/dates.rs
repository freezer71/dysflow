//! Réécrit les dates en toutes lettres avant la mise en voix.
//!
//! « 2026-08-29 » sort d'un moteur TTS en morceaux — « vingt, vingt-six,
//! zéro huit, vingt-neuf » — parce que rien dans la suite de chiffres ne dit
//! qu'il s'agit d'une date. On lève l'ambiguïté ici, une fois, plutôt que de
//! l'abandonner à chaque moteur.

use crate::text::number::spell_cardinal;
use regex::{Captures, Regex};
use std::sync::LazyLock;

const MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre",
    "octobre", "novembre", "décembre",
];

/// Déterminants après lesquels un « le » de plus serait de trop.
const EXISTING_DETERMINERS: &[&str] = &["le", "du", "au", "ce", "cet", "les", "des", "aux"];

/// Trois formes, essayées dans cet ordre.
///
/// L'année à quatre chiffres est exigée partout : c'est elle qui distingue
/// une date d'une fraction. Sans elle, « 3/4 » et « 26/08/29 » deviendraient
/// des dates.
static DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)",
        // ISO : 2026-08-29, ou horodaté : 2026-08-29T10:30:00Z. L'heure est
        // consommée avec la date, sans quoi le « T » collé ferait échouer la
        // frontière de mot et la date resterait en chiffres.
        r"\b(?P<iy>\d{4})-(?P<im>\d{1,2})-(?P<id>\d{1,2})",
        r"(?:T(?P<ih>\d{2}):(?P<imn>\d{2})(?::\d{2}(?:\.\d+)?)?(?:Z|[+\-]\d{2}:?\d{2})?)?\b",
        r"|",
        // Numérique : 26/08/2029, 26.08.2029, 26-08-2029, 1er/01/2030
        r"\b(?P<na>\d{1,2})(?:er)?[/.\-](?P<nb>\d{1,2})[/.\-](?P<ny>\d{4})\b",
        r"|",
        // Mois nommé : 26 août 2029. Le jour et l'année restent des chiffres,
        // que le moteur lit aussi mal qu'ailleurs.
        r"\b(?P<ld>\d{1,2})(?:er)?\s+(?P<lm>janvier|février|fevrier|mars|avril|mai|juin|",
        r"juillet|août|aout|septembre|octobre|novembre|décembre|decembre)\s+(?P<ly>\d{4})\b",
    ))
    .unwrap()
});

/// Réécrit toutes les dates reconnues de `text`.
///
/// Ce qui n'est pas une date certaine reste intact : mieux vaut une lecture
/// hésitante qu'un contresens.
pub fn verbalize_dates(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for caps in DATE_PATTERN.captures_iter(text) {
        let matched = caps.get(0).expect("le groupe 0 existe toujours");
        // Une date rejetée n'avance pas le curseur : le fragment d'origine
        // sera recopié avec le reste du texte.
        let Some((day, month, year)) = resolve(&caps) else {
            continue;
        };
        out.push_str(&text[last..matched.start()]);
        out.push_str(article_before(&text[..matched.start()]));
        out.push_str(&spell_date(day, month, year));
        if let Some(hours) = caps.name("ih") {
            if let Some(clock) = spell_hour(hours.as_str(), caps.name("imn").map(|m| m.as_str())) {
                out.push_str(" à ");
                out.push_str(&clock);
            }
        }
        last = matched.end();
    }
    out.push_str(&text[last..]);
    out
}

/// Extrait le triplet d'une capture, quelle que soit la forme reconnue.
fn resolve(caps: &Captures) -> Option<(u64, usize, u64)> {
    if let (Some(y), Some(m), Some(d)) = (caps.name("iy"), caps.name("im"), caps.name("id")) {
        return checked(parse(d)?, parse(m)?, parse(y)?);
    }
    if let (Some(a), Some(b), Some(y)) = (caps.name("na"), caps.name("nb"), caps.name("ny")) {
        let (a, b, year) = (parse(a)?, parse(b)?, parse(y)?);
        // Convention française d'abord ; on ne bascule en mois-jour que si
        // elle est impossible — « 08/29/2026 » n'a pas de vingt-neuvième mois.
        return checked(a, b, year).or_else(|| checked(b, a, year));
    }
    if let (Some(d), Some(m), Some(y)) = (caps.name("ld"), caps.name("lm"), caps.name("ly")) {
        return checked(parse(d)?, month_index(m.as_str())? as u64 + 1, parse(y)?);
    }
    None
}

fn parse(m: regex::Match) -> Option<u64> {
    m.as_str().parse().ok()
}

fn checked(day: u64, month: u64, year: u64) -> Option<(u64, usize, u64)> {
    ((1..=31).contains(&day) && (1..=12).contains(&month))
        .then(|| (day, (month - 1) as usize, year))
}

/// Reconnaît un mois écrit, accents et casse indifférents.
fn month_index(name: &str) -> Option<usize> {
    let folded: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'û' | 'ù' => 'u',
            other => other,
        })
        .collect();
    MONTHS
        .iter()
        .position(|month| fold(month) == folded)
}

fn fold(month: &str) -> String {
    month
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'û' | 'ù' => 'u',
            other => other,
        })
        .collect()
}

/// L'heure d'un horodatage : « dix heures trente », « une heure ».
fn spell_hour(hours: &str, minutes: Option<&str>) -> Option<String> {
    let hours: u64 = hours.parse().ok()?;
    let mut out = match hours {
        0 => "zéro heure".to_string(),
        1 => "une heure".to_string(),
        other => format!("{} heures", spell_cardinal(other)),
    };
    match minutes.map(str::parse::<u64>) {
        None | Some(Ok(0)) => {}
        Some(Ok(1)) => out.push_str(" une"),
        Some(Ok(other)) => {
            out.push(' ');
            out.push_str(&spell_cardinal(other));
        }
        Some(Err(_)) => return None,
    }
    Some(out)
}

fn spell_date(day: u64, month: usize, year: u64) -> String {
    // Le premier du mois est le seul ordinal du calendrier français.
    let day = if day == 1 { "premier".to_string() } else { spell_cardinal(day) };
    format!("{day} {} {}", MONTHS[month], spell_cardinal(year))
}

/// Choisit l'article à insérer devant la date, d'après ce qui la précède.
fn article_before(before: &str) -> &'static str {
    let trimmed = before.trim_end();
    let Some(last_char) = trimmed.chars().last() else {
        return "Le ";
    };
    if matches!(last_char, '.' | '!' | '?' | '…') {
        return "Le ";
    }
    // Un deux-points annonce déjà la date — « Date : le vingt-neuf août » a un
    // article de trop. Une barre de tableau isole une cellule, où l'article
    // n'aurait pas plus de sens.
    if matches!(last_char, ':' | '|') {
        return "";
    }
    let word: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if EXISTING_DETERMINERS.contains(&word.to_lowercase().as_str()) {
        return "";
    }
    "le "
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_iso_dates() {
        assert_eq!(
            verbalize_dates("2026-08-29"),
            "Le vingt-neuf août deux mille vingt-six"
        );
    }

    #[test]
    fn converts_day_first_numeric_dates() {
        assert_eq!(
            verbalize_dates("Réunion le 26/08/2029."),
            "Réunion le vingt-six août deux mille vingt-neuf."
        );
    }

    #[test]
    fn accepts_dot_and_dash_separators() {
        let expected = "Le vingt-six août deux mille vingt-neuf";
        assert_eq!(verbalize_dates("26.08.2029"), expected);
        assert_eq!(verbalize_dates("26-08-2029"), expected);
    }

    #[test]
    fn falls_back_to_month_first_when_the_day_is_impossible() {
        assert_eq!(
            verbalize_dates("08/29/2026"),
            "Le vingt-neuf août deux mille vingt-six"
        );
    }

    #[test]
    fn spells_the_first_of_the_month_as_premier() {
        assert_eq!(
            verbalize_dates("01/01/2030"),
            "Le premier janvier deux mille trente"
        );
        assert_eq!(
            verbalize_dates("1er/01/2030"),
            "Le premier janvier deux mille trente"
        );
    }

    #[test]
    fn verbalises_dates_already_written_with_a_month_name() {
        assert_eq!(
            verbalize_dates("le 26 août 2029"),
            "le vingt-six août deux mille vingt-neuf"
        );
        assert_eq!(
            verbalize_dates("le 1er janvier 2030"),
            "le premier janvier deux mille trente"
        );
    }

    #[test]
    fn adds_the_article_only_when_it_is_missing() {
        assert!(verbalize_dates("à partir du 26/08/2029").starts_with("à partir du vingt-six"));
        assert!(verbalize_dates("expire 26/08/2029").starts_with("expire le vingt-six"));
    }

    #[test]
    fn drops_the_article_after_a_colon_or_a_table_cell() {
        // « Date : le vingt-neuf août » : le deux-points introduit déjà.
        assert_eq!(
            verbalize_dates("Date : 2026-08-29"),
            "Date : vingt-neuf août deux mille vingt-six"
        );
        assert_eq!(
            verbalize_dates("Date: 2026-08-29"),
            "Date: vingt-neuf août deux mille vingt-six"
        );
        // Les barres de tableau tiennent encore à ce stade du nettoyage.
        assert_eq!(
            verbalize_dates("| Date | 2026-08-29 |"),
            "| Date | vingt-neuf août deux mille vingt-six |"
        );
    }

    #[test]
    fn capitalises_the_article_at_the_start_of_a_sentence() {
        assert_eq!(
            verbalize_dates("Fin. 26/08/2029 arrive."),
            "Fin. Le vingt-six août deux mille vingt-neuf arrive."
        );
    }

    #[test]
    fn leaves_fractions_and_short_years_alone() {
        assert_eq!(verbalize_dates("3/4 du total"), "3/4 du total");
        assert_eq!(verbalize_dates("version 1/2"), "version 1/2");
        assert_eq!(verbalize_dates("26/08/29"), "26/08/29");
    }

    #[test]
    fn leaves_impossible_dates_alone() {
        assert_eq!(verbalize_dates("32/08/2029"), "32/08/2029");
        assert_eq!(verbalize_dates("26/13/2029"), "26/13/2029");
        assert_eq!(verbalize_dates("2026-13-29"), "2026-13-29");
    }

    #[test]
    fn always_says_day_then_month_then_year() {
        // Quel que soit l'ordre d'écriture, la date se dit à la française :
        // le jour en nombre, le mois en toutes lettres, l'année en entier.
        let expected = "vingt-neuf août deux mille vingt-six";
        for written in [
            "2026-08-29",  // ISO, année en tête
            "29/08/2026",  // français
            "29.08.2026",
            "29-08-2026",
            "08/29/2026",  // américain, mois en tête
            "29 août 2026", // déjà en lettres
        ] {
            let spoken = verbalize_dates(written);
            assert!(
                spoken.ends_with(expected),
                "{written:?} s'est dit {spoken:?}"
            );
        }
    }

    #[test]
    fn converts_every_date_in_a_paragraph() {
        assert_eq!(
            verbalize_dates("Du 26/08/2029 au 02/09/2029."),
            "Du vingt-six août deux mille vingt-neuf au deux septembre deux mille vingt-neuf."
        );
    }


    #[test]
    fn an_iso_timestamp_is_a_date_followed_by_its_hour() {
        assert_eq!(
            verbalize_dates("2026-08-29T10:30:00Z"),
            "Le vingt-neuf août deux mille vingt-six à dix heures trente"
        );
        assert_eq!(
            verbalize_dates("2026-08-29T10:00:00+02:00"),
            "Le vingt-neuf août deux mille vingt-six à dix heures"
        );
    }
}
