# Une interface graphique

Conception rédigée le 2026-09-02. Elle attend la relecture de l'utilisateur ;
les décisions prises sans lui sont marquées **(décision par défaut)**.

## Pourquoi

Le lecteur vit dans un terminal, et le terminal impose sa typographie : une
seule police, une seule taille, des cellules carrées, pas d'interlignage réel.
Or l'outil est fait pour être **suivi des yeux** pendant que la voix avance,
par un lecteur dyslexique. Ce que le terminal ne peut pas offrir est
précisément ce qui compte pour lui :

| Le terminal | Une fenêtre |
|---|---|
| La police du terminal | Une police faite pour la lisibilité, embarquée |
| Une taille de texte | La taille qu'on veut, au réglage près |
| Une ligne vide ou rien | Un interlignage continu, en fraction de ligne |
| Le mot surligné change de cellule | Le mot surligné garde sa place, la ligne ne bouge pas |
| Le clavier seul | Le clavier, la souris, un fichier déposé, un clic sur un mot |
| Lancé depuis un shell | Lancé depuis le Dock ou un menu, comme une application |

Une interface graphique n'est donc pas un habillage : c'est la lecture bimodale
poussée jusqu'au bout.

## Ce qui n'est pas le problème

Le lecteur est déjà **un contrôleur sans écran**. `App` porte le document, le
moteur, la sortie audio, la position, la recherche, les signets, l'écran de
configuration et ses effets. Le terminal n'y ajoute que deux choses : une
fonction de rendu (`ui::render`) et une correspondance touches → commandes
(`keymap`). Aucun type de ratatui ou de crossterm ne sort de ces deux fichiers.

Le travail n'est donc ni le moteur, ni le document, ni les réglages : c'est un
**second rendu** du même `App`, et les gestes qu'une fenêtre permet en plus.

## Boîte à outils retenue, et pourquoi

Quatre voies, jugées sur les contraintes du projet — un binaire portable sur
macOS et Linux (x86_64, ARM64), sans dépendance système à installer, le tout en
Rust :

| | egui / eframe | iced | Tauri (webview) | GTK 4 |
|---|---|---|---|---|
| Langage | Rust seul | Rust seul | Rust + HTML/JS | Rust + C |
| Dépendance système | aucune | aucune | webview du système | GTK installé |
| Paradigme | immédiat, comme ratatui | Elm (messages, `update`) | DOM | retenu, signaux |
| Style par mot dans un texte | `LayoutJob`, natif | `rich_text`, plus limité | CSS, excellent | Pango, correct |
| Police embarquée | `include_bytes!` | oui | oui | fontconfig |
| Interlignage réglable | `line_height`, natif | non | CSS | Pango |
| Lecteur d'écran | AccessKit, par défaut | partiel | natif | ATK |
| Dépendances compilées, à froid | **30 s** (mesuré) | ~2 min | ~3 min + npm | selon GTK |

**egui, par eframe** **(décision par défaut)**. Trois raisons :

- Il est **immédiat**, comme ratatui : `render(frame, app)` devient
  `ui(ui, app)`. Le rendu se porte, il ne se réinvente pas. Le contrôleur reste
  celui du terminal.
- Il ne demande **rien au système** : wgpu choisit Metal sur macOS, Vulkan puis
  OpenGL sur Linux ; les polices sont dans le binaire ; le dialogue de fichiers
  passe par le portail XDG sur Linux et par Cocoa sur macOS, sans GTK.
- Son texte se **compose section par section** : chaque mot reçoit sa couleur,
  son fond, sa police, sans reconstruire de widget — c'est exactement la forme
  du problème.

Tauri est écarté : deux langages, un webview dont la version dépend de la
machine, et WebKitGTK sur Linux — la dépendance captive que le projet refuse.
GTK est écarté pour la même raison. iced est un second choix honorable, mais son
architecture Elm forcerait à réécrire le contrôleur en messages, pour un texte
riche moins souple.

### Ce que coûte eframe

Mesuré sur une application eframe minimale, Apple M5, profil par défaut :

| | Mesuré |
|---|---|
| Compilation des dépendances, debug, à froid | 30 s |
| Compilation des dépendances, release, à froid | 1 min 16 s |
| Binaire release, fenêtre vide | 13,4 Mo · 10,8 Mo strippé |
| Version | eframe 0.36, wgpu 30 |
| Polices embarquées (Atkinson Hyperlegible ×4, OpenDyslexic ×4) | 1,0 Mo |

Le profil release du projet — LTO, une unité de codegen, `strip` — fera un peu
mieux. Le binaire distribué passera donc d'une dizaine de Mo à une vingtaine,
polices comprises.

La feature est optionnelle, comme `audio-cpp` et `ocr` : sans `--features gui`,
rien de tout cela n'est compilé, et `cargo build` reste une affaire de secondes.

## Architecture

### Deux binaires, un contrôleur

```
                 ┌──────────┐      ┌─────────┐
  args ──▶ launch ──▶ App ──▶ │  tui     │  ou  │  gui    │
  (clap)  (prépare)  (contrôle) │ ratatui  │      │ eframe  │
                 └──────────┘      └─────────┘
```

| Module | Rôle | Aujourd'hui |
|---|---|---|
| `app` | Le contrôleur et ce qui lui appartient : `App`, écran de configuration, recherche, ancre de vue, cadence, démarrage, **touches neutres** | dispersé sous `tui::` |
| `launch` | Des arguments à un `App` prêt : lecture des options, moteur, source, position | dans `main.rs` |
| `tui` | Rendu ratatui, adaptation crossterm → touches neutres | `tui::{mod, ui, keymap}` |
| `gui` | Rendu eframe, adaptation egui → touches neutres, gestes de fenêtre | n'existe pas |

Le déplacement de `tui::{app, config, search, view, pace, startup}` vers `app::`
est mécanique et vient en premier, seul dans son commit : le second rendu ne
doit pas importer `tui::app`.

Deux binaires : `sillon` (inchangé) et `sillon-gui` (feature `gui`,
`required-features`). Les deux tiennent en vingt lignes : arguments → `launch`
→ `run`. `install.sh` et `package.sh` posent le second quand la feature est
demandée.

### Touches neutres

Le contrôleur reçoit une touche sans savoir d'où elle vient :

```rust
pub struct Key { pub code: KeyCode, pub shift: bool, pub ctrl: bool }
pub enum KeyCode { Char(char), Left, Right, Up, Down, Enter, Esc, Backspace,
                   Tab, BackTab, Home, End, PageUp, PageDown, Delete }
pub fn handle(app: &mut App, key: Key)
```

La logique de `keymap.rs` — trois contextes, saisie puis configuration puis
lecture — déménage dans `app::keys` avec ses tests. `tui::keymap` ne garde que
la conversion crossterm → `Key` ; `gui::keys` fait de même pour egui. Les
tables d'aide (`HELP`, `CONFIG_HELP`) suivent : les deux rendus affichent les
mêmes raccourcis, parce que ce sont les mêmes.

Deux réserves propres à la fenêtre :

- Quand un champ de saisie egui a le clavier — recherche, réglage texte — les
  touches ne descendent pas au contrôleur. C'est egui qui le dit
  (`memory.focused()`), pas une liste de touches à exclure.
- `Pg↑` `Pg↓` défilent d'une hauteur de fenêtre en pixels, pas de quatre
  blocs : la fenêtre sait ce qu'elle montre.

### Ce que le contrôleur apprend

Quelques méthodes, toutes testables sans fenêtre :

| Ajout | Pourquoi |
|---|---|
| `App::open_document(builder, stream, source)` | Ouvrir un autre fichier **sans recharger le modèle** : le moteur est un `Arc`, la synthèse se reconstruit dessus comme `reload_engine` le fait déjà. La position du document quitté est retenue avant. |
| `App::search_with(query)` | La recherche se tape dans un champ egui, pas caractère par caractère : ouvrir, remplir, valider en un appel. |
| `App::config_set_number / set_choice / set_text(field, value)` | Un curseur ou une liste déroulante pose une valeur ; ils ne font pas « plus un ». Même logique d'effet que `config_commit_edit`, famille et voix réalignées. |
| `Field::control(config) -> Control` | Ce qu'un réglage **est** — curseur, entier, choix, case, texte — avec ses bornes, aujourd'hui enfouies dans `adjust`. Le terminal continue d'appeler `adjust` ; la fenêtre choisit son widget. |
| `Effect::SetVolume(f32)` | Un curseur de volume pose une valeur absolue. |
| `ViewAnchor::pin(block)` | La fenêtre sait quel bloc est en haut de l'écran : elle l'épingle directement, sans compter des crans. |
| `App::apply_effect` public | La rubrique « Affichage », propre à la fenêtre, agit sur des réglages que `Field` ne connaît pas. |

`Control` :

```rust
pub enum Control {
    /// Nombre à virgule ; `unset` porte le libellé et la valeur de départ
    /// quand le réglage peut être laissé au modèle (« (modèle) », 0,7).
    Slider { min: f64, max: f64, step: f64, decimals: usize, unset: Option<(&'static str, f64)> },
    /// Entier ; `zero` nomme la valeur zéro quand elle a un sens à part
    /// (« plein écran », « auto »).
    Integer { min: u32, max: u32, unset: Option<(&'static str, u32)>, zero: Option<&'static str> },
    Choice(&'static [&'static str]),
    Toggle,
    Text,
}
```

## L'écran

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ▣ Ouvrir  ▤ Coller │ rapport.md · § 3.2 Tarif de repli      ⟳ flux │ 🔍 ____ │
├──────────────────────────────────────────────────────────┬───────────────────┤
│                                                          │ Sommaire          │
│   3.2 Tarif de repli                                     │   1 Contexte      │
│   ────────────────────                                   │   2 Périmètre     │
│                                                          │ ▸ 3 Tarifs        │
│   Le tarif de repli s'applique quand le fournisseur      │     3.1 Base      │
│   ▌n'a pas transmis▐ ses relevés dans le délai prévu     │   ▸ 3.2 Repli     │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │   4 Pénalités     │
│   à l'article 12. Il est calculé sur la moyenne des      │                   │
│   trois derniers mois.                                   │                   │
│                                                          │                   │
├──────────────────────────────────────────────────────────┴───────────────────┤
│ ▶  ━━━━━━━━━━━━━━━●─────────────────  12:04 / ~31:10   phrase 218 / 611      │
│ 1,25×  − +  ·  🔊 ───●── ·  Supertonic 3 · 4 prêtes · 1 session   ⚑        │
├──────────────────────────────────────────────────────────────────────────────┤
│ ⏸ vue figée — f ou ↧ revient à la lecture        · , virgule                 │
└──────────────────────────────────────────────────────────────────────────────┘
```

Quatre zones, comme dans le terminal : en-tête, texte (avec panneau latéral
optionnel), barre de lecture, ligne d'état. La configuration remplace le texte
quand elle est ouverte, comme aujourd'hui. Dans le croquis, le bandeau `░` de
la règle est dessiné sous la ligne faute de mieux : à l'écran il passe
**derrière** la ligne du mot prononcé, sur toute la largeur de la colonne.

### Le texte

C'est la zone qui justifie tout le reste. Ses règles :

**La typographie porte la hiérarchie.** Un titre de niveau 1 fait 1,6 fois la
taille du texte, en accent, souligné d'un filet ; un niveau 2, 1,35 fois, clair,
filet fin ; au-delà, 1,15 fois, en gras. Un paragraphe est du texte. Une liste
porte sa puce ou son rang en accent, **dans une colonne à part**, pour que la
suite d'un item pende sous son texte et non sous sa puce — ce que le terminal
fait déjà. Une citation porte un filet vertical sur toute sa hauteur, en
italique atténué.

**Le mot prononcé ne fait pas bouger la ligne.** Dans une police
proportionnelle, passer un mot en gras change sa largeur, et la ligne entière
se recompose à chaque mot : le texte tremble sous les yeux. Le mot prononcé ne
change donc **que de fond et de couleur** — fond accent, texte sombre —, jamais
de graisse. La phrase en cours s'éclaircit de la même façon : couleur seule. La
graisse et l'italique disent ce que le fichier dit, et rien d'autre.

**La règle de lecture est un bandeau.** La ligne qui porte le mot prononcé
reçoit un fond discret sur toute la largeur de la colonne, peint sous le texte
avant lui. Le mot dit où en est la voix ; le bandeau dit où revenir quand on
relève les yeux. C'est la même distinction que dans le terminal, avec la
géométrie réelle des lignes : egui donne la position de chaque rangée du texte
composé, et le mot prononcé se retrouve dans la sienne.

**Hors du bloc lu, le texte s'efface sans disparaître.** Même règle qu'au
terminal : couleur atténuée, graisse et soulignement conservés.

**La page reste celle du fichier — tant qu'elle tient.** Les lignes du document
sont respectées quand chacune tient dans la colonne ; sinon le bloc est
recomposé d'un seul tenant. Le test est le même qu'au terminal, mais mesuré :
le bloc est composé avec ses sauts de ligne, et s'il produit plus de rangées
que le fichier n'en a, il est recomposé.

**Le code et les tableaux se montrent tels qu'ils sont écrits.** En police à
chasse fixe, à 0,85 fois la taille du texte, sur un fond léger ; un filet à
gauche pour le code, la grille du document pour le tableau. Une ligne trop
longue est repliée sous un filet `╎`, jamais tronquée — les `display_lines` du
document sont déjà ce qu'il faut afficher.

**La colonne se règle en caractères, comme aujourd'hui.** Le réglage *Largeur
de colonne* garde son sens et sa valeur ; la fenêtre le convertit en pixels par
la chasse moyenne de la police de lecture. Zéro reste plein écran, le défaut,
parce que c'est ce que l'utilisateur a dit préférer.

**La vue suit la voix, et la lâche au premier geste.** Tant qu'elle suit, la
ligne prononcée est tenue au tiers supérieur de la fenêtre, et le défilement y
va en douceur. Un coup de molette, un glissement de la barre, `Pg↑` : la vue
s'épingle sur le bloc en haut de l'écran, la voix continue, la ligne d'état dit
`⏸ vue figée`, et un bouton `↧` y ramène — comme `f`. Un saut de lecture
recolle la vue, comme au terminal.

**Un clic sur un mot y envoie la voix.** C'est le geste que la fenêtre ajoute :
la position sous le pointeur donne un caractère, le caractère donne sa phrase,
et la lecture repart dessus. Le survol ne change rien à l'écran — pas de
soulignement au passage, qui ferait un second curseur.

**Le texte est composé bloc par bloc, à la demande.** Chaque bloc garde sa
composition (`Galley`) tant que ni sa largeur, ni son style, ni son état — bloc
lu, phrase lue, mot lu — ne changent. À chaque trame, seuls les blocs qui
changent d'état se recomposent : celui qu'on quitte et celui qu'on rejoint. Un
bloc jamais vu a une hauteur estimée sur son nombre de caractères ; il est
composé quand il approche de la fenêtre, avec un budget par trame, et sa
hauteur réelle remplace l'estimation. Quand un bloc au-dessus de la fenêtre
change de hauteur, le décalage de défilement absorbe la différence : ce qui est
à l'écran ne bouge pas. Le coût d'une trame dépend de la fenêtre, pas du
document — le principe du terminal, transposé.

### L'en-tête

Titre du document, nom de la source, section courante en accent, `⟳ flux` tant
que l'entrée n'est pas complète. À gauche, deux boutons : **Ouvrir…** (dialogue
de fichiers) et **Coller** (le presse-papiers, texte ou image reconnue). À
droite, le champ de recherche, puis les boutons **Sommaire**, **Signets**,
**Réglages**, **Aide** — chacun avec sa touche en infobulle.

### Le panneau latéral

Le sommaire ou les signets, à droite du texte, sur demande. Mêmes listes qu'au
terminal (`navigable_sections`, `bookmarks.for_document`), mais **cliquables** :
un clic sur un titre y envoie la lecture, un clic sur un signet aussi, et un
bouton `×` retire un signet. La section en cours est en accent. Les flèches
verticales y circulent quand il est ouvert, comme au terminal.

### La barre de lecture

Deux lignes. La première : lecture/pause, une **barre de progression
cliquable** — cliquer y déplace la lecture à la phrase correspondante —, les
temps mesurés, le rang de la phrase. La seconde : la vitesse avec `−` `+` et
les quatre préréglages, le volume en curseur avec la sourdine, le nom du moteur,
l'avance de synthèse, le drapeau de signet. Avant le premier son, la première
ligne cède la place au sablier et à l'étape en cours, comme au terminal.

### La ligne d'état

Même contenu, mêmes priorités qu'au terminal — `status_spans` en donne l'ordre :
saisie en cours, référence manquante, vue figée, message, voix système,
dernière remarque, rappel des touches ; puis la recherche, la ponctuation dite,
les phrases sautées, la fin. Elle se réécrit en widgets, pas en spans ratatui,
mais le texte est le même.

### La recherche

Un champ dans l'en-tête. `/` lui donne le clavier ; `Entrée` lance la
recherche depuis le curseur ; `Échap` la rend et efface. `N` `P` et deux
boutons parcourent les résultats, soulignés dans le texte comme aujourd'hui.
Le compte `« terme » 3/12` s'affiche à côté du champ.

### La configuration

Elle remplace le texte quand elle est ouverte. À gauche, la liste des
rubriques avec leur résumé — celui du terminal, `Section::summary`. À droite,
la rubrique ouverte, en colonnes cette fois : la fenêtre a la place que le
terminal n'avait pas, et les deux pages successives redeviennent une liste et
un contenu.

| Rubrique | Widgets |
|---|---|
| **Modèles** | Une ligne par paquet : nom, badge (utilisé · installé · à venir · tronqué), taille, langues ; boutons **Installer** / **Utiliser** / **Supprimer** ; barre de progression du téléchargement avec **Interrompre** ; espace disque libre. |
| **Voix**, **Échantillonnage**, **Lecture**, **Moteur** | Un widget par `Field`, choisi par `Control` : curseur, champ entier, liste déroulante, case, champ texte. Sous chacun, son explication (`hint_for`). Un réglage inerte est grisé et marqué `(ignoré par ce modèle)` ; il est masqué sauf si la case **Afficher les réglages sans effet** est cochée. Un réglage laissé au modèle a un bouton **(modèle)** qui le rend. Le chemin du modèle a un bouton **Parcourir…**. La fenêtre ne montre pas *Interlignage* en lignes vides : elle a le sien, en fraction de ligne, dans **Affichage**. |
| **Affichage** *(fenêtre seule)* | Police de lecture, taille, interlignage, thème. Voir *Typographie*. |

En bas : **Écouter la voix** (`t`), **Enregistrer** (`w`), et le message de la
rubrique — aperçu en cours, installation, erreur.

**Supprimer** demande une confirmation : le bouton devient **Confirmer** pendant
trois secondes. Au terminal, `d` supprime d'un coup ; à la souris, un clic de
travers effacerait 1,6 Go.

Les valeurs prennent effet immédiatement, comme au terminal : les mêmes
`Effect` reviennent par les mêmes chemins.

### L'aide

Une fenêtre flottante, les mêmes tables `HELP` et `CONFIG_HELP`, fermée par
n'importe quelle touche ou un clic dehors.

### Ouvrir

Quatre entrées, toutes vers `App::open_document` :

| Geste | Source |
|---|---|
| `sillon-gui fichier` | comme `sillon` : `source::load_with_cache` |
| `cat x \| sillon-gui` | le flux, lu au fil de l'eau, comme au terminal |
| **Ouvrir…**, `Ctrl+O` (`⌘O` sur macOS) | dialogue natif, filtré sur markdown, texte, PDF, images |
| Fichier déposé sur la fenêtre | `dropped_files`, le premier chemin ; un voile « Déposer pour lire » pendant le survol |
| **Coller**, `Ctrl+V` (`⌘V`) | `source::clipboard()` — texte, ou image reconnue avec `ocr` |

L'extraction d'un PDF ou d'un scan prend des secondes : elle part dans un
thread, la ligne d'état dit `extraction de rapport.pdf…`, et l'interface reste
vivante. Le document arrive quand il est prêt ; sa position retenue est
reprise. Sans document — lancé depuis le Dock —, la fenêtre s'ouvre sur les
boutons **Ouvrir…** et **Coller** au centre du texte vide, et non sur la
configuration : dans une fenêtre, régler avant d'avoir quelque chose à lire
n'est pas le geste naturel.

La position se retient à la fermeture (`App::on_exit`) et à chaque changement
de document.

## Typographie et confort

**Deux polices embarquées, une par défaut.** Atkinson Hyperlegible (Braille
Institute, OFL) par défaut **(décision par défaut)** : dessinée pour distinguer
les glyphes qui se confondent — `I` `l` `1`, `O` `0`, `b` `d` —, elle est
sobre et se lit longtemps. OpenDyslexic (OFL) en option : c'est celle qu'on
demande par son nom, et certains lecteurs la préfèrent ; la recherche ne la
donne pas gagnante, l'utilisateur seul le sait pour lui. La police standard
d'egui reste une troisième option. Quatre graisses chacune — régulier, gras,
italique, gras italique —, pour que le gras du fichier soit du vrai gras et non
un faux gras synthétique. Le code garde la police à chasse fixe d'egui.

Les licences OFL accompagnent les fichiers dans `assets/fonts/`.

**Réglages**, dans une rubrique **Affichage** et une section `[gui]` du
`config.toml` :

```toml
[gui]
font = "atkinson"        # atkinson | opendyslexic | default
font_size = 22.0         # pixels
line_height = 1.6        # en fraction de la taille
theme = "dark"           # dark | light
```

| Réglage | Défaut | Pourquoi |
|---|---|---|
| Taille | 22 px | Grande, pour un grand écran et une lecture longue ; réglable de 14 à 40. |
| Interlignage | 1,6 | La recommandation courante pour la lecture accompagnée ; réglable de 1,0 à 2,5. Il remplace *Interlignage* en lignes, qui n'a de sens qu'en cellules ; ce dernier reste au terminal. |
| Thème | sombre | Le terminal l'est ; la palette est la sienne — accent `#F26B27`, gris étagés. Le thème clair inverse les gris et garde l'accent. |

Le mot prononcé, la règle, la phrase en cours gardent leurs rôles et leurs
contrastes dans les deux thèmes.

La police et la taille changent en direct ; toutes les compositions sont
invalidées d'un coup.

## Ce qui change dans le code existant

Dans l'ordre des commits :

1. **Déplacement** de `tui::{app, config, search, view, pace, startup}` vers
   `app::` ; `ReadingStyle` suit. `main.rs` et les tests suivent par `sed`.
   Aucun comportement ne change ; `cargo test` le prouve.
2. **Touches neutres** : `app::keys` reçoit la logique et les tests de
   `tui::keymap`, qui devient une conversion.
3. **`launch`** : `Args` et la préparation sortent de `main.rs` ; `main.rs`
   fait vingt lignes.
4. **Contrôleur** : `open_document`, `search_with`, `config_set_*`,
   `Field::control`, `Effect::SetVolume`, `ViewAnchor::pin`, `[gui]` dans
   `Config`. Tests unitaires pour chacun.
5. **`gui`** : feature, binaire, polices, puis les zones une à une.

## Tests

Ce qui se teste sans fenêtre se teste :

- `app::keys` — les tests de `keymap` déménagent tels quels ; ils portent sur
  `Key`, plus sur crossterm.
- `Field::control`, `set_number`, `set_choice` — bornes, valeurs optionnelles,
  effets rendus.
- `App::open_document` — le moteur est conservé (même `Arc`), la position de
  l'ancien document est retenue, la synthèse repart sur le nouveau.
- `App::search_with` — même résultat qu'une saisie touche à touche.
- `ViewAnchor::pin` — épingle, borne, se recolle.
- `gui::text` — la mise en **runs** d'un bloc : chaque jeton du fichier reçoit
  le style de son mot, dans l'ordre ; le mot prononcé est le seul à porter un
  fond ; un résultat de recherche est souligné ; hors du bloc lu, tout
  s'atténue ; la recomposition se déclenche quand une ligne dépasse. C'est le
  pendant de `document_lines`, sans type egui : il rend une structure que le
  rendu traduit en `LayoutJob`.
- `gui::flow` — l'arithmétique du défilement : hauteur estimée, absorption
  d'un changement de hauteur au-dessus de la fenêtre, position cible de la
  ligne prononcée au tiers.

Ce qui exige une fenêtre se vérifie **à l'œil**, avec des captures d'écran
prises sur cette machine à chaque étape : ouverture, lecture en cours avec
règle et mot, panneau, recherche, configuration, thème clair, police
OpenDyslexic. Les captures ne sont pas versionnées.

## Distribution

- `cargo build --release --features gui,audio-cpp,ocr` produit `sillon` et
  `sillon-gui`.
- `package.sh` copie `sillon-gui` dans `dist/` quand la feature est demandée ;
  le shim est le même, cherché aux mêmes endroits. Sur macOS, il assemble aussi
  `dist/Sillon.app` — un `Info.plist`, le binaire et le shim dans
  `Contents/MacOS` — pour que le lecteur se lance du Dock et reçoive un fichier
  déposé sur son icône. Sur Linux, il écrit `dist/sillon.desktop`.
- `install.sh` pose `sillon-gui` à côté de `sillon`, et le `.desktop` dans
  `~/.local/share/applications` sur Linux.

## Hors périmètre

- Une icône dessinée : le bundle en prend une par défaut. À faire quand le nom
  et le trait seront choisis.
- L'édition du texte, les annotations, plusieurs documents ouverts.
- Windows, écarté par ailleurs.
- Le rendu des images d'un PDF : le lecteur lit du texte.
- Un interlignage en lignes vides au terminal converti vers la fenêtre : les
  deux réglages coexistent, chacun dans son médium.
