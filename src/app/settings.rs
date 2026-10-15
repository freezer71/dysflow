//! Réglages depuis le terminal : la page des réglages, l'aperçu de voix, le rechargement du moteur, l'enregistrement de chaque changement.

use super::*;

impl App {
    /// Ouvre les réglages. La lecture continue derrière : ce qu'on y change
    /// s'entend aussitôt.
    pub fn open_config(&mut self) {
        self.show_config = true;
        self.panel = Panel::None;
    }

    /// `q`, `Échap` ou `s` : les réglages se ferment, la lecture reste. Sans
    /// rien à lire (`sillon --config` seul), il n'y a rien derrière : fermer
    /// rend la main au terminal.
    pub fn close_config(&mut self) {
        self.stop_preview();
        if self.doc().is_empty() && self.input_complete() {
            self.should_quit = true;
            return;
        }
        self.show_config = false;
    }

    pub fn config_move_row(&mut self, delta: i32) {
        self.screen.move_row(delta);
    }

    /// La voix d'une autre langue que celle du document ne s'entend pas
    /// ici : la phrase en cours n'a pas à reprendre pour elle.
    pub(super) fn effect_in_document(&self, field: Field, effect: Effect) -> Effect {
        let language = self.builder.language();
        let code = crate::text::language::Lang::parse(language).map_or(language, |l| l.code());
        match field.language() {
            Some(voice_language) if voice_language != code => Effect::None,
            _ => effect,
        }
    }

    /// Fait d'un modèle installé le moteur courant, et l'écrit aussitôt.
    pub(super) fn select_model(&mut self, package: &'static models::ModelPackage) {
        let Some(path) = models::install_path(package) else { return };
        self.config.engine.model_path = path.to_string_lossy().into_owned();
        self.config.engine.family = package.family.to_string();
        self.config.engine.engine = "auto".to_string();
        // Le modèle change, ses voix aussi : chaque langue reçoit la sienne.
        self.config.align_voice_with_family();
        self.reload_engine();
        self.notify(&format!("Voix {} prête", package.display_name));
        // Le choix doit survivre au relancement : il s'écrit tout de suite.
        if let Err(err) = self.save_model_choice() {
            self.notify(&format!("Modèle choisi, non enregistré : {err:#}"));
        }
    }

    // ----------------------------------------------------------- aperçu de voix

    /// `t` dans la page : la voix de la ligne choisie, dans sa langue ; sur
    /// les autres lignes, la voix du document.
    pub fn preview_voice(&mut self) {
        // La touche répond toujours quelque chose : laisser tourner l'aperçu
        // précédent ferait passer un refus pour une touche sans effet.
        self.stop_preview();
        let language = match self.screen.current_field().language() {
            Some(language) => language.to_string(),
            None => self.builder.language().to_string(),
        };
        match self.request_preview_in(&language) {
            Ok(()) => self.screen.notice.clear(),
            Err(err) => self.screen.notice = err,
        }
    }

    /// L'aperçu dans une langue donnée : chaque voix de l'onglet Voix
    /// s'écoute dans la sienne.
    pub fn request_preview_in(&mut self, language: &str) -> Result<(), String> {
        self.preview_error = None;
        let label = self.preview_label(language);
        preview::start(
            &mut self.previewer,
            &mut self.player,
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

    /// Décrit ce que l'aperçu fait entendre : la voix de cette langue et la
    /// vitesse.
    pub(super) fn preview_label(&self, language: &str) -> String {
        let voice = self.config.tts_params_in(language).voice;
        let voice = if voice.is_empty() { "voix du système".to_string() } else { voice };
        format!("{voice} · {:.2}×", self.config.voice.speed)
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
    pub(super) fn poll_preview(&mut self) {
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

    pub(super) fn poll_download(&mut self) {
        if let Some(package) = self.screen.poll_download() {
            self.select_model(package);
        }
    }

    /// Répercute un changement de réglage sur la lecture.
    pub fn apply_effect(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Speed => {
                let speed = self.config.voice.speed;
                self.player.set_speed(speed);
            }
            Effect::Resynthesize => {
                self.apply_tts_params();
                let current = self.current_sentence();
                self.seek_to(current);
            }
        }
    }

    /// Reconstruit le moteur et le pipeline de synthèse.
    ///
    /// Utilisé au changement de modèle, de backend ou de nombre de sessions.
    /// L'ancien ordonnanceur est libéré hors du thread d'interface : sa
    /// destruction attend la fin des synthèses en cours, ce qui figerait
    /// l'affichage pendant une seconde ou deux.
    ///
    /// Terminal seulement : `tts::build_engine` ne reçoit pas de calcul MLX,
    /// Kokoro y passe par ONNX ou se replie. L'app recharge par le lecteur
    /// (`Reader::reload_voice`), qui transmet le calcul MLX.
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
            synth.push_sentence(
                sentence.text.clone(),
                sentence.spoken_words(),
                sentence.written_words(),
                sentence.trailing_pause_ms,
            );
        }
        self.published = self.builder.sentence_count();
        // La cadence mesurée valait pour l'ancien moteur, et une phrase qu'il
        // avait refusée mérite sa chance avec le nouveau.
        self.pace = Pace::default();
        self.skipped.clear();

        let previous = std::mem::replace(&mut self.synth, synth);
        crate::sync::spawn_named("sillon-liberation", move || drop(previous));

        // L'aperçu doit parler avec le nouveau moteur, pas avec l'ancien. Son
        // arrêt attend la synthèse en cours : hors du thread d'interface, donc.
        self.stop_preview();
        let previous = std::mem::replace(&mut self.previewer, Previewer::new(self.synth.engine()));
        crate::sync::spawn_named("sillon-liberation", move || drop(previous));

        self.player.flush();
        self.timings.clear();
        self.split_parts.clear();
        self.awaiting_tail = None;
        self.starved = true;
        let current = self.current_sentence();
        self.queue_head = current;
        self.synth.set_cursor(current);
        self.notify(&format!("Moteur : {}", self.synth.engine_name()));
    }

    /// Où l'enregistrement automatique écrit ; `None` : nulle part.
    pub fn set_config_target(&mut self, path: Option<std::path::PathBuf>) {
        self.config_target = path;
    }

    /// Écrit ce seul réglage dans `config.toml` : le fichier est relu, le
    /// champ y est recopié depuis la configuration en cours, le reste du
    /// fichier ne bouge pas (les options de la ligne de commande n'y entrent
    /// pas). Un fichier illisible n'est jamais réécrit.
    pub fn save_field(&mut self, field: Field) -> anyhow::Result<std::path::PathBuf> {
        let path = self
            .config_target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("répertoire de configuration introuvable"))?;
        let mut on_disk = Config::read_from(&path)?;
        field.copy_value(&self.config, &mut on_disk);
        on_disk.save_to(&path)?;
        Ok(path)
    }

    /// Change le réglage choisi, le fait entendre, et l'écrit aussitôt. Un
    /// enregistrement impossible laisse le réglage appliqué et le dit.
    pub fn config_adjust_and_save(&mut self, delta: i32) {
        let field = self.screen.current_field();
        let effect = field.adjust(&mut self.config, delta);
        let effect = self.effect_in_document(field, effect);
        self.apply_effect(effect);
        self.report_save(field);
    }

    fn report_save(&mut self, field: Field) {
        if let Err(err) = self.save_field(field) {
            self.notify(&format!("Réglage appliqué, non enregistré : {err:#}"));
        }
    }

    /// `+` et `-` pendant la lecture : la vitesse change sur-le-champ, et
    /// s'enregistre comme si elle avait été réglée dans la page.
    pub fn nudge_speed(&mut self, delta: f32) {
        self.change_speed(delta);
        self.report_save(Field::Speed);
    }

    /// Écrit le modèle choisi — moteur, chemin, famille, et la voix de
    /// chaque langue qu'il apporte — sur le fichier relu : rien d'autre ne
    /// bouge, et un fichier illisible n'est jamais réécrit.
    fn save_model_choice(&self) -> anyhow::Result<()> {
        let path = self
            .config_target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("répertoire de configuration introuvable"))?;
        let mut on_disk = Config::read_from(&path)?;
        on_disk.engine.engine = self.config.engine.engine.clone();
        on_disk.engine.model_path = self.config.engine.model_path.clone();
        on_disk.engine.family = self.config.engine.family.clone();
        on_disk.align_voice_with_family();
        on_disk.save_to(&path)
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
