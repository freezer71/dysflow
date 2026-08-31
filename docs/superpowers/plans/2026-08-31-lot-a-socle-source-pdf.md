# Lot A — Socle `source` et PDF à couche texte

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Faire lire au lecteur un PDF qui porte une couche texte, aussi
naturellement qu'un fichier markdown.

**Architecture:** Un module `src/source/` en amont du pipeline existant.
Il reconnaît la nature d'une entrée à ses octets d'en-tête, extrait le PDF page
par page via `pdf-extract`, et rend du markdown que `input::split_blocks`
consomme sans rien savoir de son origine. `main.rs` remplace un seul appel :
`read_to_string` devient `source::load`.

**Tech Stack:** Rust 2021, `pdf-extract 0.12` (pur Rust), `anyhow`.

**Spec:** `docs/superpowers/specs/2026-08-30-lecture-pdf-et-images-design.md`

## Global Constraints

- **Aucun `cfg(target_os)` nouveau.** Cibles : macOS, Linux x86_64, Linux
  ARM64. Le code doit se comporter identiquement sur les trois.
- **Commentaires, messages et noms de tests** : commentaires et messages
  d'erreur en français, noms de fonctions de test en anglais descriptif. C'est
  la convention du dépôt — voir `src/input.rs`.
- **Un commentaire dit pourquoi, pas quoi.** Le dépôt n'a pas de commentaire
  redondant ; n'en ajoutez pas.
- **`rust-version = "1.80"`** : rien qui exige plus récent.
- **Pas de `unwrap()` ni de `expect()` hors tests.** Toute erreur remonte en
  `anyhow::Result` et nomme le fichier en cause.
- **Le build par défaut reste rapide.** `pdf-extract` est pur Rust et léger :
  il entre dans les dépendances normales, sans feature.
- Après chaque tâche : `cargo test` et `cargo clippy --all-targets` doivent
  être propres, sans avertissement.

---

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `src/source/mod.rs` | Routage : à partir d'un chemin, rendre du markdown |
| `src/source/kind.rs` | Reconnaître la nature d'une entrée à ses octets |
| `src/source/pdf.rs` | Extraire un PDF page par page |
| `src/source/layout.rs` | Assembler des pages en markdown |
| `tests/fixtures/spec.pdf` | PDF de deux pages, accentué, écrit à la main |
| `tests/fixtures/make_spec_pdf.py` | Le script qui l'a produit, pour le régénérer |
| `tests/pdf_input.rs` | Bout en bout : fixture → markdown → blocs |

`layout.rs` naît volontairement pauvre : il ne fait au lot A que ce qui ne
demande aucune interprétation. Les règles de reconstruction — césures,
paragraphes, titres, en-têtes répétés — sont le lot B et viendront s'y ajouter.

---

### Task 0: Initialiser le dépôt git

Le projet n'est pas sous git. Sans lui, aucune étape de commit de ce plan n'est
possible et rien n'est réversible.

**Files:**
- Create: `.gitignore`

**Interfaces:**
- Consumes: rien
- Produces: un dépôt où les tâches suivantes peuvent committer

- [ ] **Step 1: Vérifier qu'on n'écrase pas un dépôt existant**

```bash
cd /Users/tharsan/Workspace/reader
git rev-parse --is-inside-work-tree 2>&1 | head -1
```

Attendu : `fatal: not a git repository`. Si un dépôt existe déjà, **arrêtez** :
cette tâche est sans objet, passez à la tâche 1.

- [ ] **Step 2: Écrire le `.gitignore`**

```gitignore
/target
```

- [ ] **Step 3: Initialiser et faire le premier commit**

```bash
git init
git add -A
git commit -m "chore: mise sous suivi de version du lecteur"
```

- [ ] **Step 4: Vérifier que `target/` est bien ignoré**

Run: `git status --porcelain | grep -c '^?? target/'`
Attendu : `0`

---

### Task 1: Reconnaître la nature d'une entrée

**Files:**
- Create: `src/source/kind.rs`
- Create: `src/source/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: rien
- Produces:
  - `pub enum Kind { Text, Pdf, Image, Directory }` (dérive `Debug, Clone,
    Copy, PartialEq, Eq`)
  - `pub fn sniff(head: &[u8]) -> Kind`
  - `pub fn of_path(path: &Path) -> anyhow::Result<Kind>`

- [ ] **Step 1: Écrire le test qui échoue**

Créez `src/source/kind.rs` avec **seulement** ceci :

```rust
//! Nature d'une entrée, décidée sur ses premiers octets.
//!
//! L'extension ment : un PDF enregistré en `.txt` reste un PDF, et une capture
//! d'écran renommée reste une image. Les octets tranchent, jamais le nom.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Pdf,
    Image,
    Directory,
}

pub fn sniff(_head: &[u8]) -> Kind {
    todo!("reconnaissance des signatures")
}

pub fn of_path(_path: &Path) -> Result<Kind> {
    todo!("lecture de l'en-tête")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_is_recognised_by_its_header() {
        assert_eq!(sniff(b"%PDF-1.7\n1 0 obj"), Kind::Pdf);
    }

    #[test]
    fn the_usual_image_signatures_are_recognised() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Kind::Image);
        assert_eq!(sniff(b"\xff\xd8\xff\xe0\0\x10JFIF\0\x01"), Kind::Image);
        assert_eq!(sniff(b"II*\0\x08\0\0\0"), Kind::Image);
        assert_eq!(sniff(b"MM\0*\0\0\0\x08"), Kind::Image);
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Kind::Image);
        assert_eq!(sniff(b"BM\x36\0\0\0\0\0\0\0"), Kind::Image);
    }

    #[test]
    fn anything_else_is_read_as_text() {
        assert_eq!(sniff(b"# Titre\n\nUn paragraphe."), Kind::Text);
        assert_eq!(sniff(b""), Kind::Text);
    }

    #[test]
    fn a_video_is_not_mistaken_for_an_image() {
        // `ftyp` couvre les vidéos autant que le HEIC : seule la marque
        // qui suit distingue les deux.
        assert_eq!(sniff(b"\0\0\0\x18ftypmp42"), Kind::Text);
        assert_eq!(sniff(b"\0\0\0\x18ftypheic"), Kind::Image);
    }

    #[test]
    fn the_header_wins_over_the_extension() {
        let path = std::env::temp_dir().join("rs-reader-menteur.txt");
        std::fs::write(&path, b"%PDF-1.4\n1 0 obj\n").expect("écriture du témoin");
        let kind = of_path(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(kind.expect("lecture du témoin"), Kind::Pdf);
    }

    #[test]
    fn a_directory_is_recognised_without_being_opened() {
        assert_eq!(
            of_path(&std::env::temp_dir()).expect("lecture du dossier"),
            Kind::Directory
        );
    }

    #[test]
    fn a_missing_file_names_itself_in_the_error() {
        let err = of_path(Path::new("/introuvable/absolument")).unwrap_err();
        assert!(
            err.to_string().contains("introuvable"),
            "l'erreur doit nommer le fichier : {err}"
        );
    }
}
```

Créez `src/source/mod.rs` :

```rust
//! Entrée non textuelle : le PDF aujourd'hui, les images et les dossiers plus tard.

pub mod kind;

pub use kind::Kind;
```

Ajoutez dans `src/lib.rs`, en gardant l'ordre alphabétique des modules :

```rust
pub mod source;
```

- [ ] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::kind`
Attendu : ÉCHEC — chaque test panique sur `not yet implemented`.

- [ ] **Step 3: Écrire l'implémentation minimale**

Remplacez les deux `todo!()` :

```rust
/// Octets suffisants pour reconnaître toutes les signatures connues.
const HEAD: usize = 16;

/// Reconnaît une entrée à ses premiers octets.
pub fn sniff(head: &[u8]) -> Kind {
    if head.starts_with(b"%PDF-") {
        return Kind::Pdf;
    }
    let heif = head.len() >= 12
        && &head[4..8] == b"ftyp"
        && matches!(&head[8..12], b"heic" | b"heix" | b"hevc" | b"mif1" | b"avif");
    let webp = head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP";
    if head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"\xff\xd8\xff")
        || head.starts_with(b"II*\0")
        || head.starts_with(b"MM\0*")
        || head.starts_with(b"BM")
        || webp
        || heif
    {
        return Kind::Image;
    }
    Kind::Text
}

/// Reconnaît ce que désigne un chemin, sans le lire entièrement.
pub fn of_path(path: &Path) -> Result<Kind> {
    if path.is_dir() {
        return Ok(Kind::Directory);
    }
    let mut file =
        File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut head = [0u8; HEAD];
    let read = file
        .read(&mut head)
        .with_context(|| format!("lecture de {}", path.display()))?;
    Ok(sniff(&head[..read]))
}
```

- [ ] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::kind`
Attendu : 7 tests passent.

Run: `cargo clippy --all-targets 2>&1 | grep -c warning`
Attendu : `0`

- [ ] **Step 5: Commit**

```bash
git add src/source/kind.rs src/source/mod.rs src/lib.rs
git commit -m "feat(source): reconnaître PDF, image et texte aux octets d'en-tête"
```

---

### Task 2: Extraire un PDF page par page

**Files:**
- Create: `src/source/pdf.rs`
- Create: `tests/fixtures/make_spec_pdf.py`
- Create: `tests/fixtures/spec.pdf` (produit par le script)
- Create: `tests/fixtures/tronque.pdf`
- Modify: `src/source/mod.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: rien de la tâche 1
- Produces:
  - `pub struct Page { pub lines: Vec<String> }` (dérive `Debug, Clone,
    Default, PartialEq, Eq`)
  - `pub fn pages(path: &Path) -> anyhow::Result<Vec<Page>>`

Le découpage par page n'est pas un confort : les règles du lot B qui suppriment
les en-têtes répétés et recollent les phrases coupées par un saut de page ne
peuvent pas travailler sur un document aplati.

- [ ] **Step 1: Créer la fixture PDF**

Créez `tests/fixtures/make_spec_pdf.py`. Ce script écrit un PDF de deux pages,
sans dépendance : les accents passent par `WinAnsiEncoding`, qui est du
cp1252 — c'est la raison de l'encodage ci-dessous.

```python
#!/usr/bin/env python3
"""Écrit tests/fixtures/spec.pdf : deux pages accentuées, sans dépendance.

Le PDF est écrit à la main plutôt que produit par un outil : la fixture doit
rester reproductible à l'octet près sur les trois plateformes cibles.
"""
import pathlib


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def page_stream(lines):
    out = ["BT", "/F1 12 Tf", "72 720 Td", "14 TL"]
    for i, line in enumerate(lines):
        if i:
            out.append("T*")
        out.append("(%s) Tj" % esc(line))
    out.append("ET")
    return "\n".join(out).encode("cp1252")  # WinAnsiEncoding


PAGE1 = [
    "Spécification — Passerelle de paiement",
    "",
    "1. Contexte",
    "",
    "Le système doit gérer les règlements différés, puis",
    "confirmer chaque opération.",
]
PAGE2 = [
    "2. Contraintes",
    "",
    "La latence maximale est fixée à 250 ms.",
    "Les montants sont exprimés en euros.",
]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
       b"/Resources << /Font << /F1 5 0 R >> >> >>",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
       b"/Encoding /WinAnsiEncoding >>",
    6: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 7 0 R "
       b"/Resources << /Font << /F1 5 0 R >> >> >>",
}
for num, page in ((4, PAGE1), (7, PAGE2)):
    body = page_stream(page)
    objs[num] = b"<< /Length %d >>\nstream\n" % len(body) + body + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"

xref = len(buf)
buf += b"xref\n0 %d\n" % (len(objs) + 1)
buf += b"0000000000 65535 f \n"
for num in sorted(objs):
    buf += b"%010d 00000 n \n" % offsets[num]
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    len(objs) + 1,
    xref,
)

out = pathlib.Path(__file__).with_name("spec.pdf")
out.write_bytes(bytes(buf))
print("écrit %s : %d octets" % (out, len(buf)))
```

Puis produisez la fixture et un fichier tronqué :

```bash
python3 tests/fixtures/make_spec_pdf.py
printf '%%PDF-1.4\ntronqu\xc3\xa9 volontairement' > tests/fixtures/tronque.pdf
```

Attendu : `écrit …/spec.pdf : 1157 octets`

- [ ] **Step 2: Ajouter la dépendance**

Dans `Cargo.toml`, sous `[dependencies]`, après `pulldown-cmark` :

```toml
pdf-extract = "0.12.0"
```

- [ ] **Step 3: Écrire le test qui échoue**

Créez `src/source/pdf.rs` :

```rust
//! Extraction du texte d'un PDF qui en porte une couche.
//!
//! Le texte est rendu page par page, et non d'un bloc : les règles de
//! reconstruction ont besoin de savoir où une page finit.

use anyhow::Result;
use std::path::Path;

/// Une page telle que le PDF la livre, avant toute reconstruction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub lines: Vec<String>,
}

pub fn pages(_path: &Path) -> Result<Vec<Page>> {
    todo!("extraction page par page")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_pdf_yields_one_entry_per_page() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        assert_eq!(pages.len(), 2, "le document a deux pages");
    }

    #[test]
    fn accents_survive_the_extraction() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        let first = pages[0].lines.join("\n");
        assert!(
            first.contains("Spécification"),
            "les accents ont été perdus : {first}"
        );
        assert!(
            first.contains("règlements différés"),
            "les accents ont été perdus : {first}"
        );
    }

    #[test]
    fn each_page_holds_only_its_own_text() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        let second = pages[1].lines.join("\n");
        assert!(second.contains("2. Contraintes"), "page 2 : {second}");
        assert!(
            !second.contains("1. Contexte"),
            "la page 1 a débordé sur la page 2 : {second}"
        );
    }

    #[test]
    fn lines_come_back_without_their_right_padding() {
        let pages = pages(&fixture("spec.pdf")).expect("la fixture doit se lire");
        for line in pages.iter().flat_map(|p| &p.lines) {
            assert_eq!(line.trim_end(), line, "reste une fin de ligne : {line:?}");
        }
    }

    #[test]
    fn a_broken_file_names_itself_in_the_error() {
        let err = pages(&fixture("tronque.pdf")).unwrap_err();
        assert!(
            err.to_string().contains("tronque.pdf"),
            "l'erreur doit nommer le fichier : {err}"
        );
    }
}
```

Déclarez le module dans `src/source/mod.rs` :

```rust
pub mod pdf;
```

- [ ] **Step 4: Lancer le test et le voir échouer**

Run: `cargo test --lib source::pdf`
Attendu : ÉCHEC — `not yet implemented` sur les cinq tests.

- [ ] **Step 5: Écrire l'implémentation minimale**

Remplacez l'import et le `todo!()` :

```rust
use anyhow::{bail, Context, Result};
use pdf_extract::{output_doc_page, Document, PlainTextOutput};
use std::path::Path;
```

```rust
/// Rend le texte du document, une entrée par page.
pub fn pages(path: &Path) -> Result<Vec<Page>> {
    let mut doc =
        Document::load(path).with_context(|| format!("lecture de {}", path.display()))?;
    // Un PDF « protégé » sans mot de passe s'ouvre avec une chaîne vide ; les
    // autres ne s'ouvriront pas, et mieux vaut le dire que rendre un document
    // vide.
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        bail!("{} est protégé par un mot de passe", path.display());
    }
    let mut out = Vec::new();
    for (number, _) in doc.get_pages() {
        let mut text = String::new();
        {
            let mut sink = PlainTextOutput::new(&mut text);
            output_doc_page(&doc, &mut sink, number)
                .with_context(|| format!("page {number} de {}", path.display()))?;
        }
        out.push(Page {
            lines: text.lines().map(|line| line.trim_end().to_string()).collect(),
        });
    }
    Ok(out)
}
```

- [ ] **Step 6: Lancer les tests et les voir passer**

Run: `cargo test --lib source::pdf`
Attendu : 5 tests passent.

Si `a_broken_file_names_itself_in_the_error` échoue parce que `Document::load`
accepte le fichier tronqué, c'est que lopdf a reconstruit la table des objets.
Dans ce cas le document n'a aucune page : ajoutez, juste avant la boucle,

```rust
    if doc.get_pages().is_empty() {
        bail!("{} ne contient aucune page lisible", path.display());
    }
```

et relancez.

- [ ] **Step 7: Commit**

```bash
git add src/source/pdf.rs src/source/mod.rs Cargo.toml Cargo.lock tests/fixtures
git commit -m "feat(source): extraire un PDF page par page"
```

---

### Task 3: Assembler les pages en markdown

**Files:**
- Create: `src/source/layout.rs`
- Modify: `src/source/mod.rs`

**Interfaces:**
- Consumes: `source::pdf::Page` (tâche 2)
- Produces: `pub fn to_markdown(pages: &[Page]) -> String`

Ce module portera au lot B toute la reconstruction. Au lot A il ne fait que ce
qui ne demande aucune interprétation du sens : réduire les blancs, séparer les
pages, ne rien laisser traîner en fin de texte.

- [ ] **Step 1: Écrire le test qui échoue**

Créez `src/source/layout.rs` :

```rust
//! Reconstruction d'un texte mis en page en markdown lisible.
//!
//! Un PDF ne livre que des lignes placées sur une feuille. Les rendre telles
//! quelles ferait de chaque ligne un bloc, et la navigation deviendrait
//! inutilisable. Ce module leur redonne une structure.

use super::pdf::Page;

pub fn to_markdown(_pages: &[Page]) -> String {
    todo!("assemblage des pages")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> Page {
        Page {
            lines: lines.iter().map(|l| l.to_string()).collect(),
        }
    }

    #[test]
    fn a_single_page_keeps_its_lines_in_order() {
        let out = to_markdown(&[page(&["1. Contexte", "", "Le service expose deux routes."])]);
        assert_eq!(out, "1. Contexte\n\nLe service expose deux routes.");
    }

    #[test]
    fn runs_of_blank_lines_collapse_into_one() {
        let out = to_markdown(&[page(&["Titre", "", "", "", "Suite"])]);
        assert_eq!(out, "Titre\n\nSuite");
    }

    #[test]
    fn right_padding_disappears() {
        let out = to_markdown(&[page(&["Titre    ", "Suite\t"])]);
        assert_eq!(out, "Titre\nSuite");
    }

    #[test]
    fn two_pages_are_separated_by_a_blank_line() {
        let out = to_markdown(&[page(&["fin de page"]), page(&["début de page"])]);
        assert_eq!(out, "fin de page\n\ndébut de page");
    }

    #[test]
    fn a_page_break_never_doubles_an_existing_blank() {
        let out = to_markdown(&[page(&["fin", ""]), page(&["suite"])]);
        assert_eq!(out, "fin\n\nsuite", "le saut de page a ajouté un blanc de trop");
    }

    #[test]
    fn nothing_trails_at_the_end() {
        let out = to_markdown(&[page(&["seule", "", ""])]);
        assert_eq!(out, "seule");
    }

    #[test]
    fn an_empty_document_yields_nothing() {
        assert_eq!(to_markdown(&[]), "");
        assert_eq!(to_markdown(&[page(&["", "  "])]), "");
    }
}
```

Déclarez le module dans `src/source/mod.rs` :

```rust
pub mod layout;
```

- [ ] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC — `not yet implemented` sur les sept tests.

- [ ] **Step 3: Écrire l'implémentation minimale**

```rust
/// Assemble des pages en un markdown que `input::split_blocks` sait découper.
pub fn to_markdown(pages: &[Page]) -> String {
    let mut out: Vec<String> = Vec::new();
    for page in pages {
        // Une page qui s'ouvre ne colle pas à la précédente, mais ne creuse
        // pas non plus un second blanc là où il y en a déjà un.
        if out.last().is_some_and(|last| !last.is_empty()) {
            out.push(String::new());
        }
        for line in &page.lines {
            let line = line.trim_end();
            if line.is_empty() {
                if !out.is_empty() && out.last().is_some_and(|last| !last.is_empty()) {
                    out.push(String::new());
                }
            } else {
                out.push(line.to_string());
            }
        }
    }
    while out.last().is_some_and(|last| last.is_empty()) {
        out.pop();
    }
    out.join("\n")
}
```

- [ ] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 7 tests passent.

Run: `cargo clippy --all-targets 2>&1 | grep -c warning`
Attendu : `0`

- [ ] **Step 5: Commit**

```bash
git add src/source/layout.rs src/source/mod.rs
git commit -m "feat(source): assembler les pages extraites en markdown"
```

---

### Task 4: Router un chemin vers son extracteur

**Files:**
- Modify: `src/source/mod.rs`

**Interfaces:**
- Consumes: `kind::of_path` (tâche 1), `pdf::pages` (tâche 2),
  `layout::to_markdown` (tâche 3)
- Produces: `pub fn load(path: &Path) -> anyhow::Result<String>`

Les images et les dossiers sont reconnus mais refusés : ils arrivent au lot C.
Un refus qui nomme la raison vaut mieux qu'une lecture silencieusement vide.

- [ ] **Step 1: Écrire le test qui échoue**

Complétez `src/source/mod.rs` :

```rust
//! Entrée non textuelle : le PDF aujourd'hui, les images et les dossiers plus tard.
//!
//! Tout ce qui entre dans le lecteur passe par ici et en ressort en markdown ;
//! le reste du pipeline ignore d'où vient le texte.

pub mod kind;
pub mod layout;
pub mod pdf;

use anyhow::{bail, Context, Result};
use std::path::Path;

pub use kind::Kind;

pub fn load(_path: &Path) -> Result<String> {
    todo!("routage vers l'extracteur")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_text_file_comes_back_unchanged() {
        let path = std::env::temp_dir().join("rs-reader-source-texte.md");
        std::fs::write(&path, "# Titre\n\nUn paragraphe.\n").expect("écriture du témoin");
        let out = load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(out.expect("lecture"), "# Titre\n\nUn paragraphe.\n");
    }

    #[test]
    fn a_pdf_comes_back_as_markdown() {
        let out = load(&fixture("spec.pdf")).expect("la fixture doit se lire");
        assert!(out.contains("Spécification"), "accents perdus : {out}");
        assert!(out.contains("2. Contraintes"), "seconde page absente : {out}");
    }

    #[test]
    fn an_image_says_what_is_missing_rather_than_failing_flatly() {
        let path = std::env::temp_dir().join("rs-reader-source-image.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("écriture du témoin");
        let err = load(&path).unwrap_err();
        let _ = std::fs::remove_file(&path);
        let message = err.to_string();
        assert!(message.contains("image"), "l'erreur doit dire ce qui bloque : {message}");
        assert!(
            message.contains("reconnaissance de texte"),
            "l'erreur doit nommer ce qui manque : {message}"
        );
    }

    #[test]
    fn a_directory_says_so_too() {
        let err = load(&std::env::temp_dir()).unwrap_err();
        assert!(
            err.to_string().contains("dossier"),
            "l'erreur doit dire ce qui bloque : {err}"
        );
    }
}
```

- [ ] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::tests`
Attendu : ÉCHEC — `not yet implemented` sur les quatre tests.

- [ ] **Step 3: Écrire l'implémentation minimale**

```rust
/// Rend le contenu d'un chemin sous forme de markdown, quelle qu'en soit
/// l'origine.
pub fn load(path: &Path) -> Result<String> {
    match kind::of_path(path)? {
        Kind::Text => std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display())),
        Kind::Pdf => Ok(layout::to_markdown(&pdf::pages(path)?)),
        Kind::Image => bail!(
            "{} est une image : la reconnaissance de texte n'est pas encore disponible",
            path.display()
        ),
        Kind::Directory => bail!(
            "{} est un dossier : la lecture d'un dossier d'images n'est pas encore disponible",
            path.display()
        ),
    }
}
```

- [ ] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source`
Attendu : les 23 tests du module passent (7 + 5 + 7 + 4).

- [ ] **Step 5: Commit**

```bash
git add src/source/mod.rs
git commit -m "feat(source): router un chemin vers son extracteur"
```

---

### Task 5: Brancher le lecteur et ajouter `--to-markdown`

**Files:**
- Modify: `src/main.rs:113-128` (la branche `Some(path)`), et la structure `Args`
- Create: `tests/pdf_input.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `source::load` (tâche 4), `input::split_blocks` (existant)
- Produces: le comportement visible — `reader spec.pdf` et
  `reader spec.pdf --to-markdown`

- [ ] **Step 1: Écrire le test de bout en bout qui échoue**

Le test porte sur le **binaire**, pas sur la bibliothèque : `source::load`
fonctionne déjà après la tâche 4, alors que `--to-markdown` n'existe pas
encore. C'est cette option qui doit faire échouer le test.

Créez `tests/pdf_input.rs` :

```rust
//! Lecture d'un PDF : de la fixture jusqu'aux blocs que le lecteur navigue.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn the_binary_writes_the_extracted_markdown_and_stops() {
    let out = Command::new(env!("CARGO_BIN_EXE_reader"))
        .arg(fixture("spec.pdf"))
        .arg("--to-markdown")
        .output()
        .expect("le binaire doit se lancer");

    assert!(
        out.status.success(),
        "sortie en erreur : {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("sortie en UTF-8");
    assert!(text.contains("Spécification"), "accents perdus : {text}");
    assert!(text.contains("2. Contraintes"), "seconde page absente : {text}");
}

#[test]
fn a_pdf_becomes_markdown_the_reader_can_navigate() {
    let markdown = rs_reader::source::load(&fixture("spec.pdf")).expect("le PDF doit se lire");
    let blocks = rs_reader::input::split_blocks(&markdown);
    assert!(
        blocks.len() >= 2,
        "le document doit se découper en blocs navigables, obtenu {} : {blocks:?}",
        blocks.len()
    );
}

#[test]
fn a_pdf_produces_sentences_to_speak() {
    use rs_reader::doc::builder::{parse_document, CodePolicy};

    let markdown = rs_reader::source::load(&fixture("spec.pdf")).expect("le PDF doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    assert!(!doc.sentences.is_empty(), "aucune phrase à prononcer");
    let spoken: String = doc.sentences.iter().map(|s| s.text.as_str()).collect();
    assert!(
        spoken.contains("règlements"),
        "le texte du PDF n'a pas atteint la synthèse : {spoken}"
    );
}
```

- [ ] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --test pdf_input`
Attendu : ÉCHEC de `the_binary_writes_the_extracted_markdown_and_stops` —
clap rejette une option inconnue, le binaire sort en code 2 et le message
contient `unexpected argument '--to-markdown'`.

Les deux autres tests passent déjà : ils vérifient le câblage des tâches 1 à 4.
**Si l'un des deux échoue**, c'est une régression de ce lot : corrigez-la avant
d'aller plus loin.

- [ ] **Step 3: Ajouter l'option à la ligne de commande**

Dans `src/main.rs`, dans `struct Args`, juste après le champ `show_config` :

```rust
    /// Écrit le markdown extrait du fichier puis quitte.
    #[arg(long)]
    to_markdown: bool,
```

- [ ] **Step 4: Brancher `source::load` dans la lecture du fichier**

Dans `src/main.rs`, branche `Some(path)`, remplacez :

```rust
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("lecture de {}", path.display()))?;
```

par :

```rust
            let text = rs_reader::source::load(path)?;
            if args.to_markdown {
                print!("{text}");
                return Ok(());
            }
```

- [ ] **Step 5: Vérifier que le binaire lit un PDF**

```bash
cargo run -- tests/fixtures/spec.pdf --to-markdown
```

Attendu, exactement :

```text
Spécification — Passerelle de paiement

1. Contexte

Le système doit gérer les règlements différés, puis
confirmer chaque opération.

2. Contraintes

La latence maximale est fixée à 250 ms.
Les montants sont exprimés en euros.
```

Les deux dernières lignes de la page 1 restent séparées : la reconstruction des
paragraphes est le lot B. C'est le comportement attendu ici.

- [ ] **Step 6: Vérifier que rien n'a régressé**

Run: `cargo test`
Attendu : toute la suite passe. Le dépôt comptait 232 tests de
bibliothèque avant ce lot ; il doit en compter 255, plus les tests
d'intégration.

Run: `cargo clippy --all-targets 2>&1 | grep -c warning`
Attendu : `0`

`Context` doit rester importé dans `main.rs` : il sert encore à
`detach_piped_stdin`. Si clippy signale un import inutilisé, c'est que la
substitution de l'étape 4 a été faite au mauvais endroit.

- [ ] **Step 7: Documenter**

Dans `README.md`, section « Ce qu'il fait », ajoutez une puce :

```markdown
- Lit un fichier markdown, texte ou **PDF** ; `--to-markdown` écrit le texte extrait sans lancer la lecture
```

- [ ] **Step 8: Commit**

```bash
git add src/main.rs tests/pdf_input.rs README.md
git commit -m "feat: lire un PDF et exposer --to-markdown"
```

---

## Ce que ce lot ne fait pas

À vérifier avant de déclarer le lot fini — ces manques sont voulus :

| | Lot |
|---|---|
| Césures, paragraphes, titres, en-têtes répétés | B |
| OCR, images, dossiers, presse-papiers | C |
| PDF scannés, flux page par page, cache | D |

Le rendu du lot A garde donc une ligne par ligne du PDF. C'est un jalon
technique, pas encore un confort de lecture.
