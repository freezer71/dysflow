//! Le fil de la lecture : entrée, synthèse, sortie, déplacements, vitesse et volume.

use super::*;

impl App {
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
        self.announce_unavailable_voice();
    }

    /// Raison pour laquelle la voix ne peut rien dire : l'ouverture d'une
    /// session de synthèse a échoué (une famille qui ne sait ni synthétiser
    /// ni cloner hors ligne). Chaque phrase échoue alors et se saute.
    pub fn voice_unavailable(&self) -> Option<String> {
        self.synth.open_failure()
    }

    /// Tant que la voix est indisponible, le statut le dit, sans délai :
    /// sans cela le lecteur sautait toutes les phrases en silence.
    pub(super) fn announce_unavailable_voice(&mut self) {
        match self.voice_unavailable() {
            Some(reason) => {
                let message = format!("{VOICE_UNAVAILABLE}{reason}");
                if self.status != message {
                    self.status = message;
                    self.status_until = None;
                }
            }
            None if self.status.starts_with(VOICE_UNAVAILABLE) => self.status.clear(),
            None => {}
        }
    }

    pub(super) fn drain_input(&mut self) {
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

    /// Transmet à la synthèse les phrases apparues depuis le dernier tour,
    /// avec leurs mots : un moteur qui sait minuter les mots s'en sert.
    pub(super) fn publish_new_sentences(&mut self) {
        let total = self.builder.sentence_count();
        while self.published < total {
            let (text, words, written, pause) = {
                let sentence = &self.builder.document().sentences[self.published];
                (sentence.text.clone(), sentence.spoken_words(), sentence.written_words(), sentence.trailing_pause_ms)
            };
            self.synth.push_sentence(text, words, written, pause);
            self.published += 1;
        }
    }

    pub(super) fn advance_playback(&mut self) {
        let mut finished = self.player.reconcile();
        // Une tête jouée dont la suite n'est pas encore là ne termine pas sa
        // phrase : son minutage et la place de lecture restent.
        let waiting = self.awaiting_tail.map(|w| w.sentence);
        finished.retain(|&id| Some(id) != waiting);
        for id in &finished {
            self.timings.remove(id);
            self.split_parts.remove(id);
        }
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
    /// Deux parties en file suffisent à enchaîner sans blanc, tout en gardant
    /// la possibilité de sauter avec une latence imperceptible. Une phrase
    /// coupée entre en deux fois : sa tête dès qu'elle est prête, puis sa
    /// suite, au même chronomètre.
    pub(super) fn fill_output(&mut self) {
        if !self.player.is_available() {
            return;
        }
        while self.player.queue_len() < 2 {
            if let Some(waiting) = self.awaiting_tail {
                if let Some(tail) = self.synth.tail(waiting.sentence) {
                    self.enqueue_tail(waiting, &tail);
                    continue;
                }
                if self.synth.failure(waiting.sentence).is_some() {
                    // La tête a pu sonner, la phrase reste à moitié muette :
                    // elle se compte, et la lecture passe à la suivante.
                    let id = waiting.sentence;
                    self.awaiting_tail = None;
                    self.skipped.insert(id);
                    self.queue_head = id + 1;
                    if self.player.position().is_none() {
                        self.timings.remove(&id);
                        self.split_parts.remove(&id);
                        self.last_known = id + 1;
                    }
                    continue;
                }
                break;
            }
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
                if let crate::tts::pool::Part::Head { words: head_words } = rendered.part {
                    self.enqueue_head(head_words, &rendered);
                    continue;
                }
                self.pace.record(rendered.speech.as_secs_f64(), words);
                let offset = match self.start_at.as_mut() {
                    Some(start) if start.sentence == self.queue_head && !start.consumed => {
                        start.consumed = true;
                        start_offset(start, &rendered)
                    }
                    _ => Duration::ZERO,
                };
                if let Some(spans) = &rendered.words {
                    self.timings.insert(self.queue_head, Arc::clone(spans));
                }
                self.player.enqueue_from(self.queue_head, &rendered.audio, rendered.speech, offset);
                self.starved = false;
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
        let reading = self.player.is_playing() && self.queue_head < self.builder.sentence_count();
        if reading && self.player.queue_len() == 0 && !self.starved {
            self.stalls += 1;
            self.starved = true;
        }
    }

    /// La tête d'une phrase coupée : jouée, sauf si le saut en cours vise un
    /// mot de la suite.
    pub(super) fn enqueue_head(&mut self, head_words: usize, head: &crate::tts::pool::Rendered) {
        let id = self.queue_head;
        if let Some(spans) = &head.words {
            self.timings.insert(id, Arc::clone(spans));
        }
        self.split_parts.insert(
            id,
            SplitParts { head_words, head_speech: head.speech, head_spans: head.words.clone(), tail_spans: None },
        );
        let start = self.start_at.filter(|s| s.sentence == id && !s.consumed);
        match start {
            Some(s) if s.word >= head_words => {}
            Some(s) => {
                if let Some(start) = self.start_at.as_mut() {
                    start.consumed = true;
                }
                self.player.enqueue_part(id, &head.audio, head.speech, Duration::ZERO, start_offset(&s, head));
                self.starved = false;
            }
            None => {
                self.player.enqueue_part(id, &head.audio, head.speech, Duration::ZERO, Duration::ZERO);
                self.starved = false;
            }
        }
        self.awaiting_tail = Some(AwaitingTail { sentence: id, base: head.speech, head_words });
    }

    /// La suite d'une phrase coupée, au chronomètre de la tête ; son
    /// minutage est mis bout à bout avec celui de la tête.
    pub(super) fn enqueue_tail(&mut self, waiting: AwaitingTail, tail: &crate::tts::pool::Rendered) {
        let id = waiting.sentence;
        if let Some(parts) = self.split_parts.get_mut(&id) {
            parts.tail_spans = tail.words.clone();
        }
        let shift = waiting.base.as_secs_f32();
        let head = self.timings.remove(&id);
        if let (Some(head), Some(rest)) = (head, &tail.words) {
            let joined: Vec<crate::doc::WordSpan> = head
                .iter()
                .copied()
                .chain(rest.iter().map(|w| crate::doc::WordSpan { start: w.start + shift, end: w.end + shift }))
                .collect();
            self.timings.insert(id, joined.into());
        }
        let skip = match self.start_at.as_mut() {
            Some(start) if start.sentence == id && !start.consumed => {
                start.consumed = true;
                let word = start.word.saturating_sub(waiting.head_words);
                tail.words
                    .as_deref()
                    .and_then(|w| w.get(word))
                    .map_or(Duration::ZERO, |w| Duration::from_secs_f32(w.start.max(0.0)))
            }
            _ => Duration::ZERO,
        };
        let words = self.builder.document().sentence(id).map_or(0, |s| s.words.len());
        self.pace.record((waiting.base + tail.speech).as_secs_f64(), words);
        self.player.enqueue_part(id, &tail.audio, tail.speech, waiting.base, skip);
        self.starved = false;
        self.awaiting_tail = None;
        self.queue_head = id + 1;
    }

    /// Coupures comptées depuis l'ouverture (mesure du filtre de charge).
    pub fn stalls(&self) -> usize {
        self.stalls
    }

    pub fn is_finished(&self) -> bool {
        self.input_done
            && self.queue_head >= self.builder.sentence_count()
            && self.player.queue_len() == 0
    }

    /// Secondes d'écoute par mot : la cadence mesurée à 1× sur ce qui a été
    /// synthétisé, divisée par la vitesse de lecture.
    pub(super) fn seconds_per_word(&self) -> Option<f64> {
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

    /// La sortie audio change (choisie, retirée, rebranchée) : un nouveau
    /// lecteur, au volume, à la vitesse et au muet de l'ancien, repart de la
    /// phrase en cours. `resume` : il reprend la lecture, sinon il attend.
    pub fn replace_player(&mut self, mut player: Player, resume: bool) {
        let at = self.current_sentence();
        player.set_volume(self.player.volume());
        player.set_speed(self.player.speed());
        if self.player.is_muted() {
            player.toggle_mute();
        }
        self.stop_preview();
        self.player = player;
        self.seek_to(at);
        if resume {
            self.player.play();
        }
    }

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
        self.timings.clear();
        self.split_parts.clear();
        self.awaiting_tail = None;
        // La sortie vidée par un saut n'est pas une coupure.
        self.starved = true;
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

    pub(super) fn apply_tts_params(&mut self) {
        let params = self.synthesis_params();
        self.synth.set_params(params);
    }


    // ------------------------------------------------------------ vue et recherche
}
