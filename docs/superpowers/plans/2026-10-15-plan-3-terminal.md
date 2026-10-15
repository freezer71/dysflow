# Simplification de l'expérience, plan 3 : terminal — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** au terminal, ne garder que ce qui sert à écouter un texte en le suivant des yeux : signets retirés du cœur, clavier de lecture réduit, réglages sur une page enregistrés à chaque changement, voix installée seule, `sillon` sans argument qui explique comment lire.

**Architecture:** les signets quittent le cœur (`src/bookmark.rs` et tout ce qui en dépend) ; les touches retirées disparaissent de `app::keys` ; l'écran de réglages perd ses rubriques et devient une liste fixe de six `Field`, branchée sur l'enregistrement automatique livré par le plan 1 (`App::config_adjust_and_save`, `Field::copy_value`) ; l'installation de la voix passe par `models::ensure_default_voice` (plan 1), lancée par `launch::prepare` ; `launch` décide, par une fonction pure, d'afficher l'aide courte quand il n'y a rien à lire.

**Tech Stack:** Rust 2021 (crate `sillon`, MSRV 1.88 ; `sillon-ffi` pour la vérification de compilation), ratatui 0.30, crossterm 0.29, clap 4.5 (derive), toml 1.

**Spec:** `docs/superpowers/specs/2026-10-15-simplification-ux-design.md`, § 2 (terminal, tout ce plan) et § 3 (signets côté moteur). Plan 1 lu pour ses interfaces : `docs/superpowers/plans/2026-10-15-plan-1-moteur-ffi.md` (« Bloc des interfaces produites »).

## Prérequis

- Les plans 1 et 2 sont fusionnés sur la branche. Vérifier avant la Tâche 1 :
  ```bash
  grep -n "pub fn ensure_default_voice\|pub fn default_voice_to_install" src/models.rs
  grep -n "pub fn config_adjust_and_save\|pub fn save_field\|pub fn set_config_target\|fn report_save" src/app/settings.rs
  grep -n "pub fn copy_value" src/app/config.rs
  grep -n "pub fn read_from" src/config.rs
  grep -rn -i "bookmark" macos/Sources macos/Tests
  ```
  Attendu : les quatre premières commandes trouvent chacune leurs fonctions ; la dernière ne trouve **rien** (le plan 2 a retiré les signets de l'app). Si Swift utilise encore un signet, s'arrêter : le plan 2 n'est pas fusionné.
- Noms du plan 1 utilisés ici, tels quels : `models::ensure_default_voice(mlx: bool, running: Option<&Download>) -> anyhow::Result<Option<Download>>`, `Config::read_from(path) -> anyhow::Result<Config>`, `Config::save_to` (`pub(crate)`), `Field::copy_value(&self, from: &Config, to: &mut Config)`, `App::set_config_target(Option<PathBuf>)` (défaut `Config::path()` dans `from_parts`), `App::save_field(Field) -> anyhow::Result<PathBuf>`, `App::config_adjust_and_save(i32)`, `App::config_commit_edit_and_save()`, `App::report_save(Field)` (privée), champ `App.config_target`, `ReadingConfig.font` / `.size` (sans interface au terminal), et l'assistant de test `autosave_target(name) -> (PathBuf, PathBuf)` du module `app::tests`.

## Global Constraints

- Touches de lecture gardées (spec § 2) : **Espace, ← →, ↑ ↓, n/p, /, N/P, + −, t (sommaire), s (réglages), ? (aide), q (quitter), Échap (fermer panneau)**. Gardées en plus, faute de quoi le sommaire et le terminal deviennent inutilisables : **Entrée** (aller au titre choisi dans le sommaire) et **Ctrl+C** (quitter).
- Retirées : `b`, `B`, `d` (signets), `[` `]` `m` (volume), `1`–`4` (préréglages), alias `=`, `_`, `h`, `Tab`, et aussi `Maj+← →`, `g`, `G`, `Début`, `Fin`, `r`, `w`, `Pg↑`, `Pg↓`, `f`.
- L'aide (`?`) liste **toutes** les touches qui agissent dans l'écran courant, et aucune autre.
- Réglages : **une page, sans rubriques** — voix FR, voix EN, vitesse, largeur de colonne, interlignage, règle de lecture. **Enregistrement automatique à chaque changement**, `w` disparaît. `q` et `Échap` ferment l'écran **sans quitter**.
- Plus de rubrique Modèles : installation automatique au premier usage, progression visible.
- `sillon` sans argument dans un terminal : aide courte (les quatre manières de lire), puis sortie. `--config` ouvre toujours les réglages.
- `--help` montre `FILE`, `--clip`, `--speed`, `--voice`, `--to-markdown`, `--no-cache`, `--config` ; les autres options ont `hide = true` et restent acceptées.
- Les clés retirées de l'interface gardent leur valeur dans `config.toml` (rien n'est réécrit pour elles). `bookmarks.json` reste sur le disque, plus lu.
- Un `config.toml` illisible n'est jamais réécrit (garanti par `Config::read_from`, plan 1).
- Une option de la ligne de commande (`--speed`, `--voice`…) n'est jamais écrite dans `config.toml` par l'enregistrement automatique.
- Tests sans réseau ni fichiers réels : sous `cfg(test)`, `paths::config_dir` / `data_dir` / `cache_dir` sont des dossiers temporaires ; les téléchargements passent par `Download::fake` / `fake_ended`. Un test qui appellerait `models::ensure_default_voice` sans garde est marqué `#[cfg(not(feature = "kokoro-onnx"))]` (sinon il téléchargerait).
- Lint comme la CI : `cargo clippy --workspace --all-targets --locked -- -D warnings` et `cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`. MSRV : `cargo +1.88 check -p sillon --all-features --all-targets --locked`.
- Commits en français, style du dépôt (`feat(terminal): …`, `refactor(moteur): …`, `docs: …`), terminés par `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **`sillon --config` sans document** : `q` ou `Échap` ferment les réglages… et derrière il n'y a rien. Attendu : la main revient au terminal, pas un écran de lecture vide. Test : Tâche 5, `closing_the_settings_with_nothing_to_read_quits`.
2. **Changer la voix anglaise en lisant un document français** : rien ne s'entend de ce changement, la phrase en cours ne doit pas repartir du début. Test : Tâche 5, `a_voice_of_another_language_does_not_restart_the_sentence`.
3. **Binaire sans `kokoro-onnx` (`cargo install --path .`) ou `--engine system`** : rien ne se télécharge, aucune ligne « Installation de la voix » n'apparaît. Tests : Tâche 6, `the_system_voice_asked_for_installs_nothing` et `a_terminal_without_kokoro_installs_nothing`.
4. **Téléchargement de la voix qui échoue** (réseau coupé) : la voix du système continue de lire, l'échec se lit à l'écran, `config.toml` n'est pas touché, un nouvel essai a lieu au lancement suivant. Test : Tâche 6, `a_failed_install_says_so_and_writes_nothing`.
5. **`cat notes.md | sillon`** : l'entrée standard n'est pas un terminal ; l'aide courte ne doit **pas** s'afficher, la lecture doit démarrer. Test : Tâche 7, `nothing_to_read_only_from_a_bare_terminal`.

(Les deux risques « option de la ligne de commande écrite » et « fichier illisible écrasé » sont épinglés par les tests du plan 1, Tâche 5 ; ce plan les garde verts en les adaptant à la page unique, Tâche 5.)


## Amendements validés par l'utilisateur (15 oct. 2026) — priment sur les tâches

1. **Tâche 3 (vue figée retirée) est SUPPRIMÉE.** Pg↑, Pg↓ et `f` restent, avec `ViewAnchor`. L'aide (`HELP`) de la Tâche 2 les liste : « Pg↑ Pg↓ — relire sans déplacer la lecture », « f — reprendre le suivi ».
2. **Début est gardé** (Tâche 2) : la touche Début → aller au début du document, listée dans l'aide. `g`, `G` et Fin restent retirés. Adapter les tests de la Tâche 2 qui vérifient que Début n'a plus d'effet.

---

## Structure des fichiers

| Fichier | Rôle dans ce plan |
|---|---|
| `src/bookmark.rs` | **Supprimé** (Tâche 1). |
| `src/lib.rs` | `pub mod bookmark;` retiré. |
| `src/app/mod.rs` | `Panel::Bookmarks`, champ `bookmarks`, `config_dirty` retirés ; module `view` retiré (Tâche 3) ; tests. |
| `src/app/navigation.rs` | Signets, vue figée retirés ; sommaire seul. |
| `src/app/view.rs` | **Supprimé** (Tâche 3). |
| `src/app/keys.rs` | `HELP`, `CONFIG_HELP`, `handle_reader`, `handle_config` réduits ; tests. |
| `src/app/config.rs` | Réécrit (Tâche 5) : `Field` à six variantes, `FIELDS`, `ConfigScreen` sans rubriques. |
| `src/app/settings.rs` | Ouverture/fermeture de la page, enregistrement (vitesse, modèle), aperçu par langue, installation automatique. |
| `src/app/playback.rs` | `seek_to` sans vue figée. |
| `src/config.rs` | `Effect` réduit à `None`, `Resynthesize`, `Speed`. |
| `src/reader/{command,data,mod,view,migration}.rs` | Signets retirés ; `flush_session_config` retiré. |
| `src/tui/mod.rs` | Plus de `flush_config`. |
| `src/tui/ui/mod.rs` | Imports ; aide « Réglages » ; tests de rendu. |
| `src/tui/ui/player.rs` | Panneau Signets, volume, vue figée retirés ; ligne d'état : installation, rappel. |
| `src/tui/ui/config.rs` | Réécrit (Tâche 5) : page unique, lignes testables. |
| `src/launch.rs` | `hide = true`, `SHORT_HELP`, `nothing_to_read`, installation automatique, avis sans « `s` ». |
| `src/main.rs` | Lancement sans `has_input`. |
| `ffi/src/lib.rs` | Seulement si un reste de signet y subsiste (Tâche 1, étape 1). |
| `README.md` | Sections terminal (Tâche 8). |

---

### Task 1: Signets retirés du cœur et du terminal

**Files:**
- Delete: `src/bookmark.rs`
- Modify: `src/lib.rs:13`, `src/app/mod.rs` (import l. 19, `Panel` l. 36-40, champ l. 68, `from_parts` l. 215-217 et 235, test l. 898-911), `src/app/navigation.rs:1,116-123,200-225,245-302`, `src/app/keys.rs` (HELP, `handle_reader`, test `typing_a_query_never_reaches_the_player`), `src/tui/ui/player.rs:1,100-102,186-190,237-242,294-321`, `src/reader/command.rs:22,50`, `src/reader/mod.rs:323-360,765-803`, `src/reader/data.rs`, `src/reader/view.rs:69-70,233`, `src/launch.rs:201-202`
- Modify (conditionnel): `ffi/src/lib.rs`
- Test: `src/app/keys.rs` (module `tests`)

**Interfaces:**
- Consumes: rien.
- Produces: `Panel` n'a plus que `None` et `Contents` ; `App` n'a plus de champ `bookmarks` ni de méthodes `toggle_bookmark`, `bookmarked_here`, `delete_selection` ; `Reader` n'a plus `bookmarks`, `bookmarks_in`, `all_bookmarks`, `remove_bookmark` ; `reader::data` n'a plus `StoredBookmark` ni `document_label` ; `Snapshot` n'a plus `bookmarked` ; `Command` n'a plus `ToggleBookmark`.

- [ ] **Step 1: Vérifier ce qui reste au pont**

Run: `grep -n -i "bookmark" ffi/src/lib.rs`
Expected: rien (le plan 2 a tout retiré). **Si des lignes sortent**, les retirer toutes dans cette tâche : la variante `ReaderCommand::ToggleBookmark` et son bras `ReaderCommand::ToggleBookmark => Command::ToggleBookmark,` ; le champ `pub bookmarked: bool,` de `Snapshot` et la ligne `bookmarked: s.bookmarked,` de `snapshot_of` ; les records `BookmarkView` et `StoredBookmarkView` ; les méthodes `bookmarks`, `all_bookmarks`, `remove_bookmark` de `SillonReader` ; la fonction `stored_bookmark_of` ; la ligne `assert!(reader.remove_bookmark("x".into(), 0).is_err());` du test `the_silent_reader_writes_and_erases_nothing`. Puis l'import `data` s'il n'est plus utilisé (le compilateur le dira à l'étape 5).

- [ ] **Step 2: Write the failing test**

Dans `src/app/keys.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn bookmark_keys_do_nothing_any_more() {
        let mut app = app_reading(DOC);
        for c in ['b', 'B', 'd'] {
            press(&mut app, KeyCode::Char(c));
            assert_eq!(app.panel, Panel::None, "{c} a ouvert un panneau");
            assert!(app.status().is_empty(), "{c} a parlé : {:?}", app.status());
        }
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p sillon --lib keys::tests::bookmark_keys_do_nothing_any_more`
Expected: FAIL — « b a parlé : "Signet ajouté" ».

- [ ] **Step 4: Retirer les signets**

1. `git rm src/bookmark.rs` ; dans `src/lib.rs`, supprimer la ligne `pub mod bookmark;`.

2. `src/app/mod.rs` :
   - supprimer `use crate::bookmark::BookmarkStore;` ;
   - `Panel` devient :
     ```rust
     /// Panneau latéral affiché à droite du texte.
     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     pub enum Panel {
         None,
         Contents,
     }
     ```
   - supprimer le champ `pub bookmarks: BookmarkStore,` de `App` ;
   - dans `from_parts`, remplacer
     ```rust
             let (bookmarks, bookmark_notice) = BookmarkStore::load();
             let mut notices = notices;
             notices.extend(bookmark_notice);
     ```
     par
     ```rust
             let mut notices = notices;
     ```
     et supprimer la ligne `bookmarks,` du littéral `Self { … }` ;
   - dans le test `a_place_kept_under_the_bare_file_name_is_found_under_the_path`, supprimer les lignes `app.bookmarks.toggle("test.md", 1, 0, "Titre", "x");` et `assert!(app.bookmarks.contains("/docs/test.md", 1));`.

3. `src/app/navigation.rs` :
   - en-tête : `//! Se repérer dans le texte : vue, recherche, reprise, sommaire.`
   - `adopt_former_identity` devient :
     ```rust
         /// Rattache à ce document la position retenue sous un ancien nom.
         ///
         /// Les positions étaient rattachées au nom du fichier seul ; elles le
         /// sont désormais à son chemin. Rien de ce qui a été retenu ne se perd.
         pub fn adopt_former_identity(&mut self, former: &str) {
             self.positions.adopt(former, &self.source);
         }
     ```
   - supprimer `toggle_bookmark`, `bookmarked_here` et `delete_selection` ;
   - `move_selection` et `activate_selection` deviennent :
     ```rust
         /// Déplace la sélection dans le sommaire ouvert.
         pub fn move_selection(&mut self, delta: isize) {
             let count = match self.panel {
                 Panel::Contents => self.navigable_sections().len(),
                 Panel::None => 0,
             };
             if count == 0 {
                 return;
             }
             let next = (self.selection as isize + delta).clamp(0, count as isize - 1);
             self.selection = next as usize;
         }

         /// Saute au titre sélectionné dans le sommaire.
         pub fn activate_selection(&mut self) {
             if self.panel != Panel::Contents {
                 return;
             }
             let target = self
                 .navigable_sections()
                 .get(self.selection)
                 .and_then(|&id| self.doc().section(id))
                 .and_then(|s| s.first_sentence);
             if let Some(target) = target {
                 self.seek_to(target);
                 self.notify("Saut au titre");
             }
         }
     ```

4. `src/app/keys.rs` :
   - dans `HELP`, supprimer `("b", "Poser / retirer un signet"),` et remplacer `("t / B", "Sommaire / signets"),` par `("t", "Sommaire"),` ;
   - dans `handle_reader`, les bras des flèches verticales deviennent :
     ```rust
             KeyCode::Up => match app.panel {
                 Panel::Contents => app.move_selection(-1),
                 Panel::None => app.seek_block(-1),
             },
             KeyCode::Down => match app.panel {
                 Panel::Contents => app.move_selection(1),
                 Panel::None => app.seek_block(1),
             },
     ```
     et supprimer les lignes `KeyCode::Char('b') => app.toggle_bookmark(),`, `KeyCode::Char('B') => app.toggle_panel(Panel::Bookmarks),`, `KeyCode::Char('d') => app.delete_selection(),` ;
   - le commentaire en tête de `handle_reader` devient `// La saisie d'une recherche capte tout : sinon « q » quitterait au lieu de s'écrire.` ;
   - le test `typing_a_query_never_reaches_the_player` devient :
     ```rust
         #[test]
         fn typing_a_query_never_reaches_the_player() {
             let mut app = app_reading(DOC);
             press(&mut app, KeyCode::Char('/'));
             // « q » quitte en lecture : pendant une saisie, il ne doit
             // qu'écrire.
             for c in "tq".chars() {
                 press(&mut app, KeyCode::Char(c));
             }
             assert_eq!(app.search_input(), Some("tq"));
             assert_eq!(app.panel, Panel::None, "le sommaire s'est ouvert pendant la saisie");
             assert!(!app.should_quit, "le lecteur a quitté pendant la saisie");
         }
     ```

5. `src/tui/ui/player.rs` :
   - en-tête : `//! Barre de lecture, ligne d'état, sommaire et aide.`
   - supprimer le bloc `if app.bookmarked_here() { second.push(Span::styled("  ⚑ signet", …)); }` ;
   - dans le rappel permanent, remplacer `" aide · espace lecture · / chercher · n/p titre · b signet "` par `" aide · espace lecture · / chercher · n/p titre "` ;
   - `render_panel` commence par :
     ```rust
         let (title, lines) = match app.panel {
             Panel::Contents => ("Sommaire", contents_lines(app)),
             Panel::None => return,
         };
     ```
   - supprimer la fonction `bookmark_lines`.

6. `src/reader/command.rs` : supprimer la variante `ToggleBookmark,` et le bras `Command::ToggleBookmark => app.toggle_bookmark(),`.

7. `src/reader/mod.rs` :
   - la doc de `open_text` devient : `/// Ouvre un texte capturé (sélection, presse-papiers). Son identité — sous` / `/// laquelle sa position se range — est sa source suivie d'une empreinte du` / `/// texte : deux sélections ne partagent pas leur position, et le même texte,` / `/// recapturé, retrouve la sienne.` ;
   - supprimer `pub fn bookmarks(&self)` et `pub fn bookmarks_in(&self, …)` avec leurs commentaires ;
   - dans le module `tests`, supprimer la fonction `fn bookmark(document: &str, sentence_id: usize)` et le test `a_resting_text_reads_its_own_bookmarks` ; dans `each_captured_text_has_its_own_identity`, le message devient `"deux sélections ne partagent pas leur position"`.

8. `src/reader/data.rs` :
   - en-tête : `//! Ce que l'app efface : modèles, téléchargement en cours, cache des` / `//! documents.` (garder le second paragraphe) ;
   - supprimer `use crate::bookmark::BookmarkStore;`, la struct `StoredBookmark`, la fonction `document_label`, les méthodes `all_bookmarks`, `remove_bookmark`, `remove_bookmark_at` ;
   - module `tests` : dans `a_reader_off_disk_erases_nothing`, supprimer la ligne `assert_eq!(r.remove_bookmark("a", 0).unwrap_err().to_string(), TRIAL);` ; supprimer `open_isolated`, `scratch_file`, `removing_a_bookmark_also_forgets_it_in_the_open_session`, `the_open_session_lists_its_bookmarks_with_a_readable_name`, `documents_are_named_for_the_eye`.

9. `src/reader/view.rs` : supprimer `/// Vrai quand la phrase courante porte un signet.` / `pub bookmarked: bool,` et la ligne `bookmarked: app.bookmarked_here(),`.

10. `src/launch.rs`, dans `prepare` : le commentaire `// Positions et signets ont pu être retenus sous le nom seul, par une` / `// version antérieure : ils suivent le document.` devient `// Une position a pu être retenue sous le nom seul, par une version` / `// antérieure : elle suit le document.`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS, dont `bookmark_keys_do_nothing_any_more`.

Run: `cargo build -p sillon-ffi --locked && grep -rn -i "bookmark\|signet" --include='*.rs' src ffi | grep -v "^src/paths.rs\|^src/position.rs\|^src/doc/mod.rs\|^tests/"`
Expected: le pont compile ; le `grep` ne sort que des commentaires sans rapport avec une fonction (aucune). `src/paths.rs` garde `bookmarks.json` dans son test de déménagement du dossier de données : c'est un nom de fichier, on n'y touche pas.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: aucun avertissement.

- [ ] **Step 6: Commit**

```bash
git add -A src ffi
git commit -m "$(cat <<'EOF'
refactor(moteur): signets retirés du cœur et du terminal — bookmark.rs, Command::ToggleBookmark, Panel::Bookmarks, Snapshot.bookmarked, touches b, B et d ; bookmarks.json reste sur le disque, plus lu

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Clavier de lecture réduit, aide et rappel à jour

**Files:**
- Modify: `src/app/keys.rs` (`HELP`, `handle_reader`, tests)
- Modify: `src/app/navigation.rs` (`resume_where_left`, message de reprise)
- Modify: `src/tui/ui/player.rs` (volume retiré de la barre, constante `REMINDER`)
- Test: `src/app/keys.rs`, `src/tui/ui/mod.rs`

**Interfaces:**
- Consumes: `Panel` à deux variantes (Tâche 1).
- Produces: `pub const HELP: &[(&str, &str)]` (liste ci-dessous) ; `pub(super) const REMINDER: &str` dans `src/tui/ui/player.rs`. `handle_reader` ne lit plus `key.shift`.

- [ ] **Step 1: Write the failing tests**

Dans `src/app/keys.rs`, module `tests`, **remplacer** le test `shift_with_an_arrow_seeks_by_the_configured_step` par les tests suivants :

```rust
    /// Ce qui ne doit pas bouger quand une touche retirée est pressée.
    fn assert_untouched(app: &App, key: &str, volume: f32) {
        assert_eq!(app.current_sentence(), 1, "{key} a déplacé la lecture");
        assert_eq!(app.config.voice.speed, 1.0, "{key} a changé la vitesse");
        assert_eq!(app.player.volume(), volume, "{key} a changé le volume");
        assert!(!app.player.is_muted(), "{key} a coupé le son");
        assert_eq!(app.panel, Panel::None, "{key} a ouvert un panneau");
        assert!(!app.show_help, "{key} a ouvert l'aide");
        assert!(!app.should_quit, "{key} a quitté");
        assert!(app.status().is_empty(), "{key} a parlé : {:?}", app.status());
    }

    #[test]
    fn the_keys_taken_out_do_nothing_any_more() {
        let mut app = app_reading(DOC);
        app.seek_to(1);
        let volume = app.player.volume();
        for c in ['[', ']', 'm', '1', '2', '3', '4', 'g', 'G', 'r', 'w', '=', '_', 'h'] {
            press(&mut app, KeyCode::Char(c));
            assert_untouched(&app, &c.to_string(), volume);
        }
        for code in [KeyCode::Home, KeyCode::End, KeyCode::Tab] {
            press(&mut app, code);
            assert_untouched(&app, &format!("{code:?}"), volume);
        }
    }

    #[test]
    fn shift_with_an_arrow_moves_one_sentence_like_the_arrow() {
        let mut app = app_reading(DOC);
        app.config.reading.seek_step = 2;
        handle(&mut app, Key { code: KeyCode::Right, shift: true, ctrl: false });
        assert_eq!(app.current_sentence(), 1, "plus de saut de plusieurs phrases");
    }

    #[test]
    fn escape_closes_the_contents_then_quits() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.panel, Panel::Contents);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.panel, Panel::None, "Échap ferme d'abord le sommaire");
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        assert!(app.should_quit, "rien d'ouvert : Échap quitte");
    }

    #[test]
    fn q_quits_even_with_the_contents_open() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn the_help_names_every_reading_key_and_no_other() {
        let keys: Vec<&str> = HELP.iter().map(|(k, _)| *k).collect();
        for kept in ["Espace", "← →", "↑ ↓", "n / p", "/", "N / P", "+ / -", "t", "s", "?", "Échap", "q"] {
            assert!(keys.contains(&kept), "{kept} manque à l'aide : {keys:?}");
        }
        for gone in ["Maj+← →", "g / Début · Fin", "r", "1 2 3 4", "[ / ]", "m", "b", "t / B", "? / h", "q / Échap"] {
            assert!(!keys.contains(&gone), "{gone} est encore dans l'aide");
        }
    }
```

Dans `src/tui/ui/mod.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn the_reminder_names_the_contents_and_no_bookmark() {
        assert!(REMINDER.contains("t sommaire"), "{REMINDER}");
        assert!(!REMINDER.contains("signet"), "{REMINDER}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib keys::tests`
Expected: FAIL — `the_keys_taken_out_do_nothing_any_more` (« [ a changé le volume »), `shift_with_an_arrow_moves_one_sentence_like_the_arrow` (2 ≠ 1), `q_quits_even_with_the_contents_open`, `the_help_names_every_reading_key_and_no_other`.

Run: `cargo test -p sillon --lib ui::tests::the_reminder`
Expected: FAIL à la compilation — « cannot find value `REMINDER` ».

- [ ] **Step 3: Implémenter**

`src/app/keys.rs`, `HELP` devient (les deux lignes `Pg↑ Pg↓` et `f` restent jusqu'à la Tâche 3) :

```rust
/// Aide de la vue de lecture : toutes les touches qui y agissent, et
/// seulement elles.
pub const HELP: &[(&str, &str)] = &[
    ("Espace", "Lecture / pause"),
    ("← →", "Phrase précédente / suivante"),
    ("↑ ↓", "Bloc précédent / suivant"),
    ("n / p", "Titre suivant / précédent"),
    ("/", "Chercher dans le document"),
    ("N / P", "Résultat suivant / précédent"),
    ("+ / -", "Vitesse"),
    ("t", "Sommaire : ↑ ↓ puis Entrée"),
    ("s", "Réglages"),
    ("Pg↑ Pg↓", "Défiler sans déplacer la lecture"),
    ("f", "Revenir à la lecture"),
    ("?", "Aide"),
    ("Échap", "Fermer le sommaire, sinon quitter"),
    ("q", "Quitter"),
];
```

`handle_reader`, après le bloc de saisie de recherche (inchangé), devient :

```rust
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            // Échap referme d'abord le sommaire ; rien d'ouvert, il quitte.
            if app.panel != Panel::None {
                app.panel = Panel::None;
            } else {
                app.should_quit = true;
            }
        }

        KeyCode::Char(' ') => app.toggle_play(),

        KeyCode::Right => app.seek_sentences(1),
        KeyCode::Left => app.seek_sentences(-1),

        // Les flèches verticales pilotent le sommaire ouvert, sinon le texte.
        KeyCode::Up => match app.panel {
            Panel::Contents => app.move_selection(-1),
            Panel::None => app.seek_block(-1),
        },
        KeyCode::Down => match app.panel {
            Panel::Contents => app.move_selection(1),
            Panel::None => app.seek_block(1),
        },

        KeyCode::Char('n') => app.seek_section(1),
        KeyCode::Char('p') => app.seek_section(-1),

        KeyCode::Char('+') => app.change_speed(0.1),
        KeyCode::Char('-') => app.change_speed(-0.1),

        KeyCode::Char('t') => app.toggle_panel(Panel::Contents),
        KeyCode::Char('s') => app.toggle_config(),
        KeyCode::Char('f') => app.toggle_follow(),

        // Défiler fige la vue : on relit sans interrompre la voix.
        KeyCode::PageUp => app.scroll_view(-VIEW_STEP),
        KeyCode::PageDown => app.scroll_view(VIEW_STEP),

        KeyCode::Char('/') => app.search_open(),
        KeyCode::Char('N') => app.search_advance(1),
        KeyCode::Char('P') => app.search_advance(-1),

        KeyCode::Enter => app.activate_selection(),
        KeyCode::Char('?') => app.show_help = true,

        _ => {}
    }
```

(supprimer la ligne `let step = app.config.reading.seek_step as isize;`.)

`src/app/navigation.rs`, dans `resume_where_left`, remplacer `self.notify("Reprise là où vous en étiez — g revient au début");` par `self.notify("Reprise là où vous en étiez");`.

`src/tui/ui/player.rs` :
- dans `render_player`, supprimer de `second` le `Span` du volume (le bloc `Span::styled(if app.player.is_muted() { "  🔇 coupé ".to_string() } else { format!("  🔊 {:>3.0} % ", …) }, …)`) : plus aucune touche du terminal ne le règle ;
- ajouter, après `use super::*;` :
  ```rust
  /// Le rappel permanent de la ligne d'état, après « s réglages · ? » : les
  /// touches qu'un nouveau venu cherche d'abord.
  pub(super) const REMINDER: &str = " aide · espace lecture · ← → phrase · / chercher · t sommaire ";
  ```
- dans `status_spans`, remplacer le `Span::styled(" aide · espace lecture · / chercher · n/p titre ", Style::default().fg(FAINT))` par `Span::styled(REMINDER, Style::default().fg(FAINT))`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/keys.rs src/app/navigation.rs src/tui/ui/player.rs src/tui/ui/mod.rs
git commit -m "$(cat <<'EOF'
feat(terminal): clavier de lecture réduit — volume, préréglages 1 à 4, Maj+flèches, g/G/Début/Fin, r, w et alias (=, _, h, Tab) retirés ; q quitte, Échap ferme le sommaire puis quitte ; aide et rappel à jour

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Vue figée retirée (Pg↑, Pg↓, f)

Tâche **séparable** : la spec ne garde pas ces touches ; si l'utilisateur décide de les garder (voir fin de rendu), sauter cette tâche entière, rien d'autre n'en dépend.

**Files:**
- Delete: `src/app/view.rs`
- Modify: `src/app/mod.rs` (`pub mod view;`, `use crate::app::view::ViewAnchor;`, champ `view`, son initialisation, tests `scrolling_the_view_leaves_the_voice_where_it_was` et `seeking_snaps_the_view_back_onto_the_voice`)
- Modify: `src/app/navigation.rs` (`anchor_block`, `scroll_view`, `follow_reading`, `toggle_follow`)
- Modify: `src/app/playback.rs` (`seek_to`)
- Modify: `src/app/keys.rs` (`VIEW_STEP`, `HELP`, `handle_reader`, test `paging_scrolls_the_view_and_leaves_the_voice_alone`)
- Modify: `src/tui/ui/player.rs` (`status_spans`), `src/tui/ui/mod.rs` (test `the_status_line_keeps_saying_that_the_view_is_frozen`)

**Interfaces:**
- Consumes: `HELP` (Tâche 2).
- Produces: `App::anchor_block(&self) -> usize` (inchangée de signature) rend toujours le bloc lu. Plus de `App.view`, `scroll_view`, `follow_reading`, `toggle_follow`.

- [ ] **Step 1: Write the failing test**

Dans `src/app/keys.rs`, module `tests`, **remplacer** `paging_scrolls_the_view_and_leaves_the_voice_alone` par :

```rust
    #[test]
    fn page_keys_and_f_no_longer_move_the_view() {
        let mut app = app_reading(DOC);
        app.seek_to(app.doc().sentences.len() - 1);
        let anchor = app.anchor_block();
        for code in [KeyCode::PageUp, KeyCode::PageDown, KeyCode::Char('f')] {
            press(&mut app, code);
            assert_eq!(app.anchor_block(), anchor, "{code:?} : la vue suit toujours la voix");
            assert!(app.status().is_empty(), "{code:?} a parlé : {:?}", app.status());
        }
    }
```

et dans `the_help_names_every_reading_key_and_no_other`, ajouter `"Pg↑ Pg↓", "f"` à la liste `gone`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sillon --lib keys::tests`
Expected: FAIL — « PageUp : la vue suit toujours la voix » et « Pg↑ Pg↓ est encore dans l'aide ».

- [ ] **Step 3: Implémenter**

1. `git rm src/app/view.rs`. Dans `src/app/mod.rs`, supprimer `pub mod view;`, `use crate::app::view::ViewAnchor;`, le champ `/// Position de la vue, …` / `pub view: ViewAnchor,`, la ligne `view: ViewAnchor::default(),`, et les tests `scrolling_the_view_leaves_the_voice_where_it_was` et `seeking_snaps_the_view_back_onto_the_voice`.

2. `src/app/navigation.rs` : supprimer `scroll_view`, `follow_reading`, `toggle_follow` ; `anchor_block` devient :
   ```rust
       /// Bloc autour duquel dessiner : celui que la voix lit.
       pub fn anchor_block(&self) -> usize {
           self.doc()
               .sentence(self.current_sentence())
               .map(|s| s.block_id)
               .unwrap_or(0)
       }
   ```

3. `src/app/playback.rs`, fin de `seek_to` : supprimer les trois lignes
   ```rust
           // Un saut de lecture ramène toujours la vue : on veut voir ce qu'on
           // vient de rejoindre.
           self.view.follow();
   ```

4. `src/app/keys.rs` : supprimer `const VIEW_STEP: isize = 4;`, les lignes `("Pg↑ Pg↓", …)` et `("f", …)` de `HELP`, et dans `handle_reader` les lignes `KeyCode::Char('f') => app.toggle_follow(),`, le commentaire `// Défiler fige la vue …` et les deux bras `KeyCode::PageUp` / `KeyCode::PageDown`.

5. `src/tui/ui/player.rs`, `status_spans` : supprimer la branche `if !app.view.is_following() { … " ⏸ vue figée " … }` ; la chaîne commence alors par `if !app.status().is_empty() {`. Supprimer le commentaire « La vue figée est un état… ».

6. `src/tui/ui/mod.rs` : supprimer le test `the_status_line_keeps_saying_that_the_view_is_frozen`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: PASS, aucun avertissement.

- [ ] **Step 5: Commit**

```bash
git add -A src
git commit -m "$(cat <<'EOF'
feat(terminal): vue figée retirée — Pg↑, Pg↓ et f disparaissent, la vue suit toujours la voix (ViewAnchor supprimé)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Enregistrement automatique branché, `w` et `config_dirty` retirés

**Files:**
- Modify: `src/app/keys.rs` (`CONFIG_HELP`, `handle_config`, `handle_reader` `+`/`-`, tests)
- Modify: `src/app/settings.rs` (`config_activate`, `select_model`, `save_config`, `config_needs_saving`, `config_saved`, `mark_config_dirty`, `flush_config` ; ajouts `nudge_speed`, `save_model_choice`)
- Modify: `src/app/mod.rs` (champ `config_dirty`, tests ; `autosave_target` rendu `pub(crate)`)
- Modify: `src/tui/mod.rs:28-31,62-65`, `src/tui/ui/config.rs` (pied de page)
- Modify: `src/reader/mod.rs` (`flush_session_config`, `rest`, `close`), `src/reader/migration.rs:586`

**Interfaces:**
- Consumes (plan 1) : `App::config_adjust_and_save(i32)`, `App::config_commit_edit_and_save()`, `App::report_save(Field)`, `App.config_target`, `Config::read_from`, `Config::save_to`, `autosave_target`.
- Produces :
  - `impl App { pub fn nudge_speed(&mut self, delta: f32) }` : `+` / `-` en lecture, vitesse appliquée et enregistrée.
  - `impl App { fn save_model_choice(&self) -> anyhow::Result<()> }` (privée) : écrit moteur, chemin, famille et voix par langue du modèle choisi, sur le fichier relu.
  - Plus de `config_dirty`, `save_config`, `config_needs_saving`, `config_saved`, `mark_config_dirty`, `flush_config` ; plus de `Reader::flush_session_config`.

- [ ] **Step 1: Write the failing tests**

Dans `src/app/mod.rs`, module `tests` : rendre l'assistant du plan 1 visible des autres modules de test — `fn autosave_target(` devient `pub(crate) fn autosave_target(`. Dans `a_setting_changed_on_screen_is_written_at_once` (plan 1), supprimer la ligne `assert!(!app.config_needs_saving());`. **Remplacer** le test `choosing_a_model_asks_for_the_configuration_to_be_written` par :

```rust
    #[test]
    fn choosing_a_model_is_written_at_once() {
        // Le choix ne survivait au relancement que si `w` l'écrivait.
        let (dir, path) = autosave_target("modele");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        let package = models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap();
        app.select_model(package);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.engine.family, "kokoro");
        assert!(written.engine.model_path.ends_with("model_quantized.onnx"), "{}", written.engine.model_path);
        assert_eq!(written.voice.per_language["fr"], "p-fenrir60", "les voix du modèle suivent");
        assert!(written.reading.autoplay, "l'autoplay coupé du test n'est pas écrit");
    }

    #[test]
    fn the_speed_set_while_reading_is_written_at_once() {
        let (dir, path) = autosave_target("vitesse");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.config.voice.pitch = 0.5; // comme `--pitch 0.5`
        app.nudge_speed(0.1);
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!((written.voice.speed - 1.1).abs() < 1e-5, "{}", written.voice.speed);
        assert_eq!(written.voice.pitch, 0.0, "l'option de la ligne de commande n'est pas écrite");
    }
```

Dans `src/app/keys.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn w_writes_nothing_any_more_in_the_settings() {
        let mut app = app_reading(DOC);
        app.show_config = true;
        press(&mut app, KeyCode::Char('w'));
        assert!(app.status().is_empty(), "w a écrit : {:?}", app.status());
    }

    #[test]
    fn the_arrows_of_the_settings_write_at_once() {
        let (dir, path) = crate::app::tests::autosave_target("fleches");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.show_config = true;
        app.screen.goto_section(2); // Lecture
        app.screen.row = 1; // Interlignage
        let before = app.config.reading.line_spacing;
        press(&mut app, KeyCode::Right);
        let written = crate::config::Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(written.reading.line_spacing, before + 1);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib`
Expected: FAIL à la compilation — « no method named `nudge_speed` ». (Après l'étape 3, `w_writes_nothing…` et `the_arrows_of_the_settings_write_at_once` échoueraient aussi sur l'ancien clavier.)

- [ ] **Step 3: Implémenter**

1. `src/app/settings.rs` :
   - dans `config_activate`, la branche finale `self.config_adjust(1);` devient `self.config_adjust_and_save(1);` ;
   - dans `select_model`, remplacer
     ```rust
             // Le choix doit survivre au relancement : sans écriture, le premier
             // modèle installé revenait à la place de celui qu'on avait pris.
             self.config_dirty = true;
     ```
     par rien, et ajouter **à la fin** de `select_model`, après l'affectation de `self.screen.notice` :
     ```rust
             // Le choix doit survivre au relancement : il s'écrit tout de suite.
             if let Err(err) = self.save_model_choice() {
                 self.notify(&format!("Modèle choisi, non enregistré : {err:#}"));
             }
     ```
   - supprimer `save_config`, `config_needs_saving`, `config_saved`, `mark_config_dirty` (et son `#[cfg(test)]`), `flush_config` ;
   - ajouter, après `config_commit_edit_and_save` (plan 1) :
     ```rust
         /// `+` et `-` pendant la lecture : la vitesse change sur-le-champ, et
         /// s'enregistre comme si elle avait été réglée dans la page.
         pub fn nudge_speed(&mut self, delta: f32) {
             self.change_speed(delta);
             self.report_save(Field::Speed);
         }

         /// Écrit le modèle choisi — moteur, chemin, famille, et la voix de
         /// chaque langue qu'il apporte — sur le fichier relu : rien d'autre ne
         /// bouge, et un fichier illisible n'est jamais réécrit.
         fn save_model_choice(&self) -> anyhow::Result<()> {
             let path = self
                 .config_target
                 .clone()
                 .ok_or_else(|| anyhow::anyhow!("répertoire de configuration introuvable"))?;
             let mut on_disk = Config::read_from(&path)?;
             on_disk.engine.engine = self.config.engine.engine.clone();
             on_disk.engine.model_path = self.config.engine.model_path.clone();
             on_disk.engine.family = self.config.engine.family.clone();
             on_disk.align_voice_with_family();
             on_disk.save_to(&path)
         }
     ```

2. `src/app/mod.rs` : supprimer le champ `/// Un choix fait à l'écran attend d'être écrit dans `config.toml`.` / `config_dirty: bool,` et la ligne `config_dirty: false,`.

3. `src/app/keys.rs` :
   - `CONFIG_HELP` : supprimer `("w", "Enregistrer la configuration"),` et remplacer `("← →", "Modifier une valeur"),` par `("← →", "Modifier une valeur (enregistrée aussitôt)"),` ;
   - `handle_config` : dans la saisie, `KeyCode::Enter => app.config_commit_edit(),` devient `KeyCode::Enter => app.config_commit_edit_and_save(),` ; `KeyCode::Right => app.config_adjust(1),` devient `KeyCode::Right => app.config_adjust_and_save(1),` ; `KeyCode::Left => app.config_adjust(-1),` devient `KeyCode::Left => app.config_adjust_and_save(-1),` ; supprimer `KeyCode::Char('w') => app.save_config(),` ;
   - `handle_reader` : `KeyCode::Char('+') => app.change_speed(0.1),` devient `KeyCode::Char('+') => app.nudge_speed(0.1),` et `KeyCode::Char('-') => app.change_speed(-0.1),` devient `KeyCode::Char('-') => app.nudge_speed(-0.1),` ;
   - `HELP` : `("+ / -", "Vitesse"),` devient `("+ / -", "Vitesse (enregistrée)"),`.

4. `src/tui/ui/config.rs`, `render_config_footer` : la chaîne `"↑↓ réglage · ←→ valeur · Entrée éditer · a tout voir · w enregistrer · Échap menu"` devient `"↑↓ réglage · ←→ valeur, enregistrée aussitôt · Entrée éditer · a tout voir · Échap menu"`.

5. `src/tui/mod.rs` : dans `run`, supprimer `app.flush_config();` ; dans `event_loop`, supprimer le commentaire `// Un choix de modèle s'écrit tout de suite : …` et `app.flush_config();`.

6. `src/reader/mod.rs` : supprimer la fonction `flush_session_config` et sa doc ; dans `rest`, supprimer `Self::flush_session_config(&mut app, self.config_unreadable, self.config_path.as_deref());` ; `close` devient :
   ```rust
       /// Fermeture de l'app : rien de ce qui a été lu ne se perd.
       pub fn close(&mut self) {
           if let Some(app) = self.session.as_mut() {
               app.remember_position();
           }
       }
   ```
   (les réglages de l'app s'écrivent déjà à chaque `update_settings` ; seul un modèle pris au terminal passait par là.)

7. `src/reader/migration.rs` : dans le test du témoin illisible, supprimer `r.with_session(t0, |app| app.mark_config_dirty());`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS, dont `choosing_a_model_is_written_at_once`, `the_speed_set_while_reading_is_written_at_once`, `w_writes_nothing_any_more_in_the_settings`, `the_arrows_of_the_settings_write_at_once`, et les tests d'enregistrement du plan 1.

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: PASS, aucun avertissement.

- [ ] **Step 5: Commit**

```bash
git add src/app src/tui src/reader
git commit -m "$(cat <<'EOF'
feat(terminal): chaque réglage s'enregistre dès qu'il change — flèches et Entrée sur config_adjust_and_save, + et - enregistrent la vitesse, le modèle choisi s'écrit aussitôt ; w, config_dirty et flush_config retirés

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Réglages sur une page

**Files:**
- Rewrite: `src/app/config.rs`
- Rewrite: `src/tui/ui/config.rs`
- Modify: `src/app/settings.rs` (page, aperçu, effets), `src/app/keys.rs` (`CONFIG_HELP`, `handle_config`, `s` en lecture, tests), `src/app/mod.rs` (import, tests du plan 1 adaptés), `src/config.rs` (`Effect`), `src/tui/ui/mod.rs` (import, titre de l'aide, test), `src/main.rs` (pause à l'ouverture)

**Interfaces:**
- Consumes (plan 1) : `Field::copy_value`, `App::config_adjust_and_save`, `App::report_save`, `App::save_field`.
- Produces :
  - `pub enum Field { VoiceFr, VoiceEn, Speed, LineWidth, LineSpacing, ReadingRuler }` ; `pub const FIELDS: [Field; 6]` ; `Field::label(&self) -> &'static str`, `Field::language(&self) -> Option<&'static str>`, `Field::hint(&self, &Config) -> &'static str`, `Field::value(&self, &Config) -> String`, `Field::adjust(&self, &mut Config, i32) -> Effect`, `Field::copy_value(&self, &Config, &mut Config)`.
  - `pub struct ConfigScreen { pub row: usize, pub download: Option<models::Download>, pub notice: String }` ; `ConfigScreen::current_field(&self) -> Field`, `ConfigScreen::select(&mut self, Field)`, `ConfigScreen::move_row(&mut self, i32)`, `ConfigScreen::poll_download(&mut self) -> Option<&'static ModelPackage>` (inchangée).
  - `App::open_config(&mut self)` (sans pause), `App::close_config(&mut self)`, `App::config_move_row(&mut self, i32)`, `App::config_adjust_and_save(&mut self, i32)` (signature inchangée), `App::effect_in_document(&self, Field, Effect) -> Effect` (`pub(super)`), `App::preview_voice(&mut self)`, `App::preview_label(&self, language: &str) -> String` (`pub(super)`).
  - `src/tui/ui/config.rs` : `config_lines(app, width) -> Vec<Line<'static>>`, `config_footer(app) -> Vec<Line<'static>>` (`pub(super)`).
  - Retirés : `Section`, `Page`, `ConfigScreen::{section, page, show_all, editing, …}`, `App::{toggle_config, config_move_section, config_goto_section, config_open_section, config_back_to_menu, config_toggle_show_all, config_activate, config_edit_push, config_edit_backspace, config_commit_edit, config_commit_edit_and_save, config_set_text, config_cancel_edit, activate_model, config_delete, request_preview, config_adjust}`, `Effect::{Volume, ReloadEngine, Reparse}`.

- [ ] **Step 1: Write the failing tests**

Ces tests sont écrits contre la nouvelle interface : ils échouent à la compilation tant que l'étape 3 n'est pas faite.

Dans `src/app/keys.rs`, module `tests`, **remplacer** `w_writes_nothing_any_more_in_the_settings` et `the_arrows_of_the_settings_write_at_once` (Tâche 4) par :

```rust
    #[test]
    fn q_and_escape_close_the_settings_without_quitting() {
        for code in [KeyCode::Char('q'), KeyCode::Esc, KeyCode::Char('s')] {
            let mut app = app_reading(DOC);
            press(&mut app, KeyCode::Char('s'));
            assert!(app.show_config, "s ouvre les réglages");
            press(&mut app, code);
            assert!(!app.show_config, "{code:?} ferme les réglages");
            assert!(!app.should_quit, "{code:?} ne quitte pas");
        }
    }

    #[test]
    fn closing_the_settings_with_nothing_to_read_quits() {
        // `sillon --config` seul : derrière la page, il n'y a rien.
        let mut app = app_reading("");
        app.open_config();
        press(&mut app, KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn w_writes_nothing_any_more_in_the_settings() {
        let mut app = app_reading(DOC);
        app.open_config();
        press(&mut app, KeyCode::Char('w'));
        assert!(app.status().is_empty(), "w a écrit : {:?}", app.status());
    }

    #[test]
    fn the_arrows_of_the_settings_write_at_once() {
        let (dir, path) = crate::app::tests::autosave_target("fleches");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.open_config();
        app.screen.select(Field::LineSpacing);
        let before = app.config.reading.line_spacing;
        press(&mut app, KeyCode::Right);
        let after_right = crate::config::Config::read_from(&path).unwrap().reading.line_spacing;
        press(&mut app, KeyCode::Enter);
        let after_enter = crate::config::Config::read_from(&path).unwrap().reading.line_spacing;
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(after_right, before + 1, "→ écrit");
        assert_eq!(after_enter, before + 2, "Entrée avance d'un cran et écrit");
    }

    #[test]
    fn the_settings_help_lists_only_what_acts_there() {
        let keys: Vec<&str> = CONFIG_HELP.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["↑ ↓", "← → · Entrée", "t", "q · Échap · s", "?"]);
    }
```

et en tête du module `tests` de `keys.rs`, ajouter `use crate::app::config::Field;`.

Dans `src/app/mod.rs`, module `tests` :
- l'assistant du plan 1 `on_line_spacing` devient :
  ```rust
      /// La page des réglages, sur « Interlignage ».
      fn on_line_spacing(app: &mut App) {
          app.screen.select(Field::LineSpacing);
          assert_eq!(app.screen.current_field(), Field::LineSpacing);
      }
  ```
- supprimer `a_text_field_is_written_when_its_edit_is_committed` (plus aucun champ ne se tape) ;
- supprimer `a_text_setting_reloads_the_engine_only_when_it_names_the_model` ;
- ajouter :
  ```rust
      #[test]
      fn a_voice_of_another_language_does_not_restart_the_sentence() {
          let app = app_reading(DOC); // document français
          assert_eq!(app.effect_in_document(Field::VoiceEn, Effect::Resynthesize), Effect::None);
          assert_eq!(app.effect_in_document(Field::VoiceFr, Effect::Resynthesize), Effect::Resynthesize);
          assert_eq!(app.effect_in_document(Field::Speed, Effect::Speed), Effect::Speed);
      }

      #[test]
      fn the_preview_names_the_voice_of_its_language() {
          let mut app = app_reading(DOC);
          app.config.engine.family = "kokoro".into();
          assert!(app.preview_label("en").contains("af_bella"), "{}", app.preview_label("en"));
          assert!(app.preview_label("fr").contains("p-fenrir60"), "{}", app.preview_label("fr"));
      }
  ```

Dans `src/tui/ui/mod.rs`, module `tests`, ajouter :

```rust
    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.to_string()).collect()).collect()
    }

    #[test]
    fn the_settings_are_one_page_of_six_lines() {
        let app = crate::app::tests::app_reading("Une phrase.\n");
        let page = texts(&config_lines(&app, 80));
        for field in FIELDS {
            assert!(page.iter().any(|l| l.contains(field.label())), "{field:?} manque : {page:?}");
        }
        for gone in ["Modèles", "Moteur", "Hauteur", "Blocs de code", "Langue"] {
            assert!(!page.iter().any(|l| l.contains(gone)), "{gone} est encore là : {page:?}");
        }
        let footer = texts(&config_footer(&app)).join(" ");
        assert!(footer.contains("enregistrée aussitôt"), "{footer}");
        assert!(!footer.contains(" w "), "{footer}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib`
Expected: FAIL à la compilation — « no method named `select` found for struct `ConfigScreen` », « cannot find value `FIELDS` ».

- [ ] **Step 3: Réécrire `src/app/config.rs`**

Contenu complet :

```rust
//! La page des réglages du terminal : six réglages, sans rubriques, chacun
//! enregistré dès qu'il change.
//!
//! Le reste de `config.toml` (moteur, fils de calcul, blocs de code, langue,
//! hauteur…) ne s'affiche plus : il garde sa valeur, et se règle à la main
//! pour qui y tient.

use crate::config::{Config, Effect};
use crate::models;

/// Un réglage de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    VoiceFr,
    VoiceEn,
    Speed,
    LineWidth,
    LineSpacing,
    ReadingRuler,
}

/// Les réglages, dans l'ordre de la page.
pub const FIELDS: [Field; 6] = [
    Field::VoiceFr,
    Field::VoiceEn,
    Field::Speed,
    Field::LineWidth,
    Field::LineSpacing,
    Field::ReadingRuler,
];

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Field::VoiceFr => "Voix française",
            Field::VoiceEn => "Voix anglaise",
            Field::Speed => "Vitesse",
            Field::LineWidth => "Largeur de colonne",
            Field::LineSpacing => "Interlignage",
            Field::ReadingRuler => "Règle de lecture",
        }
    }

    /// La langue d'une ligne de voix.
    pub fn language(&self) -> Option<&'static str> {
        match self {
            Field::VoiceFr => Some("fr"),
            Field::VoiceEn => Some("en"),
            _ => None,
        }
    }

    /// L'explication affichée sous la page, pour la ligne choisie.
    pub fn hint(&self, config: &Config) -> &'static str {
        match self {
            Field::VoiceFr | Field::VoiceEn if models::voices_follow_language(&config.engine.family) => {
                "← → pour changer de voix, t pour l'écouter. Chaque document parle avec la voix de sa langue."
            }
            Field::VoiceFr | Field::VoiceEn => "La voix du système lit tant que Kokoro n'est pas installé.",
            Field::Speed => {
                "Appliquée au son, sans resynthèse : la voix accélère sans monter dans les aigus. \
                 t pour l'entendre."
            }
            Field::LineWidth => "Une ligne trop large fait perdre le début de la suivante.",
            Field::LineSpacing => "Lignes vides entre deux lignes de texte.",
            Field::ReadingRuler => "Souligne d'un fond la ligne en cours de lecture.",
        }
    }

    /// Valeur affichée.
    pub fn value(&self, config: &Config) -> String {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                if !models::voices_follow_language(&config.engine.family) {
                    return "voix du système".to_string();
                }
                let language = self.language().expect("une ligne de voix a sa langue");
                let id = config.tts_params_in(language).voice;
                models::voices_for_family(&config.engine.family)
                    .iter()
                    .find(|v| v.id == id)
                    .map_or_else(|| id.clone(), |v| v.label.to_string())
            }
            Field::Speed => format!("{:.2}×", config.voice.speed),
            Field::LineWidth => match config.reading.line_width {
                0 => "plein écran".to_string(),
                n => format!("{n} caractères"),
            },
            Field::LineSpacing => match config.reading.line_spacing {
                0 => "aucun".to_string(),
                1 => "une ligne".to_string(),
                n => format!("{n} lignes"),
            },
            Field::ReadingRuler => if config.reading.reading_ruler { "oui" } else { "non" }.to_string(),
        }
    }

    /// Applique un pas de réglage. Renvoie ce qui doit être répercuté.
    pub fn adjust(&self, config: &mut Config, delta: i32) -> Effect {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                let language = self.language().expect("une ligne de voix a sa langue");
                let family = config.engine.family.clone();
                if !models::voices_follow_language(&family) {
                    return Effect::None;
                }
                let own: Vec<&models::VoiceOption> =
                    models::voices_for_family(&family).iter().filter(|v| v.language == language).collect();
                if own.is_empty() {
                    return Effect::None;
                }
                let current = config.tts_params_in(language).voice;
                let index = own.iter().position(|v| v.id == current).unwrap_or(0) as i32;
                let next = own[(index + delta).rem_euclid(own.len() as i32) as usize];
                // Une seule voix dans la langue (le français) : rien ne change,
                // rien à écrire ni à resynthétiser.
                if next.id == current {
                    return Effect::None;
                }
                config.voice.per_language.insert(language.to_string(), next.id.to_string());
                Effect::Resynthesize
            }
            Field::Speed => {
                config.voice.speed = (config.voice.speed + 0.05 * delta as f32).clamp(0.5, 3.0);
                Effect::Speed
            }
            // Le plein écran, noté zéro, prolonge la course vers le haut : il
            // est la largeur la plus grande, pas une valeur à part.
            Field::LineWidth => {
                const MIN: u32 = 40;
                const MAX: u32 = 120;
                const STEP: u32 = 4;
                config.reading.line_width = match (config.reading.line_width, delta) {
                    (0, d) if d < 0 => MAX,
                    (0, _) => 0,
                    (w, d) if d > 0 && w >= MAX => 0,
                    (w, d) => (w.saturating_add_signed(d * STEP as i32)).clamp(MIN, MAX),
                };
                Effect::None
            }
            Field::LineSpacing => {
                config.reading.line_spacing = config.reading.line_spacing.saturating_add_signed(delta).min(3);
                Effect::None
            }
            Field::ReadingRuler => {
                config.reading.reading_ruler = !config.reading.reading_ruler;
                Effect::None
            }
        }
    }

    /// Recopie la valeur de ce réglage de `from` dans `to`, et rien d'autre :
    /// l'enregistrement automatique n'écrit que ce que la page vient de
    /// changer, jamais une option de la ligne de commande (`--speed`…).
    pub fn copy_value(&self, from: &Config, to: &mut Config) {
        match self {
            Field::VoiceFr | Field::VoiceEn => {
                let language = self.language().expect("une ligne de voix a sa langue");
                if let Some(voice) = from.voice.per_language.get(language) {
                    to.voice.per_language.insert(language.to_string(), voice.clone());
                }
            }
            Field::Speed => to.voice.speed = from.voice.speed,
            Field::LineWidth => to.reading.line_width = from.reading.line_width,
            Field::LineSpacing => to.reading.line_spacing = from.reading.line_spacing,
            Field::ReadingRuler => to.reading.reading_ruler = from.reading.reading_ruler,
        }
    }
}

/// État de la page des réglages.
pub struct ConfigScreen {
    /// Ligne choisie, rang dans `FIELDS`.
    pub row: usize,
    /// Téléchargement de la voix recommandée, lancé seul au premier usage.
    pub download: Option<models::Download>,
    /// Dernier message de la page (aperçu refusé…).
    pub notice: String,
}

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigScreen {
    pub fn new() -> Self {
        Self { row: 0, download: None, notice: String::new() }
    }

    pub fn current_field(&self) -> Field {
        FIELDS[self.row.min(FIELDS.len() - 1)]
    }

    /// Place le curseur sur `field`.
    pub fn select(&mut self, field: Field) {
        self.row = FIELDS.iter().position(|f| *f == field).unwrap_or(0);
    }

    pub fn move_row(&mut self, delta: i32) {
        self.row = (self.row as i32 + delta).clamp(0, FIELDS.len() as i32 - 1) as usize;
    }

    /// Consulte l'avancement du téléchargement et range ce qui est terminé.
    ///
    /// Renvoie le paquet fraîchement installé, pour que l'appelant le
    /// sélectionne.
    pub fn poll_download(&mut self) -> Option<&'static models::ModelPackage> {
        let download = self.download.as_ref()?;
        if !download.is_finished() {
            return None;
        }
        let package = download.package;
        let outcome = download.progress();
        self.download = None;
        match outcome {
            models::Progress::Done => {
                self.notice = format!("{} installé", package.display_name);
                Some(package)
            }
            models::Progress::Failed(err) => {
                self.notice = format!("échec : {err}");
                None
            }
            models::Progress::Cancelled => {
                self.notice = "téléchargement interrompu (reprise possible)".to_string();
                None
            }
            models::Progress::Running { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_holds_the_six_settings_of_the_spec() {
        let labels: Vec<&str> = FIELDS.iter().map(|f| f.label()).collect();
        assert_eq!(
            labels,
            ["Voix française", "Voix anglaise", "Vitesse", "Largeur de colonne", "Interlignage", "Règle de lecture"]
        );
    }

    #[test]
    fn the_cursor_stays_on_the_page() {
        let mut screen = ConfigScreen::new();
        screen.move_row(-5);
        assert_eq!(screen.current_field(), Field::VoiceFr);
        screen.move_row(99);
        assert_eq!(screen.current_field(), Field::ReadingRuler);
        screen.select(Field::Speed);
        assert_eq!(screen.current_field(), Field::Speed);
    }

    fn kokoro() -> Config {
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config
    }

    #[test]
    fn the_english_voice_cycles_through_the_english_voices() {
        let mut config = kokoro();
        assert_eq!(Field::VoiceEn.value(&config), "af_bella · américaine, femme, recommandée");
        assert_eq!(Field::VoiceEn.adjust(&mut config, 1), Effect::Resynthesize);
        assert_eq!(config.voice.per_language["en"], "am_fenrir");
        Field::VoiceEn.adjust(&mut config, -2);
        assert_eq!(config.voice.per_language["en"], "af_heart", "la liste boucle");
        assert!(!config.voice.per_language.contains_key("fr"), "l'autre langue ne bouge pas");
    }

    #[test]
    fn the_french_voice_alone_changes_nothing() {
        let mut config = kokoro();
        assert_eq!(Field::VoiceFr.value(&config), "p-fenrir60 · française, recommandée");
        assert_eq!(Field::VoiceFr.adjust(&mut config, 1), Effect::None);
        assert!(!config.voice.per_language.contains_key("fr"), "rien d'écrit");
    }

    #[test]
    fn without_kokoro_the_voice_lines_name_the_system_voice() {
        let mut config = Config::default();
        assert_eq!(Field::VoiceFr.value(&config), "voix du système");
        assert_eq!(Field::VoiceEn.adjust(&mut config, 1), Effect::None);
        assert!(config.voice.per_language.is_empty());
    }

    #[test]
    fn speed_is_bounded_and_applies_at_once() {
        let mut config = Config::default();
        for _ in 0..200 {
            assert_eq!(Field::Speed.adjust(&mut config, 1), Effect::Speed);
        }
        assert_eq!(config.voice.speed, 3.0);
        for _ in 0..200 {
            Field::Speed.adjust(&mut config, -1);
        }
        assert_eq!(config.voice.speed, 0.5);
    }

    #[test]
    fn the_reading_column_walks_between_forty_and_full_screen() {
        let mut config = Config::default();
        assert_eq!(config.reading.line_width, 0);
        assert_eq!(Field::LineWidth.value(&config), "plein écran");
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 120);
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 116, "la colonne recule par quatre");
        config.reading.line_width = 40;
        Field::LineWidth.adjust(&mut config, -1);
        assert_eq!(config.reading.line_width, 40, "quarante caractères est le plancher");
        config.reading.line_width = 120;
        Field::LineWidth.adjust(&mut config, 1);
        assert_eq!(config.reading.line_width, 0);
    }

    #[test]
    fn line_spacing_stays_between_none_and_three() {
        let mut config = Config::default();
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, 1);
        }
        assert_eq!(config.reading.line_spacing, 3);
        for _ in 0..5 {
            Field::LineSpacing.adjust(&mut config, -1);
        }
        assert_eq!(config.reading.line_spacing, 0);
        assert_eq!(Field::LineSpacing.value(&config), "aucun");
    }

    #[test]
    fn the_reading_ruler_toggles() {
        let mut config = Config::default();
        assert!(config.reading.reading_ruler);
        Field::ReadingRuler.adjust(&mut config, 1);
        assert!(!config.reading.reading_ruler);
        assert_eq!(Field::ReadingRuler.value(&config), "non");
    }

    #[test]
    fn every_setting_has_a_label_and_a_hint() {
        for config in [Config::default(), kokoro()] {
            for field in FIELDS {
                assert!(!field.label().is_empty(), "{field:?}");
                assert!(field.hint(&config).len() > 10, "{field:?}");
                assert!(!field.value(&config).is_empty(), "{field:?}");
            }
        }
    }

    #[test]
    fn copying_a_field_copies_nothing_else() {
        let mut from = Config::default();
        from.voice.speed = 2.5; // une option --speed, par exemple
        from.reading.line_spacing = 3;
        let mut to = Config::default();
        Field::LineSpacing.copy_value(&from, &mut to);
        assert_eq!(to.reading.line_spacing, 3);
        assert_eq!(to.voice.speed, Config::default().voice.speed, "la vitesse ne suit pas");
    }

    #[test]
    fn copying_a_voice_copies_only_its_language_line() {
        let mut from = kokoro();
        from.voice.voice = "voix-de-la-ligne-de-commande".into();
        Field::VoiceEn.adjust(&mut from, 1);
        let mut to = Config::default();
        to.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        Field::VoiceEn.copy_value(&from, &mut to);
        assert_eq!(to.voice.per_language["en"], "am_fenrir");
        assert_eq!(to.voice.per_language["fr"], "p-fenrir60", "l'autre langue reste");
        assert_eq!(to.voice.voice, "", "voice.voice n'est pas écrit");
    }
}
```

(Les tests `copying_*` du plan 1 sont remplacés par les deux derniers, sur le nouveau jeu de champs.)

- [ ] **Step 4: `Effect` réduit**

`src/config.rs`, l'énumération devient :

```rust
/// Ce qu'un changement de réglage impose au reste de l'application.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    None,
    /// Régénérer l'audio déjà préparé.
    Resynthesize,
    /// Appliquer la vitesse de la configuration au son, sur-le-champ.
    Speed,
}
```

- [ ] **Step 5: Adapter `src/app/settings.rs`**

1. En-tête : `//! Réglages depuis le terminal : la page des réglages, l'aperçu de voix, le rechargement du moteur, l'enregistrement de chaque changement.`

2. Supprimer : `open_config` (ancienne), `toggle_config`, `config_move_section`, `config_goto_section`, `config_open_section`, `config_back_to_menu`, `config_toggle_show_all`, `config_move_row` (ancienne), `config_adjust`, `config_activate`, `config_edit_push`, `config_edit_backspace`, `config_commit_edit`, `config_set_text`, `config_cancel_edit`, `activate_model`, `config_delete`, `config_commit_edit_and_save`, `request_preview`. Les remplacer, en tête de `impl App`, par :

```rust
    /// Ouvre les réglages. La lecture continue derrière : ce qu'on y change
    /// s'entend aussitôt.
    pub fn open_config(&mut self) {
        self.show_config = true;
        self.panel = Panel::None;
    }

    /// `q`, `Échap` ou `s` : les réglages se ferment, la lecture reste. Sans
    /// rien à lire (`sillon --config` seul), il n'y a rien derrière : fermer
    /// rend la main au terminal.
    pub fn close_config(&mut self) {
        self.stop_preview();
        if self.doc().is_empty() && self.input_complete() {
            self.should_quit = true;
            return;
        }
        self.show_config = false;
    }

    pub fn config_move_row(&mut self, delta: i32) {
        self.screen.move_row(delta);
    }

    /// La voix d'une autre langue que celle du document ne s'entend pas
    /// ici : la phrase en cours n'a pas à reprendre pour elle.
    pub(super) fn effect_in_document(&self, field: Field, effect: Effect) -> Effect {
        let language = self.builder.language();
        let code = crate::text::language::Lang::parse(language).map_or(language, |l| l.code());
        match field.language() {
            Some(voice_language) if voice_language != code => Effect::None,
            _ => effect,
        }
    }
```

3. `config_adjust_and_save` (plan 1) devient :

```rust
    /// Change le réglage choisi, le fait entendre, et l'écrit aussitôt. Un
    /// enregistrement impossible laisse le réglage appliqué et le dit.
    pub fn config_adjust_and_save(&mut self, delta: i32) {
        let field = self.screen.current_field();
        let effect = field.adjust(&mut self.config, delta);
        let effect = self.effect_in_document(field, effect);
        self.apply_effect(effect);
        self.report_save(field);
    }
```

4. `select_model` lisait `Field::Voice`, qui n'existe plus : remplacer toute la fonction par :

```rust
    /// Fait d'un modèle installé le moteur courant, et l'écrit aussitôt.
    pub(super) fn select_model(&mut self, package: &'static models::ModelPackage) {
        let Some(path) = models::install_path(package) else { return };
        self.config.engine.model_path = path.to_string_lossy().into_owned();
        self.config.engine.family = package.family.to_string();
        self.config.engine.engine = "auto".to_string();
        // Le modèle change, ses voix aussi : chaque langue reçoit la sienne.
        self.config.align_voice_with_family();
        self.reload_engine();
        self.notify(&format!("Voix {} prête", package.display_name));
        // Le choix doit survivre au relancement : il s'écrit tout de suite.
        if let Err(err) = self.save_model_choice() {
            self.notify(&format!("Modèle choisi, non enregistré : {err:#}"));
        }
    }
```

5. Aperçu : `preview_voice` devient

```rust
    /// `t` dans la page : la voix de la ligne choisie, dans sa langue ; sur
    /// les autres lignes, la voix du document.
    pub fn preview_voice(&mut self) {
        // La touche répond toujours quelque chose : laisser tourner l'aperçu
        // précédent ferait passer un refus pour une touche sans effet.
        self.stop_preview();
        let language = match self.screen.current_field().language() {
            Some(language) => language.to_string(),
            None => self.builder.language().to_string(),
        };
        match self.request_preview_in(&language) {
            Ok(()) => self.screen.notice.clear(),
            Err(err) => self.screen.notice = err,
        }
    }
```

dans `request_preview_in`, `let label = self.preview_label();` devient `let label = self.preview_label(language);` ; et `preview_label` devient

```rust
    /// Décrit ce que l'aperçu fait entendre : la voix de cette langue et la
    /// vitesse.
    pub(super) fn preview_label(&self, language: &str) -> String {
        let voice = self.config.tts_params_in(language).voice;
        let voice = if voice.is_empty() { "voix du système".to_string() } else { voice };
        format!("{voice} · {:.2}×", self.config.voice.speed)
    }
```

6. `apply_effect` devient :

```rust
    /// Répercute un changement de réglage sur la lecture.
    pub fn apply_effect(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Speed => {
                let speed = self.config.voice.speed;
                self.player.set_speed(speed);
            }
            Effect::Resynthesize => {
                self.apply_tts_params();
                let current = self.current_sentence();
                self.seek_to(current);
            }
        }
    }
```

7. `src/app/mod.rs` : `use crate::app::config::{ConfigScreen, Field, Page, Section};` devient `use crate::app::config::{ConfigScreen, Field};`.

- [ ] **Step 6: Clavier de la page**

`src/app/keys.rs` :
- supprimer `use super::config::Page;` ;
- `CONFIG_HELP` devient :
  ```rust
  /// Aide de la page des réglages : toutes les touches qui y agissent.
  pub const CONFIG_HELP: &[(&str, &str)] = &[
      ("↑ ↓", "Choisir un réglage"),
      ("← → · Entrée", "Changer sa valeur, enregistrée aussitôt"),
      ("t", "Écouter la voix"),
      ("q · Échap · s", "Fermer les réglages"),
      ("?", "Aide"),
  ];
  ```
- `handle_config` devient :
  ```rust
  fn handle_config(app: &mut App, key: Key) {
      match key.code {
          // Fermer les réglages ne quitte jamais une lecture.
          KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => app.close_config(),
          KeyCode::Up => app.config_move_row(-1),
          KeyCode::Down => app.config_move_row(1),
          KeyCode::Right | KeyCode::Enter => app.config_adjust_and_save(1),
          KeyCode::Left => app.config_adjust_and_save(-1),
          KeyCode::Char('t') => app.preview_voice(),
          KeyCode::Char('?') => app.show_help = true,
          _ => {}
      }
  }
  ```
- dans `handle_reader`, `KeyCode::Char('s') => app.toggle_config(),` devient `KeyCode::Char('s') => app.open_config(),` ;
- l'en-tête du fichier : `//! Deux contextes : la page des réglages, puis la lecture. Chacun consomme` / `//! les touches qui lui reviennent.` (remplace les trois lignes « Trois contextes… »).

- [ ] **Step 7: Réécrire `src/tui/ui/config.rs`**

Contenu complet :

```rust
//! La page des réglages : six lignes, l'explication de la ligne choisie, le
//! rappel des touches.

use super::*;

/// Colonne des libellés : le plus long, « Largeur de colonne », et de l'air.
const LABEL_WIDTH: usize = 22;

/// Dessine la page des réglages, qui remplace la vue de lecture.
pub(super) fn render_config(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // titre
            Constraint::Min(8),    // réglages et explication
            Constraint::Length(2), // touches et messages
        ])
        .split(area);
    frame.render_widget(Paragraph::new(config_title(app)), rows[0]);
    frame.render_widget(Paragraph::new(config_lines(app, rows[1].width as usize)), rows[1]);
    frame.render_widget(Paragraph::new(config_footer(app)), rows[2]);
}

fn config_title(app: &App) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Réglages", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("   moteur : {}", app.engine_name()), Style::default().fg(DIM)),
        ]),
    ]
}

/// Les six réglages, puis l'explication de celui qui est choisi.
pub(super) fn config_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let current = app.screen.current_field();
    let mut lines = vec![Line::from("")];
    for field in FIELDS {
        let selected = field == current;
        let (label_style, value_style) = if selected {
            (
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
            )
        } else {
            (Style::default().fg(NORMAL), Style::default().fg(BRIGHT))
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} {:<LABEL_WIDTH$}", if selected { "▸" } else { " " }, field.label()),
                label_style,
            ),
            Span::styled(field.value(&app.config), value_style),
        ]));
    }
    lines.push(Line::from(""));
    for line in fold(current.hint(&app.config), width.saturating_sub(6)) {
        lines.push(Line::from(Span::styled(format!("    {line}"), Style::default().fg(DIM))));
    }
    lines
}

/// Le rappel des touches, puis le message du moment.
pub(super) fn config_footer(app: &App) -> Vec<Line<'static>> {
    // Sans document derrière, fermer la page quitte : autant le dire.
    let exit = if app.doc().is_empty() { "q quitter" } else { "Échap fermer" };
    let mut lines = vec![Line::from(Span::styled(
        format!(" ↑↓ réglage · ←→ valeur, enregistrée aussitôt · t écouter · {exit}"),
        Style::default().fg(FAINT),
    ))];
    // L'aperçu passe devant : c'est le seul message qui décrit une action en
    // train de se produire.
    let message = app
        .preview_status()
        .or_else(|| (!app.screen.notice.is_empty()).then(|| app.screen.notice.clone()))
        .or_else(|| (!app.status().is_empty()).then(|| app.status().to_string()));
    if let Some(message) = message {
        lines.push(Line::from(Span::styled(format!(" {message}"), Style::default().fg(ACCENT))));
    }
    lines
}

/// Plie un texte à la largeur donnée, sur les espaces.
pub(super) fn fold(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let cost = word.chars().count() + usize::from(!current.is_empty());
        if current.chars().count() + cost > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        } else if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
```

`src/tui/ui/mod.rs` :
- `use crate::app::config::{Page, Section};` devient `use crate::app::config::FIELDS;` ;
- dans `render`, `render_help(frame, area, CONFIG_HELP, "Configuration");` devient `render_help(frame, area, CONFIG_HELP, "Réglages");`.

`src/main.rs` : `app.open_config();` devient

```rust
        app.open_config();
        // Ouverte au lancement, la page passe avant la lecture.
        app.player.pause();
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS — dont `q_and_escape_close_the_settings_without_quitting`, `closing_the_settings_with_nothing_to_read_quits`, `the_arrows_of_the_settings_write_at_once`, `a_voice_of_another_language_does_not_restart_the_sentence`, `the_settings_are_one_page_of_six_lines`, et les tests du plan 1 `the_autosave_writes_only_the_setting_changed_on_screen` et `an_unreadable_config_is_never_overwritten_by_the_autosave` (sur `on_line_spacing` adapté).

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: PASS, aucun avertissement. Si clippy signale un import devenu inutile (`Wrap`, `Color` dans `src/tui/ui/mod.rs`, `Effect` ou `Section` ailleurs), le retirer : il ne servait qu'aux rubriques.

- [ ] **Step 9: Commit**

```bash
git add -A src
git commit -m "$(cat <<'EOF'
feat(terminal): réglages sur une page — voix française, voix anglaise, vitesse, largeur de colonne, interlignage, règle de lecture ; plus de rubriques ni de modèles ; q, Échap et s ferment sans quitter ; t écoute la voix de la ligne

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Installation automatique de la voix, progression visible

**Files:**
- Modify: `src/app/config.rs` (`ConfigScreen` : champ `install_error`, `poll_download`)
- Modify: `src/app/settings.rs` (`install_default_voice`, `voice_install_status`)
- Modify: `src/tui/ui/player.rs` (`status_spans`), `src/tui/ui/config.rs` (`config_footer`), `src/tui/ui/mod.rs` (import, test)
- Modify: `src/launch.rs` (`prepare`, `migrate_terminal_in`, `adopt_installed_model`, tests)
- Test: `src/app/mod.rs`, `src/tui/ui/mod.rs`, `src/launch.rs`

**Interfaces:**
- Consumes (plan 1) : `models::ensure_default_voice(false, app.screen.download.as_ref())`.
- Produces :
  - `ConfigScreen.install_error: Option<String>`.
  - `impl App { pub fn install_default_voice(&mut self) }` : ne fait rien si la voix du système est demandée (`EnginePreference::System`) ; sinon range le téléchargement démarré dans `screen.download`, ou l'échec dans `screen.install_error`.
  - `impl App { pub fn voice_install_status(&self) -> Option<String> }` : progression en cours, ou échec ; `None` sinon.

- [ ] **Step 1: Write the failing tests**

Dans `src/app/mod.rs`, module `tests` :

```rust
    fn kokoro_int8() -> &'static models::ModelPackage {
        models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap()
    }

    #[test]
    fn the_system_voice_asked_for_installs_nothing() {
        let mut app = app_reading(DOC);
        app.config.engine.engine = "system".into(); // comme `--engine system`
        app.install_default_voice();
        assert!(app.screen.download.is_none());
        assert_eq!(app.voice_install_status(), None);
    }

    /// Sans `kokoro-onnx`, le catalogue du terminal est vide : rien à
    /// télécharger. (Avec la feature, l'appel téléchargerait pour de bon :
    /// le test n'existe pas.)
    #[cfg(not(feature = "kokoro-onnx"))]
    #[test]
    fn a_terminal_without_kokoro_installs_nothing() {
        let mut app = app_reading(DOC);
        app.install_default_voice();
        assert!(app.screen.download.is_none());
        assert_eq!(app.voice_install_status(), None);
    }

    #[test]
    fn the_install_shows_its_progress() {
        let mut app = app_reading(DOC);
        app.screen.download = Some(models::Download::fake(kokoro_int8(), false));
        let status = app.voice_install_status().expect("une progression");
        assert!(status.contains("Installation de la voix"), "{status}");
        assert!(status.contains("voix du système lit en attendant"), "{status}");
    }

    #[test]
    fn a_failed_install_says_so_and_writes_nothing() {
        let (dir, path) = autosave_target("echec");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        let family = app.config.engine.family.clone();
        app.screen.download = Some(models::Download::fake_ended(
            kokoro_int8(),
            models::Progress::Failed("réseau injoignable".into()),
        ));
        app.tick();
        let status = app.voice_install_status().expect("l'échec se dit");
        let written = path.exists();
        std::fs::remove_dir_all(&dir).ok();
        assert!(status.contains("réseau injoignable"), "{status}");
        assert!(status.contains("prochain lancement"), "{status}");
        assert_eq!(app.config.engine.family, family, "la voix du système continue");
        assert!(!written, "config.toml n'est pas touché");
    }

    #[test]
    fn a_finished_install_becomes_the_voice_and_is_written() {
        let (dir, path) = autosave_target("fini");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.screen.download = Some(models::Download::fake(kokoro_int8(), true));
        app.tick();
        let written = Config::read_from(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(app.config.engine.family, "kokoro");
        assert_eq!(written.engine.family, "kokoro");
        assert_eq!(app.voice_install_status(), None, "plus rien à dire");
    }
```

Dans `src/tui/ui/mod.rs`, module `tests` :

```rust
    #[test]
    fn the_status_line_tells_the_voice_is_installing() {
        let mut app = crate::app::tests::app_reading("Une phrase.\n");
        app.notices.clear();
        let package = crate::models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap();
        app.screen.download = Some(crate::models::Download::fake(package, false));
        let line = text_of(&status_spans(&app));
        assert!(line.contains("Installation de la voix"), "{line:?}");
        let footer = texts(&config_footer(&app)).join(" ");
        assert!(footer.contains("Installation de la voix"), "{footer}");
    }
```

Dans `src/launch.rs`, module `tests`, **remplacer** `the_terminal_never_downloads_and_records_nothing_then` par :

```rust
    #[test]
    fn an_offer_says_nothing_since_the_install_is_automatic() {
        let mut config = on_supertonic();
        assert_eq!(migrate_terminal_in(&mut config, &[kokoro_int8()], |_| false), None);
        assert_eq!(config.app.voice_migration, 0, "rien de retenu sans adoption");
        assert_eq!(config.engine.family, "supertonic");
    }
```

et dans `the_terminal_adopts_an_installed_kokoro_once`, ajouter après l'assertion `starts_with` : `assert!(!notice.contains("`s`"), "plus de rubrique Modèles où revenir : {notice}");`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib`
Expected: FAIL à la compilation — « no method named `install_default_voice` », « no method named `voice_install_status` ».

- [ ] **Step 3: Implémenter**

1. `src/app/config.rs`, `ConfigScreen` : ajouter le champ

```rust
    /// Pourquoi la voix recommandée n'a pas pu s'installer : dit jusqu'au
    /// prochain lancement, qui réessaie.
    pub install_error: Option<String>,
```

(initialisé à `None` dans `new`), et `poll_download` devient :

```rust
    pub fn poll_download(&mut self) -> Option<&'static models::ModelPackage> {
        let download = self.download.as_ref()?;
        if !download.is_finished() {
            return None;
        }
        let package = download.package;
        let outcome = download.progress();
        self.download = None;
        let reason = match outcome {
            models::Progress::Done => {
                self.install_error = None;
                return Some(package);
            }
            models::Progress::Running { .. } => return None,
            models::Progress::Failed(err) => err,
            models::Progress::Cancelled => "téléchargement interrompu".to_string(),
        };
        self.install_error = Some(format!(
            "Voix {} non installée : {reason} — nouvel essai au prochain lancement",
            package.display_name
        ));
        None
    }
```

2. `src/app/settings.rs`, après `effect_in_document` :

```rust
    /// Au premier usage, la voix recommandée s'installe sans rien demander ;
    /// la voix du système lit en attendant, et la fin du téléchargement la
    /// remplace au même endroit du texte (`poll_download` → `select_model`).
    /// Rien quand la voix du système est demandée exprès
    /// (`engine = "system"`, `--engine system`).
    pub fn install_default_voice(&mut self) {
        if matches!(self.config.engine_preference(), tts::EnginePreference::System) {
            return;
        }
        match models::ensure_default_voice(false, self.screen.download.as_ref()) {
            Ok(Some(download)) => {
                self.screen.install_error = None;
                self.screen.download = Some(download);
            }
            Ok(None) => {}
            Err(err) => {
                self.screen.install_error =
                    Some(format!("Voix non installée : {err:#} — nouvel essai au prochain lancement"));
            }
        }
    }

    /// Où en est l'installation de la voix, pour la ligne d'état et la page
    /// des réglages : la progression, ou l'échec.
    pub fn voice_install_status(&self) -> Option<String> {
        if let Some(download) = &self.screen.download {
            let progress = match download.progress() {
                models::Progress::Running { received, total } => {
                    format!("{} / {}", models::human_size(received), models::human_size(total))
                }
                _ => "fin…".to_string(),
            };
            return Some(format!(
                "Installation de la voix {} : {progress} — la voix du système lit en attendant",
                download.package.display_name
            ));
        }
        self.screen.install_error.clone()
    }
```

3. `src/tui/ui/player.rs`, `status_spans` : supprimer `let on_fallback = …;` et son commentaire, et remplacer la branche `} else if on_fallback { … " ⚠ voix système " … " pour installer un vrai modèle " … }` par :

```rust
    } else if let Some(install) = app.voice_install_status() {
        // L'installation de la voix est un état, pas un événement : elle se
        // voit tant qu'elle dure, et son échec jusqu'au prochain lancement.
        spans.push(Span::styled(
            " ⬇ voix ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {install} "), Style::default().fg(NORMAL)));
```

4. `src/tui/ui/config.rs`, `config_footer` : après `.preview_status()`, insérer `.or_else(|| app.voice_install_status())`.

5. `src/tui/ui/mod.rs` : si clippy signale `use crate::models;` inutilisé, le supprimer (les tests disent `crate::models::` en entier).

6. `src/launch.rs` :
   - dans `prepare`, juste après `let mut app = App::new(…);` :
     ```rust
         // Kokoro s'installe seul au premier usage ; la voix du système lit
         // en attendant.
         app.install_default_voice();
     ```
   - dans `migrate_terminal_in` : la doc devient `/// Le passage à Kokoro au terminal : un Kokoro installé est adopté, une` / `/// seule fois ; absent, il s'installe seul (`App::install_default_voice`)` / `/// et rien n'est dit ici. Rend l'avis.` ; le message de `Step::Switch` devient `Some(format!("Nouvelle voix : Kokoro remplace {former}. L'ancien modèle reste installé."))` ; le bras `Step::Offer(_) => Some("Nouvelle voix Kokoro disponible : `s` pour l'installer.".to_string()),` devient `Step::Offer(_) => None,` ;
   - `adopt_installed_model` : la doc dit `/// Adopte un modèle installé si la configuration n'en désigne aucun. Sans` / `/// modèle, l'installation automatique s'en charge (`App::install_default_voice`).` et le `match crate::models::first_installed() { … }` devient :
     ```rust
         if let Some(package) = crate::models::first_installed() {
             if let Some(path) = crate::models::install_path(package) {
                 config.engine.model_path = path.to_string_lossy().into_owned();
                 config.engine.family = package.family.to_string();
             }
         }
     ```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib`
Expected: PASS.

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: PASS, aucun avertissement.

- [ ] **Step 5: Commit**

```bash
git add src/app src/tui src/launch.rs
git commit -m "$(cat <<'EOF'
feat(terminal): la voix s'installe seule au premier usage (models::ensure_default_voice) — progression dans la ligne d'état et la page des réglages, échec dit avec un nouvel essai au lancement suivant ; rien avec --engine system ni sans Kokoro dans le binaire

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: `sillon` sans argument, `--help` resserré

**Files:**
- Modify: `src/launch.rs` (`Args`, `SHORT_HELP`, `nothing_to_read`, `prepare`, `Prepared`, `read_input`, tests)
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `App::open_config` (Tâche 5).
- Produces : `pub const SHORT_HELP: &str` ; `pub fn nothing_to_read(args: &Args, stdin_is_terminal: bool) -> bool` ; `Prepared` sans `has_input`.

- [ ] **Step 1: Write the failing tests**

Dans `src/launch.rs`, module `tests` :

```rust
    #[test]
    fn nothing_to_read_only_from_a_bare_terminal() {
        assert!(nothing_to_read(&Args::default(), true), "sillon seul, dans un terminal");
        assert!(!nothing_to_read(&Args::default(), false), "cat notes.md | sillon lit le tube");
        let file = Args { file: Some("notes.md".into()), ..Args::default() };
        assert!(!nothing_to_read(&file, true));
        let clip = Args { clip: true, ..Args::default() };
        assert!(!nothing_to_read(&clip, true));
        let config = Args { config: true, ..Args::default() };
        assert!(!nothing_to_read(&config, true), "--config ouvre toujours les réglages");
    }

    #[test]
    fn the_short_help_names_the_four_ways_to_read() {
        for way in ["sillon document.md", "| sillon", "sillon --clip", "app", "--config", "--help"] {
            assert!(SHORT_HELP.contains(way), "{way} manque :\n{SHORT_HELP}");
        }
    }

    #[test]
    fn help_shows_only_the_everyday_options() {
        use clap::CommandFactory;
        let help = Args::command().render_help().to_string();
        for shown in ["[FILE]", "--clip", "--speed", "--voice", "--to-markdown", "--no-cache", "--config"] {
            assert!(help.contains(shown), "{shown} manque :\n{help}");
        }
        for hidden in [
            "--pitch", "--language", "--engine", "--model", "--family", "--sessions", "--prefetch",
            "--code-blocks", "--no-autoplay", "--write-config", "--show-config", "--check-kokoro",
        ] {
            assert!(!help.contains(hidden), "{hidden} se voit encore :\n{help}");
        }
    }

    #[test]
    fn hidden_options_are_still_accepted() {
        let args = Args::try_parse_from([
            "sillon", "--pitch", "0.5", "--language", "en", "--engine", "system", "--sessions", "2",
            "--prefetch", "4", "--code-blocks", "skip", "--no-autoplay", "--show-config",
        ])
        .expect("toujours acceptées");
        assert_eq!(args.pitch, Some(0.5));
        assert!(args.show_config);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sillon --lib launch::tests`
Expected: FAIL à la compilation — « cannot find function `nothing_to_read` », « cannot find value `SHORT_HELP` ».

- [ ] **Step 3: Implémenter**

1. `Args` : ajouter `hide = true` à `pitch`, `language`, `engine`, `model`, `family`, `sessions`, `prefetch`, `code_blocks`, `no_autoplay`, `write_config`, `show_config`, `check_kokoro`. Exemple, pour chacun : `#[arg(long)]` devient `#[arg(long, hide = true)]`. `file`, `voice`, `speed`, `to_markdown`, `clip`, `no_cache`, `config` ne changent pas.

2. Après `pub struct Args { … }` :

```rust
/// Ce que `sillon` lancé seul dans un terminal affiche, avant de rendre la
/// main : il n'y a rien à lire, mais tout à savoir pour lire.
pub const SHORT_HELP: &str = "\
sillon lit un texte à voix haute, en le montrant mot à mot.

Quatre manières de lire :
  sillon document.md        un fichier : markdown, texte, PDF, image
  commande | sillon         ce qu'une commande écrit, au fil de l'eau
  sillon --clip             le presse-papiers
  l'app sillon (macOS)      ⌃⌥Espace lit la sélection

Réglages : sillon --config · toutes les options : sillon --help
";

/// Vrai quand `sillon` est lancé seul dans un terminal : ni fichier, ni
/// presse-papiers, ni réglages demandés, et rien qui arrive par un tube.
pub fn nothing_to_read(args: &Args, stdin_is_terminal: bool) -> bool {
    args.file.is_none() && !args.clip && !args.config && stdin_is_terminal
}
```

3. `prepare`, juste après le bloc `if args.to_markdown { … }` :

```rust
    // Lancé seul, sans rien à lire : dire comment lire plutôt qu'ouvrir un
    // écran vide. `--config` ouvre les réglages.
    if nothing_to_read(args, std::io::stdin().is_terminal()) {
        print!("{SHORT_HELP}");
        return Ok(Launch::Done);
    }
```

4. `Prepared` : supprimer le champ `/// Vrai quand il y a quelque chose à lire …` / `pub has_input: bool,`. Dans `prepare`, `let has_input = args.file.is_some() || args.clip || stream.is_some();` est supprimé, et la ligne `has_input,` du littéral `Prepared { … }` aussi.

5. `read_input`, le bras `None if std::io::stdin().is_terminal() =>` : le commentaire devient `// Seul `--config` arrive ici sans document : la page des réglages n'a` / `// rien à lire derrière elle.`.

6. `src/main.rs`, le corps après `let Launch::Ready(prepared) = … else { … };` devient :

```rust
    let Prepared { mut app, config_requested, quiet } = *prepared;

    if config_requested {
        app.open_config();
        // Ouverte au lancement, la page passe avant la lecture.
        app.player.pause();
    }
```

(le reste — `tui::run`, `drop(quiet)` — ne change pas ; supprimer le commentaire « Sans document ni flux, on démarre sur l'écran de configuration… ».)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sillon --lib launch::tests`
Expected: PASS.

Run (dans un vrai terminal) : `cargo run -q; echo "sortie : $?"`
Expected : l'aide courte, puis `sortie : 0`, sans écran plein.

Run: `printf '# T\n\nUne phrase.\n' | cargo run -q`
Expected : l'écran de lecture s'ouvre et lit (quitter avec `q`).

Run: `cargo run -q -- --help`
Expected : seules les sept options de la spec (plus `-h` et `-V`).

- [ ] **Step 5: Commit**

```bash
git add src/launch.rs src/main.rs
git commit -m "$(cat <<'EOF'
feat(terminal): sillon seul affiche les quatre manières de lire et rend la main ; --help ne montre que FILE, --clip, --speed, --voice, --to-markdown, --no-cache et --config, les autres options restent acceptées

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: README à jour, vérification complète

**Files:**
- Modify: `README.md` (introduction l. 11-19 ; « Installer un modèle » l. 105-144 ; « Choisir une voix » l. 220-232 ; « Contrôles » l. 234-260 ; « Lire en écoutant » : paragraphes « Reprendre » et « Défiler » ; « Configuration » l. 341-372 et 431-433)

**Interfaces:**
- Consumes: le comportement livré par les Tâches 1 à 7.
- Produces: documentation seulement.

- [ ] **Step 1: Introduction**

Remplacer le bloc de code et le paragraphe qui le suit par :

````markdown
```bash
sillon                                      # dit comment lire, puis rend la main
cat notes.md | sillon
sillon rapport.md --speed 1.5
llm "explique-moi les monades" | sillon     # la lecture démarre avant la fin du flux
sillon --config                             # les réglages
```

**Lancé sans argument, `sillon` affiche les quatre manières de lire** — un
fichier, un tube, `--clip`, l'app — et rend la main. `s` ouvre les réglages
en cours de lecture, `?` l'aide.
````

- [ ] **Step 2: « Installer un modèle » devient « La voix »**

Remplacer le titre `### Installer un modèle` et son premier paragraphe (« Lancez `sillon` sans argument : l'écran s'ouvre sur la rubrique **Modèles**… Rien n'est jamais téléchargé sans cette action. ») par :

```markdown
### La voix

**Kokoro s'installe tout seul** au premier usage, avec un binaire construit
avec `kokoro-onnx` (`./scripts/install.sh` le fait) : le téléchargement
démarre au lancement, la voix du système lit en attendant, et la ligne d'état
montre la progression (`⬇ voix  Installation de la voix Kokoro 82M · int8 :
34,0 Mo / 92,4 Mo`). Le téléchargement fini, Kokoro prend la suite à la phrase
en cours, et le choix s'écrit dans `config.toml`. Un échec — réseau coupé —
se dit dans la ligne d'état ; le lancement suivant réessaie, en reprenant ce
qui était déjà reçu. Rien ne se télécharge avec `--engine system`, ni avec un
binaire sans `kokoro-onnx`.
```

Dans le même passage, le paragraphe « **Le nombre de sessions suit la famille.** … remettez-le à `auto` dans la rubrique **Moteur** pour en profiter. » se termine désormais par : « …écrire dans `config.toml` : retirez la ligne `sessions` (ou mettez `0`) pour en profiter. »

- [ ] **Step 3: « Choisir une voix »**

Remplacer le premier paragraphe (« Rubrique **Voix**, champ *Voix*, … ») par :

```markdown
Dans les réglages (`s`), lignes *Voix française* et *Voix anglaise* : `←` `→`
parcourent les voix de la langue, `t` fait écouter celle de la ligne. Le choix
s'enregistre aussitôt.
```

et la dernière phrase de la section (« La barre d'état affiche alors `⚠ voix système` et la marche à suivre, en permanence. ») par « La ligne d'état montre alors l'installation en cours, ou pourquoi elle a échoué. »

- [ ] **Step 4: « Contrôles »**

Remplacer le tableau par :

```markdown
| Touche | Effet |
|---|---|
| `Espace` | Lecture / pause |
| `←` `→` | Phrase précédente / suivante |
| `↑` `↓` | Bloc précédent / suivant |
| `n` `p` | Titre suivant / précédent |
| `/` | Chercher dans le document |
| `N` · `P` | Résultat suivant · précédent |
| `+` `-` | Vitesse (enregistrée) |
| `t` | Sommaire — `↑` `↓` puis `Entrée` pour y aller |
| `s` | Réglages |
| `?` | Aide : toutes ces touches |
| `Échap` | Ferme le sommaire ; sinon quitte |
| `q` | Quitter |
```

- [ ] **Step 5: « Lire en écoutant »**

- Dans le paragraphe « **Reprendre où l'on s'était arrêté.** », supprimer « ; `g` ramène au début ».
- Supprimer le paragraphe « **Défiler sans perdre la voix.** `Pg↑` et `Pg↓` … tout saut de lecture aussi. » (sauf si la Tâche 3 a été sautée).
- Dans la phrase d'introduction « Trois réglages, dans la rubrique **Lecture**, gouvernent… », remplacer « dans la rubrique **Lecture** » par « dans les réglages (`s`) ».

- [ ] **Step 6: « Configuration »**

Remplacer tout le début de la section, depuis « Tout se règle depuis l'écran de configuration » jusqu'à la phrase « Les valeurs prennent effet immédiatement : … préparée. » incluse, par :

````markdown
Les réglages tiennent sur **une page** — `s` en cours de lecture, ou
`sillon --config` :

```
  Réglages   moteur : Kokoro · ONNX

  ▸ Voix française          p-fenrir60 · française, recommandée
    Voix anglaise           af_bella · américaine, femme, recommandée
    Vitesse                 1.00×
    Largeur de colonne      plein écran
    Interlignage            une ligne
    Règle de lecture        oui
```

| Touche | Effet |
|---|---|
| `↑` `↓` | Choisir un réglage |
| `←` `→` · `Entrée` | Changer sa valeur |
| `t` | Écouter la voix |
| `q` · `Échap` · `s` | Fermer les réglages (sans quitter la lecture) |
| `?` | Aide |

**Chaque changement s'enregistre aussitôt** dans `config.toml`, sans touche
à presser, et s'entend tout de suite : la vitesse sur-le-champ, une voix en
reprenant la phrase. Seul le réglage changé s'écrit : une option de la ligne
de commande (`--speed 2`) reste à ce lancement. Un `config.toml` illisible
n'est jamais réécrit ; la page dit alors que le réglage n'est pas enregistré.

Le reste du fichier — moteur, fils de calcul, sessions, blocs de code,
langue, hauteur — ne s'affiche plus : il garde sa valeur, et se règle à la
main.
````

Et remplacer la dernière phrase de la section (« Les mêmes réglages existent en ligne de commande (`--voice`, `--speed`, `--pitch`, `--language`, `--sessions`…). Voir `sillon --help`. ») par :

```markdown
`sillon --help` montre les options de tous les jours (`--clip`, `--speed`,
`--voice`, `--to-markdown`, `--no-cache`, `--config`). Les autres restent
acceptées : `--pitch`, `--language`, `--engine`, `--model`, `--family`,
`--sessions`, `--prefetch`, `--code-blocks`, `--no-autoplay`,
`--write-config`, `--show-config`, `--check-kokoro`.
```

- [ ] **Step 7: Relire le README contre le code**

Run: `grep -n "signet\|Pg↑\|rubrique\|Modèles\|\`w\`\|\`g\`\|Maj+\|\[ \]\|sans argument" README.md`
Expected : plus aucune mention d'une touche retirée ni des rubriques ; « sans argument » seulement dans l'introduction. (Le paragraphe de `install-app-command.sh` sur la « page de notes » relève du plan 2 : ne pas y toucher ici, le signaler s'il est encore là.)

- [ ] **Step 8: Vérification complète**

Run: `cargo test --workspace --locked`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings && cargo clippy -p sillon --all-targets --features kokoro-onnx --locked -- -D warnings`
Expected: aucun avertissement.

Run: `cargo +1.88 check -p sillon --all-features --all-targets --locked`
Expected: OK (compile Tesseract : une minute la première fois).

Vérification manuelle, dans un vrai terminal :
1. `cargo run -q` : aide courte, retour au shell.
2. `cargo run -q -- README.md` : `?` liste exactement les touches du tableau « Contrôles » ; `b`, `m`, `1`, `g`, `r`, `w`, `Tab`, `h` ne font rien ; `t` puis `↓` `Entrée` saute au titre ; `Échap` ferme le sommaire puis quitte.
3. `cargo run -q -- README.md`, `s`, `↓↓↓↓` sur *Interlignage*, `→` : le texte s'espace à la fermeture ; `cargo run -q -- --show-config | grep line_spacing` montre la nouvelle valeur.
4. `cargo run -q -- --config` : `Échap` rend la main.
5. `cargo run -q -- --speed 2 README.md`, `s`, change l'interlignage, `q` : `--show-config` sans option montre `speed = 1.0` (ou la valeur d'avant), pas `2.0`.
6. Avec `cargo run -q --features kokoro-onnx -- README.md` et aucun Kokoro installé : la ligne d'état montre `⬇ voix` et la progression ; la voix du système lit ; à la fin, la voix change sans que la lecture saute. (Télécharge réellement ~92 Mo dans le dossier de données.)

- [ ] **Step 9: Commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
docs: README du terminal — lancement sans argument, clavier réduit, réglages sur une page enregistrés aussitôt, voix installée seule, options de --help

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

## Self-review

**Couverture de la spec**
- § 2 « Touches » : Tâches 1 (b, B, d), 2 (volume, préréglages, alias, Maj+flèches, g/G/Début/Fin, r, w ; q, Échap ; aide et rappel), 3 (Pg↑, Pg↓, f). « L'aide liste toutes les touches » : tests `the_help_names_every_reading_key_and_no_other` et `the_settings_help_lists_only_what_acts_there`.
- § 2 « Réglages : une page… enregistrement automatique ; `w` disparaît ; `q` et Échap ferment l'écran sans quitter » : Tâches 4 et 5.
- § 2 « Sans argument… `--config` ouvre toujours les réglages » et « Options visibles dans `--help` » : Tâche 7.
- § 2 « Installation du modèle automatique ; plus de rubrique Modèles » : Tâche 5 (rubrique retirée), Tâche 6 (installation, progression).
- § 3 « Signets supprimés » (côté cœur ; le pont au plan 2, vérifié Tâche 1 étape 1) : Tâche 1. « Les clés retirées de l'interface gardent leur valeur » : `copy_value` n'écrit que le champ touché ; aucune tâche ne réécrit le fichier entier.
- § 4 « tests ajoutés pour l'enregistrement automatique des réglages et le lancement sans argument » : Tâches 4, 5, 7 (en plus des tests du plan 1).
- README : Tâche 8.

**Placeholders** : aucun « TBD ». Deux étapes conditionnelles donnent leur action exacte : le reste FFI (Tâche 1, étape 1) et les imports que clippy signalerait (Tâches 5 et 6).

**Cohérence des types** : `Field` (six variantes) partout après la Tâche 5 ; `ConfigScreen::current_field(&self) -> Field` (plus d'`Option` ni de `&Config`) dans `config_adjust_and_save`, `preview_voice`, `on_line_spacing`, `config_lines` ; `install_error: Option<String>` lu par `voice_install_status` ; `Effect` à trois variantes dans `apply_effect` et `Field::adjust` ; `models::ensure_default_voice(false, self.screen.download.as_ref())` comme le plan 1 l'expose.

**Review Focus** : 1 → Tâche 5 `closing_the_settings_with_nothing_to_read_quits` ; 2 → Tâche 5 `a_voice_of_another_language_does_not_restart_the_sentence` ; 3 → Tâche 6 `the_system_voice_asked_for_installs_nothing`, `a_terminal_without_kokoro_installs_nothing` ; 4 → Tâche 6 `a_failed_install_says_so_and_writes_nothing` ; 5 → Tâche 7 `nothing_to_read_only_from_a_bare_terminal`.
