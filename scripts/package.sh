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
    Linux-x86_64) ORT_ARCHIVE="onnxruntime-linux-x64-$ORT_VERSION" ;;
    Linux-aarch64) ORT_ARCHIVE="onnxruntime-linux-aarch64-$ORT_VERSION" ;;
    *) ORT_ARCHIVE="" ;;
    esac
    if [ -n "$ORT_ARCHIVE" ]; then
        mkdir -p target/ort
        # Extraite à part, mise en place seulement une fois complète : une
        # extraction interrompue est refaite au lancement suivant.
        if [ ! -d "target/ort/$ORT_ARCHIVE" ]; then
            rm -rf target/ort/extraction
            mkdir -p target/ort/extraction
            curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/$ORT_ARCHIVE.tgz" \
                | tar -xz -C target/ort/extraction
            mv "target/ort/extraction/$ORT_ARCHIVE" "target/ort/$ORT_ARCHIVE"
            rm -rf target/ort/extraction
        fi
        cp "target/ort/$ORT_ARCHIVE/lib/libonnxruntime.so.$ORT_VERSION" "$OUT/libonnxruntime.so"
        cp "target/ort/$ORT_ARCHIVE/LICENSE" "$OUT/LICENSE-onnxruntime"
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
