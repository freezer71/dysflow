#!/bin/sh
# Assemble un dossier distribuable : le binaire et les bibliothèques et
# programmes qu'il charge (shim audio.cpp ; pour Kokoro, ONNX Runtime et
# sillon-espeak avec ses données et ses sources GPL).
#
# Le shim audio.cpp est une bibliothèque partagée — c'est un choix assumé, qui
# laisse CMake résoudre les dépendances transitives. Le binaire la cherche donc
# à côté de lui, puis dans un `lib/` voisin. Ce script met le premier en place.
set -e
cd "$(dirname "$0")/.."

# Vrai si le fichier $2 a l'empreinte SHA-256 $1 : sha256sum (GNU, ou BSD
# sous macOS, d'où `-c -` sans `--status`), sinon shasum (Perl).
# L'empreinte épinglée doit faire exactement 64 chiffres hexadécimaux : le
# sha256sum BSD accepte une ligne mal formée (code 0), qui ne vérifierait rien.
sha256_ok() {
    case "$1" in
    *[!0-9a-f]*) echo "Erreur : empreinte épinglée mal formée : $1" >&2; exit 1 ;;
    esac
    if [ "${#1}" -ne 64 ]; then
        echo "Erreur : empreinte épinglée mal formée (${#1} caractères au lieu de 64) : $1" >&2
        exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        echo "$1  $2" | sha256sum -c - >/dev/null 2>&1
    else
        echo "$1  $2" | shasum -a 256 -c --status
    fi
}

FEATURES="${FEATURES:-audio-cpp,ocr,kokoro-onnx}"
OUT="${OUT:-dist}"

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/sillon "$OUT/"

# Le nom du répertoire de build porte une empreinte, il faut le retrouver —
# et chaque jeu de features laisse le sien : on prend le plus récent, sans
# quoi un shim d'un build antérieur pouvait être livré à la place de celui
# qu'on vient de compiler.
for ext in dylib so; do
    shim=$(find target/release/build -path "*out/lib/libaudiocpp_shim.$ext" -print0 2>/dev/null \
        | xargs -0 ls -t 2>/dev/null | head -1)
    [ -n "$shim" ] && cp "$shim" "$OUT/" && break
done

if [ -z "$shim" ]; then
    echo "Aucun shim trouvé : binaire compilé sans audio-cpp, rien à joindre." >&2
else
    echo "Joint : $(basename "$shim")"
fi

# Kokoro au terminal : ONNX Runtime 1.31 à côté du binaire, et la
# phonétisation sillon-espeak (GPL-3.0, programme séparé que Sillon lance) avec
# ses données, sa licence et ses sources correspondantes.
case ",$FEATURES," in
*,kokoro-onnx,*)
    if [ ! -x target/espeak/bin/sillon-espeak ] || [ scripts/sillon-espeak.c -nt target/espeak/bin/sillon-espeak ]; then
        ./scripts/build-espeak.sh
    fi
    cp target/espeak/bin/sillon-espeak "$OUT/"
    cp -R target/espeak/share/espeak-ng-data "$OUT/"
    mkdir -p "$OUT/sillon-espeak-source"
    cp target/espeak/COPYING target/espeak/SOURCE.txt target/espeak/sillon-espeak.c \
        scripts/build-espeak.sh "$OUT/sillon-espeak-source/"
    echo "Joint : sillon-espeak, espeak-ng-data et ses sources (sillon-espeak-source/)"

    ORT_VERSION=1.31.0
    case "$(uname -s)-$(uname -m)" in
    # Empreintes publiées avec la version v1.31.0 : une archive différente
    # n'est jamais jointe.
    Linux-x86_64)
        ORT_ARCHIVE="onnxruntime-linux-x64-$ORT_VERSION"
        ORT_SHA256=cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57 ;;
    Linux-aarch64)
        ORT_ARCHIVE="onnxruntime-linux-aarch64-$ORT_VERSION"
        ORT_SHA256=c5b8b3cca31f3d643a3b313b8f42e2d0f16a4fcefd02cda343d97bd5306afaab ;;
    *) ORT_ARCHIVE="" ;;
    esac
    if [ -n "$ORT_ARCHIVE" ]; then
        mkdir -p target/ort
        # Extraite à part, mise en place seulement une fois complète : une
        # extraction interrompue est refaite au lancement suivant.
        if [ ! -d "target/ort/$ORT_ARCHIVE" ]; then
            # L'archive : téléchargée sous un nom provisoire, gardée seulement
            # si son empreinte est la bonne.
            tarball="target/ort/$ORT_ARCHIVE.tgz"
            if [ ! -f "$tarball" ] || ! sha256_ok "$ORT_SHA256" "$tarball"; then
                rm -f "$tarball" "$tarball.part"
                curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/$ORT_ARCHIVE.tgz" \
                    -o "$tarball.part"
                if ! sha256_ok "$ORT_SHA256" "$tarball.part"; then
                    rm -f "$tarball.part"
                    echo "Erreur : empreinte SHA-256 inattendue pour $ORT_ARCHIVE.tgz" >&2
                    exit 1
                fi
                mv "$tarball.part" "$tarball"
            fi
            rm -rf target/ort/extraction
            mkdir -p target/ort/extraction
            tar -xzf "$tarball" -C target/ort/extraction
            mv "target/ort/extraction/$ORT_ARCHIVE" "target/ort/$ORT_ARCHIVE"
            rm -rf target/ort/extraction
        fi
        cp "target/ort/$ORT_ARCHIVE/lib/libonnxruntime.so.$ORT_VERSION" "$OUT/libonnxruntime.so"
        cp "target/ort/$ORT_ARCHIVE/LICENSE" "$OUT/LICENSE-onnxruntime"
        cp "target/ort/$ORT_ARCHIVE/ThirdPartyNotices.txt" "$OUT/ThirdPartyNotices-onnxruntime.txt"
        echo "Joint : ONNX Runtime $ORT_VERSION"
    else
        echo "ONNX Runtime non joint sur cette plateforme : SILLON_ORT_LIB=<libonnxruntime> au lancement." >&2
    fi
    ;;
esac

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && ./sillon --show-config >/dev/null )
echo "Prêt : $OUT/"
ls -lh "$OUT"
