# Le logo

Une tête de profil, tracée à main levée d'un seul trait, et dans le crâne un
réseau de nœuds clairs dont une branche passe le front : la pensée qui ne suit
pas le chemin habituel. C'est la dyslexie, et plus largement le cerveau
neuroatypique, pour qui dysflow est fait.

Aux couleurs de dysflow : l'encre, le blanc cassé et le vert citron de ce qui
est choisi, avec un trait qui a l'air dessiné. Elle a remplacé, le 25 octobre 2026, le « s »
géométrique à la Notion (lui-même venu après le disque à l'accent orange, le
14 octobre).

Le dessin est calculé, pas dessiné : `scripts/logo_gen.py` écrit les SVG,
`scripts/logo.sh` en tire l'icône macOS (`.icns` et le catalogue `AppIcon` de
l'app) et le PNG de Linux. Les deux sont à relancer quand le tracé change, dans
cet ordre. L'app redessine la même marque en SwiftUI (`LogoMark`, dans
`macos/Sources/Style/Kit.swift`) pour la barre des menus et les réglages :
changer l'un, c'est changer l'autre.

## Ce qu'il y a ici

| Fichier | Quand s'en servir |
|---|---|
| `dysflow-mark.svg` | la marque seule, au-dessus de 32 pixels |
| `dysflow-mark-small.svg` | la marque au trait gras, de 16 à 32 pixels |
| `dysflow-lockup.svg` | la marque et le mot, pour un en-tête |
| `dysflow-app.svg` | l'icône d'application, plaque à l'encre comprise |
| `dysflow-app-small.svg` | la même, pour les tuiles de 16 et 32 points |
| `dysflow-app-dark.svg` | l'icône sur plaque sombre |
| `dysflow.icns` | l'icône au format macOS |
| `dysflow.png` | 512 pixels, pour les thèmes d'icônes de Linux |

Les SVG de marque et de lockup ont leur jumeau `-dark`, à servir sur fond
sombre : l'encre y devient claire. Sur fond clair, le réseau citron est cerné
d'encre, sans quoi il disparaîtrait dans le blanc.

**Sous 32 pixels, prendre la variante `small`.** Le trait s'y épaissit et le
réseau ne garde que deux branches, plus grosses, dont celle qui passe le
front : il doit survivre à 16 pixels. C'est le même
logo redessiné pour tenir, pas un autre logo.

## Couleurs

Elles sont définies dans `scripts/logo_gen.py`.

| | |
|---|---|
| `#191919` | le trait sur fond clair ; la plaque de l'icône |
| `#F2F2EF` | le trait sur fond sombre et sur la plaque |
| `#C6F36B` | le réseau, le vert citron de l'app et du site |
| `#262625` | la plaque de l'icône, thème sombre |

## Le mot

« dysflow » est composé en **Atkinson Hyperlegible Bold** et converti en
contours : le lockup s'affiche partout sans rien exiger de la machine. La
police est sous licence OFL — voir `assets/fonts/OFL-AtkinsonHyperlegible.txt`.
