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
//! la charge de chaque garde) et, R70, la raison d'arrêt du décodage, égale
//! à celle du Swift : fini sur une fin de texte si et seulement si la
//! fixture l'est, plafond atteint si et seulement si la fixture l'atteint
//! (Whisper tiny atteint son plafond sur le silence dans le Swift, R64 :
//! un « < plafond » strict serait faux). Chaque garde dit d'où vient sa
//! raison d'arrêt (`Stop`). Chaque entrée des fixtures est classée ici, par
//! son nom : aucune heuristique, et une entrée inconnue fait échouer la
//! garde.

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

/// Le nombre de pas (ou de jetons) d'une entrée, comparé sur la parole
/// seulement : à 10 % près de la fixture, arrondi vers le bas (une entrée
/// de moins de 10 pas doit en avoir exactement autant). `detail` complète
/// le message.
pub fn steps(input: &str, got: usize, expected: usize, detail: &str) {
    if is_speech(input) {
        let bound = expected / 10;
        assert!(
            got.abs_diff(expected) <= bound,
            "{input} : {got} pas contre {expected}, au-delà de la borne de {bound}{detail}"
        );
    }
}

/// Pourquoi un décodage s'est arrêté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Le modèle a fini seul (fin de texte, ou garde-fou du modèle).
    Ended,
    /// Le plafond de jetons est atteint.
    Cap,
}

impl Stop {
    /// La raison déduite du nombre de jetons, pour un décodeur qui n'en
    /// garde pas d'autre trace : `Cap` si et seulement si `tokens` atteint
    /// `cap`. Appliquée aux deux côtés, le Rust et la fixture.
    pub fn from_count(tokens: usize, cap: usize) -> Self {
        if tokens >= cap {
            Self::Cap
        } else {
            Self::Ended
        }
    }
}

/// R70 : sur une entrée sans parole, la raison d'arrêt (`Stop`, ou la fin du
/// flux de Voxtral) égale à celle de la fixture, dans les deux sens. Sur la
/// parole, le nombre de pas et le texte la couvrent déjà.
pub fn stop<T: PartialEq + std::fmt::Debug>(input: &str, what: &str, got: T, expected: T) {
    if !is_speech(input) {
        assert_eq!(
            got, expected,
            "{input} : {what}, raison d'arrêt différente du Swift"
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

    /// Chaque entrée de chaque fixture de dictée (`tests/fixtures/mlx`,
    /// sans les poids) est classée.
    #[test]
    fn every_fixture_input_is_classified() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mlx");
        let mut fixtures = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let json: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let Some(inputs) = json.get("inputs").and_then(|i| i.as_array()) else {
                continue;
            };
            fixtures += 1;
            for input in inputs {
                let name = input["name"].as_str().expect("nom d'entrée");
                assert_eq!(
                    is_speech(name),
                    SPEECH.contains(&name),
                    "{}",
                    path.display()
                );
            }
        }
        assert!(fixtures >= 11, "{fixtures} fixtures lues");
    }

    #[test]
    #[should_panic(expected = "ni dans SPEECH")]
    fn an_unknown_input_fails_the_guard() {
        is_speech("musique");
    }

    #[test]
    fn non_speech_compares_only_the_stop_reason() {
        steps("silence", 3, 2, "");
        text("silence", "texte", "Je.", "C'est.");
        stop("silence", "fin", Stop::Ended, Stop::Ended);
        stop("dixieme", "fin", Stop::Cap, Stop::Cap);
        // Sur la parole, la raison n'est pas comparée (pas et texte le sont).
        stop("anglais", "fin", Stop::Cap, Stop::Ended);
    }

    #[test]
    #[should_panic(expected = "raison d'arrêt")]
    fn non_speech_ending_where_the_swift_hit_the_cap_fails() {
        stop("silence", "fin", Stop::Ended, Stop::Cap);
    }

    #[test]
    #[should_panic(expected = "raison d'arrêt")]
    fn non_speech_hitting_the_cap_where_the_swift_ended_fails() {
        stop("dixieme", "fin", Stop::Cap, Stop::Ended);
    }

    #[test]
    #[should_panic(expected = "raison d'arrêt")]
    fn a_stream_flag_that_differs_fails() {
        stop("silence", "fin du flux", false, true);
    }

    #[test]
    fn the_cap_is_reached_at_the_count() {
        assert_eq!(Stop::from_count(431, 432), Stop::Ended);
        assert_eq!(Stop::from_count(432, 432), Stop::Cap);
    }

    #[test]
    #[should_panic(expected = "borne de 0")]
    fn speech_keeps_the_step_bound() {
        steps("anglais", 3, 2, "");
    }

    #[test]
    #[should_panic(expected = "texte")]
    fn speech_keeps_the_text() {
        text("francais", "texte", "a", "b");
    }
}
