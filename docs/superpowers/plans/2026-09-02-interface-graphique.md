# Interface graphique — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Donner à sillon une fenêtre — un second rendu du même contrôleur, par egui/eframe — avec police lisible embarquée, interlignage réel, mot prononcé stable, clic sur un mot, fichier déposé, et la configuration complète en widgets.

**Architecture:** Le contrôleur `App` quitte `tui::` pour `app::`, reçoit des touches neutres, et apprend quelques gestes (`open_document`, `search_with`, réglages posés par valeur). `launch` prépare un `App` à partir des arguments pour les deux binaires. `gui::` rend cet `App` dans une fenêtre eframe, bloc par bloc, avec une composition mise en cache.

**Tech Stack:** Rust 2021, eframe/egui 0.36 (wgpu), rfd 0.17 (portail XDG sur Linux, Cocoa sur macOS), polices Atkinson Hyperlegible et OpenDyslexic (OFL), ratatui/crossterm inchangés pour le terminal.

**Spec:** `docs/superpowers/specs/2026-09-02-interface-graphique-design.md`

## Global Constraints

- Cibles : macOS (Intel, Apple Silicon), Linux x86_64, Linux ARM64. Windows écarté. Aucun `cfg(target_os)` nouveau hors de la couche d'intégration (dialogue de fichiers, bundle).
- Aucune dépendance système à installer pour compiler ou lancer la fenêtre.
- La feature `gui` est optionnelle : sans elle, `cargo build` et `cargo test` ne compilent ni eframe ni rfd.
- Les modules `gui::text` et `gui::flow` ne dépendent d'aucun type egui et se compilent sans la feature.
- Le dépôt n'est **pas** formaté par rustfmt : ne jamais lancer `cargo fmt` sur l'ensemble. Formater à la main les fichiers nouveaux dans le style des voisins (quatre espaces, lignes ≤ 100 colonnes).
- Commentaires, docs, messages de commit en français ; noms de tests en anglais descriptif (`a_pinned_view_stays_where_it_is`), comme dans le dépôt.
- Palette : accent `#F26B27` ; gris étagés `BRIGHT #EEEEEE`, `NORMAL #BABABA`, `DIM #787878`, `FAINT #525252`, `TECHNICAL #7EA8C4`, `RULER #26262C`.
- Défauts `[gui]` : `font = "atkinson"`, `font_size = 22.0`, `line_height = 1.6`, `theme = "dark"`.
- Le mot prononcé et la phrase en cours ne changent **jamais** de graisse ni de police : couleur et fond seulement.
- `reading.line_width` reste en caractères ; zéro = plein écran, le défaut.
- Un test échoue de façon intermittente sur `main` avant ce travail (`cargo test --lib`, 1 sur 357, une fois sur deux). Le relever s'il réapparaît, ne pas l'attribuer au travail en cours sans preuve.

## Structure des fichiers

| Fichier | Rôle |
|---|---|
| `src/app/mod.rs` | `App`, `Panel`, `ReadingStyle` (ex `tui/app.rs`) |
| `src/app/config.rs` | `ConfigScreen`, `Section`, `Field`, `Effect`, **`Control`** (ex `tui/config.rs`) |
| `src/app/search.rs`, `view.rs`, `pace.rs`, `startup.rs` | déplacés tels quels |
| `src/app/keys.rs` | touches neutres `Key`/`KeyCode`, `handle`, `HELP`, `CONFIG_HELP` |
| `src/launch.rs` | `Args`, `prepare()` → `Launch` |
| `src/main.rs` | binaire terminal, vingt lignes |
| `src/bin/sillon-gui.rs` | binaire fenêtre, vingt lignes |
| `src/tui/keymap.rs` | conversion crossterm → `Key` |
| `src/gui/mod.rs` | déclarations ; `text` et `flow` hors feature |
| `src/gui/window.rs` | `run`, `Gui`, boucle de trame, disposition des zones |
| `src/gui/fonts.rs` | polices embarquées, familles |
| `src/gui/theme.rs` | `Palette`, application des visuels |
| `src/gui/keys.rs` | conversion egui → `Key`, garde du focus |
| `src/gui/text.rs` | mise en runs d'un bloc, sans egui |
| `src/gui/flow.rs` | arithmétique du défilement, sans egui |
| `src/gui/reader.rs` | zone de texte : cache, composition, règle, clic |
| `src/gui/header.rs` | en-tête et recherche |
| `src/gui/player.rs` | barre de lecture et ligne d'état |
| `src/gui/panel.rs` | sommaire et signets |
| `src/gui/settings.rs` | configuration en widgets, rubrique Affichage |
| `src/gui/help.rs` | fenêtre d'aide |
| `src/gui/open.rs` | dialogue, dépôt, presse-papiers, extraction en fond, état vide |
| `assets/fonts/` | huit fichiers de police et deux licences OFL |

---

### Task 1 : Le contrôleur déménage sous `app::`

**Files:**
- Move: `src/tui/{app,config,search,view,pace,startup}.rs` → `src/app/{mod,config,search,view,pace,startup}.rs`
- Modify: `src/lib.rs`, `src/main.rs`, `src/tui/mod.rs`, `src/tui/ui.rs`, `src/tui/keymap.rs`, `README.md`

**Interfaces:**
- Produces: `sillon::app::App`, `sillon::app::Panel`, `sillon::app::ReadingStyle`, `sillon::app::config::{ConfigScreen, Effect, Field, Page, Section}`, `sillon::app::{search, view, pace, startup}` — mêmes types, nouveau chemin.

- [ ] **Step 1 : Vérifier le point de départ**

Run: `cargo test 2>&1 | grep -E "^test result"`
Expected: toutes les suites `ok` (ou l'unique échec intermittent connu).

- [ ] **Step 2 : Déplacer les fichiers**

```bash
mkdir -p src/app
git mv src/tui/app.rs src/app/mod.rs
git mv src/tui/config.rs src/app/config.rs
git mv src/tui/search.rs src/app/search.rs
git mv src/tui/view.rs src/app/view.rs
git mv src/tui/pace.rs src/app/pace.rs
git mv src/tui/startup.rs src/app/startup.rs
```

- [ ] **Step 3 : Déclarer les modules**

Dans `src/app/mod.rs`, remplacer la première ligne `//! État de l'application et commandes du lecteur.` par :

```rust
//! Le contrôleur : état de l'application et commandes du lecteur.
//!
//! Il ne sait rien de l'écran qui le montre. Le terminal (`tui`) et la fenêtre
//! (`gui`) le rendent chacun à leur manière et lui envoient les mêmes touches.

pub mod config;
pub mod keys;
pub mod pace;
pub mod search;
pub mod startup;
pub mod view;
```

(`keys` n'existe pas encore : retirer cette ligne pour l'instant, la Task 2 l'ajoute.)

Dans `src/lib.rs`, ajouter `pub mod app;` avant `pub mod audio;`.

Dans `src/tui/mod.rs`, retirer les lignes `pub mod app;`, `pub mod config;`, `pub mod search;`, `pub mod pace;`, `pub mod startup;`, `pub mod view;` et remplacer `use app::App;` par `use crate::app::App;`.

- [ ] **Step 4 : Réécrire les chemins**

```bash
sed -i '' \
  -e 's/crate::tui::config::/crate::app::config::/g' \
  -e 's/crate::tui::pace::/crate::app::pace::/g' \
  -e 's/crate::tui::search::/crate::app::search::/g' \
  -e 's/crate::tui::view::/crate::app::view::/g' \
  -e 's/crate::tui::startup/crate::app::startup/g' \
  -e 's/crate::tui::app::/crate::app::/g' \
  -e 's/crate::tui::ui::ReadingStyle/crate::app::ReadingStyle/g' \
  src/app/*.rs src/tui/*.rs
sed -i '' -e 's/use super::app::{App, Panel};/use crate::app::{App, Panel};/' \
          -e 's/use super::config::Page;/use crate::app::config::Page;/' src/tui/keymap.rs
sed -i '' -e 's/use sillon::tui::app::App;/use sillon::app::App;/' src/main.rs
grep -rn "tui::" src --include='*.rs' | grep -v "^src/tui/mod.rs" | grep -v "tui::run"
```

Expected : la dernière commande ne liste plus que des `use crate::tui::keymap` ou `crate::tui::ui` légitimes dans `src/tui/`. Tout reste à corriger à la main.

- [ ] **Step 5 : Déplacer `ReadingStyle`**

Couper de `src/tui/ui.rs` le bloc :

```rust
/// Réglages d'affichage du texte, tels que la rubrique « Lecture » les fixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingStyle {
    /// Lignes vides insérées entre deux lignes de texte.
    pub spacing: usize,
    /// Souligne d'un fond la ligne où se trouve le mot prononcé.
    pub ruler: bool,
}
```

Le coller dans `src/app/mod.rs` juste avant `pub struct App {`. Dans `src/app/mod.rs`, `reading_style()` renvoie désormais `ReadingStyle` (remplacer les deux occurrences de `crate::app::ReadingStyle` par `ReadingStyle`). Dans `src/tui/ui.rs`, remplacer `use crate::app::{App, Panel};` par `use crate::app::{App, Panel, ReadingStyle};`.

- [ ] **Step 6 : Compiler et tester**

Run: `cargo build 2>&1 | grep -E "^(error|warning)" -A 5 | head -40 && cargo test 2>&1 | grep -E "^test result"`
Expected: aucune erreur ; le même nombre de tests qu'au Step 1.

- [ ] **Step 7 : Mettre à jour la carte du dépôt**

Dans `src/lib.rs` et `README.md` (section *Architecture*), remplacer le schéma par :

```text
entrée ──▶ doc ──▶ tts ──▶ audio ──▶ app ──▶ tui
(flux,    (blocs,  (pool    (sortie,  (état,   (rendu,
 blocs)   phrases) parallèle) position) commandes) touches)
```

Dans la liste des modules du README, ajouter avant `tui` :

```markdown
- `app` — le contrôleur : document, lecture, recherche, signets, écran de configuration. Il ne connaît aucun écran.
```

et reformuler `tui` : `` `tui` — rendu ratatui du contrôleur, adaptation des touches. ``

- [ ] **Step 8 : Commit**

```bash
git add -A src README.md
git commit -m "refactor(app): le contrôleur quitte tui pour app

Aucun comportement ne change. Le second rendu qui vient ne doit pas
importer tui::app."
```

---

### Task 2 : Touches neutres

**Files:**
- Create: `src/app/keys.rs`
- Modify: `src/app/mod.rs` (ajouter `pub mod keys;`), `src/tui/keymap.rs` (réécrit), `src/tui/ui.rs` (import de `HELP`)

**Interfaces:**
- Produces: `app::keys::{Key, KeyCode, handle, HELP, CONFIG_HELP}` ; `Key::plain(KeyCode)`, `Key::char(char)`.
- Produces: `tui::keymap::convert(KeyEvent) -> Option<Key>`, `tui::keymap::handle(app, KeyEvent)` (signature conservée pour `tui::run`).

- [ ] **Step 1 : Écrire `src/app/keys.rs`**

Le corps de `handle_config` et `handle_reader` est celui de `src/tui/keymap.rs`, transposé sur `KeyCode`. Les tests de `keymap.rs` déménagent ici, sur `Key`.

```rust
//! Correspondance touches → commandes, la même pour tous les écrans.
//!
//! Trois contextes se succèdent, du plus captif au plus général : la saisie d'un
//! champ texte, l'écran de configuration, puis la lecture. Chacun consomme les
//! touches qui lui reviennent avant de rendre la main.
//!
//! Le terminal et la fenêtre convertissent leurs événements en `Key` : la
//! logique n'existe qu'une fois, et les deux aides disent la même chose.

use super::config::Page;
use super::{App, Panel};

/// Blocs parcourus par une page de défilement, au terminal.
///
/// Une page entière ferait perdre le fil ; quatre blocs laissent toujours de
/// quoi se raccrocher à ce qu'on vient de lire. La fenêtre, qui connaît sa
/// hauteur, défile en pixels et n'emprunte pas ce chemin.
const VIEW_STEP: isize = 4;

/// Aide de la vue de lecture.
pub const HELP: &[(&str, &str)] = &[
    ("Espace", "Lecture / pause"),
    ("← →", "Phrase précédente / suivante"),
    ("Maj+← →", "Recul / avance rapide"),
    ("↑ ↓", "Bloc précédent / suivant"),
    ("n / p", "Titre suivant / précédent"),
    ("g / Début · Fin", "Début · fin du document"),
    ("r", "Reprendre la phrase"),
    ("+ / -", "Vitesse (resynthèse)"),
    ("1 2 3 4", "Vitesse 0,75× 1× 1,5× 2×"),
    ("[ / ]", "Volume"),
    ("m", "Couper le son"),
    ("b", "Poser / retirer un signet"),
    ("t / B", "Sommaire / signets"),
    ("s", "Configuration et modèles"),
    ("Pg↑ Pg↓", "Défiler sans déplacer la lecture"),
    ("f", "Revenir à la lecture"),
    ("/", "Chercher dans le document"),
    ("N / P", "Résultat suivant / précédent"),
    ("? / h", "Aide"),
    ("q / Échap", "Quitter"),
];

/// Aide de l'écran de configuration.
pub const CONFIG_HELP: &[(&str, &str)] = &[
    ("1 … 5", "Ouvrir une rubrique directement"),
    ("↑ ↓", "Se déplacer dans la liste"),
    ("Entrée", "Ouvrir · éditer · installer · choisir"),
    ("← →", "Modifier une valeur"),
    ("Échap", "Revenir au menu, puis fermer"),
    ("t", "Écouter un aperçu de la voix"),
    ("a", "Montrer les réglages sans effet"),
    ("d", "Supprimer un modèle · annuler"),
    ("w", "Enregistrer la configuration"),
];

/// Une touche, sans le vocabulaire d'aucune bibliothèque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub shift: bool,
    pub ctrl: bool,
}

impl Key {
    pub fn plain(code: KeyCode) -> Self {
        Self { code, shift: false, ctrl: false }
    }

    pub fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }
}

pub fn handle(app: &mut App, key: Key) {
    if key.ctrl && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    // L'aide se referme sur n'importe quelle touche.
    if app.show_help {
        app.show_help = false;
        return;
    }

    if app.show_config {
        handle_config(app, key);
    } else {
        handle_reader(app, key);
    }
}

fn handle_config(app: &mut App, key: Key) {
    // La saisie d'un champ texte capte tout, sauf sa validation et son abandon.
    if app.screen.editing.is_some() {
        match key.code {
            KeyCode::Enter => app.config_commit_edit(),
            KeyCode::Esc => app.config_cancel_edit(),
            KeyCode::Backspace => app.config_edit_backspace(),
            KeyCode::Char(c) => app.config_edit_push(c),
            _ => {}
        }
        return;
    }

    let in_menu = app.screen.page == Page::Menu;
    match key.code {
        // Échap recule d'un cran : d'une rubrique ouverte on revient au menu,
        // et seulement ensuite on quitte l'écran.
        KeyCode::Esc => {
            if !app.config_back_to_menu() {
                app.toggle_config();
            }
        }
        KeyCode::Char('s') => app.toggle_config(),

        // Un chiffre ouvre directement sa rubrique, depuis n'importe où.
        KeyCode::Char(c @ '1'..='5') => {
            app.config_goto_section(c as usize - '1' as usize);
        }

        KeyCode::Up if in_menu => app.config_move_section(-1),
        KeyCode::Down if in_menu => app.config_move_section(1),
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab if in_menu => app.config_open_section(),
        KeyCode::Left if in_menu => {}

        KeyCode::Up => app.config_move_row(-1),
        KeyCode::Down => app.config_move_row(1),
        KeyCode::Right => app.config_adjust(1),
        KeyCode::Left => app.config_adjust(-1),
        KeyCode::Enter => app.config_activate(),
        KeyCode::Tab | KeyCode::BackTab => {
            app.config_back_to_menu();
        }

        KeyCode::Char('a') => app.config_toggle_show_all(),
        KeyCode::Char('t') => app.preview_voice(),
        KeyCode::Char('d') | KeyCode::Delete => app.config_delete(),
        KeyCode::Char('w') => app.save_config(),
        KeyCode::Char('?') | KeyCode::Char('h') => app.show_help = true,
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

fn handle_reader(app: &mut App, key: Key) {
    // La saisie d'une recherche capte tout : sinon « b » poserait un signet au
    // lieu de s'écrire.
    if app.search_input().is_some() {
        match key.code {
            KeyCode::Enter => app.search_commit(),
            KeyCode::Esc => app.search_cancel(),
            KeyCode::Backspace => app.search_backspace(),
            KeyCode::Char(c) => app.search_push(c),
            _ => {}
        }
        return;
    }

    let step = app.config.reading.seek_step as isize;

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            // Échap referme d'abord le panneau ouvert.
            if app.panel != Panel::None {
                app.panel = Panel::None;
            } else {
                app.should_quit = true;
            }
        }

        KeyCode::Char(' ') => app.toggle_play(),

        KeyCode::Right => app.seek_sentences(if key.shift { step } else { 1 }),
        KeyCode::Left => app.seek_sentences(if key.shift { -step } else { -1 }),

        // Les flèches verticales pilotent le panneau ouvert, sinon le texte.
        KeyCode::Up => match app.panel {
            Panel::Contents | Panel::Bookmarks => app.move_selection(-1),
            Panel::None => app.seek_block(-1),
        },
        KeyCode::Down => match app.panel {
            Panel::Contents | Panel::Bookmarks => app.move_selection(1),
            Panel::None => app.seek_block(1),
        },

        KeyCode::Char('n') => app.seek_section(1),
        KeyCode::Char('p') => app.seek_section(-1),
        KeyCode::Home | KeyCode::Char('g') => app.seek_start(),
        KeyCode::End | KeyCode::Char('G') => app.seek_end(),
        KeyCode::Char('r') => app.restart_sentence(),

        KeyCode::Char('+') | KeyCode::Char('=') => app.change_speed(0.1),
        KeyCode::Char('-') | KeyCode::Char('_') => app.change_speed(-0.1),
        KeyCode::Char('1') => app.set_speed(0.75),
        KeyCode::Char('2') => app.set_speed(1.0),
        KeyCode::Char('3') => app.set_speed(1.5),
        KeyCode::Char('4') => app.set_speed(2.0),

        KeyCode::Char(']') => app.change_volume(0.05),
        KeyCode::Char('[') => app.change_volume(-0.05),
        KeyCode::Char('m') => app.toggle_mute(),

        KeyCode::Char('b') => app.toggle_bookmark(),
        KeyCode::Char('B') => app.toggle_panel(Panel::Bookmarks),
        KeyCode::Char('t') | KeyCode::Tab => app.toggle_panel(Panel::Contents),
        KeyCode::Char('s') => app.toggle_config(),
        KeyCode::Char('f') => app.toggle_follow(),

        // Défiler fige la vue : on relit sans interrompre la voix.
        KeyCode::PageUp => app.scroll_view(-VIEW_STEP),
        KeyCode::PageDown => app.scroll_view(VIEW_STEP),

        KeyCode::Char('/') => app.search_open(),
        KeyCode::Char('N') => app.search_advance(1),
        KeyCode::Char('P') => app.search_advance(-1),

        KeyCode::Enter => app.activate_selection(),
        KeyCode::Char('d') => app.delete_selection(),
        KeyCode::Char('w') => app.save_config(),
        KeyCode::Char('?') | KeyCode::Char('h') => app.show_help = true,

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{app_reading, DOC};
    use std::time::Duration;

    fn press(app: &mut App, code: KeyCode) {
        handle(app, Key::plain(code));
    }

    #[test]
    fn the_left_arrow_steps_back_even_in_the_middle_of_a_sentence() {
        let mut app = app_reading(DOC);
        app.seek_to(2);
        // On revient en arrière justement quand la phrase est bien engagée :
        // c'est en l'écoutant qu'on s'aperçoit d'avoir manqué la précédente.
        app.player.simulate(2, Duration::from_secs(3), Duration::from_secs(4));

        press(&mut app, KeyCode::Left);

        assert_eq!(app.current_sentence(), 1, "la flèche a relu la phrase courante");
    }

    #[test]
    fn slash_opens_the_search_prompt() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.search_input(), Some(""));
    }

    #[test]
    fn typing_a_query_never_reaches_the_player() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        // « b » pose un signet en lecture : pendant une saisie, il ne doit
        // qu'écrire.
        for c in "bq".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(app.search_input(), Some("bq"));
        assert!(!app.bookmarked_here(), "un signet a été posé pendant la saisie");
        assert!(!app.should_quit, "le lecteur a quitté pendant la saisie");
    }

    #[test]
    fn enter_commits_the_search_and_escape_abandons_it() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        for c in "deuxieme".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.search_input().is_none());
        assert!(app.search().is_some());

        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::Esc);
        assert!(app.search_input().is_none(), "Échap ferme la saisie");
        assert!(!app.should_quit, "Échap ne doit pas quitter depuis la saisie");
    }

    #[test]
    fn backspace_erases_the_last_letter() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('b'));
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.search_input(), Some("a"));
    }

    #[test]
    fn paging_scrolls_the_view_and_leaves_the_voice_alone() {
        let mut app = app_reading(DOC);
        app.seek_to(app.doc().sentences.len() - 1);
        let reading = app.current_sentence();

        press(&mut app, KeyCode::PageUp);
        assert!(!app.view.is_following(), "la vue devrait s'être figée");
        assert_eq!(app.current_sentence(), reading, "la lecture a suivi la vue");

        press(&mut app, KeyCode::Char('f'));
        assert!(app.view.is_following());
    }

    #[test]
    fn capital_n_walks_through_the_results() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        for c in "phrase".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        let first = app.current_sentence();
        handle(&mut app, Key { code: KeyCode::Char('N'), shift: true, ctrl: false });
        assert_ne!(app.current_sentence(), first, "N n'a pas avancé");
    }

    #[test]
    fn shift_with_an_arrow_seeks_by_the_configured_step() {
        let mut app = app_reading(DOC);
        app.config.reading.seek_step = 2;
        handle(&mut app, Key { code: KeyCode::Right, shift: true, ctrl: false });
        assert_eq!(app.current_sentence(), 2);
    }

    #[test]
    fn control_c_quits_from_anywhere() {
        let mut app = app_reading(DOC);
        app.show_config = true;
        handle(&mut app, Key { code: KeyCode::Char('c'), shift: false, ctrl: true });
        assert!(app.should_quit);
    }
}
```

Ajouter `pub mod keys;` dans `src/app/mod.rs`.

- [ ] **Step 2 : Réécrire `src/tui/keymap.rs` en adaptateur**

```rust
//! Adaptation des touches crossterm aux touches neutres du contrôleur.
//!
//! La logique — qui fait quoi — vit dans `app::keys` ; ici on ne fait que
//! traduire le vocabulaire du terminal.

use crossterm::event::{KeyCode as Ct, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::keys::{self, Key, KeyCode};
use crate::app::App;
pub use crate::app::keys::{CONFIG_HELP, HELP};

pub fn handle(app: &mut App, key: KeyEvent) {
    // Sous Windows, chaque touche émet aussi un événement de relâchement.
    if key.kind == KeyEventKind::Release {
        return;
    }
    if let Some(key) = convert(key) {
        keys::handle(app, key);
    }
}

/// Traduit une touche crossterm, ou `None` si le contrôleur n'en a pas l'usage.
pub fn convert(key: KeyEvent) -> Option<Key> {
    let code = match key.code {
        Ct::Char(c) => KeyCode::Char(c),
        Ct::Left => KeyCode::Left,
        Ct::Right => KeyCode::Right,
        Ct::Up => KeyCode::Up,
        Ct::Down => KeyCode::Down,
        Ct::Enter => KeyCode::Enter,
        Ct::Esc => KeyCode::Esc,
        Ct::Backspace => KeyCode::Backspace,
        Ct::Tab => KeyCode::Tab,
        Ct::BackTab => KeyCode::BackTab,
        Ct::Home => KeyCode::Home,
        Ct::End => KeyCode::End,
        Ct::PageUp => KeyCode::PageUp,
        Ct::PageDown => KeyCode::PageDown,
        Ct::Delete => KeyCode::Delete,
        _ => return None,
    };
    Some(Key {
        code,
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shifted_arrow_keeps_its_shift() {
        let key = convert(KeyEvent::new(Ct::Right, KeyModifiers::SHIFT)).unwrap();
        assert_eq!(key.code, KeyCode::Right);
        assert!(key.shift);
        assert!(!key.ctrl);
    }

    #[test]
    fn a_function_key_has_no_meaning_here() {
        assert!(convert(KeyEvent::new(Ct::F(5), KeyModifiers::NONE)).is_none());
    }
}
```

Dans `src/tui/ui.rs`, remplacer `use crate::tui::keymap::{CONFIG_HELP, HELP};` par `use crate::app::keys::{CONFIG_HELP, HELP};`.

- [ ] **Step 3 : Compiler et tester**

Run: `cargo test 2>&1 | grep -E "^test result|error" | head`
Expected: aucune erreur ; les tests `keys::tests::*` et `keymap::tests::*` passent ; le total a augmenté de quatre (deux tests nouveaux dans `keys`, deux dans `keymap`).

- [ ] **Step 4 : Commit**

```bash
git add src/app/keys.rs src/app/mod.rs src/tui/keymap.rs src/tui/ui.rs
git commit -m "refactor(app): des touches neutres, une seule logique de clavier

Le terminal traduit crossterm ; la fenêtre traduira egui. La
correspondance touches → commandes n'existe qu'une fois, et les deux
aides diront la même chose."
```

---

### Task 3 : `launch`, ce que les deux binaires partagent

**Files:**
- Create: `src/launch.rs`
- Modify: `src/lib.rs` (ajouter `pub mod launch;`), `src/main.rs` (réécrit)

**Interfaces:**
- Produces: `launch::Args` (clap, `Parser`), `launch::prepare(&Args) -> Result<Launch>`, `launch::Launch::{Done, Ready(Prepared)}`, `launch::Prepared { app: App, config_requested: bool, has_input: bool, quiet: Option<logging::StderrRedirect> }`.

- [ ] **Step 1 : Créer `src/launch.rs`**

Tout le contenu de `src/main.rs` hors `main()` y déménage — `Args`, `extracted`, `read_input`, `read_clipboard`, `adopt_installed_model`, `apply_overrides` — inchangé, avec `pub struct Args` et `pub` sur ses champs. Le `main()` actuel devient `prepare` :

```rust
//! Des arguments à un lecteur prêt : ce que les deux binaires partagent.
//!
//! Le terminal et la fenêtre lisent les mêmes options, chargent le même moteur
//! et préparent le même document. Seul le rendu diffère, et il commence après.

use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::app::App;
use crate::config::Config;
use crate::doc::DocumentBuilder;
use crate::input::{self, InputEvent};
use crate::logging::StderrRedirect;
use crate::{doc, tts};

#[derive(Parser, Debug, Default)]
#[command(
    name = "sillon",
    version,
    about = "Lit à voix haute du markdown pipé ou un fichier, avec contrôles de lecture complets."
)]
pub struct Args {
    // … les champs de l'ancien `Args`, tous `pub`, inchangés …
}

/// Ce que `prepare` rend : soit tout est dit, soit un lecteur attend son écran.
pub enum Launch {
    /// Une option a répondu sur la sortie standard : rien à afficher.
    Done,
    Ready(Prepared),
}

pub struct Prepared {
    pub app: App,
    /// `--config` a été demandé explicitement.
    pub config_requested: bool,
    /// Vrai quand il y a quelque chose à lire : fichier, collage ou flux.
    pub has_input: bool,
    /// Redirection de `stderr` vers le journal. À garder tant que l'écran est
    /// ouvert, à lâcher avant d'afficher une erreur.
    pub quiet: Option<StderrRedirect>,
}

pub fn prepare(args: &Args) -> Result<Launch> {
    let (mut config, config_notice) = Config::load();
    let mut notices: Vec<String> = config_notice.into_iter().collect();

    apply_overrides(&mut config, args, &mut notices)?;

    if args.show_config {
        print!("{}", toml::to_string_pretty(&config)?);
        return Ok(Launch::Done);
    }
    if args.write_config {
        let path = config.save()?;
        println!("Configuration écrite dans {}", path.display());
        return Ok(Launch::Done);
    }
    if args.to_markdown {
        if let Some(text) = extracted(args)? {
            print!("{text}");
            return Ok(Launch::Done);
        }
    }

    adopt_installed_model(&mut config, &mut notices);
    if config.align_voice_with_family() {
        notices.push(format!(
            "voix inconnue de {} — repli sur sa voix par défaut",
            config.engine.family
        ));
    }

    let quiet = crate::logging::redirect_stderr();

    let (engine, engine_notice) =
        tts::build_engine(config.engine_preference(), &config.engine_settings());
    notices.extend(engine_notice);

    let (builder, stream, source) = match read_input(args, &config) {
        Ok(read) => read,
        Err(err) => {
            drop(quiet);
            return Err(err);
        }
    };

    let has_input = args.file.is_some() || args.clip || stream.is_some();
    if (args.file.is_some() || args.clip) && builder.sentence_count() == 0 {
        drop(quiet);
        anyhow::bail!("aucun contenu lisible");
    }

    let mut app = App::new(
        builder,
        stream,
        engine,
        config,
        source,
        notices,
        crate::audio::Player::new(),
    );
    // Un fichier a une identité stable : sa lecture reprend où elle en était.
    // Un flux et un collage n'en ont pas — la même position leur désignerait
    // un autre texte.
    if args.file.is_some() {
        app.resume_where_left();
    }

    Ok(Launch::Ready(Prepared {
        app,
        config_requested: args.config,
        has_input,
        quiet,
    }))
}
```

Les commentaires explicatifs de l'ancien `main` (sur la redirection, le chargement du modèle en premier) suivent leur code.

- [ ] **Step 2 : Réduire `src/main.rs`**

```rust
//! `sillon` — lecteur audio en terminal pour du texte pipé ou un fichier.

use anyhow::Result;
use clap::Parser;

use sillon::launch::{self, Args, Launch, Prepared};
use sillon::tui;

fn main() -> Result<()> {
    let args = Args::parse();
    let Launch::Ready(prepared) = launch::prepare(&args)? else {
        return Ok(());
    };
    let Prepared { mut app, config_requested, has_input, quiet } = prepared;

    // Sans document ni flux, on démarre sur l'écran de configuration : sans
    // texte à lire, régler l'outil est la seule chose utile à faire.
    if config_requested || !has_input {
        app.open_config();
    }
    let outcome = tui::run(app);
    // La garde tombe ici : une erreur remontée par `run` retrouve une sortie
    // d'erreur normale avant d'être affichée.
    drop(quiet);
    outcome
}
```

Ajouter `pub mod launch;` dans `src/lib.rs`.

- [ ] **Step 3 : Vérifier**

Run:
```bash
cargo test 2>&1 | grep -E "^test result|error"
cargo run -q -- --show-config | head -3
echo "# Essai" | cargo run -q -- --to-markdown 2>/dev/null; echo "exit $?"
```
Expected: tests inchangés ; `--show-config` imprime `[engine]` ; `--to-markdown` sur un flux se termine sans sortie (rien à extraire d'un flux, comportement d'avant).

- [ ] **Step 4 : Commit**

```bash
git add src/launch.rs src/lib.rs src/main.rs
git commit -m "refactor(launch): préparer le lecteur hors du binaire

Le binaire terminal fait vingt lignes ; celui de la fenêtre en fera
autant."
```

---

### Task 4 : Ce que le contrôleur apprend — vue, recherche, document

**Files:**
- Modify: `src/app/view.rs` (`pin`), `src/app/mod.rs` (`search_with`, `note_position`, `open_document`, tests)

**Interfaces:**
- Produces: `ViewAnchor::pin(&mut self, block: usize)` ; `App::search_with(&mut self, query: &str)` ; `App::open_document(&mut self, builder: DocumentBuilder, input: Option<Receiver<InputEvent>>, source: String)`.
- `App::remember_position` garde sa signature ; `note_position` (privée) note sans écrire.

- [ ] **Step 1 : Test de `pin`**

Dans `src/app/view.rs`, module `tests` :

```rust
    #[test]
    fn pinning_freezes_the_view_on_the_given_block() {
        let mut view = ViewAnchor::default();
        view.pin(12);
        assert!(!view.is_following());
        assert_eq!(view.block(30), 12);
        view.follow();
        assert_eq!(view.block(30), 30);
    }
```

Run: `cargo test --lib app::view 2>&1 | grep -E "error\[|^test result"`
Expected: erreur de compilation, `pin` n'existe pas.

- [ ] **Step 2 : Implémenter `pin`**

Dans `impl ViewAnchor`, après `scroll` :

```rust
    /// Épingle la vue sur `block`, sans compter de crans.
    ///
    /// La fenêtre sait quel bloc est en haut de l'écran après un coup de
    /// molette : elle l'épingle directement.
    pub fn pin(&mut self, block: usize) {
        self.pinned = Some(block);
    }
```

Run: `cargo test --lib app::view 2>&1 | grep -E "^test result"`
Expected: PASS.

- [ ] **Step 3 : Tests de `search_with` et `open_document`**

Dans `src/app/mod.rs`, module `tests` (après `a_cancelled_search_forgets_the_query`) :

```rust
    #[test]
    fn a_search_given_whole_lands_where_a_typed_one_does() {
        let mut typed = app_reading(DOC);
        typed.search_open();
        for c in "troisieme".chars() {
            typed.search_push(c);
        }
        typed.search_commit();

        let mut whole = app_reading(DOC);
        whole.search_with("troisieme");

        assert_eq!(whole.current_sentence(), typed.current_sentence());
        assert_eq!(
            whole.search().map(|s| s.count()),
            typed.search().map(|s| s.count())
        );
        assert!(whole.search_input().is_none(), "la saisie est close une fois validée");
    }

    #[test]
    fn opening_another_document_keeps_the_engine_loaded() {
        let mut app = app_reading(DOC);
        let engine = app.synth.engine();
        app.seek_to(2);

        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::input::split_blocks("# Autre\n\nUne phrase neuve.\n") {
            builder.push_block(&block);
        }
        app.open_document(builder, None, "autre.md".to_string());

        assert!(Arc::ptr_eq(&engine, &app.synth.engine()), "le moteur a été rechargé");
        assert_eq!(app.doc().title, "Autre");
        assert_eq!(app.source, "autre.md");
        assert_eq!(app.current_sentence(), 0);
        assert!(app.search().is_none());
        assert!(app.view.is_following());
        assert!(app.input_complete());
    }

    #[test]
    fn opening_another_document_notes_where_the_first_one_stopped() {
        let mut app = app_reading(DOC);
        // « test.md » devient un document dont la position se retient.
        app.resume_where_left();
        app.seek_to(1);
        let snippet = app.doc().sentence(1).unwrap().written();

        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("Un autre texte.");
        app.open_document(builder, None, "autre.md".to_string());

        let kept = app.positions.get("test.md").expect("la position du premier document");
        assert_eq!(kept.sentence_id, 1);
        assert_eq!(kept.snippet, snippet);
        // Le nouveau document n'a pas encore d'identité stable.
        assert!(app.place_to_remember().is_none());
    }
```

Run: `cargo test --lib app::tests::opening 2>&1 | grep -E "error\[|^test result"`
Expected: erreur de compilation, `open_document` et `search_with` n'existent pas.

- [ ] **Step 4 : Implémenter**

Dans `src/app/mod.rs`, après `search_advance` :

```rust
    /// Lance une recherche d'un seul geste, comme une saisie validée.
    ///
    /// Un champ de saisie d'une fenêtre livre la requête entière ; la faire
    /// entrer caractère par caractère serait mimer un clavier qu'on n'a pas.
    pub fn search_with(&mut self, query: &str) {
        self.search_input = Some(query.to_string());
        self.search_commit();
    }
```

Remplacer `remember_position` par :

```rust
    /// Note où en était la lecture, dans la mémoire des positions.
    ///
    /// Sans écrire : l'écriture a lieu à la fermeture, une fois pour tous les
    /// documents lus dans la session.
    fn note_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        match self.place_to_remember() {
            Some((id, snippet)) => self.positions.record(&self.source, id, &snippet),
            None => self.positions.forget(&self.source),
        }
    }

    /// Note où en était la lecture et l'écrit, pour la prochaine ouverture.
    pub fn remember_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        self.note_position();
        // La fermeture est en cours : plus personne n'est là pour lire une
        // erreur, et une position perdue coûte un défilement.
        let _ = self.positions.save();
    }
```

Après `remember_position`, ajouter :

```rust
    /// Remplace le document lu, en gardant le moteur chargé.
    ///
    /// Ouvrir un autre fichier ne doit pas coûter le chargement d'un modèle :
    /// le moteur est partagé, et la synthèse se reconstruit dessus comme le
    /// fait `reload_engine`. La position du document quitté est notée avant.
    pub fn open_document(
        &mut self,
        builder: DocumentBuilder,
        input: Option<Receiver<InputEvent>>,
        source: String,
    ) {
        self.note_position();
        self.stop_preview();
        self.player.flush();

        let has_stream = input.is_some();
        self.builder = builder;
        self.input = input;
        self.assembler = BlockAssembler::new();
        self.published = 0;
        self.input_done = !has_stream;
        self.source = source;
        self.remembers_place = false;
        self.queue_head = 0;
        self.last_known = 0;
        self.skipped.clear();
        self.pace = Pace::default();
        self.panel = Panel::None;
        self.selection = 0;
        self.show_config = false;
        self.view = ViewAnchor::default();
        self.search_input = None;
        self.search = None;
        self.started_at = Instant::now();
        self.synthesis_started = false;

        // Une synthèse neuve sur le même moteur. L'ancienne se libère hors du
        // thread d'interface : sa destruction attend les phrases en cours.
        let synth = Synthesizer::new(
            self.synth.engine(),
            self.config.tts_params(),
            self.config.sessions() as usize,
            self.config.engine.prefetch as usize,
        );
        let previous = std::mem::replace(&mut self.synth, synth);
        std::thread::spawn(move || drop(previous));

        if !has_stream {
            self.builder.finish();
            self.builder.sync_word_total();
        }
        self.publish_new_sentences();
        if self.config.reading.autoplay {
            self.player.play();
        } else {
            self.player.pause();
        }
    }
```

Run: `cargo test --lib app:: 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: PASS, tous.

- [ ] **Step 5 : Commit**

```bash
git add src/app/view.rs src/app/mod.rs
git commit -m "feat(app): épingler la vue, chercher d'un geste, changer de document

Trois gestes qu'une fenêtre fait autrement qu'un terminal : la molette
épingle un bloc, un champ livre la requête entière, et ouvrir un autre
fichier garde le modèle chargé."
```

---

### Task 5 : Un réglage se pose par valeur

**Files:**
- Modify: `src/app/config.rs` (`Control`, `Field::control/number/set_number/set_choice`, `Effect::SetVolume`, tests), `src/app/mod.rs` (`apply_effect` public, `config_set_number/choice/text`, `config_commit_edit`, test)

**Interfaces:**
- Produces: `Control::{Slider{min,max,step,decimals,unset}, Integer{min,max,unset,zero}, Choice(&[&str]), Voices(&[VoiceOption]), Toggle, Text}` ; `Field::control(&self, &Config) -> Control` ; `Field::number(&self, &Config, volume: f32) -> Option<f64>` ; `Field::set_number(&self, &mut Config, Option<f64>) -> Effect` ; `Field::set_choice(&self, &mut Config, &str) -> Effect` ; `Effect::SetVolume(f32)` ; `App::apply_effect(&mut self, Effect)` public ; `App::config_set_number(Field, Option<f64>)`, `App::config_set_choice(Field, &str)`, `App::config_set_text(Field, String)`.

- [ ] **Step 1 : Tests dans `src/app/config.rs`**

Dans le module `tests` :

```rust
    #[test]
    fn every_field_declares_a_control() {
        let config = Config::default();
        for section in Section::ALL {
            for field in section.fields() {
                // Aucune panique, et un champ textuel se dit textuel.
                let control = field.control(&config);
                assert_eq!(
                    matches!(control, Control::Text),
                    field.is_text(&config),
                    "{field:?}"
                );
            }
        }
    }

    #[test]
    fn a_value_set_from_a_slider_is_clamped_like_a_step() {
        let mut config = Config::default();
        assert_eq!(Field::Speed.set_number(&mut config, Some(9.0)), Effect::Resynthesize);
        assert_eq!(config.voice.speed, 3.0);
        Field::Speed.set_number(&mut config, Some(0.1));
        assert_eq!(config.voice.speed, 0.5);
    }

    #[test]
    fn an_optional_setting_can_be_given_back_to_the_model() {
        let mut config = Config::default();
        Field::Temperature.set_number(&mut config, Some(1.2));
        assert_eq!(config.sampling.temperature, Some(1.2));
        Field::Temperature.set_number(&mut config, None);
        assert_eq!(config.sampling.temperature, None);
        // Un réglage qui n'a pas d'état « modèle » ignore `None`.
        assert_eq!(Field::Speed.set_number(&mut config, None), Effect::None);
        assert_eq!(config.voice.speed, 1.0);
    }

    #[test]
    fn zero_keeps_its_special_meaning() {
        let mut config = Config::default();
        Field::LineWidth.set_number(&mut config, Some(0.0));
        assert_eq!(config.reading.line_width, 0, "zéro reste le plein écran");
        Field::LineWidth.set_number(&mut config, Some(10.0));
        assert_eq!(config.reading.line_width, 40, "sous le minimum, le minimum");
        Field::Sessions.set_number(&mut config, Some(0.0));
        assert_eq!(config.engine.sessions, 0, "zéro reste « auto »");
    }

    #[test]
    fn the_volume_is_set_rather_than_stepped() {
        let mut config = Config::default();
        assert_eq!(Field::Volume.set_number(&mut config, Some(0.3)), Effect::SetVolume(0.3));
    }

    #[test]
    fn a_choice_outside_the_list_is_refused() {
        let mut config = Config::default();
        assert_eq!(Field::Language.set_choice(&mut config, "xx"), Effect::None);
        assert_eq!(config.voice.language, "fr");
        assert_eq!(Field::Language.set_choice(&mut config, "en"), Effect::Resynthesize);
        assert_eq!(config.voice.language, "en");
    }

    #[test]
    fn the_current_number_reads_back_what_was_set() {
        let mut config = Config::default();
        Field::Threads.set_number(&mut config, Some(6.0));
        assert_eq!(Field::Threads.number(&config, 1.0), Some(6.0));
        assert_eq!(Field::Volume.number(&config, 0.4), Some(0.4f32 as f64));
        assert_eq!(Field::TopK.number(&config, 1.0), None);
    }
```

Run: `cargo test --lib app::config 2>&1 | grep -E "error\[|^test result" | head -3`
Expected: erreurs de compilation (`Control`, `set_number`…).

- [ ] **Step 2 : `Control` et les accesseurs**

Dans `src/app/config.rs`, après la définition de `Effect`, remplacer l'enum par :

```rust
/// Ce qu'un changement de réglage impose au reste de l'application.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    None,
    /// Régénérer l'audio déjà préparé.
    Resynthesize,
    /// Recharger le moteur, donc tout le pipeline de synthèse.
    ReloadEngine,
    /// Le découpage du document changerait : effet au prochain lancement.
    Reparse,
    /// Ajuster le volume de sortie du delta indiqué.
    Volume(f32),
    /// Poser le volume de sortie à cette valeur — un curseur, pas une flèche.
    SetVolume(f32),
}

/// Ce qu'un réglage est, pour qu'un écran choisisse comment le montrer.
///
/// Le terminal fait « plus un » avec les flèches et n'a pas besoin de le
/// savoir ; une fenêtre pose une valeur d'un curseur ou d'une liste, et doit
/// connaître les bornes qu'`adjust` gardait pour lui.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// Nombre à virgule. `unset` décrit l'état « laissé au modèle » quand il
    /// existe : son libellé, et la valeur de départ quand on le quitte.
    Slider {
        min: f64,
        max: f64,
        step: f64,
        decimals: usize,
        unset: Option<(&'static str, f64)>,
    },
    /// Entier. `zero` nomme la valeur zéro quand elle a un sens à part
    /// (« plein écran », « auto ») ; `min` s'applique alors au-delà de zéro.
    Integer {
        min: u64,
        max: u64,
        unset: Option<(&'static str, u64)>,
        zero: Option<&'static str>,
    },
    /// Une valeur parmi une liste fermée.
    Choice(&'static [&'static str]),
    /// Une voix parmi celles de la famille chargée.
    Voices(&'static [models::VoiceOption]),
    Toggle,
    Text,
}

fn slider(min: f64, max: f64, step: f64, decimals: usize) -> Control {
    Control::Slider { min, max, step, decimals, unset: None }
}

fn optional(min: f64, max: f64, step: f64, decimals: usize, start: f64) -> Control {
    Control::Slider { min, max, step, decimals, unset: Some(("(modèle)", start)) }
}

fn integer(min: u64, max: u64) -> Control {
    Control::Integer { min, max, unset: None, zero: None }
}
```

Dans `impl Field`, après `adjust` :

```rust
    /// Ce que le réglage est : ses bornes, ses choix, sa nature.
    pub fn control(&self, config: &Config) -> Control {
        match self {
            Field::Speed => slider(0.5, 3.0, 0.05, 2),
            Field::Pitch => slider(-1.0, 1.0, 0.05, 2),
            Field::Energy => slider(0.0, 2.0, 0.05, 2),
            Field::Volume => slider(0.0, 1.0, 0.05, 2),
            Field::Language => Control::Choice(LANGUAGES),
            Field::Emotion => Control::Choice(EMOTIONS),
            Field::Voice => {
                let voices = models::voices_for_family(&config.engine.family);
                if voices.is_empty() {
                    Control::Text
                } else {
                    Control::Voices(voices)
                }
            }
            Field::Instruction | Field::ModelPath | Field::Family => Control::Text,
            Field::Temperature => optional(0.0, 2.0, 0.05, 2, 0.7),
            Field::TopP => optional(0.0, 1.0, 0.05, 2, 0.9),
            Field::TopK => Control::Integer {
                min: 1,
                max: 200,
                unset: Some(("(modèle)", 50)),
                zero: None,
            },
            Field::GuidanceScale => optional(0.0, 20.0, 0.5, 1, 3.0),
            Field::RepetitionPenalty => optional(0.5, 2.0, 0.05, 2, 1.1),
            Field::InferenceSteps => Control::Integer {
                min: 1,
                max: 200,
                unset: Some(("(modèle)", 16)),
                zero: None,
            },
            Field::Seed => Control::Integer {
                min: 0,
                max: u32::MAX as u64,
                unset: Some(("(aléatoire)", 0)),
                zero: None,
            },
            Field::CodeBlocks => Control::Choice(CODE_POLICIES),
            Field::Autoplay | Field::ReadingRuler => Control::Toggle,
            Field::SeekStep => integer(1, 100),
            Field::LineWidth => Control::Integer {
                min: 40,
                max: 120,
                unset: None,
                zero: Some("plein écran"),
            },
            Field::LineSpacing => integer(0, 2),
            Field::EnginePref => Control::Choice(ENGINES),
            Field::Compute => Control::Choice(COMPUTES),
            Field::Threads => integer(1, 64),
            Field::Sessions => Control::Integer {
                min: 1,
                max: 16,
                unset: None,
                zero: Some("auto"),
            },
            Field::Prefetch => integer(2, 64),
        }
    }

    /// Valeur numérique courante, `None` pour un réglage laissé au modèle.
    ///
    /// Le volume vient de la sortie audio, pas de la configuration : il est
    /// passé par l'appelant, comme pour `value`.
    pub fn number(&self, config: &Config, volume: f32) -> Option<f64> {
        let wide = |v: f32| Some(f64::from(v));
        let optional = |v: Option<f32>| v.map(f64::from);
        match self {
            Field::Speed => wide(config.voice.speed),
            Field::Pitch => wide(config.voice.pitch),
            Field::Energy => wide(config.voice.energy),
            Field::Volume => wide(volume),
            Field::Temperature => optional(config.sampling.temperature),
            Field::TopP => optional(config.sampling.top_p),
            Field::TopK => config.sampling.top_k.map(f64::from),
            Field::GuidanceScale => optional(config.sampling.guidance_scale),
            Field::RepetitionPenalty => optional(config.sampling.repetition_penalty),
            Field::InferenceSteps => config.sampling.inference_steps.map(f64::from),
            Field::Seed => config.sampling.seed.map(|s| s as f64),
            Field::SeekStep => Some(f64::from(config.reading.seek_step)),
            Field::LineWidth => Some(f64::from(config.reading.line_width)),
            Field::LineSpacing => Some(f64::from(config.reading.line_spacing)),
            Field::Threads => Some(f64::from(config.engine.threads)),
            Field::Sessions => Some(f64::from(config.engine.sessions)),
            Field::Prefetch => Some(f64::from(config.engine.prefetch)),
            _ => None,
        }
    }

    /// Pose une valeur numérique, bornée comme `adjust` la bornerait.
    ///
    /// `None` rend le réglage au modèle quand il peut l'être, et ne fait rien
    /// sinon. Renvoie ce qui doit être répercuté.
    pub fn set_number(&self, config: &mut Config, value: Option<f64>) -> Effect {
        let (bounded, optional) = match self.control(config) {
            Control::Slider { min, max, unset, .. } => {
                (value.map(|v| v.clamp(min, max)), unset.is_some())
            }
            Control::Integer { min, max, unset, zero } => {
                let bounded = value.map(|v| {
                    let v = v.round().max(0.0);
                    if zero.is_some() && v == 0.0 {
                        0.0
                    } else {
                        v.clamp(min as f64, max as f64)
                    }
                });
                (bounded, unset.is_some())
            }
            _ => return Effect::None,
        };
        if bounded.is_none() && !optional {
            return Effect::None;
        }
        let narrow = |v: f64| v as f32;
        let whole = |v: f64| v as u32;
        match self {
            Field::Speed => {
                if let Some(v) = bounded {
                    config.voice.speed = narrow(v);
                }
                Effect::Resynthesize
            }
            Field::Pitch => {
                if let Some(v) = bounded {
                    config.voice.pitch = narrow(v);
                }
                Effect::Resynthesize
            }
            Field::Energy => {
                if let Some(v) = bounded {
                    config.voice.energy = narrow(v);
                }
                Effect::Resynthesize
            }
            Field::Volume => match bounded {
                Some(v) => Effect::SetVolume(narrow(v)),
                None => Effect::None,
            },
            Field::Temperature => {
                config.sampling.temperature = bounded.map(narrow);
                Effect::Resynthesize
            }
            Field::TopP => {
                config.sampling.top_p = bounded.map(narrow);
                Effect::Resynthesize
            }
            Field::TopK => {
                config.sampling.top_k = bounded.map(whole);
                Effect::Resynthesize
            }
            Field::GuidanceScale => {
                config.sampling.guidance_scale = bounded.map(narrow);
                Effect::Resynthesize
            }
            Field::RepetitionPenalty => {
                config.sampling.repetition_penalty = bounded.map(narrow);
                Effect::Resynthesize
            }
            Field::InferenceSteps => {
                config.sampling.inference_steps = bounded.map(whole);
                Effect::Resynthesize
            }
            Field::Seed => {
                config.sampling.seed = bounded.map(|v| v as u64);
                Effect::Resynthesize
            }
            Field::SeekStep => {
                if let Some(v) = bounded {
                    config.reading.seek_step = whole(v);
                }
                Effect::None
            }
            Field::LineWidth => {
                if let Some(v) = bounded {
                    config.reading.line_width = whole(v);
                }
                Effect::None
            }
            Field::LineSpacing => {
                if let Some(v) = bounded {
                    config.reading.line_spacing = whole(v);
                }
                Effect::None
            }
            Field::Threads => {
                if let Some(v) = bounded {
                    config.engine.threads = whole(v);
                }
                Effect::ReloadEngine
            }
            Field::Sessions => {
                if let Some(v) = bounded {
                    config.engine.sessions = whole(v);
                }
                Effect::ReloadEngine
            }
            Field::Prefetch => {
                if let Some(v) = bounded {
                    config.engine.prefetch = whole(v);
                }
                Effect::ReloadEngine
            }
            _ => Effect::None,
        }
    }

    /// Pose un choix parmi la liste du réglage. Un choix inconnu est refusé.
    pub fn set_choice(&self, config: &mut Config, value: &str) -> Effect {
        match self.control(config) {
            Control::Choice(choices) if choices.contains(&value) => {}
            Control::Voices(voices) if voices.iter().any(|v| v.id == value) => {}
            _ => return Effect::None,
        }
        match self {
            Field::Language => {
                config.voice.language = value.to_string();
                Effect::Resynthesize
            }
            Field::Emotion => {
                config.voice.emotion = value.to_string();
                Effect::Resynthesize
            }
            Field::Voice => {
                config.voice.voice = value.to_string();
                Effect::Resynthesize
            }
            Field::CodeBlocks => {
                config.reading.code_blocks = value.to_string();
                Effect::Reparse
            }
            Field::EnginePref => {
                config.engine.engine = value.to_string();
                Effect::ReloadEngine
            }
            Field::Compute => {
                config.engine.compute = value.to_string();
                Effect::ReloadEngine
            }
            _ => Effect::None,
        }
    }
```

Run: `cargo test --lib app::config 2>&1 | grep -E "^test result|FAILED"`
Expected: PASS.

- [ ] **Step 3 : Test côté `App`**

Dans `src/app/mod.rs`, module `tests` :

```rust
    #[test]
    fn setting_the_volume_moves_the_player_and_the_config() {
        let mut app = app_reading(DOC);
        app.config_set_number(Field::Volume, Some(0.25));
        assert!((app.player.volume() - 0.25).abs() < 1e-6);
        assert!((app.config.voice.volume - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_text_setting_reloads_the_engine_only_when_it_names_the_model() {
        let mut app = app_reading(DOC);
        let engine = app.synth.engine();
        app.config_set_text(Field::Instruction, "  lentement ".to_string());
        assert_eq!(app.config.voice.instruction, "lentement");
        assert!(Arc::ptr_eq(&engine, &app.synth.engine()), "une consigne ne recharge rien");
    }
```

- [ ] **Step 4 : Méthodes de `App`**

Dans `src/app/mod.rs`, rendre `apply_effect` publique avec sa doc et son nouveau bras :

```rust
    /// Répercute un changement de réglage sur la lecture.
    pub fn apply_effect(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Volume(delta) => self.change_volume(delta),
            Effect::SetVolume(volume) => {
                let volume = volume.clamp(0.0, 2.0);
                self.player.set_volume(volume);
                self.config.voice.volume = volume;
                self.notify(&format!("Volume {:.0} %", volume * 100.0));
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
```

Remplacer `config_commit_edit` par :

```rust
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

    /// Pose la valeur d'un réglage numérique — un curseur, pas une flèche.
    pub fn config_set_number(&mut self, field: Field, value: Option<f64>) {
        let effect = field.set_number(&mut self.config, value);
        self.apply_effect(effect);
    }

    /// Pose un choix — une liste déroulante, pas une flèche.
    pub fn config_set_choice(&mut self, field: Field, value: &str) {
        let effect = field.set_choice(&mut self.config, value);
        self.apply_effect(effect);
    }
```

Run: `cargo test 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: PASS partout.

- [ ] **Step 5 : Commit**

```bash
git add src/app/config.rs src/app/mod.rs
git commit -m "feat(app): un réglage se pose par valeur, pas seulement par cran

Control dit ce qu'un réglage est — curseur, entier, choix, case, texte —
avec les bornes qu'adjust gardait pour lui. Une consigne de style ne
recharge plus le moteur : elle part avec chaque requête."
```

---

### Task 6 : La section `[gui]` de la configuration

**Files:**
- Modify: `src/config.rs` (`GuiConfig`, champ `gui`, tests), `README.md` (exemple de configuration)

**Interfaces:**
- Produces: `config::GuiConfig { font: String, font_size: f32, line_height: f32, theme: String }`, `GuiConfig::FONTS`, `GuiConfig::THEMES`, `GuiConfig::font_size() -> f32` (borné 14–40), `GuiConfig::line_height() -> f32` (borné 1,0–2,5) ; `Config::gui`.

- [ ] **Step 1 : Tests**

Dans `src/config.rs`, module `tests` :

```rust
    #[test]
    fn a_config_without_a_gui_section_gets_the_window_defaults() {
        let parsed: Config = toml::from_str("[voice]\nspeed = 1.5\n").unwrap();
        assert_eq!(parsed.gui.font, "atkinson");
        assert_eq!(parsed.gui.font_size, 22.0);
        assert_eq!(parsed.gui.line_height, 1.6);
        assert_eq!(parsed.gui.theme, "dark");
    }

    #[test]
    fn window_sizes_are_kept_within_reason() {
        let gui = GuiConfig { font_size: 200.0, line_height: 0.2, ..GuiConfig::default() };
        assert_eq!(gui.font_size(), 40.0);
        assert_eq!(gui.line_height(), 1.0);
    }
```

Run: `cargo test --lib config::tests 2>&1 | grep -E "error\[|^test result" | head -3`
Expected: erreur, `GuiConfig` inconnu.

- [ ] **Step 2 : Implémenter**

Dans `src/config.rs`, ajouter `pub gui: GuiConfig,` à `Config` après `reading`, et après `ReadingConfig` :

```rust
/// Réglages propres à la fenêtre. Le terminal ne les lit pas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GuiConfig {
    /// Police de lecture : `atkinson`, `opendyslexic` ou `default`.
    pub font: String,
    /// Taille du texte de lecture, en pixels.
    pub font_size: f32,
    /// Interlignage, en fraction de la taille.
    pub line_height: f32,
    /// `dark` ou `light`.
    pub theme: String,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            font: "atkinson".to_string(),
            // Grande, pour un grand écran et une lecture longue.
            font_size: 22.0,
            // La recommandation courante pour la lecture accompagnée.
            line_height: 1.6,
            theme: "dark".to_string(),
        }
    }
}

impl GuiConfig {
    pub const FONTS: &'static [&'static str] = &["atkinson", "opendyslexic", "default"];
    pub const THEMES: &'static [&'static str] = &["dark", "light"];

    /// Taille bornée : un fichier édité à la main ne doit pas rendre la
    /// fenêtre illisible.
    pub fn font_size(&self) -> f32 {
        self.font_size.clamp(14.0, 40.0)
    }

    pub fn line_height(&self) -> f32 {
        self.line_height.clamp(1.0, 2.5)
    }
}
```

Dans `README.md`, après le bloc `[reading]` de l'exemple `config.toml` :

```toml
[gui]                     # la fenêtre seule ; voir « Interface graphique »
font = "atkinson"         # atkinson | opendyslexic | default
font_size = 22.0          # pixels
line_height = 1.6         # en fraction de la taille
theme = "dark"            # dark | light
```

Run: `cargo test --lib config 2>&1 | grep -E "^test result|FAILED"`
Expected: PASS, y compris `defaults_round_trip_through_toml`.

- [ ] **Step 3 : Commit**

```bash
git add src/config.rs README.md
git commit -m "feat(config): une section [gui] pour la fenêtre

Police, taille, interlignage, thème. Le terminal ne la lit pas."
```

---

### Task 7 : La fenêtre s'ouvre — feature, binaire, polices, thème

**Files:**
- Modify: `Cargo.toml`, `src/lib.rs`, `.gitignore` (rien), `scripts/` (Task 16)
- Create: `src/bin/sillon-gui.rs`, `src/gui/mod.rs`, `src/gui/window.rs`, `src/gui/fonts.rs`, `src/gui/theme.rs`, `assets/fonts/*`

**Interfaces:**
- Produces: `sillon::gui::run(app: App) -> anyhow::Result<()>` (feature `gui`) ; `gui::window::Gui { app, … }` ; `gui::fonts::{install(ctx, font), family(font, strong, italic) -> FontFamily, ATKINSON, OPENDYSLEXIC, DEFAULT}` ; `gui::theme::{Palette, palette(theme) -> Palette, apply(ctx, theme, font_size)}`.
- `gui::text` et `gui::flow` (Tasks 8–9) sont déclarés sans garde de feature ; tout le reste sous `#[cfg(feature = "gui")]`.

- [ ] **Step 1 : Dépendances et binaire**

Dans `Cargo.toml`, ajouter aux `[dependencies]` :

```toml
eframe = { version = "0.36", optional = true }
rfd = { version = "0.17", optional = true }
```

Aux `[features]` :

```toml
# La fenêtre : eframe (wgpu) et le dialogue de fichiers. Optionnelle, comme
# audio-cpp — sans elle le build reste une affaire de secondes.
gui = ["dep:eframe", "dep:rfd"]
```

Après le `[[bin]] sillon` :

```toml
[[bin]]
name = "sillon-gui"
path = "src/bin/sillon-gui.rs"
required-features = ["gui"]
```

- [ ] **Step 2 : Polices**

```bash
mkdir -p assets/fonts
SP=/private/tmp/claude-501/-Users-tharsan-Workspace-reader/9a34b428-c627-4190-a132-6b93bdd1f7be/scratchpad/fonts
cp $SP/AtkinsonHyperlegible-{Regular,Bold,Italic,BoldItalic}.ttf assets/fonts/
cp $SP/OpenDyslexic-{Regular,Bold,Italic,BoldItalic}.otf assets/fonts/
cp $SP/OFL-Atkinson.txt assets/fonts/OFL-AtkinsonHyperlegible.txt
cp $SP/OFL-OpenDyslexic.txt assets/fonts/OFL-OpenDyslexic.txt
ls -la assets/fonts
```

(Sources : `github.com/googlefonts/atkinson-hyperlegible`, `github.com/antijingoist/opendyslexic`, licence SIL OFL 1.1.)

- [ ] **Step 3 : `src/gui/mod.rs`**

```rust
//! Interface graphique : une fenêtre eframe autour du même contrôleur.
//!
//! Deux modules ne touchent pas à egui et se compilent toujours : `text` met un
//! bloc en runs, `flow` fait l'arithmétique du défilement. C'est là que se
//! vérifie ce que la fenêtre montre. Le reste dessine, et n'existe qu'avec la
//! feature `gui`.

pub mod flow;
pub mod text;

#[cfg(feature = "gui")]
mod fonts;
#[cfg(feature = "gui")]
mod theme;
#[cfg(feature = "gui")]
mod window;

#[cfg(feature = "gui")]
pub use window::run;
```

Dans `src/lib.rs`, ajouter `pub mod gui;` après `pub mod doc;`.

- [ ] **Step 4 : `src/gui/fonts.rs`**

```rust
//! Polices embarquées : la fenêtre ne dépend pas de celles de la machine.
//!
//! egui ne connaît pas la graisse : chaque face est une famille à part, et
//! `family` choisit la bonne pour un mot en gras, en italique, ou les deux.
//! Les polices d'origine restent en repli, pour les symboles et les emoji.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub const ATKINSON: &str = "atkinson";
pub const OPENDYSLEXIC: &str = "opendyslexic";
pub const DEFAULT: &str = "default";

struct Face {
    name: &'static str,
    data: &'static [u8],
}

const FACES: &[Face] = &[
    Face { name: "atkinson", data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Regular.ttf") },
    Face { name: "atkinson-bold", data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Bold.ttf") },
    Face { name: "atkinson-italic", data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Italic.ttf") },
    Face { name: "atkinson-bold-italic", data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-BoldItalic.ttf") },
    Face { name: "opendyslexic", data: include_bytes!("../../assets/fonts/OpenDyslexic-Regular.otf") },
    Face { name: "opendyslexic-bold", data: include_bytes!("../../assets/fonts/OpenDyslexic-Bold.otf") },
    Face { name: "opendyslexic-italic", data: include_bytes!("../../assets/fonts/OpenDyslexic-Italic.otf") },
    Face { name: "opendyslexic-bold-italic", data: include_bytes!("../../assets/fonts/OpenDyslexic-BoldItalic.otf") },
];

/// Installe les polices embarquées et fait de `font` celle de toute la fenêtre.
pub fn install(ctx: &egui::Context, font: &str) {
    let mut definitions = FontDefinitions::default();
    let fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for face in FACES {
        definitions
            .font_data
            .insert(face.name.to_string(), Arc::new(FontData::from_static(face.data)));
        let mut chain = vec![face.name.to_string()];
        chain.extend(fallback.iter().cloned());
        definitions
            .families
            .insert(FontFamily::Name(face.name.into()), chain);
    }
    // L'interface entière parle avec la police de lecture : boutons, listes,
    // réglages. Une seule police à l'écran, c'est une chose de moins à lire.
    if let FontFamily::Name(name) = family(font, false, false) {
        let mut chain = vec![name.to_string()];
        chain.extend(fallback.iter().cloned());
        definitions.families.insert(FontFamily::Proportional, chain);
    }
    ctx.set_fonts(definitions);
}

/// La famille d'un texte de cette police, dans cette graisse et cette posture.
pub fn family(font: &str, strong: bool, italic: bool) -> FontFamily {
    let base = match font {
        ATKINSON => "atkinson",
        OPENDYSLEXIC => "opendyslexic",
        _ => return FontFamily::Proportional,
    };
    let name = match (strong, italic) {
        (false, false) => base.to_string(),
        (true, false) => format!("{base}-bold"),
        (false, true) => format!("{base}-italic"),
        (true, true) => format!("{base}-bold-italic"),
    };
    FontFamily::Name(name.into())
}

/// Vrai quand la police a une vraie italique : egui n'a alors pas à en
/// fabriquer une en penchant les glyphes.
pub fn has_italic(font: &str) -> bool {
    matches!(font, ATKINSON | OPENDYSLEXIC)
}
```

- [ ] **Step 5 : `src/gui/theme.rs`**

```rust
//! Palette et visuels de la fenêtre.
//!
//! La palette est celle du terminal : un accent, des gris étagés. Le thème
//! clair inverse les gris et garde l'accent ; le mot prononcé, la règle et la
//! phrase en cours gardent leurs contrastes dans les deux.

use eframe::egui::{self, Color32, FontId, TextStyle, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0xF2, 0x6B, 0x27);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub accent: Color32,
    /// Texte posé sur l'accent : le mot prononcé.
    pub on_accent: Color32,
    pub bright: Color32,
    pub normal: Color32,
    pub dim: Color32,
    pub faint: Color32,
    /// Ce qui n'est pas de la prose : un lien, un fragment de code.
    pub technical: Color32,
    /// Fond de la règle de lecture.
    pub ruler: Color32,
    /// Fond de la zone de texte.
    pub background: Color32,
    /// Fond des barres et panneaux.
    pub panel: Color32,
    /// Fond d'un bloc de code ou d'un tableau.
    pub code_bg: Color32,
}

pub fn palette(theme: &str) -> Palette {
    if theme == "light" {
        Palette {
            accent: ACCENT,
            on_accent: Color32::from_rgb(0x1A, 0x12, 0x0A),
            bright: Color32::from_rgb(0x16, 0x16, 0x16),
            normal: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            dim: Color32::from_rgb(0x70, 0x70, 0x70),
            faint: Color32::from_rgb(0xA6, 0xA6, 0xA6),
            technical: Color32::from_rgb(0x2E, 0x6E, 0x9E),
            ruler: Color32::from_rgb(0xF4, 0xE8, 0xDA),
            background: Color32::from_rgb(0xFB, 0xF9, 0xF5),
            panel: Color32::from_rgb(0xEF, 0xED, 0xE8),
            code_bg: Color32::from_rgb(0xEE, 0xEE, 0xEE),
        }
    } else {
        Palette {
            accent: ACCENT,
            on_accent: Color32::BLACK,
            bright: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            normal: Color32::from_rgb(0xBA, 0xBA, 0xBA),
            dim: Color32::from_rgb(0x78, 0x78, 0x78),
            faint: Color32::from_rgb(0x52, 0x52, 0x52),
            technical: Color32::from_rgb(0x7E, 0xA8, 0xC4),
            ruler: Color32::from_rgb(0x26, 0x26, 0x2C),
            background: Color32::from_rgb(0x14, 0x14, 0x16),
            panel: Color32::from_rgb(0x1D, 0x1D, 0x20),
            code_bg: Color32::from_rgb(0x1A, 0x1A, 0x1E),
        }
    }
}

/// Applique le thème au contexte : fond, texte, widgets, tailles.
pub fn apply(ctx: &egui::Context, theme: &str, font_size: f32) {
    let palette = palette(theme);
    let mut visuals = if theme == "light" { Visuals::light() } else { Visuals::dark() };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = palette.background;
    visuals.faint_bg_color = palette.code_bg;
    visuals.override_text_color = Some(palette.normal);
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke.color = palette.on_accent;
    visuals.hyperlink_color = palette.technical;
    visuals.widgets.hovered.bg_stroke.color = palette.accent;
    visuals.widgets.active.bg_stroke.color = palette.accent;
    ctx.set_visuals(visuals);

    // L'interface suit la taille du texte, un peu plus petite : les réglages
    // se lisent, le texte se lit d'abord.
    let ui_size = (font_size * 0.72).clamp(13.0, 20.0);
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Body, FontId::proportional(ui_size));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(ui_size));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(ui_size * 1.3));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(ui_size * 0.85));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(ui_size * 0.9));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}
```

- [ ] **Step 6 : `src/gui/window.rs`, première version**

```rust
//! La fenêtre : une trame est un tour du contrôleur, puis les zones.

use std::time::Duration;

use anyhow::{anyhow, Result};
use eframe::egui::{self, CentralPanel, TopBottomPanel, ViewportBuilder};

use super::{fonts, theme};
use crate::app::App;

pub struct Gui {
    pub(super) app: App,
    /// Police, thème et taille appliqués au contexte : on ne refait que ce
    /// qui change.
    applied: Option<(String, String, f32)>,
}

pub fn run(app: App) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("sillon")
            .with_app_id("org.koumalabs.sillon")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([640.0, 420.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "sillon",
        options,
        Box::new(move |_cc| Ok(Box::new(Gui::new(app)))),
    )
    .map_err(|err| anyhow!("fenêtre : {err}"))
}

impl Gui {
    fn new(app: App) -> Self {
        Self { app, applied: None }
    }

    /// Met le contexte au diapason de la configuration quand elle change.
    fn refresh_theme(&mut self, ctx: &egui::Context) {
        let gui = &self.app.config.gui;
        let wanted = (gui.font.clone(), gui.theme.clone(), gui.font_size());
        if self.applied.as_ref() == Some(&wanted) {
            return;
        }
        if self.applied.as_ref().map(|a| &a.0) != Some(&wanted.0) {
            fonts::install(ctx, &wanted.0);
        }
        theme::apply(ctx, &wanted.1, wanted.2);
        self.applied = Some(wanted);
    }
}

impl eframe::App for Gui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.refresh_theme(&ctx);
        self.app.tick();
        if self.app.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let palette = theme::palette(&self.app.config.gui.theme);

        TopBottomPanel::top("header").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.app.doc().title).strong().color(palette.bright));
                ui.label(egui::RichText::new(&self.app.source).color(palette.dim));
            });
        });
        CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.background).inner_margin(16.0))
            .show_inside(ui, |ui| {
                ui.label("fenêtre en construction");
            });

        // Le mot suivi doit rester fluide : trente trames par seconde en
        // lecture, moins au repos — juste assez pour voir passer un message.
        ctx.request_repaint_after(if self.app.player.is_playing() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        });
    }

    fn on_exit(&mut self) {
        self.app.remember_position();
    }
}
```

- [ ] **Step 7 : `src/bin/sillon-gui.rs`**

```rust
//! `sillon-gui` — le lecteur dans une fenêtre.

use anyhow::Result;
use clap::Parser;

use sillon::launch::{self, Args, Launch, Prepared};

fn main() -> Result<()> {
    let args = Args::parse();
    let Launch::Ready(prepared) = launch::prepare(&args)? else {
        return Ok(());
    };
    let Prepared { mut app, config_requested, has_input: _, quiet } = prepared;
    // Sans document, la fenêtre s'ouvre sur « Ouvrir… », pas sur les réglages :
    // régler avant d'avoir quelque chose à lire n'est pas son geste naturel.
    if config_requested {
        app.open_config();
    }
    let outcome = sillon::gui::run(app);
    drop(quiet);
    outcome
}
```

- [ ] **Step 8 : Compiler, lancer, regarder**

```bash
cargo build --features gui 2>&1 | grep -E "^(error|warning)" -A 6 | head -60
cargo test 2>&1 | grep -E "^test result"        # sans la feature : rien ne change
SP=/private/tmp/claude-501/-Users-tharsan-Workspace-reader/9a34b428-c627-4190-a132-6b93bdd1f7be/scratchpad
./target/debug/sillon-gui dist/demo.md --engine system --no-autoplay & PID=$!
sleep 4; screencapture -x $SP/gui-07.png; kill $PID
```

Lire `$SP/gui-07.png` : une fenêtre sombre, titre du document en clair dans l'en-tête, texte « fenêtre en construction ».

- [ ] **Step 9 : Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/gui src/bin assets/fonts
git commit -m "feat(gui): la fenêtre s'ouvre

Feature gui, binaire sillon-gui, polices Atkinson Hyperlegible et
OpenDyslexic embarquées (OFL), palette du terminal en deux thèmes."
```

---

### Task 8 : `gui::text` — un bloc mis en runs

**Files:**
- Create: `src/gui/text.rs`

**Interfaces:**
- Produces: `text::Tone::{Cursor, Sentence, Block, Faded}` ; `text::Mark { tone, emphasis: Emphasis, hit: bool }` ; `text::Run { text: String, gap: usize, newline: bool, mark: Mark, sentence: Option<usize> }` ; `text::Cursor { sentence: usize, word: Option<usize> }` ; `text::runs(doc, block, cursor, search) -> Vec<Run>` ; `text::cells(line) -> Vec<(usize, String)>`.

- [ ] **Step 1 : Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::builder::{parse_document, CodePolicy};

    const DOC: &str = "Première phrase ici. Encore une.\n\nDeuxième bloc là.\n";

    fn doc() -> Document {
        parse_document(DOC, CodePolicy::Announce)
    }

    fn at(sentence: usize, word: Option<usize>) -> Cursor {
        Cursor { sentence, word }
    }

    #[test]
    fn the_spoken_word_is_the_only_one_with_the_cursor_tone() {
        let doc = doc();
        let runs = runs(&doc, &doc.blocks[0], at(0, Some(1)), None);
        let cursors: Vec<&Run> = runs.iter().filter(|r| r.mark.tone == Tone::Cursor).collect();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].text, "phrase");
        assert!(runs.iter().filter(|r| r.sentence == Some(0) && r.mark.tone != Tone::Cursor)
            .all(|r| r.mark.tone == Tone::Sentence));
    }

    #[test]
    fn the_rest_of_the_block_and_of_the_document_step_back() {
        let doc = doc();
        let first = runs(&doc, &doc.blocks[0], at(0, Some(0)), None);
        // « Encore une. » est dans le bloc lu, mais pas dans la phrase lue.
        assert!(first.iter().filter(|r| r.sentence == Some(1)).all(|r| r.mark.tone == Tone::Block));
        let second = runs(&doc, &doc.blocks[1], at(0, Some(0)), None);
        assert!(second.iter().all(|r| r.mark.tone == Tone::Faded));
    }

    #[test]
    fn a_search_hit_is_marked_except_on_the_spoken_word() {
        let doc = doc();
        let search = crate::app::search::Search::new(&doc, "phrase", 0);
        let runs = runs(&doc, &doc.blocks[0], at(0, Some(1)), Some(&search));
        let spoken = runs.iter().find(|r| r.text == "phrase").unwrap();
        assert!(!spoken.mark.hit, "le fond plein suffit au mot prononcé");
        let other = runs(&doc, &doc.blocks[0], at(0, Some(0)), Some(&search));
        assert!(other.iter().find(|r| r.text == "phrase").unwrap().mark.hit);
    }

    #[test]
    fn tokens_follow_the_lines_and_the_gaps_of_the_file() {
        let doc = parse_document("un   deux\ntrois quatre\n", CodePolicy::Announce);
        let block = &doc.blocks[0];
        assert_eq!(block.display_lines.len(), 2, "le document garde ses lignes");
        let runs = runs(&doc, block, at(0, None), None);
        let texts: Vec<(&str, usize, bool)> =
            runs.iter().map(|r| (r.text.as_str(), r.gap, r.newline)).collect();
        assert_eq!(
            texts,
            vec![("un", 0, true), ("deux", 3, false), ("trois", 0, true), ("quatre", 1, false)]
        );
    }

    #[test]
    fn every_token_knows_its_sentence() {
        let doc = doc();
        let runs = runs(&doc, &doc.blocks[0], at(5, None), None);
        assert!(runs.iter().all(|r| r.sentence.is_some()));
        assert_eq!(runs.last().unwrap().sentence, Some(1));
    }

    #[test]
    fn cells_count_the_gap_before_each_word() {
        assert_eq!(
            cells("  a   b c"),
            vec![(2, "a".to_string()), (3, "b".to_string()), (1, "c".to_string())]
        );
        assert!(cells("   ").is_empty());
    }
}
```

Run: `cargo test --lib gui::text 2>&1 | grep -E "error\[|^test result" | head -3`
Expected: erreur, module vide.

- [ ] **Step 2 : Implémenter**

```rust
//! Mise en runs d'un bloc : ce que chaque jeton du fichier porte à l'écran.
//!
//! Le pendant de `tui::ui::document_lines`, sans type de bibliothèque : la
//! fenêtre traduit ces runs en sections de texte, et c'est ici que se vérifie
//! qui porte le fond, la couleur, le soulignement.
//!
//! La composition vient du document — mêmes lignes, mêmes écarts ; le style
//! vient des mots, consommés dans l'ordre. Un jeton de plus que de mots garde
//! le ton du bloc.

use crate::app::search::Search;
use crate::doc::{Block, Document, Emphasis};

/// Couleur d'un jeton, par proximité avec la voix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Le mot prononcé : fond accent, texte sombre. Le seul à porter un fond.
    Cursor,
    /// La phrase en cours : la plus claire.
    Sentence,
    /// Le bloc lu, hors de la phrase en cours.
    Block,
    /// Le reste du document, atténué sans disparaître.
    Faded,
}

/// Ce qu'un jeton porte, hors de son texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub tone: Tone,
    /// Ce que le fichier dit : gras, italique, barré, lien, code.
    pub emphasis: Emphasis,
    /// Résultat de la recherche en cours.
    pub hit: bool,
}

/// Un jeton du fichier, prêt à être composé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    /// Espaces qui le précèdent sur sa ligne, tels que le fichier les écrit.
    pub gap: usize,
    /// Vrai s'il ouvre une ligne du fichier.
    pub newline: bool,
    pub mark: Mark,
    /// Phrase dont il fait partie, pour qu'un clic y envoie la voix.
    pub sentence: Option<usize>,
}

/// Où en est la voix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub sentence: usize,
    pub word: Option<usize>,
}

/// Les jetons d'un bloc de prose, dans l'ordre du fichier.
pub fn runs(doc: &Document, block: &Block, cursor: Cursor, search: Option<&Search>) -> Vec<Run> {
    let current_block = doc.sentence(cursor.sentence).map(|s| s.block_id) == Some(block.id);

    let mut words = Vec::new();
    for id in block.sentences.0..block.sentences.1 {
        let Some(sentence) = doc.sentence(id) else { continue };
        let in_sentence = id == cursor.sentence;
        for (i, word) in sentence.words.iter().enumerate() {
            let is_cursor = in_sentence && cursor.word == Some(i);
            let tone = if is_cursor {
                Tone::Cursor
            } else if in_sentence {
                Tone::Sentence
            } else if current_block {
                Tone::Block
            } else {
                Tone::Faded
            };
            // Un résultat se souligne — le fond plein reste au mot prononcé,
            // qui ne doit jamais se confondre.
            let hit = !is_cursor && search.is_some_and(|s| s.matches_word(word));
            words.push((Mark { tone, emphasis: word.emphasis, hit }, id));
        }
    }
    let base = Mark {
        tone: if current_block { Tone::Block } else { Tone::Faded },
        emphasis: Emphasis::default(),
        hit: false,
    };

    let source: Vec<&str> = if block.display_lines.is_empty() {
        vec![block.text.as_str()]
    } else {
        block.display_lines.iter().map(String::as_str).collect()
    };

    let mut out = Vec::new();
    let mut next = words.into_iter();
    for line in source {
        let mut opens = true;
        for (gap, text) in cells(line) {
            let (mark, sentence) = match next.next() {
                Some((mark, id)) => (mark, Some(id)),
                None => (base, None),
            };
            out.push(Run { text, gap, newline: opens, mark, sentence });
            opens = false;
        }
        // Une ligne vide du fichier reste vide à l'écran : elle en compose la
        // respiration.
        if opens {
            out.push(Run { text: String::new(), gap: 0, newline: true, mark: base, sentence: None });
        }
    }
    out
}

/// Découpe une ligne en jetons, chacun précédé de son écart.
///
/// L'écart est compté plutôt qu'écrasé : c'est lui qui aligne une colonne.
pub fn cells(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut gap = 0usize;
    let mut word = String::new();
    for c in source.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                out.push((gap, std::mem::take(&mut word)));
                gap = 0;
            }
            gap += 1;
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push((gap, word));
    }
    out
}
```

Run: `cargo test --lib gui::text 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: PASS. Si `tokens_follow_the_lines…` échoue sur `display_lines.len()`, regarder comment `tui::ui::tests::a_paragraph_keeps_the_lines_of_the_file` construit son document et s'aligner dessus.

- [ ] **Step 3 : Commit**

```bash
git add src/gui/text.rs
git commit -m "feat(gui): mettre un bloc en runs, sans egui

Chaque jeton du fichier reçoit le ton de son mot : le prononcé seul porte
un fond, la phrase lue s'éclaircit, le reste s'atténue, un résultat se
souligne. Vérifiable sans fenêtre."
```

---

### Task 9 : `gui::flow` — l'arithmétique du défilement

**Files:**
- Create: `src/gui/flow.rs`

**Interfaces:**
- Produces: `flow::estimated_height(chars, chars_per_line, line_height, gap) -> f32` ; `flow::follow_target(row_top, viewport_height, content_height) -> f32` ; `flow::approach(current, target, viewport_height) -> f32` ; `flow::absorb(offset, block_top, old_height, new_height) -> f32` ; `flow::block_at(tops: &[f32], offset) -> usize` ; `flow::user_scrolled(set, read) -> bool`.

- [ ] **Step 1 : Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_never_laid_out_is_estimated_from_its_length() {
        // 100 caractères à 40 par ligne : trois lignes, plus l'écart.
        assert_eq!(estimated_height(100, 40.0, 30.0, 10.0), 100.0);
        // Un bloc vide tient quand même une ligne.
        assert_eq!(estimated_height(0, 40.0, 30.0, 10.0), 40.0);
    }

    #[test]
    fn the_spoken_line_is_held_at_the_upper_third() {
        assert_eq!(follow_target(600.0, 300.0, 10_000.0), 500.0);
        // Jamais avant le début, jamais au-delà de la fin.
        assert_eq!(follow_target(50.0, 300.0, 10_000.0), 0.0);
        assert_eq!(follow_target(9_950.0, 300.0, 10_000.0), 9_700.0);
    }

    #[test]
    fn the_view_glides_when_close_and_jumps_when_far() {
        assert_eq!(approach(0.0, 100.0, 600.0), 25.0);
        assert_eq!(approach(0.0, 5_000.0, 600.0), 5_000.0);
        assert_eq!(approach(99.8, 100.0, 600.0), 100.0);
    }

    #[test]
    fn a_block_above_the_view_that_changes_height_does_not_move_the_text() {
        // Le bloc 100..200 devient 100..260 : la vue à 500 descend de 60.
        assert_eq!(absorb(500.0, 100.0, 100.0, 160.0), 560.0);
        // Un bloc sous la vue ne la déplace pas.
        assert_eq!(absorb(500.0, 800.0, 100.0, 160.0), 500.0);
        // Et le décalage ne passe jamais sous zéro.
        assert_eq!(absorb(20.0, 0.0, 100.0, 50.0), 0.0);
    }

    #[test]
    fn the_block_at_an_offset_is_the_last_one_starting_above_it() {
        let tops = [0.0, 100.0, 250.0, 400.0];
        assert_eq!(block_at(&tops, 0.0), 0);
        assert_eq!(block_at(&tops, 120.0), 1);
        assert_eq!(block_at(&tops, 250.0), 2);
        assert_eq!(block_at(&tops, 9_000.0), 3);
        assert_eq!(block_at(&[], 10.0), 0);
    }

    #[test]
    fn a_pixel_of_rounding_is_not_a_scroll() {
        assert!(!user_scrolled(100.0, 100.6));
        assert!(user_scrolled(100.0, 140.0));
    }
}
```

- [ ] **Step 2 : Implémenter**

```rust
//! Arithmétique du défilement, sans egui.
//!
//! La fenêtre compose les blocs à la demande ; en attendant, chacun a une
//! hauteur estimée. Ce module décide où placer la vue, comment l'y amener, et
//! comment absorber les corrections sans que le texte bouge sous les yeux.

/// Hauteur estimée d'un bloc jamais composé.
pub fn estimated_height(chars: usize, chars_per_line: f32, line_height: f32, gap: f32) -> f32 {
    let per_line = chars_per_line.max(1.0);
    let lines = (chars as f32 / per_line).ceil().max(1.0);
    lines * line_height + gap
}

/// Décalage qui tient `row_top` au tiers supérieur de la fenêtre.
///
/// Au tiers, pas au centre : on voit ce qui vient plus que ce qui est passé.
pub fn follow_target(row_top: f32, viewport_height: f32, content_height: f32) -> f32 {
    let target = row_top - viewport_height / 3.0;
    target.clamp(0.0, (content_height - viewport_height).max(0.0))
}

/// Rapproche le décalage de sa cible : en douceur de près, d'un bond de loin.
///
/// Glisser sur un document entier après un saut au sommaire prendrait des
/// secondes ; un bond dit mieux qu'on a changé de page.
pub fn approach(current: f32, target: f32, viewport_height: f32) -> f32 {
    let delta = target - current;
    if delta.abs() < 0.5 || delta.abs() > viewport_height {
        return target;
    }
    current + delta * 0.25
}

/// Absorbe le changement de hauteur d'un bloc entièrement au-dessus de la vue.
///
/// Ce qui est à l'écran ne doit pas bouger quand un bloc hors champ prend sa
/// hauteur réelle.
pub fn absorb(offset: f32, block_top: f32, old_height: f32, new_height: f32) -> f32 {
    if block_top + old_height <= offset {
        (offset + new_height - old_height).max(0.0)
    } else {
        offset
    }
}

/// Bloc dont le haut est le dernier à ne pas dépasser `offset`.
pub fn block_at(tops: &[f32], offset: f32) -> usize {
    let after = tops.partition_point(|&top| top <= offset);
    after.saturating_sub(1)
}

/// Vrai quand le décalage relu diffère de celui qu'on avait posé : quelqu'un
/// a défilé. Un pixel d'arrondi n'est pas un geste.
pub fn user_scrolled(set: f32, read: f32) -> bool {
    (set - read).abs() > 1.0
}
```

Run: `cargo test --lib gui::flow 2>&1 | grep -E "^test result|FAILED"`
Expected: PASS.

- [ ] **Step 3 : Commit**

```bash
git add src/gui/flow.rs
git commit -m "feat(gui): l'arithmétique du défilement, vérifiée sans fenêtre"
```

---

### Task 10 : `gui::reader` — le texte à l'écran

**Files:**
- Create: `src/gui/reader.rs`
- Modify: `src/gui/mod.rs` (déclarer `reader`), `src/gui/window.rs` (le panneau central appelle `Reader::show`)

**Interfaces:**
- Consumes: `text::runs`, `flow::*`, `fonts::family`, `theme::Palette`, `App::{doc, current_sentence, current_word, search, view, seek_to, config}`.
- Produces: `reader::Reader::new()`, `Reader::show(&mut self, ui, app, &Palette)`, `Reader::page(&mut self, direction: i32)`, `Reader::reset(&mut self)`.

- [ ] **Step 1 : Le module**

```rust
//! La zone de texte : composition mise en cache, règle, suivi de la voix, clic.
//!
//! Chaque bloc garde sa composition tant que rien n'a changé pour lui. Un bloc
//! jamais vu a une hauteur estimée, prise sur son nombre de caractères ; il est
//! composé quand il approche de la fenêtre, et sa hauteur réelle remplace
//! l'estimation sans que ce qui est à l'écran bouge. Le coût d'une trame dépend
//! de la fenêtre, pas du document.

use std::ops::Range;
use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Galley, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};

use super::text::{self, Cursor, Mark, Run, Tone};
use super::theme::Palette;
use super::{flow, fonts};
use crate::app::App;
use crate::doc::{Block, BlockKind, Document};

/// Ce dont dépend la composition d'un bloc : quand ça change, on recompose.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    /// Largeur de colonne, style, thème, recherche : tout ce qui vaut pour le
    /// document entier, résumé par un compteur.
    generation: u64,
    current_block: bool,
    /// Phrase et mot lus, s'ils sont dans ce bloc.
    sentence: Option<usize>,
    word: Option<usize>,
    /// Nombre de phrases : un flux peut en ajouter au dernier bloc.
    sentences: usize,
}

struct Slot {
    key: Option<Key>,
    galley: Option<Arc<Galley>>,
    /// Puce ou rang d'un item de liste, composé à part pour que la suite du
    /// texte pende sous le texte et non sous la puce.
    marker: Option<Arc<Galley>>,
    height: f32,
    /// Plages de caractères → phrase, pour le clic.
    spans: Vec<(Range<usize>, usize)>,
    /// Plage du mot prononcé, en caractères, pour la règle et le suivi.
    cursor: Option<Range<usize>>,
}

impl Slot {
    fn estimated(height: f32) -> Self {
        Self { key: None, galley: None, marker: None, height, spans: Vec::new(), cursor: None }
    }
}

/// Mise en page d'une trame, dérivée de la configuration et de la fenêtre.
struct Metrics {
    column_x: f32,
    column_w: f32,
    size: f32,
    line_h: f32,
    /// Espace sous un bloc.
    gap: f32,
    font: String,
    ruler: bool,
}

/// Ce qu'une trame de composition rend au défilement.
struct Pass {
    tops: Vec<f32>,
    total: f32,
    /// Haut et hauteur de la ligne du mot prononcé, en coordonnées du contenu.
    spoken: Option<(f32, f32)>,
}

pub struct Reader {
    slots: Vec<Slot>,
    /// Résumé de ce qui vaut pour tous les blocs ; incrémenté quand ça change.
    generation: u64,
    fingerprint: Option<(i32, String, String, f32, f32, bool, Option<String>)>,
    /// Décalage de défilement tenu par la vue.
    offset: f32,
    /// Décalage posé à la dernière trame ; s'il diffère de celui qu'on relit,
    /// quelqu'un a défilé.
    set: Option<f32>,
    tops: Vec<f32>,
    total: f32,
    /// Pages demandées au clavier, en signe, à servir à la prochaine trame.
    page: i32,
}

/// Blocs composés au plus par trame au-delà de ceux qui sont visibles.
const BUDGET: usize = 24;

impl Reader {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            generation: 0,
            fingerprint: None,
            offset: 0.0,
            set: None,
            tops: Vec::new(),
            total: 0.0,
            page: 0,
        }
    }

    /// Oublie toute composition : un autre document arrive.
    pub fn reset(&mut self) {
        self.slots.clear();
        self.offset = 0.0;
        self.set = None;
        self.tops.clear();
        self.total = 0.0;
        self.page = 0;
    }

    /// Défile d'une page dans ce sens à la prochaine trame, et épingle la vue.
    pub fn page(&mut self, direction: i32) {
        self.page += direction;
    }

    pub fn show(&mut self, ui: &mut Ui, app: &mut App, palette: &Palette) {
        if app.doc().blocks.is_empty() {
            let message = if app.input_complete() {
                "Aucun contenu lisible dans l'entrée."
            } else {
                "En attente du flux…"
            };
            ui.add_space(24.0);
            ui.label(egui::RichText::new(message).color(palette.dim));
            return;
        }

        let metrics = self.metrics(ui, app);
        self.bump_generation(&metrics, app, palette);
        self.fit_slots(app.doc(), &metrics);

        let viewport_h = ui.available_height();
        if self.page != 0 {
            let max = (self.total - viewport_h).max(0.0);
            self.offset = (self.offset + self.page as f32 * viewport_h * 0.9).clamp(0.0, max);
            app.view.pin(flow::block_at(&self.tops, self.offset));
            self.page = 0;
            self.set = Some(self.offset);
        }
        let following = app.view.is_following();
        let mut area = egui::ScrollArea::vertical()
            .id_salt("lecture")
            .auto_shrink([false, false]);
        if following || self.set.is_some() {
            area = area.vertical_scroll_offset(self.offset);
            self.set = Some(self.offset);
        }

        let cursor = Cursor { sentence: app.current_sentence(), word: app.current_word() };
        let search = app.search().cloned();
        let mut clicked: Option<usize> = None;
        let output = area.show_viewport(ui, |ui, viewport| {
            let pass = self.compose(ui, app.doc(), cursor, search.as_ref(), &metrics, palette, viewport, following);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), pass.total), Sense::click());
            self.paint(ui, app.doc(), &pass, &metrics, palette, rect, viewport);
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    clicked = self.sentence_at(app.doc(), &pass, &metrics, rect, pos);
                }
            }
            pass
        });
        let pass = output.inner;
        self.tops = pass.tops;
        self.total = pass.total;

        // Quelqu'un a défilé : la vue s'épingle sur le bloc en haut de l'écran.
        let read = output.state.offset.y;
        if let Some(set) = self.set {
            if flow::user_scrolled(set, read) {
                app.view.pin(flow::block_at(&self.tops, read));
                self.offset = read;
            }
        }
        if app.view.is_following() {
            if let Some((row_top, _)) = pass.spoken {
                let target = flow::follow_target(row_top, viewport_h, self.total);
                self.offset = flow::approach(self.offset, target, viewport_h);
            }
        } else {
            self.offset = read;
            self.set = None;
        }
        if let Some(sentence) = clicked {
            app.seek_to(sentence);
        }
        if self.slots.iter().any(|s| s.galley.is_none()) {
            ui.ctx().request_repaint();
        }
    }
```

Les méthodes privées :

```rust
    fn metrics(&self, ui: &Ui, app: &App) -> Metrics {
        let gui = &app.config.gui;
        let size = gui.font_size();
        let line_h = size * gui.line_height();
        let margin = size;
        let usable = (ui.available_width() - 2.0 * margin - 16.0).max(80.0);
        let column_w = match app.config.reading.line_width {
            0 => usable,
            chars => usable.min(chars as f32 * size * 0.5),
        };
        Metrics {
            column_x: margin + (usable - column_w) / 2.0,
            column_w,
            size,
            line_h,
            gap: line_h * 0.7,
            font: gui.font.clone(),
            ruler: app.config.reading.reading_ruler,
        }
    }

    /// Change de génération quand ce qui vaut pour tous les blocs a changé.
    fn bump_generation(&mut self, metrics: &Metrics, app: &App, palette: &Palette) {
        let fingerprint = (
            metrics.column_w.round() as i32,
            metrics.font.clone(),
            app.config.gui.theme.clone(),
            metrics.size,
            metrics.line_h,
            metrics.ruler,
            app.search().map(|s| s.query.clone()),
        );
        let _ = palette;
        if self.fingerprint.as_ref() != Some(&fingerprint) {
            self.generation += 1;
            self.fingerprint = Some(fingerprint);
        }
    }

    /// Autant d'emplacements que de blocs : un flux en ajoute, un autre
    /// document en retire.
    fn fit_slots(&mut self, doc: &Document, metrics: &Metrics) {
        self.slots.truncate(doc.blocks.len());
        let chars_per_line = metrics.column_w / (metrics.size * 0.5);
        for block in &doc.blocks[self.slots.len()..] {
            let chars = block.text.chars().count();
            self.slots.push(Slot::estimated(flow::estimated_height(
                chars,
                chars_per_line,
                metrics.line_h,
                metrics.gap,
            )));
        }
    }

    /// Compose ce qui approche de la fenêtre, avec un budget, et rend les
    /// positions de tous les blocs.
    #[allow(clippy::too_many_arguments)]
    fn compose(
        &mut self,
        ui: &Ui,
        doc: &Document,
        cursor: Cursor,
        search: Option<&crate::app::search::Search>,
        metrics: &Metrics,
        palette: &Palette,
        viewport: Rect,
        following: bool,
    ) -> Pass {
        let current_block = doc.sentence(cursor.sentence).map(|s| s.block_id);
        let near = viewport.expand2(Vec2::new(0.0, viewport.height()));
        let mut budget = BUDGET;
        let mut tops = Vec::with_capacity(doc.blocks.len());
        let mut y = 0.0;
        let mut spoken = None;

        for (i, block) in doc.blocks.iter().enumerate() {
            tops.push(y);
            let old = self.slots[i].height;
            let span = Rect::from_min_size(Pos2::new(0.0, y), Vec2::new(1.0, old));
            let is_current = current_block == Some(block.id);
            let key = Key {
                generation: self.generation,
                current_block: is_current,
                sentence: is_current.then_some(cursor.sentence),
                word: if is_current { cursor.word } else { None },
                sentences: block.sentences.1 - block.sentences.0,
            };
            let stale = self.slots[i].key.as_ref() != Some(&key);
            let visible = span.intersects(viewport);
            if stale && (visible || (span.intersects(near) && budget > 0)) {
                let glued = glued_to_next(doc, i);
                let slot = layout_block(ui, doc, block, cursor, search, metrics, palette, is_current, glued);
                self.slots[i] = Slot { key: Some(key), ..slot };
                if !visible {
                    budget -= 1;
                }
                // Un bloc au-dessus de la vue qui change de hauteur ne doit
                // pas déplacer ce qu'on lit.
                if !following {
                    self.offset = flow::absorb(self.offset, y, old, self.slots[i].height);
                }
            }
            if is_current {
                if let (Some(galley), Some(range)) = (&self.slots[i].galley, &self.slots[i].cursor) {
                    if let Some(rect) = row_of(galley, range.start) {
                        spoken = Some((y + rect.min.y, rect.height()));
                    }
                }
            }
            y += self.slots[i].height;
        }
        Pass { tops, total: y, spoken }
    }

    fn paint(&self, ui: &Ui, doc: &Document, pass: &Pass, metrics: &Metrics, palette: &Palette, rect: Rect, viewport: Rect) {
        let painter = ui.painter();
        let origin = rect.min;
        for (i, block) in doc.blocks.iter().enumerate() {
            let slot = &self.slots[i];
            let top = pass.tops[i];
            if top + slot.height < viewport.min.y || top > viewport.max.y {
                continue;
            }
            let Some(galley) = &slot.galley else { continue };
            let y = origin.y + top;
            let x0 = origin.x + metrics.column_x;
            let (indent, marker_w) = prefix_widths(block, slot, metrics);
            let x_text = x0 + indent + marker_w;
            let fixed = matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. });

            if fixed {
                let glued = glued_to_next(doc, i);
                let h = slot.height - if glued { 0.0 } else { metrics.gap };
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x0, y), Vec2::new(metrics.column_w, h)),
                    2.0,
                    palette.code_bg,
                );
                if matches!(block.kind, BlockKind::Code { .. }) {
                    let current = slot.key.as_ref().is_some_and(|k| k.current_block);
                    painter.vline(
                        x0 + 3.0,
                        y..=y + h,
                        Stroke::new(2.0, if current { palette.accent } else { palette.faint }),
                    );
                }
            } else if metrics.ruler {
                // La règle passe sous la ligne entière : c'est elle qu'on
                // retrouve en relevant les yeux, pas le mot seul.
                if let Some(range) = &slot.cursor {
                    if let Some(row) = row_of(galley, range.start) {
                        painter.rect_filled(
                            Rect::from_min_max(
                                Pos2::new(x0 - metrics.size * 0.4, y + row.min.y - 2.0),
                                Pos2::new(x0 + metrics.column_w + metrics.size * 0.4, y + row.max.y + 2.0),
                            ),
                            4.0,
                            palette.ruler,
                        );
                    }
                }
            }

            if matches!(block.kind, BlockKind::Quote) {
                let h = galley.size().y;
                painter.vline(x0 + indent * 0.35, y..=y + h, Stroke::new(3.0, palette.faint));
            }
            if let Some(marker) = &slot.marker {
                painter.galley(Pos2::new(x0 + indent, y), marker.clone(), palette.accent);
            }
            painter.galley(Pos2::new(x_text, y), galley.clone(), palette.normal);

            // Un titre est souligné d'un filet : la hiérarchie se lit d'un
            // coup d'œil sans qu'aucun marqueur ne soit prononcé.
            if let BlockKind::Heading { level } = block.kind {
                if level <= 2 {
                    let chars = if level == 1 { 48.0 } else { 28.0 };
                    let w = metrics.column_w.min(chars * metrics.size * 0.5);
                    let rule_y = y + galley.size().y + 4.0;
                    painter.hline(
                        x_text..=x_text + w,
                        rule_y,
                        Stroke::new(1.5, if level == 1 { palette.accent } else { palette.faint }),
                    );
                }
            }
        }
    }

    /// La phrase sous le pointeur, s'il est sur un mot.
    fn sentence_at(&self, doc: &Document, pass: &Pass, metrics: &Metrics, rect: Rect, pos: Pos2) -> Option<usize> {
        let y = pos.y - rect.min.y;
        let i = flow::block_at(&pass.tops, y);
        let block = doc.blocks.get(i)?;
        let slot = self.slots.get(i)?;
        let galley = slot.galley.as_ref()?;
        if matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. }) {
            return Some(block.sentences.0);
        }
        let (indent, marker_w) = prefix_widths(block, slot, metrics);
        let local = Vec2::new(
            pos.x - (rect.min.x + metrics.column_x + indent + marker_w),
            y - pass.tops[i],
        );
        let index = galley.cursor_from_pos(local).index;
        slot.spans
            .iter()
            .find(|(range, _)| range.contains(&index))
            .or_else(|| slot.spans.iter().rev().find(|(range, _)| range.start <= index))
            .map(|(_, sentence)| *sentence)
    }
}
```

Les fonctions libres :

```rust
/// Deux lignes d'un même tableau se tiennent : pas d'écart entre elles.
fn glued_to_next(doc: &Document, i: usize) -> bool {
    matches!(doc.blocks[i].kind, BlockKind::TableRow { .. })
        && doc.blocks.get(i + 1).is_some_and(|n| matches!(n.kind, BlockKind::TableRow { .. }))
}

/// Retrait et largeur de la marque d'un bloc, en pixels.
fn prefix_widths(block: &Block, slot: &Slot, metrics: &Metrics) -> (f32, f32) {
    let indent = match block.kind {
        BlockKind::ListItem { depth, .. } => metrics.size * (1.0 + 1.2 * depth as f32),
        BlockKind::Quote => metrics.size * 1.0,
        BlockKind::Code { .. } | BlockKind::TableRow { .. } => metrics.size * 0.8,
        _ => 0.0,
    };
    let marker_w = slot.marker.as_ref().map_or(0.0, |m| m.size().x + metrics.size * 0.4);
    (indent, marker_w)
}

/// Rangée du texte composé qui contient le caractère `index`.
fn row_of(galley: &Galley, index: usize) -> Option<Rect> {
    let mut seen = 0usize;
    for placed in &galley.rows {
        let count = placed.row.char_count_including_newline();
        if index < seen + count {
            return Some(placed.rect());
        }
        seen += count;
    }
    galley.rows.last().map(|r| r.rect())
}

/// Compose un bloc : texte, marque, plages, hauteur.
#[allow(clippy::too_many_arguments)]
fn layout_block(
    ui: &Ui,
    doc: &Document,
    block: &Block,
    cursor: Cursor,
    search: Option<&crate::app::search::Search>,
    metrics: &Metrics,
    palette: &Palette,
    current: bool,
    glued: bool,
) -> Slot {
    let gap = if glued { 0.0 } else { metrics.gap };
    if matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. }) {
        return layout_fixed(ui, block, metrics, palette, current, gap);
    }

    let marker = match &block.kind {
        BlockKind::ListItem { ordinal, .. } => {
            let text = match ordinal {
                Some(n) => format!("{n}."),
                None => "•".to_string(),
            };
            let color = if current { palette.accent } else { palette.faint };
            Some(ui.painter().layout_no_wrap(
                text,
                FontId::new(metrics.size, fonts::family(&metrics.font, false, false)),
                color,
            ))
        }
        _ => None,
    };
    let indent = match block.kind {
        BlockKind::ListItem { depth, .. } => metrics.size * (1.0 + 1.2 * depth as f32),
        BlockKind::Quote => metrics.size,
        _ => 0.0,
    };
    let marker_w = marker.as_ref().map_or(0.0, |m| m.size().x + metrics.size * 0.4);
    let width = (metrics.column_w - indent - marker_w).max(40.0);

    let runs = text::runs(doc, block, cursor, search);
    let (job, spans, cursor_range) = job_for(&runs, true, width, &block.kind, metrics, palette);
    let mut galley = ui.painter().layout_job(job);
    let mut spans = spans;
    let mut cursor_range = cursor_range;
    // La page reste celle du fichier tant qu'elle tient : si une ligne a dû
    // se replier, le bloc est recomposé d'un seul tenant.
    let file_lines = runs.iter().filter(|r| r.newline).count().max(1);
    if galley.rows.len() > file_lines {
        let (job, s, c) = job_for(&runs, false, width, &block.kind, metrics, palette);
        galley = ui.painter().layout_job(job);
        spans = s;
        cursor_range = c;
    }

    let rule = match block.kind {
        BlockKind::Heading { level } if level <= 2 => 8.0,
        _ => 0.0,
    };
    let height = galley.size().y.max(marker.as_ref().map_or(0.0, |m| m.size().y)) + rule + gap;
    Slot { key: None, galley: Some(galley), marker, height, spans, cursor: cursor_range }
}

/// Un bloc montré tel qu'il est écrit : code, tableau.
fn layout_fixed(ui: &Ui, block: &Block, metrics: &Metrics, palette: &Palette, current: bool, gap: f32) -> Slot {
    let size = metrics.size * 0.85;
    let color = match block.kind {
        BlockKind::TableRow { header: true } if current => palette.bright,
        BlockKind::TableRow { header: true } => palette.normal,
        _ if current => palette.normal,
        _ => palette.faint,
    };
    let mut format = TextFormat::simple(FontId::monospace(size), color);
    format.line_height = Some(size * 1.4);
    let text = if block.display_lines.is_empty() {
        block.text.clone()
    } else {
        block.display_lines.join("\n")
    };
    let mut job = LayoutJob::single_section(text, format);
    job.wrap.max_width = (metrics.column_w - metrics.size * 1.2).max(40.0);
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y + metrics.size * 0.4 + gap;
    Slot { key: None, galley: Some(galley), marker: None, height, spans: Vec::new(), cursor: None }
}

/// Traduit les runs en sections de texte. Rend aussi les plages de caractères
/// de chaque phrase et celle du mot prononcé.
fn job_for(
    runs: &[Run],
    keep_lines: bool,
    width: f32,
    kind: &BlockKind,
    metrics: &Metrics,
    palette: &Palette,
) -> (LayoutJob, Vec<(Range<usize>, usize)>, Option<Range<usize>>) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    job.break_on_newline = true;
    let plain = format_for(
        &Mark { tone: Tone::Faded, emphasis: Default::default(), hit: false },
        kind,
        metrics,
        palette,
    );
    let mut spans = Vec::new();
    let mut cursor = None;
    let mut chars = 0usize;
    let mut first = true;
    for run in runs {
        let spacer = if first {
            if keep_lines { " ".repeat(run.gap) } else { String::new() }
        } else if run.newline {
            if keep_lines { format!("\n{}", " ".repeat(run.gap)) } else { " ".to_string() }
        } else {
            " ".repeat(run.gap.max(1))
        };
        first = false;
        if !spacer.is_empty() {
            job.append(&spacer, 0.0, plain.clone());
            chars += spacer.chars().count();
        }
        if run.text.is_empty() {
            continue;
        }
        let len = run.text.chars().count();
        job.append(&run.text, 0.0, format_for(&run.mark, kind, metrics, palette));
        if let Some(sentence) = run.sentence {
            spans.push((chars..chars + len, sentence));
        }
        if run.mark.tone == Tone::Cursor {
            cursor = Some(chars..chars + len);
        }
        chars += len;
    }
    (job, spans, cursor)
}

/// Le format d'un jeton : police, taille, couleur, fond, traits.
///
/// Jamais de changement de graisse pour le mot prononcé ni la phrase lue : dans
/// une police proportionnelle, la ligne entière se recomposerait à chaque mot.
fn format_for(mark: &Mark, kind: &BlockKind, metrics: &Metrics, palette: &Palette) -> TextFormat {
    let (size, heading) = match kind {
        BlockKind::Heading { level: 1 } => (metrics.size * 1.6, true),
        BlockKind::Heading { level: 2 } => (metrics.size * 1.35, true),
        BlockKind::Heading { .. } => (metrics.size * 1.15, true),
        _ => (metrics.size, false),
    };
    let strong = mark.emphasis.strong || heading;
    let italic = mark.emphasis.italic || matches!(kind, BlockKind::Quote);
    let family = if mark.emphasis.code {
        FontFamily::Monospace
    } else {
        fonts::family(&metrics.font, strong, italic)
    };
    let mut color = match mark.tone {
        Tone::Cursor => palette.on_accent,
        Tone::Sentence => palette.bright,
        Tone::Block => match kind {
            BlockKind::Heading { level: 1 } => palette.accent,
            BlockKind::Heading { .. } => palette.bright,
            BlockKind::Quote => palette.dim,
            _ => palette.normal,
        },
        Tone::Faded => palette.faint,
    };
    if matches!(mark.tone, Tone::Sentence | Tone::Block) && (mark.emphasis.link || mark.emphasis.code) {
        color = palette.technical;
    }
    let line = Stroke::new(1.0, color);
    TextFormat {
        font_id: FontId::new(size, family),
        line_height: Some(size * (metrics.line_h / metrics.size)),
        color,
        background: if mark.tone == Tone::Cursor { palette.accent } else { Color32::TRANSPARENT },
        expand_bg: 2.0,
        italics: italic && !fonts::has_italic(&metrics.font),
        underline: if mark.emphasis.link || mark.hit { line } else { Stroke::NONE },
        strikethrough: if mark.emphasis.struck { line } else { Stroke::NONE },
        valign: Align::BOTTOM,
        ..Default::default()
    }
}
```

Déclarer `#[cfg(feature = "gui")] mod reader;` dans `src/gui/mod.rs`. Dans `window.rs`, `Gui` gagne `reader: reader::Reader` et `last_source: String` ; le panneau central appelle `self.reader.show(ui, &mut self.app, &palette)` ; avant, si `self.app.source != self.last_source`, `self.reader.reset()` et `self.last_source = self.app.source.clone()`.

- [ ] **Step 2 : Compiler, lancer, regarder**

```bash
cargo build --features gui 2>&1 | grep -E "^(error|warning)" -A 8 | head -80
SP=/private/tmp/claude-501/-Users-tharsan-Workspace-reader/9a34b428-c627-4190-a132-6b93bdd1f7be/scratchpad
osascript -e 'output volume of (get volume settings)' > $SP/volume.txt
osascript -e 'set volume output volume 0'
./target/debug/sillon-gui dist/demo.md --engine system & PID=$!
sleep 6; screencapture -x $SP/gui-10.png; kill $PID
osascript -e "set volume output volume $(cat $SP/volume.txt)"
```

Lire la capture : texte en Atkinson, titre en accent avec filet, mot en fond orange, bandeau de règle sous sa ligne, listes avec puces en colonne, code sur fond léger.

- [ ] **Step 3 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): le texte à l'écran

Composition par bloc, mise en cache et budgétée ; le mot prononcé ne
change que de fond, la règle passe derrière sa ligne, la vue suit la
voix au tiers et se fige au premier coup de molette ; un clic sur un
mot y envoie la voix."
```

---

### Task 11 : `gui::keys` — le clavier de la fenêtre

**Files:**
- Create: `src/gui/keys.rs`
- Modify: `src/gui/mod.rs`, `src/gui/window.rs` (appel de `dispatch`, service des demandes)

**Interfaces:**
- Produces: `keys::Requests { page: i32, open_file: bool, paste: bool, focus_search: bool }` ; `keys::dispatch(ctx, app) -> Requests`.

- [ ] **Step 1 : Implémenter**

```rust
//! Adaptation des événements egui aux touches neutres du contrôleur.
//!
//! Les lettres arrivent en texte, avec leur casse — c'est ce que le
//! contrôleur attend ; les touches spéciales arrivent en touches. Ce qu'une
//! fenêtre fait autrement qu'un terminal — défiler d'une page en pixels,
//! ouvrir un fichier, coller, donner le clavier au champ de recherche — remonte
//! en demandes que la fenêtre sert elle-même.

use eframe::egui::{self, Event, Key as EguiKey};

use crate::app::keys::{self, Key, KeyCode};
use crate::app::App;

/// Ce que la trame demande à la fenêtre, en plus du contrôleur.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    /// Pages à défiler, en signe.
    pub page: i32,
    pub open_file: bool,
    pub paste: bool,
    pub focus_search: bool,
}

/// Distribue les touches de la trame, sauf quand un champ de saisie a le clavier.
pub fn dispatch(ctx: &egui::Context, app: &mut App) -> Requests {
    let mut requests = Requests::default();
    // Un champ egui a le clavier : ce qu'on tape lui revient, tout entier.
    // C'est egui qui le dit, pas une liste de touches à exclure.
    if ctx.memory(|m| m.focused().is_some()) {
        return requests;
    }
    let events = ctx.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::Paste(_) => requests.paste = true,
            Event::Key { key, pressed: true, modifiers, .. } => {
                if modifiers.command {
                    match key {
                        EguiKey::O => requests.open_file = true,
                        EguiKey::V => requests.paste = true,
                        EguiKey::Q | EguiKey::W => app.should_quit = true,
                        _ => {}
                    }
                    continue;
                }
                match key {
                    EguiKey::PageUp => requests.page -= 1,
                    EguiKey::PageDown => requests.page += 1,
                    EguiKey::Tab if modifiers.shift => {
                        keys::handle(app, Key::plain(KeyCode::BackTab));
                    }
                    _ => {
                        if let Some(code) = special(key) {
                            keys::handle(
                                app,
                                Key { code, shift: modifiers.shift, ctrl: modifiers.ctrl },
                            );
                        } else if modifiers.ctrl {
                            if let Some(c) = letter(key) {
                                keys::handle(app, Key { code: KeyCode::Char(c), shift: false, ctrl: true });
                            }
                        }
                    }
                }
            }
            Event::Text(text) => {
                for c in text.chars() {
                    match c {
                        // L'espace est déjà passé en touche.
                        ' ' => {}
                        // Le champ de recherche prend le clavier lui-même.
                        '/' => requests.focus_search = true,
                        _ => keys::handle(
                            app,
                            Key { code: KeyCode::Char(c), shift: c.is_uppercase(), ctrl: false },
                        ),
                    }
                }
            }
            _ => {}
        }
    }
    requests
}

fn special(key: EguiKey) -> Option<KeyCode> {
    Some(match key {
        EguiKey::ArrowLeft => KeyCode::Left,
        EguiKey::ArrowRight => KeyCode::Right,
        EguiKey::ArrowUp => KeyCode::Up,
        EguiKey::ArrowDown => KeyCode::Down,
        EguiKey::Enter => KeyCode::Enter,
        EguiKey::Escape => KeyCode::Esc,
        EguiKey::Backspace => KeyCode::Backspace,
        EguiKey::Tab => KeyCode::Tab,
        EguiKey::Home => KeyCode::Home,
        EguiKey::End => KeyCode::End,
        EguiKey::Delete => KeyCode::Delete,
        EguiKey::Space => KeyCode::Char(' '),
        _ => return None,
    })
}

/// La lettre d'une touche, pour les raccourcis avec Ctrl.
fn letter(key: EguiKey) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}
```

Dans `window.rs`, après `self.app.tick()` :

```rust
        let requests = keys::dispatch(&ctx, &mut self.app);
        if requests.page != 0 {
            self.reader.page(requests.page);
        }
        if requests.focus_search {
            self.focus_search = true;
        }
        // open_file et paste : servis par la Task 15.
```

`Gui` gagne `pub(super) focus_search: bool` et `pub(super) search_text: String`.

- [ ] **Step 2 : Vérifier à la main**

Lancer `./target/debug/sillon-gui dist/demo.md --engine system --no-autoplay`, appuyer sur `→` trois fois, `n`, `Pg↓`, `f`, `?`, `Échap`, `q` : la lecture avance, le titre suivant s'affiche, la vue se fige puis revient, l'aide s'ouvre et se ferme, la fenêtre se ferme.

- [ ] **Step 3 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): le clavier de la fenêtre parle au même contrôleur"
```

---

### Task 12 : En-tête, recherche, barre de lecture, ligne d'état

**Files:**
- Create: `src/gui/header.rs`, `src/gui/player.rs`
- Modify: `src/gui/mod.rs`, `src/gui/window.rs`

**Interfaces:**
- Produces: `header::show(ui, gui: &mut Gui, palette) -> header::Actions { open_file: bool, paste: bool }` ; `player::show(ui, app, palette)` ; `player::status(ui, app, palette, opening: Option<&str>)`.
- Consumes: `App::{search_with, search_advance, search, toggle_panel, toggle_config, toggle_play, seek_to, set_speed, change_speed, config_set_number, toggle_mute, toggle_bookmark, bookmarked_here, follow_reading, startup_phase, startup_elapsed, estimated_elapsed, estimated_total, engine_name, synth.stats(), reference_missing, status, notices, skipped_count, is_finished, current_word, doc}`.

- [ ] **Step 1 : `header.rs`**

```rust
//! L'en-tête : ouvrir, coller, où l'on est, chercher, ouvrir les panneaux.

use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use super::theme::Palette;
use super::window::Gui;
use crate::app::Panel;

#[derive(Debug, Default, Clone, Copy)]
pub struct Actions {
    pub open_file: bool,
    pub paste: bool,
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) -> Actions {
    let mut actions = Actions::default();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Ouvrir…").on_hover_text("⌘O · Ctrl+O").clicked() {
            actions.open_file = true;
        }
        if ui.button("Coller").on_hover_text("⌘V · Ctrl+V — texte, ou image reconnue").clicked() {
            actions.paste = true;
        }
        ui.separator();

        let doc = gui.app.doc();
        let section = doc
            .section_of_sentence(gui.app.current_sentence())
            .map(|s| s.title.clone())
            .unwrap_or_default();
        ui.label(RichText::new(&doc.title).strong().color(palette.bright));
        ui.label(RichText::new(&gui.app.source).color(palette.dim));
        if !section.is_empty() && section != doc.title {
            ui.label(RichText::new("▸").color(palette.faint));
            ui.label(RichText::new(section).color(palette.accent));
        }
        if !gui.app.input_complete() {
            ui.label(RichText::new("⟳ flux").color(palette.dim));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Aide").on_hover_text("?").clicked() {
                gui.app.show_help = !gui.app.show_help;
            }
            if ui.selectable_label(gui.app.show_config, "Réglages").on_hover_text("s").clicked() {
                gui.app.toggle_config();
            }
            if ui.selectable_label(gui.app.panel == Panel::Bookmarks, "Signets").on_hover_text("B").clicked() {
                gui.app.toggle_panel(Panel::Bookmarks);
            }
            if ui.selectable_label(gui.app.panel == Panel::Contents, "Sommaire").on_hover_text("t").clicked() {
                gui.app.toggle_panel(Panel::Contents);
            }
            ui.separator();
            search(ui, gui, palette);
        });
    });
    ui.add_space(4.0);
    actions
}

/// Le champ de recherche et ses deux flèches.
fn search(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    if ui.button("›").on_hover_text("N — résultat suivant").clicked() {
        gui.app.search_advance(1);
    }
    if ui.button("‹").on_hover_text("P — résultat précédent").clicked() {
        gui.app.search_advance(-1);
    }
    if let Some(search) = gui.app.search() {
        if !search.is_empty() {
            ui.label(RichText::new(format!("{}/{}", search.position(), search.count())).color(palette.normal));
        }
    }
    let response = ui.add(
        TextEdit::singleline(&mut gui.search_text)
            .hint_text("Chercher… ( / )")
            .desired_width(200.0),
    );
    if gui.focus_search {
        response.request_focus();
        gui.focus_search = false;
    }
    // Entrée lance la recherche depuis le curseur ; le champ rend le clavier.
    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        gui.app.search_with(&gui.search_text.clone());
    }
    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        gui.search_text.clear();
        gui.app.search_cancel();
    }
}
```

- [ ] **Step 2 : `player.rs`**

```rust
//! La barre de lecture et la ligne d'état : la même chose qu'au terminal, en widgets.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Slider, Stroke, Ui, Vec2};

use super::theme::Palette;
use crate::app::config::Field;
use crate::app::{startup, App};
use crate::models;

pub fn show(ui: &mut Ui, app: &mut App, palette: &Palette) {
    ui.add_space(6.0);
    ui.horizontal(|ui| match app.startup_phase() {
        Some(phase) => {
            let elapsed = app.startup_elapsed();
            ui.label(RichText::new(startup::spinner(elapsed).to_string()).color(palette.accent).strong());
            ui.label(RichText::new(phase.label(&app.config.engine.family)).color(palette.normal));
            ui.label(RichText::new(format!("{:.1} s", elapsed.as_secs_f32())).color(palette.dim));
        }
        None => transport(ui, app, palette),
    });
    ui.horizontal(|ui| controls(ui, app, palette));
}

/// Lecture, progression, temps.
fn transport(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let playing = app.player.is_playing();
    let icon = if playing { "⏸" } else { "▶" };
    if ui
        .add(egui::Button::new(RichText::new(icon).color(palette.on_accent).strong()).fill(palette.accent))
        .on_hover_text("Espace")
        .clicked()
    {
        app.toggle_play();
    }

    let total = app.doc().sentences.len().max(1);
    let current = app.current_sentence();
    let times = match (app.estimated_elapsed(), app.estimated_total()) {
        (Some(elapsed), Some(total_secs)) => format!("{} / ~{}", clock(elapsed), clock(total_secs)),
        _ => "--:-- / --:--".to_string(),
    };
    let right = format!("{times}   {}/{}", current + 1, total);
    let right_w = 190.0;
    let bar_w = (ui.available_width() - right_w - 16.0).max(60.0);

    // Une barre qu'on peut saisir : cliquer y déplace la lecture.
    let (rect, response) = ui.allocate_exact_size(Vec2::new(bar_w, 18.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 6.0));
    ui.painter().rect_filled(track, 3.0, palette.faint);
    let ratio = (current as f32 / total as f32).clamp(0.0, 1.0);
    let filled = Rect::from_min_size(track.min, Vec2::new(track.width() * ratio, track.height()));
    ui.painter().rect_filled(filled, 3.0, palette.accent);
    ui.painter().circle_filled(Pos2::new(filled.max.x, track.center().y), 6.0, palette.accent);
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let at = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            app.seek_to((at * total as f32) as usize);
        }
    }
    ui.label(RichText::new(right).color(palette.normal));
}

/// Vitesse, volume, moteur, avance de synthèse, signet.
fn controls(ui: &mut Ui, app: &mut App, palette: &Palette) {
    if ui.button("−").on_hover_text("-").clicked() {
        app.change_speed(-0.1);
    }
    ui.label(RichText::new(format!("{:.2}×", app.config.voice.speed)).strong().color(palette.bright));
    if ui.button("+").on_hover_text("+").clicked() {
        app.change_speed(0.1);
    }
    for (key, speed, label) in [("1", 0.75, "0,75×"), ("2", 1.0, "1×"), ("3", 1.5, "1,5×"), ("4", 2.0, "2×")] {
        let selected = (app.config.voice.speed - speed).abs() < 0.01;
        if ui.selectable_label(selected, label).on_hover_text(key).clicked() {
            app.set_speed(speed);
        }
    }
    ui.separator();

    let muted = app.player.is_muted();
    if ui.selectable_label(muted, if muted { "🔇" } else { "🔊" }).on_hover_text("m — couper le son").clicked() {
        app.toggle_mute();
    }
    let mut volume = f64::from(app.player.volume());
    let slider = ui.add(Slider::new(&mut volume, 0.0..=1.0).show_value(false).step_by(0.05));
    if slider.changed() {
        app.config_set_number(Field::Volume, Some(volume));
    }
    ui.label(RichText::new(format!("{:>3.0} %", app.player.volume() * 100.0)).color(palette.normal));
    ui.separator();

    ui.label(RichText::new(app.engine_name()).color(palette.dim));
    let stats = app.synth.stats();
    ui.label(
        RichText::new(format!("⟐ {} prêtes · {} en cours · {} sessions", stats.ready, stats.in_flight, stats.workers))
            .color(palette.faint),
    );
    if stats.failed > 0 {
        ui.label(RichText::new(format!("⚠ {} échec(s)", stats.failed)).color(palette.accent));
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let here = app.bookmarked_here();
        if ui.selectable_label(here, "⚑ signet").on_hover_text("b").clicked() {
            app.toggle_bookmark();
        }
    });
}

/// La ligne d'état : les mêmes priorités qu'au terminal.
pub fn status(ui: &mut Ui, app: &mut App, palette: &Palette, opening: Option<&str>) {
    ui.horizontal_wrapped(|ui| {
        let on_fallback = !models::available().is_empty() && models::first_installed().is_none();
        if let Some(name) = opening {
            ui.label(RichText::new(format!("⠿ extraction de {name}…")).color(palette.accent));
        } else if app.reference_missing() {
            badge(ui, palette, "⚠ aucune voix de référence");
            ui.label(RichText::new("ce modèle clone : Réglages › Voix pour désigner un WAV").color(palette.normal));
            return;
        } else if !app.view.is_following() {
            badge(ui, palette, "⏸ vue figée");
            if ui.button("↧ revenir à la lecture").on_hover_text("f").clicked() {
                app.follow_reading();
            }
        } else if !app.status().is_empty() {
            ui.label(RichText::new(app.status().to_string()).color(palette.accent));
        } else if on_fallback {
            badge(ui, palette, "⚠ voix système");
            ui.label(RichText::new("Réglages › Modèles pour installer un vrai modèle").color(palette.normal));
        } else if let Some(notice) = app.notices.last() {
            ui.label(RichText::new(notice.clone()).color(palette.dim));
        } else {
            ui.label(
                RichText::new("s réglages · ? aide · espace lecture · / chercher · n/p titre · b signet · clic sur un mot")
                    .color(palette.faint),
            );
        }
        if let Some(search) = app.search() {
            if !search.is_empty() {
                ui.label(RichText::new(format!("· « {} » {}/{}", search.query, search.position(), search.count())).color(palette.normal));
            }
        }
        if let Some(index) = app.current_word() {
            if let Some(p) = app.doc().sentence(app.current_sentence()).and_then(|s| s.words.get(index)).and_then(|w| w.punctuation) {
                ui.label(RichText::new(format!("· {}", p.label())).color(palette.accent).strong());
            }
        }
        if app.skipped_count() > 0 {
            ui.label(RichText::new(format!("· {} phrase(s) sautée(s)", app.skipped_count())).color(palette.dim));
        }
        if app.is_finished() {
            ui.label(RichText::new("· lecture terminée").color(palette.dim));
        }
    });
}

fn badge(ui: &mut Ui, palette: &Palette, text: &str) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::TextStyle::Body.resolve(ui.style()), palette.on_accent);
    let (rect, _) = ui.allocate_exact_size(galley.size() + Vec2::new(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, palette.accent);
    ui.painter().galley(rect.min + Vec2::new(6.0, 2.0), galley, palette.on_accent);
    let _ = (Color32::TRANSPARENT, Stroke::NONE);
}

/// Formate des secondes en `M:SS` ou `H:MM:SS`.
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
```

Dans `window.rs`, la disposition devient : `TopBottomPanel::top("header")` → `header::show` ; `TopBottomPanel::bottom("status")` → `player::status` ; `TopBottomPanel::bottom("player")` → `player::show` ; puis le panneau central.

- [ ] **Step 3 : Regarder** — lancer, capturer, vérifier les deux barres et l'en-tête ; taper `/`, un mot, `Entrée` : la lecture saute, les résultats se soulignent.

- [ ] **Step 4 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): en-tête, recherche, barre de lecture et ligne d'état"
```

---

### Task 13 : Panneau latéral et aide

**Files:**
- Create: `src/gui/panel.rs`, `src/gui/help.rs`
- Modify: `src/gui/mod.rs`, `src/gui/window.rs`

**Interfaces:**
- Produces: `panel::show(ui, app, palette)` ; `help::show(ctx, app, palette)`.

- [ ] **Step 1 : `panel.rs`**

```rust
//! Sommaire et signets : les mêmes listes qu'au terminal, cliquables.

use eframe::egui::{self, RichText, ScrollArea, Ui};

use super::theme::Palette;
use crate::app::{App, Panel};

pub fn show(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let title = match app.panel {
        Panel::Contents => "Sommaire",
        Panel::Bookmarks => "Signets",
        Panel::None => return,
    };
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading(RichText::new(title).color(palette.accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Échap").clicked() {
                app.panel = Panel::None;
            }
        });
    });
    ui.separator();
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.panel {
        Panel::Contents => contents(ui, app, palette),
        Panel::Bookmarks => bookmarks(ui, app, palette),
        Panel::None => {}
    });
}

fn contents(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let current = app.doc().section_of_sentence(app.current_sentence()).map(|s| s.id);
    let rows: Vec<(usize, u8, String, bool)> = app
        .navigable_sections()
        .into_iter()
        .filter_map(|id| app.doc().section(id))
        .enumerate()
        .map(|(i, s)| (i, s.level, s.title.clone(), current == Some(s.id)))
        .collect();
    for (i, level, title, active) in rows {
        ui.horizontal(|ui| {
            ui.add_space(12.0 * f32::from(level.saturating_sub(1)));
            let text = if active {
                RichText::new(title).color(palette.accent).strong()
            } else {
                RichText::new(title).color(palette.normal)
            };
            if ui.selectable_label(i == app.selection, text).clicked() {
                app.selection = i;
                app.activate_selection();
            }
        });
    }
}

fn bookmarks(ui: &mut Ui, app: &mut App, palette: &Palette) {
    let rows: Vec<(String, String)> = app
        .bookmarks
        .for_document(&app.source)
        .map(|b| (b.snippet.clone(), format!("{} · {}", b.timestamp(), b.section_title)))
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new("Aucun signet. b pour en poser un.").color(palette.dim));
        return;
    }
    for (i, (snippet, meta)) in rows.into_iter().enumerate() {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let text = RichText::new(truncate(&snippet, 70)).color(palette.normal);
                if ui.selectable_label(i == app.selection, text).clicked() {
                    app.selection = i;
                    app.activate_selection();
                }
                ui.label(RichText::new(meta).small().color(palette.faint));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if ui.small_button("×").on_hover_text("d — retirer").clicked() {
                    app.selection = i;
                    app.delete_selection();
                }
            });
        });
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}
```

- [ ] **Step 2 : `help.rs`**

```rust
//! L'aide : les mêmes tables que le terminal, dans une fenêtre flottante.

use eframe::egui::{self, Align2, RichText};

use super::theme::Palette;
use crate::app::keys::{CONFIG_HELP, HELP};
use crate::app::App;

pub fn show(ctx: &egui::Context, app: &mut App, palette: &Palette) {
    if !app.show_help {
        return;
    }
    let (title, entries) = if app.show_config {
        ("Configuration", CONFIG_HELP)
    } else {
        ("Raccourcis", HELP)
    };
    let response = egui::Window::new(RichText::new(title).color(palette.accent))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("aide").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                for (keys, description) in entries {
                    ui.label(RichText::new(*keys).color(palette.accent).strong());
                    ui.label(RichText::new(*description).color(palette.normal));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            if ui.button("Fermer").clicked() {
                app.show_help = false;
            }
        });
    // Un clic hors de la fenêtre la referme, comme n'importe quelle touche.
    if let Some(response) = response {
        if ctx.input(|i| i.pointer.any_pressed()) && !response.response.contains_pointer() {
            app.show_help = false;
        }
    }
}
```

Dans `window.rs` : `SidePanel::right("panneau").default_width(300.0).show_inside(ui, |ui| panel::show(ui, &mut self.app, &palette))` quand `self.app.panel != Panel::None` et `!self.app.show_config` ; `help::show(&ctx, &mut self.app, &palette)` en fin de trame.

- [ ] **Step 3 : Regarder** — `t` ouvre le sommaire, un clic saute ; `b` pose un signet, `B` le montre, `×` le retire ; `?` ouvre l'aide, un clic dehors la ferme.

- [ ] **Step 4 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): sommaire et signets cliquables, aide flottante"
```

---

### Task 14 : La configuration en widgets

**Files:**
- Create: `src/gui/settings.rs`
- Modify: `src/gui/mod.rs`, `src/gui/window.rs` (`rubric`, `edits`, `confirm_delete`)

**Interfaces:**
- Produces: `settings::Rubric::{Section(usize), Display}` ; `settings::show(ui, gui, palette)`.
- Consumes: `Field::{control, number, set_number via App, value, hint_for, is_ignored, raw_text, label, adjust}`, `Control`, `Section::{ALL, label, summary, fields}`, `ConfigScreen::{section, row, page, show_all, download, notice}`, `App::{config_set_number, config_set_choice, config_set_text, apply_effect, config_activate, config_delete, preview_voice, preview_status, save_config, reference_missing}`, `models::{available, is_installed, is_truncated, install_path, human_size, free_space, installed_size, Progress}`.

- [ ] **Step 1 : Squelette et rubriques**

```rust
//! La configuration en widgets : rubriques à gauche, réglages à droite.
//!
//! Le terminal montrait une page à la fois faute de place ; la fenêtre a la
//! place, et les deux pages redeviennent une liste et un contenu. Les valeurs
//! prennent effet immédiatement, par les mêmes `Effect` qu'au terminal.

use std::time::{Duration, Instant};

use eframe::egui::{self, ComboBox, ProgressBar, RichText, ScrollArea, Slider, TextEdit, Ui};

use super::theme::Palette;
use super::window::Gui;
use crate::app::config::{Control, Field, Page, Section};
use crate::config::GuiConfig;
use crate::models;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rubric {
    /// Une rubrique du contrôleur, par son rang dans `Section::ALL`.
    Section(usize),
    /// Police, taille, interlignage, thème : la fenêtre seule.
    Display,
}

pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    egui::SidePanel::left("rubriques").default_width(260.0).resizable(false).show_inside(ui, |ui| {
        ui.add_space(8.0);
        ui.heading(RichText::new("Configuration").color(palette.accent));
        ui.label(RichText::new(format!("moteur : {}", gui.app.engine_name())).color(palette.dim));
        ui.add_space(8.0);
        for (i, section) in Section::ALL.iter().enumerate() {
            let selected = gui.rubric == Rubric::Section(i);
            let summary = section.summary(&gui.app.config);
            if rubric_row(ui, palette, selected, section.label(), &summary) {
                gui.rubric = Rubric::Section(i);
                gui.app.screen.section = i;
                gui.app.screen.page = Page::Detail;
                gui.app.screen.row = 0;
            }
        }
        let display = format!("{} · {:.0} px", gui.app.config.gui.font, gui.app.config.gui.font_size());
        if rubric_row(ui, palette, gui.rubric == Rubric::Display, "Affichage", &display) {
            gui.rubric = Rubric::Display;
        }
        ui.add_space(12.0);
        if ui.button("‹ Retour à la lecture").on_hover_text("Échap · s").clicked() {
            gui.app.toggle_config();
        }
    });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(palette.background).inner_margin(20.0))
        .show_inside(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match gui.rubric {
                Rubric::Section(i) if Section::ALL[i] == Section::Models => models_list(ui, gui, palette),
                Rubric::Section(i) => fields(ui, gui, Section::ALL[i], palette),
                Rubric::Display => display(ui, gui, palette),
            });
            footer(ui, gui, palette);
        });
}

fn rubric_row(ui: &mut Ui, palette: &Palette, selected: bool, label: &str, summary: &str) -> bool {
    let text = if selected {
        RichText::new(label).color(palette.accent).strong()
    } else {
        RichText::new(label).color(palette.normal)
    };
    let clicked = ui.selectable_label(selected, text).clicked();
    ui.label(RichText::new(summary).small().color(palette.faint));
    ui.add_space(4.0);
    clicked
}
```

- [ ] **Step 2 : Les champs**

```rust
fn fields(ui: &mut Ui, gui: &mut Gui, section: Section, palette: &Palette) {
    ui.heading(RichText::new(section.label()).color(palette.bright));
    ui.add_space(8.0);
    let show_all = gui.app.screen.show_all;
    let mut hidden = 0;
    for field in section.fields() {
        // La fenêtre a son propre interlignage, en fraction de ligne.
        if *field == Field::LineSpacing {
            continue;
        }
        let ignored = field.is_ignored(&gui.app.config);
        if ignored && !show_all {
            hidden += 1;
            continue;
        }
        ui.add_enabled_ui(!ignored, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([220.0, 20.0], egui::Label::new(RichText::new(field.label()).color(palette.normal)));
                widget(ui, gui, *field, palette);
                if ignored {
                    ui.label(RichText::new("(ignoré par ce modèle)").color(palette.faint));
                }
            });
        });
        ui.label(RichText::new(field.hint_for(&gui.app.config)).small().color(palette.dim));
        ui.add_space(10.0);
    }
    ui.separator();
    let mut show_all = gui.app.screen.show_all;
    let label = if hidden > 0 {
        format!("Afficher les {hidden} réglage(s) sans effet sur ce modèle")
    } else {
        "Afficher les réglages sans effet".to_string()
    };
    if ui.checkbox(&mut show_all, label).changed() {
        gui.app.screen.show_all = show_all;
    }
}

/// Le widget d'un réglage, choisi par ce qu'il est.
fn widget(ui: &mut Ui, gui: &mut Gui, field: Field, palette: &Palette) {
    let volume = gui.app.player.volume();
    match field.control(&gui.app.config) {
        Control::Slider { min, max, step, decimals, unset } => {
            let current = field.number(&gui.app.config, volume);
            if let Some((label, start)) = unset {
                let mut left = current.is_none();
                if ui.checkbox(&mut left, label).changed() {
                    gui.app.config_set_number(field, if left { None } else { Some(start) });
                    return;
                }
            }
            let Some(mut value) = current else { return };
            let is_volume = field == Field::Volume;
            if is_volume {
                value *= 100.0;
            }
            let (lo, hi, st) = if is_volume { (min * 100.0, max * 100.0, step * 100.0) } else { (min, max, step) };
            let mut slider = Slider::new(&mut value, lo..=hi).step_by(st).fixed_decimals(if is_volume { 0 } else { decimals });
            slider = match field {
                Field::Volume => slider.suffix(" %"),
                Field::Speed => slider.suffix("×"),
                _ => slider,
            };
            if ui.add(slider).changed() {
                gui.app.config_set_number(field, Some(if is_volume { value / 100.0 } else { value }));
            }
        }
        Control::Integer { min, max, unset, zero } => {
            let current = field.number(&gui.app.config, volume);
            if let Some((label, start)) = unset {
                let mut left = current.is_none();
                if ui.checkbox(&mut left, label).changed() {
                    gui.app.config_set_number(field, if left { None } else { Some(start as f64) });
                    return;
                }
            }
            let Some(mut value) = current else { return };
            if let Some(label) = zero {
                let mut at_zero = value == 0.0;
                if ui.checkbox(&mut at_zero, label).changed() {
                    gui.app.config_set_number(field, Some(if at_zero { 0.0 } else { min as f64 }));
                    return;
                }
                if at_zero {
                    return;
                }
            }
            let mut value = value as i64;
            let _ = &mut value;
            let mut v = value as f64;
            if ui.add(Slider::new(&mut v, min as f64..=max as f64).integer()).changed() {
                gui.app.config_set_number(field, Some(v));
            }
        }
        Control::Choice(choices) => {
            let current = choice_of(field, &gui.app.config);
            let shown = if current.is_empty() { "(défaut)" } else { current.as_str() };
            let mut picked: Option<&str> = None;
            ComboBox::from_id_salt(field.label()).selected_text(shown).show_ui(ui, |ui| {
                for choice in choices {
                    let label = if choice.is_empty() { "(défaut)" } else { choice };
                    if ui.selectable_label(*choice == current, label).clicked() {
                        picked = Some(choice);
                    }
                }
            });
            if let Some(choice) = picked {
                gui.app.config_set_choice(field, choice);
            }
        }
        Control::Voices(voices) => {
            let current = gui.app.config.voice.voice.clone();
            let shown = voices.iter().find(|v| v.id == current).map_or(voices[0].label, |v| v.label);
            let mut picked: Option<&str> = None;
            ComboBox::from_id_salt("voix").selected_text(shown).show_ui(ui, |ui| {
                for voice in voices {
                    if ui.selectable_label(voice.id == current, voice.label).clicked() {
                        picked = Some(voice.id);
                    }
                }
            });
            if let Some(id) = picked {
                gui.app.config_set_choice(field, id);
            }
        }
        Control::Toggle => {
            let mut on = field.value(&gui.app.config, volume) == "oui";
            if ui.checkbox(&mut on, "").changed() {
                let effect = field.adjust(&mut gui.app.config, 1);
                gui.app.apply_effect(effect);
            }
        }
        Control::Text => {
            let key = field.label();
            let config_text = field.raw_text(&gui.app.config);
            let buffer = gui.edits.entry(key).or_insert(config_text);
            let response = ui.add(TextEdit::singleline(buffer).desired_width(320.0));
            if response.lost_focus() {
                let text = buffer.clone();
                gui.edits.remove(key);
                if text.trim() != field.raw_text(&gui.app.config).trim() {
                    gui.app.config_set_text(field, text);
                }
            }
            if field == Field::ModelPath && ui.button("Parcourir…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    gui.edits.remove(key);
                    gui.app.config_set_text(field, path.to_string_lossy().into_owned());
                }
            }
            let _ = palette;
        }
    }
}

fn choice_of(field: Field, config: &crate::config::Config) -> String {
    match field {
        Field::Language => config.voice.language.clone(),
        Field::Emotion => config.voice.emotion.clone(),
        Field::CodeBlocks => config.reading.code_blocks.clone(),
        Field::EnginePref => config.engine.engine.clone(),
        Field::Compute => config.engine.compute.clone(),
        _ => String::new(),
    }
}
```

- [ ] **Step 3 : Affichage, modèles, pied**

```rust
fn display(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.heading(RichText::new("Affichage").color(palette.bright));
    ui.add_space(8.0);
    let gui_config = &mut gui.app.config.gui;

    ui.horizontal(|ui| {
        ui.add_sized([220.0, 20.0], egui::Label::new("Police de lecture"));
        let names = [("atkinson", "Atkinson Hyperlegible"), ("opendyslexic", "OpenDyslexic"), ("default", "Standard")];
        let shown = names.iter().find(|(id, _)| *id == gui_config.font).map_or("Standard", |(_, n)| *n);
        ComboBox::from_id_salt("police").selected_text(shown).show_ui(ui, |ui| {
            for (id, name) in names {
                if ui.selectable_label(gui_config.font == id, name).clicked() {
                    gui_config.font = id.to_string();
                }
            }
        });
    });
    ui.label(RichText::new("Atkinson Hyperlegible distingue les glyphes qui se confondent — I l 1, O 0, b d. OpenDyslexic est celle qu'on demande par son nom : à chacun de juger.").small().color(palette.dim));
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.add_sized([220.0, 20.0], egui::Label::new("Taille du texte"));
        let mut size = gui_config.font_size();
        if ui.add(Slider::new(&mut size, 14.0..=40.0).step_by(1.0).fixed_decimals(0).suffix(" px")).changed() {
            gui_config.font_size = size;
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_sized([220.0, 20.0], egui::Label::new("Interlignage"));
        let mut lh = gui_config.line_height();
        if ui.add(Slider::new(&mut lh, 1.0..=2.5).step_by(0.05).fixed_decimals(2)).changed() {
            gui_config.line_height = lh;
        }
    });
    ui.label(RichText::new("En fraction de la taille du texte. 1,6 est la recommandation courante pour la lecture accompagnée.").small().color(palette.dim));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_sized([220.0, 20.0], egui::Label::new("Thème"));
        for (id, name) in GuiConfig::THEMES.iter().zip(["Sombre", "Clair"]) {
            if ui.selectable_label(gui_config.theme == *id, name).clicked() {
                gui_config.theme = id.to_string();
            }
        }
    });
    ui.add_space(10.0);
    ui.label(RichText::new("La largeur de colonne et la règle de lecture se règlent dans « Lecture » : elles valent pour le terminal aussi.").small().color(palette.dim));
}

fn models_list(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.heading(RichText::new("Modèles").color(palette.bright));
    ui.add_space(8.0);
    let packages = models::available();
    if packages.is_empty() {
        ui.label(RichText::new("Aucun modèle disponible.").strong());
        ui.label(RichText::new("Ce binaire a été compilé sans audio.cpp ; la lecture utilise la voix du système. Pour un vrai modèle :").color(palette.dim));
        ui.code("cargo build --release --features gui,audio-cpp");
        return;
    }
    // Les actions du contrôleur portent sur la rubrique courante et sa ligne.
    gui.app.screen.section = 0;
    gui.app.screen.page = Page::Detail;
    let active = gui.app.config.engine.model_path.clone();
    let downloading = gui.app.screen.download.as_ref().map(|d| (d.package.id, d.progress()));

    for (i, package) in packages.iter().enumerate() {
        let installed = models::is_installed(package);
        let truncated = models::is_truncated(package);
        let is_active = models::install_path(package).is_some_and(|p| p.to_string_lossy() == active);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let name = if is_active {
                    RichText::new(package.display_name).color(palette.accent).strong()
                } else {
                    RichText::new(package.display_name).color(palette.bright).strong()
                };
                ui.label(name);
                let (badge, color) = if truncated {
                    ("⚠ tronqué", palette.accent)
                } else if is_active {
                    ("● utilisé", palette.accent)
                } else if installed {
                    ("○ installé", palette.normal)
                } else {
                    ("à venir", palette.faint)
                };
                ui.label(RichText::new(badge).color(color));
                ui.label(RichText::new(models::human_size(package.bytes)).color(palette.dim));
                if package.recommended && !installed {
                    ui.label(RichText::new("Recommandé pour commencer").color(palette.accent));
                }
            });
            ui.label(RichText::new(package.description).small().color(palette.dim));
            ui.label(RichText::new(package.languages_label).small().color(palette.faint));
            if truncated {
                ui.label(RichText::new(format!(
                    "Fichier incomplet : {} au lieu de {}.",
                    models::human_size(models::installed_size(package).unwrap_or(0)),
                    models::human_size(package.bytes)
                )).color(palette.accent));
            }
            ui.horizontal(|ui| {
                match &downloading {
                    Some((id, models::Progress::Running { received, total })) if *id == package.id => {
                        let ratio = if *total > 0 { *received as f32 / *total as f32 } else { 0.0 };
                        ui.add(ProgressBar::new(ratio).desired_width(320.0).text(format!("{} / {}", models::human_size(*received), models::human_size(*total))));
                        if ui.button("Interrompre").clicked() {
                            gui.app.screen.row = i;
                            gui.app.config_delete();
                        }
                    }
                    _ => {
                        if installed && !truncated {
                            if !is_active && ui.button("Utiliser").clicked() {
                                gui.app.screen.row = i;
                                gui.app.config_activate();
                            }
                            delete_button(ui, gui, i, palette);
                        } else {
                            let busy = downloading.is_some();
                            let label = if truncated { "Réinstaller" } else { "Installer" };
                            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                                gui.app.screen.row = i;
                                gui.app.config_activate();
                            }
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
    }
    if let Some(free) = models::free_space() {
        ui.label(RichText::new(format!("Espace disque libre : {}", models::human_size(free))).color(palette.faint));
    }
}

/// Supprimer demande un second clic dans les trois secondes : à la souris,
/// un clic de travers effacerait 1,6 Go.
fn delete_button(ui: &mut Ui, gui: &mut Gui, row: usize, palette: &Palette) {
    let armed = gui.confirm_delete.is_some_and(|(r, at)| r == row && at.elapsed() < Duration::from_secs(3));
    if armed {
        if ui.add(egui::Button::new(RichText::new("Confirmer la suppression").color(palette.on_accent)).fill(palette.accent)).clicked() {
            gui.confirm_delete = None;
            gui.app.screen.row = row;
            gui.app.config_delete();
        }
    } else if ui.button("Supprimer").clicked() {
        gui.confirm_delete = Some((row, Instant::now()));
    }
}

fn footer(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Écouter la voix").on_hover_text("t").clicked() {
            // Dans la liste des modèles, l'aperçu porte sur celui qui est chargé.
            if gui.rubric == Rubric::Section(0) {
                let active = gui.app.config.engine.model_path.clone();
                if let Some(i) = models::available().iter().position(|p| models::install_path(p).is_some_and(|q| q.to_string_lossy() == active)) {
                    gui.app.screen.row = i;
                }
            }
            gui.app.preview_voice();
        }
        if ui.button("Enregistrer").on_hover_text("w").clicked() {
            gui.app.save_config();
        }
        let message = if let Some(preview) = gui.app.preview_status() {
            preview
        } else if gui.app.reference_missing() {
            "⚠ ce modèle ne parle qu'en clonant : renseignez un WAV dans « Voix »".to_string()
        } else if !gui.app.screen.notice.is_empty() {
            gui.app.screen.notice.clone()
        } else {
            gui.app.status().to_string()
        };
        if !message.is_empty() {
            ui.label(RichText::new(message).color(palette.accent));
        }
    });
}
```

`Gui` gagne `pub(super) rubric: settings::Rubric` (défaut `Section(0)`), `pub(super) edits: HashMap<&'static str, String>`, `pub(super) confirm_delete: Option<(usize, Instant)>`. Quand `show_config` s'ouvre, `rubric` se cale sur `app.screen.section`. Dans `window.rs`, quand `self.app.show_config`, le panneau central appelle `settings::show`.

- [ ] **Step 4 : Regarder** — `s` : rubriques à gauche, Voix ouverte ; bouger le curseur de vitesse : la lecture reprend à la nouvelle vitesse ; Affichage : passer à OpenDyslexic et au thème clair, revenir ; Modèles : les badges et boutons.

- [ ] **Step 5 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): la configuration en widgets, et une rubrique Affichage"
```

---

### Task 15 : Ouvrir — dialogue, dépôt, presse-papiers, état vide

**Files:**
- Create: `src/gui/open.rs`
- Modify: `src/gui/mod.rs`, `src/gui/window.rs`

**Interfaces:**
- Produces: `open::Opening { name: String, resumes: bool, rx: Receiver<anyhow::Result<String>> }` ; `open::pick_file() -> Option<PathBuf>` ; `open::start_path(gui, PathBuf)` ; `open::start_clipboard(gui)` ; `open::poll(gui)` ; `open::handle_drops(ctx, gui)` ; `open::empty_state(ui, gui, palette) -> Actions` ; `open::drop_overlay(ctx, gui, palette)`.

- [ ] **Step 1 : Implémenter**

```rust
//! Ouvrir un document depuis la fenêtre : dialogue, fichier déposé, presse-papiers.
//!
//! L'extraction d'un PDF ou d'un scan prend des secondes : elle part dans un
//! thread, la ligne d'état le dit, et le document arrive quand il est prêt.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use anyhow::Result;
use eframe::egui::{self, Align2, Color32, RichText, Ui};

use super::theme::Palette;
use super::window::Gui;
use crate::doc::DocumentBuilder;
use crate::input;

/// Une extraction en cours.
pub struct Opening {
    pub name: String,
    /// Vrai pour un fichier : sa position retenue est reprise.
    resumes: bool,
    rx: Receiver<Result<String>>,
}

pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Documents",
            &["md", "markdown", "txt", "pdf", "png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp"],
        )
        .pick_file()
}

pub fn start_path(gui: &mut Gui, path: PathBuf) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::source::load_with_cache(&path, true));
    });
    gui.opening = Some(Opening { name, resumes: true, rx });
}

pub fn start_clipboard(gui: &mut Gui) {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        #[cfg(feature = "clipboard")]
        let result = crate::source::clipboard();
        #[cfg(not(feature = "clipboard"))]
        let result = Err(anyhow::anyhow!("binaire compilé sans l'option `clipboard`"));
        let _ = tx.send(result);
    });
    gui.opening = Some(Opening { name: "presse-papiers".to_string(), resumes: false, rx });
}

/// Range un document arrivé, ou l'erreur qui en tient lieu.
pub fn poll(gui: &mut Gui) {
    let Some(opening) = gui.opening.as_ref() else { return };
    let result = match opening.rx.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("extraction interrompue")),
    };
    let Opening { name, resumes, .. } = gui.opening.take().unwrap();
    match result {
        Ok(text) => {
            let mut builder = DocumentBuilder::new(gui.app.config.code_policy());
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            if builder.sentence_count() == 0 {
                gui.app.notify(&format!("{name} : aucun contenu lisible"));
                return;
            }
            gui.app.open_document(builder, None, name);
            if resumes {
                gui.app.resume_where_left();
            }
            gui.reader.reset();
        }
        Err(err) => gui.app.notify(&format!("{name} : {err}")),
    }
}

/// Un fichier survole ou tombe sur la fenêtre.
pub fn handle_drops(ctx: &egui::Context, gui: &mut Gui) {
    gui.hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
    if let Some(path) = dropped.into_iter().next() {
        start_path(gui, path);
    }
}

/// Le voile « Déposer pour lire », pendant le survol.
pub fn drop_overlay(ctx: &egui::Context, gui: &Gui, palette: &Palette) {
    if !gui.hovering_file {
        return;
    }
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dépôt")));
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Déposer pour lire",
        egui::FontId::proportional(32.0),
        palette.accent,
    );
}

/// Sans document : deux boutons au milieu du vide, pas un écran de réglages.
pub fn empty_state(ui: &mut Ui, gui: &Gui, palette: &Palette) -> super::header::Actions {
    let mut actions = super::header::Actions::default();
    ui.add_space(ui.available_height() * 0.3);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("sillon").size(40.0).color(palette.accent).strong());
        ui.label(RichText::new("Un document à lire, à écouter, à suivre des yeux.").color(palette.dim));
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 280.0).max(0.0) / 2.0);
            if ui.button(RichText::new("Ouvrir un fichier…").size(18.0)).clicked() {
                actions.open_file = true;
            }
            if ui.button(RichText::new("Lire le presse-papiers").size(18.0)).clicked() {
                actions.paste = true;
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("ou déposez un fichier sur la fenêtre — markdown, texte, PDF, image").color(palette.faint));
        if let Some(opening) = &gui.opening {
            ui.add_space(12.0);
            ui.label(RichText::new(format!("extraction de {}…", opening.name)).color(palette.accent));
        }
    });
    actions
}
```

Dans `window.rs` : `Gui` gagne `pub(super) opening: Option<open::Opening>` et `pub(super) hovering_file: bool`. Dans la trame : `open::poll(self)` après `tick` ; `open::handle_drops(&ctx, self)` ; les `Actions` de l'en-tête et de l'état vide, et les `Requests` du clavier, se servent en fin de trame :

```rust
        if open_file {
            if let Some(path) = open::pick_file() {
                open::start_path(self, path);
            }
        }
        if paste {
            open::start_clipboard(self);
        }
        open::drop_overlay(&ctx, self, &palette);
```

Le panneau central montre `open::empty_state` quand `self.app.doc().is_empty() && self.app.input_complete() && !self.app.show_config`. La ligne d'état reçoit `self.opening.as_ref().map(|o| o.name.as_str())`.

- [ ] **Step 2 : Regarder** — lancer `sillon-gui` sans argument : l'état vide ; « Ouvrir… » ouvre le dialogue natif ; déposer `dist/demo.md` sur la fenêtre : le document s'ouvre ; ⌘V avec du texte copié : le collage se lit.

- [ ] **Step 3 : Commit**

```bash
git add src/gui
git commit -m "feat(gui): ouvrir un fichier, un dépôt, le presse-papiers

L'extraction part dans un thread ; sans document, la fenêtre s'ouvre sur
« Ouvrir… », pas sur les réglages."
```

---

### Task 16 : Distribuer la fenêtre

**Files:**
- Modify: `scripts/package.sh`, `scripts/install.sh`, `README.md`, `src/lib.rs`

- [ ] **Step 1 : `package.sh`**

Après la copie de `target/release/sillon` :

```sh
# La fenêtre suit quand elle a été compilée.
case ",$FEATURES," in
    *,gui,*)
        cp target/release/sillon-gui "$OUT/"
        case "$(uname -s)" in
            Darwin)
                # Un bundle minimal : le Dock le lance, un fichier déposé sur
                # son icône lui arrive. Le shim est cherché à côté du binaire.
                APP="$OUT/Sillon.app/Contents"
                mkdir -p "$APP/MacOS" "$APP/Resources"
                cp target/release/sillon-gui "$APP/MacOS/sillon-gui"
                for shim in "$OUT"/libaudiocpp_shim.*; do
                    [ -f "$shim" ] && cp "$shim" "$APP/MacOS/"
                done
                cat > "$APP/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Sillon</string>
  <key>CFBundleDisplayName</key><string>Sillon</string>
  <key>CFBundleIdentifier</key><string>org.koumalabs.sillon</string>
  <key>CFBundleVersion</key><string>0.2.0</string>
  <key>CFBundleShortVersionString</key><string>0.2.0</string>
  <key>CFBundleExecutable</key><string>sillon-gui</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>Document</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSItemContentTypes</key><array>
      <string>net.daringfireball.markdown</string>
      <string>public.plain-text</string>
      <string>com.adobe.pdf</string>
      <string>public.image</string>
    </array>
  </dict></array>
</dict></plist>
PLIST
                echo "Joint : Sillon.app"
                ;;
            Linux)
                cat > "$OUT/sillon.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Sillon
Comment=Lit un document à voix haute, à suivre des yeux
Exec=sillon-gui %f
Terminal=false
Categories=Office;Audio;Accessibility;
MimeType=text/markdown;text/plain;application/pdf;image/png;image/jpeg;
DESKTOP
                echo "Joint : sillon.desktop"
                ;;
        esac
        ;;
esac
```

Et la vérification finale : si `dist/sillon-gui` existe, `( cd "$OUT" && ./sillon-gui --show-config >/dev/null )`.

- [ ] **Step 2 : `install.sh`**

Après l'installation de `sillon` :

```sh
if [ -f dist/sillon-gui ]; then
    install -m 755 dist/sillon-gui "$PREFIX/bin/sillon-gui"
    echo "Installé : $PREFIX/bin/sillon-gui"
    if [ -f dist/sillon.desktop ]; then
        mkdir -p "$HOME/.local/share/applications"
        install -m 644 dist/sillon.desktop "$HOME/.local/share/applications/sillon.desktop"
    fi
    if [ -d dist/Sillon.app ]; then
        echo "Bundle : dist/Sillon.app — à glisser dans /Applications si vous le voulez dans le Dock."
    fi
fi
```

Le commentaire d'en-tête gagne `FEATURES=audio-cpp,ocr,gui ./scripts/install.sh   # avec la fenêtre`.

- [ ] **Step 3 : README et `lib.rs`**

Dans le README, après *Installation*, une section :

```markdown
## Interface graphique

`sillon-gui` est le même lecteur dans une fenêtre : mêmes touches, mêmes
réglages, même moteur. Ce que la fenêtre ajoute, c'est ce qu'un terminal ne
peut pas donner à qui lit en écoutant :

- une police faite pour la lisibilité, embarquée — Atkinson Hyperlegible par
  défaut, OpenDyslexic au choix — et une taille de texte réglable ;
- un interlignage réel, en fraction de ligne, et une règle de lecture qui passe
  derrière la ligne entière ;
- un mot prononcé qui ne fait pas bouger la ligne : il change de fond et de
  couleur, jamais de graisse ;
- un clic sur un mot, qui y envoie la voix ; un clic sur la barre de progression ;
  un sommaire et des signets cliquables ;
- **Ouvrir…**, un fichier déposé sur la fenêtre, le presse-papiers.

```bash
cargo build --release --features gui,audio-cpp,ocr
./target/release/sillon-gui rapport.pdf
FEATURES=audio-cpp,ocr,gui ./scripts/install.sh     # pose aussi sillon-gui
```

La rubrique **Affichage** des réglages porte la police, la taille,
l'interlignage et le thème — sombre par défaut, clair au choix. Ils vivent dans
la section `[gui]` du `config.toml`. Les autres rubriques sont celles du
terminal, en curseurs et listes.

Sur macOS, `scripts/package.sh` assemble aussi `dist/Sillon.app`, à glisser
dans le Dock ; sur Linux, un `sillon.desktop`. La fenêtre s'appuie sur egui
(wgpu : Metal, Vulkan ou OpenGL) et ne demande aucune bibliothèque à installer.
```

Dans *Architecture*, le schéma devient :

```text
entrée ──▶ doc ──▶ tts ──▶ audio ──▶ app ──▶ tui | gui
```

et la liste gagne `` - `gui` — rendu egui du contrôleur : fenêtre, polices, réglages en widgets. `` Le même schéma dans `src/lib.rs`.

- [ ] **Step 4 : Vérifier**

```bash
FEATURES=gui ./scripts/package.sh 2>&1 | tail -8
ls dist dist/Sillon.app/Contents/MacOS 2>/dev/null
cargo test 2>&1 | grep -E "^test result"
cargo test --features gui 2>&1 | grep -E "^test result"
```

- [ ] **Step 5 : Commit**

```bash
git add scripts README.md src/lib.rs
git commit -m "build(gui): livrer la fenêtre — binaire, bundle macOS, .desktop"
```

---

## Vérification finale

- `cargo test` et `cargo test --features gui` passent.
- `cargo build --release --features gui,audio-cpp` produit `sillon-gui` ; lancé sur `dist/demo.md` avec un modèle installé, le mot suit la voix, la règle suit le mot, `Pg↓` fige, `f` revient, `s` ouvre les réglages, `Échap` revient.
- Le terminal n'a pas changé : `cargo run -- dist/demo.md` se comporte comme avant.
