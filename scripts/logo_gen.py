#!/usr/bin/env python3
# Dessine le logo : un « s » tracé d'un seul trait, le sillage que la voix
# laisse — trois rangs, deux demi-tours, comme l'œil qui suit les lignes.
# Le point, au bout du trait, est la tête de lecture.
#
# Monochrome, à la manière des icônes de Notion : une seule encre, des droites
# et des demi-cercles, rien d'autre. Le tracé est calculé sur une grille de 64.
#
#   python3 scripts/logo_gen.py     # fonttools requis pour le mot (lockup)

import os

ENCRE = "#191919"   # encre, sur fond clair
CLAIR = "#F1F1EF"   # encre, sur fond sombre
BLANC = "#FFFFFF"   # plaque de l'icône d'application, thème clair
NUIT = "#191919"    # plaque de l'icône d'application, thème sombre
FILET = "#E3E2DE"   # bord de la plaque claire

CX = CY = 32.0      # centre de la case
RACINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SORTIE = os.path.join(RACINE, "assets", "logo")
POLICE = os.path.join(RACINE, "assets", "fonts", "AtkinsonHyperlegible-Bold.ttf")


# --- La marque ------------------------------------------------------------

def sillage(largeur=26.0, ecart=13.0, trait=5.0, point=3.4, jeu=3.6):
    """Le trait en « s » et le point qui le termine.

    Trois rangs de `largeur`, espacés de `ecart` ; les demi-tours ont pour
    rayon la moitié de l'écart, ce qui les rend parfaitement ronds. Le dernier
    rang s'arrête court : le point y prend place, détaché de `jeu`."""
    r = ecart / 2
    g, d = CX - largeur / 2, CX + largeur / 2
    haut, milieu, bas = CY - ecart, CY, CY + ecart
    # Le point s'aligne sur le bord gauche des rangs ; le trait s'arrête avant.
    fin = g + point + jeu + trait / 2
    chemin = (f"M {d:.2f} {haut:.2f} H {g:.2f}"
              f" A {r:.2f} {r:.2f} 0 0 0 {g:.2f} {milieu:.2f}"
              f" H {d:.2f}"
              f" A {r:.2f} {r:.2f} 0 0 1 {d:.2f} {bas:.2f}"
              f" H {fin:.2f}")
    return chemin, (g, bas, point), trait


def marque(encre=ENCRE, indent="", **dessin):
    chemin, (px, py, pr), trait = sillage(**dessin)
    return (f'{indent}<path d="{chemin}" fill="none" stroke="{encre}"'
            f' stroke-width="{trait}" stroke-linecap="round" stroke-linejoin="round"/>\n'
            f'{indent}<circle cx="{px:.2f}" cy="{py:.2f}" r="{pr}" fill="{encre}"/>')


# Sous 32 pixels, le trait s'épaissit et les rangs s'écartent un peu : le
# blanc entre eux doit survivre à 16 pixels. Même dessin, pas un autre logo.
PETITE = dict(largeur=24.0, ecart=15.0, trait=7.0, point=4.6, jeu=3.4)


# --- Le mot ---------------------------------------------------------------

def mot(texte, taille):
    """« sillage » en Atkinson Hyperlegible, converti en tracés.

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

def icone(plaque=BLANC, encre=ENCRE, filet=FILET, **dessin):
    """La marque posée sur le carré du système.

    macOS n'arrondit rien lui-même : le carré et sa marge font partie du
    dessin. Un côté de 824/1024 et un rayon de 22,37 % approchent de près le
    gabarit d'Apple. Un filet d'un demi-point détache la plaque blanche d'un
    fond blanc, comme le carré de Notion."""
    cote, rayon = 51.5, 11.5
    marge = (64 - cote) / 2
    bord = (f' stroke="{filet}" stroke-width="0.5"' if filet else "")
    return (f'  <rect x="{marge}" y="{marge}" width="{cote}" height="{cote}"'
            f' rx="{rayon}" fill="{plaque}"{bord}/>\n'
            f'  <g transform="translate({CX} {CY}) scale(0.78) translate(-{CX} -{CY})">\n'
            + marque(encre, indent="    ", **dessin) + "\n  </g>")


# --- Les fichiers ---------------------------------------------------------

def svg(corps, largeur=64, hauteur=64):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {largeur} {hauteur}"'
            f' width="{largeur}" height="{hauteur}" role="img" aria-label="sillage">'
            f'\n{corps}\n</svg>\n')


def lockup(encre=ENCRE, taille=40, gouttiere=8):
    d, largeur_mot, limites = mot("sillage", taille)
    # Le mot est calé sur le centre optique de ses propres contours, pas sur
    # sa ligne de base : c'est ce qui l'aligne vraiment avec la marque.
    ligne_de_base = CY - (limites[1] + limites[3]) / 2
    x = 64 - 10 + gouttiere
    corps = (marque(encre, indent="  ") +
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

    ecrire("sillage-mark.svg", svg(marque(ENCRE, indent="  ")))
    ecrire("sillage-mark-dark.svg", svg(marque(CLAIR, indent="  ")))
    ecrire("sillage-mark-small.svg", svg(marque(ENCRE, indent="  ", **PETITE)))
    ecrire("sillage-mark-small-dark.svg", svg(marque(CLAIR, indent="  ", **PETITE)))

    for suffixe, encre in (("", ENCRE), ("-dark", CLAIR)):
        corps, largeur = lockup(encre)
        ecrire(f"sillage-lockup{suffixe}.svg", svg(corps, largeur=round(largeur + 4), hauteur=64))

    ecrire("sillage-app.svg", svg(icone()))
    ecrire("sillage-app-small.svg", svg(icone(**PETITE)))
    ecrire("sillage-app-dark.svg", svg(icone(NUIT, CLAIR, None)))


if __name__ == "__main__":
    main()
