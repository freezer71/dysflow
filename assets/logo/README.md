# Le logo

Un sillon de disque, entaillé par la ligne de lecture. Le nom dit les deux à
la fois : la rainure où l'aiguille se pose, et la ligne qu'on suit des yeux.
L'orange est la tête de lecture — le mot en train d'être dit.

Le dessin est calculé, pas dessiné : `scripts/logo_gen.py` écrit les SVG,
`scripts/logo.sh` en tire l'icône macOS et le PNG de Linux. Les deux sont à
relancer quand le tracé change, dans cet ordre.

## Ce qu'il y a ici

| Fichier | Quand s'en servir |
|---|---|
| `sillon-mark.svg` | la marque seule, au-dessus de 32 pixels |
| `sillon-mark-small.svg` | la marque à deux tours, de 16 à 32 pixels |
| `sillon-lockup.svg` | la marque et le mot, pour un en-tête |
| `sillon-app.svg` | l'icône d'application, plaque comprise |
| `sillon-app-small.svg` | la même, pour les tuiles de 16 et 32 points |
| `sillon.icns` | l'icône macOS assemblée, posée par `package.sh` |
| `sillon.png` | 512 pixels, pour les thèmes d'icônes de Linux |

Chaque SVG a son jumeau `-dark`, à servir sur fond sombre : l'encre y devient
claire, l'orange ne bouge pas.

**Sous 32 pixels, prendre la variante `small`.** Trois tours de sillon s'y
empâtent ; deux tours sur un trait plus gras restent lisibles. C'est le même
logo redessiné pour tenir, pas un autre logo.

## Couleurs

Elles viennent du thème de la fenêtre (`src/gui/theme.rs`) — un seul endroit
décide de l'orange.

| | |
|---|---|
| `#F26B27` | l'accent : la ligne et la tête de lecture |
| `#161616` | l'encre, sur fond clair |
| `#EEEEEE` | l'encre, sur fond sombre |
| `#FBF9F5` | le papier, sous l'icône d'application |

## Le mot

« sillon » est composé en **Atkinson Hyperlegible Bold**, la police que le
lecteur emploie déjà, et converti en contours : le lockup s'affiche partout
sans rien exiger de la machine. La police est sous licence OFL — voir
`assets/fonts/OFL-AtkinsonHyperlegible.txt`.
