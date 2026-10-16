//! Ce que l'app efface : modèles, téléchargement en cours, cache des
//! documents.
//!
//! Un lecteur hors disque (`set_persistent(false)`, l'app de test) refuse
//! tout effacement : un test d'interface ne vide jamais les données de
//! l'utilisateur.

use anyhow::{anyhow, bail, Result};

use crate::models::{self, ModelPackage};

use super::Reader;

/// Refus d'un lecteur hors disque.
pub const TRIAL: &str = "mode d'essai : rien n'est modifié sur le disque";

impl Reader {
    /// Pourquoi ce modèle ne peut pas être supprimé maintenant, s'il ne le
    /// peut pas : il se télécharge. Le modèle utilisé se supprime : la voix
    /// de macOS prend alors la main.
    pub fn removal_refusal(&self, package: &ModelPackage) -> Option<String> {
        if self.download.as_ref().is_some_and(|d| d.package.id == package.id && !d.is_finished()) {
            return Some(format!(
                "{} se télécharge : annulez d'abord le téléchargement",
                package.display_name
            ));
        }
        None
    }

    pub fn remove_model(&mut self, id: &str) -> Result<()> {
        if id == "system" {
            bail!("Les voix de macOS se gèrent dans Réglages Système");
        }
        let package = self.package(id)?;
        self.remove_package_at(package, std::time::Instant::now())
    }

    /// Le cœur de `remove_model`, sur un paquet déjà résolu (le catalogue
    /// disponible dépend des familles compilées). Le modèle utilisé passe
    /// d'abord la main à la voix de macOS (la lecture continue avec elle),
    /// puis ses fichiers partent.
    pub(crate) fn remove_package_at(&mut self, package: &ModelPackage, now: std::time::Instant) -> Result<()> {
        if let Some(reason) = self.removal_refusal(package) {
            bail!(reason);
        }
        if !self.persist {
            bail!(TRIAL);
        }
        let config = self.current_config();
        if config.engine_preference() != crate::tts::EnginePreference::System
            && models::is_active_package(&config.engine.model_path, package)
        {
            self.use_voice("system", now)?;
        }
        models::remove(package)
    }

    /// Demande l'arrêt du téléchargement en cours. Le fichier partiel reste :
    /// une nouvelle installation reprendra de là.
    pub fn cancel_download(&mut self) -> Result<()> {
        match self.download.as_ref() {
            Some(d) if !d.is_finished() => {
                d.cancel();
                // Une fin qui arriverait quand même ne fait plus passer à Kokoro.
                let id = d.package.id;
                self.abandon_migration(id);
                Ok(())
            }
            _ => Err(anyhow!("aucun téléchargement en cours")),
        }
    }

    pub fn clear_document_cache(&self) -> Result<u64> {
        if !self.persist {
            bail!(TRIAL);
        }
        crate::source::cache::clear()
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

    fn reader_with(config: Config) -> Reader {
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    fn quiet() -> Config {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config
    }

    #[test]
    fn no_model_is_active_without_a_path() {
        let r = reader_with(quiet());
        for package in models::CATALOG {
            assert_eq!(r.removal_refusal(package), None);
        }
    }

    #[test]
    fn a_reader_off_disk_erases_nothing() {
        let mut r = reader_with(quiet());
        r.set_persistent(false);
        assert_eq!(r.clear_document_cache().unwrap_err().to_string(), TRIAL);
    }

    #[test]
    fn cancelling_without_a_download_is_an_error() {
        let mut r = reader_with(quiet());
        assert!(r.cancel_download().is_err());
    }

    /// Installe un faux fichier de modèle dans le dossier isolé des tests.
    fn install_fake(package: &ModelPackage) -> std::path::PathBuf {
        let path = models::install_path(package).expect("dossier isolé");
        // Le dossier de la famille est partagé par les tests : une suppression
        // (`models::remove` retire le dossier devenu vide) peut passer entre
        // sa création et l'écriture, ou dans `create_dir_all` même, qui rend
        // alors « File exists » pour un dossier disparu entre-temps. On
        // recommence dans les deux cas.
        let mut last_error = None;
        for _ in 0..50 {
            if let Err(err) = std::fs::create_dir_all(path.parent().unwrap()) {
                last_error = Some(err);
                continue;
            }
            match std::fs::write(&path, b"faux modele") {
                Ok(()) => return path,
                Err(err) => last_error = Some(err),
            }
        }
        panic!("installation du faux modèle impossible : {} ({last_error:?})", path.display());
    }

    /// Un paquet à part, dans sa propre famille : aucun autre test ne
    /// l'installe, le supprimer ne dérange personne (le catalogue n'a plus que
    /// les deux Kokoro, que d'autres tests installent).
    static DISPOSABLE: ModelPackage = ModelPackage {
        id: "essai_suppression",
        family: "essai_suppression",
        companions: &[],
        ..models::CATALOG[0]
    };

    /// Comme `DISPOSABLE`, pour le test du modèle utilisé : deux tests qui
    /// installent et suppriment le même fichier se marcheraient dessus.
    static DISPOSABLE_ACTIVE: ModelPackage = ModelPackage {
        id: "essai_suppression_actif",
        family: "essai_suppression_actif",
        companions: &[],
        ..models::CATALOG[0]
    };

    #[test]
    fn an_installed_model_is_really_deleted() {
        let package = &DISPOSABLE;
        let path = install_fake(package);
        let mut r = reader_with(quiet());
        r.remove_package_at(package, Instant::now()).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn removing_the_active_model_switches_to_the_system_voice_first() {
        let package = &DISPOSABLE_ACTIVE;
        let path = install_fake(package);
        let mut config = quiet();
        config.engine.model_path = path.to_string_lossy().into_owned();
        let mut r = reader_with(config);
        assert_eq!(r.removal_refusal(package), None, "le modèle utilisé se supprime aussi");
        r.remove_package_at(package, Instant::now()).unwrap();
        assert_eq!(r.config().engine.engine, "system");
        assert!(!path.exists());
    }

    #[test]
    fn removing_the_active_model_with_an_open_text_keeps_the_reader_alive() {
        let package = &DISPOSABLE_ACTIVE;
        let path = install_fake(package);
        let mut config = quiet();
        config.engine.model_path = path.to_string_lossy().into_owned();
        let mut r = reader_with(config);
        r.open_text("Un. Deux.", "a", Instant::now());
        r.remove_package_at(package, Instant::now()).unwrap();
        assert_eq!(r.config().engine.engine, "system");
        assert!(!path.exists());
    }

    #[test]
    fn the_system_row_cannot_be_removed() {
        let mut r = reader_with(quiet());
        assert!(r.remove_model("system").unwrap_err().to_string().contains("macOS"));
    }

    #[test]
    fn removal_is_refused_while_the_model_downloads_then_cancel_works() {
        let package = &models::CATALOG[0];
        let mut r = reader_with(quiet());
        r.download = Some(models::Download::fake(package, false));
        let reason = r.removal_refusal(package).expect("refusé");
        assert!(reason.contains("se télécharge"), "{reason}");
        r.cancel_download().unwrap();
        assert!(r.download.as_ref().unwrap().cancel_requested());
        // Un téléchargement fini ne bloque plus rien et n'est plus annulable.
        r.download = Some(models::Download::fake(package, true));
        assert_eq!(r.removal_refusal(package), None);
        assert!(r.cancel_download().is_err());
    }
}
