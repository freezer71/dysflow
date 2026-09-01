//! Retire le *formatage* sans toucher au *texte*.
//!
//! Deux entrées coexistent : du markdown déjà démonté par `pulldown-cmark`
//! (dont il ne reste que le texte), et du contenu brut pipé directement —
//! tableaux ASCII, encadrés, flèches. Ce module vise le second cas, tout en
//! restant sûr sur le premier.
//!
//! Ce qu'il ne fait pas : supprimer la ponctuation. Elle porte l'intonation
//! côté TTS et la lisibilité côté écran.

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
    bullet: Regex::new(r"(?m)^[ \t]*[-*+•·][ \t]+").unwrap(),
    numbered: Regex::new(r"(?m)^[ \t]*\d+[.)][ \t]+").unwrap(),
    arrow: Regex::new(r"(-->|<--|->|=>|<-|<=|=>|↔|⇔|→|←|⇒|⇐|↑|↓)").unwrap(),
    // Filets ASCII : ---- ==== ____ ****, au moins trois.
    rule: Regex::new(r"[-=_*+~]{3,}").unwrap(),
    footnote: Regex::new(r"\[\^[^\]]+\]").unwrap(),
    whitespace: Regex::new(r"\s+").unwrap(),
});

/// Nettoie un fragment pour l'affichage et la synthèse.
///
/// L'ordre compte : les images avant les liens, l'emphase avant les filets
/// (sinon `**gras**` serait mangé par la règle des répétitions), les puces
/// avant la normalisation des espaces, et les dates après tout ce qui peut
/// encore les entourer de bruit.
/// Met en mots ce qui s'écrit en chiffres, pour la voix seule.
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
    // sont examinées, et ne se disent donc pas deux fois.
    verbalize_numbers(&verbalize_dates(clean))
}

pub fn sanitize(raw: &str) -> String {
    let p = &*PATTERNS;
    let mut text = p.table_separator.replace_all(raw, " ").into_owned();
    text = p.md_image.replace_all(&text, "$1").into_owned();
    text = p.md_link.replace_all(&text, "$1").into_owned();
    text = p.footnote.replace_all(&text, "").into_owned();
    text = p.url.replace_all(&text, " ").into_owned();
    text = p.heading_marker.replace_all(&text, "").into_owned();
    text = p.bullet.replace_all(&text, "").into_owned();
    text = p.numbered.replace_all(&text, "").into_owned();

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
    text = text.replace('|', " , ");
    text = p.whitespace.replace_all(&text, " ").into_owned();

    tidy_punctuation(text.trim())
}

/// Recolle la ponctuation isolée par les suppressions précédentes.
///
/// Retirer « **gras** » ou une URL laisse volontiers « mot , suite » ou
/// « fin . » ; un TTS lit ces espaces comme des hésitations.
fn tidy_punctuation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' {
            // Absorbe l'espace précédant une ponctuation fermante.
            if let Some(&next) = chars.peek() {
                if matches!(next, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '…') {
                    // En français l'espace fine est d'usage devant ; : ! ?
                    if matches!(next, ';' | ':' | '!' | '?') {
                        out.push(' ');
                    }
                    continue;
                }
            }
        }
        out.push(c);
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
    fn repairs_spacing_left_by_removals() {
        // L'URL retirée laissait « doc . » avant recollage.
        let clean = sanitize("voir la doc https://example.com .");
        assert!(clean.ends_with("doc."), "obtenu: {clean:?}");
    }
}
