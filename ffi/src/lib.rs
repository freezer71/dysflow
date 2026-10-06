//! Le lecteur, vu de Swift.
//!
//! Des types miroirs, aux entiers de taille fixe que UniFFI sait passer, et
//! un objet `SillonReader` qui garde le fil du lecteur. Aucun état ici : tout
//! vit dans `sillon::reader`.

use std::sync::Arc;
use std::time::Instant;

use sillon::audio::Player;
use sillon::config::Config;
use sillon::reader::command::Command;
use sillon::reader::handle::{ReaderHandle, Request};
use sillon::reader::{settings, view, Reader};
use sillon::tts::{self, silent::SilentEngine, TtsEngine};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SillonError {
    #[error("{message}")]
    Failed { message: String },
}

fn failed(e: impl ToString) -> SillonError {
    SillonError::Failed { message: e.to_string() }
}

#[derive(uniffi::Enum)]
pub enum ReaderCommand {
    TogglePlay,
    Pause,
    Words { delta: i32 },
    Sentences { delta: i32 },
    Blocks { delta: i32 },
    Sections { delta: i32 },
    SeekTo { sentence: u32 },
    RestartSentence,
    Start,
    End,
    SetSpeed { speed: f32 },
    ChangeSpeed { delta: f32 },
    ChangeVolume { delta: f32 },
    ToggleMute,
    ToggleBookmark,
    Search { query: String },
    SearchAdvance { delta: i32 },
    SearchCancel,
}

impl From<ReaderCommand> for Command {
    fn from(c: ReaderCommand) -> Self {
        match c {
            ReaderCommand::TogglePlay => Command::TogglePlay,
            ReaderCommand::Pause => Command::Pause,
            ReaderCommand::Words { delta } => Command::Words(delta as isize),
            ReaderCommand::Sentences { delta } => Command::Sentences(delta as isize),
            ReaderCommand::Blocks { delta } => Command::Blocks(delta as isize),
            ReaderCommand::Sections { delta } => Command::Sections(delta as isize),
            ReaderCommand::SeekTo { sentence } => Command::SeekTo(sentence as usize),
            ReaderCommand::RestartSentence => Command::RestartSentence,
            ReaderCommand::Start => Command::Start,
            ReaderCommand::End => Command::End,
            ReaderCommand::SetSpeed { speed } => Command::SetSpeed(speed),
            ReaderCommand::ChangeSpeed { delta } => Command::ChangeSpeed(delta),
            ReaderCommand::ChangeVolume { delta } => Command::ChangeVolume(delta),
            ReaderCommand::ToggleMute => Command::ToggleMute,
            ReaderCommand::ToggleBookmark => Command::ToggleBookmark,
            ReaderCommand::Search { query } => Command::Search(query),
            ReaderCommand::SearchAdvance { delta } => Command::SearchAdvance(delta as isize),
            ReaderCommand::SearchCancel => Command::SearchCancel,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum VoiceState {
    Resting,
    Loading,
    Ready,
    Fallback { reason: String },
}

#[derive(uniffi::Record)]
pub struct SearchState {
    pub query: String,
    pub position: u32,
    pub count: u32,
}

#[derive(uniffi::Record)]
pub struct DownloadView {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub failed: Option<String>,
}

#[derive(uniffi::Record)]
pub struct Snapshot {
    pub revision: u64,
    pub has_document: bool,
    pub sentence: u32,
    pub word: Option<u32>,
    /// Part du mot lu déjà prononcée, de 0 à 1.
    pub word_progress: f32,
    pub section: Option<u32>,
    pub playing: bool,
    pub audible: bool,
    pub finished: bool,
    pub progress: f32,
    pub elapsed: Option<f64>,
    pub total: Option<f64>,
    pub speed: f32,
    pub volume: f32,
    pub muted: bool,
    pub voice: VoiceState,
    pub status: String,
    pub search: Option<SearchState>,
    pub bookmarked: bool,
    pub skipped: u32,
    pub download: Option<DownloadView>,
}

#[derive(uniffi::Record)]
pub struct WordView {
    pub index: u32,
    pub text: String,
    pub strong: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
    pub struck: bool,
}

#[derive(uniffi::Record)]
pub struct SentenceView {
    pub id: u32,
    pub words: Vec<WordView>,
}

#[derive(uniffi::Enum)]
pub enum BlockKindView {
    Paragraph,
    Heading { level: u8 },
    ListItem { depth: u8, ordinal: Option<u64> },
    Quote,
    Code { language: Option<String> },
    TableRow { header: bool },
}

#[derive(uniffi::Record)]
pub struct BlockView {
    pub id: u32,
    pub kind: BlockKindView,
    pub section_id: u32,
    pub display_lines: Vec<String>,
    pub sentences: Vec<SentenceView>,
}

#[derive(uniffi::Record)]
pub struct SectionView {
    pub id: u32,
    pub level: u8,
    pub title: String,
    pub first_sentence: Option<u32>,
}

#[derive(uniffi::Record)]
pub struct DocView {
    pub title: String,
    pub complete: bool,
    pub sentence_count: u32,
    pub sections: Vec<SectionView>,
    pub blocks: Vec<BlockView>,
}

#[derive(uniffi::Record)]
pub struct BookmarkView {
    pub sentence: u32,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
}

#[derive(uniffi::Record, Clone)]
pub struct SettingsView {
    pub speed: f32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
}

#[derive(uniffi::Record)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
}

#[derive(uniffi::Object)]
pub struct SillonReader {
    handle: ReaderHandle,
}

#[uniffi::export]
impl SillonReader {
    /// Le lecteur réel : configuration de l'utilisateur, voix audio.cpp.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handle: ReaderHandle::spawn(|| {
                let (config, _) = Config::load();
                Reader::new(
                    config,
                    Box::new(|config: &Config| {
                        tts::build_engine(config.engine_preference(), &config.engine_settings()).0
                    }),
                    Box::new(Player::new),
                    Instant::now(),
                )
            }),
        })
    }

    /// Lecteur muet, sans modèle ni carte son : tests et mode de test.
    #[uniffi::constructor]
    pub fn silent() -> Arc<Self> {
        Arc::new(Self {
            handle: ReaderHandle::spawn(|| {
                let mut config = Config::default();
                config.reading.autoplay = false;
                config.app.keep_voice_minutes = -1;
                Reader::new(
                    config,
                    Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                    Box::new(Player::silent),
                    Instant::now(),
                )
            }),
        })
    }

    pub fn open_text(&self, markdown: String, source: String) {
        self.handle.send(Request::OpenText { markdown, source });
    }

    pub fn open_file(&self, path: String) -> Result<(), SillonError> {
        let (reply, answer) = crossbeam_channel_bounded();
        self.handle.send(Request::OpenFile { path: path.into(), reply });
        answer
            .recv()
            .map_err(failed)?
            .map_err(failed)
    }

    /// Une image copiée, donnée par ses octets (PNG, TIFF) : son texte
    /// reconnu (option `ocr`) s'ouvre. Le presse-papiers n'est jamais relu
    /// ici : l'app a vérifié ce qu'elle passe. Bloque le temps de la
    /// reconnaissance : à appeler hors du fil principal.
    pub fn open_image(&self, bytes: Vec<u8>) -> Result<(), SillonError> {
        let markdown = sillon::source::image_bytes(&bytes).map_err(failed)?;
        if markdown.trim().is_empty() {
            return Err(failed("aucun texte reconnu dans l'image"));
        }
        self.open_text(markdown, "image".into());
        Ok(())
    }

    pub fn prepare_voice(&self) {
        self.handle.send(Request::PrepareVoice);
    }

    pub fn rest_voice(&self) {
        self.handle.send(Request::Rest);
    }

    pub fn command(&self, command: ReaderCommand) {
        self.handle.send(Request::Command(command.into()));
    }

    pub fn snapshot(&self) -> Snapshot {
        snapshot_of(self.handle.snapshot())
    }

    pub fn document(&self) -> DocView {
        doc_of(&self.handle.document())
    }

    pub fn bookmarks(&self) -> Vec<BookmarkView> {
        self.handle
            .ask(|reader| {
                // Au repos aussi : le magasin est relu pour le texte gardé.
                reader
                    .bookmarks()
                    .iter()
                    .map(|b| BookmarkView {
                        sentence: b.sentence_id as u32,
                        section_title: b.section_title.clone(),
                        snippet: b.snippet.clone(),
                        when: b.timestamp(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn settings(&self) -> SettingsView {
        self.handle
            .ask(|reader| settings_of(reader.settings()))
            .unwrap_or(SettingsView { speed: 1.0, line_spacing: 1, reading_ruler: true, keep_voice_minutes: 10 })
    }

    /// Applique sans écrire sur disque.
    pub fn apply_settings(&self, settings: SettingsView) {
        let s = settings_from(settings);
        let _ = self.handle.ask(move |reader| reader.apply_settings(s));
    }

    pub fn update_settings(&self, settings: SettingsView) -> Result<(), SillonError> {
        let s = settings_from(settings);
        self.handle
            .ask(move |reader| reader.update_settings(s).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn voices(&self) -> Vec<VoiceModel> {
        self.handle
            .ask(|reader| {
                reader
                    .voices()
                    .into_iter()
                    .map(|v| VoiceModel {
                        id: v.id,
                        name: v.name,
                        bytes: v.bytes,
                        installed: v.installed,
                        active: v.active,
                        recommended: v.recommended,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn install_voice(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.install_voice(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn use_voice(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.use_voice(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }
}

fn crossbeam_channel_bounded() -> (
    sillon::reader::handle::ReplySender,
    sillon::reader::handle::ReplyReceiver,
) {
    sillon::reader::handle::reply_channel()
}

fn settings_of(s: settings::SettingsView) -> SettingsView {
    SettingsView {
        speed: s.speed,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
    }
}

fn settings_from(s: SettingsView) -> settings::SettingsView {
    settings::SettingsView {
        speed: s.speed,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
    }
}

fn snapshot_of(s: view::Snapshot) -> Snapshot {
    Snapshot {
        revision: s.revision,
        has_document: s.has_document,
        sentence: s.sentence as u32,
        word: s.word.map(|w| w as u32),
        word_progress: s.word_progress,
        section: s.section.map(|w| w as u32),
        playing: s.playing,
        audible: s.audible,
        finished: s.finished,
        progress: s.progress,
        elapsed: s.elapsed,
        total: s.total,
        speed: s.speed,
        volume: s.volume,
        muted: s.muted,
        voice: match s.voice {
            view::VoiceState::Resting => VoiceState::Resting,
            view::VoiceState::Loading => VoiceState::Loading,
            view::VoiceState::Ready => VoiceState::Ready,
            view::VoiceState::Fallback { reason } => VoiceState::Fallback { reason },
        },
        status: s.status,
        search: s.search.map(|q| SearchState {
            query: q.query,
            position: q.position as u32,
            count: q.count as u32,
        }),
        bookmarked: s.bookmarked,
        skipped: s.skipped as u32,
        download: s.download.map(|d| DownloadView {
            id: d.id,
            received: d.received,
            total: d.total,
            failed: d.failed,
        }),
    }
}

fn doc_of(d: &view::DocView) -> DocView {
    DocView {
        title: d.title.clone(),
        complete: d.complete,
        sentence_count: d.sentence_count as u32,
        sections: d
            .sections
            .iter()
            .map(|s| SectionView {
                id: s.id as u32,
                level: s.level,
                title: s.title.clone(),
                first_sentence: s.first_sentence.map(|f| f as u32),
            })
            .collect(),
        blocks: d
            .blocks
            .iter()
            .map(|b| BlockView {
                id: b.id as u32,
                kind: match &b.kind {
                    view::BlockKindView::Paragraph => BlockKindView::Paragraph,
                    view::BlockKindView::Heading { level } => BlockKindView::Heading { level: *level },
                    view::BlockKindView::ListItem { depth, ordinal } => {
                        BlockKindView::ListItem { depth: *depth, ordinal: *ordinal }
                    }
                    view::BlockKindView::Quote => BlockKindView::Quote,
                    view::BlockKindView::Code { language } => {
                        BlockKindView::Code { language: language.clone() }
                    }
                    view::BlockKindView::TableRow { header } => BlockKindView::TableRow { header: *header },
                },
                section_id: b.section_id as u32,
                display_lines: b.display_lines.clone(),
                sentences: b
                    .sentences
                    .iter()
                    .map(|s| SentenceView {
                        id: s.id as u32,
                        words: s
                            .words
                            .iter()
                            .map(|w| WordView {
                                index: w.index as u32,
                                text: w.text.clone(),
                                strong: w.strong,
                                italic: w.italic,
                                code: w.code,
                                link: w.link,
                                struck: w.struck,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_silent_reader_opens_text_and_follows_commands() {
        let reader = SillonReader::silent();
        reader.open_text("# Titre\n\nUn. Deux.".into(), "test".into());
        let deadline = Instant::now() + Duration::from_secs(5);
        while reader.snapshot().revision != 1 {
            assert!(Instant::now() < deadline, "texte jamais publié");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(reader.document().sentence_count, 3);
        reader.command(ReaderCommand::Sentences { delta: 2 });
        while reader.snapshot().sentence != 2 {
            assert!(Instant::now() < deadline, "commande jamais appliquée");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(reader.snapshot().voice, VoiceState::Ready));
    }

    #[test]
    fn bytes_that_are_not_an_image_open_nothing() {
        let reader = SillonReader::silent();
        assert!(reader.open_image(b"pas une image".to_vec()).is_err());
        assert_eq!(reader.snapshot().revision, 0);
    }

    #[test]
    fn settings_round_trip_without_touching_disk() {
        let reader = SillonReader::silent();
        let mut s = reader.settings();
        s.keep_voice_minutes = 5;
        reader.apply_settings(s.clone());
        assert_eq!(reader.settings().keep_voice_minutes, 5);
    }
}
