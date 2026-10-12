#!/usr/bin/env bash
# Construit espeak-ng (GPL-3.0) comme programme séparé, pour la phonétisation
# de Kokoro. Sillon ne s'y lie pas : il le lance et lui parle par ses entrées
# et sorties standard, ce qui fait de l'ensemble un agrégat.
#
#   ./scripts/build-espeak.sh
#
# Produit target/espeak/bin/espeak-ng, target/espeak/share/espeak-ng-data
# (français et anglais seulement), target/espeak/COPYING et SOURCE.txt.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=1.52.0
URL="https://github.com/espeak-ng/espeak-ng/archive/refs/tags/$VERSION.tar.gz"
# Empreinte de l'archive (relevée le 12 oct. 2026) : une archive différente
# n'est jamais construite.
SHA256=bb4338102ff3b49a81423da8a1a158b420124b055b60fa76cfb4b18677130a23
OUT=target/espeak
TARBALL="$OUT/espeak-ng-$VERSION.tar.gz"
SRC="$OUT/espeak-ng-$VERSION"
mkdir -p "$OUT"

verify() { echo "$SHA256  $1" | shasum -a 256 -c --status; }

# L'archive : téléchargée sous un nom provisoire, gardée seulement si son
# empreinte est la bonne.
if [ ! -f "$TARBALL" ] || ! verify "$TARBALL"; then
    rm -f "$TARBALL" "$TARBALL.part"
    curl -fsSL "$URL" -o "$TARBALL.part"
    if ! verify "$TARBALL.part"; then
        rm -f "$TARBALL.part"
        echo "Erreur : empreinte SHA-256 inattendue pour $URL" >&2
        exit 1
    fi
    mv "$TARBALL.part" "$TARBALL"
    rm -rf "$SRC"
fi

# Les sources : extraites à part, mises en place seulement une fois complètes
# (une extraction interrompue est refaite).
if [ ! -f "$SRC/.extrait" ]; then
    rm -rf "$SRC" "$OUT/extraction"
    mkdir -p "$OUT/extraction"
    tar -xzf "$TARBALL" -C "$OUT/extraction"
    mv "$OUT/extraction/espeak-ng-$VERSION" "$SRC"
    rmdir "$OUT/extraction"
    touch "$SRC/.extrait"
fi

# Programme autonome : bibliothèque statique, ni son, ni MBROLA, ni Klatt.
cmake -S "$SRC" -B "$OUT/build" -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF -DUSE_ASYNC=OFF -DUSE_MBROLA=OFF -DUSE_LIBSONIC=OFF \
    -DUSE_LIBPCAUDIO=OFF -DUSE_KLATT=OFF -DUSE_SPEECHPLAYER=OFF \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DCMAKE_OSX_ARCHITECTURES=arm64
cmake --build "$OUT/build" -j

BIN=$(find "$OUT/build" -type f -name espeak-ng -perm -u+x | head -1)
DATA=$(find "$OUT/build" -type d -name espeak-ng-data | head -1)
for f in phontab phonindex phondata intonations fr_dict en_dict; do
    if [ ! -f "$DATA/$f" ]; then
        echo "Erreur : $DATA/$f manque (construction des données d'espeak-ng)" >&2
        exit 1
    fi
done

rm -rf "$OUT/bin" "$OUT/share"
mkdir -p "$OUT/bin" "$OUT/share/espeak-ng-data"
cp "$BIN" "$OUT/bin/espeak-ng"
for f in phontab phonindex phondata phondata-manifest intonations fr_dict en_dict; do
    [ -f "$DATA/$f" ] && cp "$DATA/$f" "$OUT/share/espeak-ng-data/"
done
cp -R "$DATA/lang" "$OUT/share/espeak-ng-data/"
[ -d "$DATA/voices" ] && cp -R "$DATA/voices" "$OUT/share/espeak-ng-data/"
cp "$SRC/COPYING" "$OUT/COPYING"
echo "espeak-ng $VERSION, sources exactes : $URL" > "$OUT/SOURCE.txt"

# Contrôle : une phrase française, une anglaise.
printf 'Bonjour.\n' | "$OUT/bin/espeak-ng" -q --ipa --tie -v fr-fr --path="$OUT/share"
printf 'Hello.\n' | "$OUT/bin/espeak-ng" -q --ipa --tie -v en-us --path="$OUT/share"
du -sh "$OUT/bin/espeak-ng" "$OUT/share/espeak-ng-data"
