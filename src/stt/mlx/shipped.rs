//! La garde de fumée du build livré (`scripts/mlx-parite.sh --livre`),
//! commune à tous les modèles de dictée : ce qu'elle compare sur le décodage
//! d'une entrée, selon que l'entrée est de la parole ou non.
//!
//! R62 : le build livré (noyaux AOT de mlx-sys) n'est pas la preuve de
//! parité, qui reste le build JIT, à l'octet sur toutes les entrées ; il est
//! jugé sur le mel à l'octet, le nombre de pas à 10 % près et le texte.
//!
//! R69 : le nombre de pas et le texte ne sont comparés que sur la parole.
//! Sur une entrée sans parole (`NON_SPEECH`), les premiers jetons sont
//! presque à égalité : un écart d'encodeur de 1e-7 (les noyaux AOT,
//! acceptés par R62) y change un mot halluciné (vu sur Qwen3-ASR 1.7B :
//! « Je. » contre « C'est. » sur le silence). On n'y vérifie que le mel (à
//! la charge de chaque garde) et que le décodage finit dans son plafond de
//! jetons. Chaque entrée des fixtures est classée ici, par son nom : aucune
//! heuristique, et une entrée inconnue fait échouer la garde.

/// Les entrées de parole : les dictées Kokoro (`francais`, `anglais`) et ce
/// qui est taillé dans la parole (`douze_secondes`, `trente_cinq_secondes`).
pub const SPEECH: [&str; 4] = [
    "francais",
    "anglais",
    "douze_secondes",
    "trente_cinq_secondes",
];
/// Les entrées sans parole (R69) : une seconde de zéros, et un dixième de
/// seconde pris dans la dictée française, trop court pour un mot.
pub const NON_SPEECH: [&str; 2] = ["silence", "dixieme"];

/// L'entrée est de la parole ; un nom qui n'est dans aucune des deux listes
/// fait échouer la garde.
pub fn is_speech(input: &str) -> bool {
    if SPEECH.contains(&input) {
        return true;
    }
    assert!(
        NON_SPEECH.contains(&input),
        "{input} : entrée ni dans SPEECH ni dans NON_SPEECH (stt::mlx::shipped)"
    );
    false
}

/// Le nombre de pas (ou de jetons) d'une entrée. Parole : à 10 % près de la
/// fixture, arrondi vers le bas (une entrée de moins de 10 pas doit en avoir
/// exactement autant). Sans parole : seulement au plus `cap`, le plafond du
/// décodage (`None` pour un transducteur, borné par ses trames : il finit
/// toujours, et la mesure revenue le prouve). `detail` complète le message.
pub fn steps(input: &str, got: usize, expected: usize, cap: Option<usize>, detail: &str) {
    if is_speech(input) {
        let bound = expected / 10;
        assert!(
            got.abs_diff(expected) <= bound,
            "{input} : {got} pas contre {expected}, au-delà de la borne de {bound}{detail}"
        );
    } else if let Some(cap) = cap {
        assert!(
            got <= cap,
            "{input} : {got} pas, au-delà du plafond de {cap}"
        );
    }
}

/// Le texte d'une entrée, comparé à la fixture sur la parole seulement.
/// `what` nomme ce qui est comparé (« texte », « entier, texte »…).
pub fn text(input: &str, what: &str, got: &str, expected: &str) {
    if is_speech(input) {
        assert_eq!(got, expected, "{input} : {what}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_input_is_classified() {
        for input in SPEECH {
            assert!(is_speech(input));
        }
        for input in NON_SPEECH {
            assert!(!is_speech(input));
        }
    }

    #[test]
    #[should_panic(expected = "ni dans SPEECH")]
    fn an_unknown_input_fails_the_guard() {
        is_speech("musique");
    }

    #[test]
    fn non_speech_checks_only_the_cap() {
        steps("silence", 3, 2, Some(8192), "");
        steps("dixieme", 30, 2, None, "");
        text("silence", "texte", "Je.", "C'est.");
    }

    #[test]
    #[should_panic(expected = "plafond")]
    fn non_speech_beyond_the_cap_fails() {
        steps("silence", 1025, 2, Some(1024), "");
    }

    #[test]
    #[should_panic(expected = "borne de 0")]
    fn speech_keeps_the_step_bound() {
        steps("anglais", 3, 2, Some(8192), "");
    }

    #[test]
    #[should_panic(expected = "texte")]
    fn speech_keeps_the_text() {
        text("francais", "texte", "a", "b");
    }
}
