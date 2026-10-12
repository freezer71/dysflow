//! Modèle de document : la hiérarchie que le lecteur parcourt et met en voix.
//!
//! Le document est *append-only* : les blocs arrivent au fil de l'eau (fichier
//! découpé d'avance, ou stdin lu en continu) et ne sont jamais réécrits. Les
//! index déjà distribués — id de phrase, de paragraphe, de section — restent
//! donc valides pour toute la durée de la session, ce dont dépendent le cache
//! de synthèse et les signets.

pub mod builder;

pub use builder::DocumentBuilder;

/// Le temps d'un mot dans sa phrase, en secondes de synthèse (1×) : l'unité
/// du chronomètre du lecteur, quelle que soit la vitesse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordSpan {
    pub start: f32,
    pub end: f32,
}

use serde::{Deserialize, Serialize};

/// Nature d'un bloc, qui détermine à la fois son rendu et sa mise en voix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    ListItem { depth: u8, ordinal: Option<u64> },
    Quote,
    Code { language: Option<String> },
    TableRow { header: bool },
}

impl BlockKind {
    /// Silence inséré après le bloc, en millisecondes.
    ///
    /// C'est la seule ponctuation « structurelle » que le lecteur ajoute : elle
    /// remplace la lecture vulgaire des marqueurs (« dièse titre »).
    pub fn trailing_pause_ms(&self) -> u64 {
        match self {
            BlockKind::Heading { level: 1 } => 900,
            BlockKind::Heading { level: 2 } => 700,
            BlockKind::Heading { .. } => 550,
            BlockKind::Paragraph => 400,
            BlockKind::ListItem { .. } => 250,
            BlockKind::Quote => 400,
            BlockKind::Code { .. } => 300,
            BlockKind::TableRow { .. } => 200,
        }
    }
}

/// Ponctuation portée par un mot, utilisée pour l'indicateur visuel et pour
/// pondérer la durée estimée du mot dans la phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Punctuation {
    Comma,
    Period,
    Question,
    Exclamation,
    Colon,
    SemiColon,
    Ellipsis,
    Dash,
}

impl Punctuation {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            ',' => Some(Punctuation::Comma),
            '.' => Some(Punctuation::Period),
            '?' => Some(Punctuation::Question),
            '!' => Some(Punctuation::Exclamation),
            ':' => Some(Punctuation::Colon),
            ';' => Some(Punctuation::SemiColon),
            '…' => Some(Punctuation::Ellipsis),
            '—' | '–' => Some(Punctuation::Dash),
            _ => None,
        }
    }

    /// Libellé affiché dans la barre d'état.
    pub fn label(&self) -> &'static str {
        match self {
            Punctuation::Comma => "virgule",
            Punctuation::Period => "point",
            Punctuation::Question => "point d'interrogation",
            Punctuation::Exclamation => "point d'exclamation",
            Punctuation::Colon => "deux-points",
            Punctuation::SemiColon => "point-virgule",
            Punctuation::Ellipsis => "points de suspension",
            Punctuation::Dash => "tiret",
        }
    }

    /// Poids de la pause associée, en « caractères équivalents ».
    ///
    /// Sert à répartir la durée réelle d'une phrase entre ses mots : une virgule
    /// ralentit le curseur, un point le ralentit davantage.
    pub fn pause_weight(&self) -> f32 {
        match self {
            Punctuation::Comma => 3.0,
            Punctuation::SemiColon | Punctuation::Colon => 4.0,
            Punctuation::Period | Punctuation::Question | Punctuation::Exclamation => 6.0,
            Punctuation::Ellipsis => 7.0,
            Punctuation::Dash => 2.0,
        }
    }
}

/// Mise en valeur portée par un mot dans le fichier.
///
/// Elle appartient à l'écrit seul : la voix ne dit ni le gras ni le lien —
/// elle en dirait le marqueur. L'œil, lui, y lit la structure de la phrase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emphasis {
    pub strong: bool,
    pub italic: bool,
    pub struck: bool,
    /// Texte d'un lien : ce qui est écrit, l'adresse restant hors du texte.
    pub link: bool,
    /// Code au fil du texte, entre accents graves.
    pub code: bool,
}

impl Emphasis {
    /// Vrai quand le mot ne porte aucune mise en valeur.
    pub fn is_plain(&self) -> bool {
        *self == Emphasis::default()
    }
}

/// Un mot tel qu'il est affiché.
///
/// `text` conserve sa ponctuation : le lecteur affiche une prose lisible, pas
/// une suite de tokens nus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    /// Index global, stable, utilisé par le surlignage.
    pub index: usize,
    pub punctuation: Option<Punctuation>,
    /// Forme parlée, quand elle diffère de la forme écrite.
    ///
    /// « 2027 » s'écrit en quatre signes et se dit en trois mots : c'est la
    /// seconde qui donne sa durée, donc son poids dans le surlignage.
    #[serde(default)]
    pub spoken: Option<String>,
    /// Mise en valeur du mot dans le fichier, pour l'écran seul.
    #[serde(default)]
    pub emphasis: Emphasis,
}

impl Word {
    /// Poids temporel du mot dans sa phrase.
    fn weight(&self) -> f32 {
        let said = self.spoken.as_deref().unwrap_or(&self.text);
        let letters = said.chars().filter(|c| c.is_alphanumeric()).count() as f32;
        letters.max(1.0) + self.punctuation.map_or(0.0, |p| p.pause_weight())
    }
}

/// Unité de synthèse. Une phrase = une requête TTS = un buffer PCM en cache.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub id: usize,
    pub block_id: usize,
    pub section_id: usize,
    /// Forme parlée, envoyée telle quelle au moteur TTS.
    ///
    /// Elle diffère de ce qui est à l'écran dès qu'un nombre ou une date est
    /// en jeu : voir `written`.
    pub text: String,
    pub words: Vec<Word>,
    pub trailing_pause_ms: u64,
}

impl Sentence {
    /// La phrase telle qu'elle est à l'écran.
    ///
    /// Ce que l'on cherche et ce que l'on cite est ce que l'on voit, pas la
    /// forme parlée : on ne retrouverait pas « 2027 » dans « deux mille
    /// vingt-sept ».
    pub fn written(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// Répartit `total` entre les mots, proportionnellement à leur poids, et
    /// renvoie l'index du mot prononcé à `elapsed`.
    ///
    /// La durée vient du PCM réellement décodé, pas d'une constante : le
    /// curseur suit donc le son au lieu de dériver.
    pub fn word_at(&self, elapsed: f32, total: f32) -> Option<usize> {
        self.word_fraction(elapsed, total).map(|(word, _)| word)
    }

    /// Le mot prononcé à `elapsed`, et la part de ce mot déjà dite (0 à 1).
    ///
    /// Même répartition que `word_at` : le trait sous la ligne avance dans le
    /// mot que le surlignage désigne, et repart de zéro au mot suivant.
    pub fn word_fraction(&self, elapsed: f32, total: f32) -> Option<(usize, f32)> {
        fraction_by_weight(&self.words, elapsed, total)
    }

    /// Le mot prononcé quand seuls les mots `range` sonnent — la tête ou la
    /// suite d'une phrase coupée, chacune jouée à part. `elapsed` et `total`
    /// comptent depuis le début de cette partie. Le minutage de la partie
    /// sert s'il couvre exactement ses mots, sinon la parole se répartit sur
    /// ces mots seuls ; au-delà, le surlignage attend sur le dernier.
    /// L'index rendu est celui du mot dans la phrase.
    pub fn word_fraction_range(
        &self,
        range: std::ops::Range<usize>,
        elapsed: f32,
        total: f32,
        spans: Option<&[WordSpan]>,
    ) -> Option<(usize, f32)> {
        let end = range.end.min(self.words.len());
        let start = range.start.min(end);
        let words = &self.words[start..end];
        let found = match spans {
            Some(spans) if !spans.is_empty() && spans.len() == words.len() => fraction_by_spans(elapsed, spans),
            _ => fraction_by_weight(words, elapsed, total),
        };
        found.map(|(i, fraction)| (start + i, fraction))
    }

    /// Instant où commence le mot `word`, en fraction de la parole.
    ///
    /// Réciproque de `word_at`, sur la même répartition par poids : un saut au
    /// mot tombe là où le surlignage le placerait.
    pub fn word_start(&self, word: usize) -> Option<f32> {
        if word >= self.words.len() {
            return None;
        }
        let sum: f32 = self.words.iter().map(Word::weight).sum();
        if sum <= 0.0 {
            return Some(0.0);
        }
        let before: f32 = self.words[..word].iter().map(Word::weight).sum();
        Some(before / sum)
    }

    /// Les mots tels que la voix les dit : forme parlée, sinon écrite.
    pub fn spoken_words(&self) -> Vec<String> {
        self.words
            .iter()
            .map(|w| w.spoken.clone().unwrap_or_else(|| w.text.clone()))
            .collect()
    }

    /// Le mot prononcé à `elapsed` d'après le minutage mesuré de chaque mot,
    /// et la part déjà dite. `None` si le minutage ne couvre pas les mots.
    pub fn word_fraction_timed(&self, elapsed: f32, spans: &[WordSpan]) -> Option<(usize, f32)> {
        if spans.is_empty() || spans.len() != self.words.len() {
            return None;
        }
        fraction_by_spans(elapsed, spans)
    }
}

/// Répartition par poids de `total` sur `words` : le mot à `elapsed`, et la
/// part déjà dite.
fn fraction_by_weight(words: &[Word], elapsed: f32, total: f32) -> Option<(usize, f32)> {
    if words.is_empty() || total <= 0.0 {
        return None;
    }
    let ratio = (elapsed / total).clamp(0.0, 1.0);
    let sum: f32 = words.iter().map(Word::weight).sum();
    if sum <= 0.0 {
        return None;
    }
    let target = ratio * sum;
    let mut acc = 0.0;
    for (i, word) in words.iter().enumerate() {
        let weight = word.weight();
        let start = acc;
        acc += weight;
        // Tolérance d'un cheveu sous la frontière : `word_start` puis
        // `word_at` repasse par des flottants et peut tomber un ulp en
        // dessous ; sans cela, un saut au mot surlignerait le précédent.
        if target < acc - sum * 1e-5 {
            // Un poids vaut au moins 1 : la division est sûre.
            return Some((i, ((target - start) / weight).clamp(0.0, 1.0)));
        }
    }
    Some((words.len() - 1, 1.0))
}

/// Le mot minuté à `elapsed`, et la part déjà dite ; au-delà du dernier, le
/// dernier, entièrement dit.
fn fraction_by_spans(elapsed: f32, spans: &[WordSpan]) -> Option<(usize, f32)> {
    for (i, span) in spans.iter().enumerate() {
        // Tolérance d'un cheveu sous la fin, comme `word_fraction` : un saut
        // au mot suivant repasse par `Duration` et peut revenir un ulp sous
        // son début, qui est la fin de celui-ci.
        let tolerance = (span.end.abs() * 1e-5).max(1e-6);
        if elapsed < span.end - tolerance {
            let length = (span.end - span.start).max(f32::EPSILON);
            return Some((i, ((elapsed - span.start) / length).clamp(0.0, 1.0)));
        }
    }
    Some((spans.len() - 1, 1.0))
}

/// Un bloc du document : un titre, un paragraphe, un item de liste…
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: usize,
    pub kind: BlockKind,
    pub section_id: usize,
    /// Texte nettoyé complet du bloc, pour l'affichage et les signets.
    pub text: String,
    /// Bornes `[start, end)` dans `Document::sentences`.
    pub sentences: (usize, usize),
    /// Lignes à afficher telles quelles, quand le texte prononcé ne peut pas
    /// tenir lieu d'affichage.
    ///
    /// Un bloc de code annoncé se dit « bloc de code rust » ; le montrer sous
    /// cette forme reviendrait à effacer de l'écran ce que la voix vient de
    /// s'abstenir de lire.
    #[serde(default)]
    pub display_lines: Vec<String>,
}

/// Un titre et l'étendue qu'il gouverne.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: usize,
    pub level: u8,
    pub title: String,
    /// Première phrase de la section, cible des sauts `n` / `p`.
    pub first_sentence: Option<usize>,
    pub block_ids: Vec<usize>,
}

/// La hiérarchie complète, en croissance continue.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub sections: Vec<Section>,
    pub blocks: Vec<Block>,
    pub sentences: Vec<Sentence>,
    pub total_words: usize,
    /// Faux tant que l'entrée n'a pas atteint EOF.
    pub complete: bool,
}

impl Document {
    pub fn sentence(&self, id: usize) -> Option<&Sentence> {
        self.sentences.get(id)
    }

    pub fn block(&self, id: usize) -> Option<&Block> {
        self.blocks.get(id)
    }

    pub fn section(&self, id: usize) -> Option<&Section> {
        self.sections.get(id)
    }

    pub fn is_empty(&self) -> bool {
        self.sentences.is_empty()
    }

    /// Section active pour une phrase donnée.
    pub fn section_of_sentence(&self, sentence_id: usize) -> Option<&Section> {
        self.sentence(sentence_id)
            .and_then(|s| self.section(s.section_id))
    }

    /// Première phrase du bloc voisin, dans la direction `delta`.
    pub fn sentence_in_neighbour_block(&self, from: usize, delta: isize) -> Option<usize> {
        let block_id = self.sentence(from)?.block_id;
        // Un cran est un bloc, dans les deux sens. Reculer passait d'abord par
        // le début du bloc courant : sur un paragraphe de plusieurs lignes, il
        // fallait deux touches pour bouger, et la première ne semblait rien
        // faire. Le recul fin appartient à la flèche gauche.
        let target = block_id.checked_add_signed(delta)?;
        self.block(target).map(|b| b.sentences.0)
    }

    /// Première phrase de la section voisine, dans la direction `delta`.
    pub fn sentence_in_neighbour_section(&self, from: usize, delta: isize) -> Option<usize> {
        let section_id = self.sentence(from)?.section_id;
        if delta < 0 {
            if let Some(start) = self.section(section_id).and_then(|s| s.first_sentence) {
                if from > start {
                    return Some(start);
                }
            }
        }
        let mut id = section_id;
        loop {
            id = id.checked_add_signed(delta)?;
            let section = self.section(id)?;
            // Une section sans phrase (titre encore seul) n'est pas une cible.
            if let Some(first) = section.first_sentence {
                return Some(first);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence_of(words: &[&str]) -> Sentence {
        Sentence {
            id: 0,
            block_id: 0,
            section_id: 0,
            text: words.join(" "),
            words: words
                .iter()
                .enumerate()
                .map(|(index, text)| Word {
                    text: text.to_string(),
                    index,
                    punctuation: None,
                    spoken: None,
                    emphasis: Emphasis::default(),
                })
                .collect(),
            trailing_pause_ms: 0,
        }
    }

    #[test]
    fn a_measured_timing_drives_the_highlight() {
        let s = sentence_of(&["un", "deux", "trois"]);
        let spans = [
            WordSpan { start: 0.0, end: 0.5 },
            WordSpan { start: 0.5, end: 0.6 },
            WordSpan { start: 0.6, end: 1.0 },
        ];
        assert_eq!(s.word_fraction_timed(0.25, &spans), Some((0, 0.5)));
        assert_eq!(s.word_fraction_timed(0.55, &spans).map(|(w, _)| w), Some(1));
        assert_eq!(s.word_fraction_timed(5.0, &spans), Some((2, 1.0)));
        // Un minutage qui ne couvre pas les mots ne sert à rien.
        assert_eq!(s.word_fraction_timed(0.1, &spans[..2]), None);
    }

    #[test]
    fn a_jump_to_a_measured_word_highlights_that_word() {
        // Le saut passe par `Duration::from_secs_f32` puis revient en `f32` :
        // l'aller-retour peut tomber un ulp sous le début du mot, qui est
        // aussi la fin du précédent. Le mot visé doit rester surligné.
        let words: Vec<String> = (0..6).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let s = sentence_of(&refs);
        for step in 1..10_000u32 {
            let unit = step as f32 * 0.0001;
            let spans: Vec<WordSpan> = (0..6)
                .map(|i| WordSpan { start: i as f32 * unit, end: (i + 1) as f32 * unit })
                .collect();
            for (i, span) in spans.iter().enumerate() {
                let elapsed = std::time::Duration::from_secs_f32(span.start).as_secs_f32();
                assert_eq!(
                    s.word_fraction_timed(elapsed, &spans).map(|(w, _)| w),
                    Some(i),
                    "mot {i}, début {} relu {elapsed}",
                    span.start
                );
            }
        }
    }

    #[test]
    fn spoken_words_prefer_the_spoken_form() {
        let mut s = sentence_of(&["en", "2027"]);
        s.words[1].spoken = Some("deux mille vingt-sept".into());
        assert_eq!(s.spoken_words(), ["en", "deux mille vingt-sept"]);
    }

    #[test]
    fn a_word_starts_where_the_highlight_places_it() {
        let s = sentence_of(&["un", "deux", "trois", "quatre"]);
        for word in 0..4 {
            let start = s.word_start(word).unwrap();
            assert_eq!(s.word_at(start, 1.0), Some(word), "mot {word}");
        }
    }

    #[test]
    fn the_fraction_grows_through_the_word_and_restarts_at_the_next() {
        // Deux mots de même poids : chacun tient la moitié de la parole.
        let s = sentence_of(&["deux", "mots"]);
        assert_eq!(s.word_fraction(0.0, 1.0), Some((0, 0.0)));
        let (word, fraction) = s.word_fraction(0.25, 1.0).unwrap();
        assert_eq!(word, 0);
        assert!((fraction - 0.5).abs() < 1e-5, "{fraction}");
        let (word, fraction) = s.word_fraction(0.75, 1.0).unwrap();
        assert_eq!(word, 1);
        assert!((fraction - 0.5).abs() < 1e-5, "{fraction}");
        // Au bout de la parole, et au-delà : le dernier mot, entièrement dit.
        assert_eq!(s.word_fraction(1.0, 1.0), Some((1, 1.0)));
        assert_eq!(s.word_fraction(3.0, 1.0), Some((1, 1.0)));
        // Avant le début : le premier mot, rien de dit.
        assert_eq!(s.word_fraction(-1.0, 1.0), Some((0, 0.0)));
    }

    #[test]
    fn the_fraction_follows_the_word_weights() {
        // « a » pèse 1, « abc » 3 : le premier mot s'achève au quart.
        let s = sentence_of(&["a", "abc"]);
        let (word, fraction) = s.word_fraction(0.125, 1.0).unwrap();
        assert_eq!(word, 0);
        assert!((fraction - 0.5).abs() < 1e-5, "{fraction}");
        let (word, fraction) = s.word_fraction(0.625, 1.0).unwrap();
        assert_eq!(word, 1);
        assert!((fraction - 0.5).abs() < 1e-5, "{fraction}");
    }

    #[test]
    fn at_a_word_start_the_fraction_is_zero_and_agrees_with_word_at() {
        let s = sentence_of(&["un", "deux", "trois", "quatre"]);
        for word in 0..4 {
            let start = s.word_start(word).unwrap();
            // Même tolérance que `word_at` : un ulp sous la frontière reste
            // au début du mot, pas à la fin du précédent.
            let (w, fraction) = s.word_fraction(start, 1.0).unwrap();
            assert_eq!(w, word, "mot {word}");
            assert!((0.0..1e-3).contains(&fraction), "mot {word} : {fraction}");
            for t in [start, start + 0.01, start + 0.1] {
                let word_of = s.word_fraction(t, 1.0).map(|(w, _)| w);
                assert_eq!(word_of, s.word_at(t, 1.0), "à {t}");
            }
        }
    }

    #[test]
    fn no_words_or_no_speech_has_no_fraction() {
        let s = sentence_of(&["un"]);
        assert_eq!(s.word_fraction(0.5, 0.0), None);
        assert_eq!(sentence_of(&[]).word_fraction(0.5, 1.0), None);
    }

    #[test]
    fn the_first_word_starts_at_zero_and_past_the_end_is_none() {
        let s = sentence_of(&["un", "deux"]);
        assert_eq!(s.word_start(0), Some(0.0));
        assert_eq!(s.word_start(2), None);
    }
}
