//! Ce que les onglets Modèles et Général effacent : modèles, téléchargement
//! en cours, cache des documents, signets.
//!
//! Un lecteur hors disque (`set_persistent(false)`, l'app de test) refuse
//! tout effacement : un test d'interface ne vide jamais les données de
//! l'utilisateur.

use anyhow::{anyhow, bail, Result};

use crate::bookmark::BookmarkStore;
use crate::models::{self, ModelPackage};

use super::settings::package;
use super::Reader;

/// Refus d'un lecteur hors disque.
pub const TRIAL: &str = "mode d'essai : rien n'est modifié sur le disque";

/// Un signet, quel que soit son document.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBookmark {
    /// Identité du document (chemin, ou source et empreinte).
    pub document: String,
    /// Nom lisible du document.
    pub label: String,
    pub sentence: usize,
    pub section_title: String,
    pub snippet: String,
    pub when: String,
}

/// Le nom d'un document pour l'œil : le fichier d'un chemin, la source d'un
/// texte capturé (« sélection », « image »).
pub fn document_label(identity: &str) -> String {
    if identity.starts_with('/') {
        return std::path::Path::new(identity)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| identity.to_string());
    }
    identity.split(':').next().unwrap_or(identity).to_string()
}

impl Reader {
    /// Pourquoi ce modèle ne peut pas être supprimé maintenant, s'il ne le
    /// peut pas : il est actif, ou il se télécharge.
    pub fn removal_refusal(&self, package: &ModelPackage) -> Option<String> {
        if models::is_active_package(&self.current_config().engine.model_path, package) {
            return Some(format!(
                "{} est le modèle actif : choisissez-en un autre avant de le supprimer",
                package.display_name
            ));
        }
        if self.download.as_ref().is_some_and(|d| d.package.id == package.id && !d.is_finished()) {
            return Some(format!(
                "{} se télécharge : annulez d'abord le téléchargement",
                package.display_name
            ));
        }
        None
    }

    pub fn remove_model(&mut self, id: &str) -> Result<()> {
        self.remove_package(package(id)?)
    }

    /// Le cœur de `remove_model`, sur un paquet déjà résolu (le catalogue
    /// disponible dépend des familles compilées).
    fn remove_package(&mut self, package: &ModelPackage) -> Result<()> {
        if let Some(reason) = self.removal_refusal(package) {
            bail!(reason);
        }
        if !self.persist {
            bail!(TRIAL);
        }
        models::remove(package)
    }

    /// Demande l'arrêt du téléchargement en cours. Le fichier partiel reste :
    /// une nouvelle installation reprendra de là.
    pub fn cancel_download(&mut self) -> Result<()> {
        match self.download.as_ref() {
            Some(d) if !d.is_finished() => {
                d.cancel();
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

    /// Tous les signets, tous documents confondus. Un texte ouvert a les
    /// siens en mémoire : ils font foi.
    pub fn all_bookmarks(&self) -> Vec<StoredBookmark> {
        let items = match &self.session {
            Some(app) => app.bookmarks.items.clone(),
            None => BookmarkStore::load().0.items,
        };
        items
            .into_iter()
            .map(|b| StoredBookmark {
                label: document_label(&b.document),
                when: b.timestamp(),
                document: b.document,
                sentence: b.sentence_id,
                section_title: b.section_title,
                snippet: b.snippet,
            })
            .collect()
    }

    pub fn remove_bookmark(&mut self, document: &str, sentence: usize) -> Result<()> {
        if !self.persist {
            bail!(TRIAL);
        }
        let path = BookmarkStore::path().ok_or_else(|| anyhow!("répertoire de données introuvable"))?;
        self.remove_bookmark_at(&path, document, sentence)
    }

    /// Retire le signet du fichier `path` et de celui du texte ouvert : sans
    /// le second, le prochain signet posé réécrirait l'ancien. Le fichier est
    /// un paramètre pour que les tests n'approchent jamais celui de
    /// l'utilisateur.
    fn remove_bookmark_at(&mut self, path: &std::path::Path, document: &str, sentence: usize) -> Result<()> {
        let mut store = BookmarkStore::load_from(path).0;
        if store.remove(document, sentence) {
            // Écrire d'abord : si le disque refuse, la session garde le signet
            // et l'interface ne ment pas.
            store.save_to(path)?;
        }
        if let Some(app) = self.session.as_mut() {
            app.bookmarks.remove(document, sentence);
        }
        Ok(())
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
            Box::new(Player::silent),
            Instant::now(),
        )
    }

    fn quiet() -> Config {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config
    }

    #[test]
    fn the_active_model_cannot_be_removed() {
        let first = &models::CATALOG[0];
        let second = &models::CATALOG[1];
        let path = models::install_path(first).expect("répertoire des modèles (isolé sous test)");
        let mut config = quiet();
        config.engine.model_path = path.to_string_lossy().into_owned();
        let r = reader_with(config);
        let reason = r.removal_refusal(first).expect("refusé");
        assert!(reason.contains("modèle actif"), "{reason}");
        assert_eq!(r.removal_refusal(second), None);
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
        assert_eq!(r.remove_bookmark("a", 0).unwrap_err().to_string(), TRIAL);
    }

    #[test]
    fn cancelling_without_a_download_is_an_error() {
        let mut r = reader_with(quiet());
        assert!(r.cancel_download().is_err());
    }

    /// Une session dont le magasin est vide : jamais les signets réels.
    fn open_isolated(r: &mut Reader) -> String {
        r.open_text("Un. Deux.", "sélection", Instant::now());
        r.session.as_mut().unwrap().bookmarks = BookmarkStore::default();
        r.document_identity().unwrap().to_string()
    }

    fn scratch_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-data-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("bookmarks.json")
    }

    #[test]
    fn removing_a_bookmark_also_forgets_it_in_the_open_session() {
        let mut r = reader_with(quiet());
        let identity = open_isolated(&mut r);
        let path = scratch_file("remove");
        // Le signet existe sur le disque (de test) et dans la session.
        let mut store = BookmarkStore::default();
        store.toggle(&identity, 1, 0, "Titre", "Deux.");
        store.toggle(&identity, 0, 0, "Titre", "Un.");
        store.save_to(&path).unwrap();
        r.session.as_mut().unwrap().bookmarks.toggle(&identity, 1, 0, "Titre", "Deux.");

        r.remove_bookmark_at(&path, &identity, 1).unwrap();

        let on_disk = BookmarkStore::load_from(&path).0;
        assert!(!on_disk.contains(&identity, 1));
        assert!(on_disk.contains(&identity, 0), "les autres signets restent");
        assert!(
            !r.session().unwrap().bookmarks.contains(&identity, 1),
            "la session ne le réécrira pas au prochain signet"
        );
        assert!(r.all_bookmarks().is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn the_open_session_lists_its_bookmarks_with_a_readable_name() {
        let mut r = reader_with(quiet());
        let identity = open_isolated(&mut r);
        r.session.as_mut().unwrap().bookmarks.toggle(&identity, 0, 0, "Titre", "Un.");
        let all = r.all_bookmarks();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].label, "sélection");
        assert_eq!(all[0].snippet, "Un.");
        assert_eq!(all[0].sentence, 0);
    }

    #[test]
    fn documents_are_named_for_the_eye() {
        assert_eq!(document_label("/Users/x/notes/cours.md"), "cours.md");
        assert_eq!(document_label("sélection:00ab12"), "sélection");
        assert_eq!(document_label("image:ff"), "image");
        assert_eq!(document_label("presse-papiers"), "presse-papiers");
    }

    /// Installe un faux fichier de modèle dans le dossier isolé des tests.
    fn install_fake(package: &ModelPackage) -> std::path::PathBuf {
        let path = models::install_path(package).expect("dossier isolé");
        // Le dossier de la famille est partagé par les tests : une suppression
        // (`models::remove` retire le dossier devenu vide) peut passer entre
        // sa création et l'écriture. On recommence alors.
        for _ in 0..50 {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if std::fs::write(&path, b"faux modele").is_ok() {
                return path;
            }
        }
        panic!("installation du faux modèle impossible : {}", path.display());
    }

    #[test]
    fn an_installed_model_is_really_deleted() {
        let package = &models::CATALOG[2];
        let path = install_fake(package);
        let mut r = reader_with(quiet());
        r.remove_package(package).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn the_active_model_survives_a_removal_attempt() {
        let package = &models::CATALOG[1];
        let path = install_fake(package);
        let mut config = quiet();
        config.engine.model_path = path.to_string_lossy().into_owned();
        let mut r = reader_with(config);
        assert!(r.remove_package(package).is_err());
        assert!(path.exists());
        std::fs::remove_file(&path).ok();
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
