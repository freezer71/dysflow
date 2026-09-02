//! Reprise de lecture : rouvrir un document là où la voix s'était arrêtée.
//!
//! Un signet se pose à la main, pour y revenir plus tard ; la position, elle,
//! se retient toute seule et ne sert qu'une fois. Les deux ne se mélangent
//! donc pas — et perdre une position coûte un défilement, là où perdre un
//! signet coûte du travail : ce fichier-ci se relit sans façons, et s'ignore
//! s'il est illisible.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

    /// Relit les positions retenues, ou repart de zéro sans le dire.
    ///
    /// Un fichier illisible n'est pas mis de côté comme celui des signets :
    /// ce qu'il contient se reconstruit à la première lecture.
    pub fn load() -> Self {
        Self::path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("répertoire de données introuvable")?;
        self.save_to(&path)
    }

    /// Séparé du chemin par défaut pour que l'aller-retour se vérifie.
    fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("écriture de {}", path.display()))?;
        Ok(())
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
        let dir = std::env::temp_dir().join(format!("sillon-{}", std::process::id()));
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
}
