# Interface fine : trois colonnes, une carte, une note

**Date** : 2 septembre 2026. **Remplace** la mise en page et le style des
contrôles décrits dans `2026-09-02-interface-graphique-design.md` ; le reste de
cette spécification — contrôleur partagé, zone de texte, polices, réglages —
reste valable.

## Ce que l'on corrige

L'interface de la première version grandissait avec le texte lu : boutons et
libellés énormes, gênants à l'usage. Un essai en pixel art a été écarté. La
référence retenue est celle des studios web récents : texte d'interface de
treize points, traits d'un point, coins à peine arrondis, une barre latérale,
une carte de lecture flottante, un panneau à droite.

## Mise en page

```
┌ barre latérale 240 ┬ en-tête 44 ─────────────────────┬ note 300 ┐
│ sillon        [⊟]  │ Titre ▸ Section      [🔍] [🔖] [📄]│ Note  ⌖ ✕ │
│ [Ouvrir] [Coller]  │                                  │           │
│ Sommaire           │   le texte lu, grand, Atkinson,  │ texte     │
│  ▸ section         │   le mot prononcé surligné       │ libre     │
│    sous-section    │                                  │           │
│ Signets            │  ( pastilles : vue figée, état ) │           │
│  « … »        12:04│ ┌──────────────────────────────┐ │           │
│                    │ │ ▶  phrase en cours…  0:42/~7 │ │           │
│ Réglages · Aide    │ │ ━━━━━━━●────┬─────  ◀ ▶ 1,0× 🔊 ⋯│ │           │
│ moteur             │ └──────────────────────────────┘ │ enregistrée│
└────────────────────┴──────────────────────────────────┴───────────┘
```

- **Barre latérale** (`gui/sidebar.rs`) : repliable (`t`, `Tab`, `B`, ou
  l'icône) ; quand les réglages sont ouverts, ses rubriques remplacent le
  sommaire — une seule colonne de navigation. Les signets s'y suppriment d'un
  clic sur la corbeille.
- **En-tête** (`gui/header.rs`) : titre et section, puis à droite la loupe
  (champ de recherche le temps d'une requête), le signet, la note.
- **Carte** (`gui/player.rs`) : posée sous le texte, jamais par-dessus. La
  ligne de temps compte en phrases (`gui/timeline.rs`, pur et testé) ; on
  glisse, la phrase cible est demandée au relâchement ; un repère par section.
  Vitesse et volume s'ouvrent en petits menus. `⋯` porte signet, repère,
  recherche, reprise, début, retour à la lecture, réglages, aide, et l'état du
  moteur.
- **Pastilles** : une ligne de trente points au-dessus de la carte, toujours
  réservée pour que le texte ne saute pas ; vue figée avec « Suivre la
  lecture », message du contrôleur, recherche en cours, voix système.
- **Note** (`gui/notes.rs`, `notes.rs`) : une page Markdown par document dans
  `<données>/notes/<mot>-<empreinte>.md`. Un fichier est nommé par son chemin ;
  ce qui n'en a pas — presse-papiers — par son titre. Écriture une seconde
  après la frappe, au changement de document, à la fermeture ; une note vide
  efface son fichier. « Repère » ajoute `— Section · 0:42` sur sa ligne.
- **Sans document** : pas de carte ; deux boutons au milieu.

## Style

- `theme::UI_SIZE = 13` : le texte d'interface ne suit pas la taille du texte
  lu. Bordures d'un point, coins de six (cartes : dix), ombre douce sous les
  cartes et menus seulement.
- Icônes tracées au trait par le peintre (`gui/icons.rs`), seize points, dans
  une case de seize unités : nettes à toute échelle, sans police de symboles.
- Boutons (`gui/widgets.rs`) : vingt-huit points ; icône seule sans bordure au
  repos ; principal plein couleur d'accent ; choisi sur voile d'accent. Lignes
  de navigation avec barre d'accent à gauche pour la ligne où l'on est, texte
  accent pour la section que la voix lit.
- Thème sombre par défaut, clair au choix ; les deux partagent l'accent.

## Touches propres à la fenêtre

`e` note, `t`/`Tab`/`B` barre latérale, `/` recherche, `⌘O`/`Ctrl+O` ouvrir,
`⌘V`/`Ctrl+V` coller, `Échap` ferme ce qui est ouvert, `q` quitte. Les autres
touches sont celles du contrôleur.

## Vérification

Sans écran : `cargo test --features snapshots gui::snapshot -- --ignored` rend
neuf vues dans `target/snapshots/` — lecture, menu `⋯`, réglages, recherche et
vue figée, note, barre repliée, thème clair, vide, aide. Les modules
`timeline` et `notes` ont leurs tests unitaires.
