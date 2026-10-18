//! Retire le *formatage* sans toucher au *texte*.
//!
//! Deux entrées coexistent : du markdown déjà démonté par `pulldown-cmark`
//! (dont il ne reste que le texte), et du contenu brut pipé directement —
//! tableaux ASCII, encadrés, flèches. Ce module vise le second cas, tout en
//! restant sûr sur le premier.
//!
//! Ce qu'il ne fait pas : supprimer la ponctuation. Elle porte l'intonation
//! côté TTS et la lisibilité côté écran.

use crate::text::acronyms::verbalize_acronyms;
use crate::text::dates::verbalize_dates;
use crate::text::numerals::verbalize_numbers;
use regex::Regex;
use std::sync::LazyLock;

/// Les regex sont compilées une seule fois pour tout le processus.
///
/// Elles étaient auparavant reconstruites à chaque appel — soit une fois par
/// bloc — ce qui dominait le coût du parsing.
struct Patterns {
    table_separator: Regex,
    box_drawing: Regex,
    md_link: Regex,
    md_image: Regex,
    url: Regex,
    heading_marker: Regex,
    emphasis: Vec<Regex>,
    bullet: Regex,
    numbered: Regex,
    arrow: Regex,
    rule: Regex,
    footnote: Regex,
    whitespace: Regex,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    // Ligne de séparation d'un tableau : |---|:--:|
    table_separator: Regex::new(r"(?m)^[ \t]*\|?[ \t:\-|]*\-[ \t:\-|]*\|?[ \t]*$").unwrap(),
    // Filets, blocs et demi-blocs Unicode.
    box_drawing: Regex::new(r"[\u{2500}-\u{259F}\u{2E80}-\u{2E99}]").unwrap(),
    // ![alt](src) avant [texte](url), sinon le « ! » resterait orphelin.
    md_image: Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap(),
    md_link: Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap(),
    url: Regex::new(r"(?i)\b(?:https?://|www\.)\S+").unwrap(),
    heading_marker: Regex::new(r"(?m)^[ \t]*#{1,6}[ \t]+").unwrap(),
    // Un motif par délimiteur : `regex` ne gère pas les backreferences, et
    // l'underscore est volontairement absent — `_` isolerait `to` dans
    // `read_to_string`. Le markdown réel est de toute façon démonté en amont.
    emphasis: vec![
        Regex::new(r"\*\*\*([^\s*](?:[^*]*[^\s*])?)\*\*\*").unwrap(),
        Regex::new(r"\*\*([^\s*](?:[^*]*[^\s*])?)\*\*").unwrap(),
        Regex::new(r"\*([^\s*](?:[^*]*[^\s*])?)\*").unwrap(),
        Regex::new(r"~~([^\s~](?:[^~]*[^\s~])?)~~").unwrap(),
        Regex::new(r"`([^`\n]+)`").unwrap(),
    ],
    // Ancrés au début d'une ligne prise à part : voir `strip_list_markers`,
    // qui décide quelle ligne est un item et laquelle n'est qu'une ligne
    // repliée commençant par un nombre ou un signe.
    bullet: Regex::new(r"\A[ \t]*[-*+•·][ \t]+").unwrap(),
    numbered: Regex::new(r"\A[ \t]*\d+[.)][ \t]+").unwrap(),
    arrow: Regex::new(r"(-->|<--|->|=>|<-|<=|=>|↔|⇔|→|←|⇒|⇐|↑|↓)").unwrap(),
    // Filets ASCII : ---- ==== ____ ****, au moins trois.
    rule: Regex::new(r"[-=_*+~]{3,}").unwrap(),
    footnote: Regex::new(r"\[\^[^\]]+\]").unwrap(),
    whitespace: Regex::new(r"\s+").unwrap(),
});

/// Met en mots et en lettres ce qui ne se dit pas comme il s'écrit.
///
/// Séparée de `sanitize` parce que les deux sorties n'ont pas le même
/// destinataire : l'écran montre « 2027 », le moteur entend « deux mille
/// vingt-sept ». Confondre les deux faisait lire au lecteur une prose qu'il
/// n'avait pas écrite.
///
/// À appliquer sur du texte déjà nettoyé : « **26/08/2029** » doit avoir perdu
/// ses astérisques pour que l'article se pose sur le bon mot, et les URL doivent
/// être parties, sinon un permalien daté se retrouverait lu à voix haute.
pub fn verbalize(clean: &str) -> String {
    // Les dates d'abord : leurs chiffres sont déjà en mots quand les quantités
    // sont examinées, et ne se disent donc pas deux fois. Les sigles en
    // dernier : un numéro cherche son libellé — « SIRET », « TVA » — et ne le
    // trouverait plus une fois celui-ci épelé.
    verbalize_acronyms(&verbalize_numbers(&verbalize_dates(clean)))
}

/// Retire le balisage sans toucher à la composition.
///
/// C'est ce que l'œil reçoit : les lignes du fichier, son indentation,
/// l'alignement de ses colonnes. Le balisage part parce que le terminal le
/// rendra autrement — un gras se montre en gras, non entre astérisques — mais
/// pas un espace de plus. Un document doit se reconnaître à l'écran.
pub fn sanitize_layout(raw: &str) -> String {
    tidy_punctuation(&strip_markup(raw))
}

/// Retire le balisage **et** la composition, pour n'en garder que la prose.
///
/// C'est ce que la voix reçoit, et ce sur quoi les phrases se découpent : un
/// texte d'un seul tenant, où un saut de ligne ne vaut qu'un espace.
pub fn sanitize(raw: &str) -> String {
    // Dérivée de la version composée, et non calculée à part : les deux
    // portent ainsi exactement les mêmes mots, dans le même ordre. C'est ce
    // qui permet au surlignage de suivre la voix sur un texte qui, lui, garde
    // la mise en page du fichier.
    collapse_layout(&sanitize_layout(raw))
}

/// La prose d'un texte déjà passé par [`sanitize_layout`] : ce que rendrait
/// `sanitize`, sans refaire le nettoyage.
pub fn collapse_layout(layout: &str) -> String {
    PATTERNS.whitespace.replace_all(layout, " ").trim().to_string()
}

/// Retire le balisage : marqueurs markdown, filets, barres, URL.
///
/// L'ordre compte : les images avant les liens, l'emphase avant les filets
/// (sinon `**gras**` serait mangé par la règle des répétitions), et les puces
/// avant tout ce qui pourrait les prendre pour des tirets.
fn strip_markup(raw: &str) -> String {
    let p = &*PATTERNS;
    let mut text = p.table_separator.replace_all(raw, " ").into_owned();
    text = p.md_image.replace_all(&text, "$1").into_owned();
    text = p.md_link.replace_all(&text, "$1").into_owned();
    text = p.footnote.replace_all(&text, "").into_owned();
    text = p.url.replace_all(&text, " ").into_owned();
    text = p.heading_marker.replace_all(&text, "").into_owned();
    text = strip_list_markers(&text);

    // L'emphase peut être imbriquée (***gras italique***) : deux passes
    // suffisent en pratique et bornent le coût.
    for _ in 0..2 {
        let before = text.clone();
        for pattern in &p.emphasis {
            text = pattern.replace_all(&text, "$1").into_owned();
        }
        if text == before {
            break;
        }
    }

    text = p.box_drawing.replace_all(&text, " ").into_owned();
    text = p.arrow.replace_all(&text, " ").into_owned();
    text = p.rule.replace_all(&text, " ").into_owned();
    // Les barres de tableau deviennent des séparations parlées.
    text.replace('|', " , ")
}

/// Retire puces et numéros des lignes qui sont des items, et d'elles seules.
///
/// Une ligne est un item si elle ouvre le texte, suit une ligne vide, ou suit
/// un autre item. Une ligne repliée au milieu d'un paragraphe — « prévue
/// pour\n2027. Le budget suit. », « Température :\n- 5 °C » — n'en est pas
/// un : y voir une puce ferait disparaître le nombre ou le signe, de l'écran
/// comme de la voix.
fn strip_list_markers(text: &str) -> String {
    let p = &*PATTERNS;
    let mut out = Vec::new();
    let mut previous_opens_item = true;
    for line in text.split('\n') {
        let marker = p.bullet.find(line).or_else(|| p.numbered.find(line));
        match marker {
            Some(m) if previous_opens_item => {
                out.push(&line[m.end()..]);
                previous_opens_item = true;
            }
            _ => {
                out.push(line);
                previous_opens_item = line.trim().is_empty();
            }
        }
    }
    out.join("\n")
}

/// Recolle la ponctuation isolée par les suppressions précédentes.
///
/// Retirer « **gras** » ou une URL laisse volontiers « mot , suite » ou
/// « fin . » ; un TTS lit ces espaces comme des hésitations.
fn tidy_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            // Absorbe les espaces précédant une ponctuation fermante — une URL
            // retirée en laisse plusieurs, et le saut de ligne n'en est pas.
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            if let Some(&next) = chars.get(j) {
                if matches!(next, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '…') {
                    // En français l'espace fine est d'usage devant ; : ! ?
                    if matches!(next, ';' | ':' | '!' | '?') {
                        out.push(' ');
                    }
                    i = j;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // Une ponctuation redoublée par le nettoyage ne doit pas se cumuler.
    let mut cleaned = String::with_capacity(out.len());
    let mut previous: Option<char> = None;
    for c in out.chars() {
        let duplicate = matches!(c, ',' | ';' | ':') && previous == Some(c);
        if !duplicate {
            cleaned.push(c);
        }
        previous = Some(c);
    }
    cleaned.trim().trim_matches(|c| c == ',').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_table_borders_and_box_drawing() {
        let input = "┌───────┬───────┐\n│ Nom   │ Rôle  │\n└───────┴───────┘";
        let clean = sanitize(input);
        assert!(!clean.contains('┌'));
        assert!(!clean.contains('│'));
        assert!(clean.contains("Nom"));
        assert!(clean.contains("Rôle"));
    }

    #[test]
    fn strips_markdown_table_pipes_but_keeps_cells() {
        let input = "| Serveur | Port |\n|---|---|\n| alpha | 8080 |";
        let clean = sanitize(input);
        assert!(!clean.contains('|'));
        assert!(!clean.contains("---"));
        assert!(clean.contains("Serveur"));
        // Le contenu de la cellule survit — dit en toutes lettres, comme tout
        // nombre destiné à la synthèse.
        assert!(clean.contains("8080"), "obtenu: {clean:?}");
    }

    #[test]
    fn keeps_the_composition_of_the_file_for_the_eye() {
        // L'œil suit le document tel qu'il est composé : ses lignes, son
        // indentation, l'alignement de ses colonnes. Seule la voix a besoin
        // d'un texte d'un seul tenant.
        let raw = "Premier paragraphe\n  une ligne indentée\n\nAprès un blanc";
        assert_eq!(
            sanitize_layout(raw),
            "Premier paragraphe\n  une ligne indentée\n\nAprès un blanc"
        );
        assert_eq!(
            sanitize(raw),
            "Premier paragraphe une ligne indentée Après un blanc"
        );
    }

    #[test]
    fn strips_the_markup_without_touching_the_layout() {
        // Le balisage part — c'est ce que le terminal rendra autrement — mais
        // pas un espace de plus.
        let raw = "## Titre\n\n- un **point**\n- deux colonnes    alignées";
        assert_eq!(
            sanitize_layout(raw),
            "Titre\n\nun point\ndeux colonnes    alignées"
        );
    }

    #[test]
    fn keeps_punctuation_intact() {
        let clean = sanitize("Vraiment ? Oui, bien sûr : c'est fait.");
        assert!(clean.contains('?'));
        assert!(clean.contains(','));
        assert!(clean.contains(':'));
        assert!(clean.ends_with('.'));
    }

    #[test]
    fn unwraps_emphasis_without_touching_identifiers() {
        assert_eq!(sanitize("un **gras** et un *italique*"), "un gras et un italique");
        // snake_case et multiplication survivent : l'emphase doit être appariée.
        assert_eq!(sanitize("appelle read_to_string ici"), "appelle read_to_string ici");
        // Ce qui compte ici est que l'astérisque de multiplication survive ;
        // la mise en mots des nombres est le fait d'une autre étape.
        assert_eq!(sanitize("3 * 4 = 12"), "3 * 4 = 12");
    }

    #[test]
    fn resolves_links_images_and_urls() {
        let clean = sanitize("voir [la doc](https://example.com/a) et ![schéma](x.png)");
        assert!(clean.contains("la doc"));
        assert!(clean.contains("schéma"));
        assert!(!clean.contains("https://"));
        assert!(!clean.contains(']'));
    }

    #[test]
    fn removes_arrows_and_rules() {
        let clean = sanitize("entrée -> sortie\n=====\nfin");
        assert!(!clean.contains("->"));
        assert!(!clean.contains("====="));
        assert!(clean.contains("entrée"));
        assert!(clean.contains("sortie"));
    }

    #[test]
    fn drops_leading_bullets_and_numbering() {
        assert_eq!(sanitize("- premier point"), "premier point");
        assert_eq!(sanitize("3. troisième point"), "troisième point");
    }

    #[test]
    fn leaves_figures_alone_for_the_eye() {
        // L'écran montre les chiffres tels qu'ils sont écrits : les mettre en
        // mots est le fait de `verbalize`, pour la voix seule.
        assert_eq!(sanitize("Le lot 1 contient 12 pièces."), "Le lot 1 contient 12 pièces.");
        assert_eq!(sanitize("Rendez-vous le 26/08/2029."), "Rendez-vous le 26/08/2029.");
    }

    #[test]
    fn verbalises_dates_for_the_synthesiser() {
        assert_eq!(
            verbalize(&sanitize("Rendez-vous le 26/08/2029 à Paris.")),
            "Rendez-vous le vingt-six août deux mille vingt-neuf à Paris."
        );
    }

    #[test]
    fn verbalises_dates_once_emphasis_is_unwrapped() {
        // L'ordre compte : détectée trop tôt, la date hériterait des « * ».
        assert_eq!(
            verbalize(&sanitize("Publié le **2026-08-29**.")),
            "Publié le vingt-neuf août deux mille vingt-six."
        );
    }

    #[test]
    fn verbalises_numbers_for_the_synthesiser() {
        assert_eq!(
            verbalize(&sanitize("Le lot 1 contient 12 pièces.")),
            "Le lot un contient douze pièces."
        );
    }

    #[test]
    fn resolves_dates_before_quantities() {
        // La date est consommée d'abord : ses chiffres ne repassent pas en
        // quantités, sans quoi « 2026 » se dirait deux fois.
        assert_eq!(
            verbalize(&sanitize("Date : 2026-08-29")),
            "Date : vingt-neuf août deux mille vingt-six"
        );
    }

    #[test]
    fn ignores_dates_buried_in_urls() {
        let clean = verbalize(&sanitize("voir https://example.com/2026-08-29/article"));
        assert!(!clean.contains("août"), "obtenu: {clean:?}");
    }

    #[test]
    fn says_acronyms_for_the_synthesiser_only() {
        // L'écran garde le sigle ; la voix reçoit ce qu'il faut dire.
        assert_eq!(sanitize("Total HT : 50 €"), "Total HT : 50 €");
        assert_eq!(
            verbalize(&sanitize("Total HT : 50 €")),
            "Total hache té : cinquante euros"
        );
    }

    #[test]
    fn spells_an_identifier_even_once_its_label_is_said() {
        // Les sigles passent après les nombres : « SIRET » doit encore être
        // lisible quand les chiffres qui le suivent cherchent leur libellé,
        // sans quoi le numéro redeviendrait une quantité.
        let said = verbalize(&sanitize("SIRET 984 733 204 00015"));
        assert!(said.starts_with("sirette"), "obtenu: {said:?}");
        assert!(said.contains("neuf huit quatre"), "obtenu: {said:?}");
    }

    #[test]
    fn repairs_spacing_left_by_removals() {
        // L'URL retirée laissait « doc . » avant recollage.
        let clean = sanitize("voir la doc https://example.com .");
        assert!(clean.ends_with("doc."), "obtenu: {clean:?}");
    }


    #[test]
    fn a_number_opening_a_continuation_line_is_not_a_list_marker() {
        // Une ligne repliée qui commence par un nombre n'est pas un item :
        // « 2027. » doit rester à l'écran comme à la voix.
        assert_eq!(
            sanitize("prévue pour\n2027. Le budget suit."),
            "prévue pour 2027. Le budget suit."
        );
        assert_eq!(
            sanitize_layout("Température :\n- 5 °C"),
            "Température :\n- 5 °C"
        );
    }
}
