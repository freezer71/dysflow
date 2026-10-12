//! Le temps de chaque mot affiché, d'après les durées de phonèmes de Kokoro.
//!
//! La phrase est phonétisée entière (liaisons, nombres en contexte) et chaque
//! mot seul. espeak-ng fusionne parfois des mots : une distance d'édition,
//! à alignement monotone, rattache chaque phonème de la phrase à un mot
//! affiché. Aligner sur les mots affichés règle les formes parlées : « 1789 »
//! est un mot à l'écran et quatre dans la voix.

use crate::doc::WordSpan;

/// Une trame de Kokoro : 600 échantillons à 24 kHz.
pub const FRAME_SECONDS: f32 = 600.0 / 24_000.0;
const PUNCTUATION: &[char] = &[';', ':', ',', '.', '!', '?', '¡', '¿', '—', '…', '"', '«', '»', '“', '”', '(', ')'];

/// Pourquoi une phrase garde la répartition par poids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignFailure {
    /// Ni la phrase ni les mots n'ont de phonème.
    NoPhonemes,
    /// Les durées ne correspondent pas aux jetons (deux jetons de bord en plus).
    LengthMismatch,
    /// Le coût dépasse la moitié de la longueur.
    TooFar { cost: usize, length: usize },
    /// Ce mot n'a reçu aucun phonème.
    EmptyWord(usize),
}

fn is_phone(c: char) -> bool {
    !c.is_whitespace() && !PUNCTUATION.contains(&c)
}

/// Le temps de chaque mot. `tokens` : les phonèmes envoyés au modèle (sans
/// jetons de bord) ; `durations` : une durée par jeton, bords compris, en
/// trames ; `words` : les phonèmes de chaque mot affiché, seul.
pub fn word_spans(tokens: &[char], durations: &[u32], words: &[String]) -> Result<Vec<WordSpan>, AlignFailure> {
    if durations.len() != tokens.len() + 2 {
        return Err(AlignFailure::LengthMismatch);
    }
    let sentence: Vec<usize> = (0..tokens.len()).filter(|&i| is_phone(tokens[i])).collect();
    let target: Vec<(char, usize)> = words
        .iter()
        .enumerate()
        .flat_map(|(w, word)| word.chars().filter(|&c| is_phone(c)).map(move |c| (c, w)))
        .collect();
    if sentence.is_empty() || target.is_empty() {
        return Err(AlignFailure::NoPhonemes);
    }
    let (n, m) = (sentence.len(), target.len());
    let at = |i: usize, j: usize| i * (m + 1) + j;
    let mut cost = vec![0usize; (n + 1) * (m + 1)];
    for i in 0..=n {
        cost[at(i, 0)] = i;
    }
    for j in 0..=m {
        cost[at(0, j)] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let differ = usize::from(tokens[sentence[i - 1]] != target[j - 1].0);
            cost[at(i, j)] = (cost[at(i - 1, j - 1)] + differ)
                .min(cost[at(i - 1, j)] + 1)
                .min(cost[at(i, j - 1)] + 1);
        }
    }
    let total = cost[at(n, m)];
    let length = n.max(m);
    if total * 2 > length {
        return Err(AlignFailure::TooFar { cost: total, length });
    }

    // Retour en arrière : chaque phonème de la phrase reçoit son mot. Un
    // phonème sans pendant (liaison) va au mot qui suit le point d'alignement.
    let mut label = vec![0usize; n];
    let (mut i, mut j) = (n, m);
    while i > 0 {
        if j > 0 {
            let differ = usize::from(tokens[sentence[i - 1]] != target[j - 1].0);
            if cost[at(i, j)] == cost[at(i - 1, j - 1)] + differ {
                label[i - 1] = target[j - 1].1;
                i -= 1;
                j -= 1;
                continue;
            }
            if cost[at(i, j)] == cost[at(i, j - 1)] + 1 {
                j -= 1;
                continue;
            }
        }
        label[i - 1] = if j < m { target[j].1 } else { target[m - 1].1 };
        i -= 1;
    }

    // En demi-trames, pour couper les espaces en deux sans arrondi.
    let mut halves = vec![0u64; words.len()];
    let mut owned = vec![0usize; words.len()];
    let mut owner: Vec<Option<usize>> = vec![None; tokens.len()];
    for (k, &t) in sentence.iter().enumerate() {
        owner[t] = Some(label[k]);
        owned[label[k]] += 1;
    }
    // Un mot fait de ponctuation seule (« dit ? », « : » de la typographie
    // française) n'a aucun phonème : il prend les ponctuations de la phrase
    // entre ses voisins, partagées dans l'ordre s'ils sont plusieurs.
    let silent: Vec<bool> = words.iter().map(|w| !w.chars().any(is_phone)).collect();
    let mut w = 0;
    while w < words.len() {
        if !silent[w] {
            w += 1;
            continue;
        }
        let group: Vec<usize> = (w..words.len()).take_while(|&s| silent[s]).collect();
        w += group.len();
        let after = sentence.iter().rposition(|&t| owner[t].is_some_and(|o| o < group[0]));
        let before = sentence.iter().position(|&t| owner[t].is_some_and(|o| o > group[0]));
        let from = after.map_or(0, |k| sentence[k] + 1);
        let to = before.map_or(tokens.len(), |k| sentence[k]);
        let marks: Vec<usize> = (from..to).filter(|&t| !tokens[t].is_whitespace()).collect();
        for (k, &t) in marks.iter().enumerate() {
            let s = group[k * group.len() / marks.len()];
            owner[t] = Some(s);
            owned[s] += 1;
        }
    }
    let previous: Vec<Option<usize>> = (0..tokens.len())
        .scan(None, |last, t| {
            let before = *last;
            if owner[t].is_some() {
                *last = owner[t];
            }
            Some(before)
        })
        .collect();
    let mut next: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut upcoming = None;
    for t in (0..tokens.len()).rev() {
        next[t] = upcoming;
        if owner[t].is_some() {
            upcoming = owner[t];
        }
    }
    for t in 0..tokens.len() {
        let d = durations[t + 1] as u64;
        match (owner[t], previous[t], next[t]) {
            (Some(w), _, _) => halves[w] += 2 * d,
            (None, Some(p), Some(q)) if p != q => {
                halves[p] += d;
                halves[q] += d;
            }
            (None, Some(p), _) => halves[p] += 2 * d,
            (None, None, Some(q)) => halves[q] += 2 * d,
            (None, None, None) => {}
        }
    }
    let last = words.len() - 1;
    halves[0] += 2 * durations[0] as u64;
    halves[last] += 2 * durations[tokens.len() + 1] as u64;
    if let Some(empty) = (0..words.len()).find(|&w| owned[w] == 0 || halves[w] == 0) {
        return Err(AlignFailure::EmptyWord(empty));
    }

    let mut spans = Vec::with_capacity(words.len());
    let mut start = 0u64;
    for h in halves {
        let end = start + h;
        spans.push(WordSpan {
            start: start as f32 * FRAME_SECONDS / 2.0,
            end: end as f32 * FRAME_SECONDS / 2.0,
        });
        start = end;
    }
    Ok(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_liaison_consonant_belongs_to_the_next_word() {
        // « est arrivé » : espeak lie le t, que le mot seul n'a pas.
        let tokens = chars("ɛt aʁivˈe.");
        let durations = vec![2u32; tokens.len() + 2];
        let spans = word_spans(&tokens, &durations, &words(&["ɛ", "aʁivˈe."])).unwrap();
        // Mot 1 : ɛ (2 trames) + le jeton de bord du début (2) = 4 trames.
        assert_eq!(spans[0].start, 0.0);
        assert!((spans[0].end - 0.1).abs() < 1e-6, "{:?}", spans[0]);
        // Tout le reste, liaison comprise, va au mot 2 : 24 trames au total.
        assert_eq!(spans[1].start, spans[0].end);
        assert!((spans[1].end - 0.6).abs() < 1e-6, "{:?}", spans[1]);
    }

    #[test]
    fn spaces_and_punctuation_are_shared_between_neighbours() {
        let tokens = chars("ab, cd");
        // jeton de bord 0, a b , ␠ c d, jeton de bord 0
        let durations = vec![0, 1, 1, 2, 2, 1, 1, 0];
        let spans = word_spans(&tokens, &durations, &words(&["ab,", "cd"])).unwrap();
        // a b = 2 trames, plus la moitié de « , » et de l'espace = 2 : 4 trames.
        assert!((spans[0].end - 4.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
        assert!((spans[1].end - 8.0 * FRAME_SECONDS).abs() < 1e-6);
    }

    #[test]
    fn a_word_made_of_punctuation_takes_its_mark() {
        // Typographie française : « dit ? » affiche « ? » comme un mot.
        let tokens = chars("di ?");
        // jeton de bord 1, d i, ␠ 2, ? 4, jeton de bord 1
        let durations = vec![1, 1, 1, 2, 4, 1];
        let spans = word_spans(&tokens, &durations, &words(&["dˈi", "?"])).unwrap();
        // Mot 1 : bord 1 + d i 2 + la moitié de l'espace 1 = 4 trames.
        assert!((spans[0].end - 4.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
        // « ? » : l'autre moitié de l'espace, le point d'interrogation, le bord.
        assert!((spans[1].end - 10.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");

        // Au milieu de la phrase : « : » prend le sien, entre ses voisins.
        let tokens = chars("a : b");
        let durations = vec![0, 1, 2, 2, 2, 1, 0];
        let spans = word_spans(&tokens, &durations, &words(&["a", ":", "b"])).unwrap();
        assert!((spans[0].end - 2.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
        assert!((spans[1].end - 6.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
        assert!((spans[2].end - 8.0 * FRAME_SECONDS).abs() < 1e-6, "{spans:?}");
    }

    #[test]
    fn a_hopeless_alignment_gives_up() {
        let tokens = chars("bɔ̃ʒuʁ");
        let durations = vec![1u32; tokens.len() + 2];
        assert!(matches!(
            word_spans(&tokens, &durations, &words(&["xyzw", "qqq"])),
            Err(AlignFailure::TooFar { .. })
        ));
        assert_eq!(
            word_spans(&tokens, &durations, &words(&["bɔ̃ʒuʁ", "..."])),
            Err(AlignFailure::EmptyWord(1)),
            "un mot sans phonème"
        );
        assert_eq!(word_spans(&tokens, &[1, 1], &words(&["bɔ̃ʒuʁ"])), Err(AlignFailure::LengthMismatch));
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        sentence: String,
        words: Vec<String>,
        word_phonemes: Vec<String>,
    }

    /// Les phrases françaises du spike, phonétisées par espeak-ng
    /// (`examples/kokoro_fixtures.rs`) : le test tourne sans espeak.
    #[test]
    fn every_spike_sentence_is_aligned_in_order_and_in_full() {
        let fixtures: Vec<Fixture> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/align-fr.json")).unwrap();
        assert!(fixtures.len() >= 16, "{}", fixtures.len());
        let mismatched = fixtures
            .iter()
            .filter(|f| f.sentence.split_whitespace().count() != f.words.len())
            .count();
        assert!(mismatched >= 4, "les cas où les groupes ne suivent pas les mots : {mismatched}");
        for f in &fixtures {
            let tokens = chars(&f.sentence);
            let durations: Vec<u32> = (0..tokens.len() + 2).map(|i| 2 + (i % 3) as u32).collect();
            let total: u32 = durations.iter().sum();
            let spans = word_spans(&tokens, &durations, &f.word_phonemes).unwrap();
            assert_eq!(spans.len(), f.words.len());
            for pair in spans.windows(2) {
                assert!((pair[0].end - pair[1].start).abs() < 1e-6, "mots dans l'ordre, sans trou");
            }
            for s in &spans {
                assert!(s.end - s.start >= FRAME_SECONDS - 1e-6, "chaque mot dure au moins une trame");
            }
            assert!((spans.last().unwrap().end - total as f32 * FRAME_SECONDS).abs() < 1e-4, "somme = durée");
        }
    }
}
