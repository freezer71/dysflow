//! Lecture audio et position réelle dans le document.
//!
//! La position n'est pas estimée : elle est déduite de ce que la carte son a
//! effectivement consommé. `rodio` ne dit pas quelle source joue, mais il donne
//! la taille de sa file ; en tenant un miroir de cette file, on sait quelle
//! phrase sonne, et on redémarre un chronomètre à chaque frontière. L'erreur est
//! donc bornée par un tick d'interface, et remise à zéro à chaque phrase — au
//! lieu de dériver indéfiniment.

use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::tempo::{SpeedControl, Tempo};
use crate::tts::PcmBuffer;

/// Une phrase présente dans la file de sortie.
#[derive(Debug, Clone)]
struct Queued {
    sentence_id: usize,
    /// Numéro de la partie dans la file : une phrase coupée y entre en deux
    /// parties, que seul ce numéro distingue.
    seq: u64,
    total: Duration,
    speech: Duration,
    /// Parole déjà passée au moment où la phrase entre en file : un saut au
    /// mot la fait commencer en son milieu.
    offset: Duration,
}

/// La phrase qui sonne actuellement.
///
/// Son chronomètre compte en temps **de synthèse** (à 1×) : une seconde
/// d'horloge à 2× avance la phrase de deux secondes. Durées de la phrase et
/// position restent ainsi dans la même unité, quelle que soit la vitesse, et
/// le surlignage des mots tombe juste.
#[derive(Debug, Clone)]
struct Active {
    sentence_id: usize,
    /// Numéro de la partie qui sonne.
    seq: u64,
    total: Duration,
    speech: Duration,
    /// Temps de synthèse accumulé avant la dernière reprise.
    accumulated: Duration,
    resumed_at: Option<Instant>,
    /// Vitesse en vigueur depuis `resumed_at`.
    speed: f32,
}

/// L'horloge est passée en paramètre (`now`) : les essais la posent, au lieu
/// de dormir et d'espérer que la machine suive.
impl Active {
    fn elapsed_at(&self, now: Instant) -> Duration {
        match self.resumed_at {
            Some(t) => self.accumulated + now.saturating_duration_since(t).mul_f32(self.speed),
            None => self.accumulated,
        }
    }

    fn pause_at(&mut self, now: Instant) {
        if let Some(t) = self.resumed_at.take() {
            self.accumulated += now.saturating_duration_since(t).mul_f32(self.speed);
        }
    }

    /// Change de vitesse sans perdre ce qui a déjà été entendu.
    fn set_speed_at(&mut self, speed: f32, now: Instant) {
        if let Some(t) = self.resumed_at {
            self.accumulated += now.saturating_duration_since(t).mul_f32(self.speed);
            self.resumed_at = Some(now);
        }
        self.speed = speed;
    }

    fn resume_at(&mut self, now: Instant) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(now);
        }
    }
}

/// Où en est la lecture, du point de vue du son.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackPosition {
    pub sentence_id: usize,
    /// Temps écoulé dans la parole de la phrase, en temps de synthèse (1×).
    pub elapsed: Duration,
    /// Durée de la parole à 1×, silence de fin exclu.
    pub speech: Duration,
}

pub struct Player {
    // Le flux doit rester vivant tant qu'on joue.
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    /// Puits distinct pour les aperçus de voix.
    ///
    /// Les faire passer par la file de lecture décalerait la position et
    /// ferait perdre sa place ; un second puits les garde hors du document.
    preview: Option<Sink>,
    queued: VecDeque<Queued>,
    active: Option<Active>,
    playing: bool,
    volume: f32,
    muted: bool,
    /// Vitesse lue par chaque phrase en file, à chaque bloc de 5 ms.
    speed: SpeedControl,
    /// Fréquence de la carte son : les phrases y sont converties avant le
    /// mélangeur de rodio, qui sinon perd des échantillons. `None` sans
    /// sortie connue : la fréquence de la synthèse est gardée.
    output_rate: Option<u32>,
    error: Option<String>,
    /// Dernier numéro de partie donné.
    next_seq: u64,
}

/// Fréquence que rodio choisira pour la sortie par défaut : celle de la
/// configuration par défaut du périphérique (`OutputStream::try_default`).
fn default_output_rate() -> Option<u32> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let device = rodio::cpal::default_host().default_output_device()?;
    Some(device.default_output_config().ok()?.sample_rate().0)
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        let (stream, handle, sink, error) = match OutputStream::try_default() {
            Ok((stream, handle)) => match Sink::try_new(&handle) {
                Ok(sink) => (Some(stream), Some(handle), Some(sink), None),
                Err(err) => (Some(stream), None, None, Some(err.to_string())),
            },
            Err(err) => (None, None, None, Some(err.to_string())),
        };
        // La file démarre en pause : rien ne doit sortir avant le premier
        // buffer, sans quoi la position partirait avant le son.
        if let Some(sink) = &sink {
            sink.pause();
        }
        let output_rate = stream.as_ref().and_then(|_| default_output_rate());
        // Le fil qui libère les phrases finies naît ici, pas dans le fil audio.
        super::tempo::prepare();
        Self {
            _stream: stream,
            handle,
            sink,
            preview: None,
            queued: VecDeque::new(),
            active: None,
            playing: false,
            volume: 1.0,
            muted: false,
            speed: SpeedControl::default(),
            output_rate,
            error,
            next_seq: 0,
        }
    }

    /// Lecteur sans sortie audio, pour les essais.
    ///
    /// Ouvrir un vrai flux dans chaque test bloque la suite dès que deux
    /// s'ouvrent de front : la carte son n'est pas partageable ainsi. Sert
    /// aussi au mode de test de l'app.
    pub fn silent() -> Self {
        Self {
            _stream: None,
            handle: None,
            sink: None,
            preview: None,
            queued: VecDeque::new(),
            active: None,
            playing: false,
            volume: 1.0,
            muted: false,
            speed: SpeedControl::default(),
            output_rate: None,
            error: Some("sortie audio désactivée".to_string()),
            next_seq: 0,
        }
    }

    /// Fait comme si une phrase sonnait déjà depuis un moment.
    ///
    /// Sans sortie audio, aucune position n'existe : les essais qui portent
    /// sur la progression dans la phrase doivent la poser eux-mêmes.
    #[cfg(test)]
    pub fn simulate(&mut self, sentence_id: usize, elapsed: Duration, speech: Duration) {
        self.active = Some(Active {
            sentence_id,
            seq: 0,
            total: speech,
            speech,
            accumulated: elapsed,
            resumed_at: None,
            speed: self.speed.get(),
        });
    }

    /// Lecteur relié à une file sans carte son, que l'essai vide lui-même au
    /// rythme qu'il veut — comme le ferait la sortie audio.
    #[cfg(test)]
    pub fn idle(output_rate: u32) -> (Self, rodio::queue::SourcesQueueOutput<f32>) {
        let (sink, output) = Sink::new_idle();
        sink.pause();
        let mut player = Self::silent();
        player.error = None;
        player.sink = Some(sink);
        player.output_rate = Some(output_rate);
        (player, output)
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_available(&self) -> bool {
        self.sink.is_some()
    }

    /// Nombre de phrases encore dans la file de sortie.
    pub fn queue_len(&self) -> usize {
        self.queued.len()
    }

    /// Les parties en file : phrase et position de départ dans sa parole.
    #[cfg(test)]
    pub fn queued_parts(&self) -> Vec<(usize, Duration)> {
        self.queued.iter().map(|q| (q.sentence_id, q.offset)).collect()
    }

    /// Empile une phrase à jouer depuis son début.
    pub fn enqueue(&mut self, sentence_id: usize, audio: &Arc<PcmBuffer>, speech: Duration) {
        self.enqueue_from(sentence_id, audio, speech, Duration::ZERO);
    }

    /// Empile une phrase à jouer à partir de `offset` dans sa parole.
    ///
    /// La position rendue compte depuis le début de la phrase : le
    /// surlignage reste juste, comme si le début avait été entendu.
    pub fn enqueue_from(
        &mut self,
        sentence_id: usize,
        audio: &Arc<PcmBuffer>,
        speech: Duration,
        offset: Duration,
    ) {
        self.enqueue_part(sentence_id, audio, speech, Duration::ZERO, offset);
    }

    /// Empile une partie de phrase : `base` est la parole de la phrase qui
    /// la précède (la tête, pour une suite), `skip` ce qu'on passe dans
    /// cette partie. Le chronomètre de la phrase part de `base + skip`.
    ///
    /// Les parties d'une même phrase passent chacune par leur propre
    /// `Tempo`, bout à bout dans la file : à 1×, la suite commence à
    /// l'échantillon qui suit la tête, sans silence ni raccord ajouté.
    pub fn enqueue_part(
        &mut self,
        sentence_id: usize,
        audio: &Arc<PcmBuffer>,
        part_speech: Duration,
        base: Duration,
        skip: Duration,
    ) {
        let Some(sink) = &self.sink else { return };
        let skip = skip.min(part_speech);
        // Le son reste celui de la synthèse, à 1× ; la vitesse s'applique en
        // le jouant, et peut changer en route.
        sink.append(Tempo::new(
            Arc::clone(&audio.samples),
            audio.channels,
            audio.sample_rate,
            self.output_rate.unwrap_or(audio.sample_rate),
            audio.sample_index(skip),
            audio.sample_index(part_speech),
            self.speed.clone(),
        ));
        self.next_seq += 1;
        self.queued.push_back(Queued {
            sentence_id,
            seq: self.next_seq,
            total: base + audio.duration,
            speech: base + part_speech,
            offset: base + skip,
        });
        if self.playing {
            sink.play();
        }
    }

    /// Synchronise l'état interne sur la file réelle de `rodio`.
    ///
    /// Renvoie les phrases dont la lecture vient de se terminer. Une tête
    /// suivie de sa suite en file ne termine pas sa phrase.
    pub fn reconcile(&mut self) -> Vec<usize> {
        if self.preview.as_ref().is_some_and(|p| p.empty()) {
            self.preview = None;
        }
        let Some(sink) = &self.sink else {
            return Vec::new();
        };
        let remaining = sink.len();
        let mut finished = Vec::new();
        while self.queued.len() > remaining {
            if let Some(done) = self.queued.pop_front() {
                let continues = self.queued.front().is_some_and(|q| q.sentence_id == done.sentence_id);
                if !continues {
                    finished.push(done.sentence_id);
                }
            }
        }

        match self.queued.front() {
            Some(front) => {
                let changed = self.active.as_ref().map(|a| a.seq) != Some(front.seq);
                if changed {
                    self.active = Some(Active {
                        sentence_id: front.sentence_id,
                        seq: front.seq,
                        total: front.total,
                        speech: front.speech,
                        accumulated: front.offset,
                        resumed_at: self.playing.then(Instant::now),
                        speed: self.speed.get(),
                    });
                }
            }
            None => self.active = None,
        }
        finished
    }

    pub fn position(&self) -> Option<PlaybackPosition> {
        self.active.as_ref().map(|a| PlaybackPosition {
            sentence_id: a.sentence_id,
            elapsed: a.elapsed_at(Instant::now()).min(a.total),
            speech: a.speech,
        })
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        self.playing = true;
        if let Some(sink) = &self.sink {
            sink.play();
        }
        if let Some(active) = self.active.as_mut() {
            active.resume_at(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
        if let Some(sink) = &self.sink {
            sink.pause();
        }
        if let Some(active) = self.active.as_mut() {
            active.pause_at(Instant::now());
        }
    }

    pub fn toggle(&mut self) -> bool {
        if self.playing {
            self.pause();
        } else {
            self.play();
        }
        self.playing
    }

    /// Vide la file de sortie et repart d'une file neuve.
    ///
    /// `Sink::stop` condamne définitivement la file et `Sink::clear` bloque le
    /// thread appelant ; recréer la file est la seule opération à la fois
    /// immédiate et réutilisable — c'est la primitive de tous les sauts.
    pub fn flush(&mut self) {
        self.queued.clear();
        self.active = None;
        let Some(handle) = &self.handle else { return };
        // L'ancienne file arrête ses sources en étant libérée.
        match Sink::try_new(handle) {
            Ok(sink) => {
                sink.set_volume(self.effective_volume());
                if self.playing {
                    sink.play();
                } else {
                    sink.pause();
                }
                self.sink = Some(sink);
            }
            Err(err) => {
                self.error = Some(err.to_string());
                self.sink = None;
            }
        }
    }

    /// Joue un extrait hors de la file de lecture.
    ///
    /// Remplace l'aperçu précédent : enchaîner deux essais fait entendre le
    /// second, pas les deux à la fois.
    pub fn play_preview(&mut self, audio: &PcmBuffer) {
        let Some(handle) = &self.handle else { return };
        match Sink::try_new(handle) {
            Ok(sink) => {
                sink.set_volume(self.effective_volume());
                // L'aperçu dit la vérité sur la vitesse : il passe par le
                // même étirement que la lecture.
                sink.append(Tempo::new(
                    Arc::clone(&audio.samples),
                    audio.channels,
                    audio.sample_rate,
                    self.output_rate.unwrap_or(audio.sample_rate),
                    0,
                    audio.samples.len(),
                    self.speed.clone(),
                ));
                sink.play();
                self.preview = Some(sink);
            }
            Err(err) => self.error = Some(err.to_string()),
        }
    }

    pub fn stop_preview(&mut self) {
        self.preview = None;
    }

    /// Vrai tant que l'extrait sonne.
    pub fn preview_playing(&self) -> bool {
        self.preview.as_ref().is_some_and(|sink| !sink.empty())
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn speed(&self) -> f32 {
        self.speed.get()
    }

    /// Change la vitesse de lecture, phrase en cours comprise.
    ///
    /// Rien n'est resynthétisé ni vidé : chaque phrase en file lit la vitesse
    /// à son prochain bloc de 5 ms. Le chronomètre de la phrase change de pas
    /// au même instant, pour que le surlignage suive.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed.set(speed);
        let applied = self.speed.get();
        if let Some(active) = self.active.as_mut() {
            active.set_speed_at(applied, Instant::now());
        }
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_volume(&mut self, value: f32) {
        self.volume = value.clamp(0.0, 2.0);
        self.apply_volume();
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        self.apply_volume();
        self.muted
    }

    fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }

    fn apply_volume(&self) {
        let volume = self.effective_volume();
        if let Some(sink) = &self.sink {
            sink.set_volume(volume);
        }
        // Régler le volume pendant un aperçu doit s'entendre sur l'aperçu.
        if let Some(preview) = &self.preview {
            preview.set_volume(volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// Une phrase qui sonne depuis `t0`, à 1×.
    fn active(speech_ms: u64, t0: Instant) -> Active {
        Active {
            sentence_id: 0,
            seq: 0,
            total: MS(speech_ms),
            speech: MS(speech_ms),
            accumulated: Duration::ZERO,
            resumed_at: Some(t0),
            speed: 1.0,
        }
    }

    #[test]
    fn timer_freezes_while_paused() {
        let t0 = Instant::now();
        let mut a = active(1000, t0);
        a.pause_at(t0 + MS(30));
        assert_eq!(a.elapsed_at(t0 + MS(30)), MS(30));
        assert_eq!(a.elapsed_at(t0 + MS(70)), MS(30), "le chronomètre a avancé en pause");
    }

    #[test]
    fn timer_resumes_where_it_stopped() {
        let t0 = Instant::now();
        let mut a = active(1000, t0);
        a.pause_at(t0 + MS(30));
        a.resume_at(t0 + MS(100));
        // La reprise ne repart pas de zéro, et ne compte pas la pause.
        assert_eq!(a.elapsed_at(t0 + MS(130)), MS(60));
    }

    #[test]
    fn the_timer_counts_synthesis_time_at_the_playback_speed() {
        // À 2×, une seconde d'horloge fait entendre deux secondes de parole
        // synthétisée : le surlignage doit avancer d'autant.
        let t0 = Instant::now();
        let mut a = active(10_000, t0);
        a.speed = 2.0;
        assert_eq!(a.elapsed_at(t0 + MS(100)), MS(200));
        a.pause_at(t0 + MS(100));
        assert_eq!(a.elapsed_at(t0 + MS(500)), MS(200));
    }

    #[test]
    fn a_speed_change_keeps_what_was_already_heard() {
        let t0 = Instant::now();
        let mut a = active(10_000, t0);
        a.set_speed_at(3.0, t0 + MS(60));
        assert_eq!(a.elapsed_at(t0 + MS(60)), MS(60));
        // 60 ms à 1×, puis 60 ms à 3× : 240 ms de parole.
        assert_eq!(a.elapsed_at(t0 + MS(120)), MS(240));
        // En pause, un changement de vitesse ne fait rien avancer.
        a.pause_at(t0 + MS(120));
        a.set_speed_at(0.5, t0 + MS(200));
        assert_eq!(a.elapsed_at(t0 + MS(300)), MS(240));
    }

    /// Du lecteur jusqu'au son : la file est vidée comme le ferait la carte
    /// son, et la vitesse change en pleine phrase.
    #[test]
    fn a_speed_change_reaches_the_output_without_flushing_the_queue() {
        use crate::audio::sonic::tests::{speech_like, RATE};
        // Sortie à 24 kHz comme la synthèse : on compare au son d'origine.
        let (mut player, mut output) = Player::idle(RATE);
        let speech = speech_like(3.0);
        let audio = Arc::new(PcmBuffer::new(speech.clone(), RATE, 1));
        player.enqueue(0, &audio, audio.duration);
        player.enqueue(1, &audio, audio.duration);
        player.play();

        let switch_at = RATE as usize;
        let mut out: Vec<f32> = output.by_ref().take(switch_at).collect();
        player.set_speed(2.0);
        assert_eq!(player.queue_len(), 2, "rien n'a été vidé");
        out.extend(output.by_ref().take(RATE as usize * 2));

        let diverges = (switch_at..out.len())
            .find(|&i| (out[i] - speech[i]).abs() > 2e-3)
            .expect("la vitesse n'a jamais changé");
        let latency_ms = (diverges - switch_at) as f64 * 1000.0 / RATE as f64;
        eprintln!("lecteur : changement de vitesse entendu après {latency_ms:.2} ms de son");
        assert!(latency_ms <= 10.0, "{latency_ms} ms");
    }

    #[test]
    fn the_tail_of_a_sentence_continues_its_clock() {
        const RATE: u32 = 1000;
        let (mut player, mut output) = Player::idle(RATE);
        let head = Arc::new(PcmBuffer::new(vec![0.1; 1000], RATE, 1));
        let tail = Arc::new(PcmBuffer::new(vec![0.1; 500], RATE, 1));
        player.enqueue_part(0, &head, Duration::from_secs(1), Duration::ZERO, Duration::ZERO);
        player.enqueue_part(0, &tail, Duration::from_millis(500), Duration::from_secs(1), Duration::ZERO);
        player.play();
        player.reconcile();
        assert_eq!(player.position().unwrap().speech, Duration::from_secs(1));
        // La tête sort entière : la suite prend la main, au même mot près.
        let _: Vec<f32> = output.by_ref().take(1200).collect();
        player.reconcile();
        let p = player.position().expect("la suite sonne");
        assert_eq!(p.sentence_id, 0);
        assert!(p.elapsed >= Duration::from_secs(1), "{:?}", p.elapsed);
        assert_eq!(p.speech, Duration::from_millis(1500));
    }

    #[test]
    fn head_and_tail_join_without_a_gap_and_end_the_sentence_once() {
        const RATE: u32 = 1000;
        let (mut player, mut output) = Player::idle(RATE);
        let head: Vec<f32> = (0..1000).map(|i| 0.2 + i as f32 * 1e-4).collect();
        let tail: Vec<f32> = (0..500).map(|i| -0.2 - i as f32 * 1e-4).collect();
        let head_audio = Arc::new(PcmBuffer::new(head.clone(), RATE, 1));
        let tail_audio = Arc::new(PcmBuffer::new(tail.clone(), RATE, 1));
        player.enqueue_part(0, &head_audio, head_audio.duration, Duration::ZERO, Duration::ZERO);
        player.enqueue_part(0, &tail_audio, tail_audio.duration, head_audio.duration, Duration::ZERO);
        player.play();
        player.reconcile();

        // Sonic, à 1×, rend le son à sa quantification près.
        let close = |heard: &[f32], expected: &[f32]| {
            heard.len() == expected.len() && heard.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3)
        };
        let heard: Vec<f32> = output.by_ref().take(1000).collect();
        assert!(close(&heard, &head), "la tête telle que synthétisée");
        assert!(player.reconcile().is_empty(), "la phrase n'est pas finie : sa suite sonne");
        // Pas un échantillon de silence ni de raccord entre les deux.
        let heard: Vec<f32> = output.by_ref().take(500).collect();
        assert!(close(&heard, &tail), "la suite dès l'échantillon suivant : {:?}", &heard[..4]);
        let _: Vec<f32> = output.by_ref().take(10).collect();
        assert_eq!(player.reconcile(), vec![0], "finie une fois, à la fin de sa suite");
    }

    #[test]
    fn player_without_output_stays_inert() {
        // Sur une machine sans carte son, le lecteur ne doit pas paniquer.
        let mut player = Player::new();
        if player.is_available() {
            return;
        }
        assert!(player.error().is_some());
        player.play();
        assert!(player.position().is_none());
        assert_eq!(player.reconcile(), Vec::<usize>::new());
    }
}
