#!/usr/bin/env python3
# Dessine le logo : une tête de profil, tracée à main levée d'un seul trait,
# et dans le crâne un réseau de nœuds clairs dont une branche passe le front.
# C'est la pensée qui ne suit pas le chemin habituel : la dyslexie, et plus
# largement le cerveau neuroatypique, pour qui dysflow est fait.
#
# Aux couleurs de dysflow : l'encre, le blanc cassé et le vert citron de ce
# qui est choisi. Un trait qui a l'air dessiné, des nœuds ronds, rien d'autre. Le tracé est
# calculé sur une grille de 64.
#
#   python3 scripts/logo_gen.py     # fonttools requis pour le mot (lockup)

import os

ENCRE = "#191919"   # le trait sur fond clair ; la plaque de l'icône
CLAIR = "#F2F2EF"   # le trait sur fond sombre et sur la plaque
CITRON = "#C6F36B"  # le réseau
SOIR = "#262625"    # plaque de l'icône d'application, thème sombre

CX = CY = 32.0      # centre de la case
RACINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SORTIE = os.path.join(RACINE, "assets", "logo")
POLICE = os.path.join(RACINE, "assets", "fonts", "AtkinsonHyperlegible-Bold.ttf")


# --- La marque ------------------------------------------------------------

# Le profil, en courbes de Bézier : la nuque, le crâne, le front, puis le nez,
# les lèvres et le menton, et le cou. Le trait reste ouvert aux deux bouts.
TRAIT = ("M 23 55"
         " C 22 49 17 44 16.5 36"
         " C 15.5 22 24 12.5 34 12.5"
         " C 42 12.5 46 18.5 45.5 26"
         " C 45.3 27.8 44.3 28.8 44.8 30.5"
         " C 46 33 49.5 35 49.8 36.3"
         " C 50 37.3 48 37.8 46.8 38"
         " C 47.8 39 47.8 40 46.6 40.8"
         " C 47.6 41.5 47.4 42.6 46.4 43.3"
         " C 46 44 46.6 45.2 46.6 46"
         " C 46.4 48 43 48.6 40 48.4"
         " C 38.6 50.5 39 53 39.8 55.5")

# Le réseau : un nœud central dans le crâne, et ses branches ; la dernière
# traverse le front. (x, y, rayon) ; le premier est le centre.
RESEAU = [(30.0, 27.0, 3.8), (22.5, 21.0, 2.0), (23.5, 33.5, 1.8),
          (36.0, 18.5, 2.0), (33.0, 35.0, 1.7), (52.0, 21.5, 2.1)]


def marque(encre=ENCRE, reseau=CITRON, indent="", trait=3.2, branche=1.5,
           noeuds=RESEAU, detour=None, marge=0.7):
    """Le trait, puis le réseau posé dessus.

    `detour` cerne le réseau d'un liseré de `marge` : à l'encre sur fond
    clair, il tient le citron, trop pâle pour se détacher seul du blanc."""
    (cx, cy, _), feuilles = noeuds[0], noeuds[1:]
    lignes = []
    lignes.append(f'{indent}<path d="{TRAIT}" fill="none" stroke="{encre}"'
                  f' stroke-width="{trait}" stroke-linecap="round" stroke-linejoin="round"/>')
    passes = [(detour, marge)] if detour else []
    for couleur, liseré in passes + [(reseau, 0.0)]:
        lignes.append(f'{indent}<g fill="{couleur}" stroke="{couleur}" stroke-linecap="round"'
                      f' stroke-width="{branche + 2 * liseré:.2f}">')
        for x, y, _ in feuilles:
            lignes.append(f'{indent}  <line x1="{cx}" y1="{cy}" x2="{x}" y2="{y}"/>')
        for x, y, r in noeuds:
            lignes.append(f'{indent}  <circle cx="{x}" cy="{y}" r="{r + liseré:.2f}" stroke="none"/>')
        lignes.append(f"{indent}</g>")
    return "\n".join(lignes)


# Sous 32 pixels, le trait s'épaissit et le réseau ne garde que deux
# branches, plus grosses, dont celle qui passe le front : il doit rester
# lisible à 16 pixels. Même dessin, pas un autre logo.
PETITE = dict(trait=4.6, branche=2.4,
              noeuds=[(30.0, 27.0, 5.0), (22.5, 20.0, 2.8), (52.0, 21.5, 3.0)])


# --- Le mot ---------------------------------------------------------------

def mot(texte, taille):
    """« Dysflow » en Atkinson Hyperlegible, converti en tracés.

    Le lockup ne doit rien exiger de la machine qui l'affiche : la police est
    celle que le lecteur emploie déjà, mais elle voyage ici sous forme de
    contours, pas de texte."""
    from fontTools.ttLib import TTFont
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.misc.transform import Transform

    police = TTFont(POLICE)
    upem = police["head"].unitsPerEm
    table, glyphes, largeurs = police.getBestCmap(), police.getGlyphSet(), police["hmtx"]
    echelle = taille / upem

    def tracer(plume, depart_x):
        curseur = depart_x
        for c in texte:
            nom = table[ord(c)]
            # Y descend en SVG, monte dans la police : la matrice le retourne.
            glyphes[nom].draw(TransformPen(plume, Transform(echelle, 0, 0, -echelle, curseur, 0)))
            curseur += largeurs[nom][0] * echelle
        return curseur

    limites = BoundsPen(glyphes)
    tracer(limites, 0.0)
    plume = SVGPathPen(glyphes)
    largeur = tracer(plume, 0.0)
    return plume.getCommands(), largeur, limites.bounds


# --- L'icône d'application ------------------------------------------------

def icone(plaque=ENCRE, encre=CLAIR, reseau=CITRON, **dessin):
    """La marque posée sur le carré du système.

    macOS n'arrondit rien lui-même : le carré et sa marge font partie du
    dessin. Un côté de 824/1024 et un rayon de 22,37 % approchent de près le
    gabarit d'Apple."""
    cote, rayon = 51.5, 11.5
    marge = (64 - cote) / 2
    return (f'  <rect x="{marge}" y="{marge}" width="{cote}" height="{cote}"'
            f' rx="{rayon}" fill="{plaque}"/>\n'
            f'  <g transform="translate({CX} {CY}) scale(0.9) translate(-{CX} -{CY})">\n'
            + marque(encre, reseau, indent="    ", **dessin) + "\n  </g>")


# --- Les fichiers ---------------------------------------------------------

def svg(corps, largeur=64, hauteur=64):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {largeur} {hauteur}"'
            f' width="{largeur}" height="{hauteur}" role="img" aria-label="Dysflow">'
            f'\n{corps}\n</svg>\n')


def lockup(encre=ENCRE, taille=40, gouttiere=8):
    d, largeur_mot, limites = mot("Dysflow", taille)
    # Le mot est calé sur le centre optique de ses propres contours, pas sur
    # sa ligne de base : c'est ce qui l'aligne vraiment avec la marque.
    ligne_de_base = CY - (limites[1] + limites[3]) / 2
    x = 64 - 10 + gouttiere
    corps = (marque(encre, detour=encre if encre == ENCRE else None, indent="  ") +
             f'\n  <g transform="translate({x} {ligne_de_base:.2f})">'
             f'\n    <path d="{d}" fill="{encre}"/>\n  </g>')
    return corps, x + largeur_mot


def ecrire(nom, contenu):
    chemin = os.path.join(SORTIE, nom)
    with open(chemin, "w") as f:
        f.write(contenu)
    print(f"  {os.path.relpath(chemin, RACINE)}")


def main():
    os.makedirs(SORTIE, exist_ok=True)
    print("Logo :")

    ecrire("dysflow-mark.svg", svg(marque(ENCRE, detour=ENCRE, indent="  ")))
    ecrire("dysflow-mark-dark.svg", svg(marque(CLAIR, indent="  ")))
    ecrire("dysflow-mark-small.svg", svg(marque(ENCRE, detour=ENCRE, indent="  ", **PETITE)))
    ecrire("dysflow-mark-small-dark.svg", svg(marque(CLAIR, indent="  ", **PETITE)))

    for suffixe, encre in (("", ENCRE), ("-dark", CLAIR)):
        corps, largeur = lockup(encre)
        ecrire(f"dysflow-lockup{suffixe}.svg", svg(corps, largeur=round(largeur + 4), hauteur=64))

    ecrire("dysflow-app.svg", svg(icone()))
    ecrire("dysflow-app-small.svg", svg(icone(**PETITE)))
    ecrire("dysflow-app-dark.svg", svg(icone(SOIR)))


if __name__ == "__main__":
    main()
