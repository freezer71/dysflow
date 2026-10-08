# Quatre modes d'affichage dans l'app macOS

Conception validée le 2026-10-08. Elle prolonge
`2026-10-05-interface-macos-design.md` et
`2026-10-06-reglages-complets-design.md`.

## Pourquoi

L'utilisateur écoute souvent sans suivre le texte des yeux. Il veut pouvoir
réduire Sillon à une barre de contrôle, ou le faire disparaître tout en
gardant la lecture, comme une app de musique. Le panneau et le lecteur agrandi
actuels restent pour les moments où il suit le texte.

## Les modes

| Mode | Touche | Contenu |
|---|---|---|
| Petit | ⌘1 | Barre flottante d'environ 360 × 44 pt : lecture/pause, timeline, temps restant, vitesse, ⚙ |
| Moyen | ⌘2 | Le panneau actuel avec le texte suivi |
| Grand | ⌘3 | Le lecteur agrandi actuel (⏎ y mène toujours) |
| Arrière-plan | Échap | Aucune fenêtre ; la lecture continue |

- **Une seule fenêtre** (le `ReadingPanel` actuel) : chaque mode change son
  contenu et sa taille. Le mode petit garde sa propre position, déplaçable,
  retenue d'une fois sur l'autre ; il flotte au-dessus des autres fenêtres
  comme le panneau.
- **⌃⌥S** fait revenir le dernier mode visible. Le mode courant et le dernier
  mode visible sont retenus dans les préférences, y compris après une
  relance de l'app.
- **Touches dans le mode petit** : celles de la lecture restent actives
  (Espace, ← →, ⇧← ⇧→, ⌥← ⌥→, ↑ ↓ — la ligne suit le texte même invisible —,
  ⇧↑ ⇧↓, ⌥↑ ⌥↓, `v`, ⌘,).
- **⌃⌥Espace** (lire la sélection) garde le mode en cours ; depuis
  l'arrière-plan, il ouvre le mode moyen pour montrer ce qui est lu.
- **Échap** dans le grand mode garde son sens actuel en deux temps : il
  réduit d'abord au mode moyen, puis passe en arrière-plan.
- L'aide (`?`) et le menu de la barre des menus listent ⌘1, ⌘2, ⌘3.

### Mode petit

`▶ ━━━━━━━━━━━──────── -2:00  1,1× ⚙`

- Bouton lecture/pause, timeline (même source que la barre du panneau),
  temps restant estimé, vitesse, écrou des réglages (onglet Voix).
- 13 pt, libellés VoiceOver comme la barre du panneau ; la timeline annonce
  sa valeur (« 0:56 sur 2:56 »).
- Sans document ouvert : la barre affiche « Rien à lire » et ⌃⌥Espace.
- Coût au repos nul : pas de lien d'affichage, mise à jour au rythme du
  modèle actuel (seulement pendant la lecture et la courte grâce).

## L'arrière-plan

- **Touches média et Centre de contrôle** (`MPNowPlayingInfoCenter`,
  `MPRemoteCommandCenter`) :
  - « À l'écoute » montre le nom du document, la durée estimée, la position,
    l'état lecture/pause, la vitesse ;
  - ⏯ (clavier, AirPods, Centre de contrôle) : lecture/pause ;
  - ⏭ / ⏮ : phrase suivante / précédente ;
  - glisser la timeline : saut à la phrase la plus proche de la position
    visée.
- **Menu de la barre des menus** (le `MenuBarExtra` existant) : lecture/pause,
  phrase précédente / suivante, temps restant, sous-menu « Afficher » (petit,
  moyen, grand), en plus de ce qu'il contient déjà.
- **Raccourcis globaux**, en plus de ⌃⌥Espace et ⌃⌥S, modifiables dans
  l'onglet Raccourcis :
  - ⌃⌥P : lecture/pause ;
  - ⌃⌥→ / ⌃⌥← : phrase suivante / précédente.
- **Au repos** : quand la voix est mise au repos (durée « Garder la voix
  chargée »), Sillon retire ses informations de « À l'écoute ». Aucune
  minuterie ni boucle tant que rien ne joue ; processeur ~0 %.

## Architecture

- **`DisplayMode`** (`small`, `medium`, `large`, `background`) : un type pur
  avec les transitions (⌘1/2/3, Échap, ⌃⌥S, ⌃⌥Espace) et le dernier mode
  visible. Testé seul.
- **`DisplayModeStore`** : retient le mode courant, le dernier mode visible
  et la position du mode petit dans les préférences (suite isolée en test).
- **`PanelController`** applique le mode : taille et contenu de la fenêtre,
  affichage ou masquage. Le grand mode reste l'actuel « agrandi ».
- **`MiniPlayerView`** : la barre du mode petit, réutilisant les éléments de
  `PlayerBar`.
- **`NowPlaying`** : publie les informations à « À l'écoute » et branche les
  commandes média sur `ReaderModel`. Mis à jour aux changements d'état
  (lecture, pause, phrase, vitesse, document), jamais à chaque image.
- **`KeyMap`** : ⌘1, ⌘2, ⌘3 → actions de mode.
- **Raccourcis** : trois nouveaux noms `KeyboardShortcuts` et leurs lignes
  dans l'onglet Raccourcis.
- Pas de changement du moteur Rust : les commandes existent
  (`togglePlay`, `sentences(delta:)`, `seekTo(sentence:)`).

## Erreurs

| Situation | Réaction |
|---|---|
| Raccourci global déjà pris par une autre app | Message dans l'onglet Raccourcis, comme aujourd'hui |
| Commande média sans document | Ignorée ; « À l'écoute » n'affiche rien |
| Saut de timeline au-delà de la fin | Dernière phrase |
| Position du mode petit hors des écrans (écran débranché) | Replacée sur l'écran principal |

## Tests

- **Unitaires** : transitions de `DisplayMode` ; mode et position retenus ;
  ⌘1/2/3 dans `KeyMap` ; informations publiées par `NowPlaying` (titre,
  durée, position, état) avec un faux centre ; commandes média traduites en
  commandes du lecteur ; position du mode petit ramenée sur un écran.
- **Tests d'interface** (écrits et compilés, exécutés avec le feu vert de
  l'utilisateur) : ⌘1 montre la barre, ⌘2 le panneau, ⌘3 le lecteur
  agrandi, Échap masque ; audit d'accessibilité du mode petit.

## Hors périmètre

- Pochette ou image dans « À l'écoute ».
- Mode petit redimensionnable.
- Contrôle depuis l'écran verrouillé au-delà de ce que macOS fait seul.
