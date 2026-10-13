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
use sillon::reader::{data, preview, settings, view, Reader};
use sillon::tts::kokoro::{self, KokoroBackend, KokoroRaw};
use sillon::tts::{self, silent::SilentEngine, TtsEngine};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SillonError {
    #[error("{message}")]
    Failed { message: String },
    /// Une synthèse Kokoro abandonnée en route (`SynthesisCancel`) : ni un
    /// échec ni un résultat.
    #[error("synthèse abandonnée")]
    Cancelled,
}

fn failed(e: impl ToString) -> SillonError {
    SillonError::Failed { message: e.to_string() }
}

/// Une erreur inattendue côté Swift (exception non prévue) devient un échec
/// ordinaire : UniFFI l'exige des erreurs d'une interface étrangère.
impl From<uniffi::UnexpectedUniFFICallbackError> for SillonError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        SillonError::Failed { message: e.reason }
    }
}

/// Le calcul de Kokoro, fourni par l'app (MLX). Le moteur Rust garde
/// phonétisation, découpage, voix, minutage, Sonic et lecteur.
///
/// Contrat pour l'implémentation étrangère :
/// - les appels arrivent de fils d'arrière-plan du moteur (synthèse,
///   chargement), jamais du fil principal, et bloquent jusqu'au résultat ;
/// - ils peuvent être **simultanés** : un ancien moteur finit une synthèse
///   pendant qu'un nouveau se charge et se prépare, et `SharedCompute` ne
///   sérialise ni `synthesize` ni `warm_up`. L'implémentation sérialise
///   elle-même son calcul (un verrou ou une file) ;
/// - elle n'attend jamais le fil principal (ni `DispatchQueue.main.sync`, ni
///   `MainActor`) : il peut lui-même attendre le moteur ;
/// - `load` et `release` vont par paires, éventuellement de plusieurs
///   détenteurs (la préparation des noyaux de l'app en est un) : les poids
///   ne partent qu'au dernier `release` ;
/// - `synthesize` consulte `cancel` avant le passage et entre les durées et
///   le décodeur, et aux points d'arrêt du décodeur ; s'il dit vrai, elle
///   s'arrête et rend `SillonError::Cancelled` (jamais `Failed`). Elle ne
///   garde jamais `cancel` au-delà de l'appel (il retient le pool).
#[uniffi::export(with_foreign)]
pub trait KokoroCompute: Send + Sync {
    fn load(&self, model_dir: String) -> Result<(), SillonError>;
    fn warm_up(&self) -> Result<(), SillonError>;
    fn synthesize(
        &self,
        ids: Vec<i64>,
        style: Vec<f32>,
        cancel: Arc<SynthesisCancel>,
    ) -> Result<KokoroOutput, SillonError>;
    fn release(&self);
}

/// Le test d'abandon d'une synthèse, passé au calcul de l'app : vrai quand
/// le pool n'a plus l'usage du résultat (curseur parti, génération
/// changée). Chaque appel prend un instant le verrou du pool : à consulter
/// à un point de synchronisation, pas en boucle.
#[derive(uniffi::Object)]
pub struct SynthesisCancel(tts::Cancel);

#[uniffi::export]
impl SynthesisCancel {
    /// Une réponse fixe : pour les essais et la préparation.
    #[uniffi::constructor]
    pub fn fixed(cancelled: bool) -> Arc<Self> {
        Arc::new(Self(if cancelled { tts::Cancel::when(|| true) } else { tts::Cancel::never() }))
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[derive(uniffi::Record)]
pub struct KokoroOutput {
    pub samples: Vec<f32>,
    pub durations: Vec<u32>,
}

/// `KokoroCompute` vu par le moteur.
struct ForeignKokoro(Arc<dyn KokoroCompute>);

impl KokoroBackend for ForeignKokoro {
    fn label(&self) -> &'static str {
        "MLX"
    }
    fn load(&self, model_dir: &std::path::Path) -> anyhow::Result<()> {
        self.0.load(model_dir.to_string_lossy().into_owned()).map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    fn warm_up(&self) -> anyhow::Result<()> {
        self.0.warm_up().map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    fn infer(&self, ids: &[i64], style: &[f32], cancel: &tts::Cancel) -> anyhow::Result<KokoroRaw> {
        let cancel = Arc::new(SynthesisCancel(cancel.clone()));
        let out = self.0.synthesize(ids.to_vec(), style.to_vec(), cancel).map_err(|e| match e {
            SillonError::Cancelled => anyhow::Error::new(tts::Cancelled),
            e => anyhow::anyhow!(e.to_string()),
        })?;
        Ok(KokoroRaw { samples: out.samples, durations: out.durations })
    }
    fn release(&self) {
        self.0.release();
    }
}

/// Vérifie le pont vers un calcul Kokoro étranger : chargement, trois
/// jetons, libération, sortie contrôlée. Rend le nombre d'échantillons.
#[uniffi::export]
pub fn kokoro_bridge_check(compute: Arc<dyn KokoroCompute>, model_dir: String) -> Result<u64, SillonError> {
    let backend = ForeignKokoro(compute);
    backend.load(std::path::Path::new(&model_dir)).map_err(failed)?;
    let raw = backend.infer(&[0, 43, 0], &[0.0; 256], &tts::Cancel::never()).map_err(failed);
    backend.release();
    let raw = raw?;
    kokoro::check(&raw, 3).map_err(failed)?;
    Ok(raw.samples.len() as u64)
}

/// Le lecteur de l'app sur la configuration lue (`Config::load`). Un
/// `config.toml` illisible est retenu : la migration ne l'écrasera pas.
/// `mlx` : le calcul Kokoro de l'app, enveloppé **une fois** — chaque moteur
/// reçoit ce même `Arc` (voir `SharedCompute`).
fn app_reader(loaded: (Config, Option<String>), mlx: Option<Arc<kokoro::SharedCompute>>) -> Reader {
    let (config, unreadable) = loaded;
    let available = mlx.is_some();
    let mut reader = Reader::new(
        config,
        Box::new(move |config: &Config| {
            tts::build_engine_with(config.engine_preference(), &config.engine_settings(), mlx.clone()).0
        }),
        Box::new(Player::new),
        Instant::now(),
    );
    reader.set_mlx_compute(available);
    reader.set_config_unreadable(unreadable.is_some());
    reader
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
    SeekToWord { sentence: u32, word: u32 },
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
            ReaderCommand::SeekToWord { sentence, word } => {
                Command::SeekToWord { sentence: sentence as usize, word: word as usize }
            }
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
    /// Voix chargée mais muette : aucune session de synthèse ne s'ouvre.
    Unavailable { reason: String },
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
pub struct PreviewView {
    pub pending: bool,
    pub playing: bool,
    pub error: Option<String>,
}

#[derive(uniffi::Record, Clone, PartialEq, Debug)]
pub struct LanguageVoice {
    pub language: String,
    pub voice: String,
}

#[derive(uniffi::Record)]
pub struct DocumentLanguageView {
    pub detected: Option<String>,
    pub chosen: Option<String>,
    pub effective: String,
    pub minority_share: f32,
    /// Plus de 30 % des lettres dans l'autre langue.
    pub mixed: bool,
}

/// La proposition de passer à Kokoro, en attente d'une réponse.
#[derive(uniffi::Record)]
pub struct VoiceOfferView {
    /// Le paquet proposé.
    pub id: String,
    /// Ce qu'il pèse à télécharger (l'app l'écrit avec `ByteCountFormatter`).
    pub bytes: u64,
    /// La voix qu'il remplacerait.
    pub former: String,
    /// Le message du moteur (taille écrite à sa façon).
    pub message: String,
}

#[derive(uniffi::Record)]
pub struct Measures {
    pub busy_seconds: f64,
    pub stalls: u32,
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
    /// Nom du moteur qui parle (« Kokoro · MLX »), vide au repos.
    pub engine: String,
    pub status: String,
    pub search: Option<SearchState>,
    pub bookmarked: bool,
    pub skipped: u32,
    pub download: Option<DownloadView>,
    pub preview: PreviewView,
    pub document_language: Option<DocumentLanguageView>,
    /// L'avis du passage à Kokoro, jusqu'à `dismiss_notice`.
    pub notice: Option<String>,
    /// La proposition de passer à Kokoro, jusqu'à une réponse.
    pub offer: Option<VoiceOfferView>,
    /// `config.toml` n'a pu être lu : les réglages s'appliquent sans s'écrire
    /// jusqu'à `replace_unreadable_config`.
    pub config_unreadable: bool,
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
pub struct ExtraOption {
    pub key: String,
    pub value: String,
}

#[derive(uniffi::Record, Clone)]
pub struct SettingsView {
    pub voice: String,
    pub language: String,
    pub speed: f32,
    pub pitch: f32,
    pub energy: f32,
    pub emotion: String,
    pub instruction: String,
    pub volume: f32,
    pub engine: String,
    pub model_path: String,
    pub family: String,
    pub compute: String,
    pub threads: u32,
    pub sessions: u32,
    pub prefetch: u32,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub inference_steps: Option<u32>,
    pub seed: Option<u64>,
    pub code_blocks: String,
    pub autoplay: bool,
    pub seek_step: u32,
    pub line_width: u32,
    pub line_spacing: u32,
    pub reading_ruler: bool,
    pub keep_voice_minutes: i64,
    pub extra: Vec<ExtraOption>,
    pub voices_by_language: Vec<LanguageVoice>,
}

/// Un réglage d'échantillonnage : la valeur du modèle, ou une valeur posée.
#[derive(uniffi::Enum, Clone)]
pub enum SamplingFloat {
    Model,
    Value { value: f32 },
}

#[derive(uniffi::Enum, Clone)]
pub enum SamplingInt {
    Model,
    Value { value: u32 },
}

/// La graine : un tirage au hasard, ou une valeur fixe.
#[derive(uniffi::Enum, Clone)]
pub enum SamplingSeed {
    Random,
    Value { value: u64 },
}

/// Une modification : `None` pour un réglage non touché.
#[derive(uniffi::Record, Clone, Default)]
pub struct SettingsPatch {
    pub voice: Option<String>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub energy: Option<f32>,
    pub emotion: Option<String>,
    pub instruction: Option<String>,
    pub volume: Option<f32>,
    pub engine: Option<String>,
    pub compute: Option<String>,
    pub threads: Option<u32>,
    pub sessions: Option<u32>,
    pub prefetch: Option<u32>,
    pub temperature: Option<SamplingFloat>,
    pub top_p: Option<SamplingFloat>,
    pub top_k: Option<SamplingInt>,
    pub guidance_scale: Option<SamplingFloat>,
    pub repetition_penalty: Option<SamplingFloat>,
    pub inference_steps: Option<SamplingInt>,
    pub seed: Option<SamplingSeed>,
    pub code_blocks: Option<String>,
    pub autoplay: Option<bool>,
    pub seek_step: Option<u32>,
    pub line_width: Option<u32>,
    pub line_spacing: Option<u32>,
    pub reading_ruler: Option<bool>,
    pub keep_voice_minutes: Option<i64>,
    pub extra_set: Vec<ExtraOption>,
    pub extra_remove: Vec<String>,
    /// La voix d'une langue (Kokoro : une voix par langue).
    pub voice_for_language: Option<LanguageVoice>,
}

#[derive(uniffi::Record)]
pub struct VoiceChoice {
    pub id: String,
    pub label: String,
    /// La langue de la voix (`fr`, `en`), vide si elle les parle toutes.
    pub language: String,
}

#[derive(uniffi::Record)]
pub struct ModelCapabilities {
    pub family: String,
    pub voices: Vec<VoiceChoice>,
    pub languages: Vec<String>,
    pub honoured: Vec<String>,
    pub requires_reference: bool,
}

#[derive(uniffi::Record)]
pub struct VoiceModel {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub recommended: bool,
    pub summary: String,
    pub family: String,
    pub precision: String,
    pub languages_label: String,
    pub languages: Vec<String>,
    pub pace: String,
    pub voices_label: String,
    pub licenses: String,
}

#[derive(uniffi::Record)]
pub struct StoredBookmarkView {
    pub document: String,
    pub label: String,
    pub sentence: u32,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
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
        Arc::new(Self { handle: ReaderHandle::spawn(|| app_reader(Config::load(), None)) })
    }

    /// Le lecteur réel avec Kokoro en MLX : le calcul vient de l'app. Il est
    /// enveloppé une seule fois, et chaque moteur reconstruit le partage.
    #[uniffi::constructor]
    pub fn with_kokoro(compute: Arc<dyn KokoroCompute>) -> Arc<Self> {
        let shared = Arc::new(kokoro::SharedCompute::new(Arc::new(ForeignKokoro(compute))));
        Arc::new(Self { handle: ReaderHandle::spawn(move || app_reader(Config::load(), Some(shared))) })
    }

    /// Lecteur muet, sans modèle ni carte son : tests et mode de test.
    #[uniffi::constructor]
    pub fn silent() -> Arc<Self> {
        Arc::new(Self {
            handle: ReaderHandle::spawn(|| {
                let mut config = Config::default();
                config.reading.autoplay = false;
                config.app.keep_voice_minutes = -1;
                let mut reader = Reader::new(
                    config,
                    Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
                    Box::new(Player::silent),
                    Instant::now(),
                );
                // L'app de test ne touche pas au disque de l'utilisateur.
                reader.set_persistent(false);
                reader
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
            .unwrap_or_else(|| settings_of(settings::SettingsView::of(&Config::default())))
    }

    /// Applique sans écrire sur disque ; rend les valeurs appliquées.
    pub fn apply_patch(&self, patch: SettingsPatch) -> SettingsView {
        let patch = patch_from(patch);
        self.handle
            .ask(move |reader| settings_of(reader.apply_patch(&patch, Instant::now())))
            .unwrap_or_else(|| self.settings())
    }

    /// Applique et écrit `config.toml` ; rend les valeurs appliquées.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsView, SillonError> {
        let patch = patch_from(patch);
        self.handle
            .ask(move |reader| {
                reader
                    .update_settings(&patch, Instant::now())
                    .map(settings_of)
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn capabilities(&self) -> ModelCapabilities {
        self.handle
            .ask(|reader| {
                let c = reader.capabilities();
                ModelCapabilities {
                    family: c.family,
                    voices: c
                        .voices
                        .into_iter()
                        .map(|v| VoiceChoice { id: v.id, label: v.label, language: v.language })
                        .collect(),
                    languages: c.languages,
                    honoured: c.honoured,
                    requires_reference: c.requires_reference,
                }
            })
            .unwrap_or(ModelCapabilities {
                family: String::new(),
                voices: Vec::new(),
                languages: Vec::new(),
                honoured: Vec::new(),
                requires_reference: false,
            })
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
                        summary: v.summary,
                        family: v.family,
                        precision: v.precision,
                        languages_label: v.languages_label,
                        languages: v.languages,
                        pace: v.pace,
                        voices_label: v.voices_label,
                        licenses: v.licenses,
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
            .ask(move |reader| reader.use_voice(&id, Instant::now()).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// La phrase d'exemple avec les réglages en cours ; la raison d'un refus.
    pub fn preview_voice(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.preview_voice(Instant::now()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn stop_preview(&self) {
        let _ = self.handle.ask(|reader| reader.stop_preview());
    }

    /// La langue du document gardé (`None` : automatique).
    pub fn set_document_language(&self, language: Option<String>) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| {
                reader
                    .set_document_language(language.as_deref(), Instant::now())
                    .map_err(|e| e.to_string())
            })
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// La phrase d'exemple dans une langue : chaque voix dans la sienne.
    pub fn preview_voice_in(&self, language: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.preview_voice_in(&language, Instant::now()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// Le passage unique à Kokoro, au lancement de l'app ; l'avis à montrer
    /// (passage fait, ou proposition, alors aussi dans `Snapshot.offer`).
    pub fn migrate_voice(&self) -> Option<String> {
        self.handle.ask(|reader| reader.migrate_voice(Instant::now())).flatten()
    }

    /// « Passer à Kokoro » : le téléchargement démarre (ou Kokoro devient
    /// actif tout de suite s'il est déjà installé).
    pub fn accept_voice_offer(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.accept_offer(Instant::now()).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// « Plus tard » : le refus est retenu.
    pub fn dismiss_voice_offer(&self) {
        let _ = self.handle.ask(|reader| reader.dismiss_offer());
    }

    /// « Remplacer » un `config.toml` illisible : une copie est gardée à
    /// côté (`config.toml.illisible`), puis les réglages en cours sont écrits.
    pub fn replace_unreadable_config(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.replace_unreadable_config().map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn dismiss_notice(&self) {
        let _ = self.handle.ask(|reader| reader.dismiss_notice());
    }

    /// Temps passé à synthétiser et coupures du document ouvert : la mesure
    /// du filtre de charge.
    pub fn measures(&self) -> Measures {
        self.handle
            .ask(|reader| {
                reader.session().map(|app| Measures {
                    busy_seconds: app.synth.stats().busy.as_secs_f64(),
                    stalls: app.stalls() as u32,
                })
            })
            .flatten()
            .unwrap_or(Measures { busy_seconds: 0.0, stalls: 0 })
    }

    pub fn cancel_download(&self) -> Result<(), SillonError> {
        self.handle
            .ask(|reader| reader.cancel_download().map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn remove_model(&self, id: String) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| reader.remove_model(&id).map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    /// Espace libre du volume des modèles. Ne passe pas par le fil du lecteur.
    pub fn free_space(&self) -> Option<u64> {
        sillon::models::free_space()
    }

    /// Le dossier de `config.toml`. Ne passe pas par le fil du lecteur.
    pub fn config_dir(&self) -> Option<String> {
        sillon::paths::config_dir().map(|d| d.to_string_lossy().into_owned())
    }

    /// Vide le cache des documents ; rend les octets libérés.
    pub fn clear_document_cache(&self) -> Result<u64, SillonError> {
        self.handle
            .ask(|reader| reader.clear_document_cache().map_err(|e| e.to_string()))
            .ok_or_else(|| failed("lecteur indisponible"))?
            .map_err(failed)
    }

    pub fn all_bookmarks(&self) -> Vec<StoredBookmarkView> {
        self.handle
            .ask(|reader| reader.all_bookmarks().into_iter().map(stored_bookmark_of).collect())
            .unwrap_or_default()
    }

    pub fn remove_bookmark(&self, document: String, sentence: u32) -> Result<(), SillonError> {
        self.handle
            .ask(move |reader| {
                reader
                    .remove_bookmark(&document, sentence as usize)
                    .map_err(|e| e.to_string())
            })
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

/// Les défauts du moteur (`Config::default`), sans passer par le fil du
/// lecteur : ce qu'un onglet montre avant son premier chargement.
#[uniffi::export]
pub fn default_settings() -> SettingsView {
    settings_of(settings::SettingsView::of(&Config::default()))
}

fn settings_of(s: settings::SettingsView) -> SettingsView {
    SettingsView {
        voice: s.voice,
        language: s.language,
        speed: s.speed,
        pitch: s.pitch,
        energy: s.energy,
        emotion: s.emotion,
        instruction: s.instruction,
        volume: s.volume,
        engine: s.engine,
        model_path: s.model_path,
        family: s.family,
        compute: s.compute,
        threads: s.threads,
        sessions: s.sessions,
        prefetch: s.prefetch,
        temperature: s.temperature,
        top_p: s.top_p,
        top_k: s.top_k,
        guidance_scale: s.guidance_scale,
        repetition_penalty: s.repetition_penalty,
        inference_steps: s.inference_steps,
        seed: s.seed,
        code_blocks: s.code_blocks,
        autoplay: s.autoplay,
        seek_step: s.seek_step,
        line_width: s.line_width,
        line_spacing: s.line_spacing,
        reading_ruler: s.reading_ruler,
        keep_voice_minutes: s.keep_voice_minutes,
        extra: s
            .extra
            .into_iter()
            .map(|(key, value)| ExtraOption { key, value })
            .collect(),
        voices_by_language: s
            .voices_by_language
            .into_iter()
            .map(|(language, voice)| LanguageVoice { language, voice })
            .collect(),
    }
}

fn float(v: Option<SamplingFloat>) -> Option<Option<f32>> {
    v.map(|s| match s {
        SamplingFloat::Model => None,
        SamplingFloat::Value { value } => Some(value),
    })
}

fn int(v: Option<SamplingInt>) -> Option<Option<u32>> {
    v.map(|s| match s {
        SamplingInt::Model => None,
        SamplingInt::Value { value } => Some(value),
    })
}

fn patch_from(p: SettingsPatch) -> settings::SettingsPatch {
    settings::SettingsPatch {
        voice: p.voice,
        language: p.language,
        speed: p.speed,
        pitch: p.pitch,
        energy: p.energy,
        emotion: p.emotion,
        instruction: p.instruction,
        volume: p.volume,
        engine: p.engine,
        compute: p.compute,
        threads: p.threads,
        sessions: p.sessions,
        prefetch: p.prefetch,
        temperature: float(p.temperature),
        top_p: float(p.top_p),
        top_k: int(p.top_k),
        guidance_scale: float(p.guidance_scale),
        repetition_penalty: float(p.repetition_penalty),
        inference_steps: int(p.inference_steps),
        seed: p.seed.map(|s| match s {
            SamplingSeed::Random => None,
            SamplingSeed::Value { value } => Some(value),
        }),
        code_blocks: p.code_blocks,
        autoplay: p.autoplay,
        seek_step: p.seek_step,
        line_width: p.line_width,
        line_spacing: p.line_spacing,
        reading_ruler: p.reading_ruler,
        keep_voice_minutes: p.keep_voice_minutes,
        extra_set: p.extra_set.into_iter().map(|o| (o.key, o.value)).collect(),
        extra_remove: p.extra_remove,
        voice_for_language: p.voice_for_language.map(|v| (v.language, v.voice)),
    }
}

fn preview_of(p: preview::PreviewView) -> PreviewView {
    PreviewView { pending: p.pending, playing: p.playing, error: p.error }
}

fn stored_bookmark_of(b: data::StoredBookmark) -> StoredBookmarkView {
    StoredBookmarkView {
        document: b.document,
        label: b.label,
        sentence: b.sentence as u32,
        section_title: b.section_title,
        snippet: b.snippet,
        when: b.when,
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
            view::VoiceState::Unavailable { reason } => VoiceState::Unavailable { reason },
        },
        engine: s.engine,
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
        preview: preview_of(s.preview),
        document_language: s.document_language.map(|l| DocumentLanguageView {
            mixed: l.mixed(),
            detected: l.detected,
            chosen: l.chosen,
            effective: l.effective,
            minority_share: l.minority_share,
        }),
        notice: s.notice,
        offer: s.offer.map(|o| VoiceOfferView { id: o.id, bytes: o.bytes, former: o.former, message: o.message }),
        config_unreadable: s.config_unreadable,
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

    /// Le nom que l'outil de mesure exige (`Measure.moteurKokoroMLX`) :
    /// celui du moteur Kokoro sur le calcul de l'app, enveloppé comme dans
    /// `SillonReader::with_kokoro`. Qu'il change, et la mesure refuserait
    /// toute voix : ce test le dit d'abord.
    #[test]
    fn the_kokoro_engine_on_the_app_compute_is_named_kokoro_mlx() {
        struct NoCompute;
        impl KokoroCompute for NoCompute {
            fn load(&self, _: String) -> Result<(), SillonError> {
                Ok(())
            }
            fn warm_up(&self) -> Result<(), SillonError> {
                Ok(())
            }
            fn synthesize(&self, _: Vec<i64>, _: Vec<f32>, _: Arc<SynthesisCancel>) -> Result<KokoroOutput, SillonError> {
                Ok(KokoroOutput { samples: Vec::new(), durations: Vec::new() })
            }
            fn release(&self) {}
        }
        struct NoPhonemes;
        impl kokoro::phonemes::Phonemizer for NoPhonemes {
            fn phonemize(&mut self, _: &str, items: &[String]) -> anyhow::Result<Vec<String>> {
                Ok(vec![String::new(); items.len()])
            }
        }
        let shared = Arc::new(kokoro::SharedCompute::new(Arc::new(ForeignKokoro(Arc::new(NoCompute)))));
        let engine = kokoro::KokoroEngine::with_parts(
            shared,
            Box::new(NoPhonemes),
            kokoro::Vocab::from_pairs([]),
            std::path::PathBuf::new(),
            Arc::new(SilentEngine),
        );
        assert_eq!(engine.name(), "Kokoro · MLX");
    }

    #[test]
    fn seek_to_word_crosses_the_bridge_as_an_absolute_word() {
        let command: Command = ReaderCommand::SeekToWord { sentence: 2, word: 5 }.into();
        assert_eq!(command, Command::SeekToWord { sentence: 2, word: 5 });
    }

    #[test]
    fn bytes_that_are_not_an_image_open_nothing() {
        let reader = SillonReader::silent();
        assert!(reader.open_image(b"pas une image".to_vec()).is_err());
        assert_eq!(reader.snapshot().revision, 0);
    }

    #[test]
    fn settings_round_trip_through_a_patch_without_touching_disk() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.keep_voice_minutes = Some(5);
        patch.line_spacing = Some(9);
        patch.temperature = Some(SamplingFloat::Value { value: 0.7 });
        patch.extra_set = vec![ExtraOption { key: "cfg_scale".into(), value: "3".into() }];
        let applied = reader.apply_patch(patch);
        assert_eq!(applied.keep_voice_minutes, 5);
        assert_eq!(applied.line_spacing, 3, "ramené dans les bornes");
        assert_eq!(applied.temperature, Some(0.7));
        assert_eq!(applied.extra.len(), 1);
        let mut back = SettingsPatch::default();
        back.temperature = Some(SamplingFloat::Model);
        back.extra_remove = vec!["cfg_scale".into()];
        let applied = reader.apply_patch(back);
        assert_eq!(applied.temperature, None);
        assert!(applied.extra.is_empty());
        assert_eq!(reader.settings().keep_voice_minutes, 5);
    }

    #[test]
    fn the_silent_reader_writes_and_erases_nothing() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.seek_step = Some(7);
        assert_eq!(reader.update_settings(patch).expect("rien à écrire").seek_step, 7);
        let err = reader.clear_document_cache().unwrap_err().to_string();
        assert!(err.contains("mode d'essai"), "{err}");
        assert!(reader.remove_bookmark("x".into(), 0).is_err());
    }

    #[test]
    fn capabilities_and_preview_answer_without_a_model() {
        let reader = SillonReader::silent();
        let caps = reader.capabilities();
        assert!(caps.voices.is_empty());
        assert_eq!(caps.languages.len(), 7);
        let err = reader.preview_voice().unwrap_err().to_string();
        assert!(err.contains("sortie audio"), "{err}");
        reader.stop_preview();
        assert!(!reader.snapshot().preview.pending);
    }

    #[test]
    fn the_defaults_come_from_the_engine_configuration() {
        let d = default_settings();
        let c = Config::default();
        assert_eq!(d.threads, c.engine.threads as u32);
        assert_eq!(d.speed, settings::SettingsView::of(&c).speed);
        assert_eq!(d.line_spacing, settings::SettingsView::of(&c).line_spacing);
        assert!(d.temperature.is_none() && d.extra.is_empty());
    }

    /// Un calcul « étranger » écrit en Rust : deux trames par jeton.
    struct FakeCompute {
        fail: bool,
    }

    impl KokoroCompute for FakeCompute {
        fn load(&self, _model_dir: String) -> Result<(), SillonError> {
            Ok(())
        }
        fn warm_up(&self) -> Result<(), SillonError> {
            Ok(())
        }
        fn synthesize(
            &self,
            ids: Vec<i64>,
            _style: Vec<f32>,
            cancel: Arc<SynthesisCancel>,
        ) -> Result<KokoroOutput, SillonError> {
            if cancel.is_cancelled() {
                return Err(SillonError::Cancelled);
            }
            if self.fail {
                return Err(SillonError::Failed { message: "Calcul Metal indisponible".into() });
            }
            Ok(KokoroOutput { samples: vec![0.0; ids.len() * 2 * 600], durations: vec![2; ids.len()] })
        }
        fn release(&self) {}
    }

    /// Le test d'abandon du pool traverse le pont, et l'abandon revient
    /// comme tel : ni un échec, ni un résultat.
    #[test]
    fn an_abandon_crosses_the_bridge_both_ways() {
        let backend = ForeignKokoro(Arc::new(FakeCompute { fail: false }));
        let err = backend.infer(&[0, 43, 0], &[0.0; 256], &tts::Cancel::when(|| true)).unwrap_err();
        assert!(tts::is_cancelled(&err), "{err}");
        assert!(backend.infer(&[0, 43, 0], &[0.0; 256], &tts::Cancel::never()).is_ok());
        assert!(SynthesisCancel::fixed(true).is_cancelled());
        assert!(!SynthesisCancel::fixed(false).is_cancelled());
    }

    #[test]
    fn a_foreign_compute_crosses_the_bridge() {
        assert_eq!(kokoro_bridge_check(Arc::new(FakeCompute { fail: false }), "/x".into()).unwrap(), 3 * 2 * 600);
        let err = kokoro_bridge_check(Arc::new(FakeCompute { fail: true }), "/x".into()).unwrap_err().to_string();
        assert!(err.contains("Calcul Metal indisponible"), "{err}");
    }

    #[test]
    fn the_language_of_the_document_reaches_the_snapshot() {
        let reader = SillonReader::silent();
        reader.open_text(
            "Monads let you chain computations that carry a context, without handling it by hand at every step.".into(),
            "test".into(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while reader.snapshot().document_language.is_none() {
            assert!(Instant::now() < deadline, "langue jamais publiée");
            std::thread::sleep(Duration::from_millis(5));
        }
        let lang = reader.snapshot().document_language.unwrap();
        assert_eq!(lang.effective, "en");
        assert!(!lang.mixed);
        reader.set_document_language(Some("fr".into())).unwrap();
        // L'instantané se republie au tour suivant du fil du lecteur.
        while reader.snapshot().document_language.and_then(|l| l.chosen).as_deref() != Some("fr") {
            assert!(Instant::now() < deadline, "choix jamais publié");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(reader.set_document_language(Some("de".into())).is_err());
    }

    #[test]
    fn a_voice_per_language_round_trips_and_the_silent_reader_never_migrates() {
        let reader = SillonReader::silent();
        let mut patch = SettingsPatch::default();
        patch.voice_for_language = Some(LanguageVoice { language: "en".into(), voice: "af_sky".into() });
        let applied = reader.apply_patch(patch);
        assert_eq!(applied.voices_by_language, [LanguageVoice { language: "en".into(), voice: "af_sky".into() }]);
        assert_eq!(reader.migrate_voice(), None, "hors disque : rien ne bouge");
        assert_eq!(reader.measures().stalls, 0);
    }

    #[test]
    fn the_offer_answers_cross_the_bridge() {
        let reader = SillonReader::silent();
        assert!(reader.snapshot().offer.is_none());
        let err = reader.accept_voice_offer().unwrap_err().to_string();
        assert!(err.contains("aucune proposition"), "{err}");
        reader.dismiss_voice_offer();
        reader.dismiss_notice();
        assert!(reader.snapshot().notice.is_none());
    }

    /// Un `config.toml` illisible : l'app tourne sur les défauts, et le
    /// lecteur le sait (la migration ne réécrira pas le fichier). Aucun
    /// disque touché : le lecteur est seulement construit.
    #[test]
    fn an_unreadable_config_reaches_the_reader() {
        let damaged = app_reader((Config::default(), Some("configuration ignorée".into())), None);
        assert!(damaged.config_unreadable());
        let fine = app_reader((Config::default(), None), None);
        assert!(!fine.config_unreadable());
    }

    /// L'app le montre (« Configuration illisible ») : l'instantané le porte.
    /// Le lecteur muet n'écrit rien : « Remplacer » y est refusé.
    #[test]
    fn an_unreadable_config_reaches_the_snapshot() {
        let reader = SillonReader::silent();
        assert!(!reader.snapshot().config_unreadable);
        let err = reader.replace_unreadable_config().unwrap_err().to_string();
        assert!(err.contains("mode d'essai"), "{err}");
    }
}
