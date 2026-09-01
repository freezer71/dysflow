//! Dit les sigles comme on les dit, non comme ils s'écrivent.
//!
//! Le moteur lit caractère par caractère, sans lexique : « HT » lui parvient
//! comme deux consonnes qu'aucune bouche française ne sait enchaîner, et
//! « SIRET » comme un mot dont le T final tomberait. Ce module tranche à sa
//! place — pour la voix seule, l'écran gardant le sigle tel qu'il est écrit.
//!
//! Deux moyens, dans cet ordre : un dictionnaire pour ce que l'usage seul
//! décide, puis une règle pour tout le reste. Ce qu'aucune syllabe française
//! ne peut porter s'épelle ; ce qui se dit se laisse dire.

/// Sigles dont la prononciation ne se déduit pas de la graphie.
///
/// Les premiers se disent comme des mots, et sont donc réécrits comme ils
/// sonnent. Les seconds s'épellent alors que la règle les croirait
/// prononçables : seul l'usage les distingue.
const SPOKEN: &[(&str, &str)] = &[
    ("SIRET", "sirette"),
    ("SIREN", "sirène"),
    ("URSSAF", "urssaf"),
    ("CNIL", "knil"),
    ("INSEE", "inssé"),
    ("SEPA", "sépa"),
    ("SASU", "sassu"),
    ("SAS", "sass"),
    ("KBIS", "ka bisse"),
    ("OK", "okay"),
    ("SA", "esse a"),
    ("APE", "a pé e"),
    ("API", "a pé i"),
    ("IA", "i a"),
    ("SAV", "esse a vé"),
    ("IP", "i pé"),
    ("UE", "u e"),
    ("URL", "u erre elle"),
    ("ONG", "o enne jé"),
    ("RATP", "erre a té pé"),
];

/// Au-delà, un mot tout en capitales est un mot — un titre, un nom — et
/// l'épeler rendrait un document entier illisible à l'oreille.
const MAX_SPELLED: usize = 6;

/// Met en voix les sigles d'un texte déjà nettoyé.
///
/// À appliquer **après** la mise en mots des nombres : « SIRET » doit encore
/// être lisible quand les chiffres qui le suivent cherchent leur libellé.
pub fn verbalize_acronyms(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars() {
        if c.is_alphabetic() {
            run.push(c);
            continue;
        }
        flush(&mut run, &mut out);
        out.push(c);
    }
    flush(&mut run, &mut out);
    out
}

fn flush(run: &mut String, out: &mut String) {
    if run.is_empty() {
        return;
    }
    out.push_str(&said(run));
    run.clear();
}

/// Ce qu'il faut dire d'un mot, quand ce mot n'en est pas un.
fn said(word: &str) -> String {
    // Une lettre seule ne se décide pas : « A » est un article autant qu'un
    // rang. Et un mot qui porte des minuscules n'est jamais un sigle.
    if word.chars().count() < 2 || !word.chars().all(char::is_uppercase) {
        return word.to_string();
    }
    if let Some((_, spoken)) = SPOKEN.iter().find(|(sigle, _)| *sigle == word) {
        return (*spoken).to_string();
    }
    if word.chars().count() > MAX_SPELLED || pronounceable(word) {
        return word.to_string();
    }
    spell(word).unwrap_or_else(|| word.to_string())
}

/// Épelle avec les noms français des lettres.
///
/// Rend `None` sur une lettre sans nom — un alphabet étranger, une lettre
/// accentuée — plutôt que d'en dire une partie : un sigle à moitié épelé
/// serait pire que le sigle intact.
fn spell(word: &str) -> Option<String> {
    let mut names = Vec::new();
    for c in word.chars() {
        names.push(letter_name(c)?);
    }
    Some(names.join(" "))
}

fn letter_name(c: char) -> Option<&'static str> {
    Some(match c {
        'A' => "a",
        'B' => "bé",
        'C' => "cé",
        'D' => "dé",
        'E' => "eu",
        'F' => "effe",
        'G' => "jé",
        'H' => "hache",
        'I' => "i",
        'J' => "ji",
        'K' => "ka",
        'L' => "elle",
        'M' => "aime",
        'N' => "enne",
        'O' => "o",
        'P' => "pé",
        'Q' => "ku",
        'R' => "erre",
        'S' => "esse",
        'T' => "té",
        'U' => "u",
        'V' => "vé",
        'W' => "double vé",
        'X' => "ixe",
        'Y' => "i grec",
        'Z' => "zède",
        _ => return None,
    })
}

/// Vrai quand le mot se laisse dire d'une traite en français.
///
/// La question n'est pas de savoir si le mot existe, mais si ses consonnes
/// s'enchaînent : sans voyelle il n'y a pas de syllabe, et « TV » n'ouvre
/// aucun mot français. Ce qui passe ce filtre est laissé au moteur, qui le
/// dira comme il l'aurait dit en minuscules.
fn pronounceable(word: &str) -> bool {
    let letters: Vec<char> = word.chars().flat_map(char::to_lowercase).collect();
    if !letters.iter().copied().any(is_vowel) {
        return false;
    }
    let mut i = 0;
    while i < letters.len() {
        if is_vowel(letters[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < letters.len() && !is_vowel(letters[i]) {
            i += 1;
        }
        if !cluster_fits(&letters[start..i], start == 0, i == letters.len()) {
            return false;
        }
    }
    true
}

/// Une suite de consonnes tient-elle dans une syllabe française ?
fn cluster_fits(cluster: &[char], opening: bool, closing: bool) -> bool {
    match cluster.len() {
        1 => true,
        2 if opening => {
            // Une attaque double se termine par une liquide : « br », « cl »,
            // « ch ». « TV », « CD », « PM » n'ouvrent rien.
            matches!(cluster[1], 'l' | 'r' | 'h')
        }
        2 if closing => {
            // Une coda double s'appuie sur une liquide ou une nasale, ou
            // s'achève en « s » ou « t » : « sarl », « labs », « ouest ».
            matches!(cluster[0], 'l' | 'r' | 'm' | 'n') || matches!(cluster[1], 's' | 't')
        }
        // Au milieu, deux consonnes se partagent entre deux syllabes ; trois
        // ne passent que si la dernière est liquide, comme dans « ordre ».
        2 => true,
        3 if !opening && !closing => matches!(cluster[2], 'l' | 'r'),
        _ => false,
    }
}

fn is_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'à' | 'â' | 'ä'
            | 'e' | 'é' | 'è' | 'ê' | 'ë'
            | 'i' | 'î' | 'ï'
            | 'o' | 'ô' | 'ö'
            | 'u' | 'ù' | 'û' | 'ü'
            | 'y' | 'ÿ'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_acronym_without_a_vowel_is_spelled_out() {
        // « HT » n'a aucune voyelle : le moteur, qui lit caractère par
        // caractère, ne peut qu'inventer. Les noms de lettres lèvent le doute.
        assert_eq!(verbalize_acronyms("Total HT"), "Total hache té");
        assert_eq!(verbalize_acronyms("prix TTC"), "prix té té cé");
        assert_eq!(verbalize_acronyms("la TVA"), "la té vé a");
    }

    #[test]
    fn an_acronym_read_as_a_word_gets_its_french_spelling() {
        // « SIRET » se dit « sirette » : lu tel quel, le T final tombe.
        assert_eq!(verbalize_acronyms("le SIRET"), "le sirette");
        assert_eq!(verbalize_acronyms("le SIREN"), "le sirène");
        assert_eq!(verbalize_acronyms("l'URSSAF"), "l'urssaf");
    }

    #[test]
    fn an_ordinary_word_in_capitals_is_left_alone() {
        // Un titre en capitales est fait de mots, pas de sigles : les épeler
        // rendrait tout un document illisible à l'oreille.
        assert_eq!(verbalize_acronyms("MENTIONS LÉGALES"), "MENTIONS LÉGALES");
        assert_eq!(verbalize_acronyms("KOUMA LABS"), "KOUMA LABS");
    }

    #[test]
    fn a_lone_letter_is_never_spelled() {
        // « A » est aussi bien un article qu'une lettre : sans certitude, rien.
        assert_eq!(verbalize_acronyms("le plan A"), "le plan A");
        assert_eq!(verbalize_acronyms("A B C"), "A B C");
    }

    #[test]
    fn an_unknown_unpronounceable_acronym_is_spelled_too() {
        // Aucun dictionnaire ne les tiendra tous : ce qu'aucune bouche
        // française ne peut dire s'épelle.
        assert_eq!(verbalize_acronyms("la SNCF"), "la esse enne cé effe");
        assert_eq!(verbalize_acronyms("un CDI"), "un cé dé i");
    }

    #[test]
    fn lower_case_words_are_untouched() {
        // « sas » et « sarl » existent en français : seule la casse distingue
        // le sigle du mot.
        assert_eq!(verbalize_acronyms("le sas d'entrée"), "le sas d'entrée");
    }
}
