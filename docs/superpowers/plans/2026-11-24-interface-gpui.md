# Interface GPUI — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** une fenêtre de lecture `dysflow-app` en Rust (GPUI), sur macOS,
Windows et Linux. Elle lit à voix haute en mettant en évidence le mot
prononcé et ne contient aucune logique de lecture propre.

**Architecture :** une crate `app/` (paquet `dysflow-app`) au-dessus de
`dysflow::reader::handle::ReaderHandle`. L'interface envoie des `Request` et
des `Command`, et lit un `Snapshot` à chaque image et le `DocView` à chaque
révision. Tout ce qui est pur se trouve dans des modules sans GPUI, testés
seuls : la correspondance des touches, la mise en page en lignes, la
navigation par ligne et le Markdown d'édition. Les vues GPUI ne font
qu'afficher et relayer.

**Tech Stack :**
- Rust 2021 ;
- `gpui-pre =0.3.14` et `gpui-component =0.11.0` (Apache-2.0) ;
- le cœur `dysflow` : MLX sur Mac, ONNX Runtime ailleurs, Tesseract.

**Spec :** `docs/superpowers/specs/2026-11-24-interface-gpui-design.md`

## Global Constraints

- **Rust seulement** côté projet. Le C et le C++ des dépendances sont
  acceptés.
- **Toute la logique du lecteur reste dans le cœur.** S'il manque quelque
  chose, on l'ajoute au cœur, avec ses tests, et non à `app/`. La mise en
  page (retour à la ligne, position des mots, ligne voisine) reste dans
  `app/`.
- **Versions exactes :** `gpui-pre = "=0.3.14"`, `gpui-component =
  "=0.11.0"`. `Cargo.lock` est suivi par git, et aucune mise à jour n'est
  faite en passant.
- **Le cœur ne dépend jamais de GPUI.** Aucun changement dans `macos/`.
- **Tests headless :**
  - aucun test ne montre de fenêtre, ne prend le focus, ne joue de son,
    n'ouvre le micro ni ne prend de raccourci global ;
  - on passe par la plateforme de test de GPUI, avec `Player::silent()` et
    `SilentEngine` ;
  - aucun agent ne lance `dysflow-app` ni une fenêtre réelle : c'est
    l'utilisateur qui le fait.
- **ONNX Runtime :** `ORT_DISABLE_TELEMETRY=1` partout où ORT se charge.
- **Textes de l'interface :** bilingues par `dysflow::tr!` (anglais,
  français), sans exception, erreurs comprises.
- **Commits :**
  - en français, et chacun se termine exactement par
    `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` ;
  - sur la branche `feat/interface-gpui`, partie de `460eaff` ;
  - pas de push, de CI, de PR ni de merge sans l'accord de l'utilisateur.
- **Hygiène :**
  - pas de `cargo fmt` sur tout le dépôt, seulement sur les fichiers
    touchés ;
  - un seul build lourd à la fois ;
  - pas de reset, de rebase, de stash ni de `git clean` ;
  - un build avec `--features mlx` source d'abord
    `scripts/mlx-sources.sh`, sinon mlx-sys se reconstruit.
- **Dossiers interdits :**
  - aucune écriture dans `~/.local`, `/usr/local`, `/Applications` ou
    `~/Library/Application Support/Dysflow` ;
  - les essais qui écrivent une configuration passent par un HOME
    temporaire.
- **Le clavier :** celui de `macos/Sources/Input/KeyMap.swift`, avec Ctrl
  à la place de ⌘ hors du Mac. ⌘M est retiré.
- **Les polices :** `config::READING_FONTS` (literata, atkinson,
  openDyslexic), avec une taille entre `READING_SIZE_MIN` (14) et
  `READING_SIZE_MAX` (40) et `READING_SIZE_DEFAULT` (22).
- **Le contraste :** au moins 4,5:1 pour le texte courant, en clair comme
  en sombre. Tout se fait au clavier, et le focus est toujours visible.

## Review Focus

Les cinq situations que la spec implique et qu'un utilisateur rencontrera,
avec chacune son test dans la tâche qui l'a en charge :

1. **Un document qui grandit encore** (`DocView::complete == false`, PDF
   long ou OCR) : la vue se met à jour à chaque révision, sans perdre le
   mot en cours ni la position de défilement. Tâche 5.
2. **Un mot très long ou une ligne sans espace** (URL, code) plus large que
   la fenêtre : il est coupé dans la largeur et ne déborde jamais. ↑ ↓
   continuent de marcher. Tâche 4.
3. **Coller un presse-papiers vide ou une image sans texte reconnu** : un
   message bilingue dans la barre d'état, et le document en cours reste
   ouvert. Tâche 6.
4. **Un fichier refusé** (format inconnu, PDF chiffré, fichier illisible,
   glissé ou passé en argument) : même chose, l'erreur du cœur affichée
   telle quelle. Tâche 6.
5. **Valider une édition qui a supprimé la phrase en cours ou tout le
   texte** : la lecture reprend à la phrase la plus proche qui existe
   encore. Un texte vide revient à l'état « rien à lire », sans panique.
   Tâche 8.

---

### Task 1 : Socle `dysflow-app` et levée des risques (porte d'arrêt)

**Files :**
- Modify : `Cargo.toml` (membres de l'espace de travail : ajouter `"app"`)
- Create : `app/Cargo.toml`, `app/src/main.rs`, `app/src/lib.rs`, `app/tests/smoke.rs`
- Create : `.superpowers/sdd/<plan>/task-1-risques.md` (le relevé des risques)

**Interfaces :**
- Produces :
  - la crate `dysflow_app` (lib et bin `dysflow-app`) ;
  - les features `mlx = ["dysflow/mlx"]`, `onnx = ["dysflow/kokoro-onnx"]`
    et `ocr = ["dysflow/ocr"]`, avec `default = []`. Les tests construisent
    sans moteur ; les paquets choisissent leurs features ;
  - `pub fn run(path: Option<std::path::PathBuf>)` dans `lib.rs`, qui
    ouvre l'application ; `main.rs` lit `std::env::args_os().nth(1)` ;
  - la dev-dependency `gpui-pre`, avec la feature de sa plateforme de test.

- [ ] **Étape 1 : écrire le test de fumée headless.** Dans
  `app/tests/smoke.rs`, `#[gpui::test] fn opens_an_empty_root_view(cx)`
  crée la vue racine dans la plateforme de test, sans fenêtre réelle, et
  vérifie qu'elle se rend sans erreur.
- [ ] **Étape 2 : le lancer et le voir échouer.**
  `cargo test -p dysflow-app --test smoke` doit échouer parce que la vue
  racine n'existe pas encore.
- [ ] **Étape 3 : écrire `run` et une vue racine vide**, qui affiche
  « Rien à lire / Nothing to read » par `tr!`. Ne pas la lancer.
- [ ] **Étape 4 : le test passe.**
  `cargo test -p dysflow-app --test smoke` doit donner PASS.
- [ ] **Étape 5 : relever les risques dans `task-1-risques.md`**, avec la
  commande et sa sortie pour chacun :
  1. Le nom exact de la feature de test de `gpui-pre` et la macro de test.
     Le test doit tourner sans écran (Mac sans fenêtre). Noter si GPUI
     demande l'outil `metal` de Xcode pour ses shaders.
  2. `cargo build -p dysflow-app --target x86_64-pc-windows-gnu`, si une
     chaîne MinGW est présente sur la machine. Sinon, le noter « à faire
     sur le PC Windows » et donner la commande.
  3. La liste des bibliothèques système liées sous Linux. Sans machine
     Linux, donner la commande pour le PC Linux :
     `python3 -I scripts/elf-version-ref.py` sur le binaire.
  4. Le temps de build à froid et la taille du binaire release, sans
     feature de moteur.
  5. Si GPUI publie son contenu aux lecteurs d'écran (limite acceptée,
     à confirmer).
- [ ] **Étape 6 : commit**
  `feat(app): socle dysflow-app sur GPUI, essai headless`.
- [ ] **Étape 7 : arrêt.** Le contrôleur donne le relevé à l'utilisateur.
  Celui-ci lance lui-même `cargo run -p dysflow-app` sur ses trois
  machines. La suite attend son accord. Si un risque ne se lève pas, on
  ne change ni l'approche, ni le plancher de glibc, ni la règle des tests
  headless sans lui.

### Task 2 : Le cœur construit le lecteur de l'app, avec thème et langue

**Files :**
- Create : `src/reader/build.rs`
- Modify : `src/reader/mod.rs` (`pub mod build;`), `ffi/src/lib.rs`
  (`app_reader` et `silent` appellent le cœur), `src/config.rs` (section
  `[app]`), `src/reader/settings.rs`
- Test : `src/reader/build.rs` et `src/reader/settings.rs` (`mod tests`)

**Interfaces :**
- Produces :
  - `pub fn app_reader(loaded: (Config, Option<String>), compute: tts::AppCompute) -> Reader`,
    déplacée telle quelle depuis `ffi/src/lib.rs:252` ;
  - `pub fn silent_reader() -> Reader`, déplacée depuis le constructeur
    `DysflowReader::silent` : `autoplay = false`,
    `keep_voice_minutes = -1`, `SilentEngine`, `Player::silent()` ;
  - `Config.app.theme: Theme`, où `pub enum Theme { System, Light, Dark }`
    sérialisé en `"system"`, `"light"` ou `"dark"`, avec `System` par
    défaut ;
  - `Config.app.ui_language: String`, où `""` veut dire la langue du
    système, et sinon `"fr"` ou `"en"` ;
  - `SettingsView.theme`, `SettingsView.ui_language`, et dans
    `SettingsPatch` `theme: Option<Theme>` et `ui_language: Option<String>`.

- [ ] **Étape 1 : tests.**
  - `silent_reader_never_autoplays` vérifie que `reader.config().reading.autoplay == false`.
  - `theme_round_trips_through_toml` écrit `[app]\ntheme = "dark"`, le
    relit et vérifie qu'on obtient `Theme::Dark`.
  - `unknown_theme_falls_back_to_system` vérifie que `theme = "violet"`
    donne `Theme::System`, sans erreur de lecture, sur le même principe
    que la police inconnue (`config.rs:274`).
  - `unknown_ui_language_falls_back_to_system` vérifie que `"de"` donne
    `""`.
  - `patch_writes_theme_and_ui_language` applique le patch et vérifie la
    vue rendue.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow --lib reader::build reader::settings config::`
- [ ] **Étape 3 : déplacer, ajouter les champs, brancher `ffi` sur le
  cœur.** Le comportement de l'app Swift ne change pas. Elle ignore les
  nouveaux champs, et `SettingsDiff` du Swift n'est pas touché.
- [ ] **Étape 4 : tout passe.**
  - `cargo test -p dysflow --lib` doit donner PASS ;
  - `cargo build -p dysflow-ffi` doit donner OK (le nom du paquet est
    dans `ffi/Cargo.toml`).
- [ ] **Étape 5 : commit**
  `feat(reader): le cœur construit le lecteur de l'app, thème et langue d'interface`.

### Task 3 : Les touches du lecteur

**Files :**
- Create : `app/src/keymap.rs`
- Test : `app/src/keymap.rs` (`mod tests`)

**Interfaces :**
- Produces :
  - `pub struct KeyPress { pub key: String, pub shift: bool, pub alt: bool, pub control: bool, pub platform: bool }`,
    où `key` est le nom de touche de GPUI (`"space"`, `"left"`, `"home"`,
    `"escape"`, `"n"`…) et `platform` est ⌘ sur Mac ;
  - `pub enum Modifier { Command, Control }` donne la touche de commande
    du système : `Command` sur Mac, `Control` ailleurs ;
  - `pub enum Action { Reader(dysflow::reader::command::Command), Line(isize), Escape, Close, Copy, Paste, OpenFile, OpenSearch, ToggleContents, ShowHelp, OpenSettings, Edit, Follow }` ;
  - `pub fn action(key: &KeyPress, command: Modifier) -> Option<Action>`.

- [ ] **Étape 1 : tests table par table**, qui reprennent
  `KeyMap.swift` cas par cas :
  - `space_toggles_play` : Espace donne `Reader(TogglePlay)` ;
  - `arrows` : ← et → donnent `Reader(Sentences(-1))` et
    `Reader(Sentences(1))`. ↑ et ↓ donnent `Line(-1)` et `Line(1)`. Avec
    Maj, rien ;
  - `home_starts` : Début donne `Reader(Start)` ;
  - `letters` :
    - n et p donnent `Sections(±1)` ;
    - N et P donnent `SearchAdvance(±1)` ;
    - + et - donnent `ChangeSpeed(±0.1)` ;
    - / donne `OpenSearch` ;
    - t donne `ToggleContents` ;
    - e donne `Edit` ;
    - ? donne `ShowHelp` ;
    - f donne `Follow` ;
  - `command_keys_follow_the_platform` : avec `Modifier::Control`, Ctrl+c,
    v, o, f, w et , donnent `Copy`, `Paste`, `OpenFile`, `OpenSearch`,
    `Close` et `OpenSettings`. La même touche avec ⌘ ne donne rien. Avec
    `Modifier::Command`, c'est l'inverse ;
  - `command_m_is_gone` : ⌘M et Ctrl+M ne donnent rien ;
  - `alt_or_extra_modifiers_do_nothing` : ⌥ seul, ou ⌘ et Ctrl ensemble,
    ne donnent rien.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --lib keymap`
- [ ] **Étape 3 : écrire `action`.**
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): les touches du lecteur`.

### Task 4 : La mise en page en lignes et la navigation par ligne

**Files :**
- Create : `app/src/layout.rs`
- Test : `app/src/layout.rs` (`mod tests`)

**Interfaces :**
- Consumes : `dysflow::reader::view::{DocView, BlockView, BlockKindView, WordView}`.
- Produces :
  - `pub struct WordBox { pub sentence: usize, pub word: usize, pub x: f32, pub width: f32 }` ;
  - `pub struct Line { pub block: usize, pub y: f32, pub height: f32, pub words: Vec<WordBox> }` ;
  - `pub struct Layout { pub lines: Vec<Line>, pub height: f32 }` ;
  - `pub trait Measure { fn width(&self, text: &str, word: &WordView, kind: &BlockKindView) -> f32; fn line_height(&self, kind: &BlockKindView) -> f32; }`.
    La vue GPUI l'implémente par la mise en forme du texte de GPUI. Les
    tests utilisent une chasse fixe ;
  - `pub fn lay_out(doc: &DocView, width: f32, spacing: usize, measure: &dyn Measure) -> Layout` ;
  - `impl Layout { pub fn line_of(&self, sentence: usize, word: usize) -> Option<usize>; pub fn neighbour(&self, sentence: usize, word: usize, delta: isize) -> Option<(usize, usize)>; pub fn hit(&self, x: f32, y: f32) -> Option<(usize, usize)>; }`.
    `neighbour` rend le premier mot de la ligne voisine, celui que
    `SeekToWord` reçoit, comme `LineNavigator.swift`.

- [ ] **Étape 1 : tests**, avec une chasse fixe de 10 px par caractère,
  une espace comprise :
  - `wraps_at_width` : « un deux trois » sur 80 px donne 2 lignes, « un
    deux » puis « trois » ;
  - `overlong_word_is_split_never_overflows` : un mot de 30 caractères
    sur 100 px donne des boîtes dont chacune a `x + width <= 100` ;
  - `neighbour_moves_by_displayed_line` : depuis la ligne 0, +1 rend le
    premier mot de la ligne 1. Depuis la dernière ligne, +1 rend `None` ;
  - `neighbour_crosses_blocks` : du dernier mot d'un paragraphe, +1 rend
    le premier mot du bloc suivant ;
  - `hit_finds_the_word_under_the_pointer`, et `hit` hors du texte rend
    `None` ;
  - `spacing_adds_blank_lines` : `spacing = 1` double la hauteur d'un
    paragraphe d'une ligne ;
  - `empty_document_has_no_lines`.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --lib layout`
- [ ] **Étape 3 : écrire `lay_out`.** Les retours à la ligne se font
  mot à mot, avec une coupure dans le mot seulement quand il est plus
  large que la ligne. Le code et les tableaux suivent `display_lines`,
  ligne par ligne.
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): mise en page en lignes et navigation par ligne`.

### Task 5 : La vue de lecture

**Files :**
- Create : `app/src/reading.rs` (vue GPUI), `app/src/session.rs`
  (le lien avec `ReaderHandle`)
- Test : `app/tests/reading.rs`

**Interfaces :**
- Consumes :
  - `layout::{lay_out, Layout, Measure}` (tâche 4) ;
  - `keymap::Action` (tâche 3) ;
  - `reader::build::silent_reader` (tâche 2).
- Produces :
  - `pub struct Session { handle: ReaderHandle }`, avec
    `pub fn new(handle: ReaderHandle) -> Self`,
    `pub fn send(&self, r: Request)`,
    `pub fn snapshot(&self) -> Snapshot` et
    `pub fn document(&self) -> Arc<DocView>` ;
  - `pub struct ReadingView`, avec `pub fn new(session: Arc<Session>, cx) -> Self`,
    `pub fn following(&self) -> bool`, `pub fn follow(&mut self, cx)` et
    `pub fn line(&mut self, delta: isize, cx)` ;
  - le rafraîchissement : une tâche GPUI lit l'instantané toutes les 16 ms
    tant que `playing` est vrai, sinon toutes les 250 ms, et ne
    redessine que si l'instantané ou la révision a changé.

- [ ] **Étape 1 : tests headless**, avec `silent_reader` dans un
  `ReaderHandle`, `OpenText` d'un petit Markdown et la plateforme de
  test :
  - `highlights_the_current_word` : après `SeekToWord{sentence: 1, word: 2}`,
    la vue déclare (par un accesseur de test) le mot en évidence `(1, 2)` ;
  - `scrolling_detaches_and_follow_reattaches` : un défilement manuel fait
    passer `following()` à faux, puis `follow()` le remet à vrai ;
  - `line_down_sends_seek_to_neighbour` : `line(1)` envoie `SeekToWord`
    vers `Layout::neighbour` ;
  - `click_seeks_to_word` ;
  - `growing_document_keeps_word_and_scroll` (Review Focus 1) : une
    nouvelle révision avec plus de blocs garde le mot en évidence et la
    position de défilement.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --test reading`
- [ ] **Étape 3 : écrire `Session` et `ReadingView`.**
  - Le rendu passe par un élément GPUI personnalisé : un fond arrondi
    sous le mot en cours, et la règle sous la ligne si
    `reading_ruler` est actif.
  - La police vient de `SettingsView.reading_font` et
    `SettingsView.reading_size`.
  - La ligne lue reste dans le tiers haut de la vue tant qu'on suit le
    texte, comme `FollowPolicy.swift`.
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): vue de lecture avec mise en évidence du mot`.

### Task 6 : La fenêtre, les entrées et la barre de lecture

**Files :**
- Create : `app/src/window.rs`, `app/src/transport.rs`, `app/src/help.rs`,
  `app/src/inputs.rs`
- Test : `app/tests/window.rs`

**Interfaces :**
- Consumes : `Session`, `ReadingView` (tâche 5) ; `keymap::action` (tâche 3).
- Produces :
  - `pub struct MainWindow` : la vue racine, qui remplace celle de la
    tâche 1. Elle porte la vue de lecture, la barre de lecture, la barre
    d'état, l'aide et les panneaux des tâches 7 à 9 ;
  - `pub fn open_path(session: &Session, path: &Path) -> Result<(), String>`,
    par `Request::OpenFile` et sa réponse ;
  - `pub fn paste(session: &Session) -> Result<(), String>`, par
    `dysflow::source::clipboard()` puis `Request::OpenText`. Le texte
    renvoyé par `clipboard()` est déjà en Markdown ;
  - `pub fn status_text(s: &Snapshot) -> String` : ce que dit la barre
    d'état (voix, téléchargement, erreur, avis).

- [ ] **Étape 1 : tests headless.**
  - `argument_path_is_opened` : `run` avec un fichier .md temporaire, et
    le document a une révision non nulle.
  - `dropped_file_is_opened` : une dépose simulée de chemins externes
    dans la plateforme de test.
  - `refused_file_keeps_current_document` (Review Focus 4) : un `.xyz` et
    un PDF illisible. L'erreur du cœur apparaît dans `status_text`, et
    `revision` ne change pas.
  - `empty_clipboard_keeps_current_document` (Review Focus 3) : passe par
    une fonction de presse-papiers injectée
    (`paste_with(session, || Err(...))`), jamais par le vrai
    presse-papiers.
  - `keys_reach_the_reader` : Espace dans la fenêtre fait passer
    `playing` à vrai, avec `SilentEngine`.
  - `help_lists_every_key` : chaque action de `keymap` a sa ligne d'aide.
  - `transport_buttons_send_commands` : chaque bouton (phrase
    précédente, lecture/pause, phrase suivante, vitesse) envoie sa
    commande. Les boutons ne prennent pas le focus.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --test window`
- [ ] **Étape 3 : écrire la fenêtre.**
  - La boîte d'ouverture vient du système (`prompt_for_paths` de GPUI),
    filtrée sur md, txt, pdf, png, jpg, jpeg, tiff et webp.
  - La barre de lecture vient du mode moyen de l'app Mac
    (`2026-10-08-modes-affichage-design.md`) : temps écoulé, frise,
    temps restant, vitesse, transport, et un bouton de réglages.
  - Les composants viennent de `gpui-component`, avec leurs états de
    survol, de focus et de désactivation.
  - Ctrl+C (⌘C sur Mac) copie la sélection, sinon la phrase en cours.
  - Ctrl+W (⌘W) ferme et arrête la lecture.
  - Échap ferme ce qui est ouvert (aide, recherche, sommaire, édition),
    sinon il ferme la fenêtre.
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): fenêtre, entrées et barre de lecture`.

### Task 7 : La recherche et le sommaire

**Files :**
- Create : `app/src/search.rs`, `app/src/contents.rs`
- Test : `app/tests/search_contents.rs`

**Interfaces :**
- Consumes : `MainWindow` (tâche 6) ; `Command::{Search, SearchAdvance, SearchCancel, SeekTo}` ; `Snapshot.search`, `Snapshot.section` ; `DocView.sections`.
- Produces : `SearchBar` (champ de `gpui-component`) et `ContentsPanel` (liste des titres).

- [ ] **Étape 1 : tests.**
  - `typing_searches_and_counts` : saisir « deux » fait que le
    `Snapshot.search` donne la requête, la position 1 et le total.
  - `n_and_p_advance` : N et P avancent dans les résultats.
  - `escape_cancels_search`.
  - `contents_lists_sections_and_marks_current` : la section de
    `Snapshot.section` est marquée.
  - `choosing_a_section_seeks` : Entrée sur un titre envoie
    `SeekTo(first_sentence)`.
  - `section_without_sentence_is_disabled` : un titre dont
    `first_sentence` vaut `None` est affiché comme désactivé.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --test search_contents`
- [ ] **Étape 3 : écrire la barre de recherche et le panneau.**
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): recherche et sommaire`.

### Task 8 : La modification du texte

**Files :**
- Create : `src/reader/editable.rs` (cœur), `app/src/editor.rs`
- Modify : `src/reader/mod.rs`
- Test : `src/reader/editable.rs` (`mod tests`), `app/tests/editor.rs`

**Interfaces :**
- Produces :
  - `pub fn editable_markdown(reader: &Reader) -> Option<String>` : le
    texte ouvert tel quel (`Reader::text_markdown`) s'il existe, sinon
    `markdown_from(&DocView)` ;
  - `pub fn markdown_from(doc: &DocView) -> String` : le port exact de
    `macos/Sources/Model/EditableText.swift` ;
  - `pub fn resume_sentence(previous: usize, new_count: usize) -> Option<usize>` :
    `None` si `new_count == 0`, sinon `min(previous, new_count - 1)`.

- [ ] **Étape 1 : tests du cœur.** Ils reprennent les cas Swift :
  - `headings_lists_quotes_code_tables_keep_their_form` : `# Titre`,
    `  - item`, `1. item`, `> cite`, ```` ```rust ````, et un tableau avec
    sa ligne `| --- |` ;
  - `resume_sentence_clamps` : `(5, 3)` donne `Some(2)` et `(5, 0)` donne
    `None`.
- [ ] **Étape 2 : tests de l'app.**
  - `e_opens_editor_with_markdown` ;
  - `validating_reopens_and_resumes_at_clamped_sentence` (Review
    Focus 5) : on supprime la moitié du texte, on valide, et la phrase en
    cours vaut `resume_sentence` ;
  - `validating_empty_text_shows_nothing_to_read` ;
  - `escape_validates_like_the_mac`, qui suit le commentaire de
    `ReaderAction.escape` : « la saisie se valide ».
- [ ] **Étape 3 : les voir échouer.**
  - `cargo test -p dysflow --lib reader::editable`
  - `cargo test -p dysflow-app --test editor`
- [ ] **Étape 4 : écrire.**
  - L'éditeur est le champ multiligne de `gpui-component`.
  - La validation envoie `OpenText { markdown, source }` avec la même
    `source`, puis `SeekTo(resume_sentence(..))`.
- [ ] **Étape 5 : tout passe.**
- [ ] **Étape 6 : commit** `feat: modifier le texte lu`.

### Task 9 : La page des réglages, le thème et les polices

**Files :**
- Create : `app/src/settings.rs`, `app/src/theme.rs`, `app/src/fonts.rs`,
  `app/assets/fonts/` (les trois familles et leurs licences OFL, copiées
  depuis `macos/Resources/Fonts`)
- Modify : `NOTICE` (gpui-pre, gpui-component, et les polices dans le
  binaire de l'app)
- Test : `app/tests/settings.rs`, `app/src/theme.rs` (`mod tests`)

**Interfaces :**
- Consumes : `SettingsView`, `SettingsPatch` et `Theme` (tâche 2) ;
  `Reader::{settings, update_settings, voices, install_voice, use_voice}` par
  `Request::Ask` ; `preview` (aperçu de voix).
- Produces :
  - `SettingsPage`, sur un seul écran, dans cet ordre : voix (avec le
    bouton Écouter), vitesse, thème, police (famille et taille), langue
    de l'interface ;
  - `pub fn resolve(theme: Theme, system_dark: bool) -> Appearance` ;
  - `pub fn contrast(fg: Rgb, bg: Rgb) -> f32`.

- [ ] **Étape 1 : tests.**
  - `every_change_is_saved_at_once` : avec un HOME temporaire, on change
    la vitesse, et `config.toml` contient la nouvelle valeur.
  - `voice_list_is_the_language_s` : seules les voix de la langue choisie
    sont listées.
  - `font_size_is_clamped` : le curseur ne descend pas sous 14 et ne
    monte pas au-dessus de 40.
  - `ui_language_switches_live` : en passant à `"en"`, le titre de la
    page devient anglais, par `i18n::set_ui_lang`.
  - `theme_system_follows_appearance`.
  - `palette_meets_contrast` : chaque paire texte/fond des deux thèmes
    donne au moins 4,5.
  - `fonts_are_embedded` : les trois familles sont enregistrées au
    démarrage, sans police du système.
  - `preview_never_plays_in_tests` : l'aperçu passe par `Player::silent()`.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --test settings --lib theme`
- [ ] **Étape 3 : écrire.**
  - L'identité visuelle (palette, typographie) se choisit après avoir
    étudié de près une vraie app de lecture très finie. Le style vient de
    `frontend-design`, jamais l'organisation de l'écran.
  - Le seul point fort de l'écran est la voix.
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): réglages, thème et polices de lecture`.

### Task 10 : Premier lancement, voix de repli et absence de GPU

**Files :**
- Modify : `app/src/lib.rs` (`run`), `app/src/window.rs`, `app/src/transport.rs`
- Create : `app/src/startup.rs`
- Test : `app/tests/startup.rs`

**Interfaces :**
- Consumes :
  - `Reader::ensure_default_voice` (`src/reader/migration.rs:197`) par
    `Request::Ask` ;
  - `Snapshot.download` (`DownloadView { received, total, failed }`) ;
  - `Snapshot.voice` (`VoiceState::Fallback`) ;
  - `app_reader` (tâche 2), avec `AppCompute::default()`. Sur Mac,
    `--features mlx` fait choisir Kokoro MLX en Rust (`tts::rust_mlx`).
- Produces :
  - `pub fn first_launch(session: &Session)` ;
  - `pub fn download_label(d: &DownloadView) -> String` ;
  - `pub fn no_gpu_message(err: &str) -> String`, bilingue.

- [ ] **Étape 1 : tests.**
  - `download_label_shows_percent_and_failure` : `(50, 200)` donne
    « 25 % ». Un `failed` donne le message et « Réessayer ».
  - `first_launch_asks_core_to_install` : un lecteur muet dans un HOME
    temporaire, avec un téléchargement factice injecté. Aucun accès
    réseau dans les tests.
  - `reading_starts_on_fallback_voice` : `VoiceState::Fallback` permet
    de lire, et la barre le dit.
  - `no_gpu_message_is_bilingual`.
- [ ] **Étape 2 : les voir échouer.**
  `cargo test -p dysflow-app --test startup`
- [ ] **Étape 3 : écrire.**
  - `run` positionne `ORT_DISABLE_TELEMETRY=1` avant tout.
  - `run` construit `app_reader(Config::load(), AppCompute::default())`
    et appelle `first_launch`.
  - Si GPUI ne peut pas ouvrir de fenêtre, `run` écrit `no_gpu_message`
    sur la sortie d'erreur et sort avec le code 1.
- [ ] **Étape 4 : tout passe.**
- [ ] **Étape 5 : commit** `feat(app): premier lancement et voix de repli`.

### Task 11 : Paquets Windows et Linux, CI

**Files :**
- Modify : `scripts/package.sh`, `.github/workflows/ci.yml`,
  `scripts/windows-imports.py` (liste blanche, si les DLL de GPUI
  l'exigent)
- Test : `scripts/test_windows_imports.py` (s'il est touché)

- [ ] **Étape 1 : ajouter `dysflow-app` au paquet.**
  - `package.sh` construit `-p dysflow-app --release --features onnx,ocr`
    et place le binaire à côté de `dysflow` dans le zip Windows et le tar
    Linux.
  - Le Mac n'est pas touché (`package-macos.sh`).
- [ ] **Étape 2 : la CI.** La matrice existante construit et lance
  `cargo test -p dysflow-app` sur les quatre systèmes. Les tests passent
  par la plateforme de test, sans GPU ni serveur graphique.
- [ ] **Étape 3 : vérification locale.**
  - `python3 -I scripts/test_windows_imports.py` doit donner PASS.
  - Pour `package.sh`, faire un essai à sec sur Mac s'il le permet,
    sinon le noter.
  - Ne lancer aucune CI : la demande passe par le contrôleur, puis par
    l'utilisateur.
- [ ] **Étape 4 : mesurer.** Noter dans le rapport la taille ajoutée au
  paquet et le temps ajouté au build.
- [ ] **Étape 5 : commit** `ci: dysflow-app dans les paquets Windows et Linux`.
