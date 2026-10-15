# Simplification de l'expérience — design

Date : 15 octobre 2026. Branche : `feat/simplification-ux`.

## Intention

sillon sert à une chose : **écouter un texte en le suivant des yeux**. Tout ce
qui ne sert pas directement ce geste part. Ce que l'utilisateur garde doit être
visible ; le technique devient automatique.

Usages confirmés : sélection + ⌃⌥Espace, copier puis lire, ouvrir un
fichier/PDF, terminal. Fonctions gardées pendant la lecture : recherche,
sommaire, modification du texte. Retirées : Notes, signets.

Critère de réussite : un nouvel utilisateur lit un texte sans ouvrir les
réglages ni l'aide ; chaque action n'a qu'un endroit évident.

## 1. App macOS

### 1.1 Lancer une lecture

- **⌃⌥Espace** : lit la sélection ; à défaut, le presse-papiers (chaîne de
  capture actuelle, inchangée).
- **⌃⌥S** (modifiable) : affiche le lecteur dans sa dernière taille, ou le
  masque s'il est affiché. Ce sont les deux seuls raccourcis globaux.
- **Ouvrir l'app** à la main (Dock, Finder, Spotlight, réouverture) : le
  lecteur s'ouvre. Vide, il affiche « Collez un texte (⌘V) ou déposez un
  fichier ». Remplace la fenêtre Notes.
- **⌘V dans le lecteur** lit le presse-papiers (texte ou image).
- **Glisser-déposer** un fichier (markdown, texte, PDF, image) sur le lecteur,
  ou **⌘O** (panneau d'ouverture).
- Lancement à la connexion : rien ne s'affiche (inchangé).

### 1.2 Menu de barre (5 lignes)

1. Lire la sélection — ⌃⌥Espace
2. Ouvrir un fichier… — ⌘O
3. Afficher le lecteur
4. Réglages… — ⌘,
5. Quitter sillon — ⌘Q

Plus l'éventuelle ligne unique « Autoriser la lecture de la sélection… » tant
que l'accessibilité n'est pas accordée. Retirés : Notes, Taille, transport,
temps restant, sortie audio, état de la voix, « Mettre la voix au repos ».

### 1.3 Le panneau : deux états

`DisplayMode` passe de `small/medium/large/background` à
`mini/reader/hidden`.

**Lecteur** (redimensionnable, taille et position retenues)
- En-tête : titre courant, et trois boutons visibles **Sommaire**,
  **Rechercher**, **Modifier**. Le sommaire s'ouvre en barre latérale ; la
  recherche dans l'en-tête.
- Centre : le texte.
- Barre du bas : ⏮ ▶ ⏭ (phrase), barre de progression **cliquable** (saut à
  la phrase sous le pointeur), temps écoulé / total, vitesse **cliquable**
  (menu 0,75 · 1 · 1,25 · 1,5 · 2×), bouton « réduire » (→ mini).
- Ligne d'état : seulement en cas de problème de voix (indisponible, en
  préparation, voix du système en repli). Sinon rien.

**Mini** (barre flottante, position retenue)
- ⏮ ▶ ⏭ par **phrase** (corrige l'incohérence ligne/mot actuelle),
  progression, bouton « agrandir ».

**Réduire / agrandir** : le bouton de la barre, ou **⌘M**, fait passer du
lecteur au mini et inversement, avec une transition animée (le cadre du
panneau se resserre vers la position du mini). La lecture continue.

**Fermer** (⌘W, Échap, ou le bouton fermer) : le panneau disparaît **et la
lecture s'arrête** (pause ; le document et la position restent, la
réouverture reprend là). Fermer ne transforme jamais le lecteur en mini.
Échap dans le lecteur ferme d'abord la recherche ou le sommaire.

**Taille à l'ouverture** : toujours la dernière utilisée (mini ou lecteur),
pour ⌃⌥Espace, ⌃⌥S, ⌘V, l'ouverture d'un fichier et le lancement manuel.
Pas de réglage dédié. Lecteur vide au lancement manuel : toujours en grand.

### 1.4 Sélectionner et copier, distinct de l'édition

**En lecture, le texte se sélectionne et ne se modifie pas.**
- Glisser sélectionne ; double clic : un mot ; triple clic : le paragraphe.
- ⌘C copie la sélection ; sans sélection, la phrase en cours.
- Clic droit : « Copier », « Lire à partir d'ici ».
- La sélection n'interrompt pas la voix ; le suivi de lecture ne l'efface pas.

**L'édition ne s'ouvre que volontairement** : bouton **Modifier** ou touche
**e**. Le double clic ne mène plus à l'édition. En édition : fond teinté,
barre « Modification du texte » avec **Annuler** et **Terminé**, lecture en
pause. Valider : Terminé, ⌘↩ ou Échap (comportement actuel : Échap ne fait
jamais perdre une saisie). Annuler : seulement par le bouton.

### 1.5 Clavier du lecteur

| Touche | Action |
|---|---|
| Espace | lecture / pause |
| ← → | phrase |
| ↑ ↓ | ligne affichée |
| n / p | titre suivant / précédent |
| / ou ⌘F | rechercher (N / P : résultat suivant / précédent) |
| t | sommaire |
| e | modifier |
| + / − | vitesse ±0,1 |
| ⌘C · ⌘V · ⌘O | copier · lire le presse-papiers · ouvrir |
| ⌘M | réduire en mini / agrandir |
| ⌘W | fermer (arrête la lecture) |
| Échap | fermer recherche/sommaire, sinon fermer (comme ⌘W) |
| ? | aide (liste **toutes** ces touches) |
| Début | revenir au début du document |

Relire sans couper la voix reste possible : faire défiler le texte suspend
le suivi (`FollowPolicy`, inchangé).

Retirés : signets (b, B), volume ([ ] m ⌥↑↓), préréglages 1–4, mot (⌥← →),
saut de N phrases (⇧← →), bloc (⇧↑↓), r, g, Fin, v, ⌘1/2/3, alias
(h, =, _, chiffres AZERTY).

### 1.6 Réglages : une page

Fenêtre unique, sans barre latérale ni onglets :

- **Voix** : voix française (menu + ▶ écouter), voix anglaise (idem),
  vitesse, sortie audio.
- **Lecture** : police, taille, interlignage, règle de lecture.
- **Raccourcis** : lire la sélection, afficher / masquer le lecteur.
- **Ouvrir sillon à la connexion**.
- En bas, discret : état de la voix si elle n'est pas prête, et le
  téléchargement en cours le cas échéant.

Devient automatique (plus d'interface) : modèles, moteur, fils de calcul,
sessions, phrases préparées, garder la voix chargée (10 min), blocs de code
(annoncés), lire dès l'ouverture (toujours), pas d'avance rapide, volume,
langue du document (auto), cache, dossier de configuration, signets.

Police et taille passent des UserDefaults à `config.toml` (un seul
stockage), avec reprise des valeurs existantes.

### 1.7 Voix : installation automatique

Plus d'onglet Modèles. Au premier lancement sans Kokoro installé,
le téléchargement démarre seul ; la voix du système lit en attendant ; la
progression s'affiche dans la ligne d'état du lecteur et en bas des
réglages. En cas d'échec : un bouton « Réessayer » à ces deux endroits.

### 1.8 Retraits

- **Notes** : `Sources/Notes/*`, `Tests/NotesStoreTests.swift`, le bouton du
  menu, `services.notes`, les branches de `AppDelegate` (lancement manuel,
  réouverture, fin, `--show-notes`). `LaunchKind` est déplacé (il sert au
  choix « ouvrir le lecteur au lancement manuel »). Les fichiers
  `~/Library/Application Support/sillon/Notes/` ne sont **pas** supprimés.
- **Onglets** Voix, Lecture, Modèles, Raccourcis, Avancé, Général → une vue.
- **Raccourcis globaux** autres que « Lire la sélection » et « Afficher /
  masquer le lecteur » (lecture/pause et phrase ± globaux) ; les touches média
  et AirPods restent.
- L'app reste sans icône de Dock (`LSUIElement`), sans exception : le
  passage en `.regular` n'existait que pour Notes.

## 2. Terminal

- **Touches** : Espace, ← →, ↑ ↓, n/p, /, N/P, + −, t (sommaire), s
  (réglages), ? (aide), q (quitter), Échap (fermer panneau), Début (revenir au début), Pg↑ Pg↓ et f (relire
  sans déplacer la lecture, puis reprendre le suivi). Mêmes retraits
  que l'app (signets, volume, préréglages, alias). L'aide liste toutes les
  touches.
- **Réglages** : une page (voix FR, voix EN, vitesse, largeur de colonne,
  interlignage, règle de lecture). **Enregistrement automatique** à chaque
  changement ; `w` disparaît. `q` et Échap ferment l'écran sans quitter.
- **Sans argument** : affiche une aide courte (les quatre manières de lire)
  et quitte, au lieu d'un écran de configuration vide. `--config` ouvre
  toujours les réglages.
- **Options** visibles dans `--help` : `FILE`, `--clip`, `--speed`,
  `--voice`, `--to-markdown`, `--no-cache`, `--config`. Les autres restent
  acceptées mais `hide = true`.
- Installation du modèle : comme dans l'app, automatique au premier usage ;
  plus de rubrique Modèles.

## 3. Moteur et FFI

- **Signets supprimés** : `src/bookmark.rs`, `Command::ToggleBookmark`,
  `bookmarks()`, `all_bookmarks`, `remove_bookmark`, `Snapshot.bookmarked`,
  `BookmarkView`, `StoredBookmarkView`, `Panel::Bookmarks`. `bookmarks.json`
  reste sur le disque, simplement plus lu.
- **FFI** : retrait des signets ; `ReaderCommand::Words` et `Blocks` retirés
  s'ils n'ont plus d'appelant ; ajout de `seek_to_fraction` si la barre de
  progression cliquable l'exige (sinon `SeekToWord` suffit).
- **Réglages** : `SettingsPatch` gagne `reading_font` et `reading_size`. Les
  clés retirées de l'interface gardent leur valeur dans `config.toml` (rien
  n'est cassé pour qui les a modifiées).
- Installation automatique : réutilise `install_model` et l'offre Kokoro
  existantes, déclenchées sans demande.

## 4. Tests

- Rust : tests existants adaptés (signets retirés), tests ajoutés pour
  l'enregistrement automatique des réglages et le lancement sans argument.
- macOS : tests unitaires adaptés ; tests d'interface pour : lecteur vide au
  lancement manuel, ⌘V lit, sélection + ⌘C copie sans entrer en édition,
  bouton Modifier entre en édition, double clic n'y entre pas, ⌘M bascule
  mini ↔ lecteur, ⌘W ferme et met en pause (jamais de mini), réouverture
  dans la dernière taille, ⌃⌥S affiche/masque, réglages sur une page.
- Vérification manuelle : les quatre chemins de lecture de bout en bout.

## Ordre de livraison

1. Moteur et FFI (signets, police et taille dans `config.toml`,
   installation automatique) : l'app et le terminal en dépendent.
2. App macOS (retrait Notes, panneau, sélection, menu, réglages).
3. Terminal.

Chaque lot construit, passe ses tests et se fusionne séparément.
