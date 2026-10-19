#!/usr/bin/env bash
# Compte les textes de l'app macOS qui n'ont pas de français.
#
#   (cd macos && xcodegen generate && xcodebuild … build-for-testing)  # UIBUILD
#   ./scripts/check-xcstrings.sh
#
# 1. Synchronise macos/Resources/Localizable.xcstrings avec les textes que le
#    compilateur a relevés (SWIFT_EMIT_LOC_STRINGS : un .stringsdata par
#    fichier, dans macos/build/derived) : clés exactes, interpolations
#    comprises (`%@`, `%lld`), ce que l'extraction légère ne sait pas.
# 2. Compte les clés sans français `translated`, et celles dont l'anglais
#    est encore du français (lettre accentuée, mot outil).
# 3. Relève aussi, par l'extraction légère de tous les littéraux, les textes
#    français que le catalogue ne voit pas (un `String` passé à une vue, une
#    alerte, une annonce). `// xcstrings: garder` sur la ligne du texte ou
#    celle d'avant le garde en français exprès (texte lu, identifiant).
#
# Écrit les lieux, puis « xcstrings : N texte(s) sans français » ; sort en
# erreur si N > 0. Une clé `shouldTranslate: false` (scripts/xcstrings.py
# garder) ou sans lettre (`%lld %%`) ne compte pas.
set -euo pipefail
cd "$(dirname "$0")/.."

MACOS=$PWD/macos
CATALOGUE=$MACOS/Resources/Localizable.xcstrings
# La configuration de UIBUILD (Debug) seulement : scripts/build-macos.sh
# compile Release dans le même build/derived, et le code `#if DEBUG`
# (Measure.swift…) y manque ; ses clés passeraient pour périmées.
DERIVED=$MACOS/build/derived/Build/Intermediates.noindex/Sillage.build/Debug/Sillage.build/Objects-normal

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Les .stringsdata Debug de la cible Sillage : pour chaque fichier de
# macos/Sources, le plus récent, qui doit l'être plus que le fichier
# lui-même.
if [ ! -d "$DERIVED" ]; then
    echo "Erreur : $DERIVED absent — compiler l'app d'abord (UIBUILD)" >&2
    exit 2
fi
find "$DERIVED" -name '*.stringsdata' >"$TMP/tous"
python3 -I - "$MACOS" "$TMP/tous" "$TMP/compiles" <<'EOF'
import json, os, sys
macos, tous, sortie = sys.argv[1:]
choisis = {}
liste = open(tous, encoding="utf-8").read().splitlines()
for chemin in sorted(liste, key=os.path.getmtime, reverse=True):
    with open(chemin, encoding="utf-8") as f:
        source = json.load(f)["source"]
    if source not in choisis and os.path.exists(source):
        choisis[source] = chemin
perimes = []
for dossier, _, fichiers in os.walk(os.path.join(macos, "Sources")):
    for nom in fichiers:
        if not nom.endswith(".swift"):
            continue
        source = os.path.join(dossier, nom)
        donnees = choisis.get(source)
        if donnees is None or os.path.getmtime(donnees) < os.path.getmtime(source):
            perimes.append(os.path.relpath(source, macos))
if perimes:
    print("Erreur : compilation plus ancienne que le code — relancer UIBUILD :", file=sys.stderr)
    for p in sorted(perimes)[:10]:
        print(f"  {p}", file=sys.stderr)
    sys.exit(2)
with open(sortie, "w", encoding="utf-8") as f:
    f.write("\n".join(choisis.values()))
EOF
COMPILES=()
while IFS= read -r f; do COMPILES+=("$f"); done <"$TMP/compiles"

xcrun xcstringstool sync "$CATALOGUE" --stringsdata "${COMPILES[@]}"

# L'extraction légère : tous les littéraux de macos/Sources.
SOURCES=()
while IFS= read -r f; do SOURCES+=("$f"); done < <(find "$MACOS/Sources" -name '*.swift' | sort)
mkdir "$TMP/leger"
xcrun xcstringstool extract --all-potential-swift-keys --SwiftUI \
    --modern-localizable-strings --legacy-localizable-strings \
    -o "$TMP/leger" "${SOURCES[@]}"
LEGERS=()
while IFS= read -r f; do LEGERS+=("$f"); done < <(find "$TMP/leger" -name '*.stringsdata' | sort)

python3 -I scripts/xcstrings.py verifier --compile "${COMPILES[@]}" --leger "${LEGERS[@]}"
