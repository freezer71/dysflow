//! Ce que l'interface voit du lecteur : un instantané par image, et le
//! document une fois par révision. Des valeurs plates, copiables, sans
//! référence vers le lecteur.

use crate::doc::{BlockKind, Document};
use crate::tts::Readiness;

use super::Reader;

/// État de la voix, tel que l'interface l'annonce.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum VoiceState {
    /// Voix relâchée ou pas encore chargée.
    #[default]
    Resting,
    /// Voix en cours de chargement.
    Loading,
    /// Voix prête à parler.
    Ready,
    /// Voix de repli, le moteur voulu ayant échoué.
    Fallback { reason: String },
}

/// Recherche en cours : requête, rang du résultat courant (à partir de un), total.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchState {
    pub query: String,
    pub position: usize,
    pub count: usize,
}

/// Tout ce qui change d'une image à l'autre.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    /// Change à chaque nouveau document.
    pub revision: u64,
    pub has_document: bool,
    /// Phrase courante.
    pub sentence: usize,
    /// Index global du mot lu, s'il y en a un.
    pub word: Option<usize>,
    /// Part du mot lu déjà prononcée, de 0 à 1 : le trait sous la ligne
    /// avance d'autant dans le mot.
    pub word_progress: f32,
    /// Section de la phrase courante.
    pub section: Option<usize>,
    pub playing: bool,
    /// Vrai quand un son sort réellement.
    pub audible: bool,
    pub finished: bool,
    /// Avancement dans le document, de 0 à 1.
    pub progress: f32,
    /// Secondes écoulées, estimées.
    pub elapsed: Option<f64>,
    /// Durée totale estimée, en secondes.
    pub total: Option<f64>,
    pub speed: f32,
    pub volume: f32,
    pub muted: bool,
    pub voice: VoiceState,
    /// Message d'état du lecteur.
    pub status: String,
    pub search: Option<SearchState>,
    /// Vrai quand la phrase courante porte un signet.
    pub bookmarked: bool,
    /// Nombre de phrases sautées.
    pub skipped: usize,
    /// Téléchargement de voix en cours ou échoué.
    pub download: Option<super::settings::DownloadView>,
    /// L'aperçu de voix (« Écouter »).
    pub preview: super::preview::PreviewView,
    /// Langue du document gardé : détectée, choisie, part de l'autre langue.
    pub document_language: Option<super::language::DocumentLanguage>,
    /// L'avis du passage à Kokoro, à montrer jusqu'à sa fermeture.
    pub notice: Option<String>,
    /// La proposition de passer à Kokoro, en attente d'une réponse.
    pub offer: Option<super::migration::Offer>,
}

/// Un mot, avec sa mise en valeur.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WordView {
    /// Index global du mot dans le document.
    pub index: usize,
    pub text: String,
    pub strong: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
    pub struck: bool,
}

/// Une phrase et ses mots.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SentenceView {
    pub id: usize,
    pub words: Vec<WordView>,
}

/// Le genre d'un bloc.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BlockKindView {
    #[default]
    Paragraph,
    Heading { level: u8 },
    ListItem { depth: u8, ordinal: Option<u64> },
    Quote,
    Code { language: Option<String> },
    TableRow { header: bool },
}

/// Un bloc du document et ses phrases.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockView {
    pub id: usize,
    pub kind: BlockKindView,
    pub section_id: usize,
    /// Lignes à afficher telles quelles (code, tableaux).
    pub display_lines: Vec<String>,
    pub sentences: Vec<SentenceView>,
}

/// Une section, pour le plan.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionView {
    pub id: usize,
    pub level: u8,
    pub title: String,
    pub first_sentence: Option<usize>,
}

/// Le document entier, une fois par révision.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocView {
    pub title: String,
    /// Faux tant que le document grandit encore.
    pub complete: bool,
    pub sentence_count: usize,
    pub sections: Vec<SectionView>,
    pub blocks: Vec<BlockView>,
}

pub fn snapshot(reader: &Reader) -> Snapshot {
    let voice = match (reader.is_resting(), reader.engine()) {
        (true, _) | (_, None) => VoiceState::Resting,
        (false, Some(engine)) => match engine.readiness() {
            Readiness::Ready => VoiceState::Ready,
            Readiness::Loading => VoiceState::Loading,
            Readiness::Fallback(reason) => VoiceState::Fallback { reason },
        },
    };
    let Some(app) = reader.session() else {
        // Au repos, la position retenue reste celle où l'on s'est arrêté ;
        // le document se reconstruit pour en tirer section et avancement.
        let sentence = reader.resting_at.unwrap_or(0);
        let (section, progress) = match reader.resting_document() {
            Some(doc) => {
                let total = doc.sentences.len().max(1) as f32;
                (
                    doc.section_of_sentence(sentence).map(|s| s.id),
                    (sentence as f32 / total).clamp(0.0, 1.0),
                )
            }
            None => (None, 0.0),
        };
        return Snapshot {
            revision: reader.revision(),
            has_document: reader.text_markdown().is_some(),
            sentence,
            section,
            progress,
            speed: reader.config().voice.speed,
            volume: reader.config().voice.volume,
            voice,
            download: reader.download(),
            preview: reader.preview_view(),
            document_language: reader.document_language().cloned(),
            notice: reader.notice().map(str::to_string),
            offer: reader.offer().cloned(),
            ..Snapshot::default()
        };
    };
    let doc = app.doc();
    let sentence = app.current_sentence();
    let word = app
        .reading_word()
        .and_then(|w| doc.sentence(sentence)?.words.get(w))
        .map(|w| w.index);
    let total = doc.sentences.len().max(1) as f32;
    Snapshot {
        revision: reader.revision(),
        has_document: true,
        sentence,
        word,
        word_progress: app.reading_word_progress(),
        section: doc.section_of_sentence(sentence).map(|s| s.id),
        playing: app.player.is_playing(),
        audible: app.player.is_playing() && app.player.position().is_some(),
        finished: app.is_finished(),
        progress: ((sentence as f32 + app.sentence_progress()) / total).clamp(0.0, 1.0),
        elapsed: app.estimated_elapsed(),
        total: app.estimated_total(),
        speed: app.config.voice.speed,
        volume: app.player.volume(),
        muted: app.player.is_muted(),
        voice,
        status: app.status().to_string(),
        search: app.search().map(|s| SearchState {
            query: s.query.clone(),
            position: s.position(),
            count: s.count(),
        }),
        bookmarked: app.bookmarked_here(),
        skipped: app.skipped_count(),
        download: reader.download(),
        preview: reader.preview_view(),
        document_language: reader.document_language().cloned(),
        notice: reader.notice().map(str::to_string),
        offer: reader.offer().cloned(),
    }
}

pub fn document(reader: &Reader) -> DocView {
    match (reader.session(), reader.text_markdown()) {
        (Some(app), _) => view_of(app.doc()),
        // Au repos, le document se reconstruit à l'identique depuis son
        // texte : mêmes blocs, mêmes index.
        (None, _) => reader.resting_document().map(view_of).unwrap_or_default(),
    }
}

fn view_of(doc: &Document) -> DocView {
    DocView {
        title: doc.title.clone(),
        complete: doc.complete,
        sentence_count: doc.sentences.len(),
        sections: doc
            .sections
            .iter()
            .map(|s| SectionView {
                id: s.id,
                level: s.level,
                title: s.title.clone(),
                first_sentence: s.first_sentence,
            })
            .collect(),
        blocks: doc
            .blocks
            .iter()
            .map(|b| BlockView {
                id: b.id,
                kind: kind_of(&b.kind),
                section_id: b.section_id,
                display_lines: b.display_lines.clone(),
                sentences: doc.sentences[b.sentences.0..b.sentences.1]
                    .iter()
                    .map(|s| SentenceView {
                        id: s.id,
                        words: s
                            .words
                            .iter()
                            .map(|w| WordView {
                                index: w.index,
                                text: w.text.clone(),
                                strong: w.emphasis.strong,
                                italic: w.emphasis.italic,
                                code: w.emphasis.code,
                                link: w.emphasis.link,
                                struck: w.emphasis.struck,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn kind_of(kind: &BlockKind) -> BlockKindView {
    match kind {
        BlockKind::Heading { level } => BlockKindView::Heading { level: *level },
        BlockKind::Paragraph => BlockKindView::Paragraph,
        BlockKind::ListItem { depth, ordinal } => BlockKindView::ListItem { depth: *depth, ordinal: *ordinal },
        BlockKind::Quote => BlockKindView::Quote,
        BlockKind::Code { language } => BlockKindView::Code { language: language.clone() },
        BlockKind::TableRow { header } => BlockKindView::TableRow { header: *header },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Player;
    use crate::config::Config;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::Arc;
    use std::time::Instant;

    fn reader_with(markdown: &str) -> Reader {
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut r = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        r.open_text(markdown, "test", Instant::now());
        r
    }

    #[test]
    fn an_empty_reader_has_no_document_and_a_resting_voice() {
        let r = Reader::new(
            Config::default(),
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(Player::silent),
            Instant::now(),
        );
        let s = snapshot(&r);
        assert!(!s.has_document);
        assert_eq!(s.voice, VoiceState::Resting);
        assert!(document(&r).blocks.is_empty());
    }

    #[test]
    fn the_document_keeps_structure_and_global_word_indices() {
        let r = reader_with("# Titre\n\nUn **deux** trois. Quatre.\n\n- item");
        let d = document(&r);
        // Section 0 est l'introduction implicite, avant tout titre.
        assert_eq!(d.sections[1].title, "Titre");
        assert_eq!(d.blocks[0].kind, BlockKindView::Heading { level: 1 });
        let para = &d.blocks[1];
        assert_eq!(para.kind, BlockKindView::Paragraph);
        assert_eq!(para.sentences.len(), 2);
        let deux = &para.sentences[0].words[1];
        assert_eq!(deux.text, "deux");
        assert!(deux.strong);
        // Les index sont globaux : le titre a pris le mot 0.
        assert_eq!(para.sentences[0].words[0].index, 1);
        assert!(matches!(d.blocks[2].kind, BlockKindView::ListItem { .. }));
        assert_eq!(d.sentence_count, 4);
    }

    #[test]
    fn a_word_jump_shows_in_the_snapshot_as_a_global_index() {
        let mut r = reader_with("# Titre\n\nUn deux trois.");
        r.with_session(Instant::now(), |app| app.seek_sentences(1));
        r.with_session(Instant::now(), |app| app.seek_words(2));
        let s = snapshot(&r);
        assert!(s.has_document);
        assert_eq!(s.revision, 1);
        assert_eq!(s.sentence, 1);
        assert_eq!(s.word, Some(3));
        // Aucun son ne mesure encore le mot : rien n'en est dit.
        assert_eq!(s.word_progress, 0.0);
        assert_eq!(s.voice, VoiceState::Ready);
        assert!(!s.playing);
    }

    #[test]
    fn a_resting_reader_still_shows_its_document() {
        let mut r = reader_with("Un. Deux.");
        r.rest();
        let s = snapshot(&r);
        assert!(s.has_document);
        assert_eq!(s.voice, VoiceState::Resting);
        assert_eq!(document(&r).sentence_count, 2);
    }

    #[test]
    fn a_resting_reader_keeps_the_position_where_it_stopped() {
        let mut r = reader_with("# Titre\n\nUn. Deux. Trois. Quatre.");
        r.with_session(Instant::now(), |app| app.seek_sentences(3));
        let avant = snapshot(&r);
        r.rest();
        let s = snapshot(&r);
        assert_eq!(s.voice, VoiceState::Resting);
        assert_eq!(s.sentence, avant.sentence);
        assert!(s.sentence >= 3);
        assert_eq!(s.section, avant.section);
        assert!(s.progress > 0.0);
    }
}
