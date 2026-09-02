# Lot B — Reconstruction d'un PDF en markdown lisible

> **État au 1ᵉʳ septembre 2026**
> **Livré.** `unligature`, `join_hyphenated`, `strip_running_heads`,
> `weld_pages`, `heading_level` et `list_item` composent
> `layout::to_markdown`, vérifiés sur `tests/fixtures/rapport.pdf`. Les étapes
> sont cochées au vu du code livré, non au fil de l'exécution.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Qu'un PDF de spécification se lise comme un markdown écrit à la main :
titres dans le sommaire, paragraphes entiers, ni en-tête répété ni mot coupé.

**Architecture:** Tout tient dans `src/source/layout.rs`, en fonctions pures
enchaînées. Aucun autre fichier du lecteur ne change. `to_markdown` devient un
pipeline : retirer les lignes courantes, souder les pages, recoller les
césures, puis classer chaque ligne en titre, item ou paragraphe.

**Tech Stack:** Rust 2021, `regex` (déjà présent).

**Spec:** `docs/superpowers/specs/2026-08-30-lecture-pdf-et-images-design.md`,
section « La reconstruction ».

## Global Constraints

- **Aucun `cfg(target_os)`.** Cibles : macOS, Linux x86_64, Linux ARM64.
- **Tout est pur.** Chaque fonction de ce lot prend des données et en rend ;
  aucune n'ouvre de fichier. C'est ce qui rend le lot testable sans PDF.
- Commentaires et messages en français, noms de tests en anglais descriptif.
- Un commentaire dit **pourquoi**, jamais quoi.
- Pas de `unwrap()` ni `expect()` hors tests.
- Après chaque tâche : `cargo test` et `cargo clippy --all-targets` propres.
- Le lot A a laissé 255 tests de bibliothèque. Ce nombre ne doit que croître.

## Ce que le lot doit corriger

Mesuré sur `tests/fixtures/rapport.pdf`, produit par la tâche 1 :

| Défaut aujourd'hui | Attendu après |
|---|---|
| `Spécification technique — Confidentiel` répété 3 fois | supprimé |
| `Page 1 sur 3`, `Page 2 sur 3`, `Page 3 sur 3` | supprimés |
| `diffé-` / `rés sur trente jours` | `différés sur trente jours` |
| `1. Contexte` lu comme puce de liste | `## 1. Contexte` |
| `2.1 Latence` noyé dans le texte | `### 2.1 Latence` |
| `• latence maximale` | `- latence maximale` |
| `aux opérations de` ⁄ saut de page ⁄ `remboursement,` | phrase entière |

---

### Task 1: La fixture réaliste

**Files:**
- Create: `tests/fixtures/make_rapport_pdf.py`
- Create: `tests/fixtures/rapport.pdf`

**Interfaces:**
- Consumes: rien
- Produces: `tests/fixtures/rapport.pdf`, trois pages portant les sept défauts
  du tableau ci-dessus

- [x] **Step 1: Écrire le générateur**

Créez `tests/fixtures/make_rapport_pdf.py` :

```python
#!/usr/bin/env python3
"""Écrit tests/fixtures/rapport.pdf : trois pages portant chaque défaut que le
lot B doit corriger — en-tête et pied courants, césures, titres numérotés,
liste à puces, phrase coupée par un saut de page.
"""
import pathlib


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def stream(lines):
    out = ["BT", "/F1 11 Tf", "60 740 Td", "13 TL"]
    for i, line in enumerate(lines):
        if i:
            out.append("T*")
        out.append("(%s) Tj" % esc(line))
    out.append("ET")
    return "\n".join(out).encode("cp1252")


PAGES = [
    ["Spécification technique — Confidentiel", "",
     "1. Contexte", "",
     "Le service de paiement doit gérer les règlements diffé-",
     "rés sur trente jours. Chaque opération est enregistrée",
     "à l'état « en attente », puis confirmée par le presta-",
     "taire externe.", "",
     "Page 1 sur 3"],
    ["Spécification technique — Confidentiel", "",
     "2. Contraintes", "",
     "Les contraintes retenues sont les suivantes :", "",
     "• latence maximale de 250 ms",
     "• disponibilité de 99,9 %",
     "• journalisation intégrale", "",
     "2.1 Latence", "",
     "La mesure est prise au percentile 95. Le seuil ne",
     "s'applique pas aux opérations de",
     "Page 2 sur 3"],
    ["Spécification technique — Confidentiel", "",
     "remboursement, traitées en différé.", "",
     "3. Interfaces", "",
     "L'API expose deux routes documentées.", "",
     "Page 3 sur 3"],
]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
       b"/Encoding /WinAnsiEncoding >>",
}
kids = " ".join("%d 0 R" % (10 + i * 2) for i in range(len(PAGES)))
objs[2] = ("<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(PAGES))).encode()
for i, page in enumerate(PAGES):
    pid, cid = 10 + i * 2, 11 + i * 2
    objs[pid] = (
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
        "/Resources << /Font << /F1 5 0 R >> >> >>" % cid
    ).encode()
    body = stream(page)
    objs[cid] = b"<< /Length %d >>\nstream\n" % len(body) + body + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"

xref = len(buf)
buf += b"xref\n0 %d\n" % (max(objs) + 1)
buf += b"0000000000 65535 f \n"
for num in range(1, max(objs) + 1):
    buf += b"%010d 00000 n \n" % offsets.get(num, 0)
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    max(objs) + 1,
    xref,
)

out = pathlib.Path(__file__).with_name("rapport.pdf")
out.write_bytes(bytes(buf))
print("écrit %s : %d octets" % (out, len(buf)))
```

- [x] **Step 2: Produire la fixture et constater l'état de départ**

```bash
python3 tests/fixtures/make_rapport_pdf.py
cargo run -q -- tests/fixtures/rapport.pdf --to-markdown
```

Attendu : `écrit …/rapport.pdf : 2196 octets`, puis une sortie qui contient
encore `Page 1 sur 3`, `diffé-` et `1. Contexte` sans dièse. C'est l'état que
les tâches suivantes corrigent.

- [x] **Step 3: Commit**

```bash
git add tests/fixtures/make_rapport_pdf.py tests/fixtures/rapport.pdf
git commit -m "test(source): fixture PDF portant les défauts de mise en page"
```

---

### Task 2: Ligatures et traits d'union conditionnels

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: rien
- Produces: `fn unligature(text: &str) -> String` (privée au module)

Un PDF porte les ligatures typographiques comme un seul caractère. `ﬁ` n'est
pas `fi` pour un moteur de synthèse, et le trait d'union conditionnel `\u{ad}`
est invisible à l'écran mais bien présent dans le texte.

- [x] **Step 1: Écrire le test qui échoue**

Ajoutez dans `src/source/layout.rs`, avant `#[cfg(test)]` :

```rust
/// Rend leurs lettres aux ligatures typographiques.
fn unligature(_text: &str) -> String {
    todo!("ligatures")
}
```

et dans `mod tests` :

```rust
    #[test]
    fn ligatures_become_plain_letters() {
        assert_eq!(unligature("ﬁchier de conﬁguration"), "fichier de configuration");
        assert_eq!(unligature("aﬄux et eﬀort"), "afflux et effort");
    }

    #[test]
    fn a_soft_hyphen_leaves_no_trace() {
        assert_eq!(unligature("exem\u{ad}ple"), "exemple");
    }

    #[test]
    fn ordinary_text_passes_through_untouched() {
        assert_eq!(unligature("Spécification — « en attente »"), "Spécification — « en attente »");
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC sur les trois nouveaux tests, `not yet implemented`.

- [x] **Step 3: Écrire l'implémentation**

```rust
/// Rend leurs lettres aux ligatures typographiques.
///
/// Un moteur de synthèse ne reconnaît pas `ﬁ` comme `fi`, et le trait d'union
/// conditionnel se glisse dans le texte sans se voir à l'écran.
fn unligature(text: &str) -> String {
    // La très grande majorité des lignes n'a aucune ligature : ne rien
    // reconstruire quand il n'y a rien à changer.
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬀ' => out.push_str("ff"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' | 'ﬆ' => out.push_str("st"),
            '\u{ad}' => {}
            other => out.push(other),
        }
    }
    out
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 10 tests passent (7 du lot A + 3).

- [x] **Step 5: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): rendre leurs lettres aux ligatures"
```

---

### Task 3: Recoller les mots coupés en fin de ligne

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: rien
- Produces: `fn join_hyphenated(lines: Vec<String>) -> Vec<String>`

C'est la règle qui a le plus d'effet sur la voix : sans elle, « diffé- / rés »
se prononce en deux morceaux.

Un mot composé coupé au trait d'union doit garder le sien. La marque retenue :
si le premier mot de la ligne suivante porte lui-même un trait d'union
(`en-ciel`), c'est un mot composé, et le trait est conservé.

- [x] **Step 1: Écrire le test qui échoue**

```rust
/// Recolle les mots que la mise en page a coupés en fin de ligne.
fn join_hyphenated(_lines: Vec<String>) -> Vec<String> {
    todo!("césures")
}
```

```rust
    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_word_split_across_lines_is_made_whole() {
        assert_eq!(
            join_hyphenated(lines(&["les règlements diffé-", "rés sur trente jours"])),
            lines(&["les règlements différés sur trente jours"])
        );
    }

    #[test]
    fn a_compound_word_keeps_its_hyphen() {
        assert_eq!(
            join_hyphenated(lines(&["il voit un arc-", "en-ciel superbe"])),
            lines(&["il voit un arc-en-ciel superbe"])
        );
    }

    #[test]
    fn a_dash_before_a_capital_is_left_alone() {
        // Un tiret suivi d'une majuscule termine une énumération, il ne coupe
        // pas un mot.
        assert_eq!(
            join_hyphenated(lines(&["voir annexe -", "Tableau des seuils"])),
            lines(&["voir annexe -", "Tableau des seuils"])
        );
    }

    #[test]
    fn a_blank_line_stops_the_join() {
        assert_eq!(
            join_hyphenated(lines(&["fin de section diffé-", "", "rés"])),
            lines(&["fin de section diffé-", "", "rés"])
        );
    }

    #[test]
    fn several_hyphenations_in_a_row_all_close() {
        assert_eq!(
            join_hyphenated(lines(&["pre-", "mier deu-", "xième"])),
            lines(&["premier deuxième"])
        );
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC sur les cinq nouveaux tests.

- [x] **Step 3: Écrire l'implémentation**

```rust
/// Recolle les mots que la mise en page a coupés en fin de ligne.
///
/// Un mot composé garde son trait : on le reconnaît à ce que la suite en porte
/// un elle-même — `arc-` suivi de `en-ciel`.
fn join_hyphenated(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(previous) = out.last_mut() else {
            out.push(line);
            continue;
        };
        let cut = previous
            .strip_suffix('-')
            .filter(|head| head.chars().last().is_some_and(|c| c.is_alphabetic()));
        let follows = line
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() && c.is_lowercase());
        let Some(head) = cut.filter(|_| follows) else {
            out.push(line);
            continue;
        };
        // `head` emprunte `previous`, qu'on va réécrire : il faut le détacher.
        let head = head.to_string();
        let compound = line
            .split_whitespace()
            .next()
            .is_some_and(|word| word.contains('-'));
        *previous = if compound {
            format!("{head}-{line}")
        } else {
            format!("{head}{line}")
        };
    }
    out
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 15 tests passent.

- [x] **Step 5: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): recoller les mots coupés en fin de ligne"
```

---

### Task 4: Retirer les en-têtes et pieds courants

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: `super::pdf::Page`
- Produces: `fn strip_running_heads(pages: &[Page]) -> Vec<Vec<String>>`

Sans cette règle, « Confidentiel — page 12 » revient toutes les trente
secondes. C'est ce qui use le plus à l'écoute.

Un numéro de page change d'une page à l'autre : les lignes sont donc comparées
après avoir réduit toute suite de chiffres à un seul jeton.

- [x] **Step 1: Écrire le test qui échoue**

```rust
/// Retire les lignes que chaque page répète en tête ou en pied.
fn strip_running_heads(_pages: &[Page]) -> Vec<Vec<String>> {
    todo!("lignes courantes")
}
```

```rust
    #[test]
    fn a_header_repeated_on_every_page_disappears() {
        let doc = [
            page(&["Confidentiel", "", "Un"]),
            page(&["Confidentiel", "", "Deux"]),
            page(&["Confidentiel", "", "Trois"]),
        ];
        let kept = strip_running_heads(&doc);
        assert!(
            !kept.iter().flatten().any(|l| l == "Confidentiel"),
            "l'en-tête courant subsiste : {kept:?}"
        );
        assert!(kept.iter().flatten().any(|l| l == "Deux"), "le texte a été emporté");
    }

    #[test]
    fn a_page_number_that_changes_is_still_recognised() {
        let doc = [
            page(&["Un", "", "Page 1 sur 3"]),
            page(&["Deux", "", "Page 2 sur 3"]),
            page(&["Trois", "", "Page 3 sur 3"]),
        ];
        let kept = strip_running_heads(&doc);
        assert!(
            !kept.iter().flatten().any(|l| l.starts_with("Page ")),
            "le pied courant subsiste : {kept:?}"
        );
    }

    #[test]
    fn a_line_that_only_looks_repeated_is_kept() {
        // Deux pages sur cinq ne font pas une ligne courante.
        let doc = [
            page(&["Confidentiel", "", "Un"]),
            page(&["Confidentiel", "", "Deux"]),
            page(&["Trois"]),
            page(&["Quatre"]),
            page(&["Cinq"]),
        ];
        let kept = strip_running_heads(&doc);
        assert_eq!(
            kept.iter().flatten().filter(|l| *l == "Confidentiel").count(),
            2,
            "une ligne vue deux fois n'est pas courante"
        );
    }

    #[test]
    fn a_short_document_keeps_everything() {
        // Sur deux pages, rien ne prouve qu'une ligne est courante.
        let doc = [page(&["Confidentiel", "Un"]), page(&["Confidentiel", "Deux"])];
        let kept = strip_running_heads(&doc);
        assert_eq!(
            kept.iter().flatten().filter(|l| *l == "Confidentiel").count(),
            2
        );
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC sur les quatre nouveaux tests.

- [x] **Step 3: Écrire l'implémentation**

Ajoutez l'import en tête de fichier :

```rust
use std::collections::HashMap;
```

```rust
/// Nombre de pages au-delà duquel une ligne répétée est tenue pour courante.
///
/// Deux occurrences peuvent être une coïncidence ; trois font une habitude.
const RUNNING_THRESHOLD: usize = 3;

/// Réduit une ligne à sa forme comparable d'une page à l'autre.
///
/// Le numéro change à chaque page : il ne doit pas empêcher de reconnaître
/// « Page 1 sur 40 » et « Page 2 sur 40 » comme le même pied.
fn normalise_running(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_digits = false;
    for c in line.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
                in_digits = true;
            }
        } else {
            in_digits = false;
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Retire les lignes que chaque page répète en tête ou en pied.
fn strip_running_heads(pages: &[Page]) -> Vec<Vec<String>> {
    let mut kept: Vec<Vec<String>> = pages.iter().map(|p| p.lines.clone()).collect();
    if kept.len() < RUNNING_THRESHOLD {
        return kept;
    }

    let mut heads: HashMap<String, usize> = HashMap::new();
    let mut feet: HashMap<String, usize> = HashMap::new();
    for lines in &kept {
        if let Some(line) = lines.iter().find(|l| !l.trim().is_empty()) {
            *heads.entry(normalise_running(line)).or_default() += 1;
        }
        if let Some(line) = lines.iter().rev().find(|l| !l.trim().is_empty()) {
            *feet.entry(normalise_running(line)).or_default() += 1;
        }
    }
    let running = |seen: &HashMap<String, usize>, line: &str| {
        seen.get(&normalise_running(line))
            .is_some_and(|&n| n >= RUNNING_THRESHOLD)
    };

    for lines in &mut kept {
        // Le pied d'abord : le retirer ne déplace pas l'en-tête.
        if let Some(i) = lines.iter().rposition(|l| !l.trim().is_empty()) {
            if running(&feet, &lines[i]) {
                lines.remove(i);
            }
        }
        if let Some(i) = lines.iter().position(|l| !l.trim().is_empty()) {
            if running(&heads, &lines[i]) {
                lines.remove(i);
            }
        }
    }
    kept
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 19 tests passent.

- [x] **Step 5: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): retirer les en-têtes et pieds courants"
```

---

### Task 5: Souder les pages

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: la sortie de `strip_running_heads`
- Produces: `fn weld_pages(pages: Vec<Vec<String>>) -> Vec<String>`

Une phrase coupée par un saut de page doit se refermer. Sinon la voix marque un
arrêt au milieu, et le sens se perd.

- [x] **Step 1: Écrire le test qui échoue**

```rust
/// Met les pages bout à bout, en refermant les phrases coupées.
fn weld_pages(_pages: Vec<Vec<String>>) -> Vec<String> {
    todo!("soudure des pages")
}
```

```rust
    fn raw(pages: &[&[&str]]) -> Vec<Vec<String>> {
        pages
            .iter()
            .map(|p| p.iter().map(|l| l.to_string()).collect())
            .collect()
    }

    #[test]
    fn a_sentence_cut_by_a_page_break_closes_again() {
        let out = weld_pages(raw(&[&["aux opérations de"], &["remboursement, en différé."]]));
        assert_eq!(out, lines(&["aux opérations de remboursement, en différé."]));
    }

    #[test]
    fn a_finished_sentence_starts_a_new_line() {
        let out = weld_pages(raw(&[&["Fin de la partie."], &["Nouvelle partie."]]));
        assert_eq!(out, lines(&["Fin de la partie.", "", "Nouvelle partie."]));
    }

    #[test]
    fn a_capital_after_the_break_is_never_welded() {
        // Une majuscule ouvre une phrase : rien n'a été coupé.
        let out = weld_pages(raw(&[&["une liste des seuils"], &["Annexe technique"]]));
        assert_eq!(out, lines(&["une liste des seuils", "", "Annexe technique"]));
    }

    #[test]
    fn a_single_page_comes_back_as_it_is() {
        assert_eq!(weld_pages(raw(&[&["une", "", "deux"]])), lines(&["une", "", "deux"]));
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC sur les quatre nouveaux tests.

- [x] **Step 3: Écrire l'implémentation**

```rust
/// Ponctuations qui referment une phrase.
const CLOSING: [char; 5] = ['.', '!', '?', ':', ';'];

/// Met les pages bout à bout, en refermant les phrases coupées.
fn weld_pages(pages: Vec<Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for lines in pages {
        let opening = lines.iter().position(|l| !l.trim().is_empty());
        let welds = match (out.iter().rposition(|l| !l.trim().is_empty()), opening) {
            (Some(last), Some(first)) => {
                let tail = out[last].trim_end();
                let head = lines[first].trim_start();
                !tail.ends_with(CLOSING)
                    && head.chars().next().is_some_and(|c| c.is_lowercase())
            }
            _ => false,
        };
        if welds {
            let last = out
                .iter()
                .rposition(|l| !l.trim().is_empty())
                .expect("la branche ci-dessus l'a trouvé");
            let first = opening.expect("la branche ci-dessus l'a trouvé");
            // La fusion est calculée avant d'écrire : `out[last]` ne peut pas
            // être lu et assigné dans la même expression.
            let merged = format!("{} {}", out[last].trim_end(), lines[first].trim_start());
            out.truncate(last + 1);
            out[last] = merged;
            out.extend(lines.into_iter().skip(first + 1));
        } else {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend(lines);
        }
    }
    out
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 23 tests passent.

- [x] **Step 5: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): refermer les phrases coupées par un saut de page"
```

---

### Task 6: Reconnaître titres et listes

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: rien
- Produces:
  - `fn heading_level(line: &str) -> Option<u8>`
  - `fn list_item(line: &str) -> Option<&str>`

C'est la règle qui rend le sommaire. Le niveau suit la profondeur de la
numérotation : `1.` ouvre un niveau 2, `2.1` un niveau 3 — le niveau 1 reste au
titre du document.

Le motif de numérotation est celui que `text::numerals` emploie déjà pour
décider qu'un titre porte un rang : un nombre suivi d'un point ou d'une
parenthèse, ou une numérotation à plusieurs composantes. « 250 ms de latence »
n'en est donc pas un.

- [x] **Step 1: Écrire le test qui échoue**

```rust
/// Niveau de titre d'une ligne isolée, s'il y en a un.
fn heading_level(_line: &str) -> Option<u8> {
    todo!("titres")
}

/// Contenu d'un item de liste, si la ligne en est un.
fn list_item(_line: &str) -> Option<&str> {
    todo!("listes")
}
```

```rust
    #[test]
    fn a_numbered_line_is_a_heading_of_its_depth() {
        assert_eq!(heading_level("1. Contexte"), Some(2));
        assert_eq!(heading_level("2) Contraintes"), Some(2));
        assert_eq!(heading_level("2.1 Latence"), Some(3));
        assert_eq!(heading_level("4.10.2 Repli"), Some(4));
    }

    #[test]
    fn a_line_in_capitals_is_a_heading() {
        assert_eq!(heading_level("ANNEXE TECHNIQUE"), Some(2));
    }

    #[test]
    fn ordinary_prose_is_not_a_heading() {
        assert_eq!(heading_level("250 ms de latence maximale"), None);
        assert_eq!(heading_level("Le service expose deux routes."), None);
        assert_eq!(heading_level(""), None);
    }

    #[test]
    fn a_long_line_is_never_a_heading() {
        let long = format!("1. {}", "mot ".repeat(40));
        assert_eq!(heading_level(&long), None, "un titre ne fait pas un paragraphe");
    }

    #[test]
    fn a_sentence_that_ends_in_a_full_stop_is_not_a_heading() {
        assert_eq!(heading_level("1. Le service traite les paiements."), None);
    }

    #[test]
    fn the_usual_bullets_are_recognised() {
        assert_eq!(list_item("• latence maximale"), Some("latence maximale"));
        assert_eq!(list_item("– disponibilité"), Some("disponibilité"));
        assert_eq!(list_item("- journalisation"), Some("journalisation"));
        assert_eq!(list_item("* trace"), Some("trace"));
    }

    #[test]
    fn a_bullet_needs_a_space_after_it() {
        assert_eq!(list_item("-sans espace"), None);
        assert_eq!(list_item("Une phrase - avec tiret"), None);
    }
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC sur les sept nouveaux tests.

- [x] **Step 3: Écrire l'implémentation**

Ajoutez les imports en tête de fichier :

```rust
use regex::Regex;
use std::sync::LazyLock;
```

```rust
/// Longueur au-delà de laquelle une ligne est un paragraphe, pas un titre.
const HEADING_MAX: usize = 80;

/// Niveau de titre d'une ligne isolée, s'il y en a un.
///
/// Le niveau 1 est laissé au titre du document : une numérotation de premier
/// rang ouvre donc un niveau 2.
fn heading_level(line: &str) -> Option<u8> {
    // Même motif que `text::numerals` : un rang suivi de sa ponctuation, ou
    // une numérotation à plusieurs composantes. « 250 ms » n'en est pas un.
    static NUMBERED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:(?P<deep>\d+(?:\.\d+)+)\.?|(?P<flat>\d+)[.)])[ \t]+\S")
            .expect("motif constant")
    });

    let line = line.trim();
    if line.is_empty() || line.chars().count() > HEADING_MAX {
        return None;
    }
    // Un titre n'est pas une phrase : il ne se referme pas.
    if line.ends_with(['.', ',', ';']) {
        return None;
    }
    if let Some(caps) = NUMBERED.captures(line) {
        let number = caps.name("deep").or_else(|| caps.name("flat"))?;
        let depth = number.as_str().split('.').count() as u8;
        return Some((depth + 1).min(6));
    }
    let letters = line.chars().filter(|c| c.is_alphabetic());
    let mut count = 0;
    for c in letters {
        if c.is_lowercase() {
            return None;
        }
        count += 1;
    }
    (count >= 3).then_some(2)
}

/// Contenu d'un item de liste, si la ligne en est un.
fn list_item(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = ['•', '●', '▪', '◦', '·', '–', '—', '*', '-']
        .iter()
        .find_map(|bullet| line.strip_prefix(*bullet))?;
    // Sans espace, le tiret appartient au mot qui suit.
    rest.starts_with([' ', '\t'])
        .then(|| rest.trim())
        .filter(|rest| !rest.is_empty())
}
```

- [x] **Step 4: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 30 tests passent.

- [x] **Step 5: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): reconnaître les titres numérotés et les listes"
```

---

### Task 7: Assembler le pipeline

**Files:**
- Modify: `src/source/layout.rs`

**Interfaces:**
- Consumes: toutes les fonctions des tâches 2 à 6
- Produces: `pub fn to_markdown(pages: &[Page]) -> String`, signature inchangée

**Quatre tests du lot A décrivent un comportement que ce lot change
délibérément.** Ils ne constatent pas une régression : ils décrivent l'absence
de reconstruction. L'étape 3 les remplace, et cette substitution est le cœur de
la tâche — ne la traitez pas comme un contournement.

| Test du lot A | Pourquoi il change |
|---|---|
| `a_single_page_keeps_its_lines_in_order` | `1. Contexte` devient `## 1. Contexte` |
| `right_padding_disappears` | les deux lignes se joignent désormais |
| `two_pages_are_separated_by_a_blank_line` | la phrase ouverte est soudée |
| `a_page_break_never_doubles_an_existing_blank` | même raison |

Les trois autres — `runs_of_blank_lines_collapse_into_one`,
`nothing_trails_at_the_end`, `an_empty_document_yields_nothing` — doivent
passer **sans être touchés**. S'ils échouent, c'est une vraie régression.

- [x] **Step 1: Écrire les tests qui échouent**

Ajoutez dans `mod tests` :

```rust
    #[test]
    fn a_numbered_title_becomes_a_heading() {
        let out = to_markdown(&[page(&["1. Contexte", "", "Du texte."])]);
        assert_eq!(out, "## 1. Contexte\n\nDu texte.");
    }

    #[test]
    fn a_sub_numbered_title_goes_one_level_deeper() {
        let out = to_markdown(&[page(&["2.1 Latence", "", "Du texte."])]);
        assert_eq!(out, "### 2.1 Latence\n\nDu texte.");
    }

    #[test]
    fn bullets_become_a_markdown_list_without_gaps() {
        let out = to_markdown(&[page(&["• un", "• deux"])]);
        assert_eq!(out, "- un\n- deux", "les items d'une liste se suivent");
    }

    #[test]
    fn the_lines_of_a_paragraph_are_joined() {
        let out = to_markdown(&[page(&[
            "Le service doit gérer les règlements",
            "sur trente jours.",
        ])]);
        assert_eq!(out, "Le service doit gérer les règlements sur trente jours.");
    }

    #[test]
    fn a_heading_is_only_a_heading_when_it_stands_alone() {
        // Au milieu d'un paragraphe, « 1. » ouvre une énumération, pas une
        // section.
        let out = to_markdown(&[page(&["Les étapes sont :", "1. Ouvrir", "2. Fermer"])]);
        assert!(!out.contains('#'), "un titre a été inventé : {out}");
    }

    #[test]
    fn a_short_closing_line_ends_its_paragraph() {
        // Sans ligne vide, un PDF dense ferait un seul bloc géant : une ligne
        // nettement plus courte que les autres et refermée marque la fin.
        let out = to_markdown(&[page(&[
            "Le service de paiement traite les règlements différés",
            "et les remboursements sur trente jours au plus tard.",
            "La règle s'arrête là.",
            "Une autre partie commence ici et court sur toute la ligne",
        ])]);
        assert!(
            out.contains("au plus tard. La règle s'arrête là.\n\nUne autre partie"),
            "le paragraphe n'a pas été fermé : {out}"
        );
    }
```

- [x] **Step 2: Lancer les tests et les voir échouer**

Run: `cargo test --lib source::layout`
Attendu : ÉCHEC des six nouveaux tests — `to_markdown` rend encore les lignes
telles quelles.

- [x] **Step 3: Remplacer les quatre tests que le lot rend faux**

Dans `mod tests`, remplacez ces quatre tests par les versions ci-dessous. Elles
vérifient la même intention — ordre conservé, padding retiré, pages séparées,
pas de blanc doublé — sur des données que la reconstruction ne transforme pas.

```rust
    #[test]
    fn a_single_page_keeps_its_content_in_order() {
        let out = to_markdown(&[page(&[
            "Contexte du service",
            "",
            "Le service expose deux routes.",
        ])]);
        assert_eq!(out, "Contexte du service\n\nLe service expose deux routes.");
    }

    #[test]
    fn right_padding_disappears() {
        let out = to_markdown(&[page(&["Titre    ", "", "Suite\t"])]);
        assert_eq!(out, "Titre\n\nSuite");
    }

    #[test]
    fn two_pages_are_separated_when_the_sentence_is_closed() {
        let out = to_markdown(&[page(&["Fin de page."]), page(&["Début de page."])]);
        assert_eq!(out, "Fin de page.\n\nDébut de page.");
    }

    #[test]
    fn a_page_break_never_doubles_an_existing_blank() {
        let out = to_markdown(&[page(&["Fin.", ""]), page(&["Suite."])]);
        assert_eq!(out, "Fin.\n\nSuite.");
    }
```

- [x] **Step 4: Réécrire `to_markdown`**

Remplacez la fonction du lot A par le pipeline, et ajoutez ses trois aides :

```rust
/// Assemble des pages en un markdown que `input::split_blocks` sait découper.
///
/// L'ordre compte : les lignes courantes partent avant que les pages ne soient
/// soudées, et les césures se referment avant qu'on ne juge de la longueur des
/// lignes.
pub fn to_markdown(pages: &[Page]) -> String {
    let lines = join_hyphenated(weld_pages(strip_running_heads(pages)));
    let lines: Vec<String> = lines.iter().map(|l| unligature(l.trim())).collect();
    let short = short_line_width(&lines);

    let mut out: Vec<String> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut in_list = false;

    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            flush(&mut para, &mut out);
            in_list = false;
            continue;
        }
        // Un titre se tient seul : entouré de blanc, ou en bord de document.
        let alone = lines[..i].last().map_or(true, |l| l.is_empty())
            && lines.get(i + 1).map_or(true, |l| l.is_empty());
        if alone {
            if let Some(level) = heading_level(line) {
                flush(&mut para, &mut out);
                separate(&mut out);
                out.push(format!("{} {line}", "#".repeat(level as usize)));
                in_list = false;
                continue;
            }
        }
        if let Some(rest) = list_item(line) {
            flush(&mut para, &mut out);
            // Une liste s'ouvre après un blanc, mais ses items se suivent.
            if !in_list {
                separate(&mut out);
            }
            out.push(format!("- {rest}"));
            in_list = true;
            continue;
        }
        in_list = false;
        para.push(line.clone());
        // Sans ligne vide, une ligne nettement plus courte que les autres et
        // refermée par une ponctuation marque la fin du paragraphe.
        if line.chars().count() < short && line.ends_with(CLOSING) {
            flush(&mut para, &mut out);
        }
    }
    flush(&mut para, &mut out);
    out.join("\n")
}

/// Largeur en deçà de laquelle une ligne est tenue pour une fin de paragraphe.
///
/// La médiane sert de référence : elle résiste aux titres courts comme aux
/// lignes exceptionnellement longues.
fn short_line_width(lines: &[String]) -> usize {
    let mut widths: Vec<usize> = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.chars().count())
        .collect();
    if widths.is_empty() {
        return 0;
    }
    widths.sort_unstable();
    widths[widths.len() / 2] * 3 / 5
}

/// Sépare d'une ligne vide ce qui va suivre.
fn separate(out: &mut Vec<String>) {
    if out.last().is_some_and(|l| !l.is_empty()) {
        out.push(String::new());
    }
}

/// Ferme le paragraphe en cours.
fn flush(para: &mut Vec<String>, out: &mut Vec<String>) {
    if para.is_empty() {
        return;
    }
    separate(out);
    out.push(para.join(" "));
    para.clear();
}
```

**`map_or(true, …)` et non `is_none_or`** : `Option::is_none_or` n'est stable
que depuis Rust 1.82, et `Cargo.toml` fixe `rust-version = "1.80"`.

- [x] **Step 5: Lancer les tests et les voir passer**

Run: `cargo test --lib source::layout`
Attendu : 36 tests passent. Les trois tests du lot A laissés intacts —
`runs_of_blank_lines_collapse_into_one`, `nothing_trails_at_the_end`,
`an_empty_document_yields_nothing` — en font partie. **S'ils échouent, c'est
une régression** : corrigez `to_markdown`, pas le test.

- [x] **Step 6: Vérifier que rien n'a régressé ailleurs**

Run: `cargo test`
Attendu : toute la suite passe.

Run: `cargo clippy --all-targets 2>&1 | grep -c warning`
Attendu : `0`

- [x] **Step 7: Commit**

```bash
git add src/source/layout.rs
git commit -m "feat(layout): assembler la reconstruction complète"
```

---

### Task 8: Vérifier sur le PDF réaliste

**Files:**
- Modify: `tests/pdf_input.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `source::load`
- Produces: la preuve que les sept défauts du tableau initial ont disparu

- [x] **Step 1: Écrire le test qui échoue**

Ajoutez dans `tests/pdf_input.rs` :

```rust
#[test]
fn a_laid_out_pdf_is_rebuilt_into_readable_markdown() {
    let markdown = sillon::source::load(&fixture("rapport.pdf")).expect("le PDF doit se lire");

    assert!(
        !markdown.contains("Confidentiel"),
        "l'en-tête courant subsiste : {markdown}"
    );
    assert!(
        !markdown.contains("Page 1 sur"),
        "le pied courant subsiste : {markdown}"
    );
    assert!(
        markdown.contains("règlements différés sur trente jours"),
        "la césure n'a pas été recollée : {markdown}"
    );
    assert!(
        markdown.contains("par le prestataire externe"),
        "la seconde césure n'a pas été recollée : {markdown}"
    );
    assert!(
        markdown.contains("## 1. Contexte"),
        "le titre n'est pas devenu un titre : {markdown}"
    );
    assert!(
        markdown.contains("### 2.1 Latence"),
        "le sous-titre n'a pas son niveau : {markdown}"
    );
    assert!(
        markdown.contains("- latence maximale de 250 ms"),
        "la puce n'est pas devenue une liste : {markdown}"
    );
    assert!(
        markdown.contains("aux opérations de remboursement"),
        "la phrase coupée par le saut de page n'a pas été refermée : {markdown}"
    );
}

#[test]
fn the_rebuilt_pdf_fills_the_table_of_contents() {
    use sillon::doc::builder::{parse_document, CodePolicy};

    let markdown = sillon::source::load(&fixture("rapport.pdf")).expect("le PDF doit se lire");
    let doc = parse_document(&markdown, CodePolicy::Announce);
    assert!(
        doc.sections.len() >= 4,
        "le sommaire doit porter les quatre titres, obtenu {} : {:?}",
        doc.sections.len(),
        doc.sections.iter().map(|s| &s.title).collect::<Vec<_>>()
    );
}
```

- [x] **Step 2: Lancer le test et le voir échouer**

Run: `cargo test --test pdf_input`
Attendu : si les tâches 1 à 7 sont faites, ces deux tests passent. **S'ils
échouent**, lisez le markdown que le message d'échec affiche et corrigez la
règle en cause dans `layout.rs` — jamais le test.

- [x] **Step 3: Regarder le résultat**

```bash
cargo run -q -- tests/fixtures/rapport.pdf --to-markdown
```

Attendu, exactement :

```markdown
## 1. Contexte

Le service de paiement doit gérer les règlements différés sur trente jours. Chaque opération est enregistrée à l'état « en attente », puis confirmée par le prestataire externe.

## 2. Contraintes

Les contraintes retenues sont les suivantes :

- latence maximale de 250 ms
- disponibilité de 99,9 %
- journalisation intégrale

### 2.1 Latence

La mesure est prise au percentile 95. Le seuil ne s'applique pas aux opérations de remboursement, traitées en différé.

## 3. Interfaces

L'API expose deux routes documentées.
```

- [x] **Step 4: Documenter**

Dans `README.md`, remplacez la puce du lot A par :

```markdown
- **Accepte un PDF** qui porte une couche texte, et le reconstruit : titres
  rendus au sommaire, mots coupés recollés, en-têtes et pieds de page retirés.
  `--to-markdown` écrit le texte extrait sans lancer la lecture.
```

- [x] **Step 5: Commit**

```bash
git add tests/pdf_input.rs README.md
git commit -m "test: vérifier la reconstruction sur un PDF mis en page"
```

---

## Ce que ce lot ne fait pas

| | Lot |
|---|---|
| OCR, images, dossiers, presse-papiers | C |
| PDF scannés, flux page par page, cache | D |
| Titre affiché « un, Contexte » au lieu de « 1. Contexte » | chantier distinct |

Le dernier point mérite une note : `doc/builder.rs` remplit `Section.title`
avec la version **parlée** du titre. Le sommaire d'un PDF reconstruit affichera
donc « un, Contexte ». Le défaut existe déjà pour les fichiers markdown ; ce
lot le rend seulement plus visible, puisqu'il crée enfin des titres. Le
corriger touche `doc/builder.rs` et non `source/`, et profite aussi bien au
markdown qu'au PDF : c'est un chantier à part.
