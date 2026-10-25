#!/bin/sh
# Assemble un dossier distribuable : le binaire et les bibliothèques et
# programmes qu'il charge (pour Kokoro, ONNX Runtime et dysflow-espeak avec ses
# données et ses sources GPL).
set -eu
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

FEATURES="${FEATURES:-ocr,kokoro-onnx}"
OUT="${OUT:-dist}"

# Les licences des crates Rust liées : THIRD-PARTY-RUST.txt doit suivre
# Cargo.lock (python3 scripts/third-party-rust.py pour le régénérer) ; le
# script refuse une licence GPL, AGPL ou inconnue dans le graphe lié. Il
# décrit les features par défaut ci-dessous : avec d'autres, le régénérer.
python3 -I scripts/third-party-rust.py --check

echo "Compilation (features : $FEATURES)…"
cargo build --release --features "$FEATURES"

rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/dysflow "$OUT/"

# Kokoro au terminal : ONNX Runtime 1.31 à côté du binaire, et la
# phonétisation dysflow-espeak (GPL-3.0, programme séparé que Dysflow lance) avec
# ses données, sa licence et ses sources correspondantes.
case ",$FEATURES," in
*,kokoro-onnx,*)
    if [ ! -x target/espeak/bin/dysflow-espeak ] || [ -n "$(find scripts/dysflow-espeak.c -newer target/espeak/bin/dysflow-espeak)" ]; then
        ./scripts/build-espeak.sh
    fi
    cp target/espeak/bin/dysflow-espeak "$OUT/"
    cp -R target/espeak/share/espeak-ng-data "$OUT/"
    mkdir -p "$OUT/dysflow-espeak-source"
    cp target/espeak/COPYING target/espeak/SOURCE.txt target/espeak/dysflow-espeak.c \
        scripts/build-espeak.sh "$OUT/dysflow-espeak-source/"
    echo "Joint : dysflow-espeak, espeak-ng-data et ses sources (dysflow-espeak-source/)"

    ORT_VERSION=1.31.0
    case "$(uname -s)-$(uname -m)" in
    # Empreintes publiées avec la version v1.31.0 (champ `digest` des
    # fichiers de la publication GitHub) : une archive différente n'est
    # jamais jointe. ORT_LIB : la bibliothèque dans l'archive, ORT_DEST son
    # nom à côté du binaire (celui que cherche src/tts/kokoro/onnx.rs).
    Linux-x86_64)
        ORT_ARCHIVE="onnxruntime-linux-x64-$ORT_VERSION"
        ORT_SHA256=cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57
        ORT_LIB="libonnxruntime.so.$ORT_VERSION" ORT_DEST=libonnxruntime.so ;;
    Linux-aarch64)
        ORT_ARCHIVE="onnxruntime-linux-aarch64-$ORT_VERSION"
        ORT_SHA256=c5b8b3cca31f3d643a3b313b8f42e2d0f16a4fcefd02cda343d97bd5306afaab
        ORT_LIB="libonnxruntime.so.$ORT_VERSION" ORT_DEST=libonnxruntime.so ;;
    Darwin-arm64)
        ORT_ARCHIVE="onnxruntime-osx-arm64-$ORT_VERSION"
        ORT_SHA256=1031b36dd3bdaa8d976a6130d9cf2a05e9be320b93ae804f8427b91dc3432afc
        ORT_LIB="libonnxruntime.$ORT_VERSION.dylib" ORT_DEST=libonnxruntime.dylib ;;
    *) ORT_ARCHIVE="" ;;
    esac
    if [ -n "$ORT_ARCHIVE" ]; then
        mkdir -p target/ort
        # Extraite à part, mise en place seulement une fois complète : une
        # extraction interrompue est refaite au lancement suivant.
        # Le fichier, pas le dossier : le cache de la CI (rust-cache) rend
        # target/ort vidé de ses fichiers, et un dossier présent mais vide
        # faisait échouer la copie plus bas.
        if [ ! -f "target/ort/$ORT_ARCHIVE/lib/$ORT_LIB" ] \
            || [ ! -f "target/ort/$ORT_ARCHIVE/LICENSE" ] \
            || [ ! -f "target/ort/$ORT_ARCHIVE/ThirdPartyNotices.txt" ]; then
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
            rm -rf "target/ort/$ORT_ARCHIVE"
            mv "target/ort/extraction/$ORT_ARCHIVE" "target/ort/$ORT_ARCHIVE"
            rm -rf target/ort/extraction
        fi
        cp "target/ort/$ORT_ARCHIVE/lib/$ORT_LIB" "$OUT/$ORT_DEST"
        cp "target/ort/$ORT_ARCHIVE/LICENSE" "$OUT/LICENSE-onnxruntime"
        cp "target/ort/$ORT_ARCHIVE/ThirdPartyNotices.txt" "$OUT/ThirdPartyNotices-onnxruntime.txt"
        echo "Joint : ONNX Runtime $ORT_VERSION"
    else
        echo "ONNX Runtime non joint sur cette plateforme : DYSFLOW_ORT_LIB=<libonnxruntime> au lancement." >&2
    fi
    ;;
esac

# Les avis : NOTICE, les crates Rust, Sonic, et Tesseract et Leptonica
# (feature ocr). ONNX
# Runtime (LICENSE-onnxruntime, ThirdPartyNotices-onnxruntime.txt) et
# dysflow-espeak (dysflow-espeak-source/) ont été joints plus haut.
cp NOTICE THIRD-PARTY-RUST.txt "$OUT/"
mkdir -p "$OUT/licences"
cp third_party/sonic/LICENSE "$OUT/licences/sonic-LICENSE"
case ",$FEATURES," in
*,ocr,*)
    cp third_party/licences/tesseract-LICENSE third_party/licences/leptonica-LICENSE "$OUT/licences/" ;;
esac
echo "Joint : NOTICE, THIRD-PARTY-RUST.txt et licences/"

echo "Vérification hors de l'arbre de compilation…"
( cd "$OUT" && ./dysflow --show-config >/dev/null )
echo "Prêt : $OUT/"
ls -lh "$OUT"
