# Des réglages complets dans l'app macOS

> **Document historique** — état au 6 oct. 2026 ; audio.cpp, Supertonic et MagpieTTS ont été retirés le 14 oct. 2026.

Conception validée le 2026-10-06. Elle prolonge
`2026-10-05-interface-macos-design.md` : l'app expose désormais **tout** ce que
le moteur sait régler, au lieu de 8 réglages sur une quarantaine.

## Pourquoi

L'utilisateur veut une application complète, sans retour au terminal : choisir
et écouter les voix, régler la lecture, gérer les modèles, toucher au moteur.
Il demande en particulier un accès rapide à la configuration de la voix
depuis le panneau, pour essayer les voix l'une après l'autre.

## Ce qui existe

Le moteur porte déjà tous ces réglages (`src/config.rs`) et l'écran du
terminal sait les modifier (`src/app/config.rs`). L'app n'en montre que huit :
police, taille, interlignage, règle, vitesse, raccourcis, durée de chargement
de la voix, ouverture à la connexion. Le reste n'est atteignable qu'en éditant
`config.toml` ou au terminal.

## Organisation : six onglets

| Onglet | Contenu |
|---|---|
| **Voix** | Modèle actif (parmi les installés) ; voix du modèle, avec **Écouter** ; langue (celles du modèle seulement) ; vitesse ; volume ; voix clonée (fichier WAV) quand le modèle l'exige |
| **Lecture** | Police, taille, interlignage, règle de lecture ; blocs de code (annoncer / sauter / lire) ; lecture automatique à l'ouverture ; pas de l'avance rapide |
| **Modèles** | Catalogue : nom, description, langues, taille, précision, rythme mesuré ; installer, annuler un téléchargement, supprimer (confirmé) ; espace disque libre |
| **Raccourcis** | Lire la sélection, Afficher sillon |
| **Avancé** | Moteur (automatique / audio.cpp / voix du système) ; calcul (Metal / CPU) ; fils de calcul ; sessions ; phrases préparées d'avance ; échantillonnage (température, top-k, guidage, pas d'inférence, graine) ; options libres du modèle (`[extra]`) |
| **Général** | Ouverture à la connexion ; durée de chargement de la voix ; données : vider le cache des documents, gérer les signets, ouvrir le dossier de configuration |

**L'écrou ⚙ :** un bouton en bout de barre de lecture du panneau, 13 pt,
atteignable au clavier, libellé VoiceOver « Réglages de la voix ». Il ouvre la
fenêtre de réglages **sur l'onglet Voix**. `⌘,` ouvre les réglages sur le
dernier onglet utilisé.

## Réglages sans effet

Certains réglages ne sont lus par aucun modèle installé : hauteur, énergie,
émotion, consigne de style, top-p, pénalité de répétition (relevé de
`models::honoured_options`). Ils sont **cachés** par défaut. Une case
« Afficher aussi les réglages ignorés par ce modèle » (onglets Voix et Avancé)
les montre, chacun marqué « sans effet sur ce modèle ». Le même filtre suit le
modèle actif : passer de Supertonic à Magpie fait apparaître température,
top-k et guidage, et disparaître la vitesse.

## Comportement

- **Tout s'applique tout de suite.** Pas de bouton Enregistrer : `config.toml`
  est écrit après une courte pause, comme aujourd'hui (`Committer`).
- **Chaque modification déclenche l'effet de sa nature**, repris du terminal
  (`Effect` dans `src/app/config.rs`) :

| Réglages | Effet |
|---|---|
| Voix, langue, vitesse, échantillonnage, options libres | Resynthèse à partir de la phrase en cours |
| Blocs de code | Remise en forme du document, à la même position |
| Moteur, calcul, fils, sessions, phrases d'avance, modèle actif | Rechargement de la voix |
| Volume | Immédiat, sans resynthèse |
| Police, taille, interlignage, règle | Affichage seulement |

- **Rechargement :** une ligne « Voix en chargement… » dans le panneau et dans
  l'onglet ; la lecture reprend seule à la même phrase.
- **Bornes : celles du moteur** (`Config::normalised`). Les écarts relevés sont
  corrigés : interlignage 0 à 3 partout, pas d'avance rapide 1 à 50 partout.
  Les langues proposées sont celles du modèle (31 pour Supertonic, 11 pour
  Magpie), plus seulement les sept du terminal.
- **Écouter :** fait dire une phrase d'exemple fixe avec la voix et les
  réglages en cours, même sans texte ouvert (la voix se charge au besoin).
  `Espace` sur la liste des voix le déclenche. Un nouvel essai coupe le
  précédent. Refus expliqué si la sortie audio manque ou si le modèle exige
  une voix clonée sans fichier.
- **Supprimer un modèle :** confirmation dans une alerte. Le modèle actif ne
  se supprime pas tant qu'un autre n'est pas choisi.
- **Valeurs non listées** (écrites à la main dans `config.toml`) : affichées
  telles quelles, jamais réécrites tant que l'utilisateur n'y touche pas.
- **Accessibilité :** tous les contrôles sont atteignables au clavier, ont un
  libellé et une valeur VoiceOver ; texte d'interface en 13 pt ; contrastes
  conformes à l'audit.

## Architecture

### Rust

- **`SettingsView` complet** (`src/reader/settings.rs`) : tous les champs de
  `[voice]`, `[engine]`, `[sampling]`, `[reading]`, `[app]` et `[extra]`.
- **Modifications par patch** : `SettingsPatch`, un champ optionnel par
  réglage. Seuls les champs présents sont appliqués, puis l'effet le plus
  fort de ceux touchés est déclenché. Rien n'écrase une valeur changée
  ailleurs entre la lecture et l'écriture.
- **Capacités du modèle actif** : `ModelCapabilities { family, voices,
  languages, honoured, requires_reference }`, tirées de
  `models::voices_for_family`, du catalogue et de `honoured_options`.
- **Catalogue enrichi** : `VoiceModel` gagne description, famille, langues,
  précision, rythme.
- **Actions** : `preview_voice()` sans texte ouvert (`Previewer` sur la voix
  du lecteur), `stop_preview()`, `remove_model(id)`, `cancel_download()`,
  `free_space()`, `clear_document_cache()`, `config_dir()`.
- **FFI** : les mêmes types et actions dans `ffi/src/lib.rs`, bindings
  régénérés par `scripts/build-ffi.sh`.

### Swift

- La fenêtre de réglages (`macos/Sources/Settings/`) passe à six onglets, un
  fichier par onglet. Le modèle de vue garde les réglages, les capacités et
  l'état « afficher les ignorés ».
- `SettingsOpener` sait ouvrir un onglet précis ; l'écrou du panneau
  l'appelle avec l'onglet Voix.
- Les appels au moteur qui attendent son fil restent hors du fil principal.

## Erreurs

| Situation | Réaction |
|---|---|
| Patch refusé (valeur hors bornes) | Valeur ramenée dans les bornes, affichée telle qu'appliquée |
| Écriture de `config.toml` impossible | Message dans l'onglet, réglage gardé en mémoire |
| Rechargement de la voix échoué | Repli sur la voix du système, message avec lien vers l'onglet Modèles |
| Aperçu impossible | Raison affichée à côté du bouton Écouter |
| Suppression d'un modèle échouée | Message, catalogue relu |

## Tests

- **Rust** : aller-retour de chaque réglage par patch ; effet déclenché par
  catégorie ; capacités par famille ; aperçu sans texte (moteur muet) ;
  suppression refusée pour le modèle actif ; bornes corrigées.
- **Swift unitaires** : construction du patch (seul le champ touché part) ;
  filtre des réglages ignorés selon les capacités ; valeurs non listées non
  réécrites ; confirmation de suppression ; ouverture sur un onglet donné.
- **Tests d'interface** (avec le feu vert de l'utilisateur) : l'écrou ouvre
  l'onglet Voix ; chaque onglet se parcourt au clavier ; audit
  d'accessibilité sur chaque onglet.

## Hors périmètre

- Les chemins de données (dossiers de configuration, de modèles, de cache) ne
  se changent pas ; on peut seulement ouvrir le dossier de configuration.
- Pas de gestion des positions de lecture retenues (aucune interface nulle
  part aujourd'hui).
- Pas d'import / export de configuration.
