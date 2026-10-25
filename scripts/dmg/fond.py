"""Le fond de la fenêtre du .dmg : l'app à gauche, Applications à droite,
une flèche entre les deux, aux couleurs de Dysflow.

    uv run --with fonttools python3 scripts/dmg/fond.py assets/dmg

Écrit <dossier>/fond.png (640 × 400) et fond@2x.png (1280 × 800), que
dmgbuild assemble pour les écrans Retina. Les deux images sont versionnées :
la CI les prend telles quelles, sans fonttools ni rsvg-convert. Le texte voyage en tracés
(Atkinson Hyperlegible) : rien n'est demandé à la machine qui l'affiche.
Les positions des icônes sont celles de scripts/dmg/reglages.py."""

import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from logo_gen import CITRON, ENCRE, mot  # noqa: E402

# Le bas reste libre : la barre de chemin du Finder, si on l'a activée,
# s'y pose par-dessus le fond.
LARGEUR, HAUTEUR = 640, 400
APP, APPLICATIONS = (170, 165), (470, 165)  # centres des icônes, en points
CREME, ENCRE_DOUCE = "#F8F3E6", "#6B6558"


def texte(chaine, taille, centre_x, base_y, couleur):
    d, largeur, _ = mot(chaine, taille)
    x = centre_x - largeur / 2
    return f'<path transform="translate({x:.1f} {base_y})" d="{d}" fill="{couleur}"/>'


def fond():
    (ax, ay), (bx, by) = APP, APPLICATIONS
    # La flèche : un trait souple, du même dessin que la tête du logo
    # (trait rond, 3,2), qui monte un peu et retombe sur Applications.
    x0, x1, y = ax + 86, bx - 86, ay - 4
    fleche = (
        f'<path d="M {x0} {y} C {x0 + 40} {y - 26} {x1 - 40} {y - 26} {x1} {y}" '
        f'fill="none" stroke="{ENCRE}" stroke-width="3.2" stroke-linecap="round"/>'
        f'<path d="M {x1 - 13} {y - 10} L {x1} {y} L {x1 - 15} {y + 5}" '
        f'fill="none" stroke="{ENCRE}" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>'
        f'<circle cx="{x0}" cy="{y}" r="4.2" fill="{CITRON}" stroke="{ENCRE}" stroke-width="1.4"/>'
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {LARGEUR} {HAUTEUR}">'
        f'<rect width="{LARGEUR}" height="{HAUTEUR}" fill="{CREME}"/>'
        + fleche
        + texte("Glissez Dysflow dans Applications", 19, LARGEUR / 2, 300, ENCRE)
        + texte("Drag Dysflow to Applications", 14, LARGEUR / 2, 326, ENCRE_DOUCE)
        + "</svg>"
    )


if __name__ == "__main__":
    dossier = sys.argv[1]
    os.makedirs(dossier, exist_ok=True)
    svg = os.path.join(dossier, "fond.svg")
    with open(svg, "w", encoding="utf-8") as f:
        f.write(fond())
    for nom, echelle in (("fond.png", 1), ("fond@2x.png", 2)):
        subprocess.run(["rsvg-convert", "-w", str(LARGEUR * echelle), "-h", str(HAUTEUR * echelle),
                        svg, "-o", os.path.join(dossier, nom)], check=True)
