# Interface GPUI : le lecteur sur macOS, Windows et Linux

Conception du 2026-11-24. C'est le projet 2 de
`2026-11-21-moteurs-par-plateforme-design.md`. Elle reprend l'intention de
`2026-10-15-simplification-ux-design.md`, « écouter un texte en le suivant des
yeux », ainsi que le clavier du lecteur Mac (`macos/Sources/Input/KeyMap.swift`).

## Pourquoi

Dysflow doit tourner sur macOS, Windows et Linux, et l'utilisateur ne veut
maintenir que du Rust, interface comprise. Le C++ des dépendances est
accepté. Les moteurs existent déjà en Rust sur les trois systèmes (MLX sur
Mac, ONNX Runtime ailleurs). Ce qui manque, c'est une interface de bureau
commune. Aujourd'hui, Windows et Linux n'ont que le terminal, et le Mac a son
app Swift (environ 17 500 lignes).

On commence par le **lecteur** (choix A de l'utilisateur), sur les trois
systèmes. La dictée et l'intégration au système viendront au projet 3.

## Objectifs de la V1

Une seule fenêtre de lecture, qui est une vraie app finie, et non une
maquette :

1. **Faire entrer un texte** de quatre façons :
   - coller (Ctrl+V, ou ⌘V sur Mac) ;
   - glisser-déposer un fichier ;
   - ouvrir un fichier (Ctrl+O, boîte de dialogue du système) ;
   - formats : md, txt, PDF, image (par l'OCR). Le presse-papiers peut aussi
     contenir une image, reconnue comme dans `source::clipboard`.
2. **Le lire à voix haute** avec le mot prononcé mis en évidence, en
   suivant le texte. Si on fait défiler le texte, la vue se détache de la
   lecture ; `f`, ou un bouton, la fait revenir.
3. **Le même clavier que le lecteur Mac**, avec Ctrl à la place de ⌘ hors du
   Mac :
   - Espace ; ← → ; ↑ ↓ (une ligne affichée) ; Début ;
   - n / p ; N / P ; + / - ; / ; t ; e ; ? ; Échap ;
   - Ctrl+C, Ctrl+V, Ctrl+O, Ctrl+F, Ctrl+W, Ctrl+,.
   - ⌘M (taille réduite) est retiré, faute de panneau compact en V1.
4. **La recherche** dans le document : champ, nombre de résultats, N / P.
5. **Le sommaire** : un panneau latéral des titres ; on choisit au clavier
   ou à la souris.
6. **La modification du texte** (`e`, ou un bouton Modifier). Le texte
   devient modifiable :
   - pour un fichier texte, c'est le texte ouvert tel quel ;
   - sinon, du Markdown est reconstruit depuis les blocs, comme le fait
     `EditableText.swift` ;
   - quand on valide, le document est relu depuis cette position.
7. **Une page de réglages d'un seul écran** : voix, vitesse, thème
   (clair / sombre / système), police, langue de l'interface. Chaque
   changement est enregistré aussitôt, comme dans le terminal et l'app Mac.
8. **L'installation automatique de Kokoro** au premier lancement, avec sa
   progression, et la voix de repli du système en attendant.

## Hors V1

Tout cela relève du projet 3 :

- la dictée ;
- les raccourcis globaux (⌃⌥Espace, ⌃⌥S) ;
- la capture de la sélection d'une autre app ;
- le panneau compact et les modes petit / arrière-plan ;
- le lancement à l'ouverture de session ;
- l'icône dans la barre des menus ou la zone de notification.

Sont aussi hors V1 :

- les lecteurs d'écran (VoiceOver, Narrator, Orca). La V1 vise les
  lecteurs dyslexiques : elle garantit le clavier complet, les contrastes et
  la taille de police. Que GPUI n'expose pas son contenu aux lecteurs
  d'écran est une limite connue, acceptée par l'utilisateur. Elle est à
  confirmer sur la version figée ;

- la bibliothèque complète des modèles, avec scores et tailles (la V1 ne
  propose que les voix) ;
- les mises à jour automatiques.

## Anti-objectifs

La V1 serait un échec, même si elle « marche », dans ces cas :

- elle a l'air d'une démo : composants sans états de survol, de focus ou
  de sélection, densité approximative, contrastes faibles ;
- un réglage ou un geste du lecteur se comporte autrement que dans le
  terminal ou l'app Mac, parce que la logique aurait été réécrite dans
  l'interface au lieu de passer par `src/reader` ;
- un test ouvre une fenêtre, prend le focus, joue un son ou ouvre le
  micro ;
- sur Mac, elle remplace l'app Swift livrée avant d'être au niveau.

## Contraintes

- **Rust seulement** côté projet. Les dépendances C ou C++ sont acceptées
  (GPUI, MLX, ONNX Runtime, Tesseract).
- **Toute la logique du lecteur reste dans le cœur.** L'interface parle à
  `reader::handle::ReaderHandle` :
  - elle envoie des `Request` et des `Command` (`src/reader/command.rs`) ;
  - elle lit à chaque image un `Snapshot`, et le `DocView` à chaque
    révision (`src/reader/view.rs`) ;
  - les réglages passent par `SettingsView` et `SettingsPatch`
    (`src/reader/settings.rs`).

  Si quelque chose manque au cœur, on l'ajoute au cœur, avec ses tests,
  et non à l'interface.
- **Moteurs selon le système :**
  - sur Mac, `--features mlx` ;
  - ailleurs, `kokoro-onnx`, avec ONNX Runtime fourni à côté du binaire
    comme dans les paquets actuels (`scripts/package.sh`) ;
  - `ORT_DISABLE_TELEMETRY=1` partout où ORT se charge ;
  - `ocr` partout.
- **Les tests sont headless.** Aucun test ne montre de fenêtre, ne prend
  le focus, ne joue de son, n'ouvre le micro ni ne prend de raccourci
  global. On teste avec la plateforme de test de GPUI (`TestAppContext`,
  sans fenêtre réelle), avec un lecteur dont la sortie audio est muette
  (`audio::Player` sans périphérique) et une voix factice.
- **Sur Mac, la version GPUI ne sert qu'au développement.** L'app Swift
  reste celle qu'on livre. Aucun changement dans `macos/` pour ce projet.
- **Plateformes visées :**
  - Windows 10 et 11 x86_64 ;
  - Linux x86_64 et arm64, sous Wayland et X11 ;
  - macOS Apple Silicon.
- **Langues de l'interface :** français et anglais, avec la macro `tr!` du
  cœur (`src/i18n.rs`).
- **Licences :** GPUI, `gpui-pre` et `gpui-component` sont sous
  Apache-2.0. Le dépôt et `NOTICE` les mentionnent.

## Approches

1. **GPUI pris dans le dépôt de Zed, figé à un commit, avec nos propres
   composants.**
   - Pour : une seule dépendance, la source officielle.
   - Contre : il faut écrire et finir soi-même chaque bouton, champ,
     menu, curseur et liste déroulante, ainsi que la page de réglages.
     C'est le gros du travail pour une app soignée, et les mises à jour
     de l'interface de programmation de Zed se suivent à la main.
2. **`gpui-component` 0.11.0 et `gpui-pre` 0.3.14 depuis crates.io, à
   versions exactes. C'est l'approche retenue.**
   - Pour : les composants existent déjà (champs, menus, curseurs, listes
     déroulantes, onglets, thèmes, listes longues). Ils servent dans une
     app en production (Longbridge), et l'effort va à la zone de lecture.
   - Contre : un seul mainteneur publie à la fois la copie de GPUI et les
     composants, avec une version par semaine.
   - Ce qui limite le risque : versions exactes (`=`), `Cargo.lock` suivi
     par git, mises à jour faites exprès. GPUI reste dessous si on doit un
     jour partir.
3. **Commencer par 1, puis ajouter des composants plus tard.** Écarté :
   les mêmes composants seraient écrits deux fois.

L'utilisateur a choisi l'approche 2, sur recommandation (« go recos »).

## Ce qui est décidé

### Organisation du code

- Une nouvelle crate `app/` (nom du paquet : `dysflow-app`), ajoutée aux
  membres de l'espace de travail. Elle produit le binaire `dysflow-app`. Le
  binaire `dysflow` (terminal) ne change pas.
- Elle dépend de `dysflow`, avec les fonctions de chaque système, et de
  `gpui-component` `=0.11.0` et `gpui-pre` `=0.3.14`.
- Le cœur ne dépend jamais de GPUI.
- La crate ne contient que de l'interface :
  - la fenêtre ;
  - la vue de lecture ;
  - la barre de lecture ;
  - le sommaire ;
  - la recherche ;
  - l'édition ;
  - les réglages ;
  - le premier lancement ;
  - la correspondance entre touches et actions.

### La vue de lecture

- La vue de lecture est écrite par nous. Aucune bibliothèque ne fournit un
  texte dont le mot prononcé est mis en évidence au fil de la lecture.
- Elle met en page les blocs du `DocView` : titres, paragraphes, listes,
  citations, code et tableaux.
- Elle dessine le fond du mot en cours et, si le réglage est actif, la
  règle sous la ligne en cours (`ReadingStyle::ruler`).
- Elle garde la ligne lue à l'écran, sauf quand la vue est détachée.
- Elle sait quelle ligne est affichée au-dessus ou en dessous, pour ↑ ↓,
  comme `LineNavigator.swift`.
- Un clic sur un mot envoie `SeekToWord`.
- **Où passe la limite avec le cœur.** La mise en page reste dans `app/`,
  comme dans le Swift : retour à la ligne, position des mots, ligne
  affichée au-dessus ou en dessous. C'est de la présentation. Le cœur
  garde ce qui est lu, où l'on en est, et vers où l'on saute. Une ligne
  affichée se traduit en `SeekTo` ou en `SeekToWord`. (Mon choix.)
- **Écritures.** La V1 vise les écritures latines, dans les langues des
  voix proposées. Le texte CJK ou de droite à gauche s'affiche avec la
  mise en forme de GPUI, sans garantie sur ↑ ↓ ni sur la mise en
  évidence. (Mon choix.)

### Clavier

- La table du lecteur est celle de `KeyMap.swift`, transposée en Ctrl hors
  du Mac.
- Elle vit dans une fonction pure (touche → action) de la crate, testée
  table par table.
- Le texte de l'aide (`?`) reprend `app::keys::help()`, complété des
  raccourcis propres à la fenêtre.

### Réglages

- Une page d'un seul écran, sans onglets, affichée dans la fenêtre à la
  place du texte ou comme feuille par-dessus.
- On y trouve, dans cet ordre :
  - la voix (liste déroulante des voix de la langue, bouton « Écouter » pour
    l'aperçu, installation d'une voix absente) ;
  - la vitesse (curseur) ;
  - le thème (clair / sombre / système) ;
  - la police : la famille, parmi celles du cœur (`config::READING_FONTS` :
    Literata, Atkinson Hyperlegible, OpenDyslexic), et la taille, entre
    `READING_SIZE_MIN` et `READING_SIZE_MAX`. Les trois polices sont
    embarquées dans le binaire. Leurs licences OFL sont déjà dans
    `macos/Resources/Fonts` et `NOTICE` ;
  - la langue de l'interface.

### Premier lancement

- Si Kokoro n'est pas installé, il se télécharge tout seul
  (`ensure_default_voice`) et la progression s'affiche dans la barre de
  lecture.
- On peut lire tout de suite avec la voix de repli du cœur
  (`tts::system`) : `say` sur Mac, `espeak-ng` ailleurs (le paquet Windows
  livre `dysflow-espeak.exe`). La bascule vers Kokoro se fait dès qu'il est
  prêt, comme dans l'app Mac. Avec cette voix, la mise en évidence du mot
  suit le minutage qu'elle fournit, comme dans le terminal.

### Démarrage

- `dysflow-app [fichier]` : un chemin passé en argument est ouvert.
- Il n'y a ni association de fichiers ni instance unique en V1. Ces deux
  points relèvent du projet 3. (Mon choix.)
- Sans GPU utilisable (pas de Vulkan, VM sans accélération), l'app
  affiche un message bilingue qui le dit, sur la sortie d'erreur et dans
  une boîte de dialogue du système si elle peut s'ouvrir. Elle ne bascule
  pas d'elle-même vers un rendu logiciel. (Mon choix.)

### Livraison

- On ajoute `dysflow-app` aux paquets Windows (zip) et Linux (tar), à côté
  du terminal. Le Mac n'est pas concerné.
- La CI construit et teste la crate sur les quatre systèmes de la matrice.

## Ce qui reste au constructeur

- Le découpage en fichiers et en tâches. On commence par un essai court :
  une fenêtre GPUI vide, que l'utilisateur lance lui-même sur les trois
  systèmes (jamais un test ni un agent). Il sert à lever les risques
  ci-dessous avant le reste.
- L'identité visuelle (palette, typographie). Elle se choisit après avoir
  étudié une vraie app de lecture très finie. Elle reste sobre : un seul
  point fort par écran, le texte et son mot en cours.
- La place exacte des boutons de la barre de lecture. On part de la barre
  du mode moyen de l'app Mac (`2026-10-08-modes-affichage-design.md`).
- La façon de fournir le temps et la boucle d'images : GPUI redessine à
  chaque instantané qui change, au plus 60 fois par seconde.

## Risques à lever en premier

- **Windows en MinGW.** Le paquet Windows actuel se construit avec
  MinGW-w64 (`scripts/package.sh`). GPUI se construit d'ordinaire sous
  MSVC. Il faut vérifier que `gpui-pre` compile et s'affiche en
  `x86_64-pc-windows-gnu`. Sinon, l'app Windows passera en MSVC, avec les
  conséquences sur le paquet et sur la vérification des imports
  (`scripts/windows-imports.py`).
- **Le plancher de glibc sous Linux.** La CI vérifie les versions de
  symboles (`scripts/elf-version-ref.py`). Les dépendances de GPUI
  (Vulkan, xkbcommon, Wayland, fontconfig) ne doivent pas le relever sans
  qu'on le décide.
- **La plateforme de test de GPUI dans `gpui-pre`.** Il faut vérifier
  qu'elle est publiée, avec sa fonction de test, et qu'elle tourne sans
  écran sur les runners Linux de la CI.
- **La taille et le temps de compilation** ajoutés au paquet et à la CI.
  On les mesure et on les signale.
- **Si un risque ne se lève pas**, l'essai s'arrête et rend ses mesures à
  l'utilisateur, qui décide. On ne relève pas le plancher de glibc, on
  n'assouplit pas les tests headless et on ne change pas d'approche sans
  son accord. La CI ne teste que par la plateforme de test, sans GPU. Le
  rendu réel se vérifie à la main, sur les machines de l'utilisateur.
  (Mon choix.)

## Réception

- C'est l'utilisateur qui juge, sur ses trois machines (Mac, PC Windows,
  PC Linux).
- Chaque composant est vérifié sur une liste : survol, focus visible au
  clavier, sélection, état désactivé, menus, contrastes du texte en clair
  et en sombre (au moins 4,5:1 pour le texte courant), et navigation
  complète au clavier sans souris. (Mon choix.)
