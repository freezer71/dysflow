#!/usr/bin/env bash
# Construit sillage-espeak (GPL-3.0), la phonétisation de Kokoro : un petit
# programme à nous (scripts/sillage-espeak.c), lié à libespeak-ng construite
# depuis les sources épinglées d'espeak-ng. Sillage ne s'y lie pas : il le lance
# et lui parle par ses entrées et sorties standard, ce qui fait de l'ensemble
# un agrégat.
#
#   ./scripts/build-espeak.sh
#
# Produit target/espeak/bin/sillage-espeak, target/espeak/share/espeak-ng-data
# (français et anglais seulement), et, pour la GPL : target/espeak/COPYING,
# SOURCE.txt (l'adresse exacte des sources d'espeak-ng, et l'engagement de
# garder l'archive disponible) et sillage-espeak.c. L'archive téléchargée reste
# dans target/espeak : c'est elle qu'on fournit si l'adresse disparaît.
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

# sha256sum (GNU, ou BSD sous macOS : ni l'un ni l'autre ne lisent `--status`
# de la même façon, d'où `-c -` et la sortie jetée), sinon shasum (Perl).
# L'empreinte épinglée doit faire exactement 64 chiffres hexadécimaux : le
# sha256sum BSD accepte une ligne mal formée (code 0), qui ne vérifierait rien.
verify() {
    case "$SHA256" in
    *[!0-9a-f]*) echo "Erreur : empreinte épinglée mal formée : $SHA256" >&2; exit 1 ;;
    esac
    if [ "${#SHA256}" -ne 64 ]; then
        echo "Erreur : empreinte épinglée mal formée (${#SHA256} caractères au lieu de 64) : $SHA256" >&2
        exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        echo "$SHA256  $1" | sha256sum -c - >/dev/null 2>&1
    else
        echo "$SHA256  $1" | shasum -a 256 -c --status
    fi
}

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

LIB=$(find "$OUT/build" -type f -name libespeak-ng.a | head -1)
UCD=$(find "$OUT/build" -type f -name libucd.a | head -1)
DATA=$(find "$OUT/build" -type d -name espeak-ng-data | head -1)
for f in phontab phonindex phondata intonations fr_dict en_dict; do
    if [ ! -f "$DATA/$f" ]; then
        echo "Erreur : $DATA/$f manque (construction des données d'espeak-ng)" >&2
        exit 1
    fi
done

rm -rf "${OUT:?}/bin" "${OUT:?}/share"
mkdir -p "$OUT/bin" "$OUT/share/espeak-ng-data"
# Le programme : sillage-espeak.c et libespeak-ng, liée statiquement.
CFLAGS=(-O2 -Wall -I"$SRC/src/include")
if [ "$(uname -s)" = Darwin ]; then
    CFLAGS+=(-mmacosx-version-min=14.0 -arch arm64)
fi
cc "${CFLAGS[@]}" scripts/sillage-espeak.c "$LIB" "$UCD" -lm -o "$OUT/bin/sillage-espeak"
for f in phontab phonindex phondata phondata-manifest intonations fr_dict en_dict; do
    [ -f "$DATA/$f" ] && cp "$DATA/$f" "$OUT/share/espeak-ng-data/"
done
cp -R "$DATA/lang" "$OUT/share/espeak-ng-data/"
[ -d "$DATA/voices" ] && cp -R "$DATA/voices" "$OUT/share/espeak-ng-data/"
cp "$SRC/COPYING" "$OUT/COPYING"
cp scripts/sillage-espeak.c "$OUT/sillage-espeak.c"
{
    echo "espeak-ng $VERSION, sources exactes : $URL (SHA-256 $SHA256)"
    echo "sillage-espeak : sillage-espeak.c, à côté de ce fichier ; construit par build-espeak.sh"
    echo "(à placer dans scripts/ avec sillage-espeak.c, à la racine des sources de Sillage)"
    echo "L'archive ci-dessus (même empreinte) est conservée par le distributeur de Sillage et"
    echo "fournie sur demande, tant que ce programme est distribué, si l'adresse venait à disparaître."
} > "$OUT/SOURCE.txt"

# Contrôle : une phrase française, une anglaise (trames : u32 petit-boutiste
# puis octets ; la voix, puis un lot d'une entrée).
check() {
    local got
    # Le format porte les octets de la trame (\005, \000…) : voulu.
    # shellcheck disable=SC2059
    got=$(printf "$1" | "$OUT/bin/sillage-espeak" "$OUT/share" | LC_ALL=C tr -d '\000-\037')
    case "$got" in
        *"$2"*) echo "$2" ;;
        *) echo "Erreur : sillage-espeak ne donne pas $2" >&2; exit 1 ;;
    esac
}
check '\005\000\000\000fr-fr\001\000\000\000\010\000\000\000Bonjour.' 'bɔ̃ʒˈuʁ'
check '\005\000\000\000en-us\001\000\000\000\006\000\000\000Hello.' 'həlˈo͡ʊ'
du -sh "$OUT/bin/sillage-espeak" "$OUT/share/espeak-ng-data"
