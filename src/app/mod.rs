//! Le contrôleur : état de l'application et commandes du lecteur.

pub mod config;
pub mod keys;
mod navigation;
pub mod pace;
mod playback;
pub mod search;
mod settings;
pub mod startup;
pub mod view;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::Player;
use crate::position::PositionStore;
use crate::config::Config;
use crate::doc::{Document, DocumentBuilder};
use crate::doc::blocks::BlockAssembler;
use crate::input::InputEvent;
use crate::models;
use crate::tts::preview::{self, Previewer};
use crate::tts::{self, Synthesizer, TtsEngine, TtsParams};
use crate::app::config::{ConfigScreen, Field};
use crate::config::Effect;
use crate::app::pace::Pace;
use crate::app::search::Search;
use crate::app::view::ViewAnchor;

/// Panneau latéral affiché à droite du texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Contents,
}

/// Réglages d'affichage du texte, tels que la page des réglages les fixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingStyle {
    /// Lignes vides insérées entre deux lignes de texte.
    pub spacing: usize,
    /// Souligne d'un fond la ligne où se trouve le mot prononcé.
    pub ruler: bool,
}

/// Début du statut d'une voix dont aucune session ne s'ouvre.
const VOICE_UNAVAILABLE: &str = "Voix indisponible : ";

pub struct App {
    builder: DocumentBuilder,
    input: Option<Receiver<InputEvent>>,
    assembler: BlockAssembler,
    /// Nombre de phrases déjà transmises à la synthèse.
    published: usize,
    input_done: bool,

    pub synth: Synthesizer,
    /// Aperçus de voix, synthétisés hors de l'ordonnanceur de lecture.
    previewer: Previewer,
    /// Dernier échec d'aperçu, pour l'app (le terminal le lit dans `screen`).
    pub preview_error: Option<String>,
    pub player: Player,
    /// Positions retenues d'une session à l'autre.
    pub positions: PositionStore,
    /// Vrai quand ce document a une identité stable, donc une position qui
    /// veut encore dire quelque chose à la prochaine ouverture.
    remembers_place: bool,
    pub config: Config,
    pub source: String,

    /// Prochaine phrase à empiler dans la sortie audio.
    queue_head: usize,
    /// Dernière phrase connue comme étant en cours de lecture.
    last_known: usize,
    /// Phrases que le moteur a refusées et que la lecture a passées.
    ///
    /// Un ensemble, non un compteur : repasser sur la même phrase ne fait pas
    /// deux phrases muettes, et l'annonce doit rester vraie.
    skipped: HashSet<usize>,
    /// Cadence mesurée sur ce qui a été synthétisé — à 1× —, d'où sortent les
    /// temps affichés, divisés par la vitesse de lecture.
    pace: Pace,
    /// Saut au mot en attente de son audio : la phrase, le mot, et la part
    /// de la parole à passer. Tombe dès que la phrase entre en file.
    start_at: Option<StartAt>,
    /// Minutage des mots des phrases en file, par phrase.
    timings: std::collections::HashMap<usize, Arc<[crate::doc::WordSpan]>>,
    awaiting_tail: Option<AwaitingTail>,
    /// Phrases coupées en file, par phrase.
    split_parts: std::collections::HashMap<usize, SplitParts>,
    /// Coupures : la sortie s'est vidée en pleine lecture faute de son prêt.
    stalls: usize,
    /// La sortie est déjà vide : une coupure ne compte qu'une fois.
    starved: bool,

    pub panel: Panel,
    pub selection: usize,
    pub show_help: bool,
    /// L'écran de configuration remplace la vue de lecture quand il est ouvert.
    pub show_config: bool,
    pub screen: ConfigScreen,
    /// Position de la vue, qui peut se détacher de la lecture.
    pub view: ViewAnchor,
    /// Recherche en cours de saisie, avant validation.
    search_input: Option<String>,
    /// Résultats de la dernière recherche validée.
    search: Option<Search>,
    pub should_quit: bool,

    status: String,
    status_until: Option<Instant>,
    /// Dernière erreur de la sortie audio déjà dite : une sortie qui ne se
    /// rouvre pas au réveil le dit une fois, pas à chaque tour.
    output_error: Option<String>,
    pub notices: Vec<String>,

    /// Lancement du lecteur, origine du chrono de démarrage.
    started_at: Instant,
    /// Vrai dès la première phrase produite : le démarrage ne s'annonce qu'une
    /// fois, même quand un changement de voix vide le cache ensuite.
    synthesis_started: bool,

    /// Où l'enregistrement automatique écrit (`config.toml` ; un fichier
    /// temporaire sous test). `None` : rien ne s'écrit.
    config_target: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StartAt {
    sentence: usize,
    word: usize,
    ratio: f32,
    /// Le décalage a servi à l'entrée en file : il ne doit pas servir deux
    /// fois, mais le relais d'affichage reste jusqu'à une position réelle.
    consumed: bool,
}

/// La tête d'une phrase coupée est partie (ou a été passée) : sa suite est
/// attendue avant toute autre phrase.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AwaitingTail {
    sentence: usize,
    /// Parole de la tête : là où la suite reprend le chronomètre.
    base: Duration,
    head_words: usize,
}

/// Les deux parties d'une phrase coupée, telles qu'elles sont en file : le
/// surlignage suit la partie qui sonne, avec ses mots et son minutage.
/// Retenues jusqu'à la fin de la phrase, pas seulement pendant l'attente de
/// la suite : la tête sonne souvent encore quand la suite entre en file.
#[derive(Debug, Clone)]
struct SplitParts {
    head_words: usize,
    /// Parole de la tête, qui est aussi le départ de la suite.
    head_speech: Duration,
    head_spans: Option<Arc<[crate::doc::WordSpan]>>,
    /// Minutage de la suite, depuis son propre début.
    tail_spans: Option<Arc<[crate::doc::WordSpan]>>,
}

/// Le relais d'un saut peut tomber : le lecteur donne enfin une position pour
/// la phrase visée, et l'offset a déjà été appliqué.
fn relay_done(start: &StartAt, playing_sentence: Option<usize>) -> bool {
    start.consumed && playing_sentence == Some(start.sentence)
}

/// Où commencer une phrase après un saut au mot : au début mesuré du mot si
/// le minutage existe, sinon à sa part de la parole selon les poids.
fn start_offset(start: &StartAt, rendered: &crate::tts::pool::Rendered) -> Duration {
    match rendered.words.as_deref().and_then(|spans| spans.get(start.word)) {
        Some(span) => Duration::from_secs_f32(span.start.max(0.0)),
        None => rendered.speech.mul_f32(start.ratio),
    }
}

impl App {
    pub fn new(
        builder: DocumentBuilder,
        input: Option<Receiver<InputEvent>>,
        engine: Arc<dyn TtsEngine>,
        config: Config,
        source: String,
        notices: Vec<String>,
        player: Player,
    ) -> Self {
        let synth = Synthesizer::new(
            engine,
            config.tts_params_in(builder.language()),
            config.sessions() as usize,
            config.engine.prefetch as usize,
        );
        Self::from_parts(builder, input, synth, config, source, notices, player)
    }

    /// Un lecteur bâti sur une synthèse et une sortie déjà ouvertes.
    ///
    /// C'est ce qui rend un nouveau texte immédiat : ni session de synthèse
    /// ni flux audio à rouvrir.
    pub fn from_parts(
        builder: DocumentBuilder,
        input: Option<Receiver<InputEvent>>,
        synth: Synthesizer,
        config: Config,
        source: String,
        notices: Vec<String>,
        mut player: Player,
    ) -> Self {
        synth.set_params(config.tts_params_in(builder.language()));
        let previewer = Previewer::new(synth.engine());
        player.set_volume(config.voice.volume);
        player.set_speed(config.voice.speed);

        let mut notices = notices;
        let output_error = player.error().map(str::to_string);
        if let Some(err) = &output_error {
            notices.push(format!("sortie audio indisponible : {err}"));
        }

        let autoplay = config.reading.autoplay;

        let has_stream = input.is_some();
        let mut app = Self {
            builder,
            input,
            assembler: BlockAssembler::new(),
            published: 0,
            input_done: !has_stream,
            synth,
            previewer,
            preview_error: None,
            player,
            positions: PositionStore::load(),
            remembers_place: false,
            config,
            source,
            queue_head: 0,
            last_known: 0,
            skipped: HashSet::new(),
            pace: Pace::default(),
            start_at: None,
            timings: std::collections::HashMap::new(),
            awaiting_tail: None,
            split_parts: std::collections::HashMap::new(),
            stalls: 0,
            // Rien n'a encore joué : le premier son n'est pas une reprise.
            starved: true,
            panel: Panel::None,
            selection: 0,
            show_help: false,
            show_config: false,
            screen: ConfigScreen::new(),
            view: ViewAnchor::default(),
            search_input: None,
            search: None,
            should_quit: false,
            status: String::new(),
            status_until: None,
            output_error,
            notices,
            started_at: Instant::now(),
            synthesis_started: false,
            config_target: Config::path(),
        };
        if !has_stream {
            app.builder.finish();
            app.builder.sync_word_total();
        }
        app.publish_new_sentences();
        // Une sortie reprise du texte précédent peut jouer encore : l'état se
        // fixe ici, il ne se déduit pas de ce que la sortie faisait.
        if autoplay {
            app.player.play();
        } else {
            app.player.pause();
        }
        app
    }

    /// Rend synthèse et sortie pour le texte suivant.
    ///
    /// La sortie est vidée : l'audio de ce texte ne doit pas déborder sur
    /// le suivant.
    pub fn into_parts(mut self) -> (Synthesizer, Player) {
        self.player.flush();
        (self.synth, self.player)
    }

    /// Le document lu, la session finie : synthèse et sortie partent avec
    /// le reste.
    pub fn into_document(self) -> Document {
        self.builder.into_document()
    }

    /// Nom du moteur, tel qu'il doit s'afficher à cet instant.
    pub fn engine_name(&self) -> String {
        self.synth.engine_name()
    }

    /// Étape à annoncer avant le premier son, s'il y en a une.
    ///
    /// Le chargement du modèle prend environ trois secondes, dix à froid :
    /// sans cet écho, le lecteur paraît figé au lancement.
    pub fn startup_phase(&self) -> Option<startup::Phase> {
        startup::phase(
            &self.engine_name(),
            self.synth.stats(),
            self.synthesis_started,
        )
    }

    /// Temps écoulé depuis le lancement, pour le chrono de démarrage.
    pub fn startup_elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Réglages d'affichage du texte, tels que le rendu les attend.
    pub fn reading_style(&self) -> ReadingStyle {
        ReadingStyle {
            spacing: self.config.reading.line_spacing as usize,
            ruler: self.config.reading.reading_ruler,
        }
    }

    pub fn doc(&self) -> &Document {
        self.builder.document()
    }

    /// Phrase actuellement lue, ou celle qui va l'être.
    pub fn current_sentence(&self) -> usize {
        self.player
            .position()
            .map(|p| p.sentence_id)
            .unwrap_or(self.last_known)
    }

    /// Progression dans la phrase courante, entre 0 et 1.
    pub fn sentence_progress(&self) -> f32 {
        match self.player.position() {
            Some(p) if p.speech > Duration::ZERO => {
                (p.elapsed.as_secs_f32() / p.speech.as_secs_f32()).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Index du mot prononcé, dans la phrase courante.
    pub fn current_word(&self) -> Option<usize> {
        self.current_word_fraction().map(|(word, _)| word)
    }

    /// Mot prononcé dans la phrase courante, et la part de ce mot déjà dite :
    /// d'après le minutage mesuré s'il existe, d'après les lettres sinon.
    pub fn current_word_fraction(&self) -> Option<(usize, f32)> {
        let position = self.player.position()?;
        let sentence = self.doc().sentence(position.sentence_id)?;
        let elapsed = position.elapsed.as_secs_f32();
        // Une phrase coupée : le surlignage reste dans les mots de la partie
        // qui sonne. La tête se reconnaît à sa parole, qui finit avec elle ;
        // celle de la suite compte la tête en plus.
        if let Some(parts) = self.split_parts.get(&position.sentence_id) {
            let count = sentence.words.len();
            let cut = parts.head_words.min(count);
            if position.speech <= parts.head_speech {
                let total = parts.head_speech.as_secs_f32();
                return sentence.word_fraction_range(0..cut, elapsed, total, parts.head_spans.as_deref());
            }
            let base = parts.head_speech.as_secs_f32();
            let total = position.speech.as_secs_f32() - base;
            return sentence.word_fraction_range(cut..count, elapsed - base, total, parts.tail_spans.as_deref());
        }
        if let Some(found) = self
            .timings
            .get(&position.sentence_id)
            .and_then(|spans| sentence.word_fraction_timed(elapsed, spans))
        {
            return Some(found);
        }
        sentence.word_fraction(elapsed, position.speech.as_secs_f32())
    }

    /// La part du mot lu déjà dite : zéro tant qu'aucun son ne la mesure
    /// (un saut en attente, une pause avant le premier son).
    pub fn reading_word_progress(&self) -> f32 {
        self.current_word_fraction().map_or(0.0, |(_, fraction)| fraction)
    }

    /// Empreinte de ce que l'écran montre et qui change sans touche : mot et
    /// phrase lus, chrono, avance de synthèse, messages, téléchargement.
    /// La boucle ne refait la mise en page que si elle bouge.
    pub fn frame_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.current_sentence().hash(&mut h);
        self.current_word().hash(&mut h);
        self.player.is_playing().hash(&mut h);
        self.doc().sentences.len().hash(&mut h);
        self.input_done.hash(&mut h);
        self.is_finished().hash(&mut h);
        self.skipped.len().hash(&mut h);
        self.status.hash(&mut h);
        self.screen.notice.hash(&mut h);
        self.notices.len().hash(&mut h);
        self.config.voice.speed.to_bits().hash(&mut h);
        let stats = self.synth.stats();
        (stats.ready, stats.in_flight, stats.failed, stats.workers).hash(&mut h);
        let engine = self.engine_name();
        engine.hash(&mut h);
        self.estimated_total().map(|s| s as u64).hash(&mut h);
        match startup::phase(&engine, stats, self.synthesis_started) {
            // Le sablier tourne par pas de 80 ms, le chrono par dixièmes.
            Some(phase) => (phase as u8, self.startup_elapsed().as_millis() / 80).hash(&mut h),
            None => self.estimated_elapsed().map(|s| s as u64).hash(&mut h),
        }
        self.voice_install_status().hash(&mut h);
        self.previewer.pending_for().is_some().hash(&mut h);
        h.finish()
    }

    /// Rien ne joue, rien n'arrive : la boucle peut s'éveiller moins souvent.
    pub fn is_idle(&self) -> bool {
        !self.player.is_playing()
            && self.input_done
            && self.screen.download.is_none()
            && self.previewer.pending_for().is_none()
            && self.startup_phase().is_none()
    }

    // ---------------------------------------------------------------- cycle

}

/// Phrase et mot atteints en avançant de `delta` mots depuis
/// (`sentence`, `word`). `None` au-delà du document.
pub(crate) fn word_target(
    doc: &Document,
    sentence: usize,
    word: usize,
    delta: isize,
) -> Option<(usize, usize)> {
    let mut s = sentence;
    let mut w = word as isize + delta;
    loop {
        let len = doc.sentence(s)?.words.len() as isize;
        if w < 0 {
            s = s.checked_sub(1)?;
            w += doc.sentence(s)?.words.len() as isize;
        } else if w >= len {
            w -= len;
            s += 1;
        } else {
            return Some((s, w as usize));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sync::LockExt;
    use crate::doc::builder::CodePolicy;
    use crate::tts::{PcmBuffer, TtsSession};
    use anyhow::Result;

    /// Moteur muet : la lecture n'est pas ce qu'on éprouve ici.
    struct SilentEngine;
    struct SilentSession;

    impl TtsEngine for SilentEngine {
        fn name(&self) -> String {
            "silencieux".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(SilentSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for SilentSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.0; 100], 1000, 1))
        }
    }

    #[test]
    fn parts_carry_over_to_a_new_text() {
        let app = app_reading("Premier texte.");
        let (synth, player) = app.into_parts();
        synth.reset();
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("Second texte.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::from_parts(builder, None, synth, config, "b".into(), Vec::new(), player);
        assert_eq!(app.doc().sentences.len(), 1);
        assert_eq!(app.doc().sentences[0].written(), "Second texte.");
        assert_eq!(app.player.queue_len(), 0);
    }

    #[test]
    fn a_carried_over_player_follows_autoplay() {
        let mut app = app_reading("Premier texte.");
        app.player.play();
        assert!(app.player.is_playing());
        let (synth, player) = app.into_parts();
        synth.reset();
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("Second texte.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::from_parts(builder, None, synth, config, "b".into(), Vec::new(), player);
        assert!(!app.player.is_playing());
    }

    pub(crate) fn app_reading(markdown: &str) -> App {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::doc::blocks::split_blocks(markdown) {
            builder.push_block(&block);
        }
        let mut config = Config::default();
        config.reading.autoplay = false;
        App::new(
            builder,
            None,
            Arc::new(SilentEngine),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        )
    }

    #[test]
    fn an_unchanged_screen_keeps_its_frame_key() {
        // Le terminal ne remet le texte en page que si cette clé bouge : un
        // tour sans rien de nouveau ne doit pas la changer, un message si.
        let mut app = app_reading("Une phrase. Une autre.");
        tick_until(&mut app, |app| {
            let stats = app.synth.stats();
            stats.ready == 2 && stats.in_flight == 0
        });
        let key = app.frame_key();
        app.tick();
        assert_eq!(app.frame_key(), key);
        app.notify("message");
        assert_ne!(app.frame_key(), key);
    }

    #[test]
    fn the_synthesis_speaks_the_document_language() {
        let mut builder = DocumentBuilder::with_language(CodePolicy::Announce, "en");
        builder.push_block("The report has 37 pages.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::new(
            builder,
            None,
            Arc::new(SilentEngine),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        assert_eq!(app.synth.params().language, "en");
        assert_eq!(app.synthesis_params().language, "en");
    }

    /// Moteur qui refuse une phrase une seule fois, puis accepte tout.
    ///
    /// Reproduit l'échec passager : la phrase reste muette, et rien ne la
    /// relance tant qu'on ne revient pas dessus.
    struct FlakyEngine {
        refused: Arc<std::sync::atomic::AtomicBool>,
    }
    struct FlakySession {
        refused: Arc<std::sync::atomic::AtomicBool>,
    }

    impl TtsEngine for FlakyEngine {
        fn name(&self) -> String {
            "capricieux".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(FlakySession { refused: Arc::clone(&self.refused) }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for FlakySession {
        fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            // Le drapeau ne se consomme que sur la phrase visée : sinon la
            // première venue l'épuiserait.
            if text.contains("Deuxième")
                && !self.refused.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                anyhow::bail!("le moteur a refusé cette phrase");
            }
            Ok(PcmBuffer::new(vec![0.0; 100], 1000, 1))
        }
    }

    /// Attend qu'une condition se réalise, sans dépendre d'un délai fixe.
    fn wait_for(mut ready: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if ready() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Compte les synthèses : un changement de vitesse ne doit en relancer
    /// aucune.
    struct CountingEngine(Arc<std::sync::atomic::AtomicUsize>);
    struct CountingSession(Arc<std::sync::atomic::AtomicUsize>);

    impl TtsEngine for CountingEngine {
        fn name(&self) -> String {
            "compteur".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(CountingSession(Arc::clone(&self.0))))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for CountingSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(PcmBuffer::new(vec![0.0; 100], 1000, 1))
        }
    }

    #[test]
    fn a_measured_timing_wins_over_the_weights() {
        let mut app = app_reading("Un mot très long ici.");
        let spans: Arc<[crate::doc::WordSpan]> = (0..5)
            .map(|i| crate::doc::WordSpan { start: i as f32 * 0.2, end: (i + 1) as f32 * 0.2 })
            .collect();
        app.timings.insert(0, Arc::clone(&spans));
        app.player.simulate(0, Duration::from_millis(500), Duration::from_secs(1));
        assert_eq!(app.current_word_fraction().map(|(w, _)| w), Some(2));
        app.timings.clear();
        // Sans minutage, la répartition par poids reprend la main : à mi-parole,
        // « long » (poids 2, 3, 4, 4, 9 sur 22 : la cible 11 tombe dans le 4e).
        assert_eq!(app.current_word_fraction().map(|(w, _)| w), Some(3));
    }

    #[test]
    fn a_jump_to_a_word_starts_at_its_measured_time() {
        let audio = Arc::new(crate::tts::PcmBuffer::new(vec![0.0; 1000], 1000, 1));
        let spans: Arc<[crate::doc::WordSpan]> =
            vec![crate::doc::WordSpan { start: 0.0, end: 0.25 }, crate::doc::WordSpan { start: 0.25, end: 1.0 }].into();
        let rendered = crate::tts::pool::Rendered {
            audio,
            speech: Duration::from_secs(1),
            words: Some(spans),
            part: crate::tts::pool::Part::Whole,
        };
        let start = StartAt { sentence: 0, word: 1, ratio: 0.5, consumed: false };
        assert_eq!(start_offset(&start, &rendered), Duration::from_millis(250));
        let untimed = crate::tts::pool::Rendered { words: None, ..rendered };
        assert_eq!(start_offset(&start, &untimed), Duration::from_millis(500), "les poids, faute de mieux");
    }

    #[test]
    fn a_speed_change_keeps_the_synthesized_audio_and_reaches_the_player() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::doc::blocks::split_blocks(DOC) {
            builder.push_block(&block);
        }
        let mut config = Config::default();
        config.reading.autoplay = false;
        let mut app = App::new(
            builder,
            None,
            Arc::new(CountingEngine(Arc::clone(&calls))),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        // Attendre que la fenêtre soit entièrement synthétisée, pas seulement
        // trois phrases : la quatrième, encore à venir, tombait parfois dans
        // le compte d'après et passait pour une resynthèse.
        let total = app.doc().sentences.len();
        assert!(total <= app.config.engine.prefetch as usize, "tout le document tient dans la fenêtre");
        assert!(wait_for(|| {
            let stats = app.synth.stats();
            stats.ready == total && stats.in_flight == 0
        }));
        let before = calls.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(before, total, "une synthèse par phrase");

        app.set_speed(2.0);
        app.change_speed(0.5);

        assert_eq!(app.player.speed(), 2.5, "la vitesse va au lecteur");
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), before, "aucune resynthèse");
        for id in 0..3 {
            assert!(app.synth.get(id).is_some(), "phrase {id} sortie du cache");
        }
        assert!(app.status().contains("2.50×"), "{}", app.status());
        assert!(!app.status().contains("resynth"), "{}", app.status());
    }

    #[test]
    fn the_player_starts_at_the_configured_speed() {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::doc::blocks::split_blocks(DOC) {
            builder.push_block(&block);
        }
        let mut config = Config::default();
        config.voice.speed = 1.75;
        let app = App::new(
            builder,
            None,
            Arc::new(SilentEngine),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        assert_eq!(app.player.speed(), 1.75);
    }

    #[test]
    fn time_estimates_follow_the_speed_from_1x_measures() {
        // La cadence se mesure sur l'audio à 1× ; le temps affiché est celui
        // qu'on entendra, donc divisé par la vitesse.
        let mut app = app_reading(DOC);
        app.pace.record(4.0, 10);
        let at_1x = app.estimated_total().unwrap();
        app.set_speed(2.0);
        let at_2x = app.estimated_total().unwrap();
        assert!((at_2x - at_1x / 2.0).abs() < 1e-9, "{at_1x} → {at_2x}");
        app.set_speed(1.0);
        assert!((app.estimated_total().unwrap() - at_1x).abs() < 1e-9);
    }

    pub(crate) const DOC: &str = "# Titre\n\nPremière phrase.\n\nDeuxième phrase.\n\nTroisième phrase.\n";

    #[test]
    fn a_document_reopens_where_the_voice_stopped() {
        let mut app = app_reading(DOC);
        let snippet = app.doc().sentence(2).unwrap().written();
        let source = app.source.clone();
        app.positions.record(&source, 2, &snippet);

        app.resume_where_left();

        assert_eq!(app.current_sentence(), 2);
        assert!(!app.status().is_empty(), "la reprise doit se dire");
    }

    #[test]
    fn a_sentence_that_moved_is_found_again() {
        // Un paragraphe ajouté en tête décale tout : c'est le texte qui
        // compte, pas l'index retenu.
        let mut app = app_reading(DOC);
        let snippet = app.doc().sentence(2).unwrap().written();
        let source = app.source.clone();
        app.positions.record(&source, 99, &snippet);

        app.resume_where_left();

        assert_eq!(app.current_sentence(), 2);
    }

    #[test]
    fn a_place_that_no_longer_exists_starts_the_document_over() {
        let mut app = app_reading(DOC);
        let source = app.source.clone();
        app.positions.record(&source, 2, "Une phrase retirée depuis.");

        app.resume_where_left();

        assert_eq!(app.current_sentence(), 0);
    }

    #[test]
    fn the_place_kept_is_the_sentence_being_read() {
        let mut app = app_reading(DOC);
        app.resume_where_left();
        app.seek_to(2);
        let (id, snippet) = app.place_to_remember().expect("une position à retenir");
        assert_eq!(id, 2);
        assert_eq!(snippet, app.doc().sentence(2).unwrap().written());
    }

    #[test]
    fn a_stream_keeps_no_place() {
        // Un flux et un collage ne sont pas les mêmes d'une fois sur l'autre :
        // leur rendre une position ancienne enverrait la lecture n'importe où.
        let mut app = app_reading(DOC);
        app.seek_to(2);
        assert!(app.place_to_remember().is_none());
    }

    #[test]
    fn a_document_read_to_the_end_keeps_no_place() {
        let mut app = app_reading(DOC);
        app.resume_where_left();
        app.seek_end();
        assert!(
            app.place_to_remember().is_none(),
            "rouvrir un document fini doit le reprendre au début"
        );
    }

    #[test]
    fn coming_back_to_a_silent_sentence_gives_it_another_chance() {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::doc::blocks::split_blocks(DOC) {
            builder.push_block(&block);
        }
        let mut config = Config::default();
        config.reading.autoplay = false;
        let engine = Arc::new(FlakyEngine {
            refused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        let mut app = App::new(
            builder,
            None,
            engine,
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        let id = app
            .doc()
            .sentences
            .iter()
            .position(|s| s.text.contains("Deuxième"))
            .expect("la phrase existe");

        assert!(wait_for(|| app.synth.failure(id).is_some()), "l'échec doit être constaté");

        // Revenir sur un passage muet, c'est demander à l'entendre.
        app.seek_to(id);
        assert!(
            app.synth.failure(id).is_none(),
            "la phrase reste condamnée alors qu'on y revient"
        );
    }

    #[test]
    fn stepping_back_leaves_the_block_at_once() {
        // Un cran de recul est un bloc, du premier coup : passer d'abord par
        // le début du bloc courant obligeait à appuyer deux fois pour bouger,
        // alors que la flèche gauche couvre déjà le recul fin.
        let mut app = app_reading("Premier bloc.\n\nDeuxième bloc. Avec deux phrases.\n");
        app.seek_to(2);
        app.seek_block(-1);
        assert_eq!(app.current_sentence(), 0);
        assert!(
            app.status().contains("Bloc précédent"),
            "statut : {:?}",
            app.status()
        );
    }

    #[test]
    fn scrolling_the_view_leaves_the_voice_where_it_was() {
        let mut app = app_reading(DOC);
        app.seek_to(2);
        let reading = app.current_sentence();

        app.scroll_view(-2);
        assert_eq!(app.current_sentence(), reading, "la lecture a bougé");
        assert!(!app.view.is_following());
        assert!(app.view.block(9) < 9, "la vue n'a pas reculé");

        app.follow_reading();
        assert!(app.view.is_following());
    }

    #[test]
    fn seeking_snaps_the_view_back_onto_the_voice() {
        let mut app = app_reading(DOC);
        app.scroll_view(-2);
        app.seek_sentences(1);
        assert!(app.view.is_following(), "un saut de lecture ramène la vue");
    }

    #[test]
    fn a_committed_search_moves_the_reading_position() {
        let mut app = app_reading(DOC);
        let target = app
            .doc()
            .sentences
            .iter()
            .position(|s| s.text.contains("Troisième"))
            .expect("la phrase existe");

        app.search_open();
        for c in "troisieme".chars() {
            app.search_push(c);
        }
        app.search_commit();

        assert_eq!(app.current_sentence(), target, "l'accent manquant doit passer");
        assert!(app.search_input().is_none(), "la saisie se referme");
    }

    #[test]
    fn a_search_without_result_moves_nothing_and_says_so() {
        let mut app = app_reading(DOC);
        app.seek_to(1);
        let before = app.current_sentence();

        app.search_open();
        for c in "kubernetes".chars() {
            app.search_push(c);
        }
        app.search_commit();

        assert_eq!(app.current_sentence(), before);
        assert!(app.status().contains("Aucun"), "statut : {:?}", app.status());
    }

    #[test]
    fn cancelling_a_search_forgets_the_query() {
        let mut app = app_reading(DOC);
        app.search_open();
        app.search_push('t');
        app.search_cancel();
        assert!(app.search_input().is_none());
        assert!(app.search().is_none());
    }

    #[test]
    fn the_screen_names_the_file_not_its_whole_path() {
        let mut app = app_reading(DOC);
        app.source = "/tres/long/chemin/vers/spec.pdf".to_string();
        assert_eq!(app.source_label(), "spec.pdf");
    }

    #[test]
    fn a_place_kept_under_the_bare_file_name_is_found_under_the_path() {
        let mut app = app_reading(DOC);
        app.source = "/docs/test.md".to_string();
        let snippet = app.doc().sentence(2).unwrap().written();
        app.positions.record("test.md", 2, &snippet);

        app.adopt_former_identity("test.md");
        app.resume_where_left();

        assert_eq!(app.current_sentence(), 2);
    }


    #[test]
    fn reloading_the_engine_forgets_the_old_pace_and_the_skipped_sentences() {
        // La cadence de l'ancien moteur ne dit rien du nouveau, et une phrase
        // qu'il avait refusée mérite sa chance.
        let mut app = app_reading(DOC);
        app.pace.record(4.0, 10);
        app.skipped.insert(1);
        app.reload_engine();
        assert!(app.estimated_total().is_none());
        assert_eq!(app.skipped_count(), 0);
    }

    /// Un fichier `config.toml` propre au test, dans un dossier neuf.
    fn kokoro_int8() -> &'static models::ModelPackage {
        models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap()
    }

    #[test]
    fn the_system_voice_asked_for_installs_nothing() {
        let mut app = app_reading(DOC);
        app.config.engine.engine = "system".into(); // comme `--engine system`
        app.install_default_voice();
        assert!(app.screen.download.is_none());
        assert_eq!(app.voice_install_status(), None);
    }

    /// Sans `kokoro-onnx`, le catalogue du terminal est vide : rien à
    /// télécharger. (Avec la feature, l'appel téléchargerait pour de bon :
    /// le test n'existe pas.)
    #[cfg(not(feature = "kokoro-onnx"))]
    #[test]
    fn a_terminal_without_kokoro_installs_nothing() {
        let mut app = app_reading(DOC);
        app.install_default_voice();
        assert!(app.screen.download.is_none());
        assert_eq!(app.voice_install_status(), None);
    }

    #[test]
    fn the_install_shows_its_progress() {
        let mut app = app_reading(DOC);
        app.screen.download = Some(models::Download::fake(kokoro_int8(), false));
        let status = app.voice_install_status().expect("une progression");
        assert!(status.contains("Installation de la voix"), "{status}");
        assert!(status.contains("voix du système lit en attendant"), "{status}");
    }

    #[test]
    fn a_failed_install_says_so_and_writes_nothing() {
        let (dir, path) = autosave_target("echec");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        let family = app.config.engine.family.clone();
        app.screen.download = Some(models::Download::fake_ended(
            kokoro_int8(),
            models::Progress::Failed("réseau injoignable".into()),
        ));
        app.tick();
        let status = app.voice_install_status().expect("l'échec se dit");
        let written = path.exists();
        std::fs::remove_dir_all(&dir).ok();
        assert!(status.contains("réseau injoignable"), "{status}");
        assert!(status.contains("prochain lancement"), "{status}");
        assert_eq!(app.config.engine.family, family, "la voix du système continue");
        assert!(!written, "config.toml n'est pas touché");
    }

    #[test]
    fn a_finished_install_becomes_the_voice_and_is_written() {
        let (dir, path) = autosave_target("fini");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.screen.download = Some(models::Download::fake(kokoro_int8(), true));
        app.tick();
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(app.config.engine.family, "kokoro");
        assert_eq!(written.engine.family, "kokoro");
        assert_eq!(app.voice_install_status(), None, "plus rien à dire");
    }

    #[test]
    fn the_footer_offers_to_quit_only_when_closing_quits() {
        let mut app = app_reading("");
        assert!(app.config_closes_to_quit(), "rien à lire, entrée complète");
        app.input_done = false;
        assert!(!app.config_closes_to_quit(), "l'entrée arrive encore : fermer ne quitte pas");
        let mut reading = app_reading(DOC);
        reading.input_done = true;
        assert!(!reading.config_closes_to_quit());
    }

    pub(crate) fn autosave_target(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sillon-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        (dir, path)
    }

    /// La page des réglages, sur « Interlignage ».
    fn on_line_spacing(app: &mut App) {
        app.screen.select(Field::LineSpacing);
        assert_eq!(app.screen.current_field(), Field::LineSpacing);
    }

    #[test]
    fn a_voice_of_another_language_does_not_restart_the_sentence() {
        let app = app_reading(DOC); // document français
        assert_eq!(app.effect_in_document(Field::VoiceEn, Effect::Resynthesize), Effect::None);
        assert_eq!(app.effect_in_document(Field::VoiceFr, Effect::Resynthesize), Effect::Resynthesize);
        assert_eq!(app.effect_in_document(Field::Speed, Effect::Speed), Effect::Speed);
    }

    #[test]
    fn the_preview_names_the_voice_of_its_language() {
        let mut app = app_reading(DOC);
        app.config.engine.family = "kokoro".into();
        assert!(app.preview_label("en").contains("af_bella"), "{}", app.preview_label("en"));
        assert!(app.preview_label("fr").contains("p-fenrir60"), "{}", app.preview_label("fr"));
    }

    #[test]
    fn a_setting_changed_on_screen_is_written_at_once() {
        let (dir, path) = autosave_target("immediat");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        on_line_spacing(&mut app);
        let before = app.config.reading.line_spacing;
        app.config_adjust_and_save(1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(app.config.reading.line_spacing, before + 1, "appliqué");
        assert_eq!(written.reading.line_spacing, before + 1, "écrit sans w");
    }

    #[test]
    fn the_autosave_writes_only_the_setting_changed_on_screen() {
        let (dir, path) = autosave_target("seul");
        let mut on_disk = Config::default();
        on_disk.voice.speed = 1.2;
        on_disk.save_to(&path).unwrap();
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.config.voice.speed = 2.5; // venue de --speed 2.5
        on_line_spacing(&mut app);
        app.config_adjust_and_save(1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.voice.speed, 1.2, "l'option de la ligne de commande n'est pas écrite");
        assert!(written.reading.autoplay, "l'autoplay coupé du test non plus");
        assert_eq!(written.reading.line_spacing, app.config.reading.line_spacing);
    }

    #[test]
    fn an_unreadable_config_is_never_overwritten_by_the_autosave() {
        let (dir, path) = autosave_target("illisible");
        let witness = b"[voice\nvoice = \"F2\" # \xc3\xa9crit \xc3\xa0 la main".to_vec();
        std::fs::write(&path, &witness).unwrap();
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        on_line_spacing(&mut app);
        let before = app.config.reading.line_spacing;
        app.config_adjust_and_save(1);
        let after = std::fs::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(after, witness, "le fichier de l'utilisateur est intact");
        assert_eq!(app.config.reading.line_spacing, before + 1, "le réglage s'applique quand même");
        assert!(app.status().contains("non enregistré"), "{}", app.status());
    }

    #[test]
    fn without_a_target_nothing_is_written_and_the_screen_says_so() {
        let mut app = app_reading(DOC);
        app.set_config_target(None);
        on_line_spacing(&mut app);
        app.config_adjust_and_save(1);
        assert!(app.status().contains("non enregistré"), "{}", app.status());
    }

    #[test]
    fn choosing_a_model_is_written_at_once() {
        // Le choix ne survivait au relancement que si `w` l'écrivait.
        let (dir, path) = autosave_target("modele");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        let package = models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap();
        app.select_model(package);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.engine.family, "kokoro");
        assert!(written.engine.model_path.ends_with("model_quantized.onnx"), "{}", written.engine.model_path);
        assert_eq!(written.voice.per_language["fr"], "p-fenrir60", "les voix du modèle suivent");
        assert!(written.reading.autoplay, "l'autoplay coupé du test n'est pas écrit");
    }

    #[test]
    fn the_speed_set_while_reading_is_written_at_once() {
        let (dir, path) = autosave_target("vitesse");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.config.voice.pitch = 0.5; // comme `--pitch 0.5`
        app.nudge_speed(0.1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!((written.voice.speed - 1.1).abs() < 1e-5, "{}", written.voice.speed);
        assert_eq!(written.voice.pitch, 0.0, "l'option de la ligne de commande n'est pas écrite");
    }

    struct SlowEngine;
    struct SlowSession;

    impl TtsEngine for SlowEngine {
        fn name(&self) -> String {
            "lent".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(SlowSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for SlowSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            std::thread::sleep(Duration::from_millis(300));
            Ok(PcmBuffer::new(vec![0.0; 100], 1000, 1))
        }
    }

    #[test]
    fn closing_says_when_a_synthesis_still_has_to_finish() {
        // Quitter attend la synthèse en cours : sur un modèle lent, plusieurs
        // secondes de terminal muet. Le dire vaut mieux que laisser croire à
        // un blocage.
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::doc::blocks::split_blocks(DOC) {
            builder.push_block(&block);
        }
        let mut config = Config::default();
        config.reading.autoplay = false;
        let app = App::new(
            builder,
            None,
            Arc::new(SlowEngine),
            config,
            "test.md".to_string(),
            Vec::new(),
            crate::audio::Player::silent(),
        );
        assert!(wait_for(|| app.synth.stats().in_flight > 0));
        assert!(app.closing_notice().is_some());
        assert!(wait_for(|| app.synth.stats().ready >= 3 && app.synth.stats().in_flight == 0));
        assert!(app.closing_notice().is_none());
    }

    #[test]
    fn every_word_start_lands_on_its_own_word() {
        let mut app = app_reading(
            "Un deux extraordinairement trois a quatre cinquante-six, sept huit neuf dix.",
        );
        let speech = Duration::from_millis(3700);
        let count = app.doc().sentence(0).unwrap().words.len();
        assert!(count >= 10);
        for i in 0..count {
            let ratio = app.doc().sentence(0).unwrap().word_start(i).unwrap();
            app.player.simulate(0, speech.mul_f32(ratio), speech);
            assert_eq!(app.current_word(), Some(i), "mot {i}");
            assert_eq!(app.reading_word(), Some(i), "mot {i}");
        }
    }

    #[test]
    fn the_jump_relay_survives_the_enqueue_until_a_position_exists() {
        let mut start = StartAt { sentence: 3, word: 2, ratio: 0.4, consumed: false };
        // Avant l'entrée en file : rien ne tombe.
        assert!(!relay_done(&start, Some(3)));
        // Entrée en file faite, mais pas encore de position : le relais reste.
        start.consumed = true;
        assert!(!relay_done(&start, None));
        // Une autre phrase sonne encore : le relais reste.
        assert!(!relay_done(&start, Some(2)));
        // La phrase visée a une position : le relais tombe.
        assert!(relay_done(&start, Some(3)));
    }

    #[test]
    fn a_seek_clears_a_pending_relay() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_words(2);
        assert!(app.start_at.is_some());
        app.seek_to(1);
        assert!(app.start_at.is_none());
        assert_eq!(app.reading_word(), None);
    }

    #[test]
    fn word_target_crosses_sentences_both_ways() {
        let app = app_reading("Un deux trois quatre. Cinq six.");
        let doc = app.doc();
        assert_eq!(word_target(doc, 0, 1, 2), Some((0, 3)));
        assert_eq!(word_target(doc, 0, 2, 3), Some((1, 1)));
        assert_eq!(word_target(doc, 1, 0, -1), Some((0, 3)));
        assert_eq!(word_target(doc, 0, 0, -1), None);
        assert_eq!(word_target(doc, 1, 1, 1), None);
    }

    #[test]
    fn seeking_words_moves_the_reading_word_before_any_audio() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_words(2);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.reading_word(), Some(2));
        // Un second saut part du mot visé, pas du début de la phrase.
        app.seek_words(3);
        assert_eq!(app.current_sentence(), 1);
        assert_eq!(app.reading_word(), Some(1));
    }

    #[test]
    fn seeking_to_a_word_goes_straight_there() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_to_word(1, 1);
        assert_eq!(app.current_sentence(), 1);
        assert_eq!(app.reading_word(), Some(1));
        // Absolu : le mot lu avant le saut ne compte pas.
        app.seek_to_word(0, 2);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.reading_word(), Some(2));
    }

    #[test]
    fn seeking_to_a_word_clamps_out_of_range_indices() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_to_word(0, 99);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.reading_word(), Some(3));
        app.seek_to_word(99, 0);
        assert_eq!(app.current_sentence(), 1);
        assert_eq!(app.reading_word(), Some(0));
    }

    #[test]
    fn seeking_to_a_word_starts_at_its_ratio() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        app.seek_to_word(0, 2);
        let expected = app.doc().sentence(0).and_then(|s| s.word_start(2)).unwrap();
        let start = app.start_at.as_ref().unwrap();
        assert_eq!((start.sentence, start.word), (0, 2));
        assert_eq!(start.ratio, expected);
    }

    #[test]
    fn seek_to_word_command_reaches_the_app() {
        let mut app = app_reading("Un deux trois quatre. Cinq six.");
        crate::reader::command::Command::SeekToWord { sentence: 1, word: 1 }.apply(&mut app);
        assert_eq!(app.current_sentence(), 1);
        assert_eq!(app.reading_word(), Some(1));
    }

    #[test]
    fn seeking_words_at_the_edge_says_so_and_stays() {
        let mut app = app_reading("Un deux.");
        app.seek_words(-1);
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.status(), "Aucun mot de ce côté");
    }

    /// Le moteur de coupe des tests du pool, recopié : minute ses mots et
    /// coupe la phrase attendue. Avec une barrière, la suite « Camille… »
    /// attend un feu vert.
    #[derive(Clone, Default)]
    struct Cutting {
        gate: Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>>,
        /// Ne donne pas le minutage des mots.
        untimed: bool,
        /// Ne donne pas le minutage de la suite « Camille… ».
        untimed_tail: bool,
        /// Refuse la suite « Camille… ».
        refuse_tail: bool,
    }
    type CuttingSession = Cutting;

    impl TtsEngine for Cutting {
        fn name(&self) -> String {
            "coupe".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn crate::tts::TtsSession>> {
            Ok(Box::new(self.clone()))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn splits_first_clause(&self) -> bool {
            true
        }
    }

    impl crate::tts::TtsSession for CuttingSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> anyhow::Result<crate::tts::PcmBuffer> {
            if text.starts_with("Camille") {
                if let Some(gate) = &self.gate {
                    // Borné : un essai qui échoue ne reste pas suspendu à sa barrière.
                    let _ = gate.locked().recv_timeout(Duration::from_secs(5));
                }
                if self.refuse_tail {
                    anyhow::bail!("suite refusée");
                }
            }
            let n = text.split_whitespace().count().max(1);
            // Une seconde par mot, en durées exactes.
            Ok(crate::tts::PcmBuffer::new(vec![0.1; n * 1024], 1024, 1))
        }
        fn synthesize_timed(
            &mut self,
            text: &str,
            words: &[String],
            p: &TtsParams,
        ) -> anyhow::Result<crate::tts::Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let spans = (0..words.len())
                .map(|i| crate::doc::WordSpan { start: i as f32, end: (i + 1) as f32 })
                .collect();
            let untimed = self.untimed || (self.untimed_tail && text.starts_with("Camille"));
            Ok(crate::tts::Synthesis { pcm, words: (!untimed).then_some(spans) })
        }
    }

    fn gated(gate: std::sync::mpsc::Receiver<()>) -> Cutting {
        Cutting { gate: Some(Arc::new(std::sync::Mutex::new(gate))), ..Cutting::default() }
    }

    fn cutting_app_with(text: &str, engine: Cutting) -> (App, rodio::queue::SourcesQueueOutput<f32>) {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block(text);
        let mut config = Config::default();
        config.reading.autoplay = false;
        let (player, output) = crate::audio::Player::idle(1024);
        let engine = Arc::new(engine);
        let app = App::new(builder, None, engine, config, "t".into(), Vec::new(), player);
        (app, output)
    }

    fn cutting_app(text: &str) -> (App, rodio::queue::SourcesQueueOutput<f32>) {
        cutting_app_with(text, Cutting::default())
    }

    fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(app) {
            assert!(Instant::now() < deadline, "jamais atteint");
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Aucune session ne s'ouvre tant que `broken` est levé.
    struct Repairable {
        broken: Arc<std::sync::atomic::AtomicBool>,
    }
    impl TtsEngine for Repairable {
        fn name(&self) -> String {
            "réparable".into()
        }
        fn open_session(&self) -> anyhow::Result<Box<dyn crate::tts::TtsSession>> {
            if self.broken.load(std::sync::atomic::Ordering::SeqCst) {
                anyhow::bail!("famille sans synthèse");
            }
            Ok(Box::new(Cutting::default()))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    #[test]
    fn a_voice_that_recovers_clears_its_unavailable_status() {
        let broken = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("Un. Deux.");
        let mut config = Config::default();
        config.reading.autoplay = false;
        let engine = Arc::new(Repairable { broken: Arc::clone(&broken) });
        let mut app = App::new(builder, None, engine, config, "t".into(), Vec::new(), crate::audio::Player::silent());
        tick_until(&mut app, |a| a.status() == "Voix indisponible : famille sans synthèse");
        assert!(app.voice_unavailable().is_some());

        broken.store(false, std::sync::atomic::Ordering::SeqCst);
        app.seek_to(0); // revenir sur un passage muet redonne sa chance à la phrase
        tick_until(&mut app, |a| a.voice_unavailable().is_none());
        app.tick();
        assert!(!app.status().starts_with("Voix indisponible"), "{}", app.status());
    }

    #[test]
    fn a_refused_sentence_with_an_open_session_is_not_an_unavailable_voice() {
        let (mut app, _out) = cutting_app_with(
            "Le quatorze juillet, Camille harangua la foule.",
            Cutting { refuse_tail: true, ..Cutting::default() },
        );
        tick_until(&mut app, |a| a.synth.failure(0).is_some());
        app.tick();
        assert_eq!(app.voice_unavailable(), None);
        assert!(!app.status().starts_with("Voix indisponible"));
    }

    #[test]
    fn the_word_cut_sees_numbers_as_written() {
        // La forme parlée dit « dix-huit » ; l'écrit « 18 » interdit la
        // coupe juste après : la tête finit sur « heures ».
        let (mut app, _out) = cutting_app("Le train partira vers 18 heures depuis la gare du centre-ville.");
        tick_until(&mut app, |a| a.synth.get(0).is_some());
        assert_eq!(app.synth.get(0).unwrap().part, crate::tts::pool::Part::Head { words: 6 });
    }

    #[test]
    fn the_tail_follows_the_head_in_the_queue() {
        let (mut app, _output) = cutting_app("Le quatorze juillet, Camille harangua la foule.");
        tick_until(&mut app, |a| a.queue_head >= 1);
        assert_eq!(app.player.queued_parts(), [(0, Duration::ZERO), (0, Duration::from_secs(3))]);
        assert_eq!(app.timings.get(&0).map(|t| t.len()), Some(7), "tête et suite mises bout à bout");
        assert_eq!(app.timings[&0][3].start, 3.0, "la suite commence à la fin de la tête");
    }

    #[test]
    fn a_jump_into_the_tail_skips_the_head() {
        let (mut app, _output) = cutting_app("Le quatorze juillet, Camille harangua la foule.");
        app.seek_to_word(0, 5);
        tick_until(&mut app, |a| a.queue_head >= 1);
        let parts = app.player.queued_parts();
        assert_eq!(parts.len(), 1, "la tête n'est pas jouée : {parts:?}");
        // Base 3 s (la tête) + 2 s (deux mots de la suite) : le mot 5.
        assert_eq!(parts[0], (0, Duration::from_secs(5)));
    }

    #[test]
    fn the_highlight_crosses_from_the_head_into_the_tail() {
        let (mut app, mut output) = cutting_app("Le quatorze juillet, Camille harangua la foule.");
        tick_until(&mut app, |a| a.queue_head >= 1);
        app.player.play();
        // La tête sort entière (3 s à 1024 Hz), et un peu de la suite.
        let _: Vec<f32> = output.by_ref().take(3 * 1024 + 100).collect();
        app.tick();
        assert_eq!(app.current_sentence(), 0);
        assert_eq!(app.timings.get(&0).map(|t| t.len()), Some(7), "la fin de la tête garde le minutage");
        assert_eq!(app.current_word(), Some(3), "le premier mot de la suite");
    }

    #[test]
    fn a_tail_that_is_late_is_one_stall_and_keeps_the_place() {
        let (release, gate) = std::sync::mpsc::channel();
        let (mut app, mut output) = cutting_app_with("Le quatorze juillet, Camille harangua la foule.", gated(gate));
        app.player.play();
        tick_until(&mut app, |a| a.awaiting_tail.is_some());
        assert_eq!(app.stalls(), 0, "le premier son n'est pas une coupure");
        let _: Vec<f32> = output.by_ref().take(3 * 1024 + 10).collect();
        for _ in 0..5 {
            app.tick();
        }
        assert_eq!(app.player.queue_len(), 0);
        assert_eq!(app.stalls(), 1, "une coupure, comptée une fois");
        assert_eq!(app.current_sentence(), 0, "la phrase attend sa suite");
        assert!(app.timings.contains_key(&0));
        release.send(()).unwrap();
        tick_until(&mut app, |a| a.queue_head >= 1);
        assert_eq!(app.player.queued_parts(), [(0, Duration::from_secs(3))]);
        assert_eq!(app.stalls(), 1);
    }

    /// Une phrase coupée dont la tête est en file et la suite retenue.
    fn head_waiting_for_its_tail(engine: Cutting) -> App {
        let (mut app, _output) = cutting_app_with("Le quatorze juillet, Camille harangua la foule.", engine);
        tick_until(&mut app, |a| a.awaiting_tail.is_some());
        app
    }

    #[test]
    fn during_the_head_the_highlight_follows_the_head_words() {
        let (release, gate) = std::sync::mpsc::channel();
        let mut app = head_waiting_for_its_tail(gated(gate));
        // Tête : trois mots d'une seconde ; la suite n'est pas encore là.
        app.player.simulate(0, Duration::from_millis(1500), Duration::from_secs(3));
        assert_eq!(app.current_word(), Some(1), "le mot de la tête qui sonne");
        // La suite tarde : le surlignage attend sur le dernier mot de la tête.
        app.player.simulate(0, Duration::from_secs(3), Duration::from_secs(3));
        assert_eq!(app.current_word_fraction(), Some((2, 1.0)));
        drop(release);
    }

    #[test]
    fn an_untimed_head_spreads_the_highlight_over_its_own_words() {
        let (release, gate) = std::sync::mpsc::channel();
        let mut app = head_waiting_for_its_tail(Cutting { untimed: true, ..gated(gate) });
        assert!(!app.timings.contains_key(&0));
        app.player.simulate(0, Duration::from_millis(2900), Duration::from_secs(3));
        assert_eq!(app.current_word(), Some(2), "fin de la tête, pas fin de phrase");
        app.player.simulate(0, Duration::from_millis(100), Duration::from_secs(3));
        assert_eq!(app.current_word(), Some(0));
        drop(release);
    }

    #[test]
    fn a_refused_tail_after_its_head_moves_on_cleanly() {
        let (release, gate) = std::sync::mpsc::channel();
        let (mut app, mut output) = cutting_app_with(
            "Le quatorze juillet, Camille harangua la foule. Paris bascula sans un mot.",
            Cutting { refuse_tail: true, ..gated(gate) },
        );
        app.player.play();
        tick_until(&mut app, |a| a.awaiting_tail.is_some());
        // La tête sort entière, puis la suite est refusée.
        let _: Vec<f32> = output.by_ref().take(3 * 1024 + 10).collect();
        app.tick();
        release.send(()).unwrap();
        tick_until(&mut app, |a| a.awaiting_tail.is_none());
        assert_eq!(app.current_sentence(), 1, "rien ne sonne : la place passe à la suivante");
        assert!(!app.timings.contains_key(&0));
        assert_eq!(app.skipped_count(), 1, "la phrase restée à moitié muette se dit");
    }

    #[test]
    fn a_tail_refused_while_its_head_sounds_lets_the_head_finish() {
        let (release, gate) = std::sync::mpsc::channel();
        let mut app = head_waiting_for_its_tail(Cutting { refuse_tail: true, ..gated(gate) });
        release.send(()).unwrap();
        tick_until(&mut app, |a| a.awaiting_tail.is_none());
        assert_eq!(app.current_sentence(), 0, "la tête est encore en file");
        assert!(app.timings.contains_key(&0), "son minutage la suit jusqu'au bout");
        app.player.simulate(0, Duration::from_millis(1500), Duration::from_secs(3));
        assert_eq!(app.current_word(), Some(1));
    }

    /// Tête et suite en file, la suite arrivée pendant la tête : le
    /// surlignage reste dans la tête jusqu'à sa fin, puis reprend dans la
    /// suite sans revenir en arrière.
    fn assert_highlight_follows_the_parts(engine: Cutting, expected: [usize; 8]) {
        let (mut app, _output) = cutting_app_with("Le quatorze juillet, Camille harangua la foule.", engine);
        tick_until(&mut app, |a| a.queue_head >= 1);
        assert!(app.awaiting_tail.is_none(), "la suite est en file");
        let head = Duration::from_secs(3);
        let mut seen = Vec::new();
        for ms in [100, 1500, 2900, 3000] {
            app.player.simulate(0, Duration::from_millis(ms), head);
            seen.push(app.current_word().unwrap());
        }
        for ms in [3000, 3100, 6900, 7000] {
            app.player.simulate(0, Duration::from_millis(ms), Duration::from_secs(7));
            seen.push(app.current_word().unwrap());
        }
        assert_eq!(seen, expected);
    }

    #[test]
    fn an_untimed_head_keeps_the_highlight_once_its_tail_is_queued() {
        // Sans minutage, les poids : « juillet, » porte la pause de sa
        // virgule, et prend la mi-tête.
        assert_highlight_follows_the_parts(Cutting { untimed: true, ..Cutting::default() }, [0, 2, 2, 2, 3, 3, 6, 6]);
    }

    #[test]
    fn a_timed_head_with_an_untimed_tail_keeps_the_highlight() {
        assert_highlight_follows_the_parts(Cutting { untimed_tail: true, ..Cutting::default() }, [0, 1, 2, 2, 3, 3, 6, 6]);
    }

    #[test]
    fn a_timed_head_and_tail_keep_the_highlight() {
        assert_highlight_follows_the_parts(Cutting::default(), [0, 1, 2, 2, 3, 3, 6, 6]);
    }

    #[test]
    fn a_jump_during_the_head_forgets_its_tail() {
        let (release, gate) = std::sync::mpsc::channel();
        let (mut app, _output) = cutting_app_with(
            "Le quatorze juillet, Camille harangua la foule. Paris bascula sans un mot.",
            gated(gate),
        );
        assert_eq!(app.doc().sentences.len(), 2);
        tick_until(&mut app, |a| a.awaiting_tail.is_some());
        app.seek_to(1);
        assert!(app.awaiting_tail.is_none());
        release.send(()).unwrap();
        tick_until(&mut app, |a| a.queue_head >= 2);
        let parts = app.player.queued_parts();
        assert!(parts.iter().all(|&(id, _)| id == 1), "rien de la phrase quittée : {parts:?}");
        assert_eq!(app.stalls(), 0, "un saut n'est pas une coupure");
    }
}
