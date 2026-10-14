//! Signets : mémoriser un passage et y revenir.
//!
//! Les signets sont rattachés au document dont ils proviennent, et horodatés
//! réellement — un libellé constant ne permettait ni de les trier ni de les
//! distinguer.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    /// Document d'origine, pour ne pas mélanger les lectures.
    pub document: String,
    pub sentence_id: usize,
    pub block_id: usize,
    pub section_title: String,
    pub snippet: String,
    /// Secondes depuis l'epoch Unix.
    pub created_at: u64,
}

impl Bookmark {
    /// Horodatage lisible, `AAAA-MM-JJ HH:MM`.
    pub fn timestamp(&self) -> String {
        format_epoch(self.created_at)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookmarkStore {
    #[serde(default)]
    pub items: Vec<Bookmark>,
}

impl BookmarkStore {
    pub fn path() -> Option<PathBuf> {
        crate::paths::data_dir().map(|d| d.join("bookmarks.json"))
    }

    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        Self::load_from(&path)
    }

    /// Lit les signets d'un fichier donné (les tests visent un fichier à eux).
    ///
    /// Un fichier écrit par une version antérieure ne doit pas être écrasé en
    /// silence : il est mis de côté avant de repartir à neuf.
    pub(crate) fn load_from(path: &std::path::Path) -> (Self, Option<String>) {
        crate::paths::load_json(path, "signets")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("répertoire de données introuvable")?;
        self.save_to(&path)
    }

    pub(crate) fn save_to(&self, path: &std::path::Path) -> Result<()> {
        crate::paths::save_json(path, self)
    }

    /// Rattache au nom courant les signets posés sous un ancien nom.
    ///
    /// Les signets étaient rattachés au nom du fichier seul ; ils le sont
    /// désormais à son chemin. Un signet ne se perd pas pour un renommage.
    pub fn adopt(&mut self, former: &str, current: &str) {
        if former == current {
            return;
        }
        for item in &mut self.items {
            if item.document == former {
                item.document = current.to_string();
            }
        }
    }

    /// Ajoute un signet, ou le retire s'il existe déjà à cet endroit.
    ///
    /// Rend la même touche utilisable pour poser et retirer un repère.
    pub fn toggle(
        &mut self,
        document: &str,
        sentence_id: usize,
        block_id: usize,
        section_title: &str,
        snippet: &str,
    ) -> bool {
        if let Some(index) = self
            .items
            .iter()
            .position(|b| b.document == document && b.sentence_id == sentence_id)
        {
            self.items.remove(index);
            return false;
        }
        self.items.push(Bookmark {
            document: document.to_string(),
            sentence_id,
            block_id,
            section_title: section_title.to_string(),
            snippet: snippet.chars().take(120).collect(),
            created_at: now_epoch(),
        });
        self.items.sort_by_key(|b| b.sentence_id);
        true
    }

    pub fn remove(&mut self, document: &str, sentence_id: usize) -> bool {
        let before = self.items.len();
        self.items
            .retain(|b| !(b.document == document && b.sentence_id == sentence_id));
        self.items.len() != before
    }

    /// Signets du document courant, dans l'ordre de lecture.
    pub fn for_document<'a>(&'a self, document: &'a str) -> impl Iterator<Item = &'a Bookmark> {
        self.items.iter().filter(move |b| b.document == document)
    }

    pub fn contains(&self, document: &str, sentence_id: usize) -> bool {
        self.items
            .iter()
            .any(|b| b.document == document && b.sentence_id == sentence_id)
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Convertit un instant Unix en `AAAA-MM-JJ HH:MM` (UTC).
///
/// Le calendrier grégorien tient en quelques lignes ; tirer une dépendance de
/// dates complète pour afficher un horodatage serait disproportionné, mais
/// afficher un libellé fixe ne rend service à personne.
fn format_epoch(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
    let (hour, minute) = ((rem / 3600) as u32, ((rem % 3600) / 60) as u32);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Algorithme de Howard Hinnant : jours depuis l'epoch vers date civile.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Décale l'origine au 1er mars 0000, ce qui met février en fin de cycle.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // 0..=399
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365
    let mp = (5 * doy + 2) / 153; // 0..=11, mars = 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_epochs() {
        assert_eq!(format_epoch(0), "1970-01-01 00:00");
        // 2001-09-09T01:46:40Z, l'instant « un milliard ».
        assert_eq!(format_epoch(1_000_000_000), "2001-09-09 01:46");
        // 2026-08-18T12:00:00Z
        assert_eq!(format_epoch(1_787_054_400), "2026-08-18 12:00");
    }

    #[test]
    fn handles_leap_days() {
        // 2024-02-29T00:00:00Z
        assert_eq!(format_epoch(1_709_164_800), "2024-02-29 00:00");
    }

    #[test]
    fn timestamps_are_real_and_recent() {
        let mut store = BookmarkStore::default();
        store.toggle("doc.md", 3, 1, "Titre", "Un extrait");
        let stamp = store.items[0].timestamp();
        // Postérieur à 2020, donc pas une constante.
        assert!(store.items[0].created_at > 1_577_836_800, "{stamp}");
        assert_eq!(stamp.len(), 16);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut store = BookmarkStore::default();
        assert!(store.toggle("a.md", 5, 2, "S", "x"));
        assert_eq!(store.items.len(), 1);
        assert!(!store.toggle("a.md", 5, 2, "S", "x"));
        assert!(store.items.is_empty());
    }

    #[test]
    fn bookmarks_are_scoped_per_document() {
        let mut store = BookmarkStore::default();
        store.toggle("a.md", 1, 0, "S", "x");
        store.toggle("b.md", 2, 0, "S", "y");
        assert_eq!(store.for_document("a.md").count(), 1);
        assert_eq!(store.for_document("b.md").count(), 1);
        assert!(store.contains("a.md", 1));
        assert!(!store.contains("a.md", 2));
        // Un même index dans deux documents ne se confond pas.
        assert!(!store.contains("b.md", 1));
    }


    #[test]
    fn saving_leaves_no_temporary_file_next_to_the_bookmarks() {
        let dir = std::env::temp_dir().join(format!("sillon-bm-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("bookmarks.json");
        let mut store = BookmarkStore::default();
        store.toggle("a.md", 5, 2, "S", "x");
        store.save_to(&path).expect("écriture");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["bookmarks.json"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bookmarks_kept_under_a_former_name_follow_the_document() {
        let mut store = BookmarkStore::default();
        store.toggle("spec.pdf", 1, 0, "S", "x");
        store.toggle("spec.pdf", 7, 0, "S", "y");
        store.adopt("spec.pdf", "/docs/a/spec.pdf");
        assert_eq!(store.for_document("/docs/a/spec.pdf").count(), 2);
        assert_eq!(store.for_document("spec.pdf").count(), 0);
    }
}
