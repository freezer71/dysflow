# Une interface macOS : lire d'un raccourci

Conception validée section par section le 2026-10-05. Elle remplace
l'interface egui retirée le 18 septembre (`5c2a0fe`).

## Pourquoi

L'utilisateur est dyslexique et développeur. Il lit beaucoup — specs,
documentation, articles — pour apprendre et synthétiser. Il lit en
**bimodal** : il écoute et suit le texte des yeux.

Ce qu'il veut :

- **Lire sans délai**, comme on lit normalement : avancer, reculer, sauter des
  mots, accélérer, retrouver une information vite.
- **Lancer la lecture d'un raccourci global**, depuis n'importe quelle
  application, avec ses réglages habituels.
- À terme, **ne plus passer par le terminal**. La fenêtre devient le lecteur
  principal.
- Une interface **jolie, fluide, accessible, entièrement au clavier**.

## Ce qui a échoué la première fois

L'interface egui (2–18 septembre) a échoué sur trois plans à la fois :

| Plan | Ce qui n'allait pas |
|---|---|
| Rendu | Aspect « egui », peu soigné |
| Expérience | Trois colonnes, pas fluide à l'usage |
| Accessibilité | Clavier et lecteur d'écran incomplets |

Les trois ensemble désignent l'outil, pas seulement son usage. D'où le
changement de boîte à outils.

Leçon gardée : **les contrôles restent petits (13 pt) et ne grandissent jamais
avec le texte lu.** Une variante aux boutons géants avait été rejetée.

## Périmètre

- **macOS seul**, macOS 14 minimum, Apple Silicon. Linux viendra peut-être
  plus tard ; le moteur et le terminal restent portables.
- Le terminal (`sillon`) continue de fonctionner, inchangé.
- Hors périmètre : signature et notarisation, distribution publique,
  historique des textes lus, notes.

## Choix technique

**Application native SwiftUI + AppKit, moteur Rust appelé via UniFFI, dans le
même processus.**

| | SwiftUI + Rust | Tauri | Tout Rust (iced, GPUI) |
|---|---|---|---|
| Panneau type Spotlight | `NSPanel` natif | Plugin, fragile | À bricoler |
| VoiceOver, clavier | Celui du système | Bon (WebKit) | Faible |
| Raccourci global, sélection | API Apple | Plugins | À bricoler |
| Langages | Swift + Rust | TS + Rust | Rust |

Les trois reproches faits à egui sont précisément ce que macOS fait le mieux
en natif. Viser macOS seul rend ce choix possible.

## Architecture

```
 Sillon.app (Swift)                    sillon (Rust)
 ┌──────────────────────────┐        ┌───────────────────────────┐
 │ Panneau · Lecteur agrandi │ appels │ Reader (vit tout le temps) │
 │ Raccourcis globaux        │ ─────▶ │  ├ voix chargée (Arc)      │
 │ Capture du texte          │        │  └ Session (une par texte) │
 │ Menu dans la barre        │ ◀───── │     = l'App actuelle       │
 └──────────────────────────┘ instant.└───────────────────────────┘
            liaison générée par UniFFI (crate sillon-ffi)
```

| Pièce | Où | Rôle |
|---|---|---|
| `sillon` | crate actuel | Document, voix, audio, positions, réglages. Le terminal s'en sert toujours. |
| `Reader` | `src/reader.rs` (nouveau) | Possède la voix (`Arc<dyn TtsEngine>`) et la sortie audio. Ouvre une `Session` par texte sans recharger la voix. La décharge après un délai sans lecture. |
| `Session` | `App` actuel | Inchangé dans son rôle : un document, sa lecture, sa position. |
| `sillon-ffi` | `ffi/` (nouveau crate, workspace) | Expose `Reader`, les commandes et l'instantané à Swift, via UniFFI. Bibliothèque statique. |
| `Sillon.app` | `macos/` (nouveau) | Panneau, lecteur, raccourcis, capture, réglages, menu. `LSUIElement` : pas d'icône dans le Dock. |

### Échanges entre Swift et Rust

- Un fil Rust fait tourner la boucle actuelle (`App::tick`).
- **Swift → Rust : des commandes**, par appel de fonction (`toggle_play`,
  `seek_words`, `seek_sentences`, `set_speed`…). Elles sont mises en file et
  appliquées par le fil Rust.
- **Rust → Swift : un instantané** (`Snapshot`) lu à chaque image affichée :
  phrase et mot en cours, progression, temps, vitesse, état de lecture, état
  de la voix, message éventuel. Pas de rappels croisés : rien de fragile.
- Le document arrive à Swift **déjà découpé** : blocs, nature (titre, code,
  tableau, paragraphe), mots avec leurs index. Swift ne fait que l'afficher.
  Il est transmis une fois par session, puis par ajouts pour un flux.

### Travail côté Rust

1. **`Reader`** : posséder voix et sortie audio hors de `App` ; `open_text`,
   `open_file` ouvrent une session neuve en réutilisant la voix.
   (`open_document` a été retiré avec egui ; il revient sous cette forme.)
2. **Saut mot par mot** : `App` ne sait avancer que par phrase, bloc ou
   section. Il connaît le mot en cours (`current_word`), mais il faut pouvoir
   **reprendre l'audio au milieu d'une phrase**, à l'instant du mot visé.
3. **Déchargement** : relâcher la voix après le délai, la recharger à la
   demande.
4. **Instantané** : une structure plate, copiable, sans référence.

## Le panneau et le lecteur

### Le panneau

Ce qui s'ouvre au raccourci : un `NSPanel` non activant, comme Spotlight.

- Centré, au-dessus de l'application en cours, **qui reste active**.
- Il prend le clavier tant qu'il est affiché.
- Sa hauteur suit le texte jusqu'à une limite ; au-delà, le texte défile tout
  seul avec la lecture.
- Contenu : le texte, le mot surligné, la ligne soulignée ; une barre (lecture,
  progression, temps, vitesse) ; trois aides (`⏎` agrandir, `esc` masquer,
  `?` raccourcis).

### Le lecteur agrandi

`⏎` fait grandir **la même fenêtre** en lecteur complet.

- Le texte prend **toute la largeur** (l'utilisateur ne veut pas de colonne
  étroite).
- Sommaire et signets à gauche, masquables (`t`, `B`).
- Fil d'Ariane de section en haut, recherche (`/`).
- Ligne de temps avec un repère par section.
- `Échap` revient au panneau.

### Le clavier

Celui du terminal, avec ces changements :

| Touche | Rôle |
|---|---|
| `Espace` | Lecture / pause |
| `⌥←` `⌥→` | **Mot** précédent / suivant (nouveau) |
| `←` `→` | Phrase précédente / suivante |
| `↑` `↓` | Paragraphe précédent / suivant |
| `n` `p` | Titre suivant / précédent |
| `+` `-` · `1`…`4` | Vitesse · 0,75× 1× 1,5× 2× |
| `r` | Reprendre la phrase |
| `/` · `N` `P` | Chercher · résultat suivant / précédent |
| `t` · `B` · `b` | Sommaire · signets · poser un signet |
| `⏎` | Agrandir le panneau (nouveau) |
| `⌘C` | Copier la phrase en cours (nouveau) |
| `⌘,` | Réglages (remplace `s`) |
| `?` | Aide des raccourcis |
| `Échap` | Lecteur → panneau → masqué, **avec mise en pause** |

## Raccourcis globaux et capture

### Deux raccourcis, modifiables

| Raccourci | Par défaut | Effet |
|---|---|---|
| Lire | `⌃⌥Espace` | Capture le texte, ouvre le panneau, lit tout de suite |
| Afficher | `⌃⌥S` | Rouvre le panneau sur le dernier texte, là où il en était ; sans texte, le panneau vide invite à sélectionner ou copier |

Bibliothèque `KeyboardShortcuts` (sindresorhus) : enregistrement sans
autorisation, enregistreur dans les réglages, conflit signalé.

### Ordre de capture de « Lire »

1. **Sélection par l'API d'accessibilité** (`AXSelectedText` de l'élément
   focalisé). Instantané, presse-papiers intact.
2. **`⌘C` simulé** si l'application ne donne rien (Chrome, Electron…) :
   attendre le changement de `changeCount`, lire, **restaurer le
   presse-papiers précédent** (tous ses types).
3. **Presse-papiers** tel quel. Une image passe par la reconnaissance de texte
   existante (feature `ocr`).
4. **Rien** : une ligne dans le panneau (« Sélectionnez ou copiez du
   texte »), pas de lecture.

Le texte capturé passe par le même `DocumentBuilder` que le terminal. Chaque
capture ouvre une session ; la position de la précédente est retenue.

### Autorisations et démarrage

- Les étapes 1 et 2 demandent **Accessibilité**. Premier lancement : un écran
  explique pourquoi et ouvre le réglage système.
- Sans elle, sillon fonctionne avec le presse-papiers seul ; un rappel
  discret, une fois, dans le menu.
- « Ouvrir à la connexion » (`SMAppService`), activé par défaut.

## La voix résidente et la mémoire

### Mesures (M5, Supertonic `supertonic-3-orig`, 2026-10-05)

| Étape | Temps | Mémoire du processus |
|---|---|---|
| Au repos, sans voix | — | 8 Mo |
| Chargement de la voix | 0,3 s | ~250 Mo |
| Première phrase prête | +0,35 s | ~580 Mo |
| Après déchargement | — | 185–200 Mo (≈ 90 Mo au premier essai) |

### Dix cycles (banc `examples/voice_cycles.rs`, M5, 2026-10-05)

| Modèle | Chargé (moyenne) | Relâché cycle 1 | Relâché cycle 3 | Relâché cycle 10 | Verdict |
|---|---|---|---|---|---|
| `supertonic-3-orig` | 638 Mo | 256 Mo | 284 Mo | 289 Mo | Pas de fuite : plafond ~290 Mo |
| `supertonic-3-f16` | 479 Mo | 51 Mo | 272 Mo | 268 Mo | Pas de fuite : plafond ~270 Mo |

- Écart cycle 3 → cycle 10 : 5 Mo (orig), 4 Mo (f16). Sous le seuil de 20 Mo.
- Le relevé « relâché » ne monte pas à chaque cycle : il oscille (orig 32 à
  289 Mo, f16 47 à 272 Mo). Cache Metal ou allocateur, pas une fuite.
- **Le palier dépasse 120 Mo** (~270 à 290 Mo, pointes basses à 32–51 Mo). Le
  seuil « < 120 Mo au repos » devra être revu avec l'utilisateur à la
  Tâche 21.
- Chargement 0,25–0,4 s, première phrase ~0,5 s à chaud (1,3–1,5 s au premier
  cycle).
- **Modèle par défaut : `f16`**, ~160 Mo de moins une fois chargé, même temps
  de chargement et de première phrase. Qualité égale à confirmer à l'écoute.

### Politique

- **Voix par défaut : Supertonic** (20× le temps réel, saut en 0,25 s).
  MagpieTTS reste disponible, pas par défaut : 1,5 Go et plus lente que le
  temps réel.
- **Au raccourci, la voix se charge pendant la capture**, en parallèle.
- **Elle reste chargée 10 minutes après la dernière lecture**, puis se
  décharge. Réglable : délai, « toujours », « jamais ».
- Le menu de la barre affiche « voix prête » ou « voix au repos ».

### Anomalie à traiter d'abord

Après déchargement, **environ 100 Mo ne reviennent pas**, et le chiffre varie
d'un essai à l'autre. Cache Metal, allocateur ou fuite d'audio.cpp : inconnu.

Première tâche du plan : **dix cycles chargement/déchargement**, mémoire
relevée à chaque cycle. Si elle monte à chaque cycle, c'est une fuite, et le
déchargement ne sert à rien tant qu'elle n'est pas corrigée. Comparer aussi
`supertonic-3-f16` (313 Mo) à `orig` (454 Mo).

## Style visuel

**Direction « Papier »**, validée sur maquette.

| | Clair (défaut) | Nuit (mode sombre) |
|---|---|---|
| Fond du panneau | crème `#fbf8f1` | brun très sombre `#1e1b17` |
| Texte | `#2a2722` | crème atténué `#e8e1d4` |
| Texte déjà lu | `#9a948a` | `#8a8276` |
| Mot surligné | fond `#cfe3ff`, texte `#1b2a44` | fond `#2c4a75`, texte `#e6efff` |
| Ligne lue | soulignée `#7aa7e8` | soulignée `#7aa7e8` |
| Police de lecture | **Literata** (embarquée, OFL) | idem |

- Polices de lecture proposées : Literata (défaut), Atkinson Hyperlegible,
  OpenDyslexic. Elles sont embarquées dans l'app.
- Interface (barre, menus, réglages) : police système, **13 pt, fixe**.
- **Contraste élevé** (noir, blanc, jaune) appliqué automatiquement quand
  « Augmenter le contraste » est actif dans macOS.
- Réglages de lecture du terminal repris : taille, interlignage, règle de
  lecture.

## Accessibilité

- **Tout au clavier** : focus toujours visible, ordre de tabulation logique,
  réglages navigables au clavier.
- **VoiceOver** : chaque contrôle a un nom et une valeur (« Vitesse, 1,4× »).
  Quand VoiceOver est actif, le mot surligné n'est jamais annoncé ; seuls les
  changements d'état le sont (lecture, pause, nouvelle section), pour que les
  deux voix ne se chevauchent pas. *(Révisé le 2026-10-05 : macOS n'offre
  aucune API publique pour savoir quand VoiceOver parle ; « mettre en pause
  quand VoiceOver parle » n'est pas réalisable.)*
- Réglages système respectés : réduire les animations, augmenter le
  contraste, réduire la transparence. *(macOS n'a pas de taille de texte
  système ; la taille du texte lu se règle dans sillon.)*

## Fluidité

- Le surlignage **glisse** d'un mot au suivant en moins de 80 ms. Il est
  immédiat si « réduire les animations » est actif.
- **La ligne lue reste à hauteur fixe** : le texte défile, pas le regard.
- Rendu par TextKit 2, à la fréquence de l'écran.

## Erreurs

| Situation | Réaction |
|---|---|
| Rien à lire | Une ligne dans le panneau, pas de lecture |
| Accessibilité refusée | Presse-papiers seul, rappel unique dans le menu |
| Voix absente ou corrompue | Voix du système proposée, lien vers les réglages |
| Phrase refusée par la voix | Sautée et comptée, comme aujourd'hui |
| Pas de sortie audio | Message dans le panneau, texte lisible |
| Raccourci déjà pris | Signalé dans les réglages à l'enregistrement |

## Tests

- **Rust** : `Reader` (une voix sur plusieurs sessions, déchargement après le
  délai), saut mot par mot, instantané fidèle aux commandes. `cargo test`
  reste vert ; le terminal ne change pas.
- **Swift** : choix de la source (sélection → `⌘C` simulé → presse-papiers,
  restauration du presse-papiers), correspondance touches → commandes.
- **XCUITest** : ouvrir, naviguer sans souris, agrandir, fermer ; audit
  d'accessibilité automatique (`performAccessibilityAudit`) sur chaque écran.

## Critères d'acceptation mesurés

| Mesure | Seuil |
|---|---|
| Raccourci → premier son, voix chargée | < 0,5 s |
| Raccourci → premier son, à froid | < 0,9 s |
| Saut de mot ou de phrase | < 0,3 s |
| Images perdues pendant la lecture (Instruments) | aucune |
| Mémoire au repos, voix déchargée | < 120 Mo, stable sur dix cycles |

## Construction

- `scripts/build-macos.sh` : compile `sillon-ffi` en bibliothèque statique
  (avec `audio-cpp` et `ocr`), génère la liaison Swift, construit
  `Sillon.app` avec `xcodebuild`.
- Le shim audio.cpp est une bibliothèque partagée : il est copié dans
  `Sillon.app/Contents/Frameworks`, avec un `rpath` relatif. (Le `rpath`
  absolu actuel casse dès que le binaire bouge — dette déjà connue.)
