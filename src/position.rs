//! Reprise de lecture : rouvrir un document là où la voix s'était arrêtée.
//!
//! Un signet se pose à la main, pour y revenir plus tard ; la position, elle,
//! se retient toute seule et ne sert qu'une fois. Les deux ne se mélangent
//! donc pas — et perdre une position coûte un défilement, là où perdre un
//! signet coûte du travail. Un fichier illisible est tout de même mis de côté
//! en `.bak` plutôt qu'écrasé, et le journal le dit.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ce qui identifie un document d'une ouverture à l'autre : son chemin entier.
///
/// Le nom seul ne suffit pas — deux `spec.pdf` dans deux dossiers se
/// partageraient positions et signets, et l'un enverrait la lecture de
/// l'autre n'importe où. Le chemin est résolu pour qu'un même fichier, atteint
/// par deux chemins, reste un seul document.
pub fn document_identity(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Document d'origine, pour ne pas mélanger les lectures.
    pub document: String,
    pub sentence_id: usize,
    /// La phrase elle-même, telle qu'elle est écrite.
    ///
    /// L'index seul ne survit pas à une retouche : un paragraphe ajouté en
    /// tête décale tout, et la reprise tomberait ailleurs. Le texte, lui, se
    /// retrouve.
    pub snippet: String,
    /// Secondes depuis l'epoch Unix.
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionStore {
    #[serde(default)]
    pub items: Vec<Position>,
}

impl PositionStore {
    pub fn path() -> Option<PathBuf> {
        crate::paths::data_dir().map(|d| d.join("positions.json"))
    }

    /// Relit les positions retenues. Un fichier illisible est mis de côté
    /// (`paths::load_json`) et l'avertissement va au journal.
    pub fn load() -> Self {
        Self::path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context(crate::tr!("data directory not found", "répertoire de données introuvable"))?;
        self.save_to(&path)
    }

    /// Séparé du chemin par défaut pour que l'aller-retour se vérifie.
    fn load_from(path: &Path) -> Self {
        let (store, notice) = crate::paths::load_json(path, crate::i18n::Tr::new("reading positions", "positions de lecture"));
        if let Some(notice) = notice {
            eprintln!("{}", crate::tr!(en = "sillage: {notice}", fr = "sillage : {notice}"));
        }
        store
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        crate::paths::save_json(path, self)
    }

    /// Fait passer une position retenue sous un ancien nom au nom courant.
    ///
    /// Les positions étaient retenues sous le nom du fichier seul ; elles le
    /// sont désormais sous son chemin. Ce qui existe déjà sous le nom courant
    /// l'emporte : c'est le plus récent.
    pub fn adopt(&mut self, former: &str, current: &str) {
        if former == current {
            return;
        }
        if self.get(current).is_some() {
            self.forget(former);
            return;
        }
        for item in &mut self.items {
            if item.document == former {
                item.document = current.to_string();
            }
        }
    }

    /// Retient où en est la lecture de ce document.
    ///
    /// Une seule position par document : c'est la dernière qui vaut.
    pub fn record(&mut self, document: &str, sentence_id: usize, snippet: &str) {
        self.forget(document);
        self.items.push(Position {
            document: document.to_string(),
            sentence_id,
            snippet: snippet.to_string(),
            updated_at: now_epoch(),
        });
    }

    /// Oublie ce document : sa prochaine ouverture repartira du début.
    pub fn forget(&mut self, document: &str) {
        self.items.retain(|p| p.document != document);
    }

    pub fn get(&self, document: &str) -> Option<&Position> {
        self.items.iter().find(|p| p.document == document)
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_file_is_set_aside_not_overwritten() {
        let dir = crate::paths::test_scratch("positions");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("positions.json");
        std::fs::write(&path, "{ pas du json").unwrap();

        let mut store = PositionStore::load_from(&path);
        assert!(store.items.is_empty());
        store.record("a.md", 1, "Une phrase.");
        store.save_to(&path).unwrap();

        let backup = std::fs::read_to_string(dir.join("positions.json.bak")).unwrap();
        assert_eq!(backup, "{ pas du json", "l'ancien fichier est gardé à part");
        assert_eq!(PositionStore::load_from(&path).items.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_place_is_kept_for_each_document() {
        let mut store = PositionStore::default();
        store.record("a.md", 3, "Une phrase.");
        store.record("b.md", 7, "Une autre.");
        assert_eq!(store.get("a.md").map(|p| p.sentence_id), Some(3));
        assert_eq!(store.get("b.md").map(|p| p.sentence_id), Some(7));
        assert!(store.get("c.md").is_none());
    }

    #[test]
    fn the_last_place_is_the_one_that_counts() {
        let mut store = PositionStore::default();
        store.record("a.md", 3, "Une phrase.");
        store.record("a.md", 9, "Plus loin.");
        assert_eq!(store.items.len(), 1, "une seule position par document");
        assert_eq!(store.get("a.md").map(|p| p.sentence_id), Some(9));
        assert_eq!(store.get("a.md").map(|p| p.snippet.as_str()), Some("Plus loin."));
    }

    #[test]
    fn forgetting_one_document_leaves_the_others() {
        let mut store = PositionStore::default();
        store.record("a.md", 1, "x");
        store.record("b.md", 2, "y");
        store.forget("a.md");
        assert!(store.get("a.md").is_none());
        assert_eq!(store.get("b.md").map(|p| p.sentence_id), Some(2));
    }

    #[test]
    fn a_file_without_positions_loads_as_nothing_retained() {
        // Un fichier écrit par une version antérieure ne doit pas faire échouer
        // la relecture : ce qu'il ignore vaut « rien de retenu ».
        let store: PositionStore = serde_json::from_str("{}").expect("format tolérant");
        assert!(store.items.is_empty());
    }

    #[test]
    fn a_place_survives_the_file() {
        let mut store = PositionStore::default();
        store.record("a.md", 3, "Une phrase.");
        let written = serde_json::to_string(&store).unwrap();
        let back: PositionStore = serde_json::from_str(&written).unwrap();
        assert_eq!(back.get("a.md"), store.get("a.md"));
    }

    #[test]
    fn a_place_written_is_a_place_reread() {
        let dir = crate::paths::test_scratch("positions-dossier");
        let path = dir.join("positions.json");
        let mut store = PositionStore::default();
        store.record("a.md", 4, "Une phrase.");
        store.save_to(&path).expect("écriture");

        let back = PositionStore::load_from(&path);
        assert_eq!(back.get("a.md").map(|p| p.sentence_id), Some(4));
        assert_eq!(back.get("a.md").map(|p| p.snippet.as_str()), Some("Une phrase."));

        // Un fichier abîmé ne fait pas échouer l'ouverture : on repart sans
        // position, ce qui coûte un défilement.
        std::fs::write(&path, "ceci n'est pas du json").expect("écriture");
        assert!(PositionStore::load_from(&path).items.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_place_is_dated_for_real() {
        let mut store = PositionStore::default();
        store.record("a.md", 1, "x");
        // Postérieur à 2020, donc pas une constante.
        assert!(store.items[0].updated_at > 1_577_836_800);
    }


    #[test]
    fn saving_leaves_no_temporary_file_next_to_the_positions() {
        let dir = crate::paths::test_scratch("pos-atomic");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("positions.json");
        let mut store = PositionStore::default();
        store.record("a.md", 4, "Une phrase.");
        store.save_to(&path).expect("écriture");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["positions.json"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_place_kept_under_a_former_name_follows_the_document() {
        // Les positions étaient retenues sous le nom du fichier seul ; elles
        // le sont désormais sous son chemin. Ce qui a été retenu ne se perd pas.
        let mut store = PositionStore::default();
        store.record("spec.pdf", 12, "Une phrase.");
        store.adopt("spec.pdf", "/docs/a/spec.pdf");
        assert_eq!(store.get("/docs/a/spec.pdf").map(|p| p.sentence_id), Some(12));
        assert!(store.get("spec.pdf").is_none(), "l'ancienne clé ne traîne pas");
        // Une position déjà retenue sous le chemin l'emporte.
        store.record("spec.pdf", 3, "Autre.");
        store.adopt("spec.pdf", "/docs/a/spec.pdf");
        assert_eq!(store.get("/docs/a/spec.pdf").map(|p| p.sentence_id), Some(12));
    }
}
