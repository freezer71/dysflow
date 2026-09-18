#!/usr/bin/env python3
# Dessine le logo : un sillon de disque, coupé par la ligne de lecture.
#
# Le tracé est calculé, pas dessiné à la main — une spirale d'Archimède ne
# s'écrit pas à la souris sans que les tours se déforment. Le script en sort
# des courbes de Bézier (et non une polyligne de mille points) : les SVG
# restent courts, lisibles, et retouchables.
#
#   ./venv/bin/python scripts/logo_gen.py     # fonttools requis pour le mot

import math
import os

ENCRE = "#161616"   # thème clair : bright
CREME = "#FBF9F5"   # thème clair : background
CLAIR = "#EEEEEE"   # thème sombre : bright
ORANGE = "#F26B27"  # ACCENT, commun aux deux thèmes

CX = CY = 32.0      # centre de la case
RACINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SORTIE = os.path.join(RACINE, "assets", "logo")
POLICE = os.path.join(RACINE, "assets", "fonts", "AtkinsonHyperlegible-Bold.ttf")


# --- La spirale -----------------------------------------------------------

def _point(t, r0, k, phase):
    r = r0 + k * t
    return (CX + r * math.cos(t + phase), CY + r * math.sin(t + phase))


def _tangente(t, r0, k, phase):
    """Dérivée du point : sert à poser les poignées des Béziers."""
    r, a = r0 + k * t, t + phase
    return (k * math.cos(a) - r * math.sin(a), k * math.sin(a) + r * math.cos(a))


def _dans_la_saignee(t, r0, k, phase, gap):
    """La ligne de lecture ne creuse qu'à gauche : à droite, le disque est plein."""
    x, y = _point(t, r0, k, phase)
    return abs(y - CY) < gap and x <= CX


def _intervalles(r0, k, phase, gap, t_max, pas=math.radians(0.25)):
    """Les portions de spirale que la saignée laisse intactes.

    On balaie finement pour repérer les changements d'état, puis on affine la
    frontière par dichotomie : une entaille au pixel près, sans échantillonner
    le tracé lui-même."""
    def frontiere(a, b):
        for _ in range(40):
            m = (a + b) / 2
            if _dans_la_saignee(m, r0, k, phase, gap) == _dans_la_saignee(a, r0, k, phase, gap):
                a = m
            else:
                b = m
        return (a + b) / 2

    gardes, debut = [], None
    t, precedent = 0.0, _dans_la_saignee(0.0, r0, k, phase, gap)
    if not precedent:
        debut = 0.0
    while t < t_max:
        t = min(t + pas, t_max)
        courant = _dans_la_saignee(t, r0, k, phase, gap)
        if courant != precedent:
            bord = frontiere(t - pas, t)
            if courant:                      # on entre dans la saignée
                gardes.append((debut, bord))
                debut = None
            else:                            # on en sort
                debut = bord
            precedent = courant
    if debut is not None:
        gardes.append((debut, t_max))
    return [(a, b) for a, b in gardes if b - a > 1e-3]


def _beziers(t0, t1, r0, k, phase, arc=math.pi / 4):
    """Approxime la spirale entre t0 et t1 par des cubiques.

    Sur un huitième de tour, l'écart entre la cubique d'Hermite et la spirale
    est très inférieur à l'épaisseur du trait : inutile de découper plus fin."""
    n = max(1, math.ceil((t1 - t0) / arc))
    h = (t1 - t0) / n
    x, y = _point(t0, r0, k, phase)
    d = [f"M {x:.2f} {y:.2f}"]
    for i in range(n):
        ta, tb = t0 + i * h, t0 + (i + 1) * h
        ax, ay = _point(ta, r0, k, phase)
        bx, by = _point(tb, r0, k, phase)
        dax, day = _tangente(ta, r0, k, phase)
        dbx, dby = _tangente(tb, r0, k, phase)
        d.append(f"C {ax + dax*h/3:.2f} {ay + day*h/3:.2f}"
                 f" {bx - dbx*h/3:.2f} {by - dby*h/3:.2f}"
                 f" {bx:.2f} {by:.2f}")
    return " ".join(d)


def sillon(tours, r_int, r_ext, gap, phase=-math.pi / 2):
    """Le sillon entier, entaillé par la ligne de lecture."""
    t_max = tours * 2 * math.pi
    k = (r_ext - r_int) / t_max
    return [_beziers(a, b, r_int, k, phase)
            for a, b in _intervalles(r_int, k, phase, gap, t_max)]


# --- La marque ------------------------------------------------------------

def marque(encre=ENCRE, tours=3, r_int=6.5, r_ext=27, trait=3.4,
           gap=4.2, ligne=4.4, tete=6.0, depart=4.0, indent=""):
    """Le disque, la ligne de lecture qui y entre, et la tête de lecture."""
    traces = "".join(f'\n{indent}    <path d="{d}"/>'
                     for d in sillon(tours, r_int, r_ext, gap))
    return (
        f'{indent}<g fill="none" stroke="{encre}" stroke-width="{trait}"'
        f' stroke-linecap="round">{traces}\n{indent}</g>\n'
        f'{indent}<line x1="{depart}" y1="{CY}" x2="{CX}" y2="{CY}"'
        f' stroke="{ORANGE}" stroke-width="{ligne}" stroke-linecap="round"/>\n'
        f'{indent}<circle cx="{CX}" cy="{CY}" r="{tete}" fill="{ORANGE}"/>'
    )


# Sous 32 pixels, trois tours virent à la tache : la petite marque en garde
# deux, sur un trait plus gras. C'est le même dessin, pas un autre logo.
PETITE = dict(tours=2, r_int=8, r_ext=26, trait=5.0, gap=5.5,
              ligne=5.5, tete=7.0, depart=3.0)


# --- Le mot ---------------------------------------------------------------

def mot(texte, taille):
    """« sillon » en Atkinson Hyperlegible, converti en tracés.

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

def icone(**dessin):
    """La marque posée sur le carré du système.

    macOS n'arrondit rien lui-même : le carré et sa marge font partie du
    dessin. Un côté de 824/1024 et un rayon de 22,37 % approchent de près le
    gabarit d'Apple. La marque est réduite pour garder son air autour d'elle."""
    cote, rayon = 51.5, 11.5
    marge = (64 - cote) / 2
    return (f'  <rect x="{marge}" y="{marge}" width="{cote}" height="{cote}"'
            f' rx="{rayon}" fill="{CREME}"/>\n'
            f'  <g transform="translate({CX} {CY}) scale(0.72) translate(-{CX} -{CY})">\n'
            + marque(ENCRE, indent="    ", **dessin) + "\n  </g>")


# --- Les fichiers ---------------------------------------------------------

def svg(corps, largeur=64, hauteur=64, fond=None):
    fond = "" if fond is None else f'\n  <rect width="{largeur}" height="{hauteur}" fill="{fond}"/>'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {largeur} {hauteur}"'
            f' width="{largeur}" height="{hauteur}" role="img" aria-label="sillon">'
            f'{fond}\n{corps}\n</svg>\n')


def lockup(encre=ENCRE, taille=46, gouttiere=20):
    d, largeur_mot, limites = mot("sillon", taille)
    # Le mot est calé sur le centre optique de ses propres contours, pas sur
    # sa ligne de base : c'est ce qui l'aligne vraiment avec l'axe du disque.
    ligne_de_base = CY - (limites[1] + limites[3]) / 2
    x = 64 + gouttiere
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

    ecrire("sillon-mark.svg", svg(marque(ENCRE, indent="  ")))
    ecrire("sillon-mark-dark.svg", svg(marque(CLAIR, indent="  ")))
    ecrire("sillon-mark-small.svg", svg(marque(ENCRE, indent="  ", **PETITE)))
    ecrire("sillon-mark-small-dark.svg", svg(marque(CLAIR, indent="  ", **PETITE)))

    for suffixe, encre in (("", ENCRE), ("-dark", CLAIR)):
        corps, largeur = lockup(encre)
        ecrire(f"sillon-lockup{suffixe}.svg", svg(corps, largeur=round(largeur + 4), hauteur=64))

    ecrire("sillon-app.svg", svg(icone()))
    ecrire("sillon-app-small.svg", svg(icone(**PETITE)))


if __name__ == "__main__":
    main()
