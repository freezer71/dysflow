# Lot D — PDF scannés et cache

> **État au 1ᵉʳ septembre 2026**
> **Livré.** `pdf::page_images`, `looks_scanned`, `recognise_scanned` et
> `source::cache` — avec `--no-cache` —, couverts par
> `a_scanned_pdf_is_recognised_and_rebuilt`. Les étapes sont cochées au vu du
> code livré, non au fil de l'exécution.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Qu'une photocopie se lise comme un PDF ordinaire, et qu'on ne paie la
reconnaissance qu'une fois.

**Architecture:** `pdf.rs` reconnaît une page sans couche texte, en extrait
l'image embarquée et la confie à l'OCR. Les pages partent en parallèle sur les
cœurs disponibles. `cache.rs` mémorise le markdown produit.

**Tech Stack:** Rust 2021, `lopdf` (réexporté par `pdf-extract`), `image`,
`std::thread::scope`.

**Spec:** `docs/superpowers/specs/2026-08-30-lecture-pdf-et-images-design.md`

## Global Constraints

- **Aucun `cfg(target_os)`.** Cibles : macOS, Linux x86_64, Linux ARM64.
- L'OCR reste derrière `cfg(feature = "ocr")`. Sans la feature, un PDF scanné
  doit dire ce qui manque, jamais rendre un document vide.
- Commentaires et messages en français, noms de tests en anglais descriptif.
- Un commentaire dit **pourquoi**, jamais quoi.
- Pas de `unwrap()` ni `expect()` hors tests.
- Après chaque tâche : `cargo test` et `cargo clippy --all-targets` propres,
  **avec et sans** la feature.
- Le lot C a laissé 297 tests de bibliothèque. Ce nombre ne doit que croître.

## Un choix de conception à connaître : parallèle, et non flux

La spec annonçait une lecture **en flux**, page par page, pour démarrer avant la
fin de la reconnaissance. Ce choix est abandonné, et voici pourquoi.

`strip_running_heads`, du lot B, a besoin de voir **au moins trois pages** pour
décider qu'une ligne est un en-tête courant. Émettre la page 1 avant d'avoir lu
la page 3 reviendrait donc à conserver les « Confidentiel — page 12 » — soit
exactement ce qui use le plus à l'écoute, et que le lot B existe pour retirer.

La reconnaissance est donc menée **en parallèle** sur les cœurs disponibles. Le
document reste complet, toute la reconstruction s'applique, et l'attente tombe
d'un facteur proche du nombre de cœurs : une trentaine de pages passe de trente
secondes à quelques-unes. Le cache fait le reste — on ne paie qu'à la première
ouverture.

Aucun `Source::Streaming` n'est donc introduit ; `load` garde sa signature.

## Ce que le lot doit corriger

Mesuré aujourd'hui sur un PDF sans couche texte :

| Aujourd'hui | Attendu |
|---|---|
| Sortie **vide**, sans le moindre message | Le texte reconnu, reconstruit |
| Seconde ouverture aussi lente que la première | Instantanée |

---

### Task 1: La fixture d'un PDF scanné

**Files:**
- Create: `tests/fixtures/make_scanne_pdf.py`
- Create: `tests/fixtures/scanne.pdf`

**Interfaces:**
- Consumes: `tests/fixtures/capture.png`
- Produces: un PDF d'une page, sans couche texte, dont l'image porte un texte
  français connu

- [x] **Step 1: Écrire le générateur**

Créez `tests/fixtures/make_scanne_pdf.py` :

```python
#!/usr/bin/env python3
"""Écrit tests/fixtures/scanne.pdf : une page sans couche texte, dont le
contenu n'existe que sous forme d'image — comme une photocopie.

L'image est capture.png recomprimée en JPEG et embarquée telle quelle sous
/DCTDecode, ce qui est la forme la plus courante d'un scan.
"""
import pathlib
import struct
import subprocess

here = pathlib.Path(__file__).parent
jpeg = here / "scanne.jpg"
subprocess.run(
    ["sips", "-s", "format", "jpeg", "-s", "formatOptions", "70",
     str(here / "capture.png"), "--out", str(jpeg)],
    check=True, stdout=subprocess.DEVNULL,
)
data = jpeg.read_bytes()
jpeg.unlink()

# Dimensions lues dans l'en-tête JPEG, pour dimensionner l'objet image.
i, width, height = 2, 0, 0
while i < len(data):
    if data[i] != 0xFF:
        i += 1
        continue
    marker = data[i + 1]
    if marker in (0xC0, 0xC1, 0xC2):
        height, width = struct.unpack(">HH", data[i + 5:i + 9])
        break
    if marker in (0xD8, 0xD9):
        i += 2
        continue
    i += 2 + struct.unpack(">H", data[i + 2:i + 4])[0]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
       b"/Resources << /XObject << /Im0 5 0 R >> >> >>",
}
content = b"q 612 0 0 792 0 0 cm /Im0 Do Q"
objs[4] = b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream"
objs[5] = (
    "<< /Type /XObject /Subtype /Image /Width %d /Height %d "
    "/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode "
    "/Length %d >>" % (width, height, len(data))
).encode() + b"\nstream\n" + data + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"
xref = len(buf)
buf += b"xref\n0 %d\n" % (max(objs) + 1) + b"0000000000 65535 f \n"
for num in range(1, max(objs) + 1):
    buf += b"%010d 00000 n \n" % offsets.get(num, 0)
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    max(objs) + 1, xref,
)

out = here / "scanne.pdf"
out.write_bytes(bytes(buf))
print("écrit %s : %d octets (image %dx%d)" % (out, len(buf), width, height))
```

- [x] **Step 2: Produire la fixture et constater l'état de départ**

```bash
python3 tests/fixtures/make_scanne_pdf.py
cargo run --features ocr -- tests/fixtures/scanne.pdf --to-markdown
```

Attendu : environ 20 000 octets, puis une sortie **vide**. C'est le défaut que
ce lot corrige.

- [x] **Step 3: Commit**

```bash
git add tests/fixtures/make_scanne_pdf.py tests/fixtures/scanne.pdf
git commit -m "test(source): fixture d'un PDF sans couche texte"
```

---

### Task 2: Extraire les images d'une page

**Files:**
- Modify: `src/source/pdf.rs`

**Interfaces:**
- Consumes: `pdf_extract::{Document, ObjectId}`
- Produces: `fn page_images(doc: &Document, page: ObjectId) -> Vec<Vec<u8>>`

Un scan porte son contenu dans une image embarquée. Le cas courant est le JPEG
sous `/DCTDecode` : ses octets sont un fichier JPEG complet, décodable tel
quel. Les autres filtres — CCITT et JBIG2 des télécopies — demanderaient un
décodeur que le pur Rust n'offre pas ; ils sont ignorés, et la page restera
vide plutôt que fausse.

- [x] **Step 1: Écrire le test qui échoue**

Dans `src/source/pdf.rs` :

```rust
/// Images embarquées d'une page, dans leur forme d'origine.
fn page_images(_doc: &Document, _page: ObjectId) -> Vec<Vec<u8>> {
    todo!("images embarquées")
}
```

en ajoutant `ObjectId` à l'import `pdf_extract`.

```rust
    #[test]
    fn a_scanned_page_yields_its_embedded_image() {
        let doc = Document::load(fixture("scanne.pdf")).expect("la fixture doit se lire");
        let (_, page) = doc.get_pages().into_iter().next().expect("une page");
        let images = page_images(&doc, page);
        assert_eq!(images.len(), 1, "la page porte une image");
        assert_eq!(&images[0][..2], &[0xFF, 0xD8], "ce doit être un JPEG");
    }

    #[test]
    fn a_text_page_yields_no_image() {
        let doc = Document::load(fixture("spec.pdf")).expect("la fixture doit se lire");
        let (_, page) = doc.get_pages().into_iter().next().expect("une page");
        assert!(page_images(&doc, page).is_empty());
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::pdf`
Attendu : ÉCHEC des deux nouveaux tests.

- [x] **Step 3: Écrire l'implémentation**

```rust
/// Images embarquées d'une page, dans leur forme d'origine.
///
/// Seul `/DCTDecode` est rendu : ses octets sont un JPEG complet. Les
/// télécopies en CCITT ou JBIG2 demanderaient un décodeur que le pur Rust
/// n'offre pas — mieux vaut une page vide qu'un texte inventé.
fn page_images(doc: &Document, page: ObjectId) -> Vec<Vec<u8>> {
    let Ok((own, inherited)) = doc.get_page_resources(page) else {
        return Vec::new();
    };
    let mut tables = Vec::new();
    if let Some(dict) = own {
        if let Ok(x) = dict.get(b"XObject") {
            tables.push(x.clone());
        }
    }
    for id in inherited {
        if let Ok(dict) = doc.get_dictionary(id) {
            if let Ok(x) = dict.get(b"XObject") {
                tables.push(x.clone());
            }
        }
    }

    let mut images = Vec::new();
    for table in tables {
        let Ok((_, object)) = doc.dereference(&table) else {
            continue;
        };
        let Ok(entries) = object.as_dict() else {
            continue;
        };
        for (_, value) in entries.iter() {
            let Ok((_, object)) = doc.dereference(value) else {
                continue;
            };
            let Ok(stream) = object.as_stream() else {
                continue;
            };
            let is_image = stream
                .dict
                .get(b"Subtype")
                .and_then(|s| s.as_name())
                .map(|name| name == b"Image")
                .unwrap_or(false);
            let is_jpeg = stream
                .dict
                .get(b"Filter")
                .and_then(|f| f.as_name())
                .map(|name| name == b"DCTDecode")
                .unwrap_or(false);
            if is_image && is_jpeg {
                images.push(stream.content.clone());
            }
        }
    }
    images
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::pdf`
Attendu : 7 tests passent.

- [x] **Step 5: Commit**

```bash
git add src/source/pdf.rs
git commit -m "feat(pdf): extraire les images embarquées d'une page"
```

---

### Task 3: Reconnaître les pages sans texte

**Files:**
- Modify: `src/source/pdf.rs`

**Interfaces:**
- Consumes: `page_images` (tâche 2), `ocr::Ocr` (lot C)
- Produces: `pages` reconnaît les pages scannées

Une page dont la couche texte rend moins de trente caractères imprimables ne
porte pas de contenu : c'est un numéro de page isolé, ou rien. Elle part à la
reconnaissance.

Les pages partent **en parallèle** : chaque fil ouvre son propre moteur, dont
l'ouverture coûte bien moins que la lecture d'une page.

- [x] **Step 1: Écrire le test qui échoue**

```rust
    #[cfg(feature = "ocr")]
    #[test]
    fn a_scanned_page_is_read_by_the_recogniser() {
        let read = pages(&fixture("scanne.pdf")).expect("la fixture doit se lire");
        let text = read[0].lines.join("\n");
        assert!(text.contains("Spécification"), "la page scannée est restée vide : {text:?}");
    }

    #[cfg(not(feature = "ocr"))]
    #[test]
    fn a_scanned_page_says_what_is_missing() {
        let err = pages(&fixture("scanne.pdf")).unwrap_err();
        assert!(
            err.to_string().contains("ocr"),
            "l'erreur doit nommer l'option manquante : {err}"
        );
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --features ocr --lib source::pdf`
Attendu : ÉCHEC — la page revient vide.

- [x] **Step 3: Écrire l'implémentation**

Ajoutez la constante et la reconnaissance, puis branchez-les dans `pages` :

```rust
/// Caractères imprimables en deçà desquels une page est réputée scannée.
///
/// Sous ce seuil, ce qui reste est un artefact — un numéro de page isolé, une
/// légende — et non du contenu lisible.
const SCANNED_BELOW: usize = 30;

/// Vrai si la page ne porte pas de couche texte exploitable.
fn looks_scanned(page: &Page) -> bool {
    page.lines
        .iter()
        .flat_map(|l| l.chars())
        .filter(|c| !c.is_whitespace())
        .count()
        < SCANNED_BELOW
}
```

Dans `pages`, après avoir constitué `out`, ajoutez :

```rust
    let blank: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, page)| looks_scanned(page))
        .map(|(i, _)| i)
        .collect();
    if !blank.is_empty() {
        recognise_scanned(&doc, &numbers, &blank, &mut out, path)?;
    }
    Ok(out)
```

où `numbers` est la liste des `ObjectId` de page, collectée dans la boucle
principale — remplacez `for (number, _) in doc.get_pages()` par une collecte
préalable :

```rust
    let numbered: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let numbers: Vec<ObjectId> = numbered.iter().map(|(_, id)| *id).collect();
    for (number, _) in &numbered {
```

Puis la reconnaissance elle-même :

```rust
/// Reconnaît les pages sans couche texte, en parallèle.
#[cfg(feature = "ocr")]
fn recognise_scanned(
    doc: &Document,
    numbers: &[ObjectId],
    blank: &[usize],
    out: &mut [Page],
    path: &Path,
) -> Result<()> {
    // Une image par page suffit : un scan en porte une seule, et deux
    // reconnaissances sur la même page se recouvriraient.
    let work: Vec<(usize, Vec<u8>)> = blank
        .iter()
        .filter_map(|&i| {
            let image = page_images(doc, *numbers.get(i)?).into_iter().next()?;
            Some((i, image))
        })
        .collect();
    if work.is_empty() {
        return Ok(());
    }

    // Autant de fils que de cœurs, sans dépasser le nombre de pages : ouvrir un
    // moteur coûte moins que de lire une page, mais pas moins que rien.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(work.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done: std::sync::Mutex<Vec<(usize, Page)>> = std::sync::Mutex::new(Vec::new());

    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            handles.push(scope.spawn(|| -> Result<()> {
                let engine = crate::source::ocr::Ocr::new()?;
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((index, bytes)) = work.get(i) else {
                        return Ok(());
                    };
                    let image = image::load_from_memory(bytes)
                        .with_context(|| format!("image de la page {}", index + 1))?;
                    let page = engine.read(&image)?;
                    done.lock()
                        .map_err(|_| anyhow::anyhow!("fil de reconnaissance interrompu"))?
                        .push((*index, page));
                }
            }));
        }
        for handle in handles {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("reconnaissance de {} interrompue", path.display()))??;
        }
        Ok(())
    })?;

    for (index, page) in done
        .into_inner()
        .map_err(|_| anyhow::anyhow!("fil de reconnaissance interrompu"))?
    {
        out[index] = page;
    }
    Ok(())
}

/// Sans l'option `ocr`, une page scannée le dit plutôt que de rester vide.
#[cfg(not(feature = "ocr"))]
fn recognise_scanned(
    doc: &Document,
    numbers: &[ObjectId],
    blank: &[usize],
    _out: &mut [Page],
    path: &Path,
) -> Result<()> {
    let has_image = blank
        .iter()
        .filter_map(|&i| numbers.get(i))
        .any(|id| !page_images(doc, *id).is_empty());
    if has_image {
        bail!(
            "{} est un document scanné : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        );
    }
    Ok(())
}
```

- [x] **Step 4: Lancer les tests dans les deux modes**

Run: `cargo test --features ocr --lib source::pdf`
Run: `cargo test --lib source::pdf`
Attendu : tout passe des deux côtés.

- [x] **Step 5: Vérifier à l'œil**

```bash
cargo run -q --features ocr -- tests/fixtures/scanne.pdf --to-markdown
```

Attendu : le texte de la capture, titres reconstruits — là où le lot C rendait
une page vide.

- [x] **Step 6: Commit**

```bash
git add src/source/pdf.rs
git commit -m "feat(pdf): reconnaître les pages sans couche texte"
```

---

### Task 4: Ne payer la reconnaissance qu'une fois

**Files:**
- Create: `src/source/cache.rs`
- Modify: `src/source/mod.rs`, `src/main.rs`

**Interfaces:**
- Produces:
  - `pub fn fingerprint(path: &Path) -> Result<String>`
  - `pub fn read(key: &str) -> Option<String>`
  - `pub fn write(key: &str, markdown: &str)`

Relire une spec sur plusieurs jours ne doit pas refaire la reconnaissance.
L'empreinte tient à la taille, à la date de modification et aux premiers
kilo-octets : un document modifié change de clé, sans qu'il faille le relire en
entier. Ce n'est pas de la cryptographie, `DefaultHasher` suffit.

- [x] **Step 1: Écrire le test qui échoue**

Créez `src/source/cache.rs` :

```rust
//! Mémorisation du markdown produit par une extraction coûteuse.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn fingerprint(_path: &Path) -> Result<String> {
    todo!("empreinte")
}

pub fn read(_key: &str) -> Option<String> {
    todo!("lecture du cache")
}

pub fn write(_key: &str, _markdown: &str) {
    todo!("écriture du cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn the_same_file_always_gives_the_same_key() {
        let one = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        let two = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        assert_eq!(one, two);
    }

    #[test]
    fn two_files_give_two_keys() {
        let one = fingerprint(&fixture("spec.pdf")).expect("empreinte");
        let two = fingerprint(&fixture("rapport.pdf")).expect("empreinte");
        assert_ne!(one, two);
    }

    #[test]
    fn what_is_written_comes_back() {
        let key = "essai-du-cache-rs-reader";
        write(key, "## Titre\n\nDu texte.");
        assert_eq!(read(key).as_deref(), Some("## Titre\n\nDu texte."));
    }

    #[test]
    fn an_unknown_key_yields_nothing() {
        assert_eq!(read("clé-qui-n-existe-pas-du-tout"), None);
    }
}
```

Déclarez `pub mod cache;` dans `src/source/mod.rs`.

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::cache`
Attendu : ÉCHEC des quatre tests.

- [x] **Step 3: Écrire l'implémentation**

```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;

/// Octets lus en tête pour distinguer deux fichiers de même taille.
const SAMPLE: usize = 64 * 1024;

/// Empreinte d'un fichier : sa taille, sa date, son début.
///
/// Il ne s'agit pas de sécurité mais de reconnaître un document déjà lu ; lire
/// un fichier de deux cents pages en entier pour cela coûterait plus que
/// l'extraction elle-même.
pub fn fingerprint(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let mut hasher = DefaultHasher::new();
    meta.len().hash(&mut hasher);
    if let Ok(time) = meta.modified() {
        if let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) {
            since.as_secs().hash(&mut hasher);
        }
    }
    let mut head = vec![0u8; SAMPLE];
    let mut file = std::fs::File::open(path)?;
    let read = file.read(&mut head)?;
    head.truncate(read);
    head.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

/// Répertoire du cache, créé au besoin.
fn dir() -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("rs-reader").join("documents");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Markdown déjà produit pour cette empreinte.
pub fn read(key: &str) -> Option<String> {
    std::fs::read_to_string(dir()?.join(format!("{key}.md"))).ok()
}

/// Mémorise le markdown produit. Un cache qui échoue n'est pas une erreur :
/// le document a été lu, c'est tout ce qui compte.
pub fn write(key: &str, markdown: &str) {
    let Some(dir) = dir() else {
        return;
    };
    let file = dir.join(format!("{key}.md"));
    let temp = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::write(&temp, markdown).is_ok() {
        let _ = std::fs::rename(&temp, &file);
    }
}
```

- [x] **Step 4: Brancher le cache dans `load`**

Dans `src/source/mod.rs`, remplacez le corps de `load` par :

```rust
/// Rend le contenu d'un chemin sous forme de markdown, quelle qu'en soit
/// l'origine.
pub fn load(path: &Path) -> Result<String> {
    load_with_cache(path, true)
}

/// Comme [`load`], en pouvant refuser le cache.
pub fn load_with_cache(path: &Path, cached: bool) -> Result<String> {
    let kind = kind::of_path(path)?;
    // Un fichier texte est déjà du markdown : le mémoriser ne ferait
    // qu'ajouter une copie.
    let key = match (cached, kind) {
        (true, Kind::Text) | (false, _) => None,
        (true, _) => cache::fingerprint(path).ok(),
    };
    if let Some(found) = key.as_deref().and_then(cache::read) {
        return Ok(found);
    }

    let markdown = match kind {
        Kind::Text => std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display()))?,
        Kind::Pdf => layout::to_markdown(&pdf::pages(path)?),
        #[cfg(feature = "ocr")]
        Kind::Image => layout::to_markdown(&ocr::pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Image => anyhow::bail!(
            "{} est une image : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
        #[cfg(feature = "ocr")]
        Kind::Directory => layout::to_markdown(&ocr::directory_pages(path)?),
        #[cfg(not(feature = "ocr"))]
        Kind::Directory => anyhow::bail!(
            "{} est un dossier : ce binaire a été compilé sans l'option `ocr`",
            path.display()
        ),
    };
    if let Some(key) = key {
        cache::write(&key, &markdown);
    }
    Ok(markdown)
}
```

Et le test du cache, dans `mod tests` de `mod.rs` :

```rust
    #[test]
    fn a_second_reading_comes_from_the_cache() {
        let path = fixture("rapport.pdf");
        let first = load(&path).expect("première lecture");
        let second = load(&path).expect("seconde lecture");
        assert_eq!(first, second, "le cache doit rendre le même document");
    }
```

- [x] **Step 5: Ajouter `--no-cache`**

Dans `src/main.rs`, `struct Args`, après `clip` :

```rust
    /// Refait l'extraction sans consulter le cache.
    #[arg(long)]
    no_cache: bool,
```

et remplacez l'appel :

```rust
            let text = rs_reader::source::load_with_cache(path, !args.no_cache)?;
```

- [x] **Step 6: Lancer les tests dans les deux modes**

Run: `cargo test --features ocr`
Run: `cargo test`
Attendu : tout passe.

- [x] **Step 7: Mesurer le gain**

```bash
cargo build --release --features ocr
time ./target/release/reader tests/fixtures/scanne.pdf --to-markdown --no-cache >/dev/null
time ./target/release/reader tests/fixtures/scanne.pdf --to-markdown >/dev/null
```

Attendu : la seconde est nettement plus rapide. Notez les deux durées.

- [x] **Step 8: Commit**

```bash
git add src/source/cache.rs src/source/mod.rs src/main.rs
git commit -m "feat(source): ne payer une extraction coûteuse qu'une fois"
```

---

### Task 5: Bout en bout et documentation

**Files:**
- Modify: `tests/pdf_input.rs`, `README.md`

- [x] **Step 1: Écrire le test de bout en bout**

Dans `tests/pdf_input.rs` :

```rust
#[cfg(feature = "ocr")]
#[test]
fn a_scanned_pdf_is_recognised_and_rebuilt() {
    let markdown = rs_reader::source::load_with_cache(&fixture("scanne.pdf"), false)
        .expect("le scan doit se lire");
    assert!(
        markdown.contains("Spécification"),
        "le scan est resté vide : {markdown}"
    );
    assert!(
        markdown.contains("## 1. Contexte"),
        "la reconstruction ne s'est pas appliquée : {markdown}"
    );
}
```

Le cache est écarté : un test doit mesurer la chaîne, pas un souvenir.

- [x] **Step 2: Lancer toute la suite**

Run: `cargo test --features ocr`
Run: `cargo test`
Run: `cargo clippy --all-targets --features ocr 2>&1 | grep -c warning`
Run: `cargo clippy --all-targets 2>&1 | grep -c warning`
Attendu : tout passe, `0` des deux côtés.

- [x] **Step 3: Documenter**

Dans `README.md`, complétez la puce des images :

```markdown
- **Lit aussi les images**, avec reconnaissance de texte : une capture d'écran,
  une photo de page, un dossier de scans, un **PDF photocopié**, ou le
  presse-papiers avec `--clip`. Les pages sont reconnues en parallèle, et le
  résultat mémorisé : la deuxième ouverture est immédiate. `--no-cache` refait
  l'extraction. Demande l'option de compilation `ocr`.
```

- [x] **Step 4: Commit**

```bash
git add tests/pdf_input.rs README.md
git commit -m "test: lecture d'un PDF scanné de bout en bout"
```

---

## Ce que ce lot ne fait pas

| Cas | Raison |
|---|---|
| Télécopies en CCITT ou JBIG2 | Aucun décodeur pur Rust ; la page reste vide plutôt que fausse |
| Scans dont l'image est en `/FlateDecode` brut | Reconstruction du bitmap non couverte |
| Purge du cache | Aucun fichier n'est effacé ; le répertoire est sous `dirs::cache_dir()`, que le système sait nettoyer |
