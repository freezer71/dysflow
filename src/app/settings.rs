//! Réglages depuis le terminal : écran de configuration, modèles, aperçu, rechargement du moteur, enregistrement.

use super::*;

impl App {
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
    pub(super) fn activate_model(&mut self) {
        let Some(package) = self.screen.current_package() else {
            self.screen.notice =
                "aucun modèle disponible : binaire compilé sans `kokoro-onnx`".to_string();
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
        // Un paquet interrompu (annulé, échoué entre deux fichiers) se retire
        // aussi : son gros fichier ne resterait sinon jamais.
        if !models::has_files(package) {
            return;
        }
        if !self.screen.confirm_delete(package) {
            self.screen.notice = format!(
                "supprimer {} ({}) ? d à nouveau pour confirmer",
                package.display_name,
                models::human_size(package.total_bytes())
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
    pub(super) fn select_model(&mut self, package: &'static models::ModelPackage) {
        let Some(path) = models::install_path(package) else { return };
        self.config.engine.model_path = path.to_string_lossy().into_owned();
        self.config.engine.family = package.family.to_string();
        self.config.engine.engine = "auto".to_string();
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
    pub(super) fn preview_label(&self) -> String {
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

    /// La configuration vient d'être écrite ailleurs (le lecteur de l'app,
    /// à son chemin).
    pub fn config_saved(&mut self) {
        self.config_dirty = false;
    }

    #[cfg(test)]
    pub(crate) fn mark_config_dirty(&mut self) {
        self.config_dirty = true;
    }

    /// Écrit la configuration si un choix l'attend, sans rien dire.
    pub fn flush_config(&mut self) {
        if self.config_dirty && self.config.save().is_ok() {
            self.config_dirty = false;
        }
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

    /// Ajuste le champ sélectionné, répercute son effet, et l'enregistre
    /// aussitôt. Un enregistrement impossible laisse le réglage appliqué et
    /// le dit.
    pub fn config_adjust_and_save(&mut self, delta: i32) {
        let Some(field) = self.screen.current_field(&self.config) else { return };
        self.config_adjust(delta);
        self.report_save(field);
    }

    /// Valide la saisie d'un champ textuel, puis l'enregistre aussitôt.
    pub fn config_commit_edit_and_save(&mut self) {
        if self.screen.editing.is_none() {
            return;
        }
        let Some(field) = self.screen.current_field(&self.config) else { return };
        self.config_commit_edit();
        self.report_save(field);
    }

    fn report_save(&mut self, field: Field) {
        if let Err(err) = self.save_field(field) {
            self.notify(&format!("Réglage appliqué, non enregistré : {err:#}"));
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
