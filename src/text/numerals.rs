//! Met en mots les nombres restés dans le texte.
//!
//! Supertonic lit **caractère par caractère** : il n'a aucune normalisation de
//! nombres, et « 1 » lui parvient comme un signe dont il doit deviner la
//! prononciation. Écrire le nombre en toutes lettres lui retire la décision.
//!
//! Ce module passe après [`crate::text::dates`] : les dates sont déjà en mots,
//! il ne reste ici que des quantités, des rangs et des numéros.

use crate::text::number::{spell_cardinal, spell_ordinal};
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Au-delà, l'échelle française manque de noms et la mise en mots dirait faux.
/// Un nombre aussi long dans un texte n'est de toute façon plus une quantité.
const MAX_DIGITS: usize = 15;

/// Les formes reconnues, essayées dans cet ordre.
///
/// Les trois premières ne servent qu'à **protéger** : les faire correspondre
/// consomme le fragment sans le transformer. C'est ainsi qu'on obtient l'effet
/// d'un contexte négatif, que le moteur de regex ne sait pas exprimer.
static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)",
        // Identifiants : M5, F1, v3, v1.2 — des noms, pas des quantités.
        r"(?P<ident>[A-Za-zÀ-ÖØ-öø-ÿ]+\d+[A-Za-zÀ-ÖØ-öø-ÿ0-9.]*)",
        r"|(?P<version>\d+(?:\.\d+)+)",
        r"|(?P<fraction>\d+/\d+)",
        // Téléphone : dix chiffres derrière un 0 ou un +33, groupés ou non.
        r"|(?P<phone>(?:\+33|0)[ .\-]?\d(?:[ .\-]?\d{2}){4})\b",
        // Heures : 14h30, 14 h 30, 14h. Le \b final écarte « 5 hommes ».
        r"|(?P<th>\d{1,2})\s*h(?:\s*(?P<tm>\d{1,2}))?\b",
        r"|(?P<pct>\d+(?:,\d+)?)\s*%",
        // Unités : un symbole se passe de frontière de mot, une lettre non —
        // sans quoi « 3 gouttes » deviendrait « trois grammes ».
        "|(?P<symn>\\d+(?:[ \u{202F}\u{00A0}]\\d{3})*(?:,\\d+)?)\\s*(?P<syms>€|\\$|£|°C|°)",
        "|(?P<letn>\\d+(?:[ \u{202F}\u{00A0}]\\d{3})*(?:,\\d+)?)\\s*(?P<lets>km/h|km|cm|mm|kg|m|g)\\b",
        // Milliers séparés : 1 234 567, espace ordinaire, fine ou insécable.
        "|(?P<grp>\\d{1,3}(?:[ \u{202F}\u{00A0}]\\d{3})+)",
        r"|(?P<deci>\d+),(?P<decf>\d+)",
        r"|(?P<ordn>\d+)(?P<ords>ers|er|res|re|ères|ère|ndes|nde|nds|nd|èmes|ème|emes|eme|es|e)\b",
        r"|(?P<card>\d+)",
    ))
    .unwrap()
});

/// Met en mots la numérotation qui ouvre un titre.
///
/// Réservé aux titres : ailleurs, un nombre en tête de ligne est une puce de
/// liste, dont la structure est déjà portée autrement.
/// Numérotation qui ouvre un titre — `3.5`, `4.10`, `1.` — ou rien.
static SECTION: LazyLock<Regex> = LazyLock::new(|| {
    // Soit un sous-niveau — 3.5, 4.10 — soit un rang suivi de sa ponctuation.
    Regex::new(r"^[ \t]*(?:(?P<deep>\d+(?:\.\d+)+)\.?|(?P<flat>\d+)[.)])[ \t]+").unwrap()
});

/// Rend la numérotation d'un titre telle qu'elle est écrite, ponctuation
/// comprise.
///
/// L'affichage en a besoin : `sanitize` retire les rangs en tête de ligne,
/// utile pour une puce de liste, mais un titre doit garder le sien à l'écran.
pub fn section_number(text: &str) -> Option<&str> {
    let caps = SECTION.captures(text)?;
    let whole = caps.get(0)?.as_str();
    Some(whole.trim())
}

pub fn verbalize_section_number(text: &str) -> String {
    let Some(caps) = SECTION.captures(text) else {
        return text.to_string();
    };
    let number = caps.name("deep").or_else(|| caps.name("flat")).expect("une des deux branches");
    let spoken: Option<Vec<String>> = number
        .as_str()
        .split('.')
        .map(spell_plain)
        .collect();
    let Some(spoken) = spoken else {
        return text.to_string();
    };
    let rest = &text[caps.get(0).expect("le groupe 0 existe").end()..];
    format!("{}, {rest}", spoken.join(" point "))
}

/// Réécrit en toutes lettres tous les nombres reconnus de `text`.
pub fn verbalize_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for caps in NUMBER_PATTERN.captures_iter(text) {
        let matched = caps.get(0).expect("le groupe 0 existe toujours");
        // Rien à dire : le fragment sera recopié tel quel avec le reste.
        let Some(spoken) = render(&caps) else {
            continue;
        };
        out.push_str(&text[last..matched.start()]);
        if is_quantity(&caps) && is_negative(text, matched.start()) {
            // Le tiret vient d'être recopié : il devient le mot « moins ».
            out.pop();
            out.push_str("moins ");
        }
        out.push_str(&spoken);
        last = matched.end();
    }
    out.push_str(&text[last..]);
    out
}

fn render(caps: &Captures) -> Option<String> {
    // Les formes protégées n'ont pas de rendu : on les laisse passer.
    if caps.name("ident").is_some() || caps.name("version").is_some() {
        return None;
    }
    if caps.name("fraction").is_some() {
        return None;
    }
    if let Some(phone) = caps.name("phone") {
        return spell_phone(phone.as_str());
    }
    if let Some(hours) = caps.name("th") {
        return spell_clock(hours.as_str(), caps.name("tm").map(|m| m.as_str()));
    }
    if let Some(pct) = caps.name("pct") {
        let value = match pct.as_str().split_once(',') {
            Some((integer, fraction)) => spell_decimal(integer, fraction)?,
            None => spell_plain(pct.as_str())?,
        };
        return Some(format!("{value} pour cent"));
    }
    if let (Some(value), Some(unit)) = (caps.name("symn"), caps.name("syms")) {
        return spell_quantity(value.as_str(), unit.as_str());
    }
    if let (Some(value), Some(unit)) = (caps.name("letn"), caps.name("lets")) {
        return spell_quantity(value.as_str(), unit.as_str());
    }
    if let Some(grouped) = caps.name("grp") {
        let digits: String = grouped.as_str().chars().filter(char::is_ascii_digit).collect();
        return spell_plain(&digits);
    }
    if let (Some(integer), Some(fraction)) = (caps.name("deci"), caps.name("decf")) {
        return spell_decimal(integer.as_str(), fraction.as_str());
    }
    if let (Some(rank), Some(suffix)) = (caps.name("ordn"), caps.name("ords")) {
        let n: u64 = rank.as_str().parse().ok()?;
        let suffix = suffix.as_str().to_lowercase();
        // Seul le premier rang distingue le féminin.
        let feminine = suffix.starts_with('r') || suffix.starts_with('è') || suffix.starts_with("nde");
        return Some(spell_ordinal(n, feminine));
    }
    spell_plain(caps.name("card")?.as_str())
}

/// Met en mots une suite de chiffres, si elle reste dans l'échelle nommable.
fn spell_plain(digits: &str) -> Option<String> {
    if digits.len() > MAX_DIGITS {
        return None;
    }
    Some(spell_cardinal(digits.parse().ok()?))
}

/// « 3,05 » se dit « trois virgule zéro cinq » : les zéros de tête portent du
/// sens, les taire changerait la valeur.
fn spell_decimal(integer: &str, fraction: &str) -> Option<String> {
    let mut out = format!("{} virgule", spell_plain(integer)?);
    let rest = fraction.trim_start_matches('0');
    for _ in 0..fraction.len() - rest.len() {
        out.push_str(" zéro");
    }
    if !rest.is_empty() {
        out.push(' ');
        out.push_str(&spell_plain(rest)?);
    }
    Some(out)
}

/// Singulier et pluriel de chaque unité reconnue.
///
/// L'ordre compte : « km/h » avant « km », « kg » avant « g ».
const UNITS: &[(&str, &str, &str)] = &[
    ("€", "euro", "euros"),
    ("$", "dollar", "dollars"),
    ("£", "livre", "livres"),
    ("°C", "degré", "degrés"),
    ("°", "degré", "degrés"),
    ("km/h", "kilomètre-heure", "kilomètres-heure"),
    ("km", "kilomètre", "kilomètres"),
    ("cm", "centimètre", "centimètres"),
    ("mm", "millimètre", "millimètres"),
    ("kg", "kilogramme", "kilogrammes"),
    ("m", "mètre", "mètres"),
    ("g", "gramme", "grammes"),
];

/// Met en mots une valeur suivie de son unité, accordée.
///
/// Le français garde le singulier sous deux : « un virgule cinq euro ».
fn spell_quantity(value: &str, unit: &str) -> Option<String> {
    let (integer, fraction) = match value.split_once(',') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (value, None),
    };
    let digits: String = integer.chars().filter(char::is_ascii_digit).collect();
    let spoken = match fraction {
        Some(fraction) => spell_decimal(&digits, fraction)?,
        None => spell_plain(&digits)?,
    };
    let plural = digits.parse::<u64>().ok()? >= 2;
    let (_, singular, many) = UNITS.iter().find(|(symbol, _, _)| *symbol == unit)?;
    Some(format!("{spoken} {}", if plural { many } else { singular }))
}

/// « 14h30 » se dit « quatorze heures trente ».
///
/// Une heure est féminine et singulière — « une heure », jamais « un heures » —
/// et des minutes à zéro ne se disent pas.
fn spell_clock(hours: &str, minutes: Option<&str>) -> Option<String> {
    let hours: u64 = hours.parse().ok()?;
    let mut out = match hours {
        // Zéro et un laissent « heure » au singulier.
        0 => "zéro heure".to_string(),
        1 => "une heure".to_string(),
        other => format!("{} heures", spell_cardinal(other)),
    };
    match minutes.map(str::parse::<u64>) {
        Some(Ok(0)) | None => {}
        Some(Ok(1)) => out.push_str(" une"),
        Some(Ok(other)) => {
            out.push(' ');
            out.push_str(&spell_cardinal(other));
        }
        Some(Err(_)) => return None,
    }
    Some(out)
}

/// Vrai si le nombre est précédé d'un signe moins, et non d'un tiret de plage.
///
/// « -5 degrés » porte un signe ; « 10-20 » relie deux bornes.
fn is_negative(text: &str, start: usize) -> bool {
    let mut before = text[..start].chars().rev();
    if before.next() != Some('-') {
        return false;
    }
    matches!(before.next(), None | Some(' ') | Some('(') | Some('\u{00A0}'))
}

/// Vrai pour les formes qui expriment une quantité, seules à porter un signe.
fn is_quantity(caps: &Captures) -> bool {
    caps.name("card").is_some() || caps.name("deci").is_some() || caps.name("grp").is_some()
}

/// Lit un numéro par paires, comme il se dicte.
fn spell_phone(raw: &str) -> Option<String> {
    let mut spoken = Vec::new();
    let rest = match raw.strip_prefix("+33") {
        Some(rest) => {
            spoken.push("plus trente-trois".to_string());
            rest
        }
        None => raw,
    };
    for group in rest.split(|c: char| !c.is_ascii_digit()).filter(|g| !g.is_empty()) {
        for pair in split_in_pairs(group) {
            spoken.push(spell_pair(&pair)?);
        }
    }
    Some(spoken.join(", "))
}

/// Un numéro écrit d'un seul tenant se regroupe par deux, le chiffre isolé en
/// tête quand la longueur est impaire.
fn split_in_pairs(group: &str) -> Vec<String> {
    if group.len() <= 2 {
        return vec![group.to_string()];
    }
    let digits: Vec<char> = group.chars().collect();
    let mut pairs = Vec::new();
    let mut i = 0;
    if digits.len() % 2 == 1 {
        pairs.push(digits[0].to_string());
        i = 1;
    }
    while i + 1 < digits.len() {
        pairs.push(digits[i..i + 2].iter().collect());
        i += 2;
    }
    pairs
}

fn spell_pair(pair: &str) -> Option<String> {
    let value: u64 = pair.parse().ok()?;
    // « 06 » se dit « zéro six » : le zéro de tête s'entend.
    if pair.len() == 2 && pair.starts_with('0') {
        return Some(format!("zéro {}", spell_cardinal(value)));
    }
    Some(spell_cardinal(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_the_numbering_that_opens_a_heading() {
        assert_eq!(
            verbalize_section_number("3.5 Tarif kilométrique de repli"),
            "trois point cinq, Tarif kilométrique de repli"
        );
        assert_eq!(
            verbalize_section_number("4.10 PlaceDistance"),
            "quatre point dix, PlaceDistance"
        );
        assert_eq!(verbalize_section_number("1. Contexte"), "un, Contexte");
    }

    #[test]
    fn a_section_number_is_returned_as_written() {
        assert_eq!(section_number("1. Contexte"), Some("1."));
        assert_eq!(section_number("3.2 Latence"), Some("3.2"));
        assert_eq!(section_number("2) Contraintes"), Some("2)"));
        assert_eq!(section_number("Pile technique"), None);
        assert_eq!(section_number("2026 en chiffres"), None);
    }

    #[test]
    fn leaves_a_heading_that_merely_starts_with_a_number() {
        // Sans point ni sous-niveau, ce n'est pas une numérotation : le nombre
        // appartient au titre et sera dit comme une quantité.
        assert_eq!(verbalize_section_number("2026 en chiffres"), "2026 en chiffres");
        assert_eq!(verbalize_section_number("Pile technique"), "Pile technique");
    }

    #[test]
    fn spells_bare_cardinals() {
        assert_eq!(verbalize_numbers("lot 1"), "lot un");
        assert_eq!(verbalize_numbers("12 pièces"), "douze pièces");
        assert_eq!(verbalize_numbers("page 47"), "page quarante-sept");
        assert_eq!(verbalize_numbers("il en reste 1 sur 20"), "il en reste un sur vingt");
    }

    #[test]
    fn spells_long_numbers_in_full() {
        assert_eq!(
            verbalize_numbers("1234567"),
            "un million deux cent trente-quatre mille cinq cent soixante-sept"
        );
        assert_eq!(
            verbalize_numbers("192168"),
            "cent quatre-vingt-douze mille cent soixante-huit"
        );
    }

    #[test]
    fn spells_thousands_written_with_separators() {
        assert_eq!(
            verbalize_numbers("1 234 567"),
            "un million deux cent trente-quatre mille cinq cent soixante-sept"
        );
        assert_eq!(verbalize_numbers("12 500 euros"), "douze mille cinq cents euros");
    }

    #[test]
    fn reads_phone_numbers_in_pairs() {
        assert_eq!(
            verbalize_numbers("06 12 34 56 78"),
            "zéro six, douze, trente-quatre, cinquante-six, soixante-dix-huit"
        );
        assert_eq!(
            verbalize_numbers("+33 6 12 34 56 78"),
            "plus trente-trois, six, douze, trente-quatre, cinquante-six, soixante-dix-huit"
        );
        // Sans séparateurs, le découpage en paires est le nôtre.
        assert_eq!(
            verbalize_numbers("0612345678"),
            "zéro six, douze, trente-quatre, cinquante-six, soixante-dix-huit"
        );
    }

    #[test]
    fn spells_decimals_around_the_comma() {
        assert_eq!(verbalize_numbers("3,14"), "trois virgule quatorze");
        assert_eq!(verbalize_numbers("0,5"), "zéro virgule cinq");
        // Le zéro de tête se dit, sinon « 3,05 » deviendrait « trois virgule cinq ».
        assert_eq!(verbalize_numbers("3,05"), "trois virgule zéro cinq");
    }

    #[test]
    fn spells_percentages() {
        assert_eq!(verbalize_numbers("100 %"), "cent pour cent");
        assert_eq!(verbalize_numbers("50%"), "cinquante pour cent");
    }

    #[test]
    fn spells_ordinals() {
        assert_eq!(verbalize_numbers("le 1er"), "le premier");
        assert_eq!(verbalize_numbers("la 1re"), "la première");
        assert_eq!(verbalize_numbers("le 2e"), "le deuxième");
        assert_eq!(verbalize_numbers("le 80e"), "le quatre-vingtième");
        assert_eq!(verbalize_numbers("la 21ème"), "la vingt et unième");
    }

    #[test]
    fn spells_clock_times() {
        assert_eq!(verbalize_numbers("14h30"), "quatorze heures trente");
        assert_eq!(verbalize_numbers("14 h 30"), "quatorze heures trente");
        assert_eq!(verbalize_numbers("14h"), "quatorze heures");
        // Une heure est féminine et singulière.
        assert_eq!(verbalize_numbers("1h"), "une heure");
        assert_eq!(verbalize_numbers("1h30"), "une heure trente");
        assert_eq!(verbalize_numbers("0h05"), "zéro heure cinq");
        // Minuit et midi pile n'ont pas de minutes à dire.
        assert_eq!(verbalize_numbers("12h00"), "douze heures");
    }

    #[test]
    fn does_not_mistake_a_word_starting_with_h_for_an_hour() {
        assert_eq!(verbalize_numbers("5 hommes"), "cinq hommes");
        assert_eq!(verbalize_numbers("3 heures"), "trois heures");
    }

    #[test]
    fn says_the_minus_sign_of_a_negative_quantity() {
        // Sans cela, « -5 degrés » se lirait « cinq degrés » : un contresens.
        assert_eq!(verbalize_numbers("il fait -5 degrés"), "il fait moins cinq degrés");
        assert_eq!(verbalize_numbers("-2,5"), "moins deux virgule cinq");
        // Un tiret entre deux nombres est une plage, pas un signe.
        assert_eq!(verbalize_numbers("10-20"), "dix-vingt");
    }

    #[test]
    fn spells_currencies_with_their_agreement() {
        assert_eq!(verbalize_numbers("55 €"), "cinquante-cinq euros");
        assert_eq!(verbalize_numbers("1 €"), "un euro");
        assert_eq!(verbalize_numbers("12,50 €"), "douze virgule cinquante euros");
        assert_eq!(verbalize_numbers("30 $"), "trente dollars");
        // Sous deux, l'unité reste au singulier.
        assert_eq!(verbalize_numbers("1,5 €"), "un virgule cinq euro");
        assert_eq!(verbalize_numbers("0 €"), "zéro euro");
    }

    #[test]
    fn spells_common_units() {
        assert_eq!(verbalize_numbers("30 km"), "trente kilomètres");
        assert_eq!(verbalize_numbers("80 km/h"), "quatre-vingts kilomètres-heure");
        assert_eq!(verbalize_numbers("25 °C"), "vingt-cinq degrés");
        assert_eq!(verbalize_numbers("500 g"), "cinq cents grammes");
        assert_eq!(verbalize_numbers("1 m"), "un mètre");
    }

    #[test]
    fn does_not_mistake_a_word_for_a_unit() {
        // Le suffixe doit être l'unité entière, pas son initiale.
        assert_eq!(verbalize_numbers("3 gouttes"), "trois gouttes");
        assert_eq!(verbalize_numbers("5 minutes"), "cinq minutes");
        assert_eq!(verbalize_numbers("2 kilos"), "deux kilos");
    }

    #[test]
    fn leaves_versions_and_identifiers_alone() {
        // Un numéro de version n'est pas une quantité, et « M5 » est un nom.
        assert_eq!(verbalize_numbers("version 2.1"), "version 2.1");
        assert_eq!(verbalize_numbers("1.2.3"), "1.2.3");
        assert_eq!(verbalize_numbers("Apple M5 et la F1"), "Apple M5 et la F1");
        assert_eq!(verbalize_numbers("v3"), "v3");
    }

    #[test]
    fn leaves_fractions_alone() {
        assert_eq!(verbalize_numbers("3/4 du total"), "3/4 du total");
    }
}
