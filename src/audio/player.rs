//! Lecture audio et position réelle dans le document.
//!
//! La position n'est pas estimée : elle est déduite de ce que la carte son a
//! effectivement consommé. `rodio` ne dit pas quelle source joue, mais il donne
//! la taille de sa file ; en tenant un miroir de cette file, on sait quelle
//! phrase sonne, et on redémarre un chronomètre à chaque frontière. L'erreur est
//! donc bornée par un tick d'interface, et remise à zéro à chaque phrase — au
//! lieu de dériver indéfiniment.

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::tts::PcmBuffer;

/// Une phrase présente dans la file de sortie.
#[derive(Debug, Clone)]
struct Queued {
    sentence_id: usize,
    total: Duration,
    speech: Duration,
}

/// La phrase qui sonne actuellement.
#[derive(Debug, Clone)]
struct Active {
    sentence_id: usize,
    total: Duration,
    speech: Duration,
    /// Temps accumulé avant la dernière reprise.
    accumulated: Duration,
    resumed_at: Option<Instant>,
}

impl Active {
    fn elapsed(&self) -> Duration {
        match self.resumed_at {
            Some(t) => self.accumulated + t.elapsed(),
            None => self.accumulated,
        }
    }

    fn pause(&mut self) {
        if let Some(t) = self.resumed_at.take() {
            self.accumulated += t.elapsed();
        }
    }

    fn resume(&mut self) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(Instant::now());
        }
    }
}

/// Où en est la lecture, du point de vue du son.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackPosition {
    pub sentence_id: usize,
    /// Temps écoulé dans la parole de la phrase.
    pub elapsed: Duration,
    /// Durée de la parole, silence de fin exclu.
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
    error: Option<String>,
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
            error,
        }
    }

    /// Lecteur sans sortie audio, pour les essais.
    ///
    /// Ouvrir un vrai flux dans chaque test bloque la suite dès que deux
    /// s'ouvrent de front : la carte son n'est pas partageable ainsi.
    #[cfg(test)]
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
            error: Some("sortie audio désactivée".to_string()),
        }
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

    /// Empile une phrase à jouer.
    pub fn enqueue(&mut self, sentence_id: usize, audio: &Arc<PcmBuffer>, speech: Duration) {
        let Some(sink) = &self.sink else { return };
        sink.append(SamplesBuffer::new(
            audio.channels,
            audio.sample_rate,
            audio.samples.to_vec(),
        ));
        self.queued.push_back(Queued {
            sentence_id,
            total: audio.duration,
            speech,
        });
        if self.playing {
            sink.play();
        }
    }

    /// Synchronise l'état interne sur la file réelle de `rodio`.
    ///
    /// Renvoie les phrases dont la lecture vient de se terminer.
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
                finished.push(done.sentence_id);
            }
        }

        match self.queued.front() {
            Some(front) => {
                let changed = self.active.as_ref().map(|a| a.sentence_id) != Some(front.sentence_id);
                if changed {
                    self.active = Some(Active {
                        sentence_id: front.sentence_id,
                        total: front.total,
                        speech: front.speech,
                        accumulated: Duration::ZERO,
                        resumed_at: self.playing.then(Instant::now),
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
            elapsed: a.elapsed().min(a.total),
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
            active.resume();
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
        if let Some(sink) = &self.sink {
            sink.pause();
        }
        if let Some(active) = self.active.as_mut() {
            active.pause();
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
                sink.append(SamplesBuffer::new(
                    audio.channels,
                    audio.sample_rate,
                    audio.samples.to_vec(),
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

    fn active(speech_ms: u64) -> Active {
        Active {
            sentence_id: 0,
            total: Duration::from_millis(speech_ms),
            speech: Duration::from_millis(speech_ms),
            accumulated: Duration::ZERO,
            resumed_at: Some(Instant::now()),
        }
    }

    #[test]
    fn timer_freezes_while_paused() {
        let mut a = active(1000);
        std::thread::sleep(Duration::from_millis(30));
        a.pause();
        let frozen = a.elapsed();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(a.elapsed(), frozen, "le chronomètre a avancé en pause");
    }

    #[test]
    fn timer_resumes_where_it_stopped() {
        let mut a = active(1000);
        std::thread::sleep(Duration::from_millis(30));
        a.pause();
        let paused_at = a.elapsed();
        a.resume();
        std::thread::sleep(Duration::from_millis(30));
        assert!(a.elapsed() > paused_at);
        // La reprise ne repart pas de zéro.
        assert!(a.elapsed() >= paused_at);
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
