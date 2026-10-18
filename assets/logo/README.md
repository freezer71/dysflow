# Le logo

Un « s » tracé d'un seul trait, comme le sillage que la voix laisse derrière
elle : trois rangs et deux demi-tours, comme l'œil qui passe d'une ligne à la
suivante. Le point, au bout du trait, est la tête de lecture — le mot en train
d'être dit, que l'œil suit dans son sillage.

Monochrome, à la manière des icônes de Notion : une seule encre, des droites
et des demi-cercles. Il a remplacé le disque à l'accent orange le 14 octobre
2026.

Le dessin est calculé, pas dessiné : `scripts/logo_gen.py` écrit les SVG,
`scripts/logo.sh` en tire l'icône macOS (`.icns` et le catalogue `AppIcon` de
l'app) et le PNG de Linux. Les deux sont à relancer quand le tracé change, dans
cet ordre. L'app redessine la même marque en SwiftUI (`LogoMark`, dans
`macos/Sources/Style/Kit.swift`) pour la barre des menus et les réglages :
changer l'un, c'est changer l'autre.

## Ce qu'il y a ici

| Fichier | Quand s'en servir |
|---|---|
| `sillage-mark.svg` | la marque seule, au-dessus de 32 pixels |
| `sillage-mark-small.svg` | la marque au trait gras, de 16 à 32 pixels |
| `sillage-lockup.svg` | la marque et le mot, pour un en-tête |
| `sillage-app.svg` | l'icône d'application, plaque blanche comprise |
| `sillage-app-small.svg` | la même, pour les tuiles de 16 et 32 points |
| `sillage-app-dark.svg` | l'icône sur plaque sombre |
| `sillage.icns` | l'icône au format macOS |
| `sillage.png` | 512 pixels, pour les thèmes d'icônes de Linux |

Les SVG de marque et de lockup ont leur jumeau `-dark`, à servir sur fond
sombre : l'encre y devient claire.

**Sous 32 pixels, prendre la variante `small`.** Le trait s'y épaissit et les
rangs s'écartent : le blanc entre eux doit survivre à 16 pixels. C'est le même
logo redessiné pour tenir, pas un autre logo.

## Couleurs

Elles sont définies dans `scripts/logo_gen.py`.

| | |
|---|---|
| `#191919` | l'encre, sur fond clair ; la plaque sombre |
| `#F1F1EF` | l'encre, sur fond sombre |
| `#FFFFFF` | la plaque de l'icône d'application |
| `#E3E2DE` | le filet qui détache la plaque blanche d'un fond blanc |

## Le mot

« sillage » est composé en **Atkinson Hyperlegible Bold** et converti en
contours : le lockup s'affiche partout sans rien exiger de la machine. La
police est sous licence OFL — voir `assets/fonts/OFL-AtkinsonHyperlegible.txt`.
