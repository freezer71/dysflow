//! Modèle de document : la hiérarchie que le lecteur parcourt et met en voix.
//!
//! Le document est *append-only* : les blocs arrivent au fil de l'eau (fichier
//! découpé d'avance, ou stdin lu en continu) et ne sont jamais réécrits. Les
//! index déjà distribués — id de phrase, de paragraphe, de section — restent
//! donc valides pour toute la durée de la session, ce dont dépendent le cache
//! de synthèse et les signets.

pub mod builder;

pub use builder::DocumentBuilder;

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

/// Un mot tel qu'il est affiché *et* prononcé.
///
/// `text` conserve sa ponctuation : le lecteur affiche une prose lisible, pas
/// une suite de tokens nus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    /// Index global, stable, utilisé par le surlignage.
    pub index: usize,
    pub punctuation: Option<Punctuation>,
}

impl Word {
    /// Poids temporel du mot dans sa phrase.
    fn weight(&self) -> f32 {
        let letters = self.text.chars().filter(|c| c.is_alphanumeric()).count() as f32;
        letters.max(1.0) + self.punctuation.map_or(0.0, |p| p.pause_weight())
    }
}

/// Unité de synthèse. Une phrase = une requête TTS = un buffer PCM en cache.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub id: usize,
    pub block_id: usize,
    pub section_id: usize,
    /// Texte nettoyé, envoyé tel quel au moteur TTS.
    pub text: String,
    pub words: Vec<Word>,
    pub trailing_pause_ms: u64,
}

impl Sentence {
    /// Répartit `total` entre les mots, proportionnellement à leur poids, et
    /// renvoie l'index du mot prononcé à `elapsed`.
    ///
    /// La durée vient du PCM réellement décodé, pas d'une constante : le
    /// curseur suit donc le son au lieu de dériver.
    pub fn word_at(&self, elapsed: f32, total: f32) -> Option<usize> {
        if self.words.is_empty() || total <= 0.0 {
            return None;
        }
        let ratio = (elapsed / total).clamp(0.0, 1.0);
        let sum: f32 = self.words.iter().map(Word::weight).sum();
        if sum <= 0.0 {
            return None;
        }
        let target = ratio * sum;
        let mut acc = 0.0;
        for (i, word) in self.words.iter().enumerate() {
            acc += word.weight();
            if target <= acc {
                return Some(i);
            }
        }
        Some(self.words.len() - 1)
    }
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
        // Reculer depuis le milieu d'un bloc revient d'abord à son début.
        if delta < 0 {
            let block = self.block(block_id)?;
            if from > block.sentences.0 {
                return Some(block.sentences.0);
            }
        }
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
