//! Le contrôleur : état de l'application et commandes du lecteur.

pub mod config;
pub mod keys;
pub mod pace;
pub mod search;
pub mod startup;
pub mod view;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::Player;
use crate::bookmark::BookmarkStore;
use crate::position::PositionStore;
use crate::config::Config;
use crate::doc::{Document, DocumentBuilder};
use crate::input::{BlockAssembler, InputEvent};
use crate::models;
use crate::tts::preview::{self, Previewer};
use crate::tts::{self, Synthesizer, TtsEngine, TtsParams};
use crate::app::config::{ConfigScreen, Effect, Field, Page, Section};
use crate::app::pace::Pace;
use crate::app::search::Search;
use crate::app::view::ViewAnchor;

/// Panneau latéral affiché à droite du texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Contents,
    Bookmarks,
}

/// Réglages d'affichage du texte, tels que la rubrique « Lecture » les fixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingStyle {
    /// Lignes vides insérées entre deux lignes de texte.
    pub spacing: usize,
    /// Souligne d'un fond la ligne où se trouve le mot prononcé.
    pub ruler: bool,
}

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
    pub bookmarks: BookmarkStore,
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
    pub notices: Vec<String>,

    /// Lancement du lecteur, origine du chrono de démarrage.
    started_at: Instant,
    /// Vrai dès la première phrase produite : le démarrage ne s'annonce qu'une
    /// fois, même quand un changement de voix vide le cache ensuite.
    synthesis_started: bool,
    /// Un choix fait à l'écran attend d'être écrit dans `config.toml`.
    config_dirty: bool,
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

/// Le relais d'un saut peut tomber : le lecteur donne enfin une position pour
/// la phrase visée, et l'offset a déjà été appliqué.
fn relay_done(start: &StartAt, playing_sentence: Option<usize>) -> bool {
    start.consumed && playing_sentence == Some(start.sentence)
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

        let (bookmarks, bookmark_notice) = BookmarkStore::load();
        let mut notices = notices;
        notices.extend(bookmark_notice);
        if let Some(err) = player.error() {
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
            bookmarks,
            positions: PositionStore::load(),
            remembers_place: false,
            config,
            source,
            queue_head: 0,
            last_known: 0,
            skipped: HashSet::new(),
            pace: Pace::default(),
            start_at: None,
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
            notices,
            started_at: Instant::now(),
            synthesis_started: false,
            config_dirty: false,
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

    /// Vrai quand le moteur exige un enregistrement de référence et n'en a pas.
    ///
    /// Sans ce contrôle, une famille qui ne sait que cloner échoue une fois par
    /// phrase, et le journal se remplit du même message deux cents fois. Mieux
    /// vaut le dire une fois, là où l'on peut y remédier.
    pub fn reference_missing(&self) -> bool {
        if !self.synth.requires_reference() {
            return false;
        }
        let voice = crate::config::expand_tilde(self.config.voice.voice.trim());
        voice.is_empty() || !std::path::Path::new(&voice).is_file()
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

    /// Mot prononcé dans la phrase courante, et la part de ce mot déjà dite.
    pub fn current_word_fraction(&self) -> Option<(usize, f32)> {
        let position = self.player.position()?;
        let sentence = self.doc().sentence(position.sentence_id)?;
        sentence.word_fraction(
            position.elapsed.as_secs_f32(),
            position.speech.as_secs_f32(),
        )
    }

    /// La part du mot lu déjà dite : zéro tant qu'aucun son ne la mesure
    /// (un saut en attente, une pause avant le premier son).
    pub fn reading_word_progress(&self) -> f32 {
        self.current_word_fraction().map_or(0.0, |(_, fraction)| fraction)
    }

    // ---------------------------------------------------------------- cycle

    /// Un tour de boucle : entrée, audio, synthèse.
    pub fn tick(&mut self) {
        self.synthesis_started = startup::latch(self.synthesis_started, self.synth.stats());
        self.poll_download();
        self.poll_preview();
        self.drain_input();
        // La configuration fait foi : un réglage posé ailleurs (écran,
        // réglages de l'app) atteint le son au tour suivant.
        if self.player.speed() != self.config.voice.speed {
            self.player.set_speed(self.config.voice.speed);
        }
        self.advance_playback();
        self.fill_output();
        self.synth.set_cursor(self.current_sentence());
        if self.status_until.is_some_and(|t| Instant::now() > t) {
            self.status.clear();
            self.status_until = None;
        }
    }

    fn drain_input(&mut self) {
        let mut blocks = Vec::new();
        while let Some(rx) = self.input.as_ref() {
            match rx.try_recv() {
                Ok(InputEvent::Line(line)) => {
                    if let Some(block) = self.assembler.push(&line) {
                        blocks.push(block);
                    }
                }
                Ok(InputEvent::Eof) => {
                    blocks.extend(self.assembler.finish());
                    self.input_done = true;
                    self.input = None;
                    break;
                }
                Ok(InputEvent::Error(err)) => {
                    self.notices.push(format!("lecture de l'entrée : {err}"));
                    self.input_done = true;
                    self.input = None;
                    break;
                }
                Err(TryRecvError::Empty) => {
                    // Un flux qui marque une pause ne doit pas retenir le texte
                    // déjà reçu.
                    blocks.extend(self.assembler.poll_idle());
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    blocks.extend(self.assembler.finish());
                    self.input_done = true;
                    self.input = None;
                    break;
                }
            }
        }

        for block in blocks {
            self.builder.push_block(&block);
        }
        if self.input_done {
            self.builder.finish();
        }
        self.builder.sync_word_total();
        self.publish_new_sentences();
    }

    /// Transmet à la synthèse les phrases apparues depuis le dernier tour.
    fn publish_new_sentences(&mut self) {
        let total = self.builder.sentence_count();
        while self.published < total {
            let (text, pause) = {
                let sentence = &self.builder.document().sentences[self.published];
                (sentence.text.clone(), sentence.trailing_pause_ms)
            };
            self.synth.push(text, pause);
            self.published += 1;
        }
    }

    fn advance_playback(&mut self) {
        let finished = self.player.reconcile();
        if let Some(last) = finished.last() {
            self.last_known = last.saturating_add(1);
        }
        if let Some(position) = self.player.position() {
            self.last_known = position.sentence_id;
        }
        let playing = self.player.position().map(|p| p.sentence_id);
        if self.start_at.is_some_and(|s| relay_done(&s, playing)) {
            self.start_at = None;
        }
    }

    /// Alimente la sortie audio, sans jamais la laisser prendre trop d'avance.
    ///
    /// Deux phrases en file suffisent à enchaîner sans blanc, tout en gardant la
    /// possibilité de sauter avec une latence imperceptible.
    fn fill_output(&mut self) {
        if !self.player.is_available() {
            return;
        }
        while self.player.queue_len() < 2 {
            let total = self.builder.sentence_count();
            if self.queue_head >= total {
                break;
            }
            if let Some(rendered) = self.synth.get(self.queue_head) {
                let words = self
                    .builder
                    .document()
                    .sentence(self.queue_head)
                    .map(|s| s.words.len())
                    .unwrap_or(0);
                self.pace.record(rendered.speech.as_secs_f64(), words);
                let offset = match self.start_at.as_mut() {
                    Some(start) if start.sentence == self.queue_head && !start.consumed => {
                        start.consumed = true;
                        rendered.speech.mul_f32(start.ratio)
                    }
                    _ => Duration::ZERO,
                };
                self.player.enqueue_from(
                    self.queue_head,
                    &rendered.audio,
                    rendered.speech,
                    offset,
                );
                self.queue_head += 1;
                continue;
            }
            // Une phrase que le moteur refuse ne doit pas bloquer la lecture.
            if self.synth.failure(self.queue_head).is_some() {
                self.skipped.insert(self.queue_head);
                self.queue_head += 1;
                continue;
            }
            break; // Synthèse en cours : on attend.
        }
    }

    pub fn is_finished(&self) -> bool {
        self.input_done
            && self.queue_head >= self.builder.sentence_count()
            && self.player.queue_len() == 0
    }

    /// Secondes d'écoute par mot : la cadence mesurée à 1× sur ce qui a été
    /// synthétisé, divisée par la vitesse de lecture.
    fn seconds_per_word(&self) -> Option<f64> {
        let speed = self.player.speed().max(f32::EPSILON) as f64;
        self.pace.seconds_per_word().map(|s| s / speed)
    }

    /// Durée totale estimée du document, en secondes.
    ///
    /// Renvoie `None` tant qu'aucune phrase n'a été synthétisée : mieux vaut ne
    /// rien afficher qu'un chiffre inventé.
    pub fn estimated_total(&self) -> Option<f64> {
        let rate = self.seconds_per_word()?;
        Some(rate * self.doc().total_words as f64)
    }

    /// Temps de lecture estimé jusqu'au curseur.
    pub fn estimated_elapsed(&self) -> Option<f64> {
        let rate = self.seconds_per_word()?;
        let current = self.current_sentence();
        let words: usize = self
            .doc()
            .sentences
            .iter()
            .take(current)
            .map(|s| s.words.len())
            .sum();
        let inside = self
            .doc()
            .sentence(current)
            .map(|s| s.words.len() as f64 * self.sentence_progress() as f64)
            .unwrap_or(0.0);
        Some(rate * (words as f64 + inside))
    }

    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    pub fn input_complete(&self) -> bool {
        self.input_done
    }

    // -------------------------------------------------------------- commandes

    pub fn toggle_play(&mut self) {
        // Reprendre la lecture met fin à l'aperçu : les deux ne doivent pas se
        // superposer.
        self.stop_preview();
        let playing = self.player.toggle();
        self.notify(if playing { "Lecture" } else { "Pause" });
    }

    /// Mot lu dans la phrase courante, saut en attente compris.
    ///
    /// Entre un saut et l'arrivée de son audio, aucune position n'existe ;
    /// sans ce relais, le surlignage reviendrait au début de la phrase.
    pub fn reading_word(&self) -> Option<usize> {
        self.current_word().or_else(|| {
            self.start_at
                .filter(|s| s.sentence == self.current_sentence())
                .map(|s| s.word)
        })
    }

    /// Déplace la lecture de `delta` mots, en franchissant les phrases.
    pub fn seek_words(&mut self, delta: isize) {
        let sentence = self.current_sentence();
        let word = self.reading_word().unwrap_or(0);
        let Some((target, word)) = word_target(self.doc(), sentence, word, delta) else {
            self.notify("Aucun mot de ce côté");
            return;
        };
        self.seek_to_word(target, word);
    }

    /// Déplace la lecture au mot `word` de la phrase `sentence`.
    ///
    /// Absolu : l'interface vise un mot qu'elle voit, sans dépendre du mot
    /// que la voix a atteint pendant ce temps. Les indices trop grands
    /// s'arrêtent au dernier ; une phrase sans mots se rejoint en entier.
    pub fn seek_to_word(&mut self, sentence: usize, word: usize) {
        let total = self.builder.sentence_count();
        if total == 0 {
            return;
        }
        let target = sentence.min(total - 1);
        let words = self.doc().sentence(target).map_or(0, |s| s.words.len());
        self.seek_to(target);
        if words == 0 {
            return;
        }
        let word = word.min(words - 1);
        let ratio = self
            .doc()
            .sentence(target)
            .and_then(|s| s.word_start(word))
            .unwrap_or(0.0);
        self.start_at = Some(StartAt { sentence: target, word, ratio, consumed: false });
    }

    /// Déplace la lecture à la phrase `target`.
    ///
    /// Vider la sortie puis réamorcer au bon endroit est ce qui rend l'avance et
    /// le recul réellement instantanés : la synthèse suit le curseur.
    pub fn seek_to(&mut self, target: usize) {
        let total = self.builder.sentence_count();
        if total == 0 {
            return;
        }
        self.start_at = None;
        let target = target.min(total - 1);
        self.player.flush();
        // Ce qu'on rejoint mérite d'être tenté à nouveau : un refus du moteur
        // ne doit pas rendre un passage définitivement muet.
        self.synth.retry_from(target);
        self.queue_head = target;
        self.last_known = target;
        self.synth.set_cursor(target);
        // Un saut de lecture ramène toujours la vue : on veut voir ce qu'on
        // vient de rejoindre.
        self.view.follow();
    }

    pub fn seek_sentences(&mut self, delta: isize) {
        let current = self.current_sentence();
        let target = current.saturating_add_signed(delta);
        self.seek_to(target);
        let label = if delta.abs() == 1 {
            if delta > 0 { "Phrase suivante" } else { "Phrase précédente" }
        } else if delta > 0 {
            "Avance rapide"
        } else {
            "Retour rapide"
        };
        self.notify(label);
    }

    pub fn seek_block(&mut self, delta: isize) {
        let current = self.current_sentence();
        match self.doc().sentence_in_neighbour_block(current, delta) {
            Some(target) => {
                self.seek_to(target);
                self.notify(if delta > 0 { "Bloc suivant" } else { "Bloc précédent" });
            }
            None => self.notify("Aucun bloc de ce côté"),
        }
    }

    pub fn seek_section(&mut self, delta: isize) {
        let current = self.current_sentence();
        match self.doc().sentence_in_neighbour_section(current, delta) {
            Some(target) => {
                let title = self
                    .doc()
                    .section_of_sentence(target)
                    .map(|s| s.title.clone())
                    .unwrap_or_default();
                self.seek_to(target);
                self.notify(&format!("Titre : {title}"));
            }
            None => self.notify("Aucun titre de ce côté"),
        }
    }

    pub fn seek_start(&mut self) {
        self.seek_to(0);
        self.notify("Début du document");
    }

    pub fn seek_end(&mut self) {
        let total = self.builder.sentence_count();
        self.seek_to(total.saturating_sub(1));
        self.notify("Fin du document");
    }

    pub fn restart_sentence(&mut self) {
        let current = self.current_sentence();
        self.seek_to(current);
        self.notify("Phrase reprise");
    }

    /// Change la vitesse de lecture, sur-le-champ.
    pub fn change_speed(&mut self, delta: f32) {
        let speed = (self.config.voice.speed + delta).clamp(0.5, 3.0);
        self.set_speed(speed);
    }

    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(0.5, 3.0);
        if (speed - self.config.voice.speed).abs() < f32::EPSILON {
            return;
        }
        // L'audio en cache est à 1× et le reste : la vitesse s'applique au
        // son, dès le prochain bloc de 5 ms de la phrase en cours. Ni
        // resynthèse, ni file vidée, ni coupure.
        self.config.voice.speed = speed;
        self.player.set_speed(speed);
        self.notify(&format!("Vitesse {speed:.2}×"));
    }

    pub fn change_volume(&mut self, delta: f32) {
        let volume = (self.player.volume() + delta).clamp(0.0, 2.0);
        self.player.set_volume(volume);
        self.config.voice.volume = volume;
        self.notify(&format!("Volume {:.0} %", volume * 100.0));
    }

    pub fn toggle_mute(&mut self) {
        let muted = self.player.toggle_mute();
        self.notify(if muted { "Son coupé" } else { "Son rétabli" });
    }

    /// Paramètres de synthèse, dans la langue du document.
    pub fn synthesis_params(&self) -> TtsParams {
        self.config.tts_params_in(self.builder.language())
    }

    fn apply_tts_params(&mut self) {
        let params = self.synthesis_params();
        self.synth.set_params(params);
    }


    // ------------------------------------------------------------ vue et recherche

    /// Bloc autour duquel dessiner, lecture ou vue figée.
    pub fn anchor_block(&self) -> usize {
        let current = self
            .doc()
            .sentence(self.current_sentence())
            .map(|s| s.block_id)
            .unwrap_or(0);
        self.view.block(current)
    }

    /// Fait défiler la vue de `delta` blocs, sans toucher à la lecture.
    pub fn scroll_view(&mut self, delta: isize) {
        let current = self
            .doc()
            .sentence(self.current_sentence())
            .map(|s| s.block_id)
            .unwrap_or(0);
        let last = self.doc().blocks.len().saturating_sub(1);
        self.view.scroll(delta, current, last);
        self.notify("Vue figée — f revient à la lecture");
    }

    pub fn follow_reading(&mut self) {
        self.view.follow();
        self.notify("Retour à la lecture");
    }

    pub fn toggle_follow(&mut self) {
        if self.view.is_following() {
            self.scroll_view(0);
        } else {
            self.follow_reading();
        }
    }

    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// Requête en cours de saisie, s'il y en a une.
    pub fn search_input(&self) -> Option<&str> {
        self.search_input.as_deref()
    }

    pub fn search_open(&mut self) {
        self.search_input = Some(String::new());
    }

    pub fn search_push(&mut self, c: char) {
        if let Some(buffer) = self.search_input.as_mut() {
            buffer.push(c);
        }
    }

    pub fn search_backspace(&mut self) {
        if let Some(buffer) = self.search_input.as_mut() {
            buffer.pop();
        }
    }

    pub fn search_cancel(&mut self) {
        self.search_input = None;
        self.search = None;
    }

    /// Valide la recherche et envoie la lecture sur le premier résultat.
    ///
    /// Sans résultat, rien ne bouge : déplacer la lecture au hasard ferait
    /// perdre la page en cours pour rien.
    pub fn search_commit(&mut self) {
        let Some(query) = self.search_input.take() else {
            return;
        };
        let from = self.current_sentence();
        let search = Search::new(self.doc(), &query, from);
        if search.is_empty() {
            self.search = None;
            self.notify(&format!("Aucun résultat pour « {} »", query.trim()));
            return;
        }
        let target = search.target();
        self.search = Some(search);
        if let Some(target) = target {
            self.seek_to(target);
        }
        // Le rang des résultats est porté en permanence par la ligne d'état :
        // le répéter ici le dirait deux fois sur la même ligne.
    }

    /// Passe au résultat suivant ou précédent.
    pub fn search_advance(&mut self, delta: isize) {
        let Some(search) = self.search.as_mut() else {
            self.notify("Aucune recherche en cours — / pour en lancer une");
            return;
        };
        if let Some(target) = search.advance(delta) {
            self.seek_to(target);
        }
    }

    /// Le document tel qu'il se nomme à l'écran : son nom de fichier.
    ///
    /// L'identité, elle, est le chemin entier ; il déborderait de l'en-tête.
    pub fn source_label(&self) -> String {
        std::path::Path::new(&self.source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.source.clone())
    }

    /// Rattache à ce document ce qui a été retenu sous un ancien nom.
    ///
    /// Positions et signets étaient rattachés au nom du fichier seul ; ils le
    /// sont désormais à son chemin. Rien de ce qui a été posé ne se perd.
    pub fn adopt_former_identity(&mut self, former: &str) {
        self.positions.adopt(former, &self.source);
        self.bookmarks.adopt(former, &self.source);
    }

    /// Retient la position dans ce document, et reprend où l'on s'était arrêté.
    ///
    /// Réservé aux documents qui ont une identité stable : un flux et un
    /// collage ne sont pas les mêmes d'une fois sur l'autre, et leur rendre
    /// une position ancienne enverrait la lecture n'importe où.
    pub fn resume_where_left(&mut self) {
        self.remembers_place = true;
        let Some((at, snippet)) = self
            .positions
            .get(&self.source)
            .map(|p| (p.sentence_id, p.snippet.clone()))
        else {
            return;
        };
        // À l'index retenu si la phrase y est encore ; sinon là où elle est
        // passée. Un paragraphe ajouté en tête décale tout, et la reprise
        // tomberait au milieu d'un autre passage.
        let target = {
            let doc = self.doc();
            if doc.sentence(at).map(|s| s.written()).as_deref() == Some(snippet.as_str()) {
                Some(at)
            } else {
                doc.sentences.iter().position(|s| s.written() == snippet)
            }
        };
        // Reprendre au début n'est pas une reprise : rien à dire, rien à faire.
        let Some(target) = target.filter(|t| *t > 0) else {
            return;
        };
        self.seek_to(target);
        self.notify("Reprise là où vous en étiez — g revient au début");
    }

    /// Où en est la lecture, si cela vaut d'être retenu.
    ///
    /// Rendue plutôt qu'écrite : c'est la décision qui se vérifie, l'écriture
    /// n'ayant lieu qu'à la fermeture.
    pub fn place_to_remember(&self) -> Option<(usize, String)> {
        if !self.remembers_place {
            return None;
        }
        let id = self.current_sentence();
        // Un document lu jusqu'au bout n'a plus de position : le rouvrir doit
        // le reprendre au début, non sur sa dernière phrase.
        if id + 1 >= self.doc().sentences.len() {
            return None;
        }
        Some((id, self.doc().sentence(id)?.written()))
    }

    /// Note où en était la lecture, dans la mémoire des positions.
    ///
    /// Sans écrire : l'écriture a lieu à la fermeture, une fois pour tous les
    /// documents lus dans la session.
    fn note_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        match self.place_to_remember() {
            Some((id, snippet)) => self.positions.record(&self.source, id, &snippet),
            None => self.positions.forget(&self.source),
        }
    }

    /// Note où en était la lecture et l'écrit, pour la prochaine ouverture.
    pub fn remember_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        self.note_position();
        // La fermeture est en cours : plus personne n'est là pour lire une
        // erreur, et une position perdue coûte un défilement.
        let _ = self.positions.save();
    }

    pub fn toggle_bookmark(&mut self) {
        let sentence_id = self.current_sentence();
        let Some(sentence) = self.doc().sentence(sentence_id) else {
            return;
        };
        let block_id = sentence.block_id;
        // Un signet cite ce qu'on a sous les yeux, pas ce que le moteur entend.
        let snippet = sentence.written();
        let title = self
            .doc()
            .section_of_sentence(sentence_id)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Sans titre".to_string());

        let added = self
            .bookmarks
            .toggle(&self.source, sentence_id, block_id, &title, &snippet);
        match self.bookmarks.save() {
            Ok(()) => self.notify(if added { "Signet ajouté" } else { "Signet retiré" }),
            Err(err) => self.notify(&format!("Signet non enregistré : {err}")),
        }
    }

    pub fn bookmarked_here(&self) -> bool {
        self.bookmarks.contains(&self.source, self.current_sentence())
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        self.panel = if self.panel == panel { Panel::None } else { panel };
        self.selection = 0;
    }

    /// Sections atteignables, dans l'ordre du document.
    ///
    /// La section racine reste vide quand le document ouvre sur un titre ;
    /// l'exclure garde l'index de sélection aligné sur ce qui est affiché.
    pub fn navigable_sections(&self) -> Vec<usize> {
        self.doc()
            .sections
            .iter()
            .filter(|s| s.first_sentence.is_some())
            .map(|s| s.id)
            .collect()
    }

    /// Déplace la sélection dans le panneau ouvert.
    pub fn move_selection(&mut self, delta: isize) {
        let count = match self.panel {
            Panel::Contents => self.navigable_sections().len(),
            Panel::Bookmarks => self.bookmarks.for_document(&self.source).count(),
            _ => 0,
        };
        if count == 0 {
            return;
        }
        let next = (self.selection as isize + delta).clamp(0, count as isize - 1);
        self.selection = next as usize;
    }

    /// Saute à l'élément sélectionné dans le panneau.
    pub fn activate_selection(&mut self) {
        match self.panel {
            Panel::Contents => {
                let target = self
                    .navigable_sections()
                    .get(self.selection)
                    .and_then(|&id| self.doc().section(id))
                    .and_then(|s| s.first_sentence);
                if let Some(target) = target {
                    self.seek_to(target);
                    self.notify("Saut au titre");
                }
            }
            Panel::Bookmarks => {
                let target = self
                    .bookmarks
                    .for_document(&self.source)
                    .nth(self.selection)
                    .map(|b| b.sentence_id);
                if let Some(target) = target {
                    self.seek_to(target);
                    self.notify("Saut au signet");
                }
            }
            _ => {}
        }
    }

    pub fn delete_selection(&mut self) {
        if self.panel != Panel::Bookmarks {
            return;
        }
        let target = self
            .bookmarks
            .for_document(&self.source)
            .nth(self.selection)
            .map(|b| b.sentence_id);
        if let Some(sentence_id) = target {
            self.bookmarks.remove(&self.source, sentence_id);
            let _ = self.bookmarks.save();
            self.selection = self.selection.saturating_sub(1);
            self.notify("Signet supprimé");
        }
    }

    // ----------------------------------------------------- écran de configuration

    /// Ouvre l'écran de configuration.
    ///
    /// Utilisé au lancement sans document : sans texte à lire, régler l'outil
    /// est la seule chose utile à faire.
    pub fn open_config(&mut self) {
        self.show_config = true;
        self.panel = Panel::None;
        self.player.pause();
        // Sans modèle, la rubrique utile est celle des modèles.
        if crate::models::first_installed().is_none() {
            self.screen.section = 0;
        }
    }

    pub fn toggle_config(&mut self) {
        self.show_config = !self.show_config;
        if self.show_config {
            self.panel = Panel::None;
            self.screen.editing = None;
            // On rouvre toujours sur le menu : c'est le point de repère.
            self.screen.page = Page::Menu;
        } else {
            self.stop_preview();
        }
    }

    pub fn config_move_section(&mut self, delta: i32) {
        self.screen.move_section(delta);
    }

    pub fn config_goto_section(&mut self, index: usize) {
        self.screen.goto_section(index);
    }

    /// Ouvre la rubrique sous le curseur, en plein écran.
    pub fn config_open_section(&mut self) {
        self.screen.open_section();
    }

    /// Revient au menu. Vrai si le retour a eu lieu.
    ///
    /// Permet à Échap de reculer d'un cran avant de fermer l'écran, plutôt
    /// que de tout refermer d'un coup.
    pub fn config_back_to_menu(&mut self) -> bool {
        self.screen.back_to_menu()
    }

    pub fn config_toggle_show_all(&mut self) {
        self.screen.toggle_show_all(&self.config);
        let hidden = self.screen.hidden_count(&self.config);
        self.screen.notice = if self.screen.show_all {
            "tous les réglages affichés, y compris ceux sans effet".to_string()
        } else if hidden > 0 {
            let pluriel = if hidden > 1 { "s" } else { "" };
            format!("{hidden} réglage{pluriel} sans effet masqué{pluriel}")
        } else {
            String::new()
        };
    }

    pub fn config_move_row(&mut self, delta: i32) {
        self.screen.move_row(delta, &self.config);
    }

    /// Ajuste le champ sélectionné et répercute son effet.
    pub fn config_adjust(&mut self, delta: i32) {
        let Some(field) = self.screen.current_field(&self.config) else { return };
        let effect = field.adjust(&mut self.config, delta);
        self.apply_effect(effect);
    }

    /// Entrée : édite un champ textuel, ou agit sur le modèle sélectionné.
    pub fn config_activate(&mut self) {
        if self.screen.current_section() == Section::Models {
            self.activate_model();
            return;
        }
        let Some(field) = self.screen.current_field(&self.config) else { return };
        if field.is_text(&self.config) {
            self.screen.editing = Some(field.raw_text(&self.config));
        } else {
            self.config_adjust(1);
        }
    }

    pub fn config_edit_push(&mut self, c: char) {
        if let Some(buffer) = self.screen.editing.as_mut() {
            buffer.push(c);
        }
    }

    pub fn config_edit_backspace(&mut self) {
        if let Some(buffer) = self.screen.editing.as_mut() {
            buffer.pop();
        }
    }

    pub fn config_commit_edit(&mut self) {
        let Some(buffer) = self.screen.editing.take() else { return };
        let Some(field) = self.screen.current_field(&self.config) else { return };
        self.config_set_text(field, buffer);
    }

    /// Pose le texte d'un réglage, avec ce qu'il entraîne.
    ///
    /// Une famille saisie à la main change le jeu de voix ; un chemin de
    /// modèle ou une famille changent le moteur lui-même ; une consigne ou un
    /// WAV de référence ne demandent qu'une resynthèse — ils partent avec
    /// chaque requête.
    pub fn config_set_text(&mut self, field: Field, value: String) {
        field.set_text(&mut self.config, value.trim().to_string());
        if field == Field::Family {
            self.config.align_voice_with_family();
        }
        self.apply_effect(match field {
            Field::ModelPath | Field::Family => Effect::ReloadEngine,
            _ => Effect::Resynthesize,
        });
    }

    pub fn config_cancel_edit(&mut self) {
        self.screen.editing = None;
    }

    /// Installe le modèle sélectionné, ou le choisit s'il est déjà là.
    fn activate_model(&mut self) {
        let Some(package) = self.screen.current_package() else {
            self.screen.notice =
                "aucun modèle disponible : binaire compilé sans audio.cpp".to_string();
            return;
        };
        if self.screen.download.is_some() {
            self.screen.notice = "un téléchargement est déjà en cours".to_string();
            return;
        }
        if models::is_installed(package) {
            self.select_model(package);
            return;
        }
        // Rien n'est téléchargé sans une action explicite de l'utilisateur.
        match models::Download::start(package) {
            Ok(download) => {
                self.screen.notice =
                    format!("téléchargement de {} …", package.display_name);
                self.screen.download = Some(download);
            }
            Err(err) => self.screen.notice = format!("téléchargement impossible : {err}"),
        }
    }

    /// Supprime le modèle sélectionné, ou annule son téléchargement.
    pub fn config_delete(&mut self) {
        if self.screen.current_section() != Section::Models {
            return;
        }
        if let Some(download) = self.screen.download.as_ref() {
            download.cancel();
            self.screen.notice = "annulation demandée".to_string();
            return;
        }
        let Some(package) = self.screen.current_package() else { return };
        if !models::is_installed(package) {
            return;
        }
        if !self.screen.confirm_delete(package) {
            self.screen.notice = format!(
                "supprimer {} ({}) ? d à nouveau pour confirmer",
                package.display_name,
                models::human_size(package.bytes)
            );
            return;
        }
        match models::remove(package) {
            Ok(()) => {
                self.screen.notice = format!("{} supprimé", package.display_name);
                // Le modèle courant vient de disparaître : on repasse au repli.
                if models::is_active_package(&self.config.engine.model_path, package) {
                    self.config.engine.model_path.clear();
                    self.config.engine.family.clear();
                    self.reload_engine();
                }
            }
            Err(err) => self.screen.notice = format!("suppression impossible : {err}"),
        }
    }

    /// Fait d'un modèle installé le moteur courant.
    fn select_model(&mut self, package: &'static models::ModelPackage) {
        let Some(path) = models::install_path(package) else { return };
        self.config.engine.model_path = path.to_string_lossy().into_owned();
        self.config.engine.family = package.family.to_string();
        self.config.engine.engine = "audio_cpp".to_string();
        // Le choix doit survivre au relancement : sans écriture, le premier
        // modèle installé revenait à la place de celui qu'on avait pris.
        self.config_dirty = true;
        // Le modèle change, ses voix aussi : celle qui était retenue n'existe
        // peut-être plus.
        let remplacée = self.config.align_voice_with_family();
        self.reload_engine();
        // Le moteur vient de changer : dire tout de suite comment le juger.
        self.screen.notice = if remplacée {
            let voix = Field::Voice.value(&self.config, 1.0);
            format!("{} sélectionné · voix {voix} — t pour l'écouter", package.display_name)
        } else {
            format!("{} sélectionné — t pour l'écouter", package.display_name)
        };
    }

    // ----------------------------------------------------------- aperçu de voix

    /// Fait entendre les réglages courants sur une phrase de démonstration.
    ///
    /// Sans cela on règle une voix qu'on n'a jamais entendue : `M3` ou `F1` ne
    /// disent rien du timbre, et le seul retour possible était de quitter
    /// l'écran pour relancer la lecture.
    pub fn preview_voice(&mut self) {
        // La touche répond toujours quelque chose : laisser tourner l'aperçu
        // précédent ferait passer un refus pour une touche sans effet.
        self.stop_preview();
        // Dans la liste des modèles, l'aperçu porte sur celui qui est chargé ;
        // faire parler un modèle qu'on n'a pas choisi serait un mensonge.
        if self.screen.current_section() == Section::Models {
            if let Some(package) = self.screen.current_package() {
                let loaded = models::install_path(package)
                    .is_some_and(|p| p.to_string_lossy() == self.config.engine.model_path);
                if !loaded {
                    self.screen.notice = if models::is_installed(package) {
                        format!(
                            "{} n'est pas chargé — Entrée pour le choisir, puis t pour l'écouter",
                            package.display_name
                        )
                    } else {
                        format!("{} n'est pas installé — Entrée pour l'installer", package.display_name)
                    };
                    return;
                }
            }
        }

        match self.request_preview() {
            Ok(()) => self.screen.notice.clear(),
            Err(err) => self.screen.notice = err,
        }
    }

    /// L'aperçu tel que l'app le demande : les réglages en cours sur la
    /// phrase d'exemple, la lecture mise en pause, le précédent coupé. Rend
    /// la raison d'un refus.
    pub fn request_preview(&mut self) -> Result<(), String> {
        let language = self.builder.language().to_string();
        self.request_preview_in(&language)
    }

    /// L'aperçu dans une langue donnée : chaque voix de l'onglet Voix
    /// s'écoute dans la sienne.
    pub fn request_preview_in(&mut self, language: &str) -> Result<(), String> {
        self.preview_error = None;
        let label = self.preview_label();
        preview::start(
            &mut self.previewer,
            &mut self.player,
            self.synth.requires_reference(),
            &self.config,
            language,
            label,
        )
    }

    /// Un aperçu se synthétise.
    pub fn preview_pending(&self) -> bool {
        self.previewer.pending_for().is_some()
    }

    /// Un aperçu sonne.
    pub fn preview_playing(&self) -> bool {
        self.player.preview_playing()
    }

    /// Décrit ce que l'aperçu fait entendre.
    fn preview_label(&self) -> String {
        format!(
            "{} · {:.2}×",
            Field::Voice.value(&self.config, 1.0),
            self.config.voice.speed
        )
    }

    pub fn stop_preview(&mut self) {
        self.previewer.cancel();
        self.player.stop_preview();
    }

    /// Ce que l'aperçu fait à cet instant, s'il fait quelque chose.
    ///
    /// La synthèse dure une seconde ou deux : sans ce retour, `t` semblerait
    /// n'avoir rien déclenché.
    pub fn preview_status(&self) -> Option<String> {
        // Le moteur est nommé à l'affichage : pendant un chargement différé, il
        // passe de « chargement… » à son nom réel sous les yeux.
        if let Some(elapsed) = self.previewer.pending_for() {
            return Some(format!(
                "♪  {} · {} — synthèse de l'aperçu… {:.1} s",
                self.previewer.label(),
                self.engine_name(),
                elapsed.as_secs_f32()
            ));
        }
        self.player.preview_playing().then(|| {
            format!(
                "♪  {} · {} — écoute",
                self.previewer.label(),
                self.engine_name()
            )
        })
    }

    /// Joue l'aperçu dès qu'il est prêt.
    fn poll_preview(&mut self) {
        let Some(outcome) = self.previewer.poll() else { return };
        match outcome {
            Ok(audio) => self.player.play_preview(&audio),
            Err(err) => {
                let message = format!("aperçu impossible : {err}");
                self.preview_error = Some(message.clone());
                self.screen.notice = message;
            }
        }
    }

    fn poll_download(&mut self) {
        if let Some(package) = self.screen.poll_download() {
            self.select_model(package);
        }
    }

    /// Répercute un changement de réglage sur la lecture.
    pub fn apply_effect(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Volume(delta) => self.change_volume(delta),
            Effect::Speed => {
                let speed = self.config.voice.speed;
                self.player.set_speed(speed);
            }
            Effect::Resynthesize => {
                self.apply_tts_params();
                let current = self.current_sentence();
                self.seek_to(current);
            }
            Effect::ReloadEngine => self.reload_engine(),
            Effect::Reparse => {
                self.notify("Découpage modifié — effet au prochain lancement");
            }
        }
    }

    /// Reconstruit le moteur et le pipeline de synthèse.
    ///
    /// Utilisé au changement de modèle, de backend ou de nombre de sessions.
    /// L'ancien ordonnanceur est libéré hors du thread d'interface : sa
    /// destruction attend la fin des synthèses en cours, ce qui figerait
    /// l'affichage pendant une seconde ou deux.
    pub fn reload_engine(&mut self) {
        let (engine, notice) =
            tts::build_engine(self.config.engine_preference(), &self.config.engine_settings());
        if let Some(notice) = notice {
            self.notices.push(notice);
        }
        let synth = Synthesizer::new(
            engine,
            self.synthesis_params(),
            self.config.sessions() as usize,
            self.config.engine.prefetch as usize,
        );
        for sentence in &self.builder.document().sentences {
            synth.push(sentence.text.clone(), sentence.trailing_pause_ms);
        }
        self.published = self.builder.sentence_count();
        // La cadence mesurée valait pour l'ancien moteur, et une phrase qu'il
        // avait refusée mérite sa chance avec le nouveau.
        self.pace = Pace::default();
        self.skipped.clear();

        let previous = std::mem::replace(&mut self.synth, synth);
        std::thread::spawn(move || drop(previous));

        // L'aperçu doit parler avec le nouveau moteur, pas avec l'ancien. Son
        // arrêt attend la synthèse en cours : hors du thread d'interface, donc.
        self.stop_preview();
        let previous = std::mem::replace(&mut self.previewer, Previewer::new(self.synth.engine()));
        std::thread::spawn(move || drop(previous));

        self.player.flush();
        let current = self.current_sentence();
        self.queue_head = current;
        self.synth.set_cursor(current);
        self.notify(&format!("Moteur : {}", self.synth.engine_name()));
    }

    pub fn save_config(&mut self) {
        match self.config.save() {
            Ok(path) => {
                self.config_dirty = false;
                self.notify(&format!("Configuration écrite : {}", path.display()));
            }
            Err(err) => self.notify(&format!("Écriture impossible : {err}")),
        }
    }

    /// Vrai quand un choix fait à l'écran n'est pas encore sur le disque.
    pub fn config_needs_saving(&self) -> bool {
        self.config_dirty
    }

    /// Écrit la configuration si un choix l'attend, sans rien dire.
    pub fn flush_config(&mut self) {
        if self.config_dirty && self.config.save().is_ok() {
            self.config_dirty = false;
        }
    }

    /// Ce que la fermeture doit dire, si elle ne peut pas être immédiate.
    ///
    /// Quitter attend la synthèse en cours : sur un modèle lent, plusieurs
    /// secondes de terminal muet, qu'on prend pour un blocage.
    pub fn closing_notice(&self) -> Option<String> {
        let stats = self.synth.stats();
        (stats.in_flight > 0).then(|| {
            format!(
                "sillon : une synthèse se termine avant de rendre la main ({})…",
                self.engine_name()
            )
        })
    }

    // ----------------------------------------------------------------- statut

    pub fn notify(&mut self, message: &str) {
        self.status = message.to_string();
        self.status_until = Some(Instant::now() + Duration::from_secs(4));
    }

    pub fn status(&self) -> &str {
        &self.status
    }
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
        for block in crate::input::split_blocks(markdown) {
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
    fn a_speed_change_keeps_the_synthesized_audio_and_reaches_the_player() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::input::split_blocks(DOC) {
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
        assert!(wait_for(|| app.synth.stats().ready >= 3));
        let before = calls.load(std::sync::atomic::Ordering::SeqCst);

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
        for block in crate::input::split_blocks(DOC) {
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
        for block in crate::input::split_blocks(DOC) {
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
    fn a_text_setting_reloads_the_engine_only_when_it_names_the_model() {
        let mut app = app_reading(DOC);
        let engine = app.synth.engine();
        app.config_set_text(Field::Instruction, "  lentement ".to_string());
        assert_eq!(app.config.voice.instruction, "lentement");
        assert!(Arc::ptr_eq(&engine, &app.synth.engine()), "une consigne ne recharge rien");
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
        app.bookmarks.toggle("test.md", 1, 0, "Titre", "x");

        app.adopt_former_identity("test.md");
        app.resume_where_left();

        assert_eq!(app.current_sentence(), 2);
        assert!(app.bookmarks.contains("/docs/test.md", 1));
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

    #[test]
    fn choosing_a_model_asks_for_the_configuration_to_be_written() {
        // Le choix ne survivait pas au relancement : seul `w` écrivait, et le
        // premier modèle installé revenait à la place de celui qu'on avait pris.
        let mut app = app_reading(DOC);
        assert!(!app.config_needs_saving());
        app.select_model(&models::CATALOG[0]);
        assert!(app.config_needs_saving());
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
        for block in crate::input::split_blocks(DOC) {
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
}
