#!/usr/bin/env python3
"""Le catalogue de textes de l'app macOS (macos/Resources/Localizable.xcstrings).

    scripts/xcstrings.py verifier                 # appelé par check-xcstrings.sh
    scripts/xcstrings.py fr CLÉ FRANÇAIS [CLÉ FRANÇAIS…]
    scripts/xcstrings.py pluriel CLÉ --en UN AUTRES --fr UN AUTRES
    scripts/xcstrings.py garder CLÉ [CLÉ…]        # shouldTranslate: false
    scripts/xcstrings.py purger                   # retire les clés périmées

La clé est le texte anglais tel que le compilateur l'écrit : une
interpolation y devient son format (`\\(name)` → `%@`, un `Int` → `%lld`,
un `UInt32` → `%u`…). `check-xcstrings.sh` synchronise d'abord le catalogue
avec le code : une clé absente est une faute de frappe, ou un code pas
encore compilé.

Le catalogue est réécrit comme Xcode l'écrit (clés triées, « : » entouré
d'espaces, deux espaces d'indentation, pas de saut de ligne final) : le
diff ne montre que ce qui change.
"""

import json
import os
import re
import sys

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MACOS = os.path.join(RACINE, "macos")
SOURCES = os.path.join(MACOS, "Sources")
CATALOGUE = os.path.join(MACOS, "Resources", "Localizable.xcstrings")

# Une lettre accentuée française, ou des guillemets français.
ACCENT = re.compile("[àâäçéèêëîïôöùûüÿœæÀÂÄÇÉÈÊËÎÏÔÖÙÛÜŸŒÆ«»]")
# Des mots outils français, rares en anglais : « Ouvrir un fichier »,
# « Lire le presse-papiers ». Un trait d'union les coupe d'un mot
# (`de-DE`, `Sans-serif`, `un-mute`) ; `son`, `est`, `pas` sont aussi
# anglais (ou des identifiants), donc absents.
MOTS = re.compile(
    r"(?i)(?<![\w'’-])(le|la|les|un|une|des|du|de|pour|sur|avec|dans|sans|"
    r"au|aux|ou|et|ne|ce|cette|vous|votre|vos|sa|ses|par|tout|toute|"
    r"tous|puis)(?![\w'’-])|(?<![\w'’-])[ldnsjcq][’']\w"
)
# Un format (`%@`, `%lld`, `%1$u`, `%%`) ou le `%arg` de l'extraction légère.
FORMAT = re.compile(r"%arg|%(\d+\$)?[-#+ 0]*\d*(\.\d+)?(hh|h|ll|l|q|z|t|j)?[@dDuUxXoOfFeEgGcCsSaAp%]")
# Sur la ligne du texte ou celle d'avant : ce texte reste en français exprès
# (texte lu, identifiant d'accessibilité, sortie de mesure).
GARDER = "xcstrings: garder"
GARDER_FICHIER = "xcstrings: garder tout le fichier"


def francais(texte):
    return bool(ACCENT.search(texte) or MOTS.search(texte))


def normaliser(texte):
    """La clé sans ses formats : `Télécharger %arg` (extraction légère) et
    `Télécharger %@` (compilateur) sont le même texte."""
    return FORMAT.sub("%", texte)


def sans_lettre(texte):
    """`%lld %%`, `%u / %u`, `` : rien à traduire."""
    return not re.search(r"[^\W\d_]", FORMAT.sub("", texte))


# --- Lecture et écriture du catalogue, comme Xcode -------------------------


def lire():
    with open(CATALOGUE, encoding="utf-8") as f:
        return json.load(f)


def _ecrire(valeur, retrait):
    if isinstance(valeur, dict):
        if not valeur:
            return "{\n\n" + " " * retrait + "}"
        dedans = " " * (retrait + 2)
        lignes = [
            dedans + json.dumps(k, ensure_ascii=False) + " : " + _ecrire(valeur[k], retrait + 2)
            for k in sorted(valeur)
        ]
        return "{\n" + ",\n".join(lignes) + "\n" + " " * retrait + "}"
    if isinstance(valeur, list):
        if not valeur:
            return "[\n\n" + " " * retrait + "]"
        dedans = " " * (retrait + 2)
        lignes = [dedans + _ecrire(v, retrait + 2) for v in valeur]
        return "[\n" + ",\n".join(lignes) + "\n" + " " * retrait + "]"
    return json.dumps(valeur, ensure_ascii=False)


def ecrire(catalogue):
    with open(CATALOGUE, "w", encoding="utf-8") as f:
        f.write(_ecrire(catalogue, 0))


def unite(texte):
    return {"stringUnit": {"state": "translated", "value": texte}}


def entree(catalogue, cle):
    try:
        return catalogue["strings"][cle]
    except KeyError:
        sys.exit(
            f"Erreur : « {cle} » n'est pas dans le catalogue — lancer "
            "./scripts/check-xcstrings.sh après la compilation (UIBUILD), "
            "et vérifier la clé (une interpolation y est un format : %@, %lld…)"
        )


def commande_fr(args):
    if not args or len(args) % 2:
        sys.exit("Usage : xcstrings.py fr CLÉ FRANÇAIS [CLÉ FRANÇAIS…]")
    catalogue = lire()
    for cle, texte in zip(args[::2], args[1::2]):
        e = entree(catalogue, cle)
        e.setdefault("localizations", {})["fr"] = unite(texte)
        e.pop("shouldTranslate", None)
    ecrire(catalogue)


def commande_pluriel(args):
    try:
        cle = args[0]
        i, j = args.index("--en"), args.index("--fr")
        en, fr = args[i + 1 : i + 3], args[j + 1 : j + 3]
        assert len(args) == 7 and len(en) == 2 and len(fr) == 2
    except (IndexError, ValueError, AssertionError):
        sys.exit("Usage : xcstrings.py pluriel CLÉ --en UN AUTRES --fr UN AUTRES")
    catalogue = lire()
    e = entree(catalogue, cle)
    loc = e.setdefault("localizations", {})
    for langue, (un, autres) in (("en", en), ("fr", fr)):
        loc[langue] = {"variations": {"plural": {"one": unite(un), "other": unite(autres)}}}
    ecrire(catalogue)


def commande_garder(args):
    if not args:
        sys.exit("Usage : xcstrings.py garder CLÉ [CLÉ…]")
    catalogue = lire()
    for cle in args:
        entree(catalogue, cle)["shouldTranslate"] = False
    ecrire(catalogue)


def commande_purger(args):
    """Retire les clés que le code n'emploie plus (`stale`) : Xcode les
    garde avec leur français, pour qu'on le reprenne ; une fois repris, elles
    encombrent."""
    catalogue = lire()
    perimees = [c for c, e in catalogue["strings"].items() if e.get("extractionState") == "stale"]
    for cle in perimees:
        del catalogue["strings"][cle]
    ecrire(catalogue)
    print(f"{len(perimees)} clé(s) périmée(s) retirée(s)")


# --- Vérification ----------------------------------------------------------


def traduit(localisation):
    """Une unité `translated`, ou toutes celles de ses variations."""
    if "stringUnit" in localisation:
        return localisation["stringUnit"].get("state") == "translated"
    unites = []

    def parcourir(v):
        if isinstance(v, dict):
            if "stringUnit" in v:
                unites.append(v["stringUnit"])
            else:
                for w in v.values():
                    parcourir(w)

    parcourir(localisation.get("variations", {}))
    return bool(unites) and all(u.get("state") == "translated" for u in unites)


def textes_unites(localisation):
    """Les valeurs d'une localisation, variations comprises."""
    textes = []

    def parcourir(v):
        if isinstance(v, dict):
            if "stringUnit" in v:
                textes.append(v["stringUnit"].get("value", ""))
            else:
                for w in v.values():
                    parcourir(w)

    parcourir(localisation)
    return textes


def textes_anglais(cle, e):
    """La clé, et les valeurs anglaises qui la remplacent s'il y en a."""
    return [cle] + textes_unites(e.get("localizations", {}).get("en", {}))


def lieux_compiles(fichiers):
    """Clé → lieux `Sources/…:ligne`, d'après les .stringsdata du compilateur."""
    lieux = {}
    for chemin in fichiers:
        with open(chemin, encoding="utf-8") as f:
            d = json.load(f)
        source = os.path.relpath(d["source"], MACOS)
        for table, entrees in d.get("tables", {}).items():
            for e in entrees:
                ligne = e.get("location", {}).get("startingLine", 0)
                lieux.setdefault(e["key"], []).append(f"{source}:{ligne}")
    return lieux


# Un journal (`log.info(`, `Self.log.error(`, `print(`) ou un arrêt
# (`fatalError(`) : reste en français, jamais vu dans l'interface.
JOURNAL = re.compile(
    r"(\blog\w*\.(debug|info|notice|error|fault|warning|trace|critical|log)|"
    r"\b(print|NSLog|os_log|fatalError|preconditionFailure|assertionFailure|precondition|assert))\(\s*$"
)


def gardes(source, ligne, colonne, cache):
    """Un texte gardé en français : marqué `xcstrings: garder`, ou journal."""
    if source not in cache:
        with open(os.path.join(MACOS, source), encoding="utf-8") as f:
            cache[source] = f.read().split("\n")
    lignes = cache[source]
    if any(GARDER_FICHIER in l for l in lignes):
        return True
    if not 1 <= ligne <= len(lignes):
        return False
    if any(GARDER in l for l in lignes[max(ligne - 2, 0) : ligne]):
        return True
    avant = lignes[ligne - 1][: max(colonne - 1, 0)]
    if JOURNAL.search(avant):
        return True
    return not avant.strip() and ligne >= 2 and bool(JOURNAL.search(lignes[ligne - 2]))


def commande_verifier(args):
    """verifier --compile F… --leger F… : compte les textes sans français."""
    i = args.index("--leger")
    compiles, legers = args[1:i], args[i + 1 :]
    catalogue = lire()
    lieux = lieux_compiles(compiles)
    manquants = {}  # texte → lieux

    # 1. Les clés du catalogue : un français `translated`, et un anglais qui
    #    n'est plus le français d'avant.
    connues = set()
    # Le français déjà traduit : le même texte resté littéral ailleurs
    # (« Lecture » dans une annonce) n'est pas encore converti.
    valeurs_fr = set()
    for cle, e in catalogue["strings"].items():
        connues.add(normaliser(cle))
        fr = e.get("localizations", {}).get("fr", {})
        valeurs_fr.update(normaliser(t) for t in textes_unites(fr))
        if e.get("extractionState") == "stale" or e.get("shouldTranslate") is False:
            continue
        if sans_lettre(cle):
            continue
        sans_fr = not traduit(e.get("localizations", {}).get("fr", {}))
        if sans_fr or any(francais(t) for t in textes_anglais(cle, e)):
            manquants[cle] = lieux.get(cle, ["(hors du code : clé périmée ?)"])

    # 2. Les textes français que le catalogue ne voit pas : un `String` passé
    #    à une vue, une alerte, une annonce… (extraction légère, tous les
    #    littéraux).
    cache = {}
    for chemin in legers:
        with open(chemin, encoding="utf-8") as f:
            d = json.load(f)
        source = os.path.relpath(d["source"], MACOS)
        for entrees in d.get("tables", {}).values():
            for e in entrees:
                cle = e["key"]
                n = normaliser(cle)
                if n in connues or not (francais(cle) or n in valeurs_fr):
                    continue
                lieu = e.get("location", {})
                ligne = lieu.get("startingLine", 0)
                if gardes(source, ligne, lieu.get("startingColumn", 1), cache):
                    continue
                manquants.setdefault(cle, []).append(f"{source}:{ligne}")

    par_fichier = {}
    for cle, ou in manquants.items():
        for lieu in sorted(set(ou)):
            fichier, _, ligne = lieu.rpartition(":")
            if not fichier:
                fichier, ligne = lieu, "0"
            par_fichier.setdefault(fichier, []).append((int(ligne or 0), cle))
    for fichier in sorted(par_fichier):
        print(fichier)
        for ligne, cle in sorted(par_fichier[fichier]):
            court = cle if len(cle) <= 80 else cle[:79] + "…"
            court = court.replace("\n", "⏎")
            print(f"  {ligne:>5}  « {court} »")
    perimees = sum(1 for e in catalogue["strings"].values() if e.get("extractionState") == "stale")
    if perimees:
        print(f"note : {perimees} clé(s) périmée(s), hors du code (scripts/xcstrings.py purger)")
    n = len(manquants)
    print(f"xcstrings : {n} texte{'s' if n > 1 else ''} sans français")
    return 1 if n else 0


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    commande, args = sys.argv[1], sys.argv[2:]
    if commande == "verifier":
        sys.exit(commande_verifier(args))
    elif commande == "fr":
        commande_fr(args)
    elif commande == "pluriel":
        commande_pluriel(args)
    elif commande == "garder":
        commande_garder(args)
    elif commande == "purger":
        commande_purger(args)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
